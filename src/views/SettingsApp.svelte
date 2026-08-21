<script lang="ts">
  // Standalone settings window (Tauri window label: "settings"). Separate JS
  // context from the main popover → loads its own config. Layout follows
  // docs/wireframe.html #settings-section: left nav (170px) + right panel.
  import { invoke } from "@tauri-apps/api/core";
  import { api, type Config } from "../lib/api";
  import { applyAppearance, initAppearanceListeners } from "../lib/appearance";
  import { setLang, t } from "../lib/i18n.svelte";
  import General from "../components/settings/General.svelte";
  import MainView from "../components/settings/MainView.svelte";
  import Window from "../components/settings/Window.svelte";
  import Collection from "../components/settings/Collection.svelte";
  import Account from "../components/settings/Account.svelte";

  let cfg = $state<Config>({ currency: "both" });
  let loaded = $state(false);

  // Active settings page — LOCAL $state, set by nav clicks and by the
  // consume-target handler when opened via a quota quick-link.
  let active = $state("general");

  // Sync language to i18n module whenever config changes.
  $effect(() => { setLang(cfg.language ?? "zh"); });

  $effect(() => {
    let cancelled = false;
    api.getConfig()
      .then((c) => {
        if (!cancelled) {
          cfg = c;
          loaded = true;
        }
      })
      .catch(() => { loaded = true; });
    return () => { cancelled = true; };
  });

  function onUpdate(part: Partial<Config>): void {
    cfg = { ...cfg, ...part };
    api.setConfig(cfg).catch(() => {});
  }

  // Apply appearance (theme / animation) live in the settings window too.
  $effect(() => {
    applyAppearance(cfg);
  });
  $effect(() => {
    return initAppearanceListeners();
  });

  // Window dragging: the settings window is background-movable (set once at
  // startup in Rust). Row-drag in Account / Collection / MainView suspends
  // that while the cursor hovers a sortable row (see lib/actions/rowDrag.ts)
  // so OS-level and row-level drags never race; everywhere else the
  // background stays draggable.


  // On focus: reload config (picks up tray-menu changes) and consume the
  // pending landing page (set by open_settings, take semantics) — quota
  // quick-links open settings on a specific page. App-switch focus returns
  // null, so the user's current page is preserved.
  $effect(() => {
    async function onFocus() {
      try {
        const c = await api.getConfig();
        cfg = c;
        applyAppearance(c);
      } catch { /* ignore */ }
      try {
        const target = await invoke<string | null>("consume_settings_target");
        if (target !== null && target !== undefined && target !== "") {
          active = target;
        }
      } catch { /* consume failed — keep current page */ }
    }
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  // Reactive nav labels — language-aware via cfg.language prop read.
  // Icons are inline lucide-style SVGs (stroke matches the app's icon
  // language; the old geometric text glyphs ◯▣▢▤◉ read as placeholders).
  function label(zh: string, en: string): string { return cfg.language === "en" ? en : zh; }
  const NAV_ICONS: Record<string, string> = {
    // sliders (general settings)
    general: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><line x1="4" y1="7" x2="20" y2="7"/><circle cx="9" cy="7" r="2.1"/><line x1="4" y1="12" x2="20" y2="12"/><circle cx="15" cy="12" r="2.1"/><line x1="4" y1="17" x2="20" y2="17"/><circle cx="7" cy="17" r="2.1"/></svg>',
    // panel-left (preview layout)
    mainview: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2.5"/><line x1="9.5" y1="4" x2="9.5" y2="20"/></svg>',
    // monitor (window appearance)
    window: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="12.5" rx="2"/><line x1="8.5" y1="20" x2="15.5" y2="20"/><line x1="12" y1="16.5" x2="12" y2="20"/></svg>',
    // database (collection tracking)
    collection: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="5.5" rx="7.5" ry="2.6"/><path d="M4.5 5.5v12.8c0 1.44 3.36 2.6 7.5 2.6s7.5-1.16 7.5-2.6V5.5"/><path d="M4.5 12c0 1.44 3.36 2.6 7.5 2.6s7.5-1.16 7.5-2.6"/></svg>',
    // user (account & quotas)
    account: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.6"/><path d="M5.2 20c0-3.1 3-5.3 6.8-5.3s6.8 2.2 6.8 5.3"/></svg>',
  };
  let NAV = $derived([
    { k: "general", l: label("基本设置", "General"), i: NAV_ICONS.general },
    { k: "mainview", l: label("预览界面", "Preview"), i: NAV_ICONS.mainview },
    { k: "window", l: label("窗口外观", "Appearance"), i: NAV_ICONS.window },
    { k: "collection", l: label("采集追踪", "Collection"), i: NAV_ICONS.collection },
    { k: "account", l: label("账号额度", "Account"), i: NAV_ICONS.account },
  ]);

  function onNavClick(target: string): void {
    active = target;
  }

  function close(): void {
    invoke("close_settings");
  }
</script>

<div class="setwin">
  <div class="setmain">
    <nav class="setnav">
      {#each NAV as n (n.k)}
        <button class="item" class:active={active === n.k} onclick={() => onNavClick(n.k)}>
          <span class="si">{@html n.i}</span><span class="sl">{n.l}</span>
        </button>
      {/each}
    </nav>

    <main class="setpanel">
      {#if !loaded}
        <p class="loading">{t("common.loading")}</p>
      {:else if active === "general"}
        <General config={cfg} {onUpdate} />
      {:else if active === "mainview"}
        <MainView config={cfg} {onUpdate} />
      {:else if active === "window"}
        <Window config={cfg} {onUpdate} />
      {:else if active === "collection"}
        <Collection config={cfg} {onUpdate} />
      {:else if active === "account"}
        <Account />
      {/if}
    </main>
  </div>

  <!-- floating close button, top-right corner -->
  <button type="button" class="fclose" onclick={close} aria-label={t("settings.close")} title={t("settings.close")}>✕</button>
</div>

<style>
  .setwin {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
    border-radius: var(--radius);
    position: relative;
  }

  .setmain {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .setnav {
    width: 170px;
    flex-shrink: 0;
    padding: 32px 10px 14px;
    border-right: 1px solid var(--border-dim);
    background: var(--sidebar-bg);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    background: transparent;
    border: none;
    border-radius: 7px;
    padding: 12px 12px;
    color: var(--text-dim);
    cursor: pointer;
    font-family: inherit;
    font-size: 14px;
    transition: 0.15s;
    text-align: left;
    -webkit-app-region: no-drag;
  }
  .item:hover {
    background: var(--surface-tint);
    color: var(--text);
  }
  .item.active {
    background: rgba(232, 176, 75, 0.08);
    color: var(--amber);
  }
  .item .si {
    width: 20px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    opacity: 0.9;
  }
  .item .si :global(svg) {
    width: 17px;
    height: 17px;
  }

  .setpanel {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }
  .setpanel::-webkit-scrollbar {
    width: 6px;
  }
  .setpanel::-webkit-scrollbar-thumb {
    background: var(--glass-3);
    border-radius: 3px;
  }
  .loading {
    padding: 32px;
    color: var(--text-faint);
    font-size: 12px;
    text-align: center;
  }

  /* floating close button, top-right */
  .fclose {
    position: absolute;
    top: 14px;
    right: 14px;
    width: 30px;
    height: 30px;
    background: var(--surface-tint-strong);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    color: var(--text-faint);
    font-size: 16px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    transition: 0.15s;
    -webkit-app-region: no-drag;
  }
  .fclose:hover {
    color: var(--amber);
    border-color: var(--amber);
    background: var(--amber-hover);
  }

  /* ensure interactive elements inside the draggable window still work */
  :global(.setpanel button),
  :global(.setpanel select),
  :global(.setpanel input),
  :global(.setpanel .tg) {
    -webkit-app-region: no-drag;
  }

  /* ── matched to docs/wireframe.html set-body-header / set-row ── */
  :global(.setpanel .sh) {
    padding: 14px 30px 16px;
    background: var(--bg);
    border-bottom: 1px solid var(--border-dim);
    position: sticky;
    top: 0;
    z-index: 1;
    width: 100%;
  }
  :global(.setpanel .sh h3) {
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 24px;
    margin-bottom: 2px;
    color: var(--text);
  }
  :global(.setpanel .sh .desc) {
    font-size: 13px;
    color: var(--text-dim);
  }
  :global(.setpanel .group-title) {
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 18px;
    margin-top: 28px;
    margin-bottom: 3px;
    color: var(--text);
  }
  :global(.setpanel .section-box) {
    box-shadow: 0 0 0 1px rgba(255,255,255,0.06), 0 2px 8px rgba(0,0,0,0.5);
  }
  :global(.setpanel .sc) {
    padding: 0 30px 30px;
  }
</style>
