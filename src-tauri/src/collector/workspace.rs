//! Project resolution from tokscale's `--group-by workspace,model` report.
//!
//! tokscale already groups token stats by workspace (project), so projects are
//! fetched as a **live tokscale query** rather than derived from the DB (whose
//! `daily_usage` table has no project dimension).
//!
//! Two responsibilities live here:
//!   1. [`parse_workspace_report`] — parse the report JSON into per-project
//!      aggregates (tokens / cost / messages / top models / top tools).
//!   2. [`decode_workspace`] — turn a tokscale `workspaceKey` into a human name,
//!      real path, and latest-interaction date.
//!
//! # Why we read session files for the name
//!
//! Claude Code encodes the cwd into its cache dir name by replacing `/` with
//! `-`, which is **lossy**: the real folder `bee_repair` is encoded as
//! `bee-repair`, so naively restoring `-` → `/` yields a wrong path. The
//! authoritative real path lives in the `cwd` field of the session jsonl files
//! under `~/.claude/projects/<key>/`. We read that instead of guessing. (This
//! is the same approach token-monitor takes.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One model or tool row within a project's detail breakdown.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectDetailRow {
    pub key: String,
    pub tokens: i64,
    pub pct: f64,
}

/// Decoded display info for a single workspace key.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecodedWorkspace {
    /// Last path segment — the project's short display name.
    pub name: String,
    /// `~`-prefixed absolute path for the detail view.
    pub full_path: Option<String>,
    /// `YYYY-MM-DD` of the most recent interaction, when discoverable.
    pub latest_date: Option<String>,
}

/// A fully-resolved project, ready to serialize to the frontend.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectAgg {
    pub name: String,
    pub full_path: Option<String>,
    pub latest_date: Option<String>,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
    pub models: Vec<ProjectDetailRow>,
    pub tools: Vec<ProjectDetailRow>,
}

/// Parse tokscale's `--group-by workspace,model` JSON into per-project
/// aggregates, sorted by tokens desc. `claude_projects_dir` (`~/.claude/projects`)
/// is used to decode Claude-encoded keys via session-cwd lookup; pass `None`
/// when the directory is unavailable.
pub fn parse_workspace_report(json: &Value, claude_projects_dir: Option<&Path>) -> Vec<ProjectAgg> {
    let Some(entries) = json.get("entries").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    let mut acc: HashMap<String, Acc> = HashMap::new();
    for e in entries {
        let key = match e.get("workspaceKey").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() && is_safe_workspace_key(s) => s.to_string(),
            _ => continue,
        };
        let label = e
            .get("workspaceLabel")
            .and_then(|v| v.as_str())
            .unwrap_or(&key)
            .to_string();
        let tokens = token_total(e);
        let cost = e.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let messages = e.get("messageCount").and_then(|v| v.as_i64()).unwrap_or(0);
        let model = e
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let client = e
            .get("client")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();

        let a = acc
            .entry(key.clone())
            .or_insert_with(|| Acc::new(key, label));
        a.tokens += tokens;
        a.cost += cost;
        a.messages += messages;
        *a.models.entry(model).or_insert(0) += tokens;
        *a.tools.entry(client).or_insert(0) += tokens;
    }

    let mut out: Vec<ProjectAgg> = acc
        .into_values()
        .map(|a| a.finalize(claude_projects_dir))
        .collect();
    out.sort_by_key(|y| std::cmp::Reverse(y.tokens));
    out
}

