<script lang="ts">
  // Popover shell (M3/M4): Hero + period switcher + SegBar + active segment.
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Hero from "./components/popover/Hero.svelte";
  import PeriodSwitcher from "./components/popover/PeriodSwitcher.svelte";
  import SegBar from "./components/popover/SegBar.svelte";
  import Overview from "./components/segments/Overview.svelte";
  import BreakdownSegment from "./components/segments/BreakdownSegment.svelte";
    import Projects from "./components/segments/Projects.svelte";
  import Sessions from "./components/segments/Sessions.svelte";
  import Trend from "./components/segments/Trend.svelte";
  import Limits from "./components/segments/Limits.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { api, type Config, type Summary } from "./lib/api";
  import { applyAppearance, initAppearanceListeners } from "./lib/appearance";
  import { TODAY_UPDATED, CONFIG_CHANGED, RATE_UPDATED, TRAY_REFRESH } from "./lib/events";
  import { periodValue } from "./stores/period.svelte";
  import { segmentValue } from "./stores/segment.svelte";

  const appWindow = getCurrentWindow();

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);
  let refreshTrigger = $state(0); // Force segment refresh on global refresh
  // USD→CNY rate for cost conversion. Loaded from the latest stored value
  // (auto or manual) and refreshed on rate:updated / config:changed.
  let cnyRate = $state<number>(7.2);

  // ── Version & update check ──
  let appVersion = $state("1.0.0");
  let updateInfo = $state<{ hasUpdate: boolean; url: string; version: string } | null>(null);
  async function checkForUpdate(): Promise<void> {
    try {
      const ver = await api.getAppVersion();
      appVersion = ver;
      const repo = "gitee.com/lzc0088/token-usage"; // matches .env VITE_UPDATE_REPO
      const info = await api.checkUpdate(repo, ver);
      if (info.has_update) {
        updateInfo = { hasUpdate: true, url: info.url, version: info.version };
      }
    } catch { /* network error — skip */ }
  }

  // Auto-hide on blur — settings is now a separate window, so main always
  // hides when it loses focus.
  const refreshTimeouts = new Set<number>();
  $effect(() => {
    return () => { for (const id of refreshTimeouts) clearTimeout(id); };
  });
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
  // Debounced to prevent redundant IPC on rapid focus/blur (e.g. popover hiding).
  let focusTimer: ReturnType<typeof setTimeout> | null = null;
  // Gate network-heavy checkForUpdate to once every 5 minutes.
  let lastCheckMs = 0;
  $effect(() => {
    async function onFocus() {
      if (focusTimer) clearTimeout(focusTimer);
      focusTimer = setTimeout(async () => {
        focusTimer = null;
        try {
          const c = await api.getConfig();
          config = c;
          applyAppearance(c);
        } catch { /* ignore */ }
        const now = Date.now();
        if (now - lastCheckMs > 300_000) {
          lastCheckMs = now;
          void checkForUpdate();
        }
      }, 250);
    }
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      if (focusTimer) clearTimeout(focusTimer);
    };
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
      }).catch(() => { /* setPeriod import failed, non-critical */ });
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
      } catch (e) {
        if (!cancelled) {
          loadError = "加载失败，请稍后重试";
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const unlisten_promise = listen<Summary>(TODAY_UPDATED, (e) => {
            if (periodValue() === "day") summary = e.payload;
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Check for updates on first mount.
  $effect(() => { void checkForUpdate(); });

  // Live-apply settings changes saved from the settings window (layout,
  // currency, period, quota vendors) without requiring a manual refresh.
  $effect(() => {
    const unlisten_promise = listen<void>(CONFIG_CHANGED, () => {
      api.getConfig()
        .then((c) => { config = c; })
        .catch(() => { /* config reload failed */ });
      reloadRate();
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Tray context menu "立即刷新" → trigger a full data refresh.
  $effect(() => {
    const unlisten_promise = listen<void>(TRAY_REFRESH, () => {
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
      .catch(() => { /* rate reload failed */ });
  }

  $effect(() => {
    reloadRate();
    const unlisten_promise = listen<void>(RATE_UPDATED, () => reloadRate());
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

  // Global refresh feedback: shown to the left of the refresh button.
  let refreshStatus = $state<"idle" | "loading" | "ok" | "fail">("idle");
  let refreshMsg = $state("");

  async function refreshData() {
    refreshStatus = "loading";
    refreshMsg = "";
    try {
      // Refresh usage data + quota data in parallel.
      const [s, c] = await Promise.all([api.getSummary(periodValue()), api.getConfig(), api.refreshQuotas()]);
      summary = s;
      config = c;
      loadError = null;
      refreshStatus = "ok";
      refreshMsg = "刷新成功";
      // Increment trigger to force current segment to reload.
      refreshTrigger++;
      const _rid = window.setTimeout(() => { refreshStatus = "idle"; refreshMsg = ""; }, 3000);
      refreshTimeouts.add(_rid);
    } catch (e) {
      /* refresh failed — shown in refreshMsg */
      refreshStatus = "fail";
      refreshMsg = "刷新失败，请稍后重试";
    }
  }
</script>

<div class="popover" class:draggable={config.window_display_mode !== "fixed"}>
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
        <BreakdownSegment currency={config.currency} {cnyRate} title="工具用量" dim="tool" />
      {/key}
    {:else if segment === "models"}
      {#key "models-" + refreshTrigger}
        <BreakdownSegment currency={config.currency} {cnyRate} title="模型用量" dim="model" />
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
        <Limits currency={config.currency} {cnyRate} {config} />
      {/key}
    {:else}
      <p class="placeholder">「{segment}」分段 · M4 待实装</p>
    {/if}
  </main>

  <footer class="pop-footer">
    <div class="l">
      {#if updateInfo?.hasUpdate}
        <button
          class="ver-tag update"
          onclick={() => { updateInfo?.url && invoke("open_external", { url: updateInfo.url }).catch(() => {}); }}
          title="新版本 {updateInfo.version} 可用，点击下载"
          aria-label="新版本可用"
        >
          <span class="ver-dot"></span>
          <span class="ver-cur">v{appVersion}</span>
          <span class="ver-arrow">→</span>
          <span class="ver-new">{updateInfo.version}</span>
        </button>
      {:else}
        <span class="ver-tag" title="已是最新版本">
          <span class="ver-dot"></span>
          <span class="ver-cur">v{appVersion}</span>
        </span>
      {/if}
    </div>
    <div class="r">
      {#if refreshStatus === "loading"}
        <span class="refresh-feedback loading">刷新中…</span>
      {:else if refreshStatus === "ok"}
        <span class="refresh-feedback ok">{refreshMsg}</span>
      {:else if refreshStatus === "fail"}
        <span class="refresh-feedback fail">刷新失败</span>
      {/if}
      <button type="button" class="fbtn" onclick={() => refreshData()} disabled={refreshStatus === "loading"} title="刷新" aria-label="刷新">↻</button>
      <button type="button" class="fbtn fbtn-gear" onclick={() => { invoke("open_settings").catch(() => {}); }} title="设置" aria-label="设置">⚙</button>
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
  .ver-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 10.5px;
    color: var(--text-faint);
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
    border-radius: 10px;
    padding: 2px 8px;
    line-height: 1.6;
    cursor: default;
  }
  .ver-tag.update {
    cursor: pointer;
    color: var(--amber);
    border-color: var(--amber);
    background: var(--amber-bg);
    padding-right: 4px;
  }
  .ver-tag.update:hover {
    background: var(--amber-hover);
    color: var(--text);
  }
  .ver-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--lime);
    box-shadow: 0 0 4px var(--lime);
  }
  .ver-tag.update .ver-dot {
    background: var(--amber);
    box-shadow: 0 0 6px var(--amber);
    animation: ver-pulse 1.5s ease-in-out infinite;
  }
  @keyframes ver-pulse {
    0%, 100% { box-shadow: 0 0 4px var(--amber); }
    50% { box-shadow: 0 0 12px var(--amber); }
  }
  .ver-cur { line-height: 1; }
  .ver-arrow { font-size: 9px; margin: 0 2px; opacity: 0.7; }
  .ver-new { font-weight: 600; color: var(--amber); line-height: 1; }
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
    background: var(--glass-subtle);
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
