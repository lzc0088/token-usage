<script lang="ts">
  // 7-segment nav bar. Writes the global segment store; App renders the
  // matching view. Only 总览 is built in T4.1; others fall back to a placeholder.
  import { getSegment, setSegment } from "../../stores/segment.svelte";

  const segments: { key: string; label: string }[] = [
    { key: "ov", label: "总览" },
    { key: "tools", label: "工具" },
    { key: "models", label: "模型" },
    { key: "projects", label: "项目" },
    { key: "sess", label: "会话" },
    { key: "trend", label: "趋势" },
    { key: "limit", label: "额度" },
  ];

  let active = $derived(getSegment());
</script>

<nav class="segbar">
  {#each segments as s (s.key)}
    <button class:active={active === s.key} onclick={() => setSegment(s.key)}>{s.label}</button>
  {/each}
</nav>

<style>
  .segbar {
    display: flex;
    border-bottom: 1px solid var(--border-dim);
    padding: 0 12px;
    overflow-x: auto;
    scrollbar-width: none;
    flex-shrink: 0;
  }
  .segbar::-webkit-scrollbar {
    display: none;
  }
  .segbar button {
    flex: 1;
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-faint);
    font-family: var(--font-ui);
    font-size: 13px;
    font-weight: 500;
    padding: 16px 4px 14px;
    cursor: pointer;
    white-space: nowrap;
    transition: 0.15s;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .segbar button:hover {
    color: var(--text-dim);
  }
  .segbar button.active {
    color: var(--amber);
    font-weight: 700;
    border-bottom-color: var(--amber);
  }
</style>
