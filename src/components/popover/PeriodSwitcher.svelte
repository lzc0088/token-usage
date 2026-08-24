<script lang="ts">
  // DAY / MONTH / TOTAL switcher. Writes the global period store; the popover
  // re-queries everything when it changes.
  import { getPeriod, setPeriod } from "../../stores/period.svelte";

  let { lang = "zh" }: { lang?: string } = $props();

  let current = $derived(getPeriod());
</script>

<div class="period" data-testid="period-switcher">
  <span class="period-lbl">{lang === "en" ? "Period" : "时段"}</span>
  <div class="seg">
    <button data-testid="period-day" class:active={current === "day"} onclick={() => setPeriod("day")}>{lang === "en" ? "Today" : "今日"}</button>
    <button data-testid="period-month" class:active={current === "month"} onclick={() => setPeriod("month")}>{lang === "en" ? "Month" : "本月"}</button>
    <button data-testid="period-total" class:active={current === "total"} onclick={() => setPeriod("total")}>{lang === "en" ? "Total" : "累计"}</button>
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
    font-size: 0.7333rem;
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
    font-size: 0.7333rem;
    font-weight: 600;
    padding: 6px 10px;
    border-radius: 6px;
    cursor: pointer;
    transition: 0.15s;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .seg button:hover { color: var(--text); }
  .seg button.active {
    background: var(--amber);
    color: var(--badge-text);
    font-weight: 700;
  }
</style>
