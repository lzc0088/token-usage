<script lang="ts">
  // GitHub-style contribution heatmap. Pure SVG, no external dependencies.
  // Shows daily token usage intensity over a rolling year.

  import type { TrendPoint } from "../../lib/api";
  import { splitTokens, splitCost, type Locale } from "../../lib/format";
  import type { Currency } from "../../lib/api";
  import { t } from "../../lib/i18n.svelte";

  let {
    points,
    locale = "en",
    currency = "both",
    cnyRate = 7.2,
    cellSize = 11,
    gap = 2,
  }: {
    points: TrendPoint[];
    locale?: Locale;
    currency?: Currency;
    cnyRate?: number;
    cellSize?: number;
    gap?: number;
  } = $props();

  // ── Date utilities ──────────────────────────────────────────────────────

  function parseDate(key: string): Date | null {
    if (!key || key.length < 10) return null;
    return new Date(`${key}T00:00:00Z`);
  }

  function formatDate(d: Date): string {
    return d.toISOString().slice(0, 10);
  }

  function addDays(key: string, delta: number): string {
    const d = parseDate(key);
    if (!d) return key;
    d.setUTCDate(d.getUTCDate() + delta);
    return formatDate(d);
  }

  function dayOfWeekSun(key: string): number {
    const d = parseDate(key);
    return d ? d.getUTCDay() : 0;
  }

  // ── Heatmap computation ─────────────────────────────────────────────────

  interface HeatmapCell {
    date: string;
    intensity: number;
    tokens: number;
    cost: number;
    messages: number;
    col: number;
    row: number;
    x: number;
    y: number;
  }

  const cells = $derived.by((): HeatmapCell[] => {
    if (points.length === 0) return [];

    const sorted = [...points].sort((a, b) => a.date.localeCompare(b.date));
    const endDate = sorted[sorted.length - 1].date;
    const startDate = addDays(endDate, -364); // ~1 year

    const intensityMap = new Map<string, number>();
    const tokensMap = new Map<string, number>();
    const costMap = new Map<string, number>();
    const messagesMap = new Map<string, number>();
    let maxTokens = 0;

    for (const p of sorted) {
      if (p.date >= startDate) {
        tokensMap.set(p.date, p.tokens);
        costMap.set(p.date, p.cost_usd);
        messagesMap.set(p.date, p.messages);
        maxTokens = Math.max(maxTokens, p.tokens);
      }
    }

    for (const [date, tokens] of tokensMap) {
      if (maxTokens <= 0) {
        intensityMap.set(date, 0);
      } else {
        const ratio = tokens / maxTokens;
        intensityMap.set(date, ratio >= 0.75 ? 4 : ratio >= 0.5 ? 3 : ratio >= 0.25 ? 2 : ratio > 0 ? 1 : 0);
      }
    }

    const startDow = dayOfWeekSun(startDate);
    const gridStart = addDays(startDate, -startDow);

    const result: HeatmapCell[] = [];
    let key = gridStart;
    while (key <= endDate) {
      const dow = dayOfWeekSun(key);
      const daysFromStart = Math.round(
        (parseDate(key)!.getTime() - parseDate(gridStart)!.getTime()) / 86400000
      );
      const col = Math.floor(daysFromStart / 7);

      result.push({
        date: key,
        intensity: intensityMap.get(key) ?? 0,
        tokens: tokensMap.get(key) ?? 0,
        cost: costMap.get(key) ?? 0,
        messages: messagesMap.get(key) ?? 0,
        col,
        row: dow,
        x: col * (cellSize + gap),
        y: dow * (cellSize + gap),
      });

      key = addDays(key, 1);
    }

    return result;
  });

  const weeks = $derived(cells.length > 0 ? cells[cells.length - 1].col + 1 : 0);
  const width = $derived(weeks * (cellSize + gap) - gap);
  const gridH = $derived(7 * (cellSize + gap) - gap);
  const LABEL_GAP = 16;

  // ── Month labels & separators ─────────────────────────────────────────

  let scrollEl: HTMLDivElement | null = null;

  function scrollToEnd(): void {
    if (scrollEl) {
      scrollEl.scrollLeft = scrollEl.scrollWidth;
    }
  }

  // Auto-scroll to the latest month on mount / data change.
  $effect(() => {
    // Wait for DOM to paint, then scroll.
    const id = requestAnimationFrame(() => requestAnimationFrame(scrollToEnd));
    return () => cancelAnimationFrame(id);
  });

  const monthLabels = $derived.by(() => {
    const labels: { col: number; sepCol: number; label: string }[] = [];
    let prevMonth = "";
    for (const cell of cells) {
      const m = cell.date.slice(0, 7);
      if (m !== prevMonth) {
        const mon = parseInt(cell.date.slice(5, 7), 10);
        const isFirst = labels.length === 0;
        labels.push({
          col: cell.col,
          sepCol: isFirst ? -1 : cell.col,
          label: locale === "en" ? `${mon}M` : `${mon}月`,
        });
      }
      prevMonth = m;
    }
    return labels;
  });

  // Unique separator x-positions (one per month boundary).
  const sepPositions = $derived.by(() => {
    const s = new Set<number>();
    for (const label of monthLabels) {
      if (label.sepCol >= 0) s.add(label.sepCol);
    }
    return [...s].sort((a, b) => a - b);
  });

  // ── Intensity colors ────────────────────────────────────────────────────

  const INTENSITY_COLORS = [
    "var(--glass-3)",
    "rgba(76, 175, 80, 0.3)",
    "rgba(76, 175, 80, 0.5)",
    "rgba(76, 175, 80, 0.7)",
    "rgba(76, 175, 80, 1.0)",
  ];

  // ── Tooltip (viewport-aware, not clipped by overflow-x) ─────────────

  const TIP_W = 160;
  const TIP_H = 60;
  const PAD = 8;

  let tooltip = $state<{ left: number; top: number; date: string; tokens: number; cost: number; messages: number } | null>(null);

  function showTooltip(cell: HeatmapCell, e: MouseEvent): void {
    const rect = (e.target as SVGElement).getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top;

    let left = cx - TIP_W / 2;
    left = Math.max(PAD, Math.min(left, window.innerWidth - TIP_W - PAD));

    const above = cy - TIP_H - 4 >= PAD;
    const top = above ? cy - TIP_H - 4 : cy + rect.height + 4;

    tooltip = { left, top, date: cell.date, tokens: cell.tokens, cost: cell.cost, messages: cell.messages };
  }

  function hideTooltip(): void {
    tooltip = null;
  }
