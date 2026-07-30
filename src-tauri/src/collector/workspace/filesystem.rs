//! Filesystem scanning over `~/.claude/projects/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::path_utils::{
    last_segment, mtime_to_date, mtime_to_hhmm, project_root, read_cwds, read_project_root,
    tilde_prefix,
};
use super::security::is_safe_workspace_key;
use super::types::{ClaudeFs, DecodedWorkspace};

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
        let has_jsonl = match std::fs::read_dir(&path) {
            Ok(entries) => entries
                .flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl")),
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
/// the session file's mtime as "MM-DD HH:MM" (local tz).
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

/// Build a precise session→project map by reading each session JSONL's `cwd`.
pub fn build_precise_session_map(
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
        let key = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if !is_safe_workspace_key(&key) {
            continue;
        }
        let dir_fallback =
            read_project_root(&dir).map(|(root, _date)| (last_segment(&root), tilde_prefix(&root)));
        let Ok(sessions) = std::fs::read_dir(&dir) else {
            continue;
        };
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
            if map.contains_key(&sid) {
                continue;
            }
            let cwds = read_cwds(&p);
            let root = project_root(&cwds);
            let (name, path) = match &root {
                Some(r) => (last_segment(r), tilde_prefix(r)),
                None => match &dir_fallback {
                    Some((n, fp)) => (n.clone(), fp.clone()),
                    None => continue,
                },
            };
            let mtime_str = std::fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|m| mtime_to_hhmm(m).ok());
            map.insert(sid, (name, path, mtime_str));
        }
    }
    map
}

/// Single-pass scan of `~/.claude/projects/`. Reads each session JSONL's cwd
/// (capped at 40 lines) to build both the session→project map and the unique
/// project list in one walk.
pub fn scan_claude_filesystem(claude_projects_dir: &Path) -> ClaudeFs {
    let mut session_map = HashMap::new();
    let mut proj_index: HashMap<String, (String, Option<String>)> = HashMap::new();

    let Ok(entries) = std::fs::read_dir(claude_projects_dir) else {
        return ClaudeFs {
            session_map,
            all_projects: Vec::new(),
        };
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let key = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        if !is_safe_workspace_key(&key) {
            continue;
        }
        let dir_fallback =
            read_project_root(&dir).map(|(root, _date)| (last_segment(&root), tilde_prefix(&root)));
        let Ok(sessions) = std::fs::read_dir(&dir) else {
            continue;
        };
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
            if session_map.contains_key(&sid) {
                continue;
            }
            let cwds = read_cwds(&p);
            let root = project_root(&cwds);
            let (name, path) = match &root {
                Some(r) => (last_segment(r), tilde_prefix(r)),
                None => match &dir_fallback {
                    Some((n, fp)) => (n.clone(), fp.clone()),
                    None => continue,
                },
            };
            let date = std::fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|m| mtime_to_date(m).ok());
            session_map.insert(sid, (name.clone(), path.clone(), date.clone()));
            match proj_index.get_mut(&path) {
                Some((_, d)) => {
                    if let Some(ref new_date) = date {
                        if d.as_ref().map_or(true, |old| new_date > old) {
                            *d = Some(new_date.clone());
                        }
                    }
                }
                None => {
                    proj_index.insert(path, (name, date));
                }
            }
        }
    }

    let all_projects: Vec<DecodedWorkspace> = proj_index
        .into_iter()
        .map(|(path, (name, date))| DecodedWorkspace {
            name,
            full_path: Some(path),
            latest_date: date,
        })
        .collect();

    ClaudeFs {
        session_map,
        all_projects,
    }
}
