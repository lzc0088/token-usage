/** Tool display name & icon mapping — shared by Overview, Tools, Models segments.
 *
 * Each entry's `icon` is an SVG string loaded via Vite `?raw`. Components
 * render it with `{@html meta.icon}` inside the colored badge span.
 *
 * Icons sourced from token-monitor/assets/icons/ where available; fallbacks
 * are simple inline SVGs for tools not in that set.
 */

// ── SVG imports (Vite ?raw → string) ──────────────────────────────────────

import claudeIcon from "../icons/tools/claude.svg?raw";
import codexIcon from "../icons/tools/codex.svg?raw";
import zcodeIcon from "../icons/tools/zcode.svg?raw";
import opencodeIcon from "../icons/tools/opencode.svg?raw";
import cursorIcon from "../icons/tools/cursor.svg?raw";
import clineIcon from "../icons/tools/cline.svg?raw";
import grokIcon from "../icons/tools/grok.svg?raw";
import kimiIcon from "../icons/tools/kimi.svg?raw";
import copilotIcon from "../icons/tools/copilot.svg?raw";
import zedIcon from "../icons/tools/zed.svg?raw";
import kiroIcon from "../icons/tools/kiro.svg?raw";
import qoderIcon from "../icons/tools/qoder.svg?raw";
import traeIcon from "../icons/tools/trae.svg?raw";
import workbuddyIcon from "../icons/tools/workbuddy.svg?raw";
import codebuddyIcon from "../icons/tools/codebuddy.svg?raw";
import aideIcon from "../icons/tools/aide.svg?raw";
import crushIcon from "../icons/tools/crush.svg?raw";
import piecesIcon from "../icons/tools/pieces.svg?raw";
import tabnineIcon from "../icons/tools/tabnine.svg?raw";
import aicodeIcon from "../icons/tools/aicode.svg?raw";
import smoreIcon from "../icons/tools/smore.svg?raw";
import auggieIcon from "../icons/tools/auggie.svg?raw";
import antigravityIcon from "../icons/tools/antigravity.svg?raw";
import geminiIcon from "../icons/tools/gemini.svg?raw";
import kilocodeIcon from "../icons/tools/kilocode.svg?raw";
import openclawIcon from "../icons/tools/openclaw.svg?raw";
import hermesIcon from "../icons/tools/hermes.svg?raw";
import qwenIcon from "../icons/tools/qwen.svg?raw";
import piIcon from "../icons/tools/pi.svg?raw";
import promaIcon from "../icons/tools/proma.svg?raw";
import warpIcon from "../icons/tools/warp.svg?raw";
import muxIcon from "../icons/tools/mux.svg?raw";
import jetbrainsIcon from "../icons/tools/jetbrains.svg?raw";

// Vendor icons for model breakdown
import deepseekVendorIcon from "../icons/vendors/deepseek.svg?raw";
import stepfunVendorIcon from "../icons/vendors/stepfun.svg?raw";
import minimaxVendorIcon from "../icons/vendors/minimax.svg?raw";
import volcengineVendorIcon from "../icons/vendors/volcengine.svg?raw";
import mimoVendorIcon from "../icons/vendors/mimo.svg?raw";
import iflytekVendorIcon from "../icons/vendors/iflytek.svg?raw";
import doubaoVendorIcon from "../icons/vendors/doubao.svg?raw";
import hunyuanVendorIcon from "../icons/vendors/hunyuan.svg?raw";
import ollamaVendorIcon from "../icons/vendors/ollama.svg?raw";

import { modelVendor, VENDOR_NAME_TO_ID } from "./models";

