<script lang="ts">
  // Standalone settings window (Tauri window label: "settings"). Separate JS
  // context from the main popover → loads its own config. Layout follows
  // docs/wireframe.html #settings-section: left nav (170px) + right panel.
  import { invoke } from "@tauri-apps/api/core";
  import { api, type Config } from "../lib/api";
  import { applyAppearance, initAppearanceListeners } from "../lib/appearance";
  import { getSettingsPartition, setSettingsPartition } from "../stores/settings.svelte";
  import General from "../components/settings/General.svelte";
  import MainView from "../components/settings/MainView.svelte";
  import Window from "../components/settings/Window.svelte";
  import Collection from "../components/settings/Collection.svelte";
  import Account from "../components/settings/Account.svelte";

  let cfg = $state<Config>({ currency: "both" });
  let loaded = $state(false);

  $effect(() => {
    api.getConfig()
      .then((c) => { cfg = c; loaded = true; })
      .catch(() => { loaded = true; });
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

  // Reload config from DB whenever the settings window gains focus —
  // picks up changes made from the tray menu (theme, window mode, etc.)
  // without needing a manual refresh.
  $effect(() => {
    async function onFocus() {
      try {
        const c = await api.getConfig();
        cfg = c;
        applyAppearance(c);
      } catch { /* ignore */ }
    }
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  const NAV = [
    { k: "general", l: "基本设置", i: "◯" },
    { k: "mainview", l: "预览界面", i: "▣" },
    { k: "window", l: "窗口外观", i: "▢" },
    { k: "collection", l: "采集追踪", i: "▤" },
    { k: "account", l: "账号额度", i: "◉" },
  ];
  let active = $derived(getSettingsPartition());

  function close(): void {
    invoke("close_settings");
  }
</script>

<div class="setwin">
  <div class="setmain">
    <nav class="setnav">
      {#each NAV as n (n.k)}
        <button class="item" class:active={active === n.k} onclick={() => setSettingsPartition(n.k)}>
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
  <button class="fclose" onclick={close} aria-label="关闭" title="关闭设置">✕</button>
</div>

<style>
  .setwin {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
    border-radius: var(--radius);
    position: relative;
    -webkit-app-region: drag; /* make the whole window draggable */
    cursor: grab;
  }

  .setmain {
    flex: 1;
    display: flex;
    min-height: 0;
    -webkit-app-region: drag;
    cursor: grab;
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
    -webkit-app-region: drag;
    cursor: grab;
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
