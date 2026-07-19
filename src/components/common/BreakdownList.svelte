<script lang="ts">
  // Reusable ranked breakdown list (工具 / 模型 segments share this).
  // Sort toggle: by token / by cost / by name. Each row: rank, name + cost,
  // proportional token bar + pct, token count.
  import BreakdownBar from "./BreakdownBar.svelte";
  import { formatCost, formatTokens } from "../../lib/format";
  import type { BreakdownEntry, Currency } from "../../lib/api";

  let {
    entries,
    currency,
    cnyRate = 7.2,
  }: { entries: BreakdownEntry[]; currency: Currency; cnyRate?: number } = $props();

  const PALETTE = ["var(--amber)", "var(--lime)", "var(--cyan)", "var(--violet)", "var(--coral)"];

  type SortKey = "token" | "cost" | "name";
  let sort = $state<SortKey>("token");

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

<div class="bd-sort">
  {#each [["token", "按 token"], ["cost", "按成本"], ["name", "按名称"]] as [k, label] (k)}
    <button class:on={sort === (k as SortKey)} onclick={() => (sort = k as SortKey)}>{label}</button>
  {/each}
</div>

{#each sorted as e, i (e.key)}
  <div class="bd-row">
    <span class="rk">{i + 1}</span>
    <div class="bd-main">
      <div class="bd-name">
        <span class="bd-key">{e.key}</span>
        <span class="bd-cost">{formatCost(e.cost_usd, currency, cnyRate)}</span>
      </div>
      <div class="bd-meta">
        <BreakdownBar pct={e.token_pct} color={PALETTE[i % PALETTE.length]} />
        <span class="bd-pct">{e.token_pct.toFixed(0)}%</span>
      </div>
    </div>
    <span class="bd-tokens">{formatTokens(e.tokens)}</span>
  </div>
{:else}
  <p class="empty">无数据</p>
{/each}

<style>
  .bd-sort {
    display: flex;
    gap: 4px;
    padding: 10px 16px 6px;
  }
  .bd-sort button {
    background: transparent;
    border: 1px solid var(--border-dim);
    color: var(--text-faint);
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 10.5px;
    font-family: inherit;
    cursor: pointer;
    transition: 0.15s;
  }
  .bd-sort button.on {
    color: var(--amber);
    border-color: var(--amber-soft);
    background: rgba(232, 176, 75, 0.08);
  }
  .bd-row {
    display: grid;
    grid-template-columns: 18px 1fr 56px;
    align-items: center;
    gap: 9px;
    padding: 9px 16px;
    border-bottom: 1px solid var(--border-dim);
  }
  .bd-row:last-child {
    border-bottom: none;
  }
  .rk {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
  .bd-main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bd-name {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .bd-key {
    font-size: 12px;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bd-cost {
    font-size: 10.5px;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .bd-meta {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .bd-meta :global(.bar) {
    flex: 1;
  }
  .bd-pct {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-dim);
    width: 30px;
    text-align: right;
  }
  .bd-tokens {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-dim);
    text-align: right;
  }
  .empty {
    padding: 18px 16px;
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
