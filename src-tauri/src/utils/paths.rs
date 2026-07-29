//! Tool data-directory discovery for the file watcher (T1.4).
//!
//! Rather than hardcode a brittle per-platform path table, we ask tokscale where
//! each tool stores its sessions via `tokscale clients --json`. tokscale resolves
//! the platform-correct paths internally and reports existence, so this stays
//! correct as tokscale adds tools or tools change their layout.
//!
//! Output shape (verified 2026-07-17, v4.5.3):
//!   { headlessRoots: [..], clients: [ { client, label, sessionsPath,
//!     sessionsPathExists, additionalPaths: [{path, exists}],
//!     headlessPaths: [{path, exists}], ... } ] }

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::collector::tokscale::{self, TokscaleError};

/// Top-level `tokscale clients --json` report.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientsReport {
    #[serde(default)]
    pub headless_roots: Vec<String>,
    pub clients: Vec<ClientInfo>,
}

/// One tool's location info.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub client: String,
    pub label: String,
    pub sessions_path: String,
    #[serde(default)]
    pub sessions_path_exists: bool,
    #[serde(default)]
    pub additional_paths: Vec<PathEntry>,
    #[serde(default)]
    pub headless_paths: Vec<PathEntry>,
    /// Legacy/renamed session directories from older tool versions (v4.7.0+).
    /// e.g. OpenClaw reports `.clawdbot`, `.moltbot`, `.moldbot` legacy paths.
    #[serde(default)]
    pub legacy_paths: Vec<PathEntry>,
    #[serde(default)]
    pub message_count: i64,
    /// Optional diagnostic messages from tokscale (v4.7.0+).
    /// e.g. Claude reports stats-cache.json warnings.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// A tokscale client diagnostic message (v4.7.0+).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub paths: Vec<PathEntry>,
}

/// A secondary path with its existence flag.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    pub path: String,
    #[serde(default)]
    pub exists: bool,
}

impl ClientInfo {
    /// Is this tool installed (its primary sessions dir exists)?
    ///
    /// Some tools (e.g. zcode) use a different storage layout: the primary
    /// sessions path doesn't exist, but there is an existing additional path
    /// (e.g. `~/.zcode/cli/db/db.sqlite`) and tokscale reports a non-zero
    /// message count. We include those too so their data gets collected.
    pub fn is_installed(&self) -> bool {
        self.sessions_path_exists || self.message_count > 0
    }
}

/// Argv for `tokscale clients --json`. `--no-spinner` was removed from the
/// clients subcommand in tokscale v4.7.0 (still present on graph/report).
pub fn clients_args() -> Vec<String> {
    vec!["clients".into(), "--json".into()]
}

/// Locations to watch, deduped, using tokscale's reported existence flags
/// (pure — no filesystem access). Covers each client's sessions path plus any
/// existing `additionalPaths` / `headlessPaths`. May include **files** (e.g.
/// zcode's `db.sqlite`); the watcher (T1.4) normalizes files → parent dir.
pub fn watch_paths(report: &ClientsReport) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut push = |p: PathBuf, out: &mut Vec<PathBuf>| {
        if seen.insert(p.clone()) {
            out.push(p);
        }
    };
    for c in &report.clients {
        if c.sessions_path_exists {
            push(PathBuf::from(&c.sessions_path), &mut out);
        }
        for e in c
            .additional_paths
            .iter()
            .chain(c.headless_paths.iter())
            .chain(c.legacy_paths.iter())
        {
            if e.exists {
                push(PathBuf::from(&e.path), &mut out);
            }
        }
    }
    out
}

/// Only the installed clients (sessions dir exists). Drives the settings→采集
/// status list and decides which tools to watch.
pub fn installed_clients(report: &ClientsReport) -> Vec<&ClientInfo> {
    report.clients.iter().filter(|c| c.is_installed()).collect()
}

