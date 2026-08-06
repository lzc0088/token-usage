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
    /// Token change vs previous period (percent), e.g. +12.3 or -5.0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_pct: Option<f64>,
    /// Label for the comparison, e.g. "较昨日" / "较上月".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_label: Option<String>,
    /// Real-time throughput numerator/denominator, summed from tokscale's
    /// per-entry `performance` block (only present on the live today path).
    /// The frontend derives tokens/s or tokens/min from these. None when
    /// sourced from the DB (month/total) or when no entry reported a duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timed_duration_ms: Option<i64>,
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
        delta_pct: None,
        delta_label: None,
        // DB path has no throughput data — it's a live-only metric.
        timed_output_tokens: None,
        timed_tokens: None,
        timed_duration_ms: None,
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
    let (timed_output, timed_tokens, timed_duration) = sum_throughput(v);
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
        delta_pct: None,
        delta_label: None,
        timed_output_tokens: Some(timed_output),
        timed_tokens: Some(timed_tokens),
        timed_duration_ms: Some(timed_duration),
    })
}

fn get_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Read the first present i64 among `keys` on `obj` (camelCase + snake_case
/// aliases, since tokscale's casing has varied across versions).
fn first_i64(obj: &Value, keys: &[&str]) -> Option<i64> {
    for k in keys {
        if let Some(n) = obj.get(k).and_then(|x| x.as_i64()) {
            return Some(n);
        }
    }
    None
}

/// Sum tokscale's per-entry `performance` throughput counters across all
/// entries. Returns `(timed_output_tokens, timed_tokens, timed_duration_ms)`.
///
/// `timed_output_tokens` is not read from the performance block directly; per
/// token-monitor's gating rule, an entry contributes its own `output` tokens to
/// the throughput numerator exactly when it contributed to the denominator
/// (i.e. when its `timedDurationMs` > 0). This keeps the counter a plain sum
/// that merges/deltas like every other token field. `timed_tokens` is read
/// directly from the performance block (it already excludes cache reads).
fn sum_throughput(v: &Value) -> (i64, i64, i64) {
    let mut timed_output = 0i64;
    let mut timed_tokens = 0i64;
    let mut timed_duration = 0i64;
    let Some(entries) = v.get("entries").and_then(|e| e.as_array()) else {
        return (0, 0, 0);
    };
    for entry in entries {
        let output = first_i64(entry, &["output", "outputTokens", "output_tokens"])
            .unwrap_or(0)
            .max(0);
        let perf = entry.get("performance");
        let dur = perf
            .and_then(|p| {
                first_i64(
                    p,
                    &[
                        "totalDurationMs",
                        "total_duration_ms",
                        "timedDurationMs",
                        "timed_duration_ms",
                    ],
                )
            })
            .unwrap_or(0)
            .max(0);
        let ttoks = perf
            .and_then(|p| first_i64(p, &["timedTokens", "timed_tokens"]))
            .unwrap_or(0)
            .max(0);
        timed_duration += dur;
        timed_tokens += ttoks;
        // Gate output by duration so the numerator and denominator stay paired.
        timed_output += if dur > 0 { output } else { 0 };
    }
    (timed_output, timed_tokens, timed_duration)
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
        // No entries → throughput zeros (still Some, so the frontend can show 0).
        assert_eq!(s.timed_output_tokens, Some(0));
        assert_eq!(s.timed_tokens, Some(0));
        assert_eq!(s.timed_duration_ms, Some(0));
    }

    #[test]
    fn sum_throughput_gates_output_by_duration() {
        // Entry A: 200 output, 5000ms duration → contributes output + duration.
        // Entry B: 300 output, 0ms duration → contributes nothing (gated out).
        // Entry C: 100 output, 1500ms duration, timedTokens 400.
        let v = serde_json::json!({
            "totalInput": 0, "totalOutput": 600,
            "entries": [
                {"output": 200, "performance": {"timedDurationMs": 5000, "timedTokens": 250}},
                {"output": 300, "performance": {"timedDurationMs": 0}},
                {"output": 100, "performance": {"timedDurationMs": 1500, "timedTokens": 400}}
            ]
        });
        let (timed_output, timed_tokens, timed_duration) = sum_throughput(&v);
        assert_eq!(timed_duration, 6500); // 5000 + 0 + 1500
        assert_eq!(timed_output, 300); // 200 + (gated 0) + 100
        assert_eq!(timed_tokens, 650); // 250 + 0 + 400
    }

    #[test]
    fn sum_throughput_accepts_snake_case_aliases() {
        let v = serde_json::json!({
            "totalInput": 0, "totalOutput": 0,
            "entries": [
                {"output_tokens": 80, "performance": {"timed_duration_ms": 2000, "timed_tokens": 80}}
            ]
        });
        let (timed_output, timed_tokens, timed_duration) = sum_throughput(&v);
        assert_eq!(timed_duration, 2000);
        assert_eq!(timed_output, 80);
        assert_eq!(timed_tokens, 80);
    }

    #[test]
    fn from_today_json_carries_throughput_to_summary() {
        // output/s = 200 * 1000 / 5000 = 40; the frontend computes the rate.
        let v = serde_json::json!({
            "totalInput": 1000, "totalOutput": 200,
            "entries": [{"output": 200, "performance": {"timedDurationMs": 5000, "timedTokens": 200}}]
        });
        let s = from_today_json(&v).unwrap();
        assert_eq!(s.timed_output_tokens, Some(200));
        assert_eq!(s.timed_tokens, Some(200));
        assert_eq!(s.timed_duration_ms, Some(5000));
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
