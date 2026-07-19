<script lang="ts">
  // 总览 segment (T4.1). Modules shown/hidden via ModuleBar; data from
  // get_summary (prop) + get_breakdown(tool|model).
  import ModuleBar from "../popover/ModuleBar.svelte";
  import BreakdownBar from "../common/BreakdownBar.svelte";
  import { api, type Breakdown, type Currency, type Summary } from "../../lib/api";
  import { formatCost, formatTokens } from "../../lib/format";
  import { isModuleVisible } from "../../stores/modules.svelte";
  import { periodValue } from "../../stores/period.svelte";

  let {
    summary,
    currency,
    cnyRate = 7.2,
  }: { summary: Summary | null; currency: Currency; cnyRate?: number } = $props();

  let toolB = $state<Breakdown | null>(null);
  let modelB = $state<Breakdown | null>(null);

  // Re-fetch breakdowns when the period changes.
  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const [t, m] = await Promise.all([
          api.getBreakdown(p, "tool"),
          api.getBreakdown(p, "model"),
        ]);
        if (!cancelled) {
          toolB = t;
          modelB = m;
        }
      } catch {
        if (!cancelled) {
          toolB = null;
          modelB = null;
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  // Cache hit rate = cacheRead / (cacheRead + input).
  const hitrate = $derived(
    summary && summary.cache_read + summary.input > 0
      ? (summary.cache_read / (summary.cache_read + summary.input)) * 100
      : 0,
  );

  const palette = ["var(--amber)", "var(--lime)", "var(--cyan)", "var(--violet)", "var(--coral)"];
</script>

<ModuleBar />

<div class="ov-body">
  {#if !summary}
    <p class="muted">加载中…</p>
  {/if}

  {#if summary && isModuleVisible("split")}
    <section class="module">
      <div class="m-title">分项</div>
      <div class="split2">
        <div class="scell"><div class="k">输入</div><div class="v">{formatTokens(summary.input)}</div></div>
        <div class="scell"><div class="k">输出</div><div class="v amber">{formatTokens(summary.output)}</div></div>
        <div class="scell"><div class="k">缓存读</div><div class="v">{formatTokens(summary.cache_read)}</div></div>
        <div class="scell"><div class="k">缓存写</div><div class="v dim">{formatTokens(summary.cache_write)}</div></div>
      </div>
    </section>
  {/if}

  {#if summary && isModuleVisible("hitrate")}
    <section class="module">
      <div class="m-title">缓存命中</div>
      <div class="hit">
        <span class="hit-pct">{hitrate.toFixed(1)}%</span>
        <span class="hit-meta">{formatTokens(summary.cache_read)} / {formatTokens(summary.cache_read + summary.input)}</span>
      </div>
    </section>
  {/if}

  {#if isModuleVisible("tools") && toolB}
    <section class="module">
      <div class="m-title">工具 · Top</div>
      {#each toolB.entries.slice(0, 3) as e, i (e.key)}
        <div class="crow">
          <span class="rk">{i + 1}</span>
          <span class="cnm">{e.key}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)}</span></span>
          <BreakdownBar pct={e.token_pct} color={palette[i % palette.length]} />
          <span class="cvl">{formatTokens(e.tokens)}</span>
        </div>
      {:else}
        <p class="empty">无数据</p>
      {/each}
    </section>
  {/if}

  {#if isModuleVisible("models") && modelB}
    <section class="module">
      <div class="m-title">模型 · Top</div>
      {#each modelB.entries.slice(0, 3) as e, i (e.key)}
        <div class="crow">
          <span class="rk">{i + 1}</span>
          <span class="cnm">{e.key}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)}</span></span>
          <BreakdownBar pct={e.token_pct} color={palette[(i + 2) % palette.length]} />
          <span class="cvl">{formatTokens(e.tokens)}</span>
        </div>
      {:else}
        <p class="empty">无数据</p>
      {/each}
    </section>
  {/if}

  {#if isModuleVisible("limits")}
    <section class="module">
      <div class="m-title">额度</div>
      <p class="empty">额度分段（M2 T2.5 adapter · M4 T4.5 实装）</p>
    </section>
  {/if}
</div>

<style>
  .ov-body {
    padding: 14px 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .module {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .m-title {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--text-faint);
    text-transform: uppercase;
  }
  .split2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .scell {
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    padding: 9px 11px;
  }
  .scell .k {
    font-size: 10px;
    color: var(--text-faint);
  }
  .scell .v {
    font-size: 17px;
    font-weight: 500;
    color: var(--text);
    margin-top: 2px;
  }
  .v.amber {
    color: var(--amber);
  }
  .v.dim {
    color: var(--text-dim);
  }
  .hit {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .hit-pct {
    font-size: 22px;
    font-weight: 500;
    color: var(--lime);
  }
  .hit-meta {
    font-size: 11px;
    color: var(--text-faint);
  }
  .crow {
    display: grid;
    grid-template-columns: 14px 1fr 60px 46px;
    align-items: center;
    gap: 8px;
  }
  .rk {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .cnm {
    font-size: 12px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cnm .sub {
    display: block;
    font-size: 9.5px;
    color: var(--text-faint);
    margin-top: 1px;
  }
  .cvl {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-dim);
    text-align: right;
  }
  .empty {
    margin: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
  .muted {
    margin: 0;
    color: var(--text-dim);
  }
</style>
