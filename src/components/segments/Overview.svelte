<script lang="ts">
  import { api, type Breakdown, type Currency, type Quota, type Summary } from "../../lib/api";
  import { formatCost, formatTokens, splitTokens } from "../../lib/format";
  import { toolMeta } from "../../lib/toolMeta";
  import { periodValue } from "../../stores/period.svelte";
  import { setSegment } from "../../stores/segment.svelte";

  let {
    summary,
    currency,
    cnyRate = 7.2,
  }: { summary: Summary | null; currency: Currency; cnyRate?: number } = $props();

  let toolB = $state<Breakdown | null>(null);
  let modelB = $state<Breakdown | null>(null);
  let quotas = $state<Quota[]>([]);

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const [t, m] = await Promise.all([
          api.getBreakdown(p, "tool"),
          api.getBreakdown(p, "model"),
        ]);
        if (!cancelled) { toolB = t; modelB = m; }
      } catch {
        if (!cancelled) { toolB = null; modelB = null; }
      }
    })();
    return () => { cancelled = true; };
  });

  $effect(() => {
    api.getQuotas().then(q => quotas = q).catch(() => quotas = []);
  });

  const hitrate = $derived(
    summary && summary.cache_read + summary.input > 0
      ? (summary.cache_read / (summary.cache_read + summary.input)) * 100 : 0,
  );
  const savedCostUsd = $derived(summary ? (summary.cache_read * 2.7) / 1_000_000 : 0);

  const RING_C = 2 * Math.PI * 15;
  const ringDash = $derived((RING_C * hitrate) / 100);
  const palette = ["var(--amber)", "var(--cyan)", "var(--lime)", "var(--violet)", "var(--coral)"];
  const hasQuotas = $derived(quotas.length > 0);
</script>

