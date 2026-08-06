<script lang="ts">
  // 7-segment nav bar. Writes the global segment store; App renders the
  // matching view. Only 总览 is built in T4.1; others fall back to a placeholder.
  import { getSegment, setSegment } from "../../stores/segment.svelte";
  import type { Config } from "../../lib/api";

  let { config }: { config: Config } = $props();

  // Label lookup — returns zh/en label based on config.language.
  function label(zh: string, en: string): string {
    return config.language === "en" ? en : zh;
  }

  // Default segment order (all visible by default).
  let DEFAULT_SEGMENTS = $derived([
    { key: "ov", label: label("总览", "Overview") },
    { key: "tools", label: label("工具", "Tools") },
    { key: "models", label: label("模型", "Models") },
    { key: "projects", label: label("项目", "Projects") },
    { key: "sess", label: label("会话", "Sessions") },
    { key: "trend", label: label("趋势", "Trends") },
    { key: "limit", label: label("额度", "Quota") },
  ]);

  // Map config.layout_modules keys to segment keys.
  const MODULE_KEY_MAP: Record<string, string> = {
    overview: "ov",
    tools: "tools",
    models: "models",
    projects: "projects",
    sessions: "sess",
    trends: "trend",
    quotas: "limit",
  };

  // Build visible + ordered segments from config.
  // - null / undefined  → never configured → show all (default).
  // - [ … ]            → user has a custom order → show exactly that list.
  //   An empty array means the user explicitly cleared every module, and the
  //   tab bar should reflect that (no tabs) — NOT fall back to the default.
  const segments = $derived.by(() => {
    const configured = config.layout_modules;
    const moduleKeys = configured ?? DEFAULT_SEGMENTS.map(s => s.key);
    const visible: { key: string; label: string }[] = [];
    for (const mk of moduleKeys) {
      const sk = MODULE_KEY_MAP[mk];
      if (sk) {
        const def = DEFAULT_SEGMENTS.find(s => s.key === sk);
        if (def) visible.push(def);
      }
    }
    // Only fall back when the config was genuinely absent (null); an explicit
    // empty list is honoured as "show nothing".
    return (configured != null || visible.length > 0) ? visible : DEFAULT_SEGMENTS;
  });

  let active = $derived(getSegment());
</script>

<nav class="segbar" data-testid="segbar">
  {#each segments as s (s.key)}
    <button
      data-testid={"segment-" + s.key}
      class:active={active === s.key}
      aria-current={active === s.key ? "page" : undefined}
      onclick={() => setSegment(s.key)}
    >{s.label}</button>
  {/each}
</nav>

<style>
  .segbar {
    display: flex;
    border-bottom: 1px solid var(--border-dim);
    padding: 0 12px;
    overflow-x: auto;
    scrollbar-width: none;
    flex-shrink: 0;
  }
  .segbar::-webkit-scrollbar {
    display: none;
  }
  .segbar button {
    flex: 1;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-faint);
    font-family: var(--font-ui);
    font-size: 13px;
    font-weight: 500;
    padding: 16px 4px 14px;
    cursor: pointer;
    white-space: nowrap;
    transition: 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .segbar button:hover {
    color: var(--text-dim);
  }
  .segbar button.active {
    color: var(--amber);
    font-weight: 700;
    border-bottom-color: var(--amber);
  }
</style>
