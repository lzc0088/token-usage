// Number / currency / time formatting (mirrors the wireframe's 万进制 + currency).
// Pure functions; unit-tested where non-trivial.

import type { Currency } from "./api";

// ── Locale-aware compact formatting ────────────────────────────────────────

export type Locale = "zh" | "en" | "ja" | "ko";

/**
 * Format a number with locale-aware compact units.
 * - Western (en): 1.2K, 3.4M, 5.6B
 * - Chinese (zh): 1.2万, 3.4亿
 * - Japanese (ja): 1.2万, 3.4億
 * - Korean (ko): 1.2만, 3.4억
 */
export function formatCompact(value: number, locale: Locale = "en"): string {
  if (!Number.isFinite(value)) return "0";
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";

  if (locale === "zh") {
    if (abs >= 1_0000_0000) return `${sign}${trim(value / 1_0000_0000)}亿`;
    if (abs >= 1_0000) return `${sign}${trim(value / 1_0000)}万`;
    return `${sign}${Math.round(abs).toLocaleString("en-US")}`;
  }

  if (locale === "ja") {
    if (abs >= 1_0000_0000) return `${sign}${trim(value / 1_0000_0000)}億`;
    if (abs >= 1_0000) return `${sign}${trim(value / 1_0000)}万`;
    return `${sign}${Math.round(abs).toLocaleString("en-US")}`;
  }

  if (locale === "ko") {
    if (abs >= 1_0000_0000) return `${sign}${trim(value / 1_0000_0000)}억`;
    if (abs >= 1_0000) return `${sign}${trim(value / 1_0000)}만`;
    return `${sign}${Math.round(abs).toLocaleString("en-US")}`;
  }

  // Western (default)
  if (abs >= 1_000_000_000) return `${sign}${trim(value / 1_000_000_000)}B`;
  if (abs >= 1_000_000) return `${sign}${trim(value / 1_000_000)}M`;
  if (abs >= 1_000) return `${sign}${trim(value / 1_000)}K`;
  return `${sign}${Math.round(abs)}`;
}

/**
 * Split a number into value and unit for locale-aware display.
 * Returns { value: "1.2", unit: "万" } for Chinese or { value: "1.2", unit: "M" } for English.
 */
export function splitCompact(value: number, locale: Locale = "en", decimals = 2): { value: string; unit: string } {
  if (!Number.isFinite(value)) return { value: "0", unit: "" };
  const abs = Math.abs(value);

  if (locale === "zh") {
    if (abs >= 1_0000_0000) return { value: trim(value / 1_0000_0000, decimals), unit: "亿" };
    if (abs >= 1_0000) return { value: trim(value / 1_0000, decimals), unit: "万" };
    return { value: String(Math.round(abs)), unit: "" };
  }

  if (locale === "ja") {
    if (abs >= 1_0000_0000) return { value: trim(value / 1_0000_0000, decimals), unit: "億" };
    if (abs >= 1_0000) return { value: trim(value / 1_0000, decimals), unit: "万" };
    return { value: String(Math.round(abs)), unit: "" };
  }

  if (locale === "ko") {
    if (abs >= 1_0000_0000) return { value: trim(value / 1_0000_0000, decimals), unit: "억" };
    if (abs >= 1_0000) return { value: trim(value / 1_0000, decimals), unit: "만" };
    return { value: String(Math.round(abs)), unit: "" };
  }

  // Western
  if (abs >= 1_000_000_000) return { value: trim(value / 1_000_000_000, decimals), unit: "B" };
  if (abs >= 1_000_000) return { value: trim(value / 1_000_000, decimals), unit: "M" };
  if (abs >= 1_000) return { value: trim(value / 1_000, decimals), unit: "K" };
  return { value: String(Math.round(abs)), unit: "" };
}

/** Compact token count: 1.84M / 184 万 / 1,840,000 depending on style. */
export function formatTokens(n: number, style: TokenStyle = "compact"): string {
  if (!Number.isFinite(n)) return "0";
  if (style === "plain") return n.toLocaleString("en-US");
  if (style === "wan") {
    // 万进制 (Chinese): 1.2 万 / 350 万
    if (n >= 100_000_000) return `${(n / 100_000_000).toFixed(2)}亿`;
    if (n >= 10_000) return `${(n / 10_000).toFixed(n >= 100_000 ? 0 : 1)}万`;
    return n.toLocaleString("en-US");
  }
  // compact (default): B/M/K
  if (n >= 1_000_000_000) return `${trim(n / 1_000_000_000)}B`;
  if (n >= 1_000_000) return `${trim(n / 1_000_000)}M`;
  if (n >= 1_000) return `${trim(n / 1_000)}K`;
  return String(Math.round(n));
}

function trim(v: number, decimals = 2): string {
  // 1.84 not 1.8400
  return Number(v.toFixed(decimals)).toString();
}

