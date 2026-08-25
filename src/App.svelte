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
  import { startWindowResize } from "./lib/resize";
  import { TODAY_UPDATED, CONFIG_CHANGED, RATE_UPDATED, TRAY_REFRESH, COLLECTION_ERROR, COLLECTION_UPDATED } from "./lib/events";
  import { setLang, t } from "./lib/i18n.svelte";
  import { periodValue } from "./stores/period.svelte";
  import { segmentValue } from "./stores/segment.svelte";

  const appWindow = getCurrentWindow();

  let summary = $state<Summary | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadError = $state<string | null>(null);
  // USD→CNY rate for cost conversion. Loaded from the latest stored value
  // (auto or manual) and refreshed on rate:updated / config:changed.
  let cnyRate = $state<number>(7.2);

  // ── Version & update check ──
  let appVersion = $state("1.0.0");
  let updateInfo = $state<{ hasUpdate: boolean; url: string; version: string } | null>(null);
  // Transient update-check failure with no cached result to fall back on
  // (e.g. first-run network blip, or GitHub rate-limit). The Rust backend
  // returns the last-known-good update when cache exists; we only surface an
  // error here when there's truly nothing to show.
  let updateError = $state<{ kind: string; msg: string } | null>(null);
  let collectionError = $state<string | null>(null);
  async function checkForUpdate(): Promise<void> {
    try {
      const ver = await api.getAppVersion();
      appVersion = ver;
      const repo = "lzc0088/token-usage";
      const info = await api.checkUpdate(repo, ver);
      // Only show update badge when versions actually differ.
      // The backend may return has_update=true due to tag format differences
      // (e.g., "v1.0.5" vs "1.0.5"), so we double-check here.
      const versionsDiffer = info.has_update && info.version.replace(/^v/, '') !== ver;
      if (versionsDiffer) {
        updateInfo = { hasUpdate: true, url: info.url, version: info.version };
        updateError = null;
      } else if (info.error_kind) {
        // No cache + transient failure (rate-limit, network, etc.) — surface
        // a dim hint so the user knows the badge is missing for a reason.
        updateError = { kind: info.error_kind, msg: info.error };
      } else {
        // Clean "no update available".
        updateInfo = null;
        updateError = null;
      }
    } catch (e) {
      // invoke-level failure (very rare — backend command itself errored).
      const msg = e instanceof Error ? e.message : String(e);
      updateError = { kind: "network", msg };
    }
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

  // Load config immediately on mount — ensures language is correct before
  // first render of segments. The focus-based reload below picks up later
  // changes (tray menu, settings window) while the popover is hidden.
  $effect(() => {
    let cancelled = false;
    api.getConfig()
      .then((c) => {
        if (!cancelled) {
          config = c;
        }
      })
      .catch((e) => { api.feLog(`init config load failed: ${e instanceof Error ? e.message : String(e)}`); });
    return () => { cancelled = true; };
  });

  // Reload config on focus — picks up changes made from the tray menu
  // (theme, window display mode) without needing a config:changed event.
  // Short debounce to prevent redundant IPC on rapid focus/blur.
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
        } catch (e) { api.feLog(`focus config reload failed: ${e instanceof Error ? e.message : String(e)}`); }
        // Re-fetch the summary so the popover matches the tray on every show.
        // get_summary("day") reads the live `last_today` cache the tray just
        // wrote; without this the popover only updates via the today:updated
        // event, which can lag the tray if events were missed while hidden.
        try {
          const s = await api.getSummary(periodValue());
          summary = s;
        } catch (e) { api.feLog(`focus summary reload failed: ${e instanceof Error ? e.message : String(e)}`); }
        const now = Date.now();
        if (now - lastCheckMs > 300_000) {
          lastCheckMs = now;
          void checkForUpdate();
        }
      }, 80);
    }
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      if (focusTimer) clearTimeout(focusTimer);
    };
  });

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
    const p = periodValue();
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
          loadError = t("common.loadFailed");
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
            // Successful scan → clear any prior collection error.
            if (collectionError) collectionError = null;
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

  // Collection ingest / scan errors — show a non-blocking warning chip.
  $effect(() => {
    const unlisten_promise = listen<string>(COLLECTION_ERROR, (e) => {
      collectionError = e.payload;
    });
    return () => {
      unlisten_promise.then((un) => un());
    };
  });

  // Successful collection update → clear any prior error chip.
  $effect(() => {
    const unlisten_promise = listen<void>(COLLECTION_UPDATED, () => {
      if (collectionError) collectionError = null;
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

  // Sync language to i18n module whenever config changes.
  $effect(() => { setLang(config.language ?? "zh"); });

  // Apply appearance (theme / animation) whenever config changes, and
  // listen to system media queries for "system" mode.
  $effect(() => {
    applyAppearance(config);
  });
  $effect(() => {
    return initAppearanceListeners();
  });

  // ── Window drag policy ──
  // The main window is movable ONLY via the header's data-tauri-drag-region.
  // Native background dragging (MovableByWindowBackground) must stay OFF at
  // all times — it would make edges/content draggable and fight the JS
  // resize handles. Text selection safety is inherent: drag.js ignores
  // mousedowns on INPUT/TEXTAREA/SELECT/clickable elements.

  let segment = $derived(segmentValue());

  /** Toggle the Hero token-rate readout between "speed" (tok/s) and "burn"
   *  (tok/min), persisting the choice to config. */
  function toggleRateMode(): void {
    const next = (config.token_rate_mode === "burn" ? "speed" : "burn");
    config = { ...config, token_rate_mode: next };
    api.setConfig(config).catch(() => { /* non-critical: in-memory toggle still applies */ });
  }

  /** Map the Rust update-check error_kind to a localized i18n key. */
  function updateErrorTitleKey(kind: string): string {
    switch (kind) {
      case "rate_limited": return "hero.updateRateLimited";
      case "network":      return "hero.updateNetwork";
      case "api_error":    return "hero.updateApiError";
      case "parse":        return "hero.updateParseError";
      default:             return "hero.updateFailed";
    }
  }

  // Global refresh feedback: shown to the left of the refresh button.
  let refreshStatus = $state<"idle" | "loading" | "ok" | "fail">("idle");
  let refreshMsg = $state("");
  let refreshGen = 0;

  async function refreshData() {
    refreshStatus = "loading";
    refreshMsg = "";
    const gen = ++refreshGen;
    try {
      // Refresh usage data + quota data + trigger an immediate collector scan.
      const [s, c] = await Promise.all([
        api.getSummary(periodValue()),
        api.getConfig(),
        api.refreshQuotas(),
        api.collectNow(),
      ]);
      if (gen !== refreshGen) return; // stale — a newer refresh was triggered
      summary = s;
      config = c;
      loadError = null;
      refreshStatus = "ok";
      refreshMsg = t("hero.refreshOk");
      const _rid = window.setTimeout(() => { refreshStatus = "idle"; refreshMsg = ""; }, 3000);
      refreshTimeouts.add(_rid);
    } catch (e) {
      if (gen !== refreshGen) return;
      /* refresh failed — shown in refreshMsg */
      refreshStatus = "fail";
      refreshMsg = t("hero.refreshFail");
    }
  }
</script>

<div class="popover" data-testid="popover">
  <!-- Resize handles: invisible strips at each edge + corners.
       JS-driven resize (lib/resize.ts) — setSize only changes the window's
       size, never its origin, so resizing never moves the window. Inert in
       "fixed" display mode (window size is locked). -->
  {#if config.window_display_mode !== "fixed"}
    <!-- svelte-ignore a11y_no_static_element_interactions (mouse-only resize strips) -->
    <div class="resize-handle resize-n" onpointerdown={(e) => startWindowResize(e, "n")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-s" onpointerdown={(e) => startWindowResize(e, "s")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-e" onpointerdown={(e) => startWindowResize(e, "e")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-w" onpointerdown={(e) => startWindowResize(e, "w")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-ne" onpointerdown={(e) => startWindowResize(e, "ne")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-nw" onpointerdown={(e) => startWindowResize(e, "nw")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-se" onpointerdown={(e) => startWindowResize(e, "se")}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="resize-handle resize-sw" onpointerdown={(e) => startWindowResize(e, "sw")}></div>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <header
    class="pop-hero"
    data-tauri-drag-region
    onpointerdown={() => invoke("set_main_interacting", { interacting: true })}
    onpointerup={() => invoke("set_main_interacting", { interacting: false })}
  >
    <Hero
      {summary}
      currency={config.currency}
      {cnyRate}
      lang={config.language}
      rateMode={config.token_rate_mode ?? "speed"}
      onToggleRateMode={toggleRateMode}
    />
    <div class="hero-right">
      <PeriodSwitcher lang={config.language} />
    </div>
  </header>

  <SegBar {config} />

  <main class="seg-scroll">
    {#if loadError}
      <p class="err" data-testid="load-error">{loadError}</p>
    {:else if segment === "ov"}
      <Overview {summary} currency={config.currency} config={config} {cnyRate} />
    {:else if segment === "tools"}
      <BreakdownSegment currency={config.currency} {cnyRate} title={t("breakdown.tools")} dim="tool" />
    {:else if segment === "models"}
      <BreakdownSegment currency={config.currency} {cnyRate} title={t("breakdown.models")} dim="model" />
    {:else if segment === "projects"}
      <Projects currency={config.currency} {cnyRate} />
    {:else if segment === "sess"}
      <Sessions currency={config.currency} {cnyRate} />
    {:else if segment === "trend"}
      <Trend />
    {:else if segment === "limit"}
      <Limits currency={config.currency} {cnyRate} {config} />
    {:else}
      <p class="placeholder">「{segment}」分段 · M4 待实装</p>
    {/if}
  </main>

  <footer class="pop-footer">
    <div class="l">
      {#if updateInfo?.hasUpdate}
        <button
          class="ver-tag update"
          onclick={() => { invoke("open_settings").catch(() => {}); }}
          title="新版本 {updateInfo.version} 可用，点击打开更新页"
          aria-label="新版本可用"
        >
          <span class="ver-dot"></span>
          <span class="ver-cur">v{appVersion}</span>
          <span class="ver-arrow">→</span>
          <span class="ver-new">{updateInfo.version}</span>
        </button>
      {:else}
        <span class="ver-tag" title={updateError ? t(updateErrorTitleKey(updateError.kind)) : t("hero.upToDate")}>
          <span class="ver-dot"></span>
          <span class="ver-cur">v{appVersion}</span>
        </span>
        {#if updateError}
          <span class="ver-warn" title={updateError.msg} aria-label={updateError.msg}>
            {t(updateErrorTitleKey(updateError.kind))}
          </span>
        {/if}
        {#if collectionError}
          <span class="ver-warn coll-warn" title={collectionError} aria-label={collectionError}>
            {t("hero.collectionWarn")}
          </span>
        {/if}
      {/if}
    </div>
    <div class="r">
      {#if refreshStatus === "ok"}
        <span class="refresh-feedback ok">{refreshMsg}</span>
      {:else if refreshStatus === "fail"}
        <span class="refresh-feedback fail">{t("hero.refreshFail")}</span>
      {/if}
      <button type="button" class="fbtn {refreshStatus === 'loading' ? 'fbtn-spin' : ''}" onclick={() => refreshData()} disabled={refreshStatus === "loading"} title="刷新" aria-label="刷新">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <polyline points="21 3 21 9 15 9" />
          <path d="M20.5 15a8.5 8.5 0 1 1-2-8.8L21 9" />
        </svg>
      </button>
      <button type="button" class="fbtn fbtn-gear" onclick={() => { invoke("open_settings").catch(() => {}); }} title="设置" aria-label="设置">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
          <circle cx="12" cy="12" r="3"/>
        </svg>
      </button>
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
    position: relative;
  }
  .pop-hero {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 14px 18px 13px;
    gap: 8px;
    flex-shrink: 0;
    position: relative;
    /* Drag region: prevent text selection so pointer drags move window. */
    user-select: none;
    -webkit-user-select: none;
  }
  /* Ensure interactive elements inside the drag region are clickable
     (override Tauri's injected -webkit-app-region: drag). */
  .pop-hero :global(button),
  .pop-hero :global(a),
  .pop-hero :global(input),
  .pop-hero :global(textarea),
  .pop-hero :global(select) {
    -webkit-app-region: no-drag;
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
    background: var(--sidebar-bg);
    flex-shrink: 0;
  }
  .pop-footer .l {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 0.7333rem;
    color: var(--text-faint);
  }
  .ver-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 0.7rem;
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
  .ver-arrow { font-size: 0.6rem; margin: 0 2px; opacity: 0.7; }
  .ver-new { font-weight: 600; color: var(--amber); line-height: 1; }
  .ver-warn {
    font-size: 0.6333rem;
    font-weight: 500;
    color: var(--coral);
    background: rgba(234,84,85,0.10);
    border: 1px solid var(--coral-border);
    border-radius: 4px;
    padding: 1px 5px;
    margin-left: 4px;
    line-height: 1.4;
    max-width: 80px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .coll-warn {
    color: var(--amber);
    background: rgba(232,176,75,0.10);
    border-color: rgba(232,176,75,0.25);
    max-width: 100px;
  }
  .pop-footer .r {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .refresh-feedback { font-size: 0.7rem; line-height: 1; }
  .refresh-feedback.ok { color: var(--lime); }
  .refresh-feedback.fail { color: var(--coral); max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fbtn {
    background: var(--glass-subtle);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 1.067rem;
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
  .fbtn svg { width: 14px; height: 14px; display: block; }
  .fbtn-gear svg { width: 15px; height: 15px; }
  .fbtn:hover {
    color: var(--amber);
    border-color: var(--amber-soft);
  }

  @keyframes spin-icon {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
  .fbtn-spin {
    color: var(--amber);
    pointer-events: none;
  }
  .fbtn-spin svg {
    animation: spin-icon 0.8s linear infinite;
  }

  /* ── Invisible resize handles at window edges ──────────────────────
     JS-driven resize hotspots (pointerdown → lib/resize.ts). They sit
     above the header drag-region (z-index) so edge grabs resize instead
     of dragging the window. */
  .resize-handle {
    position: absolute;
    z-index: 50;
    pointer-events: auto;
    touch-action: none;
  }
  .resize-n  { top: 0; left: 0; right: 0; height: 6px;  cursor: n-resize; }
  .resize-s  { bottom: 0; left: 0; right: 0; height: 6px;  cursor: s-resize; }
  .resize-e  { top: 0; right: 0; bottom: 0; width: 6px;  cursor: e-resize; }
  .resize-w  { top: 0; left: 0; bottom: 0; width: 6px;  cursor: w-resize; }
  .resize-ne { top: 0; right: 0; width: 14px; height: 14px; cursor: ne-resize; }
  .resize-nw { top: 0; left: 0; width: 14px; height: 14px; cursor: nw-resize; }
  .resize-se { bottom: 0; right: 0; width: 14px; height: 14px; cursor: se-resize; }
  .resize-sw { bottom: 0; left: 0; width: 14px; height: 14px; cursor: sw-resize; }
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
    font-size: 0.8rem;
  }
  .placeholder {
    margin: 24px 16px;
    color: var(--text-faint);
    font-size: 0.8rem;
  }

</style>
