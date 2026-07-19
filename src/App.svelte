<script lang="ts">
  // Popover shell (M3/M4): Hero + period switcher + SegBar + active segment.
  import { listen } from "@tauri-apps/api/event";
  import Hero from "./components/popover/Hero.svelte";
  import PeriodSwitcher from "./components/popover/PeriodSwitcher.svelte";
  import SegBar from "./components/popover/SegBar.svelte";
  import Overview from "./components/segments/Overview.svelte";
  import Tools from "./components/segments/Tools.svelte";
  import Models from "./components/segments/Models.svelte";
  import Projects from "./components/segments/Projects.svelte";
  import Sessions from "./components/segments/Sessions.svelte";
  import Trend from "./components/segments/Trend.svelte";
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
        // Don't surface raw error text (may leak paths / internals). Log to console.
        if (!cancelled) {
          console.error("summary/config load failed", e);
          loadError = "加载失败，请稍后重试";
        }
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

  <main class="seg-scroll">
    {#if loadError}
      <p class="err">{loadError}</p>
    {:else if segment === "ov"}
      <Overview {summary} currency={config.currency} />
    {:else if segment === "tools"}
      <Tools currency={config.currency} />
    {:else if segment === "models"}
      <Models currency={config.currency} />
    {:else if segment === "projects"}
      <Projects currency={config.currency} />
    {:else if segment === "sess"}
      <Sessions currency={config.currency} />
    {:else if segment === "trend"}
      <Trend />
    {:else}
      <p class="placeholder">「{segment}」分段 · M4 待实装</p>
    {/if}
  </main>
</div>

<style>
  .popover {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .pop-hero {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 18px 16px 14px;
    gap: 12px;
    flex-shrink: 0;
  }
  .seg-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
  }
  .seg-scroll::-webkit-scrollbar {
    width: 6px;
  }
  .seg-scroll::-webkit-scrollbar-thumb {
    background: var(--glass-3);
    border-radius: 3px;
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