/// Decode a single workspace key/label into display info.
///
/// - `/`-prefixed keys are real absolute paths (non-Claude tools); the label is
///   already the clean last segment.
/// - Encoded keys (start with `-`) are Claude cache names; we read the
///   authoritative `cwd` from the newest session jsonl under
///   `claude_projects_dir/<key>/`.
/// - Falls back to the label when no cwd can be recovered.
pub fn decode_workspace(
    key: &str,
    label: &str,
    claude_projects_dir: Option<&Path>,
) -> DecodedWorkspace {
    if !is_safe_workspace_key(key) {
        return DecodedWorkspace::default();
    }
    if key.starts_with('/') {
        let latest = path_mtime_date(Path::new(key));
        return DecodedWorkspace {
            name: if label.is_empty() {
                last_segment(key)
            } else {
                label.to_string()
            },
            full_path: Some(tilde_prefix(key)),
            latest_date: latest,
        };
    }

    if let Some(dir) = claude_projects_dir {
        let proj_dir = dir.join(key);
        if let Some((root, date)) = read_project_root(&proj_dir) {
            return DecodedWorkspace {
                name: last_segment(&root),
                full_path: Some(tilde_prefix(&root)),
                latest_date: date,
            };
        }
    }

    // Last-resort fallback: the label (may still be encoded, but consistent).
    DecodedWorkspace {
        name: if label.is_empty() {
            key.to_string()
        } else {
            label.to_string()
        },
        full_path: None,
        latest_date: None,
    }
}

// ── security ────────────────────────────────────────────────────────────────

/// Reject workspace keys that could escape the intended directory.
/// Encoded keys must start with `-`; absolute keys must start with `/`.
/// Both are checked for `..` components via `Path` decomposition (handles
/// encoded and unencoded forms). Data source is trusted (local tokscale
/// binary); this is a defense-in-depth safety net.
fn is_safe_workspace_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    if std::path::Path::new(key)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return false;
    }
    key.starts_with('-') || key.starts_with('/')
}

// ── helpers ─────────────────────────────────────────────────────────────────

/// Token total for a report entry. Excludes `reasoning` (design §5.3
/// anti-double-count: reasoning is a subset of output).
fn token_total(e: &Value) -> i64 {
    let g = |k: &str| e.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    g("input") + g("output") + g("cacheRead") + g("cacheWrite")
}

/// `part / whole * 100`, 0 when whole is 0.
fn pct(part: i64, whole: i64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

/// Turn a `HashMap<key, tokens>` into sorted top-3 detail rows with percentages.
fn detail_rows(map: HashMap<String, i64>, total: i64) -> Vec<ProjectDetailRow> {
    let mut v: Vec<ProjectDetailRow> = map
        .into_iter()
        .map(|(key, tokens)| ProjectDetailRow {
            key,
            tokens,
            pct: pct(tokens, total),
        })
        .collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.tokens));
    v.truncate(3);
    v
}

/// Last segment of a path string (`/a/b/c` → `c`).
fn last_segment(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

/// Prefix an absolute path with `~` when it's under the user's home dir.
fn tilde_prefix(abs: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if abs == h.as_ref() {
            return "~".to_string();
        }
        if let Some(rest) = abs.strip_prefix(h.as_ref()) {
            return format!("~{}", rest);
        }
    }
    abs.to_string()
}

/// `YYYY-MM-DD` (local tz) of a path's mtime, when available.
fn path_mtime_date(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    mtime_to_date(m).ok()
}

/// Convert a `SystemTime` to a local `YYYY-MM-DD` string.
fn mtime_to_date(m: std::time::SystemTime) -> Result<String, String> {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = DateTime::from(m);
    Ok(dt.format("%Y-%m-%d").to_string())
}

/// Convert a `SystemTime` to a local `YYYY-MM-DD HH:MM` string.
fn mtime_to_hhmm(m: std::time::SystemTime) -> Result<String, String> {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = DateTime::from(m);
    Ok(dt.format("%Y-%m-%d %H:%M").to_string())
}

/// Read all `cwd` values from a session jsonl file (capped to keep it cheap).
fn read_cwds(path: &Path) -> Vec<String> {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines().take(500) {
        let Ok(line) = line else { break };
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                if !c.is_empty() {
                    out.push(c.to_string());
                }
            }
        }
    }
    out
}

/// From the newest `.jsonl` in `dir`, recover the project root (the cwd that is
/// an ancestor of every other cwd) and its mtime as a date. Returns `None` when
/// the directory is missing or holds no readable session with a cwd.
fn read_project_root(dir: &Path) -> Option<(String, Option<String>)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
            if newest.as_ref().map_or(true, |(_, tm)| m > *tm) {
                newest = Some((p, m));
            }
        }
    }
    let (path, mtime) = newest?;
    let cwds = read_cwds(&path);
    let root = project_root(&cwds)?;
    let date = mtime_to_date(mtime).ok();
    Some((root, date))
}

