//! Tool / model share breakdown (T2.3).
//!
//! Aggregates `daily_usage` by the chosen dimension within a date range, then
//! computes token / cost percentages in Rust. The token total excludes
//! `reasoning_tokens` (design.md §5.3).

use rusqlite::Connection;

use super::{pct, pct_f, DateRange, Dimension, QueryError};

/// One row of the breakdown (a tool or a model).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BreakdownEntry {
    pub key: String,
    pub tokens: i64,
    pub token_pct: f64,
    pub cost_usd: f64,
    pub cost_pct: f64,
    pub messages: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Breakdown {
    pub dimension: Dimension,
    pub entries: Vec<BreakdownEntry>,
    pub grand_total_tokens: i64,
    pub grand_total_cost: f64,
}

/// Raw SQL aggregate before percentage computation.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub key: String,
    pub tokens: i64,
    pub cost: f64,
    pub messages: i64,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
}

/// Pure: turn raw aggregates into a breakdown with percentages. Tested directly.
pub fn finalize(raws: Vec<RawEntry>, dim: Dimension) -> Breakdown {
    let grand_total_tokens: i64 = raws.iter().map(|r| r.tokens).sum();
    let grand_total_cost: f64 = raws.iter().map(|r| r.cost).sum();
    let entries = raws
        .into_iter()
        .map(|r| BreakdownEntry {
            key: r.key,
            tokens: r.tokens,
            token_pct: pct(r.tokens, grand_total_tokens),
            cost_usd: r.cost,
            cost_pct: pct_f(r.cost, grand_total_cost),
            messages: r.messages,
            input: r.input,
            output: r.output,
            cache_read: r.cache_read,
            cache_write: r.cache_write,
        })
        .collect();
    Breakdown {
        dimension: dim,
        entries,
        grand_total_tokens,
        grand_total_cost,
    }
}

