<script lang="ts">
  // Hero: period label + total tokens (big) + cost + message count.
  import type { Summary, Currency } from "../../lib/api";
  import { formatCost, formatTokens } from "../../lib/format";

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
</script>

<div class="hero-l">
  <span class="lbl">{label}</span>
  <span class="big">{summary ? formatTokens(summary.total_tokens) : "—"}</span>
  {#if summary}
    <span class="cost"
      >{formatCost(summary.cost_usd, currency, cnyRate)}<span class="sub">
        · {summary.messages} msgs</span></span
    >
  {/if}
</div>

<style>
  .hero-l {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .lbl {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.12em;
    color: var(--text-faint);
    text-transform: uppercase;
  }
  .big {
    font-family: "Fraunces", var(--font-ui);
    font-size: 34px;
    font-weight: 500;
    line-height: 1.05;
    color: var(--text);
  }
  .cost {
    font-size: 12px;
    color: var(--amber);
  }
  .cost .sub {
    color: var(--text-faint);
  }
</style>
