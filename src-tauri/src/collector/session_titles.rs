//! Session title resolution from local tool stores (2026-09, borrowed from
//! token-monitor #658). Titles make the Sessions page readable instead of
//! bare UUIDs. Local reads only — nothing leaves the machine.
//!
//! - Claude: the transcript JSONL carries an `ai-title` / `custom-title` row
//!   near the head of the file (observed at line ~32; we scan the first 400
//!   lines and give up — a missing title is normal, not an error).
//! - Codex: `~/.codex/state_<N>.sqlite` (highest N first, incl. the `sqlite/`
//!   subdirectory layout) keeps a `threads` table with a `title` column; the
//!   thread id matches the session id.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::OpenFlags;

/// In-memory cache: "tool|session_id" → resolved title. `None` values are
/// cached too (negative cache) so repeated `get_sessions` calls don't re-read
/// files for sessions that simply have no title. Per-app-run state only.
static TITLE_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

/// Resolve a session's title for the UI. Cheap after the first call (cache).
pub fn session_title(home: Option<&Path>, tool: &str, session_id: &str) -> Option<String> {
    let key = format!("{tool}|{session_id}");
    let map = TITLE_CACHE.get_or_init(Default::default);
    if let Ok(m) = map.lock() {
        if let Some(v) = m.get(&key) {
            return v.clone();
        }
    }
    let resolved = match (tool, home) {
        ("claude", Some(h)) => claude_title(h, session_id),
        ("codex", Some(h)) => codex_title(h, session_id),
        _ => None,
    };
    if let Ok(mut m) = map.lock() {
        m.insert(key, resolved.clone());
    }
    resolved
}

/// How many transcript lines to scan for a title row before giving up. The
/// row lands near the head in practice; a whole-file scan is not worth it.
const CLAUDE_SCAN_LINES: usize = 400;