/// Run `tokscale clients --json` via the resolved binary and deserialize.
pub async fn fetch_clients(bin: &Path) -> Result<ClientsReport, TokscaleError> {
    let value = tokscale::run_json(bin, &clients_args()).await?;
    serde_json::from_value(value).map_err(TokscaleError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ClientsReport {
        // Mirrors a slice of the real tokscale clients --json output.
        serde_json::from_value(serde_json::json!({
            "headlessRoots": [
                "/home/u/.config/tokscale/headless"
            ],
            "clients": [
                {
                    "client": "opencode",
                    "label": "OpenCode",
                    "sessionsPath": "/home/u/.local/share/opencode/storage/message",
                    "sessionsPathExists": false,
                    "messageCount": 268
                },
                {
                    "client": "claude",
                    "label": "Claude Code",
                    "sessionsPath": "/home/u/.claude/projects",
                    "sessionsPathExists": true,
                    "additionalPaths": [
                        {"path": "/home/u/.claude/transcripts", "exists": false},
                        {"path": "/home/u/.claude/something", "exists": true}
                    ],
                    "messageCount": 41134
                },
                {
                    "client": "codex",
                    "label": "Codex CLI",
                    "sessionsPath": "/home/u/.codex/sessions",
                    "sessionsPathExists": true,
                    "headlessPaths": [
                        {"path": "/home/u/.config/tokscale/headless/codex", "exists": true}
                    ],
                    "messageCount": 2902
                }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn parses_real_shape_ignoring_unknown_fields() {
        // Extra unknown fields (messageCount, diagnostics, ...) must not break parsing.
        let r: ClientsReport = serde_json::from_str(
            r#"{"headlessRoots":[],"clients":[{"client":"x","label":"X","sessionsPath":"/p","sessionsPathExists":true,"unknownFutureField":42}]}"#,
        )
        .unwrap();
        assert_eq!(r.clients.len(), 1);
        assert_eq!(r.clients[0].client, "x");
    }

    #[test]
    fn installed_clients_includes_those_with_data() {
        let r = fixture();
        let installed: Vec<&str> = installed_clients(&r)
            .into_iter()
            .map(|c| c.client.as_str())
            .collect();
        // opencode has sessionsPathExists=false but messageCount=268 → still included
        assert!(installed.contains(&"claude"));
        assert!(installed.contains(&"codex"));
        assert!(installed.contains(&"opencode"));
    }

    #[test]
    fn watch_dirs_includes_sessions_and_existing_extras_skips_missing() {
        let r = fixture();
        let dirs = watch_paths(&r);
        // opencode skipped (sessionsPathExists=false);
        // claude: projects ✓ + transcripts ✗ + something ✓;
        // codex: sessions ✓ + headless/codex ✓.
        assert!(dirs.contains(&PathBuf::from("/home/u/.claude/projects")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.claude/something")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.codex/sessions")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.config/tokscale/headless/codex")));
        // missing ones excluded
        assert!(!dirs.contains(&PathBuf::from(
            "/home/u/.local/share/opencode/storage/message"
        )));
        assert!(!dirs.contains(&PathBuf::from("/home/u/.claude/transcripts")));
        assert_eq!(dirs.len(), 4);
    }

    #[test]
    fn watch_dirs_dedupes() {
        let r = ClientsReport {
            headless_roots: vec![],
            clients: vec![ClientInfo {
                client: "x".into(),
                label: "X".into(),
                sessions_path: "/dup".into(),
                sessions_path_exists: true,
                additional_paths: vec![PathEntry {
                    path: "/dup".into(),
                    exists: true,
                }],
                headless_paths: vec![],
                legacy_paths: vec![],
                diagnostics: vec![],
                message_count: 0,
            }],
        };
        assert_eq!(watch_paths(&r), vec![PathBuf::from("/dup")]);
    }

    #[test]
    fn watch_dirs_includes_legacy_paths() {
        let r = ClientsReport {
            headless_roots: vec![],
            clients: vec![ClientInfo {
                client: "openclaw".into(),
                label: "OpenClaw".into(),
                sessions_path: "/a".into(),
                sessions_path_exists: false,
                additional_paths: vec![],
                headless_paths: vec![],
                legacy_paths: vec![PathEntry {
                    path: "/home/u/.clawdbot/agents".into(),
                    exists: true,
                }],
                diagnostics: vec![],
                message_count: 100,
            }],
        };
        let dirs = watch_paths(&r);
        assert!(dirs.contains(&PathBuf::from("/home/u/.clawdbot/agents")));
    }

    #[test]
    fn clients_args_order_subcommand_first() {
        let a = clients_args();
        // v4.7.0 removed --no-spinner from the clients subcommand.
        assert_eq!(a[0], "clients");
        assert!(a.contains(&"--json".to_string()));
    }
}