<div class="ov-body">
  {#if !summary}
    <p class="muted">加载中…</p>
  {/if}

  <!-- 分项 + 缓存命中 → 合并为一个区域，无标题 -->
  {#if summary}
    <section class="module">
      <div class="split2">
        {#each [
          { k: "输入", v: summary.input, cls: "" },
          { k: "输出", v: summary.output, cls: "amber" },
          { k: "缓存读取", v: summary.cache_read, cls: "lime" },
          { k: "缓存写入", v: summary.cache_write, cls: "coral" },
        ] as cell (cell.k)}
          {@const s = splitTokens(cell.v)}
          <div class="scell"><div class="k">{cell.k}</div><div class="v {cell.cls}">{s.value}<span class="u">{s.unit}</span></div></div>
        {/each}
      </div>
      <div class="hitbar">
        <div class="pct">{hitrate.toFixed(0)}<span class="pct-unit">%</span></div>
        <div class="meta">
          <div class="t">{formatTokens(summary.cache_read)} / {formatTokens(summary.cache_read + summary.input)}</div>
          <div class="s">≈ 节省 {formatCost(savedCostUsd, currency, cnyRate)}</div>
        </div>
        <div class="ring" aria-hidden="true">
          <svg width="40" height="40" viewBox="0 0 40 40">
            <circle cx="20" cy="20" r="15" fill="none" stroke="var(--glass-3)" stroke-width="5" />
            <circle cx="20" cy="20" r="15" fill="none" stroke="var(--lime)" stroke-width="5"
              stroke-dasharray="{ringDash.toFixed(1)} {RING_C.toFixed(1)}"
              stroke-linecap="round" transform="rotate(-90 20 20)" />
          </svg>
        </div>
      </div>
    </section>
  {/if}

  {#if toolB}
    <section class="module">
      <div class="sec-h"><span><span class="title-dot" style="background:var(--amber)"></span>工具</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("tools")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("tools")}>全部</span></div>
      {#if toolB.entries.length > 0}
        {#each toolB.entries.slice(0, 3) as e, i (e.key)}
          {@const s = splitTokens(e.tokens)}
          <div class="crow" role="button" tabindex="0" onclick={() => setSegment("tools")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("tools")}>
            <span class="rk" style="background:{toolMeta(e.key).color};color:#1a1408">{toolMeta(e.key).icon}</span>
            <span class="nm">{toolMeta(e.key).label}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {s.value}<span class="su">{s.unit}</span></span></span>
            <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[i % palette.length]}"></i></div>
            <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
            <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
          </div>
        {/each}
      {:else}
        <p class="empty">暂无工具数据 — 等待 tokscale 采集</p>
      {/if}
    </section>
  {/if}

  {#if modelB}
    <section class="module">
      <div class="sec-h"><span><span class="title-dot" style="background:var(--cyan)"></span>模型</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("models")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("models")}>全部</span></div>
      {#if modelB.entries.length > 0}
        {#each modelB.entries.slice(0, 3) as e, i (e.key)}
          {@const s = splitTokens(e.tokens)}
          <div class="crow" role="button" tabindex="0" onclick={() => setSegment("models")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("models")}>
            <span class="rk" style="background:{toolMeta(e.key).color};color:#1a1408">{toolMeta(e.key).icon}</span>
            <span class="nm">{toolMeta(e.key).label}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {e.messages} 会话</span></span>
            <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[(i + 2) % palette.length]}"></i></div>
            <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
            <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
          </div>
        {/each}
      {:else}
        <p class="empty">暂无模型数据 — 等待 tokscale 采集</p>
      {/if}
    </section>
  {/if}

  <!-- 额度：标题始终显示，有厂商时显示列表，否则显示说明文字 -->
  <section class="module">
    <div class="sec-h"><span><span class="title-dot" style="background:var(--violet)"></span>额度</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("limit")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("limit")}>全部</span></div>
    {#if hasQuotas}
      {#each quotas as q (q.vendor)}
        <div class="lim-row">
          <span class="lico" style="background:rgba(232,176,75,.15);color:var(--amber)">{q.vendor.charAt(0)}</span>
          <span class="ln">{q.vendor}</span>
          <span class="lbt" style="color:var(--text-faint)">{q.display}</span>
        </div>
      {/each}
    {:else}
      <p class="empty">尚未配置厂商账户 · 在设置中绑定账号后显示额度</p>
    {/if}
  </section>
</div>

<style>
  .ov-body { padding: 14px 18px; display: flex; flex-direction: column; }

  .module {
    margin-bottom: 14px; padding-bottom: 14px;
    border-bottom: 1px solid var(--border-dim);
  }
  .module:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }

  /* section header */
  .sec-h {
    font-size: 13px; font-weight: 700;
    color: var(--text);
    margin-bottom: 10px;
    display: flex; justify-content: space-between; align-items: center;
  }
  .sec-h .title-dot {
    display: inline-block; width: 8px; height: 8px; border-radius: 50%;
    margin-right: 6px; flex-shrink: 0;
  }
  .sec-h .more { color: var(--amber); cursor: pointer; font-size: 13px; font-weight: 600; }

  /* split2 + hitbar (merged — no title) */
  .split2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-bottom: 10px; }
  .scell {
    background: rgba(255,255,255,.025); border: 1px solid var(--border-dim);
    border-radius: 9px; padding: 10px 11px;
  }
  .scell .k { font-size: 11px; color: var(--text-faint); }
  .scell .v {
    font-size: 20px; font-weight: 500; color: var(--text); margin-top: 2px;
    display: flex; align-items: baseline; gap: 2px;
  }
  .scell .v .u { font-size: 11px; color: var(--text-faint); font-weight: 600; }
  .v.amber { color: var(--amber) !important; }
  .v.lime { color: var(--lime) !important; }
  .v.coral { color: var(--coral) !important; }

  .hitbar {
    display: flex; align-items: center; gap: 10px;
    background: rgba(180,227,76,.05); border: 1px solid rgba(180,227,76,.15);
    border-radius: 9px; padding: 10px 12px;
  }
  .hitbar .pct { font-size: 24px; color: var(--lime); line-height: 1; }
  .hitbar .pct-unit { font-size: 12px; margin-left: 2px; }
  .hitbar .meta { flex: 1; }
  .hitbar .meta .t { font-size: 12px; color: var(--text-dim); }
  .hitbar .meta .s { font-size: 12px; color: var(--lime); margin-top: 1px; }
  .hitbar .ring { width: 40px; height: 40px; flex-shrink: 0; }

  /* crow rows — rank as colored badge, dashed dividers, pct label */
  .crow {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 0; border-bottom: 1px dashed var(--border-dim); cursor: pointer;
  }
  .crow:last-child { border-bottom: none; }
  .crow:hover .nm { color: var(--amber); }
  .crow .rk {
    width: 24px; height: 24px; border-radius: 6px;
    display: flex; align-items: center; justify-content: center;
    font-size: 11px; font-weight: 700; flex-shrink: 0;
    font-style: normal;
  }
  .crow .nm {
    font-size: 13px; flex: 1; color: var(--text); transition: .15s;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .nm .sub {
    display: block; font-size: 11px; color: var(--text-faint); margin-top: 1px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .su { font-size: 9px; margin-left: 2px; }
  .crow .br {
    width: 62px; height: 4px; background: var(--glass-3);
    border-radius: 2px; overflow: hidden; flex-shrink: 0;
  }
  .crow .br i { display: block; height: 100%; }
  .crow .pct-label {
    font-family: var(--font-mono); font-size: 12px; color: var(--text-faint);
    width: 42px; text-align: right; flex-shrink: 0;
  }
  .crow .pct-unit { font-size: 8px; margin-left: 2px; }
  .crow .vl {
    font-size: 12px; color: var(--text-dim); width: 50px; text-align: right; flex-shrink: 0;
  }
  .crow .vlu { font-size: 8px; color: var(--text-faint); font-weight: 600; margin-left: 2px; }

  /* limits */
  .lim-row {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 0; border-bottom: 1px dashed var(--border-dim);
  }
  .lim-row:last-child { border-bottom: none; }
  .lim-row .lico {
    width: 18px; height: 18px; border-radius: 5px;
    display: flex; align-items: center; justify-content: center;
    font-size: 9px; font-weight: 600; flex-shrink: 0;
  }
  .lim-row .ln { font-size: 12px; color: var(--text); }
  .lim-row .lbt {
    flex: 1; padding-left: 8px; font-size: 11px;
    text-align: right; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  .empty { margin: 0; font-size: 12px; color: var(--text-faint); }
  .muted { margin: 0; color: var(--text-dim); font-size: 13px; }
</style>
