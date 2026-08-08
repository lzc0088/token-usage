import "./app.css";
import App from "./App.svelte";
import SettingsApp from "./views/SettingsApp.svelte";
import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
