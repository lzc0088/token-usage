// Shared vendor identity data for Pulse components (ring centers, detail
// card headers). Single source of truth — vendor IDs mirror the Rust
// TRACKED_VENDORS list in src-tauri/src/quota/mod.rs.

export interface VendorIconData {
  /** SVG path in a 24x24 viewBox. */
  d: string;
  /** true → stroke outline icon; false (default) → filled shape. */
  stroke?: boolean;
}

/** Vendor display names (fallback: the raw vendor id). */
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
  copilot: "GitHub Copilot",
  mimo: "MiMo",
  opencode: "OpenCode",
  zai_team: "Z.ai",
  qoder: "Qoder",
  ollama: "Ollama",
  workbuddy: "WorkBuddy",
  openrouter: "OpenRouter",
};

/** Minimal geometric marks (24x24 viewBox), recognizable at 14–26px. */
export const vendorIcons: Record<string, VendorIconData> = {
  claude:      { d: "M7 17V7h6v10H7zm2-5.5h2v3H9z" },
  codex:       { d: "M7.5 7.5l4.5 4.5m0 4.5l-4.5 4.5m9-9l-4.5 4.5m0-4.5l4.5 4.5", stroke: true },
  cursor:      { d: "M8 7l5 5-5 5m10 0l-5-5 5-5", stroke: true },
  deepseek:    { d: "M7 17V7h5.5A3.5 3.5 0 0116 10.5v6.5A3.5 3.5 0 0112.5 17H7z" },
  glm:         { d: "M12 7.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9z", stroke: true },
  grok:        { d: "M12 7v10M7 12h10" },
  kimi:        { d: "M8.5 17V7l6.5 5-6.5 5z" },
  minimax:     { d: "M12 7.5l4.5 9h-9z" },
  volcengine:  { d: "M12 7.5l4 4.5-4 4.5-4-4.5z" },
  bailian:     { d: "M7.5 7.5h9v9h-9z" },
  stepfun:     { d: "M7.5 16.5q4.5-9 9 0", stroke: true },
  iflytek:     { d: "M7.5 10.5Q12 7.5 16.5 10.5M7.5 13.5Q12 10.5 16.5 13.5M7.5 16.5Q12 13.5 16.5 16.5", stroke: true },
  copilot:     { d: "M12 7l1.5 4.5L18 12l-3.5 2.5L15 18l-3-2.5L9 18l.5-3.5L6 12l4.5-1L12 7z" },
  mimo:        { d: "M12 7.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5z" },
  opencode:    { d: "M9 8.5l-3.5 3.5 3.5 3.5m7-3.5l3.5 3.5-3.5 3.5", stroke: true },
  zai_team:    { d: "M7.5 16.5h9M7.5 7.5h9" },
  qoder:       { d: "M8.5 8.5l7 7m0-7l-7 7", stroke: true },
  ollama:      { d: "M12 7.5a4.5 4.5 0 114.5 4.5 4.5 4.5 0 01-4.5-4.5z", stroke: true },
  workbuddy:   { d: "M8 17l7-9.5", stroke: true },
  openrouter:  { d: "M12 7.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9z" },
};

/** Icon data for a vendor; `{ d: "" }` when unknown (renders nothing). */
export function vendorIcon(vendor: string): VendorIconData {
  return vendorIcons[vendor] ?? { d: "" };
}

/** Display name for a vendor; falls back to the raw id. */
export function vendorDisplayName(vendor: string): string {
  return vendorNames[vendor] ?? vendor;
}
