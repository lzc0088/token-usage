<script lang="ts">
  // Horizontal proportional bar with gradient fill using CSS color-mix.
  let { pct, color = "var(--amber)" }: { pct: number; color?: string } = $props();
  let width = $derived(`${Math.max(2, Math.min(100, pct)).toFixed(1)}%`);
</script>

<div class="bar">
  <i style="width:{width};background:linear-gradient(90deg,{color},color-mix(in srgb,{color} 30%,transparent))"></i>
</div>

<style>
  .bar {
    height: 6px;
    background: var(--bar-track);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
  }
</style>
