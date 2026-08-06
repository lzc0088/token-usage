//! Shared data types for the workspace module.

use std::collections::HashMap;

/// One model or tool row within a project's detail breakdown.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// Result of a single filesystem scan over `~/.claude/projects/`.
/// `session_map` maps every session id to its project (name, path, mtime);
/// `all_projects` is the deduplicated set of projects discovered — used to
/// surface projects with zero activity in the queried period.
pub struct ClaudeFs {
    pub session_map: HashMap<String, (String, String, Option<String>)>,
    pub all_projects: Vec<DecodedWorkspace>,
}
