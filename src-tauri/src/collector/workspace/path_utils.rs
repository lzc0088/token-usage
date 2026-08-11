//! Path resolution and time-formatting utilities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use super::security::is_safe_workspace_key;

/// A size:mtime → result cache for `read_cwds`. When a JSONL file hasn't
/// changed (same file size AND mtime), the previous parse result is reused
/// without re-opening or re-parsing the file. This mirrors token-monitor's
/// `projectPathCache` / `jsonlTimestampCache` pattern.
///
/// Bounded to 200 entries to prevent unbounded memory growth in a long-running
/// menu-bar app. When the limit is hit, the oldest half of entries are evicted.
const CACHE_MAX: usize = 200;
type CwdCacheMap = HashMap<PathBuf, (u64, i64, Vec<String>)>;
static CWD_CACHE: Mutex<Option<CwdCacheMap>> = Mutex::new(None);

fn get_cached_cwds(path: &Path) -> Option<Vec<String>> {
    let (size, mtime) = file_size_mtime(path)?;
    // Recover from mutex poisoning (a panic during set_cached_cwds) instead of
    // silently disabling the cache for the rest of the process lifetime.
    let cache = CWD_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let entries = cache.as_ref()?;
    let cached = entries.get(path)?;
    if cached.0 == size && cached.1 == mtime {
        Some(cached.2.clone())
    } else {
        None
    }
}

fn set_cached_cwds(path: &Path, parsed: Vec<String>) {
    let Some((size, mtime)) = file_size_mtime(path) else {
        return;
    };
    if let Ok(mut cache) = CWD_CACHE.lock() {
        let entries = cache.get_or_insert_with(HashMap::new);
        if entries.len() >= CACHE_MAX {
            // Evict the oldest half (lowest mtime) to bound memory.
            let mut sorted_mtimes: Vec<_> = entries.values().map(|(_, mt, _)| *mt).collect();
            sorted_mtimes.sort();
            let cutoff = sorted_mtimes[sorted_mtimes.len() / 2];
            entries.retain(|_, (_, mtime, _)| *mtime > cutoff);
        }
        entries.insert(path.to_path_buf(), (size, mtime, parsed));
    }
}

fn file_size_mtime(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)?;
    Some((size, mtime))
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
) -> super::types::DecodedWorkspace {
    if !is_safe_workspace_key(key) {
        return super::types::DecodedWorkspace::default();
    }

    let name: String;
    let mut full_path: Option<String> = None;
    let mut latest_date: Option<String> = None;

    // Absolute paths (Unix `/...` or Windows `C:\...`) carry their own full
    // path; no need to look up session files.
    if Path::new(key).is_absolute() {
        // Filter out Claude's internal memory/observation paths so they don't
        // appear as user projects.
        if is_internal_claude_path(key) {
            return super::types::DecodedWorkspace::default();
        }
        name = if label.is_empty() {
            last_segment(key)
        } else {
            label.to_string()
        };
        full_path = Some(tilde_prefix(key));
        latest_date = path_mtime_date(Path::new(key));
    } else if let Some(dir) = claude_projects_dir {
        let proj_dir = dir.join(key);
        if let Some((root, date)) = read_project_root(&proj_dir) {
            if is_internal_claude_path(&root) {
                return super::types::DecodedWorkspace::default();
            }
            name = last_segment(&root);
            full_path = Some(tilde_prefix(&root));
            latest_date = date;
        } else {
            name = if label.is_empty() {
                key.to_string()
            } else {
                label.to_string()
            };
        }
    } else {
        name = if label.is_empty() {
            key.to_string()
        } else {
            label.to_string()
        };
    }

    super::types::DecodedWorkspace {
        name,
        full_path,
        latest_date,
    }
}

/// Last segment of a path string (`/a/b/c` → `c`, `C:\a\b\c` → `c`).
pub(crate) fn last_segment(path: &str) -> String {
    use std::path::Path;
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Prefix an absolute path with `~` when it's under the user's home dir.
pub(crate) fn tilde_prefix(abs: &str) -> String {
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
pub(crate) fn path_mtime_date(p: &Path) -> Option<String> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    mtime_to_date(m).ok()
}

/// Convert a `SystemTime` to a local `YYYY-MM-DD` string.
pub(crate) fn mtime_to_date(m: std::time::SystemTime) -> Result<String, String> {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = DateTime::from(m);
    Ok(dt.format("%Y-%m-%d").to_string())
}

/// Convert a `SystemTime` to a local `YYYY-MM-DD HH:MM` string.
pub(crate) fn mtime_to_hhmm(m: std::time::SystemTime) -> Result<String, String> {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = DateTime::from(m);
    Ok(dt.format("%Y-%m-%d %H:%M").to_string())
}

/// Read all `cwd` values from a session jsonl file (capped to 40 lines).
///
/// Uses a `size×mtime` weak cache so idle session files aren't re-read on
/// every tick/query. A file is only re-parsed when its size or modification
/// time has changed — matching token-monitor's `projectPathCache` approach.
pub(crate) fn read_cwds(path: &Path) -> Vec<String> {
    // Check cache first (size×mtime weak cache).
    if let Some(cached) = get_cached_cwds(path) {
        return cached;
    }

    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in std::io::BufReader::new(file).lines().take(40) {
        let Ok(line) = line else { break };
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if let Some(c) = v.get("cwd").and_then(|x| x.as_str()) {
                if !c.is_empty() {
                    out.push(c.to_string());
                }
            }
        }
    }

    set_cached_cwds(path, out.clone());
    out
}

/// From the newest `.jsonl` in `dir`, recover the project root (the cwd that is
/// an ancestor of every other cwd) and its mtime as a date. Returns `None` when
/// the directory is missing or holds no readable session with a cwd.
pub(crate) fn read_project_root(dir: &Path) -> Option<(String, Option<String>)> {
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
pub(crate) fn project_root(cwds: &[String]) -> Option<String> {
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

/// Check whether a decoded project path belongs to Claude's internal
/// memory/observation system rather than a user coding project.
/// Paths like `~/.claude-mem/observer-sessions` should be filtered out.
fn is_internal_claude_path(path: &str) -> bool {
    path.contains(".claude-mem") || path.contains("/observer-") || path.contains("\\observer-")
}
