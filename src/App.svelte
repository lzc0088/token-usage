<script lang="ts">
  // Popover shell (M3): Hero + global period switcher + SegBar.
  // - Summary re-loads whenever the period changes ($effect deps on it).
  // - Listens to the `today:updated` event for real-time DAY refresh.
  import { listen } from "@tauri-apps/api/event";
  import Hero from "./components/popover/Hero.svelte";
  import PeriodSwitcher from "./components/popover/PeriodSwitcher.svelte";
  import SegBar from "./components/popover/SegBar.svelte";
  import { api, type Config, type Summary } from "./lib/api";
  import { periodValue } from "./stores/period.svelte";

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);

  // (Re)load summary + config whenever the period changes.
  $effect(() => {
    const period = periodValue(); // tracked → effect re-runs on change
    let cancelled = false;
    (async () => {
      try {
        const [s, c] = await Promise.all([api.getSummary(period), api.getConfig()]);
        if (cancelled) return;
        summary = s;
        config = c;
        loadError = null;
      } catch (e) {
        if (!cancelled) loadError = String(e);
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  // Real-time: collector emits `today:updated` (a Summary) after each scan.
  // Only adopt it while viewing DAY (month/total come from the DB on a timer).
  $effect(() => {
    const unlisten_promise = listen<Summary>("today:updated", (e) => {
      if (periodValue() === "day") summary = e.payload;
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });
</script>

<div class="popover">
  <header class="pop-hero">
    <Hero {summary} currency={config.currency} />
    <PeriodSwitcher />
  </header>

  <SegBar />

  <main class="pop-body">
    {#if loadError}
      <p class="err">加载失败：{loadError}</p>
    {:else if !summary}
      <p class="muted">加载中…</p>
    {:else}
      <p class="muted">M3 骨架就绪 · 分段视图见 M4</p>
      <p class="hint">
        {summary.input.toLocaleString()} 入 / {summary.output.toLocaleString()} 出 ·
        {summary.cache_read.toLocaleString()} 缓存读
      </p>
    {/if}
  </main>
</div>

<style>
  .popover {
    display: flex;
    flex-direction: column;
  }
  .pop-hero {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 18px 16px 14px;
    gap: 12px;
  }
  .pop-body {
    padding: 14px 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .muted {
    margin: 0;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
  .err {
    margin: 0;
    color: var(--coral);
    font-size: 12px;
  }
</style>
