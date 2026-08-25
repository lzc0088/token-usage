<script lang="ts">
  // Summary stat cards — total tokens, cost, active days, messages.
  // Mirrors the Overview IO cells (split2 / scell): small faint label on top,
  // large value below, each card in its own semantic color.

  import type { Summary, TrendPoint } from "../../lib/api";
  import type { Locale } from "../../lib/format";
  import { t } from "../../lib/i18n.svelte";

  let {
    summary,
    trends,
    locale = "en",
  }: {
    summary: Summary;
    trends: TrendPoint[];
    locale?: Locale;
  } = $props();

  // ── Derived stats ───────────────────────────────────────────────────────

  const activeDays = $derived(trends.filter((p) => p.tokens > 0).length);

  interface StatCard {
    key: string;
    label: string;
    value: string;
    unit: string;
    color: string;
  }

  const cards = $derived.by((): StatCard[] => {
    const total = splitCompact(summary.total_tokens, locale);
    const cost = { value: summary.cost_usd.toFixed(2), unit: "USD" };

    return [
      { key: "totalTokens", label: t("stats.totalTokens"), ...total, color: "var(--tok-input)" },
      { key: "totalCost", label: t("stats.totalCost"), ...cost, color: "var(--tok-output)" },
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
      <div class="v" style="color: {card.color}">
        {card.value}<span class="u">{card.unit}</span>
      </div>
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
    gap: 2px;
  }
  .scell .v .u {
    font-size: 0.7333rem;
    color: var(--text-faint);
    font-weight: 600;
  }
</style>
