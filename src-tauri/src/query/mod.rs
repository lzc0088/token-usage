//! Query layer — SQL → view models (VM). Percentages/sorting computed in Rust
//! (design.md §6); the frontend only renders. See T2.3.

pub mod breakdown;
pub mod sessions;
pub mod summary;
pub mod trends;

#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Global popover period (DAY / MONTH / TOTAL). Maps to a date range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Day,
    Month,
    Total,
}

/// Breakdown dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Dimension {
    Tool,
    Model,
}

impl Dimension {
    pub fn column(self) -> &'static str {
        match self {
            Dimension::Tool => "tool",
            Dimension::Model => "model",
        }
    }
}

/// Inclusive date range over `daily_usage.date`; `None` bounds are unbounded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DateRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

/// Map a global [`Period`] to a concrete date range, given today's date
/// (`YYYY-MM-DD`). Pure — no clock access — so it's unit-testable; callers pass
/// the current date explicitly.
pub fn range_for_period(period: Period, today: &str) -> DateRange {
    // today must look like YYYY-MM-DD.
    debug_assert!(today.len() >= 10, "today must be YYYY-MM-DD: {today}");
    match period {
        Period::Total => DateRange::default(),
        Period::Day => DateRange {
            start: Some(today.to_string()),
            end: Some(today.to_string()),
        },
        Period::Month => {
            let prefix = &today[..7]; // YYYY-MM
            DateRange {
                start: Some(format!("{prefix}-01")),
                end: Some(today.to_string()),
            }
        }
    }
}

/// Build a SQL `WHERE` clause + bind params for a range over `daily_usage.date`.
/// Returns `(clause, params)`.
pub(crate) fn range_clause(range: &DateRange) -> (String, Vec<String>) {
    match (&range.start, &range.end) {
        (Some(s), Some(e)) => (
            "date >= ? AND date <= ?".to_string(),
            vec![s.clone(), e.clone()],
        ),
        (Some(s), None) => ("date >= ?".to_string(), vec![s.clone()]),
        (None, Some(e)) => ("date <= ?".to_string(), vec![e.clone()]),
        (None, None) => ("1=1".to_string(), Vec::new()),
    }
}

/// Return the previous period's date range and a Chinese label (e.g. "较昨日").
/// Total period has no comparison (returns `None`).
pub fn prev_range_for_period(period: Period, today: &str) -> Option<(DateRange, &'static str)> {
    use chrono::{Datelike, NaiveDate};
    let d = NaiveDate::parse_from_str(today, "%Y-%m-%d").ok()?;
    match period {
        Period::Total => None,
        Period::Day => {
            let y = d.pred_opt()?;
            let s = y.to_string();
            Some((
                DateRange {
                    start: Some(s.clone()),
                    end: Some(s),
                },
                "较昨日",
            ))
        }
        Period::Month => {
            let prev_month = if d.month() == 1 { 12 } else { d.month() - 1 };
            let prev_year = if d.month() == 1 {
                d.year() - 1
            } else {
                d.year()
            };
            let start = chrono::NaiveDate::from_ymd_opt(prev_year, prev_month, 1)?;
            let end_day = d.day().min(days_in_month(prev_year, prev_month));
            let end = chrono::NaiveDate::from_ymd_opt(prev_year, prev_month, end_day)?;
            Some((
                DateRange {
                    start: Some(start.to_string()),
                    end: Some(end.to_string()),
                },
                "较上月",
            ))
        }
    }
}

/// Days in a month (1–31). Uses chrono to handle leap years correctly.
fn days_in_month(year: i32, month: u32) -> u32 {
    use chrono::Datelike;
    if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .map(|d| d.day())
    .unwrap_or(30)
}

/// `part / whole * 100`, 0 when whole is 0.
pub fn pct(part: i64, whole: i64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

/// Float variant.
pub fn pct_f(part: f64, whole: f64) -> f64 {
    if whole.abs() < f64::EPSILON {
        0.0
    } else {
        part * 100.0 / whole
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_total_is_unbounded() {
        assert_eq!(
            range_for_period(Period::Total, "2026-07-18"),
            DateRange::default()
        );
    }

    #[test]
    fn range_day_is_single_day() {
        let r = range_for_period(Period::Day, "2026-07-18");
        assert_eq!(r.start.as_deref(), Some("2026-07-18"));
        assert_eq!(r.end.as_deref(), Some("2026-07-18"));
    }

    #[test]
    fn range_month_spans_first_to_today() {
        let r = range_for_period(Period::Month, "2026-07-18");
        assert_eq!(r.start.as_deref(), Some("2026-07-01"));
        assert_eq!(r.end.as_deref(), Some("2026-07-18"));
    }

    #[test]
    fn range_clause_shapes() {
        assert_eq!(
            range_clause(&DateRange {
                start: Some("a".into()),
                end: Some("b".into())
            }),
            (
                "date >= ? AND date <= ?".to_string(),
                vec!["a".into(), "b".into()]
            )
        );
        assert_eq!(
            range_clause(&DateRange::default()),
            ("1=1".to_string(), Vec::new())
        );
    }

    #[test]
    fn pct_handles_zero_whole() {
        assert_eq!(pct(5, 0), 0.0);
        assert_eq!(pct(25, 100), 25.0);
        assert!((pct(1, 3) - 33.333_333_333_333_336).abs() < 1e-9);
    }
}
