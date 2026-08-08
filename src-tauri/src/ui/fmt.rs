//! Shared formatting helpers for compact token counts and localized cost.
//! Used by both the tray title (`ui::tray`) and the floating panel
//! (`ui::floating`) so the two readouts always agree.

use crate::config::Currency;

/// Compact a raw token count into a short string: `2.8M`, `1.2K`, `3`.
pub fn compact_tokens(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if abs >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// Format a USD cost according to the user's currency config.
/// CNY / 双显 use the supplied USD→CNY rate, falling back to 7.2 when unset.
pub fn format_cost(usd: f64, currency: Currency, cny_rate: f64) -> String {
    let rate = if cny_rate > 0.0 { cny_rate } else { 7.2 };
    match currency {
        Currency::Usd => format!("${:.1}", usd),
        Currency::Cny => format!("¥{:.1}", usd * rate),
        Currency::Both => format!("¥{:.1}/${:.1}", usd * rate, usd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_tokens_thresholds() {
        assert_eq!(compact_tokens(0), "0");
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(2_840_000), "2.8M");
        assert_eq!(compact_tokens(1_500_000_000), "1.5B");
        assert_eq!(compact_tokens(-3_200_000), "-3.2M");
    }

    #[test]
    fn cost_usd() {
        assert_eq!(format_cost(4.21, Currency::Usd, 7.2), "$4.2");
    }

    #[test]
    fn cost_cny_uses_rate() {
        // 4.21 * 7.0 = 29.47 → ¥29.5
        assert_eq!(format_cost(4.21, Currency::Cny, 7.0), "¥29.5");
    }

    #[test]
    fn cost_falls_back_to_72_when_rate_zero() {
        // 4.21 * 7.2 = 30.312 → ¥30.3
        assert_eq!(format_cost(4.21, Currency::Cny, 0.0), "¥30.3");
    }

    #[test]
    fn cost_both() {
        assert_eq!(format_cost(4.21, Currency::Both, 7.0), "¥29.5/$4.2");
    }
}
