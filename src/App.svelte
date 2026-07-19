<script lang="ts">
  // Popover shell (M3/M4): Hero + period switcher + SegBar + active segment.
  import { listen } from "@tauri-apps/api/event";
  import Hero from "./components/popover/Hero.svelte";
  import PeriodSwitcher from "./components/popover/PeriodSwitcher.svelte";
  import SegBar from "./components/popover/SegBar.svelte";
  import Overview from "./components/segments/Overview.svelte";
  import { api, type Config, type Summary } from "./lib/api";
  import { periodValue } from "./stores/period.svelte";
  import { segmentValue } from "./stores/segment.svelte";

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);

  $effect(() => {
    const period = periodValue();
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

  $effect(() => {
    const unlisten_promise = listen<Summary>("today:updated", (e) => {
      if (periodValue() === "day") summary = e.payload;
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  let segment = $derived(segmentValue());
</script>

<div class="popover">
  <header class="pop-hero">
    <Hero {summary} currency={config.currency} />
    <PeriodSwitcher />
  </header>

  <SegBar />

  {#if loadError}
    <p class="err">加载失败：{loadError}</p>
  {:else if segment === "ov"}
    <Overview {summary} currency={config.currency} />
  {:else}
    <p class="placeholder">「{segment}」分段 · M4 待实装</p>
  {/if}
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
  .err {
    margin: 16px;
    color: var(--coral);
    font-size: 12px;
  }
  .placeholder {
    margin: 24px 16px;
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
