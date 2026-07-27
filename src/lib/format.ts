// Number / currency / time formatting (mirrors the wireframe's 万进制 + currency).
// Pure functions; unit-tested where non-trivial.

import type { Currency } from "./api";

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

/** Format cost in the chosen currency. USD/CNY/双显 (CNY first per user preference). */
export function formatCost(usd: number, currency: Currency, cnyRate = 7.2): string {
  // Guard non-finite (NaN/Infinity) the same way formatTokens does.
  if (!Number.isFinite(usd) || !Number.isFinite(cnyRate)) {
    usd = 0;
    cnyRate = Number.isFinite(cnyRate) ? cnyRate : 7.2;
  }
  if (currency === "usd") return `$ ${usd.toFixed(2)}`;
  if (currency === "cny") return `¥ ${(usd * cnyRate).toFixed(2)}`;
  // CNY first
  return `¥ ${(usd * cnyRate).toFixed(2)} / $ ${usd.toFixed(2)}`;
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
 *  Thresholds: ≥百亿 → 十亿 → 亿 → 千万 → 百万 → 十万 → 万 → 千.
 *  @param decimals - Number of decimal places (default: 2). */
export function splitTokensCN(n: number, decimals = 2): { value: string; unit: string } {
  if (!Number.isFinite(n)) return { value: "0", unit: "" };
  const abs = Math.abs(n);
  if (abs >= 10_000_000_000) return { value: trim(n / 10_000_000_000, decimals), unit: "百亿" };
  if (abs >=  1_000_000_000) return { value: trim(n /  1_000_000_000, decimals), unit: "十亿" };
  if (abs >=    100_000_000) return { value: trim(n /    100_000_000, decimals), unit: "亿" };
  if (abs >=     10_000_000) return { value: trim(n /     10_000_000, decimals), unit: "千万" };
  if (abs >=      1_000_000) return { value: trim(n /      1_000_000, decimals), unit: "百万" };
  if (abs >=        100_000) return { value: trim(n /        100_000, decimals), unit: "十万" };
  if (abs >=         10_000) return { value: trim(n /         10_000, decimals), unit: "万" };
  if (abs >=          1_000) return { value: trim(n /          1_000, decimals), unit: "千" };
  return { value: String(Math.round(n)), unit: "" };
}

export type TokenStyle = "compact" | "wan" | "plain";
