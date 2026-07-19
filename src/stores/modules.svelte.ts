// Overview module visibility (分项 / 缓存命中 / 工具 / 模型 / 额度 show-hide).
// V1 in-memory; persisted via config later.

export type ModuleKey = "split" | "hitrate" | "tools" | "models" | "limits";

export const MODULE_LABELS: Record<ModuleKey, string> = {
  split: "分项",
  hitrate: "缓存命中",
  tools: "工具",
  models: "模型",
  limits: "额度",
};

export const MODULE_ORDER: ModuleKey[] = ["split", "hitrate", "tools", "models", "limits"];

let visible = $state<Record<ModuleKey, boolean>>({
  split: true,
  hitrate: true,
  tools: true,
  models: true,
  limits: true,
});

export function isModuleVisible(k: ModuleKey): boolean {
  return visible[k];
}

export function toggleModule(k: ModuleKey): void {
  visible[k] = !visible[k];
}

export function moduleVisibility(): Record<ModuleKey, boolean> {
  return visible;
}
