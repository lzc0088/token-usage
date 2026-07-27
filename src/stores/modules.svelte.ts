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

// Load from config on init.
api.getConfig().then(cfg => {
  if (cfg?.layout_overview_sub) {
    const s = new Set(cfg.layout_overview_sub);
    for (const k of MODULE_ORDER) {
      visible[k] = s.has(k);
    }
  }
}).catch(() => {});

export function isModuleVisible(k: ModuleKey): boolean {
  return visible[k];
}

export async function toggleModule(k: ModuleKey): Promise<void> {
  visible[k] = !visible[k];
  // Persist to config so settings page stays in sync.
  const keys = MODULE_ORDER.filter(kk => visible[kk]);
  try {
    const cfg = await api.getConfig();
    await api.setConfig({ ...cfg, layout_overview_sub: keys });
  } catch {}
}

export function moduleVisibility(): Record<ModuleKey, boolean> {
  return visible;
}