/// The cwd that is a path-ancestor of all others (the true project root).
/// Falls back to the shortest cwd when no single ancestor covers everything.
fn project_root(cwds: &[String]) -> Option<String> {
    if cwds.is_empty() {
        return None;
    }
    let mut sorted: Vec<&String> = cwds.iter().collect();
    sorted.sort_by_key(|s| s.len());
    for c in &sorted {
        let with_slash = format!("{}/", c);
        if cwds.iter().all(|o| o == *c || o.starts_with(&with_slash)) {
            return Some((*c).clone());
        }
    }
    sorted.first().map(|s| (*s).clone())
}

/// Scan `~/.claude/projects/` for all project directories that have JSONL session
/// files, and return their DecodedWorkspace info. This catches Claude Code projects
/// that tokscale skips when they have no activity in the current period — same
/// approach token-monitor uses: show everything, not just the active subset.
pub fn scan_claude_projects(claude_projects_dir: &Path) -> Vec<DecodedWorkspace> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(claude_projects_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let key = match path.file_name().and_then(|n| n.to_str()) {
            Some(k) => k.to_string(),
            None => continue,
        };
        if !is_safe_workspace_key(&key) {
            continue;
        }
        // Skip directories without any JSONL session files
        let has_jsonl = match std::fs::read_dir(&path) {
            Ok(entries) => entries.flatten().any(|e| {
                e.path().extension().and_then(|x| x.to_str()) == Some("jsonl")
            }),
            Err(_) => false,
        };
        if !has_jsonl {
            continue;
        }
        if let Some((root, date)) = read_project_root(&path) {
            out.push(DecodedWorkspace {
                name: last_segment(&root),
                full_path: Some(tilde_prefix(&root)),
                latest_date: date,
            });
        }
    }
    out
}

