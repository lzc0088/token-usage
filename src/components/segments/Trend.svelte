<script lang="ts">
  // 趋势 segment (T4.4). Line chart with an average line and per-node hover.
  // DAY = last 7 days, MONTH = current month, TOTAL = all history by month.
  import { listen } from "@tauri-apps/api/event";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import Heatmap from "../common/Heatmap.svelte";
  import StatsCards from "../common/StatsCards.svelte";
  import { api, type Summary, type Trends } from "../../lib/api";
  import { COLLECTION_UPDATED } from "../../lib/events";
  import { t } from "../../lib/i18n.svelte";
  import { formatTokens, splitTokens } from "../../lib/format";
  import { periodValue } from "../../stores/period.svelte";

  let data = $state<Trends | null>(null);
  let summary = $state<Summary | null>(null);
  let loadAttempted = $state(false);
  let hoverIdx = $state<number | null>(null);
  let chartW = $state(320);

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    const fetch = async () => {
      try {
        const [t, s] = await Promise.all([api.getTrends(p), api.getSummary(p)]);
        if (!cancelled) { data = t; summary = s; loadAttempted = true; }
      } catch {
        if (!cancelled) loadAttempted = true;
      }
    };
    fetch();
    const un = listen<void>(COLLECTION_UPDATED, () => { fetch(); });
    return () => {
      cancelled = true;
      un.then(fn => fn());
    };
  });

  const points = $derived(data?.points ?? []);
  const period = $derived(periodValue());
  const maxTokens = $derived(Math.max(1, ...points.map((p) => p.tokens)));
  const totalTokens = $derived(points.reduce((s, p) => s + p.tokens, 0));
  const avgTokens = $derived(points.length > 0 ? Math.round(totalTokens / points.length) : 0);

  // ── chart geometry (SVG viewBox) ────────────────────────────────────
  const H = 150;
  const W = $derived(chartW || 320);
  const PAD_L = 4;
  const PAD_R = 4;
  const PAD_T = 8;
  const PAD_B = 4;

  function px(i: number, n: number): number {
    if (n <= 1) return (W - PAD_L - PAD_R) / 2 + PAD_L;
    return PAD_L + (i * (W - PAD_L - PAD_R)) / (n - 1);
  }
  function py(tokens: number): number {
    return H - PAD_B - (tokens / maxTokens) * (H - PAD_T - PAD_B);
  }
  function pyPct(tokens: number): number {
    return (py(tokens) / H) * 100;
  }
  function pxPct(i: number, n: number): number {
    return (px(i, n) / W) * 100;
  }

  const linePoints = $derived(
    points.map((p, i) => `${px(i, points.length)},${py(p.tokens)}`).join(" "),
  );
  const areaPoints = $derived(
    points.length > 0
      ? `${PAD_L},${H - PAD_B} ${linePoints} ${W - PAD_R},${H - PAD_B}`
      : "",
  );
  const avgY = $derived(py(avgTokens));

  const rangeLabel = $derived.by(() => {
    if (period === "day") return t("trends.last7days");
    if (period === "month") return t("trends.thisMonth");
    return t("trends.monthlyStats");
  });

  function fmtDate(date: string): string {
    // TOTAL returns YYYY-MM; DAY/MONTH return YYYY-MM-DD → MM-DD.
    if (period === "total") return date;
    return date.length >= 10 ? date.slice(5, 10) : date;
  }

  const totalStr = $derived(splitTokens(totalTokens));
  const avgStr = $derived(splitTokens(avgTokens));
  const maxStr = $derived(splitTokens(maxTokens));

  // X-axis tick indices: first, middle, last (when enough points).
  const xTicks = $derived.by(() => {
    const n = points.length;
    if (n === 0) return [] as number[];
    if (n === 1) return [0];
    if (n <= 4) return [0, n - 1];
    return [0, Math.floor(n / 2), n - 1];
  });

  // Tooltip horizontal position (% of chart width), clamped to stay on-screen.
  const tipLeft = $derived(
    hoverIdx !== null && points.length > 0
      ? Math.min(82, Math.max(18, pxPct(hoverIdx, points.length)))
      : 50,
  );
