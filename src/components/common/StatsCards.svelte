<script lang="ts">
  // Summary stat cards — total tokens, cost, active days, messages.
  // Mirrors the Overview IO cells (split2 / scell): small faint label on top,
  // large value below, each card in its own semantic color.

  import type { Summary, TrendPoint, Currency } from "../../lib/api";
  import type { Locale } from "../../lib/format";
  import { splitCost, type CostPart } from "../../lib/format";
  import { t } from "../../lib/i18n.svelte";

  let {
    summary,
    trends,
    currency = "both",
    cnyRate = 7.2,
    locale = "en",
  }: {
    summary: Summary;
    trends: TrendPoint[];
    currency?: Currency;
    cnyRate?: number;
    locale?: Locale;
  } = $props();

  // ── Derived stats ───────────────────────────────────────────────────────

  // Prefer the backend's authoritative count (correct for monthly-aggregated
  // total period); fall back to counting non-empty trend points.
  const activeDays = $derived(summary.active_days ?? trends.filter((p) => p.tokens > 0).length);

  interface StatCard {
    key: string;
    label: string;
    value: string;
    unit: string;
    color: string;
    /** Cost card only: (unit, value) pairs rendered unit-first. */
    costParts?: CostPart[];
  }

  const cards = $derived.by((): StatCard[] => {
    const total = splitCompact(summary.total_tokens, locale);

    return [
      { key: "totalTokens", label: t("stats.totalTokens"), ...total, color: "var(--tok-input)" },
      { key: "totalCost", label: t("stats.totalCost"), value: "", unit: "", costParts: splitCost(summary.cost_usd, currency, cnyRate), color: "var(--tok-output)" },
      { key: "activeDays", label: t("stats.activeDays"), value: String(activeDays), unit: t("stats.days"), color: "var(--tok-cache-r)" },
      { key: "messages", label: t("stats.messageCount"), value: String(summary.messages), unit: t("stats.messages"), color: "var(--tok-cache-w)" },
    ];
  });

  function splitCompact(value: number, locale: Locale): { value: string; unit: string } {
    if (!Number.isFinite(value)) return { value: "0", unit: "" };
    const abs = Math.abs(value);

    if (locale === "zh") {
      if (abs >= 1_0000_0000) return { value: trim(value / 1_0000_0000), unit: "亿" };
      if (abs >= 1_0000) return { value: trim(value / 1_0000), unit: "万" };
      return { value: String(Math.round(abs)), unit: "" };
    }

    // Western
    if (abs >= 1_000_000_000) return { value: trim(value / 1_000_000_000), unit: "B" };
    if (abs >= 1_000_000) return { value: trim(value / 1_000), unit: "K" };
    if (abs >= 1_000) return { value: trim(value / 1_000), unit: "" };
    return { value: String(Math.round(abs)), unit: "" };
  }

  function trim(v: number, decimals = 1): string {
    return Number(v.toFixed(decimals)).toString();
  }
</script>

<div class="stats-grid">
  {#each cards as card (card.key)}
    <div class="scell">
      <div class="k">{card.label}</div>
      {#if card.costParts}
        <!-- Cost: small currency unit LEFT of the amount (both → ¥…/$…). -->
        <div class="v cost-v" style="color: {card.color}">
          {#each card.costParts as part, i (i)}
            {part.sep ?? ""}<span class="cu">{part.unit}</span><span class="cost-val">{part.value}</span>
          {/each}
        </div>
      {:else}
        <div class="v" style="color: {card.color}">
          {card.value}<span class="u">{card.unit}</span>
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  /* Mirrors Overview.svelte .split2 / .scell */
  .stats-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .scell {
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    border-radius: 9px;
    padding: 10px 11px;
  }
  .scell .k {
    font-size: 0.7333rem;
    color: var(--text-faint);
  }
  .scell .v {
    font-size: 1.333rem;
    font-weight: 500;
    margin-top: 2px;
    display: flex;
    align-items: baseline;
    gap: 0;
  }
  .scell .v .u {
    font-size: 0.7333rem;
    color: var(--text-faint);
    font-weight: 600;
  }
  /* Cost card: small currency unit immediately LEFT of the amount. */
  .cost-v {
    gap: 0;
  }
  .cost-val {
    font-size: 1.333rem;
    font-weight: 500;
  }
  .cu {
    font-size: 0.55rem;
    font-weight: 700;
    opacity: 0.85;
  }
</style>
