//! Per-day trend aggregation (T2.3). Feeds the 趋势 segment's simple daily bars
//! (V1); stacked-by-tool / heatmap / K-line are V2.

use rusqlite::Connection;

use super::{DateRange, QueryError};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TrendPoint {
    pub date: String,
    pub tokens: i64,
    pub cost_usd: f64,
    pub messages: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Trends {
    pub points: Vec<TrendPoint>,
}

pub fn query(conn: &Connection, range: &DateRange) -> Result<Trends, QueryError> {
    // Always daily granularity: the heatmap needs YYYY-MM-DD dates. The
    // frontend aggregates to monthly buckets for the total-period chart.
    query_inner(conn, false, range)
}

fn query_inner(conn: &Connection, monthly: bool, range: &DateRange) -> Result<Trends, QueryError> {
    let (clause, params) = super::range_clause(range);
    let group_expr = if monthly {
        "strftime('%Y-%m', date)"
    } else {
        "date"
    };
    let sql = format!(
        "SELECT {group_expr} AS period,
                COALESCE(SUM(input_tokens + output_tokens + cache_read_tokens + cache_write_tokens), 0),
                COALESCE(SUM(cost_usd), 0),
                COALESCE(SUM(messages), 0)
         FROM daily_usage
         WHERE {clause}
         GROUP BY period
         ORDER BY period ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let points = stmt
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok(TrendPoint {
                date: r.get::<_, String>(0)?,
                tokens: r.get::<_, i64>(1)?,
                cost_usd: r.get::<_, f64>(2)?,
                messages: r.get::<_, i64>(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Trends { points })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{daily_usage::ingest_graph, schema};

    fn seeded() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        ingest_graph(
            &mut conn,
            &serde_json::json!({
                "contributions": [
                    { "date": "2026-07-16", "clients": [
                        { "client": "claude", "modelId": "glm-5.2", "providerId": "x",
                          "tokens": {"input": 100, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                          "cost": 1.0, "messages": 1 }] },
                    { "date": "2026-07-17", "clients": [
                        { "client": "claude", "modelId": "glm-5.2", "providerId": "x",
                          "tokens": {"input": 400, "output": 100, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                          "cost": 5.0, "messages": 3 },
                        { "client": "codex", "modelId": "gpt-5", "providerId": "x",
                          "tokens": {"input": 200, "output": 0, "cacheRead": 0, "cacheWrite": 0, "reasoning": 0},
                          "cost": 2.0, "messages": 1 }] }
                ]
            }),
        )
        .unwrap();
        conn
    }

    #[test]
    fn aggregates_per_day_ordered_asc() {
        let conn = seeded();
        let t = query(&conn, &DateRange::default()).unwrap();
        assert_eq!(t.points.len(), 2);
        assert_eq!(t.points[0].date, "2026-07-16");
        assert_eq!(t.points[0].tokens, 100);
        assert_eq!(t.points[1].date, "2026-07-17");
        assert_eq!(t.points[1].tokens, 700); // 500 + 200
        assert!((t.points[1].cost_usd - 7.0).abs() < 1e-9); // 5 + 2
        assert_eq!(t.points[1].messages, 4);
    }

    #[test]
    fn trends_respects_range() {
        let conn = seeded();
        let r = DateRange {
            start: Some("2026-07-17".into()),
            end: Some("2026-07-17".into()),
        };
        let t = query(&conn, &r).unwrap();
        assert_eq!(t.points.len(), 1);
        assert_eq!(t.points[0].tokens, 700);
    }

    #[test]
    fn empty_range_yields_no_points() {
        let conn = seeded();
        let r = DateRange {
            start: Some("1999-01-01".into()),
            end: Some("1999-01-02".into()),
        };
        let t = query(&conn, &r).unwrap();
        assert!(t.points.is_empty());
    }
}
