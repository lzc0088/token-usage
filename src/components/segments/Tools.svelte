<script lang="ts">
  // 工具 segment (T4.2). Loads get_breakdown(period, "tool") into BreakdownList.
  import BreakdownList from "../common/BreakdownList.svelte";
  import { api, type Breakdown, type Currency } from "../../lib/api";
  import { periodValue } from "../../stores/period.svelte";

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let data = $state<Breakdown | null>(null);

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const b = await api.getBreakdown(p, "tool");
        if (!cancelled) data = b;
      } catch (e) {
        console.error("tools breakdown failed", e);
        if (!cancelled) data = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="seg-body">
  {#if data}
    <BreakdownList entries={data.entries} {currency} {cnyRate} />
  {:else}
    <p class="loading">加载中…</p>
  {/if}
</div>

<style>
  .seg-body {
    display: flex;
    flex-direction: column;
  }
  .loading {
    padding: 18px 16px;
    color: var(--text-faint);
    font-size: 11px;
  }
</style>