</script>

<div class="seg-body">
  {#if data === null && !loadAttempted}
    <Skeleton type="chart" />
  {:else if data === null}
    <EmptyState title={t("common.loadFailed")} />
  {:else if points.length === 0}
    <EmptyState title={t("trends.noData")} />
  {:else}
    <!-- period title (first row, larger font) -->
    <div class="range-label">{rangeLabel}</div>

    <!-- stats cards (above the chart) -->
    {#if summary}
      <div class="stats-section">
        <StatsCards {summary} trends={points} locale="zh" />
      </div>
    {/if}

    <!-- line chart with axes -->
    <div class="chart-grid">
      <div class="plot" bind:clientWidth={chartW}>
        {#if hoverIdx !== null && points[hoverIdx]}
          {@const pt = points[hoverIdx]}
          <div class="tip" style="left:{tipLeft.toFixed(1)}%">
            <div class="tip-date">{fmtDate(pt.date)}</div>
            <div class="tip-row"><span>{t("trends.usageLabel")}</span><span>{formatTokens(pt.tokens)}</span></div>
            <div class="tip-row"><span>{t("trends.costLabel")}</span><span>${pt.cost_usd.toFixed(2)}</span></div>
            <div class="tip-row"><span>{t("trends.messagesLabel")}</span><span>{pt.messages}</span></div>
          </div>
        {/if}
        <!-- Y-axis max value (top-left); the only fixed reference besides the avg line -->
        <div class="ymax">{maxStr.value}<span class="ymu">{maxStr.unit}</span></div>
        <!-- average value label, positioned at the avg line's height -->
        <div class="avg-label" style="top:{pyPct(avgTokens).toFixed(1)}%">{t("trends.avgShort")} {avgStr.value}<span class="alu">{avgStr.unit}</span></div>
        <svg
          viewBox="0 0 {W} {H}"
          class="chart"
          preserveAspectRatio="none"
          role="img"
          aria-label="{rangeLabel}: {t('trends.total')} {totalStr.value}{totalStr.unit}, {t('trends.dailyAvg')} {avgStr.value}{avgStr.unit}"
        >
          <!-- average dashed line -->
          <line x1={PAD_L} y1={avgY} x2={W - PAD_R} y2={avgY} class="avg-line" />
          <!-- area fill under the line -->
          {#if areaPoints}
            <polygon points={areaPoints} class="trend-area" />
          {/if}
          <!-- the line -->
          <polyline points={linePoints} class="trend-line" />
          <!-- Y axis (left) + X axis (bottom) -->
          <line x1={PAD_L} y1={PAD_T} x2={PAD_L} y2={H - PAD_B} class="axis-line" />
          <line x1={PAD_L} y1={H - PAD_B} x2={W - PAD_R} y2={H - PAD_B} class="axis-line" />
          <!-- nodes: decorative (the svg carries a text summary; per-node
               labels made screen-reader output noisy) -->
          {#each points as pt, i (pt.date)}
            <circle
              cx={px(i, points.length)}
              cy={py(pt.tokens)}
              r={hoverIdx === i ? 3.5 : 2.4}
              class="node"
              class:active={hoverIdx === i}
              aria-hidden="true"
              onmouseenter={() => (hoverIdx = i)}
              onmouseleave={() => (hoverIdx = null)}
            />
          {/each}
        </svg>
      </div>
      <div class="x-axis">
        {#each xTicks as i (i)}
          <span class="xtick" style="left:{pxPct(i, points.length).toFixed(1)}%">{fmtDate(points[i].date)}</span>
        {/each}
      </div>
    </div>

    <!-- activity heatmap (only for total period) -->
    {#if period === "total" && points.length > 7}
      <div class="heatmap-section">
        <div class="heatmap-title">{t("trends.activity")}</div>
        <Heatmap {points} locale="zh" />
      </div>
    {/if}
  {/if}
</div>

<style>
  .seg-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .range-label {
    font-size: 0.8667rem;
    color: var(--text);
    font-weight: 400;
    padding: 0 2px;
  }
  .chart-grid {
    display: flex;
    flex-direction: column;
  }
  .plot {
    position: relative;
    height: 150px;
  }
  .chart {
    width: 100%;
    height: 100%;
    display: block;
  }
  .avg-label {
    position: absolute;
    right: 4px;
    transform: translateY(-50%);
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--cyan);
    background: var(--glass-2);
    border: 1px solid rgba(0, 0, 0, 0.2);
    padding: 1px 5px;
    border-radius: 4px;
    white-space: nowrap;
    pointer-events: none;
    z-index: 5;
  }
  .ymax {
    position: absolute;
    top: 2px;
    left: 4px;
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--text-faint);
    pointer-events: none;
  }
  .ymu { font-size: 0.4667rem; font-weight: 600; }
  .alu {
    font-size: 0.4667rem;
    margin-left: 1px;
    font-weight: 600;
  }
  /* Chart colors as direct values — CSS vars may not resolve inside SVG on
   * Windows WebView2 (transparent window). Dark-theme values below; the
   * [data-theme="light"] block overrides them with light equivalents
   * (dark-only values were near-invisible on the light background). */
  .trend-area { fill: rgba(232, 176, 75, 0.1); }
  .trend-line { fill: none; stroke: #e8b04b; stroke-width: 1.5; stroke-linejoin: round; stroke-linecap: round; vector-effect: non-scaling-stroke; }
  .avg-line   { stroke: #7fd1d3; stroke-width: 1; stroke-dasharray: 4 3; vector-effect: non-scaling-stroke; opacity: 0.7; }
  .axis-line  { stroke: #8a8470; stroke-width: 1; vector-effect: non-scaling-stroke; opacity: 0.5; }
  .node       { fill: #e8b04b; stroke: #0f0e0b; stroke-width: 1; vector-effect: non-scaling-stroke; transition: r 0.12s; cursor: pointer; }
  .node.active{ fill: #b4e34c; }
  /* Light theme — same literals as the token values in app.css (kept as
   * literals for the WebView2 reason above). */
  :global([data-theme="light"]) .trend-area { fill: rgba(201, 138, 30, 0.12); }
  :global([data-theme="light"]) .trend-line { stroke: #c98a1e; }
  :global([data-theme="light"]) .avg-line   { stroke: #2a9fa3; }
  :global([data-theme="light"]) .axis-line  { stroke: #9a9384; }
  :global([data-theme="light"]) .node       { fill: #c98a1e; stroke: #f5f3ef; }
  :global([data-theme="light"]) .node.active{ fill: #6ba81f; }
  :global([data-theme="light"]) .tip { box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12); }
  .x-axis {
    position: relative;
    height: 16px;
  }
  .xtick {
    position: absolute;
    top: 4px;
    transform: translateX(-50%);
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .tip {
    position: absolute;
    top: 2px;
    transform: translateX(-50%);
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 6px;
    padding: 6px 9px;
    font-size: 0.6667rem;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
  }
  .tip-date {
    font-family: var(--font-mono);
    color: var(--text);
    font-weight: 600;
    margin-bottom: 3px;
  }
  .tip-row {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    color: var(--text-faint);
  }
  .tip-row span:last-child {
    color: var(--text-dim);
    font-family: var(--font-mono);
  }

  /* ── stats cards section (above chart) ── */
  .stats-section {
    margin-bottom: 4px;
  }

  /* ── heatmap section ── */
  .heatmap-section {
    margin-top: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--border-dim);
  }
  .heatmap-title {
    font-size: 0.8rem;
    color: var(--text-faint);
    margin-bottom: 8px;
  }
</style>
