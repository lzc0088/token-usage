<script lang="ts">
  // 趋势 segment (T4.4). V1: simple per-day vertical bars (pure CSS, no chart
  // lib). Heapscape/K-line are V2. Data: get_trends(period).
  import { api, type Trends } from "../../lib/api";
  import { formatTokens } from "../../lib/format";
  import { periodValue } from "../../stores/period.svelte";

  let data = $state<Trends | null>(null);

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const t = await api.getTrends(p);
        if (!cancelled) data = t;
      } catch (e) {
        console.error("trends failed", e);
        if (!cancelled) data = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  const points = $derived(data?.points ?? []);
  const maxTokens = $derived(Math.max(1, ...points.map((p) => p.tokens)));
  const totalTokens = $derived(points.reduce((s, p) => s + p.tokens, 0));
  const activeDays = $derived(points.filter((p) => p.tokens > 0).length);
  const peak = $derived(points.reduce<Trends["points"][number] | null>((m, p) => (!m || p.tokens > m.tokens ? p : m), null));

  function barHeight(tokens: number): string {
    return `${Math.max(tokens > 0 ? 3 : 0, (tokens / maxTokens) * 100).toFixed(1)}%`;
  }
  function mmdd(date: string): string {
    // YYYY-MM-DD → MM-DD
    return date.length >= 10 ? date.slice(5, 10) : date;
  }
</script>

<div class="seg-body">
  {#if data === null}
    <p class="loading">加载中…</p>
  {:else if points.length === 0}
    <p class="empty">暂无趋势数据</p>
  {:else}
    <div class="stats">
      <div class="stat"><span class="k">总量</span><span class="v">{formatTokens(totalTokens)}</span></div>
      <div class="stat"><span class="k">峰值</span><span class="v">{peak ? formatTokens(peak.tokens) : "—"}</span></div>
      <div class="stat"><span class="k">活跃天</span><span class="v">{activeDays}</span></div>
    </div>

    <div class="bars">
      {#each points as pt (pt.date)}
        <div
          class="bar-col"
          title="{pt.date} · {formatTokens(pt.tokens)} · ${pt.cost_usd.toFixed(2)}"
        >
          <div class="bar" style="height:{barHeight(pt.tokens)}"></div>
        </div>
      {/each}
    </div>

    {#if points.length >= 2}
      <div class="axis">
        <span>{mmdd(points[0].date)}</span>
        <span>{mmdd(points[points.length - 1].date)}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  .seg-body {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .loading,
  .empty {
    color: var(--text-faint);
    font-size: 12px;
    text-align: center;
    padding: 24px 0;
  }
  .stats {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 8px;
  }
  .stat {
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .stat .k {
    font-size: 9.5px;
    color: var(--text-faint);
  }
  .stat .v {
    font-size: 15px;
    font-weight: 500;
    color: var(--text);
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 160px;
    padding: 8px 0 4px;
    border-bottom: 1px solid var(--border-dim);
  }
  .bar-col {
    flex: 1;
    min-width: 2px;
    height: 100%;
    display: flex;
    align-items: flex-end;
  }
  .bar {
    width: 100%;
    background: linear-gradient(180deg, var(--amber), rgba(232, 176, 75, 0.4));
    border-radius: 2px 2px 0 0;
    min-height: 0;
    transition: height 0.2s;
  }
  .bar-col:hover .bar {
    background: linear-gradient(180deg, var(--lime), rgba(180, 227, 76, 0.5));
  }
  .axis {
    display: flex;
    justify-content: space-between;
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--text-faint);
  }
</style>
