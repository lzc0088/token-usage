<script lang="ts">
  // Show/hide chips for overview modules. Mirrors wireframe 总览 module-bar.
  import {
    MODULE_LABELS,
    MODULE_ORDER,
    isModuleVisible,
    toggleModule,
    type ModuleKey,
  } from "../../stores/modules.svelte";

  let visibility = $derived(MODULE_ORDER.map((k) => ({ k, on: isModuleVisible(k) })));
</script>

<div class="module-bar">
  <div class="module-toggles">
    {#each visibility as m (m.k)}
      <button
        class="mtog"
        class:on={m.on}
        onclick={() => toggleModule(m.k as ModuleKey)}
      >
        <span class="dot"></span>{MODULE_LABELS[m.k as ModuleKey]}
      </button>
    {/each}
  </div>
</div>

<style>
  .module-bar {
    /* Sits inside ov-body (padding 14px 18px) — only need bottom spacing
       to match wireframe module-bar (margin 13px, dashed divider). */
    padding-bottom: 11px;
    margin-bottom: 13px;
    border-bottom: 1px dashed var(--border-dim);
  }
  .module-toggles {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }
  .mtog {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    color: var(--text-faint);
    padding: 4px 9px;
    border-radius: 6px;
    font-size: 0.7rem;
    font-family: inherit;
    cursor: pointer;
    transition: 0.15s;
  }
  .mtog .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .mtog.on {
    color: var(--amber);
    border-color: var(--amber-soft);
    background: var(--amber-hover);
  }
  .mtog.on .dot {
    background: var(--amber);
  }
</style>
