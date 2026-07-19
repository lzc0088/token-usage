<script lang="ts">
  import { closeSettings, getSettingsPartition, setSettingsPartition } from "../stores/settings.svelte";
  import { api, type Config } from "../lib/api";
  import General from "../components/settings/General.svelte";
  import MainView from "../components/settings/MainView.svelte";
  import Window from "../components/settings/Window.svelte";
  import Appearance from "../components/settings/Appearance.svelte";
  import Collection from "../components/settings/Collection.svelte";
  import Account from "../components/settings/Account.svelte";

  let cfg = $state<Config>({ currency: "both" });
  let loaded = $state(false);

  // Load config on mount.
  $effect(() => {
    api.getConfig().then((c) => { cfg = c; loaded = true; }).catch(() => { loaded = true; });
  });

  function onUpdate(part: Partial<Config>): void {
    cfg = { ...cfg, ...part };
    api.setConfig(cfg).catch(() => {});
  }

  const NAV = [
    { k: "general",    l: "常规", i: "⚙" },
    { k: "mainview",   l: "主画面", i: "▣" },
    { k: "window",     l: "窗口", i: "▢" },
    { k: "appearance", l: "外观", i: "◐" },
    { k: "collection", l: "采集", i: "▤" },
    { k: "account",    l: "账号", i: "◉" },
  ];
  let active = $derived(getSettingsPartition());
</script>

<div class="set-overlay">
  <div class="set-modal">
    <nav class="set-nav">
      {#each NAV as n (n.k)}
        <button class="ni" class:active={active === n.k} onclick={() => setSettingsPartition(n.k)} title={n.l}>
          <span class="si">{n.i}</span><span class="sl">{n.l}</span>
        </button>
      {/each}
    </nav>
    <div class="set-body">
      {#if !loaded}
        <p class="loading">加载中…</p>
      {:else if active === "general"}
        <General config={cfg} {onUpdate} />
      {:else if active === "mainview"}
        <MainView config={cfg} {onUpdate} />
      {:else if active === "window"}
        <Window />
      {:else if active === "appearance"}
        <Appearance config={cfg} {onUpdate} />
      {:else if active === "collection"}
        <Collection />
      {:else if active === "account"}
        <Account />
      {/if}
    </div>
  </div>
  <button class="backdrop" onclick={closeSettings} aria-label="关闭设置"></button>
</div>

<style>
  .set-overlay { position: fixed; inset: 0; z-index: 100; display: flex; }
  .backdrop { position: absolute; inset: 0; background: rgba(0,0,0,0.5); border: none; cursor: pointer; }
  .set-modal { position: relative; flex: 1; display: flex; background: var(--bg); border-radius: var(--radius); overflow: hidden; margin: 10px; }
  .set-nav { width: 74px; flex-shrink: 0; border-right: 1px solid var(--border-dim); padding: 8px 4px; background: rgba(0,0,0,0.15); display: flex; flex-direction: column; gap: 1px; }
  .ni { background: transparent; border: none; border-radius: 7px; padding: 7px 4px; color: var(--text-faint); font-family: inherit; font-size: 10px; cursor: pointer; display: flex; flex-direction: column; align-items: center; gap: 1px; transition: .15s; }
  .ni:hover { color: var(--text-dim); }
  .ni.active { color: var(--amber); background: rgba(232,176,75,0.08); }
  .ni .si { font-size: 15px; }
  .ni .sl { font-size: 9px; }
  .set-body { flex: 1; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; }
  .set-body::-webkit-scrollbar { width: 6px; }
  .set-body::-webkit-scrollbar-thumb { background: var(--glass-3); border-radius: 3px; }
  .loading { padding: 24px; color: var(--text-faint); font-size: 12px; text-align: center; }
</style>