fn claude_title(home: &Path, session_id: &str) -> Option<String> {
    let projects = home.join(".claude").join("projects");
    // The transcript lives under one of the (few dozen) project dirs — probe
    // each dir directly by name instead of walking the tree.
    let mut transcript: Option<PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(&projects) {
        for entry in entries.flatten() {
            let cand = entry.path().join(format!("{session_id}.jsonl"));
            if cand.is_file() {
                transcript = Some(cand);
                break;
            }
        }
    }
    let file = std::fs::File::open(transcript?).ok()?;
    for line in std::io::BufReader::new(file)
        .lines()
        .take(CLAUDE_SCAN_LINES)
    {
        let line = line.ok()?;
        // Fast pre-filter: JSON-parse only the rare title rows.
        if !line.contains("ai-title") && !line.contains("custom-title") {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            // "ai-title" rows carry aiTitle; "custom-title" rows carry title.
            if let Some(t) = v.get("aiTitle").and_then(|x| x.as_str()) {
                return Some(t.to_string());
            }
            if let Some(t) = v.get("title").and_then(|x| x.as_str()) {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Codex's `threads.title` frequently holds the full first user message or a
/// system prompt (verified against real data, 2026-09: rows with 1000+ char
/// multi-line instruction blobs). Only short single-line values read as
/// titles — anything longer is treated as "no title" rather than flooding
/// the session row.
const CODEX_TITLE_MAX_CHARS: usize = 80;

fn sanitize_codex_title(raw: String) -> Option<String> {
    let first_line = raw.lines().next()?.trim();
    if first_line.is_empty() || first_line.chars().count() > CODEX_TITLE_MAX_CHARS {
        return None;
    }
    Some(first_line.to_string())
}

/// Codex: newest `state_<N>.sqlite` under `~/.codex` (and its `sqlite/`
/// subdir) that has a matching thread row wins.
fn codex_title(home: &Path, session_id: &str) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for base in [home.join(".codex"), home.join(".codex").join("sqlite")] {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.file_name()?.to_str()?.starts_with("state_")
                    && p.extension().map(|e| e == "sqlite").unwrap_or(false)
                {
                    candidates.push(p);
                }
            }
        }
    }
    // Highest version first (state_5 before state_4).
    candidates.sort_by(|a, b| b.cmp(a));
    for db in candidates {
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        // _sqlx databases can carry WAL sidecars; a short busy timeout keeps
        // a concurrent Codex write from failing the read outright.
        let _ = conn.busy_timeout(std::time::Duration::from_millis(250));
        let title: Option<String> = conn
            .query_row(
                "SELECT title FROM threads WHERE id = ?",
                rusqlite::params![session_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(t) = title {
            if let Some(clean) = sanitize_codex_title(t) {
                return Some(clean);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp home per test (the project has no tempfile dep — the
    /// storage tests' std::env::temp_dir pattern). Returns (home, cleanup).
    fn fixture_home(tag: &str) -> (std::path::PathBuf, fn(&str)) {
        let dir = std::env::temp_dir().join(format!("tu_titles_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".claude").join("projects").join("proj-a")).unwrap();
        let cleanup = |t: &str| {
            let _ = std::fs::remove_dir_all(
                std::env::temp_dir().join(format!("tu_titles_{t}_{}", std::process::id())),
            );
        };
        (dir, cleanup)
    }
    /// Write a transcript with a title row at `at_line` (1-based).
    fn write_transcript(home: &Path, sid: &str, at_line: usize, kind: &str, title: &str) {
        let proj = home.join(".claude").join("projects").join("proj-a");
        let field = if kind == "ai-title" {
            "aiTitle"
        } else {
            "title"
        };
        let mut body = String::new();
        for i in 1..at_line {
            body.push_str(&format!("{{\"type\":\"user\",\"i\":{i}}}\n"));
        }
        body.push_str(&format!(
            "{{\"type\":\"{kind}\",\"{field}\":\"{title}\",\"sessionId\":\"{sid}\"}}\n"
        ));
        body.push_str("{\"type\":\"user\",\"tail\":true}\n");
        std::fs::write(proj.join(format!("{sid}.jsonl")), body).unwrap();
    }

    #[test]
    fn claude_title_reads_ai_title_row() {
        let (home, cleanup) = fixture_home("ai");
        write_transcript(&home, "s-1", 3, "ai-title", "分析迁移后的项目结构");
        assert_eq!(
            claude_title(&home, "s-1").as_deref(),
            Some("分析迁移后的项目结构")
        );
        cleanup("ai");
    }

    #[test]
    fn claude_title_reads_custom_title_row() {
        let (home, cleanup) = fixture_home("custom");
        write_transcript(&home, "s-2", 1, "custom-title", "自定义标题");
        assert_eq!(claude_title(&home, "s-2").as_deref(), Some("自定义标题"));
        cleanup("custom");
    }

    #[test]
    fn claude_title_none_when_row_beyond_scan_window() {
        let (home, cleanup) = fixture_home("far");
        write_transcript(&home, "s-3", CLAUDE_SCAN_LINES + 5, "ai-title", "太远了");
        assert_eq!(claude_title(&home, "s-3"), None);
        cleanup("far");
    }

    #[test]
    fn claude_title_none_for_missing_transcript() {
        let (home, cleanup) = fixture_home("missing");
        assert_eq!(claude_title(&home, "nope"), None);
        cleanup("missing");
    }

    #[test]
    fn session_title_caches_by_tool_and_id() {
        let (home, cleanup) = fixture_home("cache");
        // Unique id so the shared process-wide cache can't collide with
        // sibling tests (same lesson as the stepfun TOKEN_CACHE fix).
        let sid = format!("s-cache-{}", std::process::id());
        write_transcript(&home, &sid, 2, "ai-title", "缓存命中");
        assert_eq!(
            session_title(Some(&home), "claude", &sid).as_deref(),
            Some("缓存命中")
        );
        // Second call hits the cache (same result even after the file is gone).
        std::fs::remove_file(
            home.join(".claude")
                .join("projects")
                .join("proj-a")
                .join(format!("{sid}.jsonl")),
        )
        .unwrap();
        assert_eq!(
            session_title(Some(&home), "claude", &sid).as_deref(),
            Some("缓存命中")
        );
        // Unknown tool → None without touching the FS.
        assert_eq!(session_title(Some(&home), "qoder", &sid), None);
        cleanup("cache");
    }

    #[test]
    fn codex_title_reads_newest_state_db() {
        let tag = "codex";
        let home = std::env::temp_dir().join(format!("tu_titles_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let codex = home.join(".codex");
        std::fs::create_dir_all(codex.join("sqlite")).unwrap();
        for (n, title) in [(4u32, Some("旧线程")), (5, None)] {
            let db = if n == 5 {
                codex.join("sqlite").join(format!("state_{n}.sqlite"))
            } else {
                codex.join(format!("state_{n}.sqlite"))
            };
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT NOT NULL, updated_at INTEGER NOT NULL)",
            )
            .unwrap();
            if let Some(t) = title {
                conn.execute(
                    "INSERT INTO threads (id, title, updated_at) VALUES ('t-1', ?1, 1)",
                    rusqlite::params![t],
                )
                .unwrap();
            }
        }
        // state_5 exists but has no matching row → falls through to state_4.
        assert_eq!(codex_title(&home, "t-1").as_deref(), Some("旧线程"));
        assert_eq!(codex_title(&home, "missing"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn codex_title_rejects_message_blobs() {
        // Real-data shapes: a 1000+ char multi-line system prompt, and a long
        // first-user-message — neither is a title. Short first lines pass.
        assert_eq!(
            sanitize_codex_title("# 你和这位用户的关系\n你们认识 26 天了\n…正文…".into()),
            Some("# 你和这位用户的关系".into())
        );
        let long = "长".repeat(CODEX_TITLE_MAX_CHARS + 1);
        assert_eq!(sanitize_codex_title(long), None);
        assert_eq!(sanitize_codex_title("你好".into()), Some("你好".into()));
        assert_eq!(sanitize_codex_title("".into()), None);
        assert_eq!(sanitize_codex_title("\n\n".into()), None);
    }
}
