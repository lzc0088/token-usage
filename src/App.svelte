<script lang="ts">
  // T0.2 scaffold placeholder. Real popover (Hero/SegBar/segments) lands in M3/M4
  // per docs/plan.md. This verifies the Tauri+Svelte+Vite pipeline end-to-end.
  let appVersion = "0.1.0-scaffold";
</script>

<div class="popover">
  <header class="hero">
    <span class="label">Token Usage</span>
    <span class="version">{appVersion}</span>
  </header>
  <main class="body">
    <p class="muted">脚手架就绪 · M0</p>
    <p class="hint">后续：tokscale 采集 → SQLite → popover 分段（M1–M4）</p>
  </main>
</div>

<style>
  .popover {
    display: flex;
    flex-direction: column;
    padding: 16px;
    gap: 12px;
  }
  .hero {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .label {
    font-size: 16px;
    font-weight: 600;
    color: var(--text);
  }
  .version {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--amber);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .muted {
    margin: 0;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
