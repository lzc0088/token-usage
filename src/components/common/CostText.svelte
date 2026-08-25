<script lang="ts">
  // Cost amount rendered per the popover unit rules: currency symbol SMALLER
  // than the digits and to their LEFT ("both" → ¥…/$…), no space between unit
  // and value. Inherits font-size/color/mono from the parent span, so it drops
  // into any row style. Trend page achieves the same via splitCost directly.
  import type { Currency } from "../../lib/api";
  import { splitCost } from "../../lib/format";

  let {
    usd,
    currency,
    cnyRate = 7.2,
  }: { usd: number; currency: Currency; cnyRate?: number } = $props();

  const parts = $derived(splitCost(usd, currency, cnyRate));
</script>

{#each parts as part, i (i)}{part.sep ?? ""}<span class="cu" aria-hidden="true">{part.unit}</span>{part.value}{/each}

<style>
  .cu {
    font-size: 0.75em;
    font-weight: 700;
  }
</style>
