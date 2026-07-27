//! Session list (T2.3). Reads from the `sessions` table (populated by
//! `tokscale --group-by session,model`), grouped by `(tool, session_id)`.
//! Project names are resolved by scanning `~/.claude/projects/` (like the
//! projects page does via workspace.rs). A second endpoint groups session
//! JSONL messages into "rounds" (one per user input) with apportioned cost.

use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use rusqlite::Connection;

use super::QueryError;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SessionVm {
    pub tool: String,
    pub session_id: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
    pub model_count: i64,
    pub models: String,
    pub last_used_at: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SessionDetailRow {
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

/// Grouped session list ordered by tokens desc. `claude_projects_dir` is
/// scanned to resolve project names for Claude sessions.
/// NOTE: The sessions table stores all-time aggregates, so this query returns
/// all sessions regardless of period. Period filtering is handled by the
/// breakdown queries (daily_usage table).
pub fn query(
    conn: &Connection,
    claude_projects_dir: Option<&Path>,
) -> Result<Vec<SessionVm>, QueryError> {
    let proj_map = claude_projects_dir
        .map(crate::collector::workspace::session_project_map)
        .unwrap_or_default();

    let mut stmt = conn.prepare(
        "SELECT tool, session_id,
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0) AS tokens,
                COALESCE(SUM(cost_usd), 0) AS cost,
                COALESCE(SUM(message_count), 0) AS messages,
                COUNT(DISTINCT model) AS model_count,
                COALESCE(GROUP_CONCAT(DISTINCT model), '') AS models,
                MAX(last_used_at) AS last_used_at
         FROM sessions
         GROUP BY tool, session_id
         ORDER BY tokens DESC
         LIMIT 100",
    )?;
    let out = stmt
        .query_map([], |r| {
            let sid: String = r.get::<_, String>(1)?;
            let (proj_name, proj_path, file_mtime) =
                proj_map.get(&sid).cloned().unwrap_or_default();
            let db_time = format_last_used(r.get::<_, Option<i64>>(7)?);
            Ok(SessionVm {
                tool: r.get::<_, String>(0)?,
                session_id: sid,
                tokens: r.get::<_, i64>(2)?,
                cost_usd: r.get::<_, f64>(3)?,
                messages: r.get::<_, i64>(4)?,
                model_count: r.get::<_, i64>(5)?,
                models: r.get::<_, String>(6)?,
                last_used_at: file_mtime.or(db_time),
                project_name: (!proj_name.is_empty()).then_some(proj_name),
                project_path: (!proj_path.is_empty()).then_some(proj_path),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

/// Per-model rows for a single session. Ordered by tokens desc.
/// Returns all-time data (the sessions table stores aggregates, not per-day).
pub fn query_detail(
    conn: &Connection,
    tool: &str,
    session_id: &str,
) -> Result<Vec<SessionDetailRow>, QueryError> {
    let mut stmt = conn.prepare(
        "SELECT model,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
                COALESCE(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens, 0) AS tokens,
                cost_usd,
                message_count
         FROM sessions
         WHERE tool = ? AND session_id = ?
         ORDER BY tokens DESC",
    )?;
    let out = stmt
        .query_map(rusqlite::params![tool, session_id], |r| {
            Ok(SessionDetailRow {
                model: r.get::<_, String>(0)?,
                input: r.get::<_, i64>(1)?,
                output: r.get::<_, i64>(2)?,
                cache_read: r.get::<_, i64>(3)?,
                cache_write: r.get::<_, i64>(4)?,
                tokens: r.get::<_, i64>(5)?,
                cost_usd: r.get::<_, f64>(6)?,
                messages: r.get::<_, i64>(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

fn format_last_used(ts: Option<i64>) -> Option<String> {
    let ms = ts?;
    if ms <= 0 {
        return None;
    }
    let secs = ms / 1000;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    let dt = Local.timestamp_opt(secs, nsecs).single()?;
    Some(dt.format("%Y-%m-%d %H:%M").to_string())
}

// ── per-round data (grouped by user input) from session JSONL ───────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SessionRoundVm {
    pub user_text: String,
    pub timestamp: Option<String>,
    pub turns: i64,
    pub tools: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    /// Model used for this round (best-effort: from turn_context for codex,
    /// most-used model from model_tokens for claude).
    pub model: Option<String>,
}

/// Group a session's messages into rounds (each starts at a user message and
/// includes all following assistant messages), apportion the session's
/// per-model cost across rounds by token share, and return newest-first
/// capped at `MAX_ROUNDS`.
pub fn query_rounds(
    conn: &Connection,
    home_dir: Option<&Path>,
    tool: &str,
    session_id: &str,
) -> Result<Vec<SessionRoundVm>, QueryError> {
    let model_totals = session_model_totals_public(conn, tool, session_id)?;
    Ok(build_rounds(home_dir, tool, session_id, model_totals))
}

/// `(model_total_tokens, model_total_cost)` per model for one session.
/// Public so a command can snapshot it under a short DB lock, then release
/// the lock before the (slow) JSONL parse runs on the blocking pool.
pub fn session_model_totals_public(
    conn: &Connection,
    tool: &str,
    session_id: &str,
) -> Result<std::collections::HashMap<String, (i64, f64)>, QueryError> {
    let mut stmt = conn.prepare(
        "SELECT model,
                COALESCE(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens, 0),
                COALESCE(cost_usd, 0)
         FROM sessions
         WHERE tool = ? AND session_id = ?",
    )?;
    let rows = stmt.query_map(rusqlite::params![tool, session_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (r.get::<_, i64>(1)?, r.get::<_, f64>(2)?),
        ))
    })?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (m, tc) = row?;
        map.insert(m, tc);
    }
    Ok(map)
}

struct RoundAcc {
    user_text: String,
    ts_raw: Option<String>,
    is_command: bool,
    turns: i64,
    tools: i64,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    model_tokens: std::collections::HashMap<String, i64>,
    model: Option<String>,
    /// Tool names from function_call / custom_tool_call events (Codex only).
    tool_names: Vec<String>,
}

impl RoundAcc {
    fn new(user_text: String, ts_raw: Option<String>, is_command: bool) -> Self {
        Self {
            user_text,
            ts_raw,
            is_command,
            turns: 0,
            tools: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            model_tokens: std::collections::HashMap::new(),
            model: None,
            tool_names: Vec::new(),
        }
    }

    fn finalize(self, cost_usd: f64) -> SessionRoundVm {
        // Pick the primary model: most-used from model_tokens, or the tracked
        // model (for tools like codex where model_tokens is empty).
        let primary_model = self.model_tokens.iter()
            .max_by_key(|(_, &tokens)| tokens)
            .map(|(m, _)| m.clone())
            .or(self.model);
        SessionRoundVm {
            total_tokens: self.input_tokens
                + self.output_tokens
                + self.cache_read_tokens
                + self.cache_write_tokens,
            user_text: self.user_text,
            timestamp: format_iso(&self.ts_raw),
            turns: self.turns,
            tools: {
                // Codex: count unique tool names from function_call events.
                if self.tool_names.is_empty() {
                    self.tools
                } else {
                    let mut seen = std::collections::HashSet::new();
                    for n in &self.tool_names {
                        seen.insert(n.as_str());
                    }
                    seen.len() as i64
                }
            },
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens,
            cost_usd,
            model: primary_model,
        }
    }
}

fn parse_rounds(
    claude_projects_dir: Option<&Path>,
    tool: &str,
    session_id: &str,
) -> Vec<RoundAcc> {
    // Find the session JSONL file for the given tool.
    let Some(path) = find_session_file_for_tool(claude_projects_dir, tool, session_id) else {
        return Vec::new();
    };
    let Ok(file) = std::fs::File::open(&path) else {
        return Vec::new();
    };

    let mut rounds: Vec<RoundAcc> = Vec::new();
    // Track the most recent model from turn_context events.  Codex emits
    // turn_context BEFORE the user message that creates the next round, so
    // we stash it here and apply it when the round is created.
    let mut last_model: Option<String> = None;
    for line in std::io::BufRead::lines(std::io::BufReader::new(file)) {
        let Ok(line) = line else {
            break;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        match v.get("type").and_then(|x| x.as_str()).unwrap_or("") {
            // ── Claude format ─────────────────────────────────────────────
            "user" => {
                let text = user_prompt_preview(&v, 80);
                if text.is_empty() { continue; }
                let is_cmd = is_command_message(&v, &text);
                let ts_raw = v.get("timestamp").and_then(|x| x.as_str()).map(String::from);
                rounds.push(RoundAcc::new(text, ts_raw, is_cmd));
            }
            "assistant" => {
                let Some(round) = rounds.last_mut() else { continue; };
                let msg = v.get("message");
                let model = msg.and_then(|m| m.get("model")).and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
                let usage = msg.and_then(|m| m.get("usage"));
                let input = usage.and_then(|u| u.get("input_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                let output = usage.and_then(|u| u.get("output_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                let cache_read = usage.and_then(|u| u.get("cache_read_input_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                let cache_write = usage.and_then(|u| u.get("cache_creation_input_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                let tool_count = msg.and_then(|m| m.get("content")).and_then(|c| c.as_array()).map(|arr| arr.iter().filter(|b| is_tool_use(b)).count() as i64).unwrap_or(0);
                round.turns += 1;
                round.tools += tool_count;
                round.input_tokens += input;
                round.output_tokens += output;
                round.cache_read_tokens += cache_read;
                round.cache_write_tokens += cache_write;
                let mt = input + output + cache_read + cache_write;
                *round.model_tokens.entry(model).or_insert(0) += mt;
            }
            // ── Codex format ──────────────────────────────────────────────
            // Codex uses two parallel event streams:
            //   event_msg/user_message  → user prompts
            //   event_msg/token_count   → per-turn token totals (in info.last_token_usage)
            //   response_item/function_call → tool tracking
            //   turn_context            → model name
            "response_item" => {
                let payload = v.get("payload");
                let msg_type = payload.and_then(|p| p.get("type")).and_then(|x| x.as_str());
                // Track tool calls for the next turn.
                if let Some("function_call" | "custom_tool_call" | "tool_search_call") = msg_type {
                    if let Some(round) = rounds.last_mut() {
                        if let Some(name) = payload.and_then(|p| p.get("name")).and_then(|x| x.as_str()) {
                            if !name.is_empty() {
                                round.tools += 1;
                                round.tool_names.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "event_msg" => {
                let payload = v.get("payload");
                let msg_type = payload.and_then(|p| p.get("type")).and_then(|x| x.as_str());
                match msg_type {
                    Some("user_message") => {
                        // User prompt: text is in payload.message (string).
                        let text = payload.and_then(|p| p.get("message")).and_then(|x| x.as_str()).unwrap_or_default().trim().to_string();
                        // Skip environment-context-only messages.
                        if text.is_empty() || text.starts_with("<environment_context>") {
                            continue;
                        }
                        let is_cmd = text.trim_start().starts_with('/');
                        let ts_raw = v.get("timestamp").and_then(|x| x.as_str()).map(String::from);
                        let mut acc = RoundAcc::new(text, ts_raw, is_cmd);
                        // Apply model from preceding turn_context.
                        acc.model = last_model.clone();
                        rounds.push(acc);
                    }
                    Some("token_count") => {
                        // Per-turn token data from payload.info.last_token_usage.
                        let info = payload.and_then(|p| p.get("info"));
                        let usage = info.and_then(|i| i.get("last_token_usage"));
                        if usage.is_none() { continue; }
                        let input = usage.and_then(|u| u.get("input_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                        let cached = usage.and_then(|u| u.get("cached_input_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                        let output = usage.and_then(|u| u.get("output_tokens")).and_then(|x| x.as_i64()).unwrap_or(0);
                        let cache_read = cached; // cached_input_tokens = cache_read
                        let cache_write: i64 = 0; // Codex doesn't have cache_write
                        let Some(round) = rounds.last_mut() else { continue; };
                        round.turns += 1;
                        round.input_tokens += input;
                        round.output_tokens += output;
                        round.cache_read_tokens += cache_read;
                        round.cache_write_tokens += cache_write;
                        let mt = input + output + cache_read + cache_write;
                        if mt > 0 {
                            let model = last_model.clone().unwrap_or_else(|| "codex-unknown".to_string());
                            *round.model_tokens.entry(model).or_insert(0) += mt;
                        }
                    }
                    _ => {}
                }
            }
            // ── Turn context (carries model name for codex) ─────────────
            "turn_context" if tool == "codex" => {
                last_model = v.get("payload")
                    .and_then(|p| p.get("model"))
                    .and_then(|x| x.as_str())
                    .map(String::from);
            }
            _ => {}
        }
    }
    rounds
}

/// Parse rounds from the session JSONL, apportion cost via `model_totals`,
/// cap to the newest `MAX_ROUNDS`, and reverse to newest-first. Pure (no DB).
pub fn build_rounds(
    home_dir: Option<&Path>,
    tool: &str,
    session_id: &str,
    model_totals: std::collections::HashMap<String, (i64, f64)>,
) -> Vec<SessionRoundVm> {
    let mut acc = parse_rounds(home_dir, tool, session_id);
    // Keep only real conversations: not slash commands, not low-signal filler
    // ("继续" etc.), produced AI output (turns > 0). For Claude we also require
    // non-zero token usage (zero-token rounds have zero apportioned cost too —
    // no signal). For other tools (codex, opencode) the JSONL format doesn't
    // carry per-turn token counts, so we skip the token filter and show all
    // valid rounds.
    acc.retain(|r| {
        !r.is_command
            && !is_filler_prompt(&r.user_text)
            && r.turns > 0
            && (tool != "claude" || {
                let total = r.input_tokens + r.output_tokens + r.cache_read_tokens + r.cache_write_tokens;
                total > 0
            })
    });

    // Sort by timestamp descending (newest first) — explicit, not relying on
    // file order. Rounds without a timestamp sink to the bottom.
    acc.sort_by(|a, b| b.ts_raw.cmp(&a.ts_raw));

    // Take the most recent MAX_ROUNDS AFTER sorting.
    const MAX_ROUNDS: usize = 300;
    if acc.len() > MAX_ROUNDS {
        acc.truncate(MAX_ROUNDS);
    }

    acc.into_iter()
        .map(|r| {
            let cost = apportion_cost(&r.model_tokens, &model_totals);
            r.finalize(cost)
        })
        .collect()
}

/// Low-signal filler prompts to hide (continuations / acknowledgements).
const FILLER_PROMPTS: &[&str] = &[
    "继续",
    "继续。",
    "继续一下",
    "继续吧",
    "continue",
    "cont",
    "go",
    "go on",
    "next",
    "好的",
    "好",
    "ok",
    "okay",
    "yes",
    "是",
    "对",
    "嗯",
    "恩",
    "行",
    "可以",
];

/// True for tiny acknowledgement / continuation prompts like "继续", "ok".
fn is_filler_prompt(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return true;
    }
    // Single character (e.g. "好", "是") → filler.
    if t.chars().count() <= 1 {
        return true;
    }
    FILLER_PROMPTS.iter().any(|f| *f == t)
}

/// round cost = Σ_models (round_model_tokens / session_model_tokens × session_model_cost).
fn apportion_cost(
    round_model_tokens: &std::collections::HashMap<String, i64>,
    model_totals: &std::collections::HashMap<String, (i64, f64)>,
) -> f64 {
    let mut cost = 0.0;
    for (model, rtok) in round_model_tokens {
        if let Some((total_tok, total_cost)) = model_totals.get(model) {
            if *total_tok > 0 {
                cost += (*rtok as f64 / *total_tok as f64) * total_cost;
            }
        }
    }
    cost
}

/// Format an ISO-8601 UTC timestamp "2026-07-15T08:03:55.008Z" → local
/// "2026-07-15 16:03". The JSONL timestamps are UTC (Z suffix); we convert to the
/// user's local timezone so the displayed time matches their clock.
fn format_iso(raw: &Option<String>) -> Option<String> {
    use chrono::{DateTime, Local};
    let raw = raw.as_ref()?;
    let parsed: DateTime<Local> = DateTime::parse_from_rfc3339(raw).ok()?.into();
    Some(parsed.format("%Y-%m-%d %H:%M").to_string())
}

/// Extract a one-line preview of the user's prompt: takes the first text
/// block (skipping `tool_result` blocks), strips command tags, ellipsises.
fn user_prompt_preview(v: &serde_json::Value, max_chars: usize) -> String {
    let msg = match v.get("message") {
        Some(m) => m,
        None => return String::new(),
    };
    let content = match msg.get("content") {
        Some(c) => c,
        None => return String::new(),
    };
    let raw = if let Some(s) = content.as_str() {
        s.to_string()
    } else if let Some(arr) = content.as_array() {
        let mut picked = String::new();
        for block in arr {
            if block.get("type").and_then(|x| x.as_str()) == Some("text") {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    if !t.trim().is_empty() {
                        picked = t.to_string();
                        break;
                    }
                }
            }
        }
        picked
    } else {
        return String::new();
    };
    let stripped = strip_command_tags(&raw);
    let first_line = stripped
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    truncate_str(first_line, max_chars)
}

fn strip_command_tags(s: &str) -> String {
    let mut out = s.to_string();
    for tag in ["command-message", "command-name", "local-command-stdout"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let (Some(a), Some(b)) = (out.find(&open), out.find(&close)) {
            if a < b {
                let inner = out[a + open.len()..b].to_string();
                out = format!("{}{}", &out[..a], &out[b + close.len()..]);
                if out.trim().is_empty() {
                    out = inner;
                }
            }
        }
    }
    out
}

// ── Multi-tool session file lookup ──────────────────────────────────────────

/// Find the session JSONL file for a given tool+session_id. Each tool stores
/// sessions in a different directory layout:
///
/// - claude:  `~/.claude/projects/{project_dir}/{session_id}.jsonl`
/// - codex:   `~/.codex/sessions/{YYYY}/{MM}/{DD}/{filename}.jsonl` (filename
///   contains the session UUID which may differ from the DB session_id)
/// - opencode: `~/.local/share/opencode/storage/message/` (not yet implemented)
pub fn find_session_file_for_tool(
    home_dir: Option<&Path>,
    tool: &str,
    session_id: &str,
) -> Option<PathBuf> {
    let home = home_dir?;
    match tool {
        "claude" => {
            let dir = home.join(".claude").join("projects");
            if !dir.is_dir() { return None; }
            crate::collector::workspace::find_session_file(&dir, session_id)
        }
        "codex" => {
            let dir = home.join(".codex").join("sessions");
            if !dir.is_dir() { return None; }
            // Codex stores sessions in YYYY/MM/DD/ subdirectories. Walk all
            // subdirectories looking for a JSONL whose content contains the
            // session UUID (the filename stem may differ from the DB session_id).
            find_codex_session_file(&dir, session_id)
        }
        "opencode" => {
            // OpenCode stores individual message files; round-level parsing
            // requires a different strategy. Not yet implemented.
            None
        }
        _ => None,
    }
}

/// Walk `~/.codex/sessions/{Y}/{M}/{D}/` and return the first JSONL file
/// whose content contains the given session UUID.
fn find_codex_session_file(sessions_dir: &Path, session_id: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(sessions_dir).ok()? {
        let year_dir = entry.ok()?.path();
        if !year_dir.is_dir() { continue; }
        for entry in std::fs::read_dir(&year_dir).ok()? {
            let month_dir = entry.ok()?.path();
            if !month_dir.is_dir() { continue; }
            for entry in std::fs::read_dir(&month_dir).ok()? {
                let day_dir = entry.ok()?.path();
                if !day_dir.is_dir() { continue; }
                for entry in std::fs::read_dir(&day_dir).ok()? {
                    let path = entry.ok()?.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }
                    // Quick check: does the file contain the session UUID?
                    if file_contains(&path, session_id) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// Check if a file contains the given string (case-sensitive).
fn file_contains(path: &Path, needle: &str) -> bool {
    const CHUNK: usize = 4096;
    let Ok(file) = std::fs::File::open(path) else { return false; };
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; CHUNK];
    loop {
        let n = match std::io::Read::read(&mut reader, &mut buf) {
            Ok(0) => return false,
            Ok(n) => n,
            Err(_) => return false,
        };
        let slice = &buf[..n];
        match std::str::from_utf8(slice) {
            Ok(s) if s.contains(needle) => return true,
            _ => {}
        }
    }
}

fn is_tool_use(block: &serde_json::Value) -> bool {
    block.get("type").and_then(|x| x.as_str()) == Some("tool_use")
}

/// True if the user message is a slash command (e.g. `/exit`, `/compact`,
/// `/model`) rather than a real prompt. Detected via a leading `/` in the
/// cleaned text or a Claude-Code `<command-name>` wrapper in the raw content.
fn is_command_message(v: &serde_json::Value, cleaned_text: &str) -> bool {
    if cleaned_text.trim_start().starts_with('/') {
        return true;
    }
    let Some(msg) = v.get("message") else {
        return false;
    };
    let raw = match msg.get("content") {
        Some(serde_json::Value::String(s)) => s.as_str().to_string(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|x| x.as_str()) == Some("text") {
                    b.get("text").and_then(|x| x.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    };
    raw.contains("<command-name>") || raw.contains("<command-message>")
}

fn truncate_str(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.len() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema;

    fn seeded() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute_batch(&format!(
            "INSERT INTO sessions (tool,session_id,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,cost_usd,message_count,last_used_at)
             VALUES ('claude','s1','glm-5.2',1000,200,300,0,1.5,10,{now}),
                    ('claude','s1','gpt-5',500,0,0,0,0.5,3,{now}),
                    ('codex','s2','gpt-5-plus',200,80,0,0,0.42,5,{now})"
        ))
        .unwrap();
        conn
    }

    #[test]
    fn query_groups_by_tool_and_session() {
        let conn = seeded();
        let v = query(&conn, None).unwrap();
        assert_eq!(v.len(), 2);
        let s1 = &v[0];
        assert_eq!(s1.tool, "claude");
        assert_eq!(s1.session_id, "s1");
        assert_eq!(s1.tokens, 2000);
        assert!((s1.cost_usd - 2.0).abs() < 1e-9);
        assert_eq!(s1.messages, 13);
        assert_eq!(s1.model_count, 2);
        assert!(s1.last_used_at.is_some());
        assert!(s1.project_name.is_none());
    }

    #[test]
    fn query_returns_all_sessions_no_period_filter() {
        let conn = seeded();
        let now = chrono::Utc::now().timestamp_millis();
        let yesterday = now - 86_400_000;
        // Insert a session from yesterday — should still appear since there's
        // no period filter (sessions table stores all-time aggregates).
        conn.execute_batch(&format!(
            "INSERT INTO sessions (tool,session_id,model,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,cost_usd,message_count,last_used_at)
             VALUES ('claude','s_old','gpt-4',100,50,0,0,0.1,1,{yesterday})"
        )).unwrap();
        let v = query(&conn, None).unwrap();
        assert_eq!(v.len(), 3); // all 3 sessions, no filtering
        assert!(v.iter().any(|s| s.session_id == "s_old"));
    }

    #[test]
    fn query_detail_returns_per_model_rows() {
        let conn = seeded();
        let rows = query_detail(&conn, "claude", "s1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "glm-5.2");
        assert_eq!(rows[0].tokens, 1500);
    }

    #[test]
    fn query_detail_empty_for_unknown() {
        let conn = seeded();
        assert!(query_detail(&conn, "no", "ne").unwrap().is_empty());
    }

    #[test]
    fn query_rounds_empty_without_session_file() {
        let conn = seeded();
        // No home_dir → no rounds.
        let rounds = query_rounds(&conn, None, "claude", "s1").unwrap();
        assert!(rounds.is_empty());
    }

    #[test]
    fn empty_when_no_sessions() {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        assert!(query(&conn, None).unwrap().is_empty());
    }

    #[test]
    fn apportion_cost_scales_by_token_share() {
        let mut totals = std::collections::HashMap::new();
        totals.insert("glm-5.2".to_string(), (1000_i64, 10.0_f64));
        let mut round = std::collections::HashMap::new();
        round.insert("glm-5.2".to_string(), 250_i64);
        // 250/1000 × 10.0 = 2.5
        assert!((apportion_cost(&round, &totals) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn apportion_cost_zero_when_no_totals() {
        let totals = std::collections::HashMap::new();
        let mut round = std::collections::HashMap::new();
        round.insert("glm-5.2".to_string(), 250_i64);
        assert_eq!(apportion_cost(&round, &totals), 0.0);
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("abcdefghij", 4), "abcd…");
    }

    #[test]
    fn filler_prompt_detection() {
        assert!(is_filler_prompt("继续"));
        assert!(is_filler_prompt(" ok "));
        assert!(is_filler_prompt("好的"));
        assert!(is_filler_prompt("是"));
        assert!(!is_filler_prompt("帮我优化会话页面"));
        assert!(!is_filler_prompt("continue with the next step please"));
    }

    #[test]
    fn codex_env_context_skipped_rounds_have_real_prompts() {
        // Codex emits event_msg/user_message for real user prompts.
        // event_msg/token_count delivers per-turn token usage.
        let tmp = std::env::temp_dir().join(format!("tu_codex_{}", std::process::id()));
        let day = tmp.join(".codex").join("sessions").join("2025").join("01").join("01");
        let _ = std::fs::create_dir_all(&day);
        let path = day.join("test-session.jsonl");
        let jsonl = r#"{"timestamp":"2025-01-01T00:00:00Z","type":"session_meta","payload":{"id":"test-session","timestamp":"2025-01-01T00:00:00Z","cwd":"/tmp"}}
{"timestamp":"2025-01-01T00:00:01Z","type":"turn_context","payload":{"cwd":"/tmp","model":"gpt-4-codex","summary":"auto"}}
{"timestamp":"2025-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"Fix the bug"}}
{"timestamp":"2025-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":50,"reasoning_output_tokens":10}}}}
"#;
        std::fs::write(&path, jsonl).unwrap();
        let rounds = build_rounds(Some(&tmp), "codex", "test-session", Default::default());
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].user_text, "Fix the bug");
        assert_eq!(rounds[0].turns, 1);
        assert_eq!(rounds[0].input_tokens, 100);
        assert_eq!(rounds[0].cache_read_tokens, 20);
        assert_eq!(rounds[0].output_tokens, 50);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn codex_turn_context_model_extracted() {
        let tmp = std::env::temp_dir().join(format!("tu_codex_tc_{}", std::process::id()));
        let day = tmp.join(".codex").join("sessions").join("2025").join("01").join("01");
        let _ = std::fs::create_dir_all(&day);
        let path = day.join("test-tc.jsonl");
        let jsonl = r#"{"timestamp":"2025-01-01T00:00:00Z","type":"session_meta","payload":{"id":"test-tc","timestamp":"2025-01-01T00:00:00Z","cwd":"/tmp"}}
{"timestamp":"2025-01-01T00:00:01Z","type":"turn_context","payload":{"cwd":"/tmp","model":"gpt-4-codex","summary":"auto"}}
{"timestamp":"2025-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"Hello"}}
{"timestamp":"2025-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":10}}}}
"#;
        std::fs::write(&path, jsonl).unwrap();
        let rounds = build_rounds(Some(&tmp), "codex", "test-tc", Default::default());
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].model, Some("gpt-4-codex".to_string()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn codex_two_rounds_sorted_newest_first() {
        let tmp = std::env::temp_dir().join(format!("tu_codex_2r_{}", std::process::id()));
        let day = tmp.join(".codex").join("sessions").join("2025").join("01").join("01");
        let _ = std::fs::create_dir_all(&day);
        let path = day.join("test-2r.jsonl");
        let jsonl = r#"{"timestamp":"2025-01-01T00:00:00Z","type":"session_meta","payload":{"id":"test-2r","timestamp":"2025-01-01T00:00:00Z","cwd":"/tmp"}}
{"timestamp":"2025-01-01T00:00:01Z","type":"turn_context","payload":{"cwd":"/tmp","model":"gpt-4-codex","summary":"auto"}}
{"timestamp":"2025-01-01T00:00:02Z","type":"event_msg","payload":{"type":"user_message","message":"First question"}}
{"timestamp":"2025-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":50}}}}
{"timestamp":"2025-01-01T00:00:04Z","type":"event_msg","payload":{"type":"user_message","message":"Second question"}}
{"timestamp":"2025-01-01T00:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":80,"cached_input_tokens":0,"output_tokens":30}}}}
"#;
        std::fs::write(&path, jsonl).unwrap();
        let rounds = build_rounds(Some(&tmp), "codex", "test-2r", Default::default());
        assert_eq!(rounds.len(), 2);
        assert_eq!(rounds[0].user_text, "Second question");
        assert_eq!(rounds[0].turns, 1);
        assert_eq!(rounds[0].input_tokens, 80);
        assert_eq!(rounds[1].user_text, "First question");
        assert_eq!(rounds[1].turns, 1);
        assert_eq!(rounds[1].input_tokens, 100);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