const ICONS: Record<string, string> = {
  // ── Tools ──
  claude: claudeIcon,
  codex: codexIcon,
  zcode: zcodeIcon,
  opencode: opencodeIcon,
  cursor: cursorIcon,
  cline: clineIcon,
  grok: grokIcon,
  kimi: kimiIcon,
  copilot: copilotIcon,
  zed: zedIcon,
  kiro: kiroIcon,
  qoder: qoderIcon,
  trae: traeIcon,
  workbuddy: workbuddyIcon,
  codebuddy: codebuddyIcon,
  aide: aideIcon,
  crush: crushIcon,
  pieces: piecesIcon,
  tabnine: tabnineIcon,
  aicode: aicodeIcon,
  smore: smoreIcon,
  auggie: auggieIcon,
  antigravity: antigravityIcon,
  gemini: geminiIcon,
  kilocode: kilocodeIcon,
  openclaw: openclawIcon,
  hermes: hermesIcon,
  qwen: qwenIcon,
  pi: piIcon,
  proma: promaIcon,
  warp: warpIcon,
  mux: muxIcon,
  jetbrains: jetbrainsIcon,
  // micode (MiMo Code CLI) → shares the Xiaomi/MiMo vendor brand icon
  // (mimo itself is already mapped in the vendor-icons block above).
  micode: mimoVendorIcon,

  // ── Vendor icons (for model keys not in KNOWN) ──
  deepseek: deepseekVendorIcon,
  // GLM → same Z.ai icon as zcode (智谱 / Zhipu AI)
  glm: zcodeIcon,
  stepfun: stepfunVendorIcon,
  minimax: minimaxVendorIcon,
  volcengine: volcengineVendorIcon,
  mimo: mimoVendorIcon,
  iflytek: iflytekVendorIcon,
  doubao: doubaoVendorIcon,
  hunyuan: hunyuanVendorIcon,
  ollama: ollamaVendorIcon,

  // GLM Team / zai_team → same Z.ai / Zhipu brand as GLM
  zai_team: zcodeIcon,

  // ── Model keys → vendor icon mapping ──
  "deepseek-v4": deepseekVendorIcon,
  "glm-5.2": zcodeIcon,
  "step-3.7-flash": stepfunVendorIcon,
  "gpt-5": deepseekVendorIcon,
};

const KNOWN: Record<string, { label: string; icon: string; color: string }> = {
  claude:    { label: "Claude Code",  icon: ICONS.claude,    color: "var(--amber)" },
  codex:     { label: "Codex",        icon: ICONS.codex,     color: "var(--violet)" },
  zcode:     { label: "ZCode",        icon: ICONS.zcode,     color: "var(--cyan)" },
  opencode:  { label: "OpenCode ( OpenCode AI )",     icon: ICONS.opencode,  color: "var(--lime)" },
  cursor:    { label: "Cursor ( Anysphere )",       icon: ICONS.cursor,    color: "var(--coral)" },
  cline:     { label: "Cline",        icon: ICONS.cline,     color: "var(--text-dim)" },
  grok:      { label: "Grok",         icon: ICONS.grok,      color: "var(--coral)" },
  kimi:      { label: "Kimi",         icon: ICONS.kimi,      color: "var(--cyan)" },
  copilot:   { label: "Copilot",      icon: ICONS.copilot,   color: "var(--text-dim)" },
  zed:       { label: "Zed",          icon: ICONS.zed,       color: "var(--text-dim)" },
  kiro:      { label: "Kiro",         icon: ICONS.kiro,      color: "var(--text-dim)" },
  qoder:     { label: "Qoder ( 阿里 )",        icon: ICONS.qoder,     color: "var(--amber)" },
  trae:      { label: "Trae",         icon: ICONS.trae,      color: "var(--lime)" },
  workbuddy: { label: "WorkBuddy",    icon: ICONS.workbuddy, color: "#64b4ff" },
  codebuddy: { label: "CodeBuddy",    icon: ICONS.codebuddy, color: "var(--lime)" },
  aide:      { label: "Aide",         icon: ICONS.aide,      color: "var(--violet)" },
  crush:     { label: "Crush",        icon: ICONS.crush,     color: "var(--amber)" },
  pieces:    { label: "Pieces",       icon: ICONS.pieces,    color: "var(--cyan)" },
  tabnine:   { label: "Tabnine",      icon: ICONS.tabnine,   color: "var(--text-dim)" },
  aicode:    { label: "AICode",       icon: ICONS.aicode,    color: "var(--amber)" },
  smore:     { label: "Smore",        icon: ICONS.smore,     color: "var(--lime)" },
  auggie:    { label: "Auggie",       icon: ICONS.auggie,    color: "#64b4ff" },
  antigravity: { label: "Antigravity", icon: ICONS.antigravity, color: "#64b4ff" },
  gemini:      { label: "Gemini CLI",  icon: ICONS.gemini,      color: "#64b4ff" },
  kilocode:    { label: "Kilo Code",   icon: ICONS.kilocode,    color: "var(--amber)" },
  openclaw:    { label: "OpenClaw",    icon: ICONS.openclaw,    color: "var(--lime)" },
  hermes:      { label: "Hermes",      icon: ICONS.hermes,      color: "var(--violet)" },
  qwen:        { label: "Qwen Code",   icon: ICONS.qwen,        color: "var(--violet)" },
  pi:          { label: "Pi",          icon: ICONS.pi,          color: "var(--text-dim)" },
  proma:       { label: "Proma",       icon: ICONS.proma,       color: "var(--amber)" },
  warp:        { label: "Warp",        icon: ICONS.warp,        color: "#01A4FF" },
  mux:         { label: "Mux",         icon: ICONS.mux,         color: "var(--text-dim)" },
  micode:      { label: "MiMo Code",   icon: ICONS.micode,      color: "#FF6900" },
  mimo:        { label: "MiMo",        icon: ICONS.mimo,        color: "#FF6900" },
  junie:       { label: "Junie",       icon: ICONS.jetbrains,   color: "#FE2857" },
  ollama:      { label: "Ollama ( Ollama Cloud )",      icon: ICONS.ollama,      color: "var(--text-dim)" },
  zai_team:    { label: "GLM Team",   icon: ICONS.zai_team,   color: "var(--amber)" },
  // model-level keys (used by model breakdown — vendor icon for the model)
  "glm-5.2":        { label: "glm-5.2",        icon: ICONS["glm-5.2"]        || zcodeIcon,         color: "var(--amber)" },
  "step-3.7-flash": { label: "step-3.7-flash", icon: ICONS["step-3.7-flash"] || stepfunVendorIcon, color: "var(--lime)" },
  "deepseek-v4":    { label: "deepseek-v4",    icon: ICONS["deepseek-v4"]    || deepseekVendorIcon, color: "var(--cyan)" },
  "gpt-5":          { label: "gpt-5",          icon: ICONS["gpt-5"]          || deepseekVendorIcon, color: "var(--violet)" },
};