</script>

<div class="heatmap-outer">
  <div class="heatmap-scroll" bind:this={scrollEl}>
    <svg width={width} height={gridH + LABEL_GAP} class="heatmap-svg">
      <!-- month separator lines -->
      {#each sepPositions as col (col)}
        <line
          x1={col * (cellSize + gap) - gap / 2}
          y1={0}
          x2={col * (cellSize + gap) - gap / 2}
          y2={gridH}
          stroke="var(--border-dim)"
          stroke-width="1.5"
          stroke-dasharray="3 2"
        />
      {/each}

      <!-- cells -->
      {#each cells as cell (cell.date)}
        <rect
          x={cell.x}
          y={cell.y}
          width={cellSize}
          height={cellSize}
          rx={2}
          fill={INTENSITY_COLORS[cell.intensity]}
          class="heatmap-cell"
          role="img"
          aria-label="{cell.date}: {cell.tokens} tokens"
          onmouseenter={(e) => showTooltip(cell, e)}
          onmouseleave={hideTooltip}
        />
      {/each}

      <!-- month labels -->
      {#each monthLabels as label (label.col)}
        <text
          x={label.col * (cellSize + gap)}
          y={gridH + 12}
          class="month-label"
        >
          {label.label}
        </text>
      {/each}
    </svg>
  </div>

  {#if tooltip}
    <div
      class="heatmap-tooltip"
      style="left: {tooltip.left}px; top: {tooltip.top}px;"
    >
      <div class="tooltip-date">{tooltip.date}</div>
      <div class="tooltip-row">
        <span>{t("trends.usageLabel")}</span>
        <span class="row-val">{splitTokens(tooltip.tokens).value}<span class="tu">{splitTokens(tooltip.tokens).unit}</span></span>
      </div>
      <div class="tooltip-row">
        <span>{t("trends.costLabel")}</span>
        <span class="row-val">{#each splitCost(tooltip.cost, currency, cnyRate) as part, i (i)}{part.sep ?? ""}<span class="cu">{part.unit}</span>{part.value}{/each}</span>
      </div>
      <div class="tooltip-row">
        <span>{t("trends.messagesLabel")}</span>
        <span class="row-val">{tooltip.messages}</span>
      </div>
    </div>
  {/if}
</div>

<style>
  /* ── outer wrapper (tooltip lives here, not clipped by overflow-x) ── */
  .heatmap-outer {
    position: relative;
    padding-bottom: 20px;
  }

  .heatmap-scroll {
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 4px;
  }
  .heatmap-scroll::-webkit-scrollbar {
    height: 5px;
  }
  .heatmap-scroll::-webkit-scrollbar-track {
    background: transparent;
  }
  .heatmap-scroll::-webkit-scrollbar-thumb {
    background: var(--glass-3);
    border-radius: 3px;
  }
  .heatmap-scroll::-webkit-scrollbar-thumb:hover {
    background: var(--text-faint);
  }
  .heatmap-scroll {
    scrollbar-width: thin;
    scrollbar-color: var(--glass-3) transparent;
  }

  .heatmap-svg {
    display: block;
  }

  .heatmap-cell {
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .heatmap-cell:hover {
    opacity: 0.8;
    stroke: var(--text);
    stroke-width: 1;
  }

  .month-label {
    font-size: 0.6667rem;
    fill: var(--text-faint);
    font-family: var(--font-mono);
  }

  /* ── tooltip: positioned by JS, no CSS transform ── */
  .heatmap-tooltip {
    position: fixed;
    background: var(--glass);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 0.7333rem;
    color: var(--text);
    pointer-events: none;
    z-index: 1000;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    white-space: nowrap;
  }

  .tooltip-date {
    font-family: var(--font-mono);
    font-weight: 600;
    margin-bottom: 3px;
  }

  /* Row layout mirrors the Trend line-chart tooltip (.tip-row): faint label
   * left, mono value right. */
  .tooltip-row {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    color: var(--text-faint);
  }
  .tooltip-row .row-val {
    color: var(--text-dim);
    font-family: var(--font-mono);
  }
  /* Small units inside tooltip values: token unit right, currency left. */
  .tooltip-row .tu {
    font-size: 0.5rem;
    font-weight: 600;
  }
  .tooltip-row .cu {
    font-size: 0.5rem;
    font-weight: 700;
  }
</style>
