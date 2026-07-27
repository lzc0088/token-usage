<script lang="ts">
  import { api, type Currency, type ProjectDetailRow, type ProjectVm } from "../../lib/api";
  import { formatCost, splitTokens } from "../../lib/format";
  import { toolMeta } from "../../lib/toolMeta";
  import ToolIcon from "../../lib/ToolIcon.svelte";
  import { periodValue } from "../../stores/period.svelte";

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let projects = $state<ProjectVm[] | null>(null);
  let expanded = $state<string | null>(null);
  let copyFeedback = $state<string | null>(null);
  let loading = $state(true);

  // Sort key: token | cost | name | latest
  type SortKey = "token" | "cost" | "name" | "latest";
  let sort = $state<SortKey>("latest");

  function toggleExpand(key: string): void {
    expanded = expanded === key ? null : key;
  }

  /** Copy full project path to clipboard */
  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path);
      copyFeedback = path;
      setTimeout(() => {
        copyFeedback = null;
      }, 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  }

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    loading = true;
    (async () => {
      try {
        const data = await api.getProjects(p);
        if (!cancelled) {
          projects = data;
          loading = false;
          expanded = null;
        }
      } catch {
        if (!cancelled) { projects = null; loading = false; }
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  const sorted = $derived.by(() => {
    const arr = projects ? [...projects] : [];
    arr.sort((a, b) => {
      if (sort === "cost") return b.cost_usd - a.cost_usd;
      if (sort === "name") return a.name.localeCompare(b.name, undefined);
      if (sort === "latest") {
        const ad = a.latest_date ?? "9999-99-99";
        const bd = b.latest_date ?? "9999-99-99";
        return bd.localeCompare(ad);
      }
      return b.tokens - a.tokens;
    });
    return arr;
  });

  /** Max tokens across all projects — used as progress bar baseline. */
  const maxTokens = $derived(projects ? projects.reduce((max, p) => Math.max(max, p.tokens), 0) : 0);

  const palette = ["var(--amber)", "var(--lime)", "var(--cyan)", "var(--violet)", "var(--coral)"];

  /** Left-ellipsis a long path so the rightmost part (leaf name) stays visible. */
  function ellipsisLeft(full: string | null): string {
    if (!full) return "—";
    const cleaned = full.replace(/^~\//, "");
    if (cleaned.length <= 80) return cleaned;
    const parts = cleaned.split("/");
    const leaf = parts.pop() ?? "";
    const prefix = ".../" + (parts.length > 0 ? parts.join("/") + "/" : "");
    return prefix + leaf;
  }

  function rowTokens(r: ProjectDetailRow): { value: string; unit: string } {
    return splitTokens(r.tokens);
  }
</script>

<div class="seg-body">
  <div class="bd-header">
    <span class="bd-title">项目列表<span class="bd-count">{sorted.length}</span></span>
    <div class="bd-sort">
      {#each [["latest", "最近"], ["token", "TOKEN"], ["cost", "成本"], ["name", "名称"]] as [k, label] (k)}
        <button class:on={sort === (k as SortKey)} onclick={() => (sort = k as SortKey)}>{label}</button>
      {/each}
    </div>
  </div>

  {#if loading}
    <div class="skel-list">
      {#each [1,2,3] as _}
        <div class="skel-row">
          <div class="skel-icon"></div>
          <div class="skel-main">
            <div class="skel-line skel-w60"></div>
            <div class="skel-line skel-w40"></div>
          </div>
          <div class="skel-right">
            <div class="skel-line skel-w30"></div>
            <div class="skel-line skel-w20"></div>
          </div>
        </div>
      {/each}
    </div>
  {:else if projects === null}
    <p class="empty">加载失败，请重试</p>
  {:else if projects.length === 0}
    <p class="empty">暂无项目数据</p>
  {:else}
    {#each sorted as p, i (p.full_path ?? `${p.name}#${i}`)}
      {@const rowKey = p.full_path ?? `${p.name}#${i}`}
      {@const st = splitTokens(p.tokens)}
      {@const open = expanded === rowKey}
      <div
        class="prow"
        role="button"
        tabindex="0"
        onclick={() => toggleExpand(rowKey)}
        onkeydown={(e: KeyboardEvent) => e.key === "Enter" && toggleExpand(rowKey)}
      >
        <span class="pk">📁</span>
        <div class="p-main">
          <div class="p-top">
            <span class="p-name">{p.name}</span>
            <span class="p-cost">{formatCost(p.cost_usd, currency, cnyRate)}</span>
          </div>
          <div class="p-bar-row">
            <div class="br">
              <i style="width:{Math.max(2, (p.tokens / (maxTokens || 1)) * 100).toFixed(1)}%;background:{palette[i % palette.length]}"></i>
            </div>
            <span class="p-pct">{(p.tokens / (maxTokens || 1) * 100).toFixed(1)}<span class="pct-u">%</span></span>
          </div>
        </div>
        <div class="p-meta">
          <span class="p-tokens">{st.value}<span class="tku">{st.unit}</span></span>
          <span class="p-days">{p.messages} 条</span>
        </div>
      </div>
      {#if open}
        <div class="p-detail">
          {#if p.full_path}
            <div class="det-row">
              <span>项目路径</span>
              <div class="det-val-row">
                <span class="det-val ellipsis-left" title={p.full_path}>{ellipsisLeft(p.full_path)}</span>
                <button
                  class="copy-btn"
                  onclick={() => copyPath(p.full_path!)}
                  title="复制完整路径"
                  aria-label="复制项目路径"
                >
                  {#if copyFeedback === p.full_path}
                    ✓
                  {:else}
                    📋
                  {/if}
                </button>
              </div>
            </div>
          {/if}
          {#if p.latest_date}
            <div class="det-row">
              <span>最近活跃</span><span class="det-val">{p.latest_date}</span>
            </div>
          {/if}
          {#if p.models.length > 0}
            <div class="det-sep"></div>
            {#each p.models as me, j (me.key + j)}
              {@const mm = toolMeta(me.key)}
              {@const ms = rowTokens(me)}
              <div class="det-row det-sub">
                <span class="det-label" style="display:flex;align-items:center;gap:4px"
                  ><ToolIcon tool={me.key} badge={false} size={11} />{mm.label}</span
                >
                <div class="det-bar">
                  <i style="width:{Math.max(2, me.pct).toFixed(1)}%;background:{palette[j % palette.length]}"></i>
                </div>
                <span class="det-pct">{me.pct.toFixed(1)}<span class="pct-u">%</span></span>
                <span class="det-tok">{ms.value}<span class="tku">{ms.unit}</span></span>
              </div>
            {/each}
          {/if}
          {#if p.tools.length > 0}
            <div class="det-sep"></div>
            {#each p.tools as te, j (te.key + j)}
              {@const tm = toolMeta(te.key)}
              {@const ts = rowTokens(te)}
              <div class="det-row det-sub">
                <span class="det-label" style="display:flex;align-items:center;gap:4px"
                  ><ToolIcon tool={te.key} badge={false} size={11} />{tm.label}</span
                >
                <div class="det-bar">
                  <i style="width:{Math.max(2, te.pct).toFixed(1)}%;background:{palette[(j + 2) % palette.length]}"></i>
                </div>
                <span class="det-pct">{te.pct.toFixed(1)}<span class="pct-u">%</span></span>
                <span class="det-tok">{ts.value}<span class="tku">{ts.unit}</span></span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    {/each}
  {/if}
  </div>

<style>
  .seg-body {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .empty {
    padding: 24px 16px;
    color: var(--text-faint);
    font-size: 12px;
    text-align: center;
  }

  /* ── Skeleton loading (matches BreakdownList .bd-row) ── */
  .skel-list {
    display: flex;
    flex-direction: column;
    width: 100%;
  }
  .skel-row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    border-bottom: 1px dashed var(--border-dim);
    width: 100%;
  }
  .skel-row:last-child { border-bottom: none; }
  .skel-icon {
    width: 28px; height: 28px; border-radius: 6px;
    background: var(--surface-tint-strong);
    flex-shrink: 0;
  }
  .skel-main {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .skel-right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    min-width: 70px;
    justify-content: flex-end;
  }
  .skel-line {
    height: 10px;
    border-radius: 4px;
    background: var(--surface-tint-strong);
  }
  .skel-w60 { width: 60%; }
  .skel-w40 { width: 40%; }
  .skel-w30 { width: 30%; }
  .skel-w20 { width: 20%; }
  .skel-icon, .skel-line { animation: skel-pulse 1.4s ease-in-out infinite; }
  @keyframes skel-pulse { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }

  .bd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px 12px;
  }
  .bd-title {
    font-size: 13px; color: var(--text-dim);
    display: flex; align-items: center; gap: 7px;
  }
  .bd-count {
    font-family: var(--font-mono); font-size: 11px; font-weight: 600;
    color: var(--amber); background: rgba(232,176,75,.12);
    padding: 1px 8px; border-radius: 10px;
    line-height: 1.4;
  }
  .bd-sort {
    display: inline-flex;
    gap: 1px;
    background: var(--glass-3);
    border-radius: 8px;
    padding: 2px;
  }
  .bd-sort button {
    background: transparent;
    border: none;
    color: var(--text-faint);
    font-family: var(--font-ui);
    font-size: 11px;
    font-weight: 600;
    padding: 4px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .bd-sort button:hover {
    color: var(--text-dim);
  }
  .bd-sort button.on {
    background: var(--amber);
    color: #1a1408;
  }

  .prow {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    border-bottom: 1px dashed var(--border-dim);
    cursor: pointer;
  }
  .prow:last-child {
    border-bottom: none;
  }
  .prow:hover {
    background: rgba(232, 176, 75, 0.04);
  }
  .pk {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 18px;
    flex-shrink: 0;
  }
  .p-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .p-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .p-name {
    font-size: 13px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }
  .p-cost {
    font-size: 11px;
    color: var(--amber);
    flex-shrink: 0;
  }
  .p-bar-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .br {
    flex: 1;
    height: 4px;
    background: var(--bar-track);
    border-radius: 2px;
    overflow: hidden;
  }
  .br i {
    display: block;
    height: 100%;
    border-radius: 2px;
  }
  .p-pct {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-dim);
    width: 42px;
    text-align: right;
  }
  .pct-u {
    font-size: 8px;
    margin-left: 1px;
  }
  .p-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    flex-shrink: 0;
  }
  .p-tokens {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-dim);
  }
  .tku {
    font-size: 8px;
    color: var(--text-faint);
    margin-left: 2px;
    font-weight: 600;
  }
  .p-days {
    font-size: 11px;
    color: var(--text-faint);
    text-align: right;
  }

  /* expand detail — matches BreakdownList */
  .p-detail {
    padding: 8px 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-bottom: 1px solid var(--border-dim);
    background: rgba(0, 0, 0, 0.08);
  }
  .det-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 10px;
    color: var(--text-faint);
    gap: 8px;
  }
  .det-row > span:first-child {
    flex-shrink: 0;
    white-space: nowrap;
  }
  .det-val-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .det-val {
    color: var(--text-dim);
    font-family: var(--font-mono);
    font-size: 10px;
    text-align: right;
    flex-shrink: 1;
    min-width: 0;
  }
  .copy-btn {
    flex-shrink: 0;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--border-dim);
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 11px;
    cursor: pointer;
    color: var(--text-dim);
    transition: all 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    min-width: 24px;
    height: 20px;
  }
  .copy-btn:hover {
    background: rgba(232, 176, 75, 0.15);
    border-color: var(--amber);
    color: var(--amber);
  }
  .copy-btn:active {
    transform: scale(0.95);
  }
  .ellipsis-left {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl; /* ellipsis on LEFT */
  }
  .det-sep {
    height: 0;
    border-top: 1px solid var(--border-dim);
    margin: 8px 0;
  }
  .det-sub {
    font-size: 11px;
    align-items: center;
    display: grid;
    grid-template-columns: 1fr 100px 50px 55px;
    gap: 8px;
  }
  .det-label {
    display: flex;
    align-items: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .det-bar {
    height: 6px;
    background: var(--bar-track);
    border-radius: 3px;
    overflow: hidden;
    align-self: center;
  }
  .det-bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
  }
  .det-pct {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
    text-align: right;
  }
  .det-tok {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
    text-align: right;
  }
</style>