/** Format cost in the chosen currency. USD/CNY/双显 (CNY first per user preference).
 *  No spaces between symbol and value (unit sits flush against the amount). */
export function formatCost(usd: number, currency: Currency, cnyRate = 7.2): string {
  // Guard non-finite (NaN/Infinity) the same way formatTokens does.
  if (!Number.isFinite(usd) || !Number.isFinite(cnyRate)) {
    usd = 0;
    cnyRate = Number.isFinite(cnyRate) ? cnyRate : 7.2;
  }
  if (currency === "usd") return `$${usd.toFixed(2)}`;
  if (currency === "cny") return `¥${(usd * cnyRate).toFixed(2)}`;
  // CNY first; " / " separator at digit size (matches splitCost's sep)
  return `¥${(usd * cnyRate).toFixed(2)} / $${usd.toFixed(2)}`;
}

/** Cost split into (unit, value) pairs so UIs can render the currency symbol
 *  smaller and to the LEFT of the amount: usd → [$, 1.23]; cny → [¥, 8.87];
 *  both → [¥, 8.87] + [" / ", $, 1.23] — the separator renders at DIGIT size
 *  (part.sep), the symbol stays small. */
export interface CostPart {
  /** Separator before this part's unit, rendered at digit size (" / " in both mode). */
  sep?: string;
  unit: string;
  value: string;
}

export function splitCost(usd: number, currency: Currency, cnyRate = 7.2): CostPart[] {
  if (!Number.isFinite(usd) || !Number.isFinite(cnyRate)) {
    usd = 0;
    cnyRate = Number.isFinite(cnyRate) ? cnyRate : 7.2;
  }
  if (currency === "usd") return [{ unit: "$", value: usd.toFixed(2) }];
  const cny = (usd * cnyRate).toFixed(2);
  if (currency === "cny") return [{ unit: "¥", value: cny }];
  return [{ unit: "¥", value: cny }, { sep: " / ", unit: "$", value: usd.toFixed(2) }];
}

/** Split a token count into numeric value and compact unit（B / M / K / ""）.
 *  Designed so the unit can be rendered in a smaller font at baseline-shift.
 *  @param decimals - Number of decimal places (default: 2). */
export function splitTokens(n: number, decimals = 2): { value: string; unit: string } {
  if (!Number.isFinite(n)) return { value: "0", unit: "" };
  if (n >= 1_000_000_000) return { value: trim(n / 1_000_000_000, decimals), unit: "B" };
  if (n >= 1_000_000) return { value: trim(n / 1_000_000, decimals), unit: "M" };
  if (n >= 1_000) return { value: trim(n / 1_000, decimals), unit: "K" };
  return { value: String(Math.round(n)), unit: "" };
}

/** Chinese-unit variant for Hero top area only.
 *  Thresholds: ≥亿 → 万 → 千; larger magnitudes scale the value (e.g.
 *  10_350_000_000 → 103.5亿) instead of compound units like 百亿/千万.
 *  @param decimals - Number of decimal places (default: 2). */
export function splitTokensCN(n: number, decimals = 2): { value: string; unit: string } {
  if (!Number.isFinite(n)) return { value: "0", unit: "" };
  const abs = Math.abs(n);
  if (abs >=    100_000_000) return { value: trim(n /    100_000_000, decimals), unit: "亿" };
  if (abs >=         10_000) return { value: trim(n /         10_000, decimals), unit: "万" };
  if (abs >=          1_000) return { value: trim(n /          1_000, decimals), unit: "千" };
  return { value: String(Math.round(n)), unit: "" };
}

export type TokenStyle = "compact" | "wan" | "plain";

/** Compute a token-rate string from the live throughput counters.
 *
 *  - "speed" → output tokens / second of model-busy time = `timedOutputTokens * 1000 / timedDurationMs`.
 *    Uses output (not total) because cache reads (>90% of total) were never generated.
 *  - "burn"  → total tokens / minute = `timedTokens * 60000 / timedDurationMs`.
 *
 *  Returns "" when there is no duration (no session was model-busy in this
 *  window) — the numerator and denominator must stay paired, so a zero
 *  denominator means the rate is undefined rather than zero. */
export function formatTokenRate(
  mode: "speed" | "burn",
  timedOutputTokens: number | undefined,
  timedTokens: number | undefined,
  timedDurationMs: number | undefined,
): string {
  const dur = timedDurationMs ?? 0;
  if (dur <= 0) return "";
  if (mode === "speed") {
    const perSec = ((timedOutputTokens ?? 0) * 1000) / dur;
    return `${formatTokens(perSec)} tok/s`;
  }
  const perMin = ((timedTokens ?? 0) * 60000) / dur;
  return `${formatTokens(perMin)} tok/min`;
}
