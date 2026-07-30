//! JSON report parsing and token aggregation for workspace reports.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use super::path_utils::decode_workspace;
use super::security::is_safe_workspace_key;
use super::types::{ProjectAgg, ProjectDetailRow};

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

/// Token total for a report entry. Excludes `reasoning` (design §5.3
/// anti-double-count: reasoning is a subset of output).
pub(crate) fn token_total(e: &Value) -> i64 {
    let g = |k: &str| e.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    g("input") + g("output") + g("cacheRead") + g("cacheWrite")
}

/// `part / whole * 100`, 0 when whole is 0.
pub(crate) fn pct(part: i64, whole: i64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

/// Turn a `HashMap<key, tokens>` into sorted top-3 detail rows with percentages.
pub(crate) fn detail_rows(map: HashMap<String, i64>, total: i64) -> Vec<ProjectDetailRow> {
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

/// Recompute percentages on a detail-rows slice based on their token totals,
/// then re-sort desc and truncate to top 3.
pub(crate) fn recompute_detail_pct(rows: &mut Vec<ProjectDetailRow>) {
    let total: i64 = rows.iter().map(|r| r.tokens).sum();
    for r in rows.iter_mut() {
        r.pct = pct(r.tokens, total);
    }
    rows.sort_by_key(|b| std::cmp::Reverse(b.tokens));
    rows.truncate(3);
}

/// Return a clone of `json` with all `entries` whose `client` equals
/// `client_to_remove` filtered out. Used to split a workspace report into
/// Claude vs non-Claude halves so each can be grouped by its own strategy.
pub fn filter_out_client(json: &Value, client_to_remove: &str) -> Value {
    let mut j = json.clone();
    if let Some(arr) = j.get_mut("entries").and_then(|v| v.as_array_mut()) {
        arr.retain(|e| e.get("client").and_then(|v| v.as_str()) != Some(client_to_remove));
    }
    j
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
