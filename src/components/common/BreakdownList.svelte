<script lang="ts">
  import BreakdownBar from "./BreakdownBar.svelte";
  import { PALETTE } from "../../lib/constants";
  import { formatCost, splitTokens } from "../../lib/format";
  import { modelVendor, vendorIdForModel } from "../../lib/meta/models";
  import { toolMeta, vendorIcon } from "../../lib/meta/tools";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { api, type Breakdown, type BreakdownEntry, type Currency, type Dimension } from "../../lib/api";
  import { periodValue } from "../../stores/period.svelte";

  let {
    entries,
    currency,
    cnyRate = 7.2,
    title = "",
    dim = "tool" as Dimension,
  }: { entries: BreakdownEntry[]; currency: Currency; cnyRate?: number; title?: string; dim?: Dimension } = $props();

  const oppDim: Dimension = $derived(dim === "tool" ? "model" : "tool");

  // PALETTE imported from constants.ts

  type SortKey = "token" | "cost" | "name";
  let sort = $state<SortKey>("token");
  let expanded = $state<Set<string>>(new Set());
  let details = $state<Record<string, Breakdown | null>>({});

  async function toggleExpand(key: string) {
    if (expanded.has(key)) { expanded = new Set(); return; }
    // Accordion: only one open at a time.
    expanded = new Set([key]);
    if (!details[key]) {
      try {
        const d = await api.getDetailBreakdown(periodValue(), oppDim, key);
        details = { ...details, [key]: d };
      } catch { details = { ...details, [key]: null }; }
    }
  }

  /** Token-composition rows for the detail expand (input/output/cache). */
  function tokenComposition(e: BreakdownEntry): { label: string; key: string; tokens: number }[] {
    return [
      { label: "输入", key: "input", tokens: e.input },
      { label: "输出", key: "output", tokens: e.output },
      { label: "缓存", key: "cache_read", tokens: e.cache_read },
    ];
  }

  const sorted = $derived.by(() => {
    const arr = [...entries];
    arr.sort((a, b) => {
      if (sort === "cost") return b.cost_usd - a.cost_usd;
      if (sort === "name") return a.key.localeCompare(b.key);
      return b.tokens - a.tokens;
    });
    return arr;
  });
</script>

