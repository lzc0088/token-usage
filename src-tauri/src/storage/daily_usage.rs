//! Ingest `tokscale graph` output into `daily_usage` (T2.2).
//!
//! `tokscale graph` returns absolute cumulative totals per (date, client, model),
//! **not** deltas. Re-ingesting the same graph output yields identical rows, so
//! upsert = REPLACE on the primary key, not accumulate. Reasoning tokens are
//! stored in their own column and are excluded from the token total downstream
//! (see design.md §5.3).

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::StorageError;

/// One row destined for `daily_usage`. Pure data — the ingest fn maps `graph`
/// JSON to a Vec<DailyRow> then writes.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyRow {
    pub date: String,  // YYYY-MM-DD
    pub tool: String,  // client
    pub model: String, // modelId
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

// ── `tokscale graph` JSON shape (v4.5.3, verified) ──────────────────────────

#[derive(Debug, Deserialize)]
pub struct GraphReport {
    pub contributions: Vec<Contribution>,
}

#[derive(Debug, Deserialize)]
pub struct Contribution {
    pub date: String,
    #[serde(default)]
    pub clients: Vec<ClientEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientEntry {
    pub client: String,
    #[serde(rename = "modelId")]
    pub model_id: String,
    #[serde(default)]
    pub tokens: TokenBreakdown,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub messages: i64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBreakdown {
    #[serde(default)]
    pub input: i64,
    #[serde(default)]
    pub output: i64,
    #[serde(default)]
    pub cache_read: i64,
    #[serde(default)]
    pub cache_write: i64,
    #[serde(default)]
    pub reasoning: i64,
}

/// Pure: flatten graph JSON into daily rows. One row per (date, client, model).
pub fn rows_from_graph(g: &GraphReport) -> Vec<DailyRow> {
    let mut out = Vec::new();
    for day in &g.contributions {
        for c in &day.clients {
            out.push(DailyRow {
                date: day.date.clone(),
                tool: c.client.clone(),
                model: c.model_id.clone(),
                input: c.tokens.input,
                output: c.tokens.output,
                cache_read: c.tokens.cache_read,
                cache_write: c.tokens.cache_write,
                reasoning: c.tokens.reasoning,
                cost_usd: c.cost,
                messages: c.messages,
            });
        }
    }
    out
}

/// Upsert (replace-on-conflict) a batch of rows into `daily_usage`. Wrapped in
/// a single transaction; either the whole batch lands or none.
pub fn upsert_rows(conn: &mut Connection, rows: &[DailyRow]) -> Result<usize, StorageError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    let mut n = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO daily_usage
               (date, tool, model, input_tokens, output_tokens,
                cache_read_tokens, cache_write_tokens, reasoning_tokens,
                cost_usd, messages)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(date, tool, model) DO UPDATE SET
               input_tokens       = excluded.input_tokens,
               output_tokens      = excluded.output_tokens,
               cache_read_tokens  = excluded.cache_read_tokens,
               cache_write_tokens = excluded.cache_write_tokens,
               reasoning_tokens   = excluded.reasoning_tokens,
               cost_usd           = excluded.cost_usd,
               messages           = excluded.messages",
        )?;
        for r in rows {
            stmt.execute(params![
                r.date,
                r.tool,
                r.model,
                r.input,
                r.output,
                r.cache_read,
                r.cache_write,
                r.reasoning,
                r.cost_usd,
                r.messages,
            ])?;
            n += 1;
        }
    }
    tx.commit()?;
    Ok(n)
}

/// Ingest a raw `tokscale graph` JSON value. Returns rows written.
pub fn ingest_graph(conn: &mut Connection, raw: &serde_json::Value) -> Result<usize, StorageError> {
    let g: GraphReport = serde_json::from_value(raw.clone())?;
    let rows = rows_from_graph(&g);
    upsert_rows(conn, &rows)
}

// ── `tokscale --today` entries ingest ────────────────────────────────────────
//
// The `--today --group-by client,model` report has entries with flat fields:
//   { client, model, input, output, cacheRead, cacheWrite, reasoning,
//     messageCount, cost, performance }
// We parse these into DailyRow and upsert so `daily_usage` stays as fresh as
// the live hero (instead of waiting 15 min for the next `graph` run).

