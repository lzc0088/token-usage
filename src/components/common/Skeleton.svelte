<script lang="ts">
  // Unified loading skeleton. One pulsing block style (lifted from the old
  // Projects-only `.skel-*`), four layouts matching the segment shapes so each
  // tab's loading state previews its own structure instead of a generic spinner.
  let {
    type = "list",
    rows = 3,
  }: {
    /** Layout shape matching the segment that's loading. */
    type?: "list" | "cards" | "chart" | "overview" | "status";
    /** Row count for list/cards/overview/status layouts. */
    rows?: number;
  } = $props();

  const rowIndices = $derived(Array.from({ length: rows }, (_, i) => i));
  const overviewGridN = $derived(Math.min(2, rows));  // max 2 cells per row
  const overviewMiniN = $derived(rows >= 2 ? 2 : 0);
</script>

{#if type === "list"}
  <!-- Projects / Sessions / Breakdown: rank icon + two-line label + right values -->
  <div class="sk skel-list">
    {#each rowIndices as i (i)}
      <div class="skel-row">
        <div class="skel-icon"></div>
        <div class="skel-main">
          <div class="skel-line w60"></div>
          <div class="skel-line w40"></div>
        </div>
        <div class="skel-right">
          <div class="skel-line w30"></div>
          <div class="skel-line w20"></div>
        </div>
      </div>
    {/each}
  </div>
{:else if type === "cards"}
  <!-- Limits: vendor header + progress bar + value row -->
  <div class="sk skel-cards">
    {#each rowIndices as i (i)}
      <div class="skel-card">
        <div class="skel-card-head">
          <div class="skel-icon"></div>
          <div class="skel-line w40"></div>
          <div class="skel-line w20"></div>
        </div>
        <div class="skel-bar"></div>
        <div class="skel-line w50"></div>
      </div>
    {/each}
  </div>
{:else if type === "chart"}
  <!-- Trend: range label + 2 stat cells + chart area -->
  <div class="sk skel-chart">
    <div class="skel-line w20"></div>
    <div class="skel-stats">
      {#each [0, 1] as i (i)}
        <div class="skel-stat"><div class="skel-line w40"></div><div class="skel-big"></div></div>
      {/each}
    </div>
    <div class="skel-graph"></div>
  </div>
{:else if type === "overview"}
  <!-- Overview: IO 2-row grid + a list module + a quota card module -->
  <div class="sk skel-overview">
    <div class="skel-module">
      <div class="skel-grid2">
        {#each Array.from({ length: overviewGridN * 2 }, (_, i) => i) as i (i)}<div class="skel-cell"></div>{/each}
      </div>
    </div>
    {#if overviewMiniN > 0}
    <div class="skel-module">
      <div class="skel-line w25"></div>
      <div class="skel-mini-list">
        {#each Array.from({ length: overviewMiniN }, (_, i) => i) as i (i)}
          <div class="skel-row"><div class="skel-icon"></div><div class="skel-main"><div class="skel-line w60"></div></div><div class="skel-right"><div class="skel-line w25"></div></div></div>
        {/each}
      </div>
    </div>
    {/if}
    <div class="skel-module">
      <div class="skel-line w25"></div>
      <div class="skel-card"><div class="skel-card-head"><div class="skel-icon"></div><div class="skel-line w40"></div></div><div class="skel-bar"></div></div>
    </div>
  </div>
{:else if type === "status"}
  <!-- Status: header (title + count chip + re-check button) then rows shaped
       like the real ones — 28px icon left, name line + smaller meta line,
       status chip right -->
  <div class="sk skel-status">
    <div class="skel-status-head">
      <div class="skel-head-left">
        <div class="skel-line w-name"></div>
        <div class="skel-count"></div>
      </div>
      <div class="skel-btn"></div>
    </div>
    {#each rowIndices as i (i)}
      <div class="skel-status-row">
        <div class="skel-icon-xl"></div>
        <div class="skel-main">
          <div class="skel-line w-name"></div>
          <div class="skel-line w-meta sm"></div>
        </div>
        <div class="skel-chip"></div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .sk { width: 100%; }

  /* ── pulsing block primitive ── */
  .skel-icon,
  .skel-line,
  .skel-bar,
  .skel-big,
  .skel-cell,
  .skel-graph,
  .skel-card {
    background: var(--surface-tint-strong);
    border-radius: 6px;
    animation: skel-pulse 1.4s ease-in-out infinite;
  }
  .skel-line { height: 10px; border-radius: 5px; }
  .skel-icon { width: 20px; height: 20px; border-radius: 50%; flex-shrink: 0; }
  .skel-bar { height: 8px; border-radius: 4px; }
  .skel-big { height: 20px; width: 70%; border-radius: 5px; margin-top: 6px; }
  .skel-graph { height: 130px; border-radius: 8px; }
  .skel-cell { height: 52px; border-radius: 8px; }
  .w20 { width: 20%; }
  .w25 { width: 25%; }
  .w30 { width: 30%; }
  .w40 { width: 40%; }
  .w50 { width: 50%; }
  .w60 { width: 60%; }

  @keyframes skel-pulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }

  /* ── list layout ── */
  .skel-list { display: flex; flex-direction: column; }
  .skel-row {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: start;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px dashed var(--border-dim);
  }
  .skel-row:last-child { border-bottom: none; }
  .skel-main { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .skel-main .skel-line { max-width: 220px; }
  .skel-right { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; }

  /* ── cards layout (Limits) ── */
  .skel-cards { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px 18px; }
  .skel-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border-dim);
    border-radius: 10px;
  }
  .skel-card-head { display: flex; align-items: center; gap: 8px; }

  /* ── chart layout (Trend) ── */
  .skel-chart { display: flex; flex-direction: column; gap: 12px; padding: 16px; }
  .skel-stats { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; }
  .skel-stat { display: flex; flex-direction: column; }

  /* ── overview layout ── */
  .skel-overview { display: flex; flex-direction: column; padding: 14px 18px; }
  .skel-module {
    margin-bottom: 14px;
    padding-bottom: 14px;
    border-bottom: 1px solid var(--border-dim);
  }
  .skel-module:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }
  .skel-grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .skel-mini-list { display: flex; flex-direction: column; margin-top: 10px; }
  .skel-overview .skel-row { padding: 8px 0; }

  /* ── status layout (Status segment) ──
     Mirrors the real rows: header with title + count pill + button; rows with
     a 28px rounded icon left, name line over a smaller meta line, and a
     status chip on the right. Same paddings/borders as the live list so the
     skeleton→content swap doesn't shift. */
  .skel-status { display: flex; flex-direction: column; }
  .skel-status-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 0 8px;
  }
  .skel-head-left { display: flex; align-items: center; gap: 6px; }
  .skel-count {
    width: 18px;
    height: 14px;
    border-radius: 20px;
    background: var(--surface-tint-strong);
    animation: skel-pulse 1.4s ease-in-out infinite;
    flex-shrink: 0;
  }
  .skel-btn {
    width: 64px;
    height: 22px;
    border-radius: 6px;
    background: var(--surface-tint-strong);
    animation: skel-pulse 1.4s ease-in-out infinite;
    flex-shrink: 0;
  }
  .skel-status-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 0;
    border-bottom: 1px dashed var(--border-dim);
  }
  .skel-status-row:last-child { border-bottom: none; }
  /* Push the status chip to the row's right edge, like the live layout. */
  .skel-status-row .skel-main { flex: 1; }
  .skel-icon-xl {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--surface-tint-strong);
    animation: skel-pulse 1.4s ease-in-out infinite;
    flex-shrink: 0;
  }
  .skel-line.sm { height: 8px; }
  .w-name { width: 90px; }
  .w-meta { width: 140px; }
  .skel-chip {
    width: 48px;
    height: 18px;
    border-radius: 5px;
    background: var(--surface-tint-strong);
    animation: skel-pulse 1.4s ease-in-out infinite;
    flex-shrink: 0;
  }
</style>
