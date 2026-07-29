use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as epoch-milliseconds (i64). Returns 0 if the system clock
/// is before the Unix epoch (should never happen in practice).
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
