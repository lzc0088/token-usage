import "./app.css";
import App from "./App.svelte";
import SettingsApp from "./views/SettingsApp.svelte";
import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./lib/api";

// An exception thrown inside a Svelte $effect during a reactive flush kills
// the whole effect chain silently (e.g. the v1.0.14 累计 freeze — a duplicate
// each-key in the heatmap). The webview console is not visible from outside,
// so bridge every uncaught error / rejection into the persistent log with a
// stack. Messages carry the [DIAG] prefix so Rust routes them to the file
// log (see frontend_log).
window.addEventListener("error", (e) => {
  const stack = e.error instanceof Error ? `\n${e.error.stack ?? ""}` : "";
  api.feLog(`[DIAG][onerror] ${e.message}${stack}`);
});
window.addEventListener("unhandledrejection", (e) => {
  const r = e.reason;
  const msg = r instanceof Error ? `${r.message}\n${r.stack ?? ""}` : String(r);
  api.feLog(`[DIAG][unhandled] ${msg}`);
});

// Detect platform for font strategy.
// Google Fonts (Hanken Grotesk, Fraunces, JetBrains Mono) render well on
// macOS where the CDN is fast and fonts are cached. On Windows, skip the
// remote fetch and rely on system fonts (Segoe UI / Consolas) for offline
// reliability and faster startup.
const platform = typeof navigator !== "undefined"
  ? /Mac|iPod|iPhone|iPad/.test(navigator.platform ?? "")
    ? "macos"
    : /Win/.test(navigator.platform ?? "")
      ? "windows"
      : "linux"
  : "macos";

if (platform !== "windows") {
  // Add Google Fonts for design consistency with wireframe
  const link = document.createElement("link");
  link.href = "https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,300;9..144,400;9..144,500;9..144,600&family=JetBrains+Mono:wght@400;500;600&family=Hanken+Grotesk:wght@300;400;500;600;700&display=swap";
  link.rel = "stylesheet";
  document.head.appendChild(link);
}

// Branch on the window label: the `settings` window mounts the settings UI,
// every other window (the `main` popover) mounts the app.
const label = getCurrentWindow().label;
const Root = label === "settings" ? SettingsApp : App;

mount(Root, {
  target: document.getElementById("app")!,
});
