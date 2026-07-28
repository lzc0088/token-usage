<script lang="ts">
  // Hero: period label + total tokens (big, with small unit) + cost (CNY first) + delta.
  import type { Summary, Currency } from "../../lib/api";
  import { formatCost, splitTokensCN } from "../../lib/format";

  let {
    summary,
    currency,
    cnyRate = 7.2,
  }: { summary: Summary | null; currency: Currency; cnyRate?: number } = $props();

  function periodLabel(p: string): string {
    if (p === "month") return "本月 · MONTH";
    if (p === "total") return "全部 · TOTAL";
    return "今日 · TODAY";
  }

  const label = $derived(summary ? periodLabel(summary.period) : "今日 · TODAY");
  const t = $derived(summary ? splitTokensCN(summary.total_tokens, 3) : { value: "—", unit: "" });
  const deltaDir = $derived(
    summary?.delta_pct != null ? (summary.delta_pct >= 0 ? "↑" : "↓") : "",
  );
</script>

<div class="hero-l">
  <span class="lbl">{label}</span>
  <span class="big">{t.value}<span class="big-unit">{t.unit}</span>
    {#if summary?.delta_pct != null}
      <span class="delta" class:up={summary.delta_pct >= 0} class:down={summary.delta_pct < 0}>
        {deltaDir}{Math.abs(summary.delta_pct).toFixed(0)}<span class="delta-unit">%</span> vs {summary.delta_label?.replace("较", "")}
      </span>
    {/if}
  </span>
  {#if summary}
    <span class="cost">{formatCost(summary.cost_usd, currency, cnyRate)}</span>
  {/if}
</div>

<style>
  .hero-l {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .lbl {
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.12em;
    color: var(--text-dim);
    text-transform: uppercase;
    font-weight: 600;
  }
  .big {
    font-family: "Fraunces", var(--font-ui);
    font-size: 28px;
    font-weight: 600;
    line-height: 1.15;
    color: var(--text);
    display: flex;
    align-items: baseline;
    gap: 3px;
    user-select: text;
    -webkit-user-select: text;
  }
  .big-unit {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    font-family: var(--font-ui);
    user-select: text;
    -webkit-user-select: text;
  }
  .cost {
    font-size: 12px;
    font-weight: 600;
    color: var(--amber);
    white-space: nowrap;
    user-select: text;
    -webkit-user-select: text;
  }
  .delta {
    margin-left: 8px;
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-ui);
  }
  .delta-unit { font-size: 9px; margin-left: 2px; }
  .delta.up { color: var(--lime); }
  .delta.down { color: var(--coral); }
</style>
