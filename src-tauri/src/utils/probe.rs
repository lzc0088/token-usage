//! Best-effort "is this tool actually installed on the machine" detection.
//!
//! tokscale reports `sessionsPathExists`, but that only checks the tool's
//! *sessions* directory — a freshly installed tool (no sessions yet) or a tool
//! whose data lives elsewhere shows as "not installed" even though the app is
//! clearly on the system (e.g. Warp). This module adds real install probes
//! (macOS `.app` bundles + per-tool config/data dirs) so the 采集追踪 status
//! reflects actual installation rather than just "has sessions".
//!
//! Pure path-existence checks — cheap, no process spawning. A tool is
//! considered installed if ANY of its probe paths exists.

use std::path::PathBuf;

/// Curated install indicators per tokscale client id. Templates:
///   `{home}`    → user home dir
///   `{apps}`    → `/Applications` (macOS only)
///   `{data}`    → platform local data dir (`~/Library/Application Support` on mac,
///                 `~/.local/share` on linux, `%LOCALAPPDATA%` on windows)
///   `{config}`  → platform config dir (`~/Library/Application Support` mac,
///                 `~/.config` linux, `%APPDATA%` windows)
const PROBES: &[(&str, &[&str])] = &[
    ("claude", &["{home}/.claude"]),
    ("codex", &["{home}/.codex", "{apps}/ChatGPT.app", "{apps}/Codex.app"]),
    ("cursor", &["{apps}/Cursor.app", "{config}/tokscale/cursor-cache"]),
    ("warp", &["{apps}/Warp.app", "{home}/.warp"]),
    ("zed", &["{apps}/Zed.app", "{home}/.zed"]),
    ("gemini", &["{home}/.gemini"]),
    ("opencode", &["{home}/.local/share/opencode", "{data}/opencode"]),
    ("openclaw", &["{home}/.openclaw"]),
    ("kimi", &["{home}/.kimi", "{home}/.kimi-code"]),
    ("qwen", &["{home}/.qwen"]),
    ("grok", &["{home}/.grok"]),
    ("copilot", &["{home}/.copilot", "{home}/.githubcopilot"]),
    ("kiro", &["{apps}/Kiro.app", "{home}/.kiro"]),
    ("trae", &["{apps}/Trae.app", "{apps}/Trae CN.app"]),
    ("zcode", &["{home}/.zcode"]),
    ("micode", &["{home}/.local/share/mimocode", "{data}/mimocode"]),
    ("cline", &["{home}/.cline", "{config}/cline"]),
    ("antigravity", &["{apps}/Antigravity.app", "{config}/tokscale/antigravity-cache"]),
    ("antigravity-cli", &["{apps}/Antigravity.app"]),
    ("goose", &["{config}/goose", "{home}/.goose"]),
    ("roocode", &["{config}/Code/User/globalStorage/rooveterinaryinc.roo-cline"]),
    ("amp", &["{home}/.config/amp", "{home}/.amp"]),
    ("droid", &["{home}/.factory", "{config}/factory"]),
    ("codebuff", &["{home}/.codebuff", "{config}/codebuff"]),
    ("junie", &["{apps}/Junie.app", "{config}/Junie"]),
    ("devin-cli", &["{home}/.devin", "{config}/devin"]),
    ("devin-desktop", &["{apps}/Devin.app", "{home}/.devin"]),
    ("codebuddy", &["{home}/.codebuddy"]),
    ("workbuddy", &["{home}/.workbuddy"]),
    ("kilocode", &["{config}/Code/User/globalStorage/kilocode.kilo-code"]),
    ("kilo", &["{home}/.kilo", "{config}/kilo"]),
];

fn resolve(template: &str) -> Option<PathBuf> {
    if let Some(rest) = template.strip_prefix("{home}/") {
        return dirs::home_dir().map(|h| h.join(rest));
    }
    if let Some(rest) = template.strip_prefix("{apps}/") {
        // macOS only; on other platforms /Applications doesn't exist, so the
        // probe simply won't match (returns a path that won't exist).
        return Some(PathBuf::from("/Applications").join(rest));
    }
    if let Some(rest) = template.strip_prefix("{data}/") {
        return dirs::data_dir().map(|d| d.join(rest));
    }
    if let Some(rest) = template.strip_prefix("{config}/") {
        return dirs::config_dir().map(|d| d.join(rest));
    }
    None
}

/// True if any install indicator for `client` exists on the local machine.
/// Unknown clients (no curated probes) return false — callers should combine
/// this with tokscale's sessions-path check.
pub fn is_installed(client: &str) -> bool {
    let Some(templates) = PROBES.iter().find(|(c, _)| *c == client).map(|(_, t)| *t) else {
        return false;
    };
    templates.iter().any(|t| resolve(t).is_some_and(|p| p.exists()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_clients_have_probes() {
        // Spot-check a few common tools have curated probes.
        assert!(PROBES.iter().any(|(c, _)| *c == "warp"));
        assert!(PROBES.iter().any(|(c, _)| *c == "cursor"));
        assert!(PROBES.iter().any(|(c, _)| *c == "claude"));
    }

    #[test]
    fn unknown_client_returns_false() {
        assert!(!is_installed("totally-made-up-tool-xyz"));
    }

    #[test]
    fn resolve_handles_all_templates() {
        // {home} always resolves on a real system.
        assert!(resolve("{home}/.claude").is_some());
        // {apps} always resolves (path may not exist, but resolution works).
        assert!(resolve("{apps}/Warp.app").is_some());
    }
}
