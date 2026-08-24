// Appearance: applies theme / animation / font config to <html> as data
// attributes and CSS custom properties so app.css overrides take effect.
// Handles "system" mode by listening to the relevant media queries.
//
// Both the main popover and the settings window call `applyAppearance(config)`
// whenever their config state changes.

import type { Config } from "./api";

type Unlisten = () => void;

let themeMq: MediaQueryList | null = null;
let animMq: MediaQueryList | null = null;
let lastConfig: Partial<Config> = {};

// ── Font size presets ──────────────────────────────────────────────────────
const FONT_SIZE_MAP: Record<string, string> = {
  small: "13px",
  medium: "15px",
  large: "17px",
};

// ── Font family presets ────────────────────────────────────────────────────
const FONT_FAMILY_MAP: Record<string, { ui: string; mono: string; display: string }> = {
  app: {
    ui: '"Hanken Grotesk", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    mono: '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
    display: '"Fraunces", serif',
  },
  system: {
    ui: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    mono: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
    display: 'system-ui, -apple-system, sans-serif',
  },
  mono: {
    ui: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
    mono: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace',
    display: '"JetBrains Mono", ui-monospace, monospace',
  },
};

/** Resolve "system" → actual preference via media query. */
function resolveTheme(theme: string | undefined): "dark" | "light" {
  if (theme === "light" || theme === "dark") return theme;
  // "system" or unset
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** Resolve "system" → actual preference via prefers-reduced-motion. */
function resolveAnimation(animation: string | undefined): "on" | "off" {
  if (animation === "on" || animation === "off") return animation;
  // "system" or unset
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "off" : "on";
}

/** Apply the theme / animation / font attributes and CSS vars to <html>. */
export function applyAppearance(config: Partial<Config>): void {
  lastConfig = config;
  const root = document.documentElement;

  // Theme
  root.setAttribute("data-theme", resolveTheme(config.theme));

  // Animation
  root.setAttribute("data-animation", resolveAnimation(config.animation));

  // Font size
  const size = FONT_SIZE_MAP[config.font_size ?? "medium"] ?? FONT_SIZE_MAP.medium;
  root.style.setProperty("--font-size-base", size);

  // Font family
  const family = FONT_FAMILY_MAP[config.font_family ?? "app"] ?? FONT_FAMILY_MAP.app;
  root.style.setProperty("--font-ui", family.ui);
  root.style.setProperty("--font-mono", family.mono);
  root.style.setProperty("--font-display", family.display);
}

/** Start listening to system media queries so "system" mode tracks OS changes.
 *  Call once on mount; returns an unlisten function. */
export function initAppearanceListeners(): Unlisten {
  themeMq = window.matchMedia("(prefers-color-scheme: light)");
  animMq = window.matchMedia("(prefers-reduced-motion: reduce)");

  const onTheme = (): void => {
    if (lastConfig.theme === "system" || !lastConfig.theme) {
      document.documentElement.setAttribute("data-theme", resolveTheme("system"));
    }
  };
  const onAnim = (): void => {
    if (lastConfig.animation === "system" || !lastConfig.animation) {
      document.documentElement.setAttribute("data-animation", resolveAnimation("system"));
    }
  };

  themeMq.addEventListener("change", onTheme);
  animMq.addEventListener("change", onAnim);

  return (): void => {
    themeMq?.removeEventListener("change", onTheme);
    animMq?.removeEventListener("change", onAnim);
  };
}
