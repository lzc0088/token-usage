<script lang="ts">
  // 项目 segment (T4.3). Loads get_projects(). V1 returns empty until a richer
  // ingest populates sessions.project_path (tokscale's report shape doesn't
  // surface it); the query is correct and ready for when that lands.
  import { api, type Currency, type ProjectVm } from "../../lib/api";
  import { formatCost, formatTokens } from "../../lib/format";

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let projects = $state<ProjectVm[] | null>(null);

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const p = await api.getProjects();
        if (!cancelled) projects = p;
      } catch (e) {
        console.error("projects failed", e);
        if (!cancelled) projects = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  function dirName(path: string): string {
    const parts = path.replace(/\/+$/, "").split("/");
    return parts[parts.length - 1] || path;
  }
</script>

<div class="seg-body">
  {#if projects === null}
    <p class="loading">加载中…</p>
  {:else if projects.length === 0}
    <p class="empty">暂无项目数据<br /><span class="hint">tokscale 尚未提供会话的项目归属</span></p>
  {:else}
    {#each projects as p (p.path)}
      <div class="prow">
        <div class="p-main">
          <div class="p-name">{dirName(p.path)}</div>
          <div class="p-path">{p.path}</div>
        </div>
        <div class="p-meta">
          <span class="p-sessions">{p.session_count} 会话</span>
          <span class="p-cost">{formatCost(p.cost_usd, currency, cnyRate)}</span>
          <span class="p-tokens">{formatTokens(p.tokens)}</span>
        </div>
      </div>
    {/each}
  {/if}
</div>

<style>
  .seg-body {
    display: flex;
    flex-direction: column;
  }
  .loading,
  .empty {
    padding: 24px 16px;
    color: var(--text-faint);
    font-size: 12px;
    text-align: center;
  }
  .empty .hint {
    font-size: 10.5px;
    color: var(--text-faint);
  }
  .prow {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 11px 16px;
    border-bottom: 1px solid var(--border-dim);
  }
  .prow:last-child {
    border-bottom: none;
  }
  .p-main {
    min-width: 0;
  }
  .p-name {
    font-size: 12.5px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .p-path {
    font-family: var(--font-mono);
    font-size: 9.5px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 1px;
  }
  .p-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    flex-shrink: 0;
  }
  .p-sessions {
    font-size: 9.5px;
    color: var(--text-faint);
  }
  .p-cost {
    font-size: 11px;
    color: var(--amber);
  }
  .p-tokens {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
  }
</style>