/// Find a session `.jsonl` file in `~/.claude/projects/` by its session ID
/// (the filename stem). Returns the full path if found.
pub fn find_session_file(claude_projects_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let Ok(entries) = std::fs::read_dir(claude_projects_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let candidate = dir.join(format!("{session_id}.jsonl"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Scan `~/.claude/projects/` and build a map from every known session ID
/// (the `.jsonl` stem) to the project's display name, `~`-prefixed path, and
/// the session file's mtime as "MM-DD HH:MM" (local tz). Each project dir is
/// quick-parsed once via [`read_project_root`]; the session IDs are collected
/// from the filenames without reading file contents.
pub fn session_project_map(
    claude_projects_dir: &Path,
) -> HashMap<String, (String, String, Option<String>)> {
    let mut map = HashMap::new();
    let Ok(entries) = std::fs::read_dir(claude_projects_dir) else {
        return map;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        let (proj_name, proj_path) = read_project_root(&dir).map_or_else(
            || (name.clone(), String::new()),
            |(root, _)| {
                let n = last_segment(&root);
                let f = tilde_prefix(&root);
                (n, f)
            },
        );
        if let Ok(sessions) = std::fs::read_dir(&dir) {
            for s in sessions.flatten() {
                let p = s.path();
                if p.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let sid = p
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                // Read the file's mtime for true "last interaction" time.
                let mtime_str = std::fs::metadata(&p)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|m| mtime_to_hhmm(m).ok());
                map.entry(sid)
                    .or_insert_with(|| (proj_name.clone(), proj_path.clone(), mtime_str));
            }
        }
    }
    map
}

/// In-flight accumulator for one workspace while iterating report entries.
struct Acc {
    key: String,
    label: String,
    tokens: i64,
    cost: f64,
    messages: i64,
    models: HashMap<String, i64>,
    tools: HashMap<String, i64>,
}

impl Acc {
    fn new(key: String, label: String) -> Self {
        Self {
            key,
            label,
            tokens: 0,
            cost: 0.0,
            messages: 0,
            models: HashMap::new(),
            tools: HashMap::new(),
        }
    }

    fn finalize(self, claude_projects_dir: Option<&Path>) -> ProjectAgg {
        let decoded = decode_workspace(&self.key, &self.label, claude_projects_dir);
        let total = self.tokens;
        ProjectAgg {
            name: decoded.name,
            full_path: decoded.full_path,
            latest_date: decoded.latest_date,
            tokens: self.tokens,
            cost_usd: self.cost,
            messages: self.messages,
            models: detail_rows(self.models, total),
            tools: detail_rows(self.tools, total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jsonl_line(cwd: &str) -> String {
        serde_json::json!({ "type": "user", "cwd": cwd, "message": {} }).to_string()
    }

    /// Create a fake Claude project dir with one session file whose cwd lines
    /// are the given values (one per line).
    fn fake_project(claude_dir: &Path, key: &str, cwds: &[&str]) -> PathBuf {
        let proj = claude_dir.join(key);
        std::fs::create_dir_all(&proj).unwrap();
        let session = proj.join("abc-123.jsonl");
        let mut f = std::fs::File::create(&session).unwrap();
        for c in cwds {
            writeln!(f, "{}", jsonl_line(c)).unwrap();
        }
        // File is created "now" → mtime is current, so date recovery succeeds.
        proj
    }

    #[test]
    fn workspace_key_rejects_dotdot() {
        assert!(!is_safe_workspace_key("/Users/z/../etc"));
        // Encoded names with literal ".." are NOT traversal — Path treats
        // them as a single Normal component. This is correct.
        assert!(is_safe_workspace_key("-Users-z-..-projects"));
    }

    #[test]
    fn workspace_key_accepts_legit_keys() {
        // Encoded Claude key
        assert!(is_safe_workspace_key("-Users-z-work-projects-token-usage"));
        // Absolute path under home
        let home = dirs::home_dir().unwrap_or_default();
        assert!(is_safe_workspace_key(home.to_str().unwrap()));
    }

    #[test]
    fn workspace_key_rejects_empty() {
        assert!(!is_safe_workspace_key(""));
    }

    #[test]
    fn last_segment_handles_trailing_slash() {
        assert_eq!(last_segment("/a/b/c"), "c");
        assert_eq!(last_segment("/a/b/c/"), "c");
        assert_eq!(last_segment("solo"), "solo");
    }

    #[test]
    fn pct_zero_whole() {
        assert_eq!(pct(5, 0), 0.0);
        assert_eq!(pct(25, 100), 25.0);
    }

    #[test]
    fn detail_rows_sorted_top3_with_pct() {
        let mut m = HashMap::new();
        m.insert("glm-5.2".to_string(), 800);
        m.insert("gpt-5".to_string(), 150);
        m.insert("claude".to_string(), 40);
        m.insert("step".to_string(), 10);
        let rows = detail_rows(m, 1000);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key, "glm-5.2");
        assert_eq!(rows[0].tokens, 800);
        assert!((rows[0].pct - 80.0).abs() < 1e-9);
        assert_eq!(rows[2].key, "claude"); // 40 > 10, step dropped
    }

    #[test]
    fn project_root_picks_common_ancestor() {
        let cwds = vec![
            "/Users/z/work/proj".to_string(),
            "/Users/z/work/proj/docs".to_string(),
            "/Users/z/work/proj/src-tauri/icons".to_string(),
        ];
        assert_eq!(project_root(&cwds).as_deref(), Some("/Users/z/work/proj"));
    }

    #[test]
    fn project_root_falls_back_to_shortest_when_no_ancestor() {
        // Sibling dirs only — no shared cwd → shortest.
        let cwds = vec!["/a/x".to_string(), "/a/y".to_string()];
        assert_eq!(project_root(&cwds).as_deref(), Some("/a/x"));
    }

    #[test]
    fn project_root_empty_is_none() {
        assert!(project_root(&[]).is_none());
    }

    #[test]
    fn decode_real_path_key_uses_label_and_stats_dir() {
        let tmp = std::env::temp_dir().join("tu_ws_realpath");
        let _ = std::fs::remove_dir_all(&tmp);
        let real = tmp.join("ZCodeProject");
        std::fs::create_dir_all(&real).unwrap();
        let d = decode_workspace(real.to_str().unwrap(), "ZCodeProject", None);
        assert_eq!(d.name, "ZCodeProject");
        // full_path is ~-prefixed only under home; in temp it stays absolute.
        assert!(d.full_path.as_deref().unwrap().ends_with("ZCodeProject"));
        assert!(d.latest_date.is_some()); // dir mtime → today
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn decode_encoded_key_reads_cwd_from_session() {
        let tmp = std::env::temp_dir().join("tu_ws_encoded");
        let _ = std::fs::remove_dir_all(&tmp);
        let claude_dir = tmp.join(".claude").join("projects");
        let key = "-Users-z-work-workspace-projects-bee-repair";
        // NOTE the underscore in the real folder — this is the lossy-encoding
        // case that defeats naive `-`→`/` restoration.
        fake_project(
            &claude_dir,
            key,
            &[
                "/Users/z/work/workspace/projects/bee_repair",
                "/Users/z/work/workspace/projects/bee_repair/admin/frontend",
            ],
        );
        let d = decode_workspace(key, key, Some(&claude_dir));
        assert_eq!(d.name, "bee_repair"); // underscore preserved — NOT "bee-repair"
        assert_eq!(
            d.full_path.as_deref(),
            Some("/Users/z/work/workspace/projects/bee_repair")
        );
        assert!(d.latest_date.is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn decode_encoded_key_without_session_falls_back_to_label() {
        let tmp = std::env::temp_dir().join("tu_ws_fallback");
        let _ = std::fs::remove_dir_all(&tmp);
        let claude_dir = tmp.join(".claude").join("projects");
        std::fs::create_dir_all(&claude_dir).unwrap();
        // key points at a non-existent project dir → no cwd → fallback.
        let d = decode_workspace("-Users-z-ghost", "-Users-z-ghost", Some(&claude_dir));
        assert_eq!(d.name, "-Users-z-ghost");
        assert!(d.full_path.is_none());
        assert!(d.latest_date.is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn parse_report_groups_by_workspace_and_decodes() {
        let tmp = std::env::temp_dir().join("tu_ws_parse");
        let _ = std::fs::remove_dir_all(&tmp);
        let claude_dir = tmp.join(".claude").join("projects");
        fake_project(
            &claude_dir,
            "-Users-z-p-token-usage",
            &["/Users/z/p/token-usage", "/Users/z/p/token-usage/docs"],
        );

        let json = serde_json::json!({
            "entries": [
                { "workspaceKey": "-Users-z-p-token-usage", "workspaceLabel": "-Users-z-p-token-usage",
                  "client": "claude", "model": "glm-5.2", "cost": 1.5, "messageCount": 3,
                  "input": 1000, "output": 200, "cacheRead": 300, "cacheWrite": 0, "reasoning": 999 },
                { "workspaceKey": "-Users-z-p-token-usage", "workspaceLabel": "-Users-z-p-token-usage",
                  "client": "claude", "model": "gpt-5", "cost": 0.5, "messageCount": 1,
                  "input": 500, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 },
                { "workspaceKey": "/Users/z/ZCodeProject", "workspaceLabel": "ZCodeProject",
                  "client": "zcode", "model": "step", "cost": 2.0, "messageCount": 5,
                  "input": 200, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0 }
            ]
        });

        let out = parse_workspace_report(&json, Some(&claude_dir));
        assert_eq!(out.len(), 2);

        // token-usage: glm-5.2 (1500, reasoning 999 excluded) + gpt-5 (500) = 2000.
        let tu = &out[0];
        assert_eq!(tu.name, "token-usage");
        assert_eq!(tu.tokens, 2000);
        assert!((tu.cost_usd - 2.0).abs() < 1e-9);
        assert_eq!(tu.messages, 4);
        assert_eq!(tu.models.len(), 2);
        assert_eq!(tu.models[0].key, "glm-5.2"); // 1500 > 500
        assert_eq!(tu.models[0].tokens, 1500);
        assert_eq!(tu.tools.len(), 1);
        assert_eq!(tu.tools[0].key, "claude");

        let z = &out[1];
        assert_eq!(z.name, "ZCodeProject");
        assert_eq!(z.tokens, 200);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn parse_report_empty_when_no_entries() {
        let out = parse_workspace_report(&serde_json::json!({}), None);
        assert!(out.is_empty());
        let out = parse_workspace_report(&serde_json::json!({ "entries": [] }), None);
        assert!(out.is_empty());
    }
}