/// Read the first present i64 among `keys` on `obj` (camelCase + snake_case
/// aliases, since tokscale's casing has varied across versions).
fn first_i64(obj: &serde_json::Value, keys: &[&str]) -> i64 {
    for k in keys {
        if let Some(n) = obj.get(k).and_then(|x| x.as_i64()) {
            return n;
        }
    }
    0
}

/// Read the first present f64 among `keys` on `obj`.
fn first_f64(obj: &serde_json::Value, keys: &[&str]) -> f64 {
    for k in keys {
        if let Some(n) = obj.get(k).and_then(|x| x.as_f64()) {
            return n;
        }
    }
    0.0
}

/// Read the first present string among `keys` on `obj`.
fn first_string<'a>(obj: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(s) = obj.get(k).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Parse entries from a `tokscale --today --group-by client,model` JSON value
/// and upsert today's rows into `daily_usage`. Returns rows written.
///
/// This keeps breakdown/trends queries in sync with the live hero data between
/// the 15-min `tokscale graph` runs. Safe to call repeatedly — upsert replaces
/// on `(date, tool, model)` conflict.
pub fn ingest_today_entries(
    conn: &mut Connection,
    v: &serde_json::Value,
    today: &str,
) -> Result<usize, StorageError> {
    let Some(entries) = v.get("entries").and_then(|e| e.as_array()) else {
        return Ok(0);
    };
    let mut rows = Vec::new();
    for entry in entries {
        let (Some(client), Some(model)) = (
            first_string(entry, &["client", "clientName"]),
            first_string(entry, &["model", "modelId", "modelName"]),
        ) else {
            continue;
        };
        // Skip synthetic/aggregate entries (e.g. "All Models").
        if client.is_empty() || model.is_empty() {
            continue;
        }
        rows.push(DailyRow {
            date: today.to_string(),
            tool: client.to_string(),
            model: model.to_string(),
            input: first_i64(entry, &["input", "inputTokens", "input_tokens"]),
            output: first_i64(entry, &["output", "outputTokens", "output_tokens"]),
            cache_read: first_i64(
                entry,
                &["cacheRead", "cacheReadTokens", "cache_read_tokens"],
            ),
            cache_write: first_i64(
                entry,
                &["cacheWrite", "cacheWriteTokens", "cache_write_tokens"],
            ),
            reasoning: first_i64(entry, &["reasoning", "reasoningTokens", "reasoning_tokens"]),
            cost_usd: first_f64(entry, &["cost", "costUsd", "cost_usd"]),
            messages: first_i64(entry, &["messageCount", "messages", "message_count"]),
        });
    }
    upsert_rows(conn, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn sample_graph() -> serde_json::Value {
        // Trimmed subset of the real `tokscale graph` JSON.
        serde_json::json!({
            "contributions": [
                {
                    "date": "2026-07-15",
                    "clients": [
                        {
                            "client": "claude",
                            "modelId": "glm-5.2",
                            "providerId": "unknown",
                            "tokens": {
                                "input": 1000, "output": 200,
                                "cacheRead": 500, "cacheWrite": 0, "reasoning": 0
                            },
                            "cost": 1.23,
                            "messages": 5
                        },
                        {
                            "client": "codex",
                            "modelId": "gpt-5-plus",
                            "providerId": "openai",
                            "tokens": {
                                "input": 200, "output": 80,
                                "cacheRead": 0, "cacheWrite": 0, "reasoning": 999
                            },
                            "cost": 0.42,
                            "messages": 3
                        }
                    ]
                },
                {
                    "date": "2026-07-16",
                    "clients": [{
                        "client": "claude", "modelId": "glm-5.2", "providerId": "unknown",
                        "tokens": {"input": 100, "output": 10, "cacheRead": 5, "cacheWrite": 0, "reasoning": 0},
                        "cost": 0.05, "messages": 1
                    }]
                }
            ]
        })
    }

    #[test]
    fn rows_from_graph_flattens_day_by_client_by_model() {
        let g: GraphReport = serde_json::from_value(sample_graph()).unwrap();
        let rows = rows_from_graph(&g);
        assert_eq!(rows.len(), 3);
        let claude = rows
            .iter()
            .find(|r| r.tool == "claude" && r.date == "2026-07-15")
            .unwrap();
        assert_eq!(claude.input, 1000);
        assert_eq!(claude.cache_read, 500);
        assert!((claude.cost_usd - 1.23).abs() < 1e-9);
        assert_eq!(claude.messages, 5);
    }

    #[test]
    fn upsert_writes_and_replaces_on_conflict() {
        let mut conn = fresh_conn();
        let n = ingest_graph(&mut conn, &sample_graph()).unwrap();
        assert_eq!(n, 3);

        // Absolute totals in row form:
        let total_claude_715: i64 = conn
            .query_row(
                "SELECT input_tokens+output_tokens+cache_read_tokens+cache_write_tokens
                 FROM daily_usage WHERE date='2026-07-15' AND tool='claude'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total_claude_715, 1_700); // 1000+200+500+0, reasoning excluded

        // Re-ingesting the *same* graph must overwrite (not accumulate):
        // simulate a later graph run reporting the same absolute totals.
        ingest_graph(&mut conn, &sample_graph()).unwrap();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_usage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 3, "upsert must not create duplicate rows");
        let same_total: i64 = conn
            .query_row(
                "SELECT input_tokens+output_tokens+cache_read_tokens+cache_write_tokens
                 FROM daily_usage WHERE date='2026-07-15' AND tool='claude'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(same_total, 1_700);
    }

    #[test]
    fn upsert_replaces_with_new_values() {
        let mut conn = fresh_conn();
        ingest_graph(&mut conn, &sample_graph()).unwrap();

        // Later graph shows growth on 2026-07-15 claude (tokscale reports absolute).
        let updated = serde_json::json!({
            "contributions": [{
                "date": "2026-07-15",
                "clients": [{
                    "client": "claude", "modelId": "glm-5.2", "providerId": "unknown",
                    "tokens": {"input": 1500, "output": 300, "cacheRead": 700, "cacheWrite": 0, "reasoning": 0},
                    "cost": 1.99, "messages": 8
                }]
            }]
        });
        ingest_graph(&mut conn, &updated).unwrap();

        let (inp, cost, msgs): (i64, f64, i64) = conn
            .query_row(
                "SELECT input_tokens, cost_usd, messages
                 FROM daily_usage WHERE date='2026-07-15' AND tool='claude'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(inp, 1500);
        assert!((cost - 1.99).abs() < 1e-9);
        assert_eq!(msgs, 8);
    }

    #[test]
    fn reasoning_column_stored_but_excluded_from_total() {
        let mut conn = fresh_conn();
        ingest_graph(&mut conn, &sample_graph()).unwrap();
        // codex 2026-07-15 has reasoning=999; the token total excludes it.
        let (reason, total): (i64, i64) = conn
            .query_row(
                "SELECT reasoning_tokens,
                        input_tokens+output_tokens+cache_read_tokens+cache_write_tokens
                 FROM daily_usage WHERE date='2026-07-15' AND tool='codex'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(reason, 999);
        assert_eq!(total, 280); // 200+80+0+0
    }

    #[test]
    fn empty_batch_is_noop() {
        let mut conn = fresh_conn();
        assert_eq!(upsert_rows(&mut conn, &[]).unwrap(), 0);
    }

    #[test]
    fn ingest_today_entries_parses_flat_fields() {
        let mut conn = fresh_conn();
        let today_json = serde_json::json!({
            "groupBy": "client,model",
            "totalInput": 1500, "totalOutput": 300,
            "entries": [
                {
                    "client": "claude", "model": "glm-5.2", "provider": "x",
                    "input": 1000, "output": 200, "cacheRead": 500, "cacheWrite": 0,
                    "reasoning": 0, "messageCount": 5, "cost": 1.23,
                    "performance": {"timedDurationMs": 3000}
                },
                {
                    "client": "codex", "model": "gpt-5-plus", "provider": "openai",
                    "input": 500, "output": 100, "cacheRead": 0, "cacheWrite": 0,
                    "reasoning": 0, "messageCount": 3, "cost": 0.42,
                    "performance": {}
                }
            ]
        });
        let n = ingest_today_entries(&mut conn, &today_json, "2026-08-20").unwrap();
        assert_eq!(n, 2);

        let total_claude: i64 = conn
            .query_row(
                "SELECT input_tokens+output_tokens+cache_read_tokens+cache_write_tokens
                 FROM daily_usage WHERE date='2026-08-20' AND tool='claude'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total_claude, 1700); // 1000+200+500+0

        let (cost, msgs): (f64, i64) = conn
            .query_row(
                "SELECT cost_usd, messages FROM daily_usage
                 WHERE date='2026-08-20' AND tool='codex'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!((cost - 0.42).abs() < 1e-9);
        assert_eq!(msgs, 3);
    }

    #[test]
    fn ingest_today_entries_upserts_on_repeat() {
        let mut conn = fresh_conn();
        let v1 = serde_json::json!({
            "entries": [{
                "client": "claude", "model": "glm-5.2",
                "input": 100, "output": 50, "cacheRead": 0, "cacheWrite": 0,
                "reasoning": 0, "messageCount": 2, "cost": 0.1
            }]
        });
        ingest_today_entries(&mut conn, &v1, "2026-08-20").unwrap();

        // Later tick with updated totals.
        let v2 = serde_json::json!({
            "entries": [{
                "client": "claude", "model": "glm-5.2",
                "input": 200, "output": 100, "cacheRead": 0, "cacheWrite": 0,
                "reasoning": 0, "messageCount": 4, "cost": 0.3
            }]
        });
        ingest_today_entries(&mut conn, &v2, "2026-08-20").unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_usage", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "upsert must not create duplicates");
        let inp: i64 = conn
            .query_row(
                "SELECT input_tokens FROM daily_usage WHERE date='2026-08-20'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(inp, 200, "should have the latest value");
    }

    #[test]
    fn ingest_today_entries_empty_entries_is_noop() {
        let mut conn = fresh_conn();
        let v = serde_json::json!({"entries": []});
        assert_eq!(
            ingest_today_entries(&mut conn, &v, "2026-08-20").unwrap(),
            0
        );
    }

    #[test]
    fn ingest_today_entries_no_entries_key_is_noop() {
        let mut conn = fresh_conn();
        let v = serde_json::json!({"totalInput": 100});
        assert_eq!(
            ingest_today_entries(&mut conn, &v, "2026-08-20").unwrap(),
            0
        );
    }

    #[test]
    fn ingest_today_entries_skips_entries_missing_client_or_model() {
        let mut conn = fresh_conn();
        let v = serde_json::json!({
            "entries": [
                {"client": "claude", "input": 100},  // missing model → skip
                {"model": "glm-5.2", "input": 200},  // missing client → skip
                {"client": "codex", "model": "gpt", "input": 300, "output": 0,
                 "cacheRead": 0, "cacheWrite": 0, "reasoning": 0,
                 "messageCount": 1, "cost": 0.0}     // complete → keep
            ]
        });
        let n = ingest_today_entries(&mut conn, &v, "2026-08-20").unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn parses_graph_ignoring_unknown_fields() {
        // 'meta', 'summary', 'years', 'timeMetrics', 'intensity', 'totals', 'tokenBreakdown'
        // all appear in real output and must not break parsing.
        let raw = serde_json::json!({
            "meta": {"generatedAt": "..."},
            "summary": {"totalTokens": 12345},
            "years": [{"year": "2026"}],
            "timeMetrics": {},
            "contributions": [{
                "date": "2026-07-17",
                "intensity": 4,
                "totals": {"tokens": 100, "cost": 0.1, "messages": 1},
                "tokenBreakdown": {"input": 60, "output": 40, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                "clients": [{
                    "client": "opencode", "modelId": "gpt-x", "providerId": "openai",
                    "tokens": {"input": 60, "output": 40, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                    "cost": 0.1, "messages": 1
                }]
            }]
        });
        let g: GraphReport = serde_json::from_value(raw).unwrap();
        assert_eq!(g.contributions.len(), 1);
        assert_eq!(g.contributions[0].clients[0].client, "opencode");
    }
}
