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

mod filesystem;
mod path_utils;
mod projects;
mod report;
mod security;
pub mod types;

pub use filesystem::{
    build_precise_session_map, find_session_file, scan_claude_filesystem, scan_claude_projects,
    session_project_map,
};
pub use path_utils::decode_workspace;
pub use projects::{
    build_projects_from_sessions, build_projects_from_sessions_with_map, merge_project,
};
pub use report::{filter_out_client, parse_workspace_report};
pub use types::{ClaudeFs, DecodedWorkspace, ProjectAgg, ProjectDetailRow};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jsonl_line(cwd: &str) -> String {
        serde_json::json!({ "type": "user", "cwd": cwd, "message": {} }).to_string()
    }

    /// Create a fake Claude project dir with one session file whose cwd lines
    /// are the given values (one per line).
    fn fake_project(claude_dir: &std::path::Path, key: &str, cwds: &[&str]) -> std::path::PathBuf {
        let proj = claude_dir.join(key);
        std::fs::create_dir_all(&proj).unwrap();
        let session = proj.join("abc-123.jsonl");
        let mut f = std::fs::File::create(&session).unwrap();
        for c in cwds {
            writeln!(f, "{}", jsonl_line(c)).unwrap();
        }
        proj
    }

    #[test]
    fn workspace_key_rejects_dotdot() {
        assert!(!security::is_safe_workspace_key("/Users/z/../etc"));
        assert!(security::is_safe_workspace_key("-Users-z-..-projects"));
    }

    #[test]
    fn workspace_key_accepts_legit_keys() {
        assert!(security::is_safe_workspace_key(
            "-Users-z-work-projects-token-usage"
        ));
        let home = dirs::home_dir().unwrap_or_default();
        assert!(security::is_safe_workspace_key(home.to_str().unwrap()));
    }

    #[test]
    fn workspace_key_rejects_empty() {
        assert!(!security::is_safe_workspace_key(""));
    }

    #[test]
    fn last_segment_handles_trailing_slash() {
        assert_eq!(path_utils::last_segment("/a/b/c"), "c");
        assert_eq!(path_utils::last_segment("/a/b/c/"), "c");
        assert_eq!(path_utils::last_segment("solo"), "solo");
    }

    #[test]
    fn pct_zero_whole() {
        assert_eq!(report::pct(5, 0), 0.0);
        assert_eq!(report::pct(25, 100), 25.0);
    }

    #[test]
    fn detail_rows_sorted_top3_with_pct() {
        let mut m = std::collections::HashMap::new();
        m.insert("glm-5.2".to_string(), 800);
        m.insert("gpt-5".to_string(), 150);
        m.insert("claude".to_string(), 40);
        m.insert("step".to_string(), 10);
        let rows = report::detail_rows(m, 1000);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key, "glm-5.2");
        assert_eq!(rows[0].tokens, 800);
        assert!((rows[0].pct - 80.0).abs() < 1e-9);
        assert_eq!(rows[2].key, "claude");
    }

    #[test]
    fn project_root_picks_common_ancestor() {
        let cwds = vec![
            "/Users/z/work/proj".to_string(),
            "/Users/z/work/proj/docs".to_string(),
            "/Users/z/work/proj/src-tauri/icons".to_string(),
        ];
        assert_eq!(
            path_utils::project_root(&cwds).as_deref(),
            Some("/Users/z/work/proj")
        );
    }

    #[test]
    fn project_root_falls_back_to_shortest_when_no_ancestor() {
        let cwds = vec!["/a/x".to_string(), "/a/y".to_string()];
        assert_eq!(path_utils::project_root(&cwds).as_deref(), Some("/a/x"));
    }

    #[test]
    fn project_root_empty_is_none() {
        assert!(path_utils::project_root(&[]).is_none());
    }

    #[test]
    fn decode_real_path_key_uses_label_and_stats_dir() {
        let tmp = std::env::temp_dir().join("tu_ws_realpath");
        let _ = std::fs::remove_dir_all(&tmp);
        let real = tmp.join("ZCodeProject");
        std::fs::create_dir_all(&real).unwrap();
        let d = decode_workspace(real.to_str().unwrap(), "ZCodeProject", None);
        assert_eq!(d.name, "ZCodeProject");
        assert!(d.full_path.as_deref().unwrap().ends_with("ZCodeProject"));
        assert!(d.latest_date.is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn decode_encoded_key_reads_cwd_from_session() {
        let tmp = std::env::temp_dir().join("tu_ws_encoded");
        let _ = std::fs::remove_dir_all(&tmp);
        let claude_dir = tmp.join(".claude").join("projects");
        let key = "-Users-z-work-workspace-projects-bee-repair";
        fake_project(
            &claude_dir,
            key,
            &[
                "/Users/z/work/workspace/projects/bee_repair",
                "/Users/z/work/workspace/projects/bee_repair/admin/frontend",
            ],
        );
        let d = decode_workspace(key, key, Some(&claude_dir));
        assert_eq!(d.name, "bee_repair");
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

        let tu = &out[0];
        assert_eq!(tu.name, "token-usage");
        assert_eq!(tu.tokens, 2000);
        assert!((tu.cost_usd - 2.0).abs() < 1e-9);
        assert_eq!(tu.messages, 4);
        assert_eq!(tu.models.len(), 2);
        assert_eq!(tu.models[0].key, "glm-5.2");
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
