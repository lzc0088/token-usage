import { VENDOR_PANEL } from "./meta/panels";
import { api } from "./api";

/** Open the vendor panel URL in the system browser.
 *  For vendors with dynamic URLs (e.g. Qoder's site-based split), pass the
 *  site/region identifier as the second argument to pick the right URL.
 */
export function openPanelUrl(vendor: string, site?: string | null): void {
  const panel = VENDOR_PANEL[vendor];
  if (!panel) return;

  let url: string;
  if (typeof panel.url === "string") {
    url = panel.url;
  } else if (site && panel.url.map[site]) {
    // Dynamic URL: use site-specific entry.
    url = panel.url.map[site];
  } else {
    // Fallback: first map entry.
    url = Object.values(panel.url.map)[0] ?? "";
  }

  if (url) api.openExternal(url).catch(() => {});
}

/**
 * Shared quota-formatting utilities.
 * Used by Limits, Overview, QuotaCard to keep behaviour in one place.
 */

const WINDOW_ZH: Record<string, string> = { "5h":"5小时", "周":"7天", "月":"每月", "MCP 月":"MCP 每月", "资源包":"资源包", "订阅":"订阅" };
const WINDOW_EN: Record<string, string> = { "5h":"5h", "周":"7d", "月":"Monthly", "MCP 月":"MCP Monthly", "资源包":"Credits", "订阅":"Subscription" };
export function windowLabel(raw: string, lang = "zh"): string {
  return (lang === "en" ? WINDOW_EN : WINDOW_ZH)[raw] ?? raw;
}

/** RFC3339 → "X秒前刷新" / "Xm ago" etc. */
export function formatRefreshed(refreshedAt: string | undefined, now: number, lang = "zh"): string {
  if (!refreshedAt) return "";
  const then = Date.parse(refreshedAt);
  if (!Number.isFinite(then)) return "";
  const secs = Math.floor((now - then) / 1000);
  const en = lang === "en";
  if (secs < 0) return en ? "Just now" : "刚刚刷新";
  if (secs < 60) return en ? `${secs}s ago` : `${secs}秒前刷新`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return en ? `${mins}m ago` : `${mins}分钟前刷新`;
  const hrs = Math.floor(mins / 60);
  return en ? `${hrs}h ago` : `${hrs}小时前刷新`;
}

/** ISO timestamp → "YYYY-MM-DD" or "". */
export function formatShortExpiry(iso: string): string {
  const target = Date.parse(iso);
  if (!Number.isFinite(target)) return "";
  const d = new Date(target);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${yyyy}-${mm}-${dd}`;
}

/** Quota plan expiry → "YYYY-MM-DD到期 · 剩余Xd Yh Zm" or "". */
export function nearestExpiry(expiresAt: string | undefined, now: number): number | undefined {
  if (!expiresAt) return undefined;
  const t = Date.parse(expiresAt);
  return (Number.isFinite(t) && t > now) ? t : undefined;
}

export function formatExpiry(expiresAt: string | undefined, now: number, lang = "zh"): string {
  const nearest = nearestExpiry(expiresAt, now);
  if (nearest === undefined) return "";
  const d = new Date(nearest);
  const yyyy = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const secs = Math.floor((nearest - now) / 1000);
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  const en = lang === "en";
  let remain: string;
  if (days > 0) remain = en ? `${days}d ${hours}h ${mins}m` : `${days}天 ${hours}小时 ${mins}分钟`;
  else if (hours > 0) remain = en ? `${hours}h ${mins}m` : `${hours}小时 ${mins}分钟`;
  else remain = en ? `${mins}m` : `${mins}分钟`;
  if (en) return `Expires ${yyyy}-${mm}-${dd} · ${remain} left`;
  return `${yyyy}-${mm}-${dd}到期 · 剩余${remain}`;
}

/** Expiry urgency CSS class: critical/soon/warn/ok/expired. */
export function expiryUrgency(expiresAt: string | undefined, now: number): string {
  const nearest = nearestExpiry(expiresAt, now);
  if (nearest === undefined) return "exp-expired";
  const days = (nearest - now) / 86400000;
  if (days <= 3) return "exp-critical";
  if (days <= 7) return "exp-soon";
  if (days <= 30) return "exp-warn";
  return "exp-ok";
}

/** Number → "1,500" or "1,234.5". */
export function fmtCredits(n: number | undefined | null): string {
  if (n == null) return "—";
  const isInt = n % 1 === 0;
  const s = isInt ? String(n) : n.toFixed(1);
  const parts = s.split(".");
  parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return parts.join(".");
}

/** ISO reset timestamp → "即将重置" / "X小时Y分钟后重置". */
export function formatReset(resetsAt: string | undefined, now: number, lang = "zh"): string {
  if (!resetsAt) return "";
  const target = Date.parse(resetsAt);
  if (!Number.isFinite(target)) return "";
  const secs = Math.floor((target - now) / 1000);
  const en = lang === "en";
  if (secs <= 0) return en ? "resetting soon" : "即将重置";
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return en ? `resets in ${days}d ${hours}h` : `${days}天${hours}小时后重置`;
  if (hours > 0) return en ? `resets in ${hours}h ${mins}m` : `${hours}小时${mins}分钟后重置`;
  return en ? `resets in ${mins}m` : `${mins}分钟后重置`;
}

/** Map known backend cookie-error messages to display language. */
export function translateCookieError(msg: string, lang = "zh"): string {
  const ZH: Record<string,string> = {
    "Cookie 已过期，请重新获取": "Cookie 已过期，请重新获取",
    "Cookie 已过期，套餐到期信息暂未显示": "Cookie 已过期，套餐到期信息暂未显示",
  };
  const EN: Record<string,string> = {
    "Cookie 已过期，请重新获取": "Cookie expired. Please obtain a new one.",
    "Cookie 已过期，套餐到期信息暂未显示": "Cookie expired. Plan expiry unavailable.",
  };
  return (lang === "en" ? EN : ZH)[msg] ?? msg;
}

/** Balance split into {unit, value} so the UI renders the currency symbol
 *  smaller and to the LEFT of the amount (popover unit rules). */
export function splitBalance(currency: string, amount: number): { unit: string; value: string } {
  const sym = currency === "CNY" ? "¥" : currency === "USD" ? "$" : "";
  return { unit: sym, value: amount.toFixed(2) };
}
