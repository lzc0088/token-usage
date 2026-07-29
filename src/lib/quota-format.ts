import { VENDOR_PANEL } from "./meta/panels";
import { api } from "./api";

/** Open the vendor panel URL in the system browser. */
export function openPanelUrl(vendor: string): void {
  const panel = VENDOR_PANEL[vendor];
  if (!panel) return;
  const url = typeof panel.url === "string" ? panel.url : Object.values(panel.url.map)[0] ?? "";
  if (url) api.openExternal(url).catch(() => {});
}

/**
 * Shared quota-formatting utilities.
 * Used by Limits, Overview, QuotaCard to keep behaviour in one place.
 */

/** Window labels → Chinese. */
export const WINDOW_LABELS: Record<string, string> = {
  "5h": "5小时",
  "周": "7天",
  "月": "每月",
  "MCP 月": "MCP 每月",
};

export function windowLabel(raw: string): string {
  return WINDOW_LABELS[raw] ?? raw;
}

/** RFC3339 → "刚刚更新" / "X秒前刷新" / "X分钟前刷新" / "X小时前刷新". */
export function formatRefreshed(refreshedAt: string | undefined, now: number): string {
  if (!refreshedAt) return "";
  const then = Date.parse(refreshedAt);
  if (!Number.isFinite(then)) return "";
  const secs = Math.floor((now - then) / 1000);
  if (secs < 0) return "刚刚刷新";
  if (secs < 60) return `${secs}秒前刷新`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}分钟前刷新`;
  const hrs = Math.floor(mins / 60);
  return `${hrs}小时前刷新`;
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

export function formatExpiry(expiresAt: string | undefined, now: number): string {
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
  let remain: string;
  if (days > 0) remain = `${days}天 ${hours}小时 ${mins}分钟`;
  else if (hours > 0) remain = `${hours}小时 ${mins}分钟`;
  else remain = `${mins}分钟`;
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
export function formatReset(resetsAt: string | undefined, now: number): string {
  if (!resetsAt) return "";
  const target = Date.parse(resetsAt);
  if (!Number.isFinite(target)) return "";
  const secs = Math.floor((target - now) / 1000);
  if (secs <= 0) return "即将重置";
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return `${days}天${hours}小时后重置`;
  if (hours > 0) return `${hours}小时${mins}分钟后重置`;
  return `${mins}分钟后重置`;
}

/** Balance → "¥1,234.56" / "$99.99". */
export function formatBalance(currency: string, amount: number): string {
  const sym = currency === "CNY" ? "¥" : currency === "USD" ? "$" : "";
  return `${sym}${amount.toFixed(2)}`;
}
