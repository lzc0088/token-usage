/** Tool display name & icon mapping — shared by Overview, Tools, Models segments. */

const KNOWN: Record<string, { label: string; icon: string; color: string }> = {
  claude:    { label: "Claude Code",  icon: "C",  color: "var(--amber)" },
  codex:     { label: "Codex",        icon: "D",  color: "var(--violet)" },
  zcode:     { label: "ZCode",        icon: "Z",  color: "var(--cyan)" },
  opencode:  { label: "OpenCode",     icon: "O",  color: "var(--lime)" },
  cursor:    { label: "Cursor",       icon: "Cu", color: "var(--coral)" },
  cline:     { label: "Cline",        icon: "Cl", color: "var(--text-dim)" },
  grok:      { label: "Grok",         icon: "G",  color: "var(--coral)" },
  kimi:      { label: "Kimi",         icon: "K",  color: "var(--cyan)" },
  copilot:   { label: "Copilot",      icon: "Co", color: "var(--text-dim)" },
  zed:       { label: "Zed",          icon: "Ze", color: "var(--text-dim)" },
  kiro:      { label: "Kiro",         icon: "Ki", color: "var(--text-dim)" },
  qoder:     { label: "Qoder",        icon: "Q",  color: "var(--amber)" },
  trae:      { label: "Trae",         icon: "Tr", color: "var(--lime)" },
  workbuddy: { label: "WorkBuddy",    icon: "Wb", color: "#64b4ff" },
  codebuddy: { label: "CodeBuddy",    icon: "Cb", color: "var(--lime)" },
  aide:      { label: "Aide",         icon: "Ai", color: "var(--violet)" },
  crush:     { label: "Crush",        icon: "Cr", color: "var(--amber)" },
  pieces:    { label: "Pieces",       icon: "P",  color: "var(--cyan)" },
  tabnine:   { label: "Tabnine",      icon: "T",  color: "var(--text-dim)" },
  aicode:    { label: "AICode",       icon: "Ac", color: "var(--amber)" },
  smore:     { label: "Smore",        icon: "Sm", color: "var(--lime)" },
  auggie:    { label: "Auggie",       icon: "Ag", color: "#64b4ff" },
  // model-level keys (used by model breakdown)
  "glm-5.2":          { label: "glm-5.2",          icon: "G", color: "var(--amber)" },
  "step-3.7-flash":   { label: "step-3.7-flash",   icon: "S", color: "var(--lime)" },
  "deepseek-v4":      { label: "deepseek-v4",      icon: "D", color: "var(--cyan)" },
  "gpt-5":            { label: "gpt-5",            icon: "G", color: "var(--violet)" },
};

export interface ToolMeta {
  label: string;
  icon: string;
  color: string;
}

/** Look up display name & icon for a raw key (case-insensitive fallback). */
export function toolMeta(key: string): ToolMeta {
  const lower = key.toLowerCase().replace(/[^a-z0-9.-]/g, "");
  const found = KNOWN[lower];
  if (found) return found;
  // Fallback: first 2 chars uppercased
  return { label: key, icon: key.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase() || "?", color: "var(--text-faint)" };
}
