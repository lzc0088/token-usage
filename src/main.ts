import "./app.css";
import App from "./App.svelte";
import SettingsApp from "./views/SettingsApp.svelte";
import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./lib/api";

// Debug mode: enabled by --debug CLI flag (checked once at startup).
// Bridges uncaught errors into the persistent log with a stack trace so
// render exceptions (e.g. Svelte each_key_duplicate) are diagnosable without
// devtools. Only active when the flag is set.

async function init(): Promise<void> {
  let debug = false;
  try {
    const mod = await import("@tauri-apps/api/core");
    debug = await (mod as unknown as { invoke: (c: string) => Promise<boolean> }).invoke("is_debug_mode");
  } catch {}

  if (debug) {
    window.addEventListener("error", (e) => {
      const stack = e.error instanceof Error ? `\n${e.error.stack ?? ""}` : "";
      api.feLog(`[DIAG][onerror] ${e.message}${stack}`);
    });
    window.addEventListener("unhandledrejection", (e) => {
      const r = e.reason;
      const msg = r instanceof Error ? `${r.message}\n${r.stack ?? ""}` : String(r);
      api.feLog(`[DIAG][unhandled] ${msg}`);
    });
  }

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
}

init();