/// Query breakdown items that match a specific key in the *other* dimension.
/// e.g. `query_filtered(conn, range, "model", "claude")` returns models used
/// by the "claude" tool, and vice versa.
pub fn query_filtered(
    conn: &Connection,
    range: &DateRange,
    dim: Dimension,
    filter_key: &str,
) -> Result<Breakdown, QueryError> {
    let col = dim.column();
    let filter_col = match dim {
        Dimension::Tool => "model",
        Dimension::Model => "tool",
    };
    let (clause, mut params) = super::range_clause(range);
    let where_clause = format!("{clause} AND {filter_col} = ?");
    params.push(filter_key.to_string());
    let sql = format!(
        "SELECT {col} AS k,
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0) AS tokens,
                COALESCE(SUM(cost_usd), 0) AS cost,
                COALESCE(SUM(messages), 0) AS messages,
                COALESCE(SUM(input_tokens), 0) AS input,
                COALESCE(SUM(output_tokens), 0) AS output,
                COALESCE(SUM(cache_read_tokens), 0) AS cache_read,
                COALESCE(SUM(cache_write_tokens), 0) AS cache_write
         FROM daily_usage
         WHERE {where_clause}
         GROUP BY {col}
         ORDER BY tokens DESC
         LIMIT 3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let raws: Vec<RawEntry> = {
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
            Ok(RawEntry {
                key: r.get::<_, String>(0)?,
                tokens: r.get::<_, i64>(1)?,
                cost: r.get::<_, f64>(2)?,
                messages: r.get::<_, i64>(3)?,
                input: r.get::<_, i64>(4)?,
                output: r.get::<_, i64>(5)?,
                cache_read: r.get::<_, i64>(6)?,
                cache_write: r.get::<_, i64>(7)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    Ok(finalize(raws, dim))
}

/// Query the breakdown by `dim` within `range`.
pub fn query(
    conn: &Connection,
    range: &DateRange,
    dim: Dimension,
) -> Result<Breakdown, QueryError> {
    let col = dim.column();
    let (clause, params) = super::range_clause(range);
    let sql = format!(
        "SELECT {col} AS k,
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0) AS tokens,
                COALESCE(SUM(cost_usd), 0) AS cost,
                COALESCE(SUM(messages), 0) AS messages,
                COALESCE(SUM(input_tokens), 0) AS input,
                COALESCE(SUM(output_tokens), 0) AS output,
                COALESCE(SUM(cache_read_tokens), 0) AS cache_read,
                COALESCE(SUM(cache_write_tokens), 0) AS cache_write
         FROM daily_usage
         WHERE {clause}
         GROUP BY {col}
         ORDER BY tokens DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let raws: Vec<RawEntry> = {
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
            Ok(RawEntry {
                key: r.get::<_, String>(0)?,
                tokens: r.get::<_, i64>(1)?,
                cost: r.get::<_, f64>(2)?,
                messages: r.get::<_, i64>(3)?,
                input: r.get::<_, i64>(4)?,
                output: r.get::<_, i64>(5)?,
                cache_read: r.get::<_, i64>(6)?,
                cache_write: r.get::<_, i64>(7)?,
            })
        })?;
        rows.collect::<Result<_, _>>()?
    };
    Ok(finalize(raws, dim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{daily_usage::ingest_graph, schema};

    fn seeded_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        ingest_graph(
            &mut conn,
            &serde_json::json!({
                "contributions": [
                    { "date": "2026-07-17", "clients": [
                        { "client": "claude", "modelId": "glm-5.2", "providerId": "x",
                          "tokens": {"input": 1000, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                          "cost": 10.0, "messages": 4 },
                        { "client": "codex", "modelId": "gpt-5", "providerId": "x",
                          "tokens": {"input": 500, "output": 500, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                          "cost": 5.0, "messages": 2 },
                    ]},
                    { "date": "2026-07-18", "clients": [
                        { "client": "claude", "modelId": "glm-5.2", "providerId": "x",
                          "tokens": {"input": 0, "output": 0, "cacheRead": 500, "cacheWrite": 0, "reasoning": 0},
                          "cost": 2.0, "messages": 1 },
                    ]}
                ]
            }),
        )
        .unwrap();
        conn
    }

    #[test]
    fn finalize_computes_percentages() {
        let raws = vec![
            RawEntry {
                key: "claude".into(),
                tokens: 1500,
                cost: 12.0,
                messages: 5,
                input: 1000,
                output: 0,
                cache_read: 500,
                cache_write: 0,
            },
            RawEntry {
                key: "codex".into(),
                tokens: 500,
                cost: 3.0,
                messages: 2,
                input: 500,
                output: 500,
                cache_read: 0,
                cache_write: 0,
            },
        ];
        let b = finalize(raws, Dimension::Tool);
        assert_eq!(b.grand_total_tokens, 2000);
        assert!((b.grand_total_cost - 15.0).abs() < 1e-9);
        let claude = &b.entries[0];
        assert_eq!(claude.key, "claude");
        assert!((claude.token_pct - 75.0).abs() < 1e-9); // 1500/2000
        assert!((claude.cost_pct - 80.0).abs() < 1e-9); // 12/15
        assert_eq!(claude.input, 1000);
        assert_eq!(claude.cache_read, 500);
        assert_eq!(b.entries[1].output, 500);
    }

    #[test]
    fn finalize_zero_whole_is_zero_pct() {
        let b = finalize(vec![], Dimension::Tool);
        assert_eq!(b.grand_total_tokens, 0);
        assert!(b.entries.is_empty());
    }

    #[test]
    fn query_by_tool_aggregates_and_excludes_reasoning() {
        let conn = seeded_conn();
        // range covering both days → claude 1500 (1000 + 500 cacheRead), codex 1000.
        let range = DateRange {
            start: None,
            end: None,
        };
        let b = query(&conn, &range, Dimension::Tool).unwrap();
        assert_eq!(b.dimension, Dimension::Tool);
        let by_key: std::collections::HashMap<&str, &BreakdownEntry> =
            b.entries.iter().map(|e| (e.key.as_str(), e)).collect();
        assert_eq!(by_key["claude"].tokens, 1500);
        assert_eq!(by_key["codex"].tokens, 1000);
        assert_eq!(b.grand_total_tokens, 2500);
        // percentages
        assert!((by_key["claude"].token_pct - 60.0).abs() < 1e-9); // 1500/2500
        assert!((by_key["codex"].token_pct - 40.0).abs() < 1e-9);
        // cost
        assert!((by_key["claude"].cost_usd - 12.0).abs() < 1e-9); // 10 + 2
    }

    #[test]
    fn query_range_filters_dates() {
        let conn = seeded_conn();
        // only 2026-07-17 → claude 1000, codex 1000.
        let range = DateRange {
            start: Some("2026-07-17".into()),
            end: Some("2026-07-17".into()),
        };
        let b = query(&conn, &range, Dimension::Tool).unwrap();
        assert_eq!(b.grand_total_tokens, 2000);
        let claude = b.entries.iter().find(|e| e.key == "claude").unwrap();
        assert_eq!(claude.tokens, 1000); // not 1500
    }

    #[test]
    fn query_by_model_groups_model_not_tool() {
        let conn = seeded_conn();
        let range = DateRange::default();
        let b = query(&conn, &range, Dimension::Model).unwrap();
        assert_eq!(b.dimension, Dimension::Model);
        let keys: Vec<&str> = b.entries.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"glm-5.2"));
        assert!(keys.contains(&"gpt-5"));
        // glm-5.2 = 1500 (claude both days), gpt-5 = 1000 (codex)
        assert_eq!(b.grand_total_tokens, 2500);
    }

    #[test]
    fn query_orders_by_tokens_desc() {
        let conn = seeded_conn();
        let b = query(&conn, &DateRange::default(), Dimension::Tool).unwrap();
        assert_eq!(b.entries[0].key, "claude"); // 1500 > 1000
        assert!(b.entries[0].tokens >= b.entries[1].tokens);
    }
}
