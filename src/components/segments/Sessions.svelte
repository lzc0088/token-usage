<script lang="ts">
  // 会话 segment (T4.3). Loads get_sessions() (ordered by tokens desc).
  // session_id is shown truncated; full id on hover via title.
  import { api, type Currency, type SessionVm } from "../../lib/api";
  import { formatCost, formatTokens } from "../../lib/format";

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let sessions = $state<SessionVm[] | null>(null);

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await api.getSessions();
        if (!cancelled) sessions = s;
      } catch (e) {
        console.error("sessions failed", e);
        if (!cancelled) sessions = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  function short(id: string): string {
    return id.length > 8 ? `${id.slice(0, 8)}…` : id;
  }
</script>

<div class="seg-body">
  {#if sessions === null}
    <p class="loading">加载中…</p>
  {:else if sessions.length === 0}
    <p class="empty">暂无会话数据</p>
  {:else}
    {#each sessions as s, i (`${s.tool}:${s.session_id}:${s.model}`)}
      <div class="srow">
        <span class="rk">{i + 1}</span>
        <div class="s-main">
          <div class="s-top">
            <span class="s-tool">{s.tool}</span>
            <span class="s-id" title={s.session_id}>{short(s.session_id)}</span>
          </div>
          <div class="s-model">{s.model}</div>
        </div>
        <div class="s-meta">
          <span class="s-cost">{formatCost(s.cost_usd, currency, cnyRate)}</span>
          <span class="s-tokens">{formatTokens(s.tokens)}</span>
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
  .srow {
    display: grid;
    grid-template-columns: 18px 1fr auto;
    align-items: center;
    gap: 9px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border-dim);
  }
  .srow:last-child {
    border-bottom: none;
  }
  .rk {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .s-main {
    min-width: 0;
  }
  .s-top {
    display: flex;
    align-items: baseline;
    gap: 7px;
  }
  .s-tool {
    font-size: 12px;
    color: var(--text);
  }
  .s-id {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .s-model {
    font-size: 10px;
    color: var(--text-faint);
    margin-top: 1px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .s-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 1px;
    flex-shrink: 0;
  }
  .s-cost {
    font-size: 11px;
    color: var(--amber);
  }
  .s-tokens {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
  }
</style>
