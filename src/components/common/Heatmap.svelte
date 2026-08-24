<script lang="ts">
  // GitHub-style contribution heatmap. Pure SVG, no external dependencies.
  // Shows daily token usage intensity over a rolling year.

  import type { TrendPoint } from "../../lib/api";
  import { formatCompact, type Locale } from "../../lib/format";

  let {
    points,
    locale = "en",
    cellSize = 11,
    gap = 2,
  }: {
    points: TrendPoint[];
    locale?: Locale;
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
    intensity: number; // 0-4
    tokens: number;
    cost: number;
    col: number;
    row: number;
    x: number;
    y: number;
  }

  const cells = $derived.by((): HeatmapCell[] => {
    if (points.length === 0) return [];

    // Find date range (rolling year from latest date)
    const sorted = [...points].sort((a, b) => a.date.localeCompare(b.date));
    const endDate = sorted[sorted.length - 1].date;
    const startDate = addDays(endDate, -364); // ~1 year

    // Build intensity map
    const intensityMap = new Map<string, number>();
    const tokensMap = new Map<string, number>();
    const costMap = new Map<string, number>();
    let maxTokens = 0;

    for (const p of sorted) {
      if (p.date >= startDate) {
        tokensMap.set(p.date, p.tokens);
        costMap.set(p.date, p.cost_usd);
        maxTokens = Math.max(maxTokens, p.tokens);
      }
    }

    // Compute intensities (0-4 scale)
    for (const [date, tokens] of tokensMap) {
      if (maxTokens <= 0) {
        intensityMap.set(date, 0);
      } else {
        const ratio = tokens / maxTokens;
        intensityMap.set(date, ratio >= 0.75 ? 4 : ratio >= 0.5 ? 3 : ratio >= 0.25 ? 2 : ratio > 0 ? 1 : 0);
      }
    }

    // Generate grid (Sunday-started columns)
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
  const height = $derived(7 * (cellSize + gap) - gap);

  // ── Month labels ────────────────────────────────────────────────────────

  const monthLabels = $derived.by(() => {
    const labels: { col: number; label: string }[] = [];
    for (const cell of cells) {
      if (cell.date.slice(8, 10) === "01") {
        labels.push({ col: cell.col, label: cell.date.slice(0, 7) });
      }
    }
    return labels;
  });

  // ── Intensity colors ────────────────────────────────────────────────────

  const INTENSITY_COLORS = [
    "var(--glass-3)",     // 0: empty
    "rgba(76, 175, 80, 0.3)",  // 1: low
    "rgba(76, 175, 80, 0.5)",  // 2: medium
    "rgba(76, 175, 80, 0.7)",  // 3: high
    "rgba(76, 175, 80, 1.0)",  // 4: very high
  ];

  // ── Tooltip ─────────────────────────────────────────────────────────────

  let tooltip = $state<{ x: number; y: number; date: string; tokens: number; cost: number } | null>(null);

  function showTooltip(cell: HeatmapCell, e: MouseEvent): void {
    const rect = (e.target as SVGElement).getBoundingClientRect();
    tooltip = {
      x: rect.left + rect.width / 2,
      y: rect.top - 8,
      date: cell.date,
      tokens: cell.tokens,
      cost: cell.cost,
    };
  }

  function hideTooltip(): void {
    tooltip = null;
  }
</script>

<div class="heatmap-container">
  <svg {width} {height} class="heatmap-svg">
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

    {#each monthLabels as label (label.col)}
      <text
        x={label.col * (cellSize + gap)}
        y={height + 14}
        class="month-label"
      >
        {label.label.slice(5)}
      </text>
    {/each}
  </svg>

  {#if tooltip}
    <div
      class="heatmap-tooltip"
      style="left: {tooltip.x}px; top: {tooltip.y}px;"
    >
      <div class="tooltip-date">{tooltip.date}</div>
      <div class="tooltip-value">
        {formatCompact(tooltip.tokens, locale)} tokens
      </div>
      <div class="tooltip-cost">
        ${tooltip.cost.toFixed(2)}
      </div>
    </div>
  {/if}
</div>

<style>
  .heatmap-container {
    position: relative;
    overflow-x: auto;
    padding-bottom: 20px;
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

  .heatmap-tooltip {
    position: fixed;
    transform: translate(-50%, -100%);
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
    font-weight: 600;
    margin-bottom: 2px;
  }

  .tooltip-value {
    color: var(--lime);
  }

  .tooltip-cost {
    color: var(--amber);
    font-size: 0.6667rem;
  }
</style>
