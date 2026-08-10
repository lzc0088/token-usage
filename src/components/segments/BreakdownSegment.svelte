<script lang="ts">
  // 用量细分 segment (T4.2). 泛型组件：dim="tool" | "model"
  import BreakdownList from "../common/BreakdownList.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import { api, type Breakdown, type Currency } from "../../lib/api";
  import { periodValue } from "../../stores/period.svelte";
  import { t } from "../../lib/i18n.svelte";

  let {
    dim,
    currency,
    cnyRate = 7.2,
    title,
  }: {
    dim: "tool" | "model";
    currency: Currency;
    cnyRate?: number;
    title: string;
  } = $props();

  let data = $state<Breakdown | null>(null);

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const b = await api.getBreakdown(p, dim);
        if (!cancelled) data = b;
      } catch {
        if (!cancelled) data = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="seg-body">
  {#if !data}
    <Skeleton type="list" />
  {:else if data.entries.length === 0}
    <EmptyState title={t("breakdown.noData")} />
  {:else}
    <BreakdownList entries={data.entries} {currency} {cnyRate} {title} {dim} />
  {/if}
</div>

<style>
  .seg-body {
    display: flex;
    flex-direction: column;
  }
</style>
