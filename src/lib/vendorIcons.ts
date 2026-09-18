// Pulse vendor identity — thin adapter over the app's shared brand icon set
// (src/lib/meta/tools.ts → src/lib/icons/**). Pulses renders the same real
// brand logos as the quota UI, plus short display names for the compact ring
// and card. Vendor IDs mirror Rust TRACKED_VENDORS.

import { vendorIcon as brandIcon } from "./meta/tools";

/** Short display names (VENDOR_LABELS elsewhere carries longer qualifiers). */
export const vendorNames: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  cursor: "Cursor",
  deepseek: "DeepSeek",
  glm: "GLM",
  grok: "Grok",
  kimi: "Kimi",
  minimax: "Minimax",
  volcengine: "火山引擎",
  bailian: "百炼",
  stepfun: "阶跃",
  iflytek: "讯飞星火",
  copilot: "Copilot",
  mimo: "MiMo",
  opencode: "OpenCode",
  zai_team: "GLM Team",
  qoder: "Qoder",
  ollama: "Ollama",
  workbuddy: "WorkBuddy",
  openrouter: "OpenRouter",
};

/** Generic orbit glyph for vendors without a brand SVG in the shared set. */
const FALLBACK_ICON =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="4" fill="currentColor" stroke="none"/><ellipse cx="12" cy="12" rx="10" ry="4.2" transform="rotate(-24 12 12)"/></svg>';

/** Brand SVG markup for a vendor (currentColor fill — size/color via the
 *  wrapper span). Falls back to a generic glyph. Render with {@html}. */
export function vendorIconMarkup(vendor: string): string {
  return brandIcon(vendor) || FALLBACK_ICON;
}

/** Short display name for a vendor; falls back to the raw id. */
export function vendorDisplayName(vendor: string): string {
  return vendorNames[vendor] ?? vendor;
}
