<script lang="ts">
  // Summary stat cards — total tokens, cost, active days, streak, etc.
  // Displays a grid of key metrics with compact formatting.

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

  const currentStreak = $derived.by(() => {
    if (trends.length === 0) return 0;
    const sorted = [...trends].sort((a, b) => b.date.localeCompare(a.date));
    let streak = 0;
    for (const p of sorted) {
      if (p.tokens > 0) {
        streak++;
      } else {
        break;
      }
    }
    return streak;
  });

  const peakDay = $derived.by(() => {
    if (trends.length === 0) return null;
    return trends.reduce((max, p) => (p.tokens > max.tokens ? p : max), trends[0]);
  });

  interface StatCard {
    key: string;
    value: string;
    unit: string;
    icon: string;
  }

  const cards = $derived.by((): StatCard[] => {
    const total = splitCompact(summary.total_tokens, locale);
    const cost = { value: summary.cost_usd.toFixed(2), unit: "USD" };
    const peak = peakDay ? splitCompact(peakDay.tokens, locale) : { value: "0", unit: "" };

    return [
      { key: "totalTokens", ...total, icon: "⚡" },
      { key: "totalCost", ...cost, icon: "💰" },
      { key: "activeDays", value: String(activeDays), unit: t("stats.days"), icon: "📅" },
      { key: "streak", value: String(currentStreak), unit: t("stats.days"), icon: "🔥" },
      { key: "peakDay", ...peak, icon: "📈" },
      { key: "messages", value: String(summary.messages), unit: t("stats.messages"), icon: "💬" },
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
    if (abs >= 1_000_000) return { value: trim(value / 1_000_000), unit: "M" };
    if (abs >= 1_000) return { value: trim(value / 1_000), unit: "K" };
    return { value: String(Math.round(abs)), unit: "" };
  }

  function trim(v: number, decimals = 1): string {
    return Number(v.toFixed(decimals)).toString();
  }
</script>

<div class="stats-grid">
  {#each cards as card (card.key)}
    <div class="stat-card">
      <div class="stat-icon">{card.icon}</div>
      <div class="stat-value">
        {card.value}
        {#if card.unit}
          <span class="stat-unit">{card.unit}</span>
        {/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .stat-card {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: var(--glass-subtle);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    transition: all 0.15s;
  }

  .stat-card:hover {
    background: var(--glass-subtle-strong);
    border-color: var(--border);
  }

  .stat-icon {
    font-size: 16px;
    flex-shrink: 0;
  }

  .stat-value {
    font-family: var(--font-mono);
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
    display: flex;
    align-items: baseline;
    gap: 4px;
  }

  .stat-unit {
    font-size: 11px;
    font-weight: 400;
    color: var(--text-faint);
  }
</style>
