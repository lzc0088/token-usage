<script lang="ts">
  // DAY / MONTH / TOTAL switcher. Writes the global period store; the popover
  // re-queries everything when it changes.
  import { getPeriod, setPeriod } from "../../stores/period.svelte";
  import type { Period } from "../../lib/api";

  const options: { key: Period; label: string }[] = [
    { key: "day", label: "DAY" },
    { key: "month", label: "MONTH" },
    { key: "total", label: "TOTAL" },
  ];

  let current = $derived(getPeriod());
</script>

<div class="period">
  <span class="period-lbl">时段</span>
  <div class="seg">
    {#each options as o (o.key)}
      <button class:active={current === o.key} onclick={() => setPeriod(o.key)}>{o.label}</button>
    {/each}
  </div>
</div>

<style>
  .period {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 5px;
  }
  .period-lbl {
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.12em;
    color: var(--text-dim);
    text-transform: uppercase;
    font-weight: 600;
  }
  .seg {
    display: inline-flex;
    gap: 2px;
    background: var(--glass-3);
    border-radius: 8px;
    padding: 2px;
  }
  .seg button {
    background: transparent;
    border: none;
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    padding: 4px 9px;
    border-radius: 6px;
    cursor: pointer;
    transition: 0.15s;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .seg button:hover {
    color: var(--text);
  }
  .seg button.active {
    background: var(--amber);
    color: #1a1408;
    font-weight: 700;
  }
</style>
