//! Hero summary VM (M3). Totals for a date range from `daily_usage`, plus a
//! pure mapping from `tokscale --today` JSON for the real-time event path.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use super::{range_clause, DateRange, QueryError};

/// Popover hero totals. `total_tokens` excludes reasoning (design.md §5.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub period: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

/// Aggregate `daily_usage` over `range` into a [`Summary`].
pub fn query(conn: &Connection, range: &DateRange) -> Result<Summary, QueryError> {
    let (clause, params) = range_clause(range);
    let sql = format!(
        "SELECT
            COALESCE(SUM(input_tokens),0),
            COALESCE(SUM(output_tokens),0),
            COALESCE(SUM(cache_read_tokens),0),
            COALESCE(SUM(cache_write_tokens),0),
            COALESCE(SUM(reasoning_tokens),0),
            COALESCE(SUM(cost_usd),0),
            COALESCE(SUM(messages),0)
         FROM daily_usage WHERE {clause}"
    );
    let (input, output, cache_read, cache_write, reasoning, cost, messages) =
        conn.query_row(&sql, rusqlite::params_from_iter(params), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })?;
    Ok(Summary {
        period: period_key(range),
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        total_tokens: input + output + cache_read + cache_write,
        cost_usd: cost,
        messages,
    })
}

/// Map `tokscale --today` JSON (`{ totalInput, totalOutput, totalCacheRead, … }`)
/// into a day-period Summary for the `today:updated` event. None if the shape
/// doesn't match (caller keeps the last value).
pub fn from_today_json(v: &Value) -> Option<Summary> {
    let input = v.get("totalInput")?.as_i64()?;
    let output = get_i64(v, "totalOutput");
    let cache_read = get_i64(v, "totalCacheRead");
    let cache_write = get_i64(v, "totalCacheWrite");
    let reasoning = get_i64(v, "totalReasoning");
    let cost = v.get("totalCost").and_then(|c| c.as_f64()).unwrap_or(0.0);
    let messages = get_i64(v, "totalMessages");
    Some(Summary {
        period: "day".into(),
        input,
        output,
        cache_read,
        cache_write,
        reasoning,
        total_tokens: input + output + cache_read + cache_write,
        cost_usd: cost,
        messages,
    })
}

fn get_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Recover the period key from a range (for the VM). Day/month inferred from the
/// range shape; anything else → total. Best-effort: the authoritative period is
/// what the caller asked for, but the VM carries it for the frontend label.
fn period_key(range: &DateRange) -> String {
    match (&range.start, &range.end) {
        (Some(s), Some(e)) if s == e => "day".into(),
        (Some(_), Some(_)) => "month".into(),
        _ => "total".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{range_for_period, Period};
    use crate::storage::{daily_usage::ingest_graph, schema};

    fn seeded() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        ingest_graph(
            &mut conn,
            &serde_json::json!({"contributions":[
                {"date":"2026-07-19","clients":[
                    {"client":"claude","modelId":"glm-5.2","providerId":"x",
                     "tokens":{"input":1000,"output":100,"cacheRead":500,"cacheWrite":0,"reasoning":0},
                     "cost":2.0,"messages":3}]}]}),
        )
        .unwrap();
        conn
    }

    #[test]
    fn query_totals_and_excludes_reasoning() {
        let conn = seeded();
        let r = range_for_period(Period::Day, "2026-07-19");
        let s = query(&conn, &r).unwrap();
        assert_eq!(s.period, "day");
        assert_eq!(s.total_tokens, 1600); // 1000+100+500+0
        assert!((s.cost_usd - 2.0).abs() < 1e-9);
        assert_eq!(s.messages, 3);
    }

    #[test]
    fn from_today_json_maps_totals() {
        let v = serde_json::json!({
            "totalInput": 204980, "totalOutput": 1417,
            "totalCacheRead": 205952, "totalCacheWrite": 0, "totalReasoning": 0,
            "totalCost": 0.346, "totalMessages": 2
        });
        let s = from_today_json(&v).unwrap();
        assert_eq!(s.period, "day");
        assert_eq!(s.total_tokens, 412349);
        assert!((s.cost_usd - 0.346).abs() < 1e-9);
        assert_eq!(s.messages, 2);
    }

    #[test]
    fn from_today_json_none_on_missing_shape() {
        assert!(from_today_json(&serde_json::json!({"foo": 1})).is_none());
    }

    #[test]
    fn period_key_inference() {
        assert_eq!(
            period_key(&range_for_period(Period::Day, "2026-07-19")),
            "day"
        );
        assert_eq!(
            period_key(&range_for_period(Period::Month, "2026-07-19")),
            "month"
        );
        assert_eq!(
            period_key(&range_for_period(Period::Total, "2026-07-19")),
            "total"
        );
    }
}
