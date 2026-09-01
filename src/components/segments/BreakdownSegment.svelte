<script lang="ts">
  // 用量细分 segment (T4.2). 泛型组件：dim="tool" | "model"
  import BreakdownList from "../common/BreakdownList.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Breakdown, type Currency } from "../../lib/api";
  import { periodValue } from "../../stores/period.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { COLLECTION_UPDATED } from "../../lib/events";

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
  let loadAttempted = $state(false);

  // `$derived` tracks the cross-module period `$state` (see stores/period).
  // Calling `periodValue()` directly inside `$effect` is NOT tracked.
  const activePeriod = $derived(periodValue());

  $effect(() => {
    const p = activePeriod;
    let cancelled = false;
    const fetch = async () => {
      try {
        const b = await api.getBreakdown(p, dim);
        if (!cancelled) { data = b; loadAttempted = true; }
      } catch {
        if (!cancelled) loadAttempted = true;
      }
    };
    fetch();
    const un = listen<void>(COLLECTION_UPDATED, () => { fetch(); });
    return () => {
      cancelled = true;
      un.then(fn => fn());
    };
  });
</script>

<div class="seg-body">
  {#if !data && !loadAttempted}
    <Skeleton type="list" rows={2} />
  {:else if !data}
    <EmptyState title={t("common.loadFailed")} />
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
