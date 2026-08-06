<script lang="ts">
  // 用量细分 segment (T4.2). 泛型组件：dim="tool" | "model"
  import BreakdownList from "../common/BreakdownList.svelte";
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
  {#if data}
    <BreakdownList entries={data.entries} {currency} {cnyRate} {title} {dim} />
  {:else}
    <p class="loading">{t("projects.loading")}</p>
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
