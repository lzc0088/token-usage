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
  // compact (default): K/M
  if (n >= 1_000_000) return `${trim(n / 1_000_000)}M`;
  if (n >= 1_000) return `${trim(n / 1_000)}K`;
  return String(Math.round(n));
}

function trim(v: number): string {
  // 1.84 not 1.8400
  return Number(v.toFixed(2)).toString();
}

/** Format cost in the chosen currency. USD/CNY/双显. */
export function formatCost(usd: number, currency: Currency, cnyRate = 7.2): string {
  if (currency === "usd") return `$${usd.toFixed(2)}`;
  if (currency === "cny") return `¥${(usd * cnyRate).toFixed(2)}`;
  return `$${usd.toFixed(2)} · ¥${(usd * cnyRate).toFixed(2)}`;
}

export type TokenStyle = "compact" | "wan" | "plain";
