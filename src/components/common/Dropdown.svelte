<script lang="ts">
  // Custom dropdown replacing native <select> — native option popup on
  // Windows WebView2 is a system control that ignores page CSS (always light),
  // making dark-theme text invisible. This component renders the options list
  // in a styled <div> that matches the app theme.
  type Opt = { value: string; label: string };

  let {
    value = $bindable(),
    options = [] as Opt[],
    disabled = false,
    invalid = false,
    class: cls = "",
    onChange,
  }: {
    value: string;
    options: Opt[];
    disabled?: boolean;
    invalid?: boolean;
    class?: string;
    onChange: (v: string) => void;
  } = $props();

  let open = $state(false);
  let triggerRef: HTMLElement | null = $state(null);

  const selected = $derived(options.find((o) => o.value === value));
  const label = $derived(selected?.label ?? "");

  function toggle() {
    if (!disabled) open = !open;
  }
  function pick(v: string) {
    value = v;
    open = false;
    onChange(v);
  }
  function close() {
    open = false;
    triggerRef?.focus();
  }

  // Close on Escape or click-outside.
  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
  function useClickOutside(node: HTMLElement) {
    const handler = (e: MouseEvent) => {
      if (!node.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", handler);
    return { destroy() { document.removeEventListener("mousedown", handler); } };
  }
</script>

<svelte:window onkeydown={onKey} />

<div
  class="dd-wrap {cls}"
  class:invalid
  class:dd-disabled={disabled}
  use:useClickOutside
  role="listbox"
>
  <button
    type="button"
    class="dd-trigger"
    class:dd-open={open}
    {disabled}
    aria-expanded={open}
    aria-haspopup="listbox"
    bind:this={triggerRef}
    onclick={toggle}
  >
    <span class="dd-label">{label}</span>
    <span class="dd-arrow">
      <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
        <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </span>
  </button>
  {#if open}
    <div class="dd-panel" role="listbox">
      {#each options as opt}
        <button
          type="button"
          class="dd-opt"
          class:dd-active={opt.value === value}
          role="option"
          aria-selected={opt.value === value}
          onclick={() => pick(opt.value)}
        >
          {opt.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* ── wrapper ── */
  .dd-wrap {
    position: relative;
    display: inline-flex;
    min-width: 130px;
    height: 32px;
  }
  .dd-wrap.invalid .dd-trigger {
    border-color: var(--coral) !important;
  }
  .dd-disabled { opacity: 0.45; pointer-events: none; }

  /* ── trigger button ── */
  .dd-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    height: 100%;
    padding: 0 10px;
    background: rgba(255,255,255,.03);
    border: 1px solid var(--border-dim);
    border-radius: 7px;
    color: var(--text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    box-sizing: border-box;
    user-select: none;
    transition: border-color 0.12s;
  }
  .dd-trigger:hover { border-color: var(--amber); }
  .dd-trigger:focus-visible { outline: none; border-color: var(--amber); box-shadow: 0 0 0 2px rgba(232,176,75,.2); }
  .dd-open { border-color: var(--amber); }

  .dd-label { text-align: left; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dd-arrow { display: flex; align-items: center; color: var(--text-faint); flex-shrink: 0; transition: transform 0.15s; }
  .dd-open .dd-arrow { transform: rotate(180deg); }

  /* ── dropdown panel ── */
  .dd-panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 1000;
    min-width: 100%;
    max-height: 260px;
    overflow-y: auto;
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    padding: 3px;
    box-shadow: 0 8px 24px rgba(0,0,0,.45);
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .dd-opt {
    display: block;
    width: 100%;
    text-align: left;
    padding: 5px 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-dim);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    white-space: nowrap;
    transition: background 0.08s, color 0.08s;
  }
  .dd-opt:hover {
    background: rgba(232,176,75,.1);
    color: var(--text);
  }
  .dd-active {
    background: rgba(232,176,75,.15);
    color: var(--amber);
    font-weight: 500;
  }

  /* ── Light theme overrides ── */
  :global([data-theme="light"] .dd-trigger) {
    background: rgba(0,0,0,.03);
  }
  :global([data-theme="light"] .dd-panel) {
    box-shadow: 0 8px 24px rgba(0,0,0,.12);
  }
  :global([data-theme="light"] .dd-opt:hover) {
    background: rgba(201,138,30,.08);
    color: var(--text);
  }
  :global([data-theme="light"] .dd-active) {
    background: rgba(201,138,30,.12);
    color: var(--amber);
  }
</style>
