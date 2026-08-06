// Overview module visibility (分项 / 工具 / 模型 / 额度 show-hide).
// Synced to config.layout_overview_sub so settings page and popover stay in sync.

import { api } from "../lib/api";

export type ModuleKey = "split" | "tools" | "models" | "limits";

export const MODULE_LABELS: Record<ModuleKey, string> = {
  split: "分项",
  tools: "工具",
  models: "模型",
  limits: "额度",
};

export const MODULE_ORDER: ModuleKey[] = ["split", "tools", "models", "limits"];

let visible = $state<Record<ModuleKey, boolean>>({
  split: true,
  tools: true,
  models: true,
  limits: true,
});

// Guards: true after loadConfig resolves, preventing toggleModule (which reads
// default state and persists it to config) from racing with initial config load.
let loaded = false;

// Load from config on init. Retry up to 3 times with backoff (250ms/500ms/1000ms)
// in case the Tauri IPC isn't ready when this module initializes (e.g. cold start
// where the webview loads before the Rust backend completes setup).
async function loadConfig(retries = 3): Promise<void> {
  for (let i = 0; i < retries; i++) {
    try {
      const cfg = await api.getConfig();
      if (cfg?.layout_overview_sub) {
        const s = new Set(cfg.layout_overview_sub);
        const next: Record<ModuleKey, boolean> = { ...visible };
        for (const k of MODULE_ORDER) {
          next[k] = s.has(k);
        }
        visible = next;
      }
      loaded = true;
      return; // success — stop retrying
    } catch {
      if (i < retries - 1) {
        await new Promise(r => setTimeout(r, 250 * (1 << i))); // 250, 500, 1000 ms
      }
    }
  }
}

loadConfig();

export function isModuleVisible(k: ModuleKey): boolean {
  return visible[k];
}

export async function toggleModule(k: ModuleKey): Promise<void> {
  // Wait for initial config load before toggling, otherwise toggling on the
  // default state then having loadConfig overwrite visible would undo the toggle.
  if (!loaded) return;
  const next = { ...visible };
  next[k] = !next[k];
  visible = next;
  // Persist to config so settings page stays in sync.
  const keys = MODULE_ORDER.filter(kk => next[kk]);
  try {
    const cfg = await api.getConfig();
    await api.setConfig({ ...cfg, layout_overview_sub: keys });
  } catch (e) {
    api.feLog(`toggleModule failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

export function moduleVisibility(): Record<ModuleKey, boolean> {
  return visible;
}
