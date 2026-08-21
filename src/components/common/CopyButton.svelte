<!--! Shared copy-to-clipboard icon button — SVG icon (no emoji), swaps to a
   checkmark for 2s after a successful copy. Used by Projects (full path) and
   Sessions (project path) so both read one interaction language. -->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";

  let {
    value,
    title = "",
    size = "sm",
  }: {
    /** Text written to the clipboard on click. */
    value: string;
    /** Accessible tooltip; defaults to the localized "Copy". */
    title?: string;
    /** sm = 24×20 compact chip; md = 30×26 (standalone rows). */
    size?: "sm" | "md";
  } = $props();

  let copied = $state(false);
  let failed = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    return () => { if (timer) clearTimeout(timer); };
  });

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      copied = true;
      failed = false;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { copied = false; }, 2000);
    } catch {
      failed = true;
      copied = false;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { failed = false; }, 2000);
    }
  }
</script>

<button
  type="button"
  class="cp-btn"
  class:copied
  class:failed
  class:md={size === "md"}
  onclick={(e) => { e.stopPropagation(); void copy(); }}
  title={title || t("common.copy")}
  aria-label={title || t("common.copy")}
>
  {#if copied}
    <!-- check -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"
         stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M20 6 9 17l-5-5" />
    </svg>
  {:else if failed}
    <!-- x mark -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2"
         stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  {:else}
    <!-- clipboard -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
         stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <rect x="8" y="2" width="8" height="4" rx="1" />
      <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
    </svg>
  {/if}
</button>

<style>
  .cp-btn {
    flex-shrink: 0;
    background: var(--surface-tint-strong);
    border: 1px solid var(--border-dim);
    border-radius: 4px;
    padding: 0;
    width: 24px;
    height: 20px;
    font-size: 11px;
    cursor: pointer;
    color: var(--text-dim);
    transition: all 0.15s;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .cp-btn.md {
    width: 30px;
    height: 26px;
    border-radius: 6px;
  }
  .cp-btn:hover {
    background: var(--amber-bg-strong);
    border-color: var(--amber);
    color: var(--amber);
  }
  .cp-btn:active {
    transform: scale(0.95);
  }
  .cp-btn.copied {
    color: var(--lime);
    border-color: var(--lime);
    background: var(--lime-bg);
  }
  .cp-btn.failed {
    color: var(--coral);
    border-color: var(--coral);
    background: var(--coral-bg);
  }
  .cp-btn svg {
    width: 12px;
    height: 12px;
  }
</style>
