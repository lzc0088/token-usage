<script lang="ts">
  // 7-segment nav bar. V1 (T3.2) is visual + selection only; the segment views
  // (Overview/Tools/…) are built in M4.
  let active = $state("ov");

  const segments: { key: string; label: string }[] = [
    { key: "ov", label: "总览" },
    { key: "tools", label: "工具" },
    { key: "models", label: "模型" },
    { key: "projects", label: "项目" },
    { key: "trend", label: "趋势" },
    { key: "sess", label: "会话" },
    { key: "limit", label: "额度" },
  ];
</script>

<nav class="segbar">
  {#each segments as s (s.key)}
    <button class:active={active === s.key} onclick={() => (active = s.key)}>{s.label}</button>
  {/each}
</nav>

<style>
  .segbar {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border-dim);
    padding: 0 16px;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .segbar::-webkit-scrollbar {
    display: none;
  }
  .segbar button {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-faint);
    font-family: var(--font-ui);
    font-size: 12px;
    padding: 9px 8px 8px;
    cursor: pointer;
    white-space: nowrap;
    transition: 0.15s;
  }
  .segbar button:hover {
    color: var(--text-dim);
  }
  .segbar button.active {
    color: var(--amber);
    border-bottom-color: var(--amber);
  }
</style>
