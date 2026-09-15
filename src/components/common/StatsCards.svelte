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
    /** Lead card only: render the digits with the app-wide amber gradient. */
    lead?: boolean;
    /** Cost card only: (unit, value) pairs rendered unit-first. */
    costParts?: CostPart[];
  }

  const cards = $derived.by((): StatCard[] => {
    const total = splitCompact(summary.total_tokens, locale);

    return [
      // Lead card echoes the hero's amber gradient — "the app's one gradient"
      // brand-cohesion rule (see DeepSeekMonitorWindows).
      { key: "totalTokens", label: t("stats.totalTokens"), ...total, color: "var(--tok-input)", lead: true },
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
      {:else if card.lead}
        <div class="v lead">
          {card.value}<span class="u">{card.unit}</span>
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
    border: none;
    border-radius: 12px;
    padding: 10px 11px;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
  }
  .scell .k {
    font-size: 0.7333rem;
    color: var(--text-faint);
  }
  .scell .v {
    font-size: 1.5rem;
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
  /* Lead card digits: the hero's amber gradient, unit stays neutral. */
  .scell .v.lead {
    background: linear-gradient(160deg, #f9e7bb, #e8b04b 52%, #cd943c);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .scell .v.lead .u {
    background: none;
    -webkit-text-fill-color: var(--text-faint);
  }
  :global([data-theme="light"]) .scell .v.lead {
    background: linear-gradient(160deg, #b57e1c, #9a6a12 55%, #7d5510);
    -webkit-background-clip: text;
    background-clip: text;
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