<div class="bd-header">
  <span class="bd-title">{title}<span class="bd-count">{entries.length}</span></span>
  <div class="bd-sort">
    {#each [["token", "TOKEN"], ["cost", "成本"], ["name", "名称"]] as [k, label] (k)}
      <button class:on={sort === (k as SortKey)} onclick={() => (sort = k as SortKey)}>{label}</button>
    {/each}
  </div>
</div>

{#each sorted as e, i (e.key)}
  {@const meta = toolMeta(e.key)}
  {@const open = expanded.has(e.key)}
  {@const st = splitTokens(e.tokens)}
  {@const mv = dim === "model" ? modelVendor(e.key) : null}
  {@const vid = vendorIdForModel(e.key)}
  {@const rkIcon = (dim === "model" && vid) ? vendorIcon(vid) : meta.icon}
  {@const rkBg = (dim === "model" && vid && mv) ? mv.color : meta.color}
  <button type="button" class="bd-row" onclick={() => toggleExpand(e.key)}>
    <span class="rk" style="background:{rkBg};color:var(--badge-text)" title={meta.label}>{@html rkIcon}</span>
    <div class="bd-main">
      <div class="bd-name">
        <span class="bd-key">{meta.label}</span>
        {#if mv}<span class="bd-vendor" style="color:{mv.color}">{mv.vendor}</span>{/if}
        <span class="bd-cost">{formatCost(e.cost_usd, currency, cnyRate)}</span>
      </div>
      <div class="bd-meta">
        <BreakdownBar pct={e.token_pct} color={PALETTE[i % PALETTE.length]} />
        <span class="bd-pct">{e.token_pct.toFixed(1)}<span class="pct-u">%</span></span>
      </div>
    </div>
    <span class="bd-tokens">{st.value}<span class="tku">{st.unit}</span></span>
  </button>
  {#if open}
    <div class="bd-detail">
      <div class="det-row"><span>会话数</span><span class="det-val">{e.messages}</span></div>
      <div class="det-row"><span>成本比</span><span class="det-val">{e.cost_pct.toFixed(1)}%</span></div>
      <div class="det-sep"></div>
      {#each tokenComposition(e) as tc, ci (tc.key)}
        {@const tcp = e.tokens > 0 ? (tc.tokens / e.tokens) * 100 : 0}
        {@const tcs = splitTokens(tc.tokens)}
        <div class="det-row det-sub">
          <span class="det-label"><span class="det-dot" style="background:{PALETTE[ci % PALETTE.length]}"></span>{tc.label}</span>
          <div class="det-bar"><i style="width:{Math.max(2, tcp).toFixed(1)}%;background:{PALETTE[ci % PALETTE.length]}"></i></div>
          <span class="det-pct">{tcp.toFixed(1)}<span class="pct-u">%</span></span>
          <span class="det-tok">{tcs.value}<span class="tku">{tcs.unit}</span></span>
        </div>
      {/each}
      {#if details[e.key]}
        <div class="det-sep"></div>
        {#each details[e.key]!.entries.slice(0, 3) as de, j (de.key)}
          {@const detailMeta = oppDim === "model" ? toolMeta(de.key) : toolMeta(de.key)}
          {@const ds = splitTokens(de.tokens)}
          <div class="det-row det-sub">
            <span class="det-label"
              ><ToolIcon tool={de.key} badge={false} size={11} />{detailMeta.label}</span
            >
            <div class="det-bar"><i style="width:{Math.max(2, de.token_pct).toFixed(1)}%;background:{PALETTE[j % PALETTE.length]}"></i></div>
            <span class="det-pct">{de.token_pct.toFixed(1)}<span class="pct-u">%</span></span>
            <span class="det-tok">{ds.value}<span class="tku">{ds.unit}</span></span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
{/each}

<style>
  .bd-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 16px 12px;
  }
  .bd-title {
    font-size: 13px; color: var(--text-dim);
    display: flex; align-items: center; gap: 7px;
  }
  .bd-count {
    font-family: var(--font-mono); font-size: 11px; font-weight: 600;
    color: var(--amber); background: var(--amber-bg);
    padding: 1px 8px; border-radius: 10px;
    line-height: 1.4;
  }
  .bd-sort {
    display: inline-flex; gap: 1px;
    background: var(--glass-3); border-radius: 8px; padding: 2px;
  }
  .bd-sort button {
    background: transparent; border: none;
    color: var(--text-faint);
    font-family: var(--font-ui); font-size: 11px; font-weight: 600;
    padding: 4px 10px; border-radius: 6px;
    cursor: pointer;
  }
  .bd-sort button:hover { color: var(--text-dim); }
  .bd-sort button.on {
    background: var(--amber); color: var(--badge-text);
  }

  .bd-row {
    display: grid; grid-template-columns: 28px 1fr 60px;
    align-items: center; gap: 12px;
    padding: 9px 16px;
    cursor: pointer;
    /* button reset FIRST, then re-declare the divider so `border:none` shorthand
       doesn't clobber border-bottom (which wiped the row dividers). */
    background: none; border: none; font-family: inherit; text-align: left; width: 100%; font-size: inherit;
    border-bottom: 1px dashed var(--border-dim);
  }
  .bd-row:hover { background: rgba(232,176,75,.04); }

  .rk {
    width: 28px; height: 28px; border-radius: 7px;
    display: flex; align-items: center; justify-content: center;
    font-size: 12px; font-weight: 700; flex-shrink: 0;
  }
  .bd-main { min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .bd-name { display: flex; align-items: center; gap: 7px; }
  .bd-key { font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 140px; }
  .bd-vendor {
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
    display: inline-flex;
    align-items: center;
    gap: 3px;
    line-height: 1.5;
  }
  .bd-cost { font-size: 11px; color: var(--amber); flex-shrink: 0; margin-left: auto; }
  .bd-meta { display: flex; align-items: center; gap: 7px; }
  .bd-meta :global(.bar) { flex: 1; }
  .bd-pct { font-family: var(--font-mono); font-size: 11px; color: var(--text-dim); width: 42px; text-align: right; }
  .pct-u { font-size: 8px; margin-left: 1px; }
  .bd-tokens { font-family: var(--font-mono); font-size: 12px; color: var(--text-dim); text-align: right; }
  .tku { font-size: 8px; color: var(--text-faint); margin-left: 2px; font-weight: 600; }

  /* expanded detail — uses shared .bd-detail / .det-* from breakdown.css */
</style>