export interface ToolMeta {
  label: string;
  icon: string;  // SVG markup string — render with {@html meta.icon}
  color: string;
}

/** Client-id variants that share an icon with a canonical tool. tokscale
 *  reports separate ids for CLI/desktop/edition variants (e.g. "antigravity"
 *  vs "antigravity-cli"); map them to the canonical entry so the same brand
 *  icon renders for all variants. */
const ALIASES: Record<string, string> = {
  "antigravity-cli": "antigravity",
  "kilo": "kilocode",
  "opencodereview": "opencode",
  "devin-cli": "devin",
  "devin-desktop": "devin",
};

/** Look up display name & icon for a raw key (case-insensitive fallback).
 *  Returns the SVG markup for the icon; components render it with {@html}.
 *  Falls back to a colored letter badge if no SVG is available. */
export function toolMeta(key: string): ToolMeta {
  const lower = key.toLowerCase().replace(/[^a-z0-9.-]/g, "");
  const found = KNOWN[lower] ?? KNOWN[ALIASES[lower] ?? ""];
  if (found) return found;

  // Try vendor matching for model keys not in KNOWN (e.g. "claude-3-opus"
  // maps to the Claude icon via the "claude" vendor rule).
  const mv = modelVendor(key);
  if (mv) {
    const vid = VENDOR_NAME_TO_ID[mv.vendor];
    if (vid) {
      const icon = vendorIcon(vid);
      if (icon) {
        return { label: key, icon, color: mv.color };
      }
    }
  }

  // Fallback: colored letter badge
  const letter = key.replace(/[^a-zA-Z]/g, "").slice(0, 2).toUpperCase() || "?";
  const fallbackSvg = `<svg fill="currentColor" viewBox="0 0 24 24" width="1em" height="1em" xmlns="http://www.w3.org/2000/svg"><text x="12" y="16" text-anchor="middle" font-size="12" font-weight="700" font-family="sans-serif">${letter}</text></svg>`;
  return { label: key, icon: fallbackSvg, color: "var(--text-faint)" };
}

/** Vendor icon SVG for a vendor ID. Used by quota cards and settings. */
export function vendorIcon(vendorId: string): string {
    return ICONS[vendorId] ?? "";
}
