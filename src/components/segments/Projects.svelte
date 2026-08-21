<script lang="ts">
  import { api, type Currency, type ProjectDetailRow, type ProjectVm } from "../../lib/api";
  import { PALETTE } from "../../lib/constants";
  import { formatCost, splitTokens } from "../../lib/format";
  import { toolMeta } from "../../lib/meta/tools";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../../lib/i18n.svelte";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import CopyButton from "../common/CopyButton.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import { periodValue } from "../../stores/period.svelte";
  import { COLLECTION_UPDATED } from "../../lib/events";

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let projects = $state<ProjectVm[] | null>(null);
  let expanded = $state<string | null>(null);
  let loading = $state(true);
  // Virtual pagination: the full project list is fetched in one request
  // (the backend snapshot path already holds it in memory), then sorted
  // client-side over the COMPLETE set. "Load more" just widens the render
  // window — no second network call — so the sort is correct (it operates
  // on the full list, not a token-ordered subset that would miss items).
  const RENDER_PAGE = 25;
  const MAX_PROJECTS = 500; // backend clamps limit to [1, 500]
  let renderCount = $state(RENDER_PAGE);

  // Sort key: token | cost | name | latest
  type SortKey = "token" | "cost" | "name" | "latest";
  let sort = $state<SortKey>("latest");

  function toggleExpand(key: string): void {
    expanded = expanded === key ? null : key;
  }

  /** Widen the render window — no network call, just shows more of the
   *  already-fetched + already-sorted full list. */
  function loadMore(): void {
    renderCount += RENDER_PAGE;
  }

  // On period change: fetch the full project set in one request. The `active`
  // flag discards a stale response if the user switched periods again before
  // it returned, so the old period's data can never overwrite the new one.
  $effect(() => {
    const p = periodValue();
    loading = true;
    renderCount = RENDER_PAGE;
    expanded = null;
    let active = true;
    const fetch = async () => {
      try {
        const data = await api.getProjects(p, 0, MAX_PROJECTS);
        if (!active) return;
        projects = data;
        loading = false;
      } catch {
        if (!active) return;
        projects = null;
        loading = false;
      }
    };
    fetch();
    const un = listen<void>(COLLECTION_UPDATED, () => { fetch(); });
    return () => {
      active = false;
      un.then(fn => fn());
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

  /** Render window into the full sorted list — virtual pagination. */
  const visible = $derived(sorted.slice(0, renderCount));
  const hasMore = $derived(sorted.length > renderCount);

  /** Max tokens across all projects — used as progress bar baseline. */
  const maxTokens = $derived(projects ? projects.reduce((max, p) => Math.max(max, p.tokens), 0) : 0);

  const palette = PALETTE;

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
    <span class="bd-title">{t("projects.title")}<span class="bd-count">{sorted.length}</span></span>
    <div class="bd-sort">
      {#each [["latest", t("projects.sortLatest")], ["token", t("projects.sortToken")], ["cost", t("projects.sortCost")], ["name", t("projects.sortName")]] as [k, label] (k)}
        <button class:on={sort === (k as SortKey)} aria-pressed={sort === (k as SortKey)} onclick={() => (sort = k as SortKey)}>{label}</button>
      {/each}
    </div>
  </div>

  {#if loading}
    <Skeleton type="list" rows={2} />
  {:else if projects === null}
    <EmptyState title={t("common.loadFailed")} />
  {:else if projects.length === 0}
    <EmptyState title={t("projects.empty")} />
  {:else}
    {#each visible as p, i (p.full_path ?? `${p.name}#${i}`)}
      {@const rowKey = p.full_path ?? `${p.name}#${i}`}
      {@const st = splitTokens(p.tokens)}
      {@const open = expanded === rowKey}
      <button type="button" class="prow" onclick={() => toggleExpand(rowKey)}>
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
          <span class="p-days">{p.messages}{t("breakdown.msgs")}</span>
        </div>
      </button>
      {#if open}
        <div class="p-detail">
          {#if p.full_path}
            <div class="det-row">
              <span>{t("projects.path")}</span>
              <div class="det-val-row">
                <span class="det-val ellipsis-left" title={p.full_path}>{ellipsisLeft(p.full_path)}</span>
                <CopyButton value={p.full_path} title={t("projects.copyPath")} />
              </div>
            </div>
          {/if}
          {#if p.latest_date}
            <div class="det-row">
              <span>{t("projects.lastActive")}</span><span class="det-val">{p.latest_date}</span>
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
    {#if hasMore}
      <div class="load-more-row">
        <button type="button" class="load-more-btn" onclick={loadMore}>
          {t("projects.loadMore")}
        </button>
      </div>
    {/if}
  {/if}
  </div>

<style>
  .seg-body {
    display: flex;
    flex-direction: column;
    width: 100%;
  }

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
    color: var(--amber); background: var(--amber-bg);
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
    color: var(--badge-text);
  }

  .prow {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 9px 16px;
    cursor: pointer;
    /* button reset FIRST, then re-declare the divider so `border:none` shorthand
       doesn't clobber border-bottom (which wiped the row dividers). */
    background: none; border: none; font-family: inherit; text-align: left; width: 100%; font-size: inherit;
    border-bottom: 1px dashed var(--border-dim);
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
    padding: 8px 24px 10px 24px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border-bottom: 1px solid var(--border-dim);
    background: var(--overlay-dark);
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
  .ellipsis-left {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl; /* ellipsis on LEFT */
  }
  /* Per-segment overrides on the shared .det-* base (breakdown.css) */
  .det-bar {
    height: 6px;
    border-radius: 3px;
  }
  .det-bar i {
    border-radius: 3px;
  }

  .load-more-row {
    display: flex;
    justify-content: center;
    padding: 16px;
  }
  .load-more-btn {
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 8px 24px;
    border-radius: 8px;
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: 0.15s;
  }
  .load-more-btn:hover:not(:disabled) {
    border-color: var(--amber);
    color: var(--amber);
  }
  .load-more-btn:disabled {
    cursor: default;
    opacity: 0.6;
  }
</style>
