// Appearance: applies theme / animation config to <html> as data attributes
// so app.css CSS-variable overrides take effect. Handles "system" mode by
// listening to the relevant media queries.
//
// Both the main popover and the settings window call `applyAppearance(config)`
// whenever their config state changes.

import type { Config } from "./api";

type Unlisten = () => void;

let themeMq: MediaQueryList | null = null;
let animMq: MediaQueryList | null = null;
let lastConfig: Partial<Config> = {};

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

/** Apply the theme / animation attributes to <html>. */
export function applyAppearance(config: Partial<Config>): void {
  lastConfig = config;
  document.documentElement.setAttribute("data-theme", resolveTheme(config.theme));
  document.documentElement.setAttribute("data-animation", resolveAnimation(config.animation));
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
