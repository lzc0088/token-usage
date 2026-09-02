<script lang="ts">
  // GitHub-style contribution heatmap. Pure SVG, no external dependencies.
  // Shows daily token usage intensity over a rolling year.

  import type { TrendPoint } from "../../lib/api";
  import { splitTokens, splitTokensCN, splitCost, type Locale } from "../../lib/format";
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

  // ── Heatmap computation ─────────────────────────────────────────────────
  // Layout: per-month mini-blocks. Each month owns ⌈31/7⌉ = 5 columns; the
  // 1st always sits at the block's top-left cell and days flow top-to-bottom
  // then left-to-right (1-7 in column 1, 8-14 in column 2, …). Blocks are
  // separated by a fixed gap, so weekday alignment is intentionally dropped.

  /** Days per row inside a month block (7 → 1-7 first row, 8-14 second …). */
  const DAYS_PER_ROW = 7;
  /** Rows every month block reserves (31 days ÷ 7 → 5). */
  const ROWS_PER_MONTH = 5;
  /** Horizontal whitespace between month blocks, in px. */
  const MONTH_GAP = 10;

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

    // Month list from the window's first month to the last data month.
    const end = parseDate(endDate);
    const start = parseDate(startDate);
    if (!end || !start) return [];
    const months: { y: number; m: number }[] = [];
    let y = start.getUTCFullYear();
    let m = start.getUTCMonth();
    while (y < end.getUTCFullYear() || (y === end.getUTCFullYear() && m <= end.getUTCMonth())) {
      months.push({ y, m });
      m += 1;
      if (m > 11) { m = 0; y += 1; }
    }

    const step = cellSize + gap;
    const result: HeatmapCell[] = [];
    months.forEach((month, mi) => {
      // Days in month via UTC (month index + 1, day 0 = last day of month).
      const days = new Date(Date.UTC(month.y, month.m + 1, 0)).getUTCDate();
      for (let d = 1; d <= days; d++) {
        const date = `${month.y}-${String(month.m + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
        // Days flow left-to-right, 7 per row; the 1st sits top-left.
        const localCol = (d - 1) % DAYS_PER_ROW;
        const row = Math.floor((d - 1) / DAYS_PER_ROW);
        result.push({
          date,
          intensity: intensityMap.get(date) ?? 0,
          tokens: tokensMap.get(date) ?? 0,
          cost: costMap.get(date) ?? 0,
          messages: messagesMap.get(date) ?? 0,
          col: mi * DAYS_PER_ROW + localCol,
          row,
          x: mi * (DAYS_PER_ROW * step + MONTH_GAP) + localCol * step,
          y: row * step,
        });
      }
    });

    return result;
  });

  const monthLabels = $derived.by(() => {
    const labels: { mi: number; cx: number; label: string; total: string }[] = [];
    let curMi = -1;
    let curTokens = 0;
    let curFirstDate = "";
    let curX = 0;
    const fmt = (n: number): string =>
      locale === "en" ? splitTokens(n, 1).value + splitTokens(n, 1).unit
                     : splitTokensCN(n, 1).value + splitTokensCN(n, 1).unit;
    const flush = (): void => {
      if (curMi === -1) return;
      // Center of the month block (7 columns; last cell ends 1 gap short).
      const cx = curX + (DAYS_PER_ROW * (cellSize + gap) - gap) / 2;
      labels.push({
        mi: curMi,
        cx,
        label: `${curFirstDate.slice(2, 4)}-${curFirstDate.slice(5, 7)}`, // yy-mm
        total: `（${fmt(curTokens)}）`,
      });
    };
    for (const cell of cells) {
      // A new month starts whenever the column jumps a whole block forward.
      const mi = Math.floor(cell.col / DAYS_PER_ROW);
      if (mi !== curMi) {
        flush();
        curMi = mi;
        curTokens = 0;
        curFirstDate = cell.date; // always the month's 1st (block col 0, row 0)
        curX = cell.x;
      }
      curTokens += cell.tokens;
    }
    flush();
    return labels;
  });

  const width = $derived(
    cells.length > 0
      ? (Math.floor(cells[cells.length - 1].col / DAYS_PER_ROW) + 1) * (DAYS_PER_ROW * (cellSize + gap) + MONTH_GAP) - MONTH_GAP - gap
      : 0,
  );
  const gridH = $derived(ROWS_PER_MONTH * (cellSize + gap) - gap);
  const LABEL_GAP = 30; // yy/mm line + month-total line below the grid

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

      <!-- month labels: yy-mm on the first line, month total below — both
           centered over their month block -->
      {#each monthLabels as label (label.mi)}
        <text
          x={label.cx}
          y={gridH + 12}
          text-anchor="middle"
          class="month-label"
        >
          {label.label}
        </text>
        <text
          x={label.cx}
          y={gridH + 24}
          text-anchor="middle"
          class="month-total"
        >
          {label.total}
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

  /* Scrollbar hidden but scrolling stays enabled (trackpad / shift-wheel /
     drag) — the 5px bar ate vertical space in the compact trend layout. */
  .heatmap-scroll {
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 4px;
    scrollbar-width: none; /* Firefox */
    -ms-overflow-style: none; /* legacy Edge */
  }
  .heatmap-scroll::-webkit-scrollbar {
    display: none; /* WebKit / WebView2 (macOS & Windows webviews) */
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
  .month-total {
    font-size: 0.6rem;
    fill: var(--text-dim);
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
