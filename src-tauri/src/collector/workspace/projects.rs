//! Project building and merging from session-level report data.

use std::collections::HashMap;

use serde_json::Value;

use super::filesystem::build_precise_session_map;
use super::report::{detail_rows, recompute_detail_pct, token_total};
use super::types::ProjectAgg;

/// Token floor for the projects-page visibility rule, acting as the
/// cost-independent "real usage" signal (see [`is_visible_project`]).
pub const MIN_VISIBLE_TOKENS: i64 = 10_000;

/// Visibility rule for the projects page: suppress noise (stray one-off
/// sessions) without ever hiding real usage just because pricing data is
/// missing. A hard `cost_usd >= 0.1` gate alone blanked the whole page on
/// days where every model was unpriced (2026-08-18: glm-5.3 wasn't in the
/// LiteLLM table yet, cost summed to 0 for 30M+ tokens), so tokens act as a
/// second signal: either the window has measurable cost, or enough tokens.
pub fn is_visible_project(p: &ProjectAgg) -> bool {
    (p.full_path.is_some() || p.latest_date.is_some())
        && p.messages >= 5
        && (p.cost_usd >= 0.1 || p.tokens >= MIN_VISIBLE_TOKENS)
}

/// Merge `incoming` into `projects` by `full_path`. When a project with the
/// same path already exists, its tokens/cost/messages are accumulated and the
/// model/tool detail rows are merged (percentages recomputed). Otherwise the
/// incoming project is appended. Projects without a path are always appended.
pub fn merge_project(projects: &mut Vec<ProjectAgg>, incoming: ProjectAgg) {
    let Some(incoming_path) = incoming.full_path.as_ref() else {
        projects.push(incoming);
        return;
    };
    let idx = projects
        .iter()
        .position(|p| p.full_path.as_ref() == Some(incoming_path));
    let Some(i) = idx else {
        projects.push(incoming);
        return;
    };
    let target = &mut projects[i];
    target.tokens += incoming.tokens;
    target.cost_usd += incoming.cost_usd;
    target.messages += incoming.messages;
    // Keep the newest latest_date across the merged sources.
    if incoming.latest_date.as_ref() > target.latest_date.as_ref() {
        target.latest_date = incoming.latest_date;
    }
    for t in incoming.tools {
        if let Some(existing) = target.tools.iter_mut().find(|x| x.key == t.key) {
            existing.tokens += t.tokens;
        } else {
            target.tools.push(t);
        }
    }
    recompute_detail_pct(&mut target.tools);
    for m in incoming.models {
        if let Some(existing) = target.models.iter_mut().find(|x| x.key == m.key) {
            existing.tokens += m.tokens;
        } else {
            target.models.push(m);
        }
    }
    recompute_detail_pct(&mut target.models);
}

/// Build a `Vec<ProjectAgg>` from tokscale's `--group-by session,model` JSON.
///
/// For each Claude Code session it reads the JSONL `cwd` to determine the
/// project — this preserves subdirectory-level projects (e.g. `uniapp-field`
/// appears as a separate project rather than being merged into `bee_miniprogram`).
/// Non-Claude sessions fall back to their workspace key for grouping.
///
/// Token/cost data comes from tokscale (precise); project names/paths come
/// from the session JSONL files (authoritative).
pub fn build_projects_from_sessions(
    json: &Value,
    claude_projects_dir: Option<&std::path::Path>,
) -> Vec<ProjectAgg> {
    let session_map = claude_projects_dir
        .map(build_precise_session_map)
        .unwrap_or_default();
    build_projects_from_sessions_with_map(json, &session_map)
}

/// Same as [`build_projects_from_sessions`] but accepts a pre-built session
/// map, so the caller can share one filesystem scan across multiple uses
/// (avoids re-walking `~/.claude/projects/` for each call).
pub fn build_projects_from_sessions_with_map(
    json: &Value,
    session_map: &HashMap<String, (String, String, Option<String>)>,
) -> Vec<ProjectAgg> {
    let Some(entries) = json.get("entries").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    // ── Pass 1: aggregate entries by (client, sessionId) ─────────────────
    struct SessionAcc {
        tokens: i64,
        cost: f64,
        messages: i64,
        models: HashMap<String, i64>,
    }
    let mut sess_acc: HashMap<(String, String), SessionAcc> = HashMap::new();

    for e in entries {
        let client = e.get("client").and_then(|v| v.as_str()).unwrap_or("?");
        let session_id = e.get("sessionId").and_then(|v| v.as_str()).unwrap_or("?");
        let model = e.get("model").and_then(|v| v.as_str()).unwrap_or("?");
        let tokens = token_total(e);
        let cost = e.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let messages = e.get("messageCount").and_then(|v| v.as_i64()).unwrap_or(0);

        let acc = sess_acc
            .entry((client.to_string(), session_id.to_string()))
            .or_insert(SessionAcc {
                tokens: 0,
                cost: 0.0,
                messages: 0,
                models: HashMap::new(),
            });
        acc.tokens += tokens;
        acc.cost += cost;
        acc.messages += messages;
        *acc.models.entry(model.to_string()).or_insert(0) += tokens;
    }

    // ── Pass 2: group sessions into projects ────────────────────────────
    struct ProjectAcc {
        name: String,
        full_path: Option<String>,
        latest_date: Option<String>,
        tokens: i64,
        cost: f64,
        messages: i64,
        models: HashMap<String, i64>,
        tools: HashMap<String, i64>,
    }
    let mut proj_acc: HashMap<String, ProjectAcc> = HashMap::new();

    for ((client, session_id), sess) in sess_acc {
        if client != "claude" {
            continue;
        }
        let (proj_key, proj_name, proj_path, proj_date) = match session_map.get(&session_id) {
            Some((name, path, date)) => {
                (path.clone(), name.clone(), Some(path.clone()), date.clone())
            }
            None => continue,
        };

        let acc = proj_acc.entry(proj_key).or_insert_with(|| ProjectAcc {
            name: proj_name.clone(),
            full_path: proj_path.clone(),
            latest_date: None,
            tokens: 0,
            cost: 0.0,
            messages: 0,
            models: HashMap::new(),
            tools: HashMap::new(),
        });
        if proj_date.as_ref() > acc.latest_date.as_ref() {
            acc.latest_date = proj_date;
        }
        acc.tokens += sess.tokens;
        acc.cost += sess.cost;
        acc.messages += sess.messages;
        for (m, t) in sess.models {
            *acc.models.entry(m).or_insert(0) += t;
        }
        *acc.tools.entry(client).or_insert(0) += sess.tokens;
    }

    // ── Pass 3: finalize into ProjectAgg ────────────────────────────────
    let mut out: Vec<ProjectAgg> = proj_acc
        .into_values()
        .map(|a| {
            let total = a.tokens;
            ProjectAgg {
                name: a.name,
                full_path: a.full_path,
                latest_date: a.latest_date,
                tokens: a.tokens,
                cost_usd: a.cost,
                messages: a.messages,
                models: detail_rows(a.models, total),
                tools: detail_rows(a.tools, total),
            }
        })
        .collect();
    out.sort_by_key(|y| std::cmp::Reverse(y.tokens));
    out
}
