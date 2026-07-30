//! Path resolution and time-formatting utilities.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::security::is_safe_workspace_key;

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
    if key.starts_with('/') {
        let latest = path_mtime_date(Path::new(key));
        return super::types::DecodedWorkspace {
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
            return super::types::DecodedWorkspace {
                name: last_segment(&root),
                full_path: Some(tilde_prefix(&root)),
                latest_date: date,
            };
        }
    }

    // Last-resort fallback: the label (may still be encoded, but consistent).
    super::types::DecodedWorkspace {
        name: if label.is_empty() {
            key.to_string()
        } else {
            label.to_string()
        },
        full_path: None,
        latest_date: None,
    }
}

/// Last segment of a path string (`/a/b/c` → `c`).
pub(crate) fn last_segment(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
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

/// Read all `cwd` values from a session jsonl file (capped to keep it cheap).
pub(crate) fn read_cwds(path: &Path) -> Vec<String> {
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
