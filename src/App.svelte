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
  import { invoke } from "@tauri-apps/api/core";
  import { api, type Config, type Summary } from "./lib/api";
  import { applyAppearance, initAppearanceListeners } from "./lib/appearance";
  import { periodValue } from "./stores/period.svelte";
  import { segmentValue } from "./stores/segment.svelte";

  const appWindow = getCurrentWindow();

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);
  let lastUpdated = $state<number>(0); // epoch ms
  let refreshTrigger = $state(0); // Force segment refresh on global refresh
  // USD→CNY rate for cost conversion. Loaded from the latest stored value
  // (auto or manual) and refreshed on rate:updated / config:changed.
  let cnyRate = $state<number>(7.2);

  // Auto-hide on blur — settings is now a separate window, so main always
  // hides when it loses focus.
  $effect(() => {
    function hideOnBlur() {
      appWindow.hide();
    }
    window.addEventListener("blur", hideOnBlur);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) appWindow.hide();
    });
    return () => {
      window.removeEventListener("blur", hideOnBlur);
    };
  });

  // Reload config on focus — picks up changes made from the tray menu
  // (theme, window display mode) without needing a config:changed event.
  $effect(() => {
    async function onFocus() {
      try {
        const c = await api.getConfig();
        config = c;
        applyAppearance(c);
      } catch { /* ignore */ }
    }
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  let period = $derived(periodValue());

  // Sync period from config.default_period ONCE on startup. The periodSynced
  // guard ensures this never re-runs when the user later switches period via
  // the PeriodSwitcher — otherwise clicking MONTH/TOTAL would read `period`,
  // see it differ from default_period, and snap straight back to "day".
  let periodSynced = $state(false);
  $effect(() => {
    const cfg = config;
    if (periodSynced || !cfg.default_period) return;
    periodSynced = true;
    if (cfg.default_period !== periodValue()) {
      // Dynamically import to avoid circular deps.
      import("./stores/period.svelte").then(({ setPeriod }) => {
        setPeriod(cfg.default_period as "day" | "month" | "total");
      }).catch(console.error);
    }
  });

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

  // Live-apply settings changes saved from the settings window (layout,
  // currency, period, quota vendors) without requiring a manual refresh.
  $effect(() => {
    const unlisten_promise = listen<void>("config:changed", () => {
      api.getConfig()
        .then((c) => { config = c; lastUpdated = Date.now(); })
        .catch(console.error);
      reloadRate();
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Tray context menu "立即刷新" → trigger a full data refresh.
  $effect(() => {
    const unlisten_promise = listen<void>("tray:refresh", () => {
      void refreshData();
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Reload the stored USD→CNY rate (covers auto-fetch, manual save, and
  // config changes — all emit one of the events above or rate:updated).
  function reloadRate(): void {
    api.getLatestRate()
      .then((info) => { cnyRate = info.rate; })
      .catch(console.error);
  }

  $effect(() => {
    reloadRate();
    const unlisten_promise = listen<void>("rate:updated", () => reloadRate());
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Apply appearance (theme / animation) whenever config changes, and
  // listen to system media queries for "system" mode.
  $effect(() => {
    applyAppearance(config);
  });
  $effect(() => {
    return initAppearanceListeners();
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

  // Global refresh feedback: shown to the left of the refresh button.
  let refreshStatus = $state<"idle" | "loading" | "ok" | "fail">("idle");
  let refreshMsg = $state("");

  async function refreshData() {
    refreshStatus = "loading";
    refreshMsg = "";
    try {
      // Refresh usage data + quota data in parallel.
      await Promise.all([api.getSummary(periodValue()), api.getConfig(), api.refreshQuotas()]);
      summary = await api.getSummary(periodValue());
      config = await api.getConfig();
      loadError = null;
      refreshStatus = "ok";
      refreshMsg = "刷新成功";
      // Increment trigger to force current segment to reload.
      refreshTrigger++;
      setTimeout(() => { refreshStatus = "idle"; refreshMsg = ""; }, 3000);
    } catch (e) {
      console.error("refresh failed", e);
      refreshStatus = "fail";
      refreshMsg = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="popover" class:draggable={config.window_display_mode === "normal"}>
  <header class="pop-hero">
    <Hero {summary} currency={config.currency} {cnyRate} />
    <div class="hero-right">
      <PeriodSwitcher />
    </div>
  </header>

  <SegBar {config} />

  <main class="seg-scroll">
    {#if loadError}
      <p class="err">{loadError}</p>
    {:else if segment === "ov"}
      {#key "ov-" + refreshTrigger}
        <Overview {summary} currency={config.currency} config={config} {cnyRate} />
      {/key}
    {:else if segment === "tools"}
      {#key "tools-" + refreshTrigger}
        <Tools currency={config.currency} {cnyRate} />
      {/key}
    {:else if segment === "models"}
      {#key "models-" + refreshTrigger}
        <Models currency={config.currency} {cnyRate} />
      {/key}
    {:else if segment === "projects"}
      {#key "projects-" + refreshTrigger}
        <Projects currency={config.currency} {cnyRate} />
      {/key}
    {:else if segment === "sess"}
      {#key "sess-" + refreshTrigger}
        <Sessions currency={config.currency} {cnyRate} />
      {/key}
    {:else if segment === "trend"}
      {#key "trend-" + refreshTrigger}
        <Trend />
      {/key}
    {:else if segment === "limit"}
      {#key "limit-" + refreshTrigger}
        <Limits />
      {/key}
    {:else}
      <p class="placeholder">「{segment}」分段 · M4 待实装</p>
    {/if}
  </main>

  <footer class="pop-footer">
    <div class="l"><span class="live"></span>最新刷新 {updatedStr}</div>
    <div class="r">
      {#if refreshStatus === "loading"}
        <span class="refresh-feedback loading">刷新中…</span>
      {:else if refreshStatus === "ok"}
        <span class="refresh-feedback ok">{refreshMsg}</span>
      {:else if refreshStatus === "fail"}
        <span class="refresh-feedback fail">刷新失败</span>
      {/if}
      <button class="fbtn" onclick={() => refreshData()} disabled={refreshStatus === "loading"} title="刷新" aria-label="刷新">↻</button>
      <button class="fbtn fbtn-gear" onclick={() => invoke("open_settings")} title="设置" aria-label="设置">⚙</button>
    </div>
  </footer>
</div>

<style>
  .popover {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: rgba(var(--app-bg), var(--app-bg-opacity));
    border-radius: 15px;
    overflow: hidden;
  }
  .popover.draggable {
    cursor: grab;
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
    align-items: center;
  }
  .refresh-feedback { font-size: 10.5px; line-height: 1; }
  .refresh-feedback.loading { color: var(--text-dim); }
  .refresh-feedback.ok { color: var(--lime); }
  .refresh-feedback.fail { color: var(--coral); max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fbtn {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    font-family: inherit;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: 0.15s;
    min-width: 32px;
    min-height: 32px;
  }
  .fbtn-gear { font-size: 21px; }
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

  /* Ensure interactive elements maintain their cursor when in draggable mode */
  .popover.draggable button,
  .popover.draggable .seg-scroll {
    cursor: auto;
  }
  .popover.draggable .fbtn {
    cursor: pointer;
  }
</style>
