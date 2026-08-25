<script lang="ts">
  // Hero: period label + total tokens (big, with small unit) + cost (CNY first) + delta
  //       + live token-rate readout (click to toggle speed / burn).
  import type { Summary, Currency } from "../../lib/api";
  import { formatTokenRate, splitTokens, splitTokensCN } from "../../lib/format";
  import { t } from "../../lib/i18n.svelte";
  import CostText from "../common/CostText.svelte";

  let {
    summary,
    currency,
    cnyRate = 7.2,
    lang = "zh",
    rateMode = "speed",
    onToggleRateMode,
  }: {
    summary: Summary | null;
    currency: Currency;
    cnyRate?: number;
    lang?: string;
    rateMode?: "speed" | "burn";
    onToggleRateMode?: () => void;
  } = $props();

  const tokenDisplay = $derived(summary
    ? (lang === "en" ? splitTokens(summary.total_tokens, 2) : splitTokensCN(summary.total_tokens, 3))
    : { value: "—", unit: "" });
  const deltaDir = $derived(
    summary?.delta_pct != null ? (summary.delta_pct >= 0 ? "↑" : "↓") : "",
  );
  // Live throughput readout. Empty when there's no model-busy duration in the
  // window (e.g. month/total periods, or a quiet today) — see formatTokenRate.
  const rateText = $derived(
    summary
      ? formatTokenRate(rateMode, summary.timed_output_tokens, summary.timed_tokens, summary.timed_duration_ms)
      : "",
  );

  function deltaText(raw: string | null | undefined): string {
    if (!raw) return "";
    const cleaned = raw.replace("较", "");
    if (lang === "en") {
      if (cleaned === "昨日") return "yesterday";
      if (cleaned === "上月") return "last month";
    }
    return cleaned;
  }
</script>

<div class="hero-l" data-testid="hero-section">
  <span class="big">{tokenDisplay.value}<span class="big-unit">{tokenDisplay.unit}</span>
    {#if summary?.delta_pct != null}
      <span class="delta" class:up={summary.delta_pct >= 0} class:down={summary.delta_pct < 0}>
        {deltaDir}{Math.abs(summary.delta_pct).toFixed(0)}<span class="delta-unit">%</span> vs {deltaText(summary.delta_label)}
      </span>
    {/if}
  </span>
  <div class="subline">
    {#if summary}
      <span class="cost"><CostText usd={summary.cost_usd} {currency} {cnyRate} /></span>
    {/if}
    {#if rateText}
      <button
        type="button"
        class="rate"
        title={t("hero.rateToggleHint")}
        aria-label={t("hero.rateToggleHint")}
        onclick={(e) => { e.stopPropagation(); onToggleRateMode?.(); }}
      >⚡ {rateText}</button>
    {/if}
  </div>
</div>

<style>
  .hero-l {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .big {
    font-family: "Fraunces", var(--font-ui);
    font-size: 1.867rem;
    font-weight: 600;
    line-height: 1.15;
    color: var(--text);
    display: flex;
    align-items: baseline;
    gap: 0;
    user-select: text;
    -webkit-user-select: text;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .big-unit {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-dim);
    font-family: var(--font-ui);
    user-select: text;
    -webkit-user-select: text;
  }
  .cost {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--amber);
    white-space: nowrap;
    user-select: text;
    -webkit-user-select: text;
  }
  .subline {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .rate {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: 0.7333rem;
    font-weight: 600;
    font-family: var(--font-ui);
    color: var(--text-dim);
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    border-radius: 5px;
    padding: 1px 6px;
    cursor: pointer;
    transition: all 0.15s;
    -webkit-app-region: no-drag;
  }
  .rate:hover { color: var(--amber); border-color: var(--amber); background: var(--amber-hover); }
  .delta {
    margin-left: 8px;
    font-size: 0.8rem;
    font-weight: 600;
    font-family: var(--font-ui);
  }
  .delta-unit { font-size: 0.6rem; margin-left: 2px; }
  .delta.up { color: var(--lime); }
  .delta.down { color: var(--coral); }
</style>
