<script lang="ts">
  // Popover shell (M3/M4): Hero + period switcher + SegBar + active segment.
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Hero from "./components/popover/Hero.svelte";
  import PeriodSwitcher from "./components/popover/PeriodSwitcher.svelte";
  import SegBar from "./components/popover/SegBar.svelte";
  import Overview from "./components/segments/Overview.svelte";
  import Tools from "./components/segments/Tools.svelte";
  import Models from "./components/segments/Models.svelte";
  import Projects from "./components/segments/Projects.svelte";
  import Sessions from "./components/segments/Sessions.svelte";
  import Trend from "./components/segments/Trend.svelte";
  import Limits from "./components/segments/Limits.svelte";
  import SettingsModal from "./views/SettingsModal.svelte";
  import { api, type Config, type Summary } from "./lib/api";
  import { periodValue } from "./stores/period.svelte";
  import { segmentValue } from "./stores/segment.svelte";
  import { isSettingsOpen, openSettings } from "./stores/settings.svelte";

  const appWindow = getCurrentWindow();

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);
  let lastUpdated = $state<number>(0); // epoch ms

  // Auto-hide on blur — works even with transparent macOS windows
  // because DOM blur / visibility events fire regardless of NSWindow type.
  $effect(() => {
    function hideOnBlur() {
      if (!isSettingsOpen()) appWindow.hide();
    }
    window.addEventListener("blur", hideOnBlur);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden && !isSettingsOpen()) appWindow.hide();
    });
    return () => {
      window.removeEventListener("blur", hideOnBlur);
    };
  });

  let period = $derived(periodValue());

  $effect(() => {
    const p = period;
    let cancelled = false;
    (async () => {
      try {
        const [s, c] = await Promise.all([api.getSummary(p), api.getConfig()]);
        if (cancelled) return;
        summary = s;
        config = c;
        loadError = null;
        if (!lastUpdated) lastUpdated = Date.now();
      } catch (e) {
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
      lastUpdated = Date.now();
      if (periodValue() === "day") summary = e.payload;
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  let segment = $derived(segmentValue());

  function updatedTime(): string {
    if (!lastUpdated) return "";
    return new Date(lastUpdated).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
  }
  let updatedStr = $state(updatedTime());

  // Sync updatedStr when lastUpdated changes
  $effect(() => {
    updatedStr = updatedTime();
  });

  async function refreshData() {
    try {
      const [s, c] = await Promise.all([api.getSummary(periodValue()), api.getConfig()]);
      summary = s;
      config = c;
      loadError = null;
    } catch (e) {
      console.error("refresh failed", e);
    }
  }
</script>

<div class="popover">
  <header class="pop-hero">
    <Hero {summary} currency={config.currency} />
    <div class="hero-right">
      <PeriodSwitcher />
    </div>
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
    {:else if segment === "limit"}
      <Limits />
    {:else}
      <p class="placeholder">「{segment}」分段 · M4 待实装</p>
    {/if}
  </main>

  <footer class="pop-footer">
    <div class="l"><span class="live"></span>最新刷新 {updatedStr}</div>
    <div class="r">
      <button class="fbtn" onclick={() => refreshData()} title="刷新" aria-label="刷新">↻</button>
      <button class="fbtn fbtn-gear" onclick={openSettings} title="设置" aria-label="设置">⚙</button>
    </div>
  </footer>
</div>

{#if isSettingsOpen()}
  <SettingsModal />
{/if}

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
    padding: 14px 18px 13px;
    gap: 8px;
    flex-shrink: 0;
    position: relative;
  }
  .pop-hero::after {
    content: "";
    position: absolute;
    left: 18px;
    right: 18px;
    bottom: 0;
    height: 1px;
    background: linear-gradient(90deg, transparent, rgba(232, 176, 75, 0.2), transparent);
  }
  .hero-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
  }
  .pop-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 18px;
    border-top: 1px solid var(--border-dim);
    background: rgba(0, 0, 0, 0.15);
    flex-shrink: 0;
  }
  .pop-footer .l {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-faint);
  }
  .pop-footer .l .live {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--lime);
    box-shadow: 0 0 5px var(--lime);
  }
  .pop-footer .r {
    display: flex;
    gap: 6px;
  }
  .fbtn {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 5px 9px;
    border-radius: 6px;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    font-family: inherit;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: 0.15s;
    min-width: 30px;
    min-height: 30px;
  }
  .fbtn-gear { font-size: 20px; }
  .fbtn:hover {
    color: var(--amber);
    border-color: var(--amber-soft);
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
