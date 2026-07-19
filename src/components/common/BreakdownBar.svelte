<script lang="ts">
  // Horizontal proportional bar for a breakdown row (tokens vs grand total).
  let { pct, color = "var(--amber)" }: { pct: number; color?: string } = $props();
  let width = $derived(`${Math.max(2, Math.min(100, pct)).toFixed(1)}%`);
</script>

<div class="bar"><i style="width:{width};background:{color}"></i></div>

<style>
  .bar {
    height: 6px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
  }
</style>
