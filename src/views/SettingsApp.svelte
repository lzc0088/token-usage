<script lang="ts">
  // Standalone settings window (Tauri window label: "settings"). Separate JS
  // context from the main popover → loads its own config. Layout follows
  // docs/wireframe.html #settings-section: left nav (170px) + right panel.
  import { invoke } from "@tauri-apps/api/core";
  import { api, type Config } from "../lib/api";
  import { applyAppearance, initAppearanceListeners } from "../lib/appearance";
  import { setLang } from "../lib/i18n.svelte";
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

  // Settings window is intentionally fixed (not draggable) so that row-drag
  // in the Account / Collection pages works reliably without conflicting
  // with OS-level window drag gestures. The main popover drags via
  // MovableByWindowBackground; the settings window doesn't.


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
  function label(zh: string, en: string): string { return cfg.language === "en" ? en : zh; }
  let NAV = $derived([
    { k: "general", l: label("基本设置", "General"), i: "◯" },
    { k: "mainview", l: label("预览界面", "Preview"), i: "▣" },
    { k: "window", l: label("窗口外观", "Appearance"), i: "▢" },
    { k: "collection", l: label("采集追踪", "Collection"), i: "▤" },
    { k: "account", l: label("账号额度", "Account"), i: "◉" },
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
          <span class="si">{n.i}</span><span class="sl">{n.l}</span>
        </button>
      {/each}
    </nav>

    <main class="setpanel">
      {#if !loaded}
        <p class="loading">加载中…</p>
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

  <!-- floating close button, bottom-right corner -->
  <button type="button" class="fclose" onclick={close} aria-label="关闭" title="关闭设置">✕</button>
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
    background: rgba(0, 0, 0, 0.15);
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
    background: rgba(255, 255, 255, 0.03);
    color: var(--text);
  }
  .item.active {
    background: rgba(232, 176, 75, 0.08);
    color: var(--amber);
  }
  .item .si {
    font-size: 16px;
    width: 20px;
    text-align: center;
    opacity: 0.9;
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
    background: rgba(0, 0, 0, 0.25);
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
    background: rgba(0, 0, 0, 0.45);
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
