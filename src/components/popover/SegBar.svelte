<script lang="ts">
  // 7-segment nav bar. Writes the global segment store; App renders the
  // matching view. Tabs never shrink below their label width (flex: 1 0 auto),
  // so in wide-label locales (EN) the bar scrolls horizontally instead of
  // clipping — edge fades + vertical-wheel-to-horizontal scrolling keep that
  // discoverable (the scrollbar itself is hidden).
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
    { key: "status", label: label("状态", "Status") },
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
    status: "status",
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

  // ── Horizontal-overflow affordance ──
  let bar = $state<HTMLDivElement | null>(null);
  let canL = $state(false);
  let canR = $state(false);

  function updateFades(): void {
    const el = bar;
    if (!el) return;
    canL = el.scrollLeft > 1;
    canR = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
  }

  // Re-check after the tab set / language changes and on font settle.
  $effect(() => {
    void segments;
    void config.language;
    // Tick past render so widths reflect the new labels.
    const raf = requestAnimationFrame(updateFades);
    return () => cancelAnimationFrame(raf);
  });

  function onWheel(e: WheelEvent): void {
    const el = bar;
    if (!el || el.scrollWidth <= el.clientWidth) return;
    // Translate vertical wheel into horizontal tab scrolling.
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      el.scrollLeft += e.deltaY;
      e.preventDefault();
    }
    updateFades();
  }
</script>

<div
  class="segbar"
  bind:this={bar}
  onscroll={updateFades}
  onwheel={onWheel}
  data-testid="segbar"
  role="tablist"
>
  {#each segments as s (s.key)}
    <button
      data-testid={"segment-" + s.key}
      role="tab"
      aria-selected={active === s.key}
      class:active={active === s.key}
      onclick={() => setSegment(s.key)}
    >{s.label}</button>
  {/each}
  <span class="fade-l" class:show={canL} aria-hidden="true"></span>
  <span class="fade-r" class:show={canR} aria-hidden="true"></span>
</div>

<style>
  .segbar {
    display: flex;
    border-bottom: 1px solid var(--border-dim);
    padding: 0 12px;
    overflow-x: auto;
    scrollbar-width: none;
    flex-shrink: 0;
    position: relative;
  }
  .segbar::-webkit-scrollbar {
    display: none;
  }
  .segbar button {
    flex: 1 0 auto;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-faint);
    font-family: var(--font-ui);
    font-size: 0.8667rem;
    font-weight: 500;
    padding: 16px 7px 14px;
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

  /* Edge fades — visible only while that side has content cut off. They sit
   * on the popover background (rgba(var(--app-bg), 1) → transparent). */
  .fade-l,
  .fade-r {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 18px;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .fade-l {
    left: 0;
    background: linear-gradient(to right, rgba(var(--app-bg), 1), transparent);
  }
  .fade-r {
    right: 0;
    background: linear-gradient(to left, rgba(var(--app-bg), 1), transparent);
  }
  .fade-l.show,
  .fade-r.show {
    opacity: 1;
  }
</style>
