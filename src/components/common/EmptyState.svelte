<script lang="ts">
  // Unified empty / loading state for every tab segment. Centered, with an
  // optional faint line icon + main copy + hint + optional action button.
  // Replaces the per-segment `.empty` / `.loading` markup that had drifted out
  // of sync (different padding, some not even centered).
  let {
    title,
    hint = "",
    icon = "empty",
    compact = false,
    actionLabel = "",
    onAction = () => {},
  }: {
    /** Main copy, e.g. "暂无项目数据". Required. */
    title: string;
    /** Secondary line under the title (optional). */
    hint?: string;
    /** "empty" (inbox icon) | "loading" (pulsing dots) | "off" (no icon). */
    icon?: "empty" | "loading" | "off";
    /** Tighter padding/icon for small card sub-regions (Overview modules). */
    compact?: boolean;
    /** Optional inline link button rendered under the hint. */
    actionLabel?: string;
    onAction?: () => void;
  } = $props();
</script>

<div class="empty-state" class:compact>
  {#if icon === "empty"}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
         stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M22 12h-6l-2 3h-4l-2-3H2" />
      <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
    </svg>
  {:else if icon === "loading"}
    <svg class="dots" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <circle cx="5" cy="12" r="2" class="dot d1" />
      <circle cx="12" cy="12" r="2" class="dot d2" />
      <circle cx="19" cy="12" r="2" class="dot d3" />
    </svg>
  {/if}

  <p class="es-title">{title}</p>
  {#if hint}<p class="es-hint">{hint}</p>{/if}
  {#if actionLabel}
    <button type="button" class="es-action" onclick={onAction}>{actionLabel}</button>
  {/if}
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 40px 16px;
    gap: 6px;
  }
  .empty-state.compact {
    padding: 24px 12px;
    gap: 4px;
  }

  .empty-state svg {
    width: 28px;
    height: 28px;
    color: var(--text-faint);
    opacity: 0.45;
    margin-bottom: 4px;
  }
  .empty-state.compact svg {
    width: 22px;
    height: 22px;
    margin-bottom: 2px;
  }

  /* Loading dots — staggered pulse. */
  .dot { animation: es-pulse 1.2s infinite ease-in-out; }
  .d2 { animation-delay: 0.2s; }
  .d3 { animation-delay: 0.4s; }
  @keyframes es-pulse {
    0%, 80%, 100% { opacity: 0.25; }
    40% { opacity: 1; }
  }

  .es-title {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
  }
  .es-hint {
    margin: 0;
    color: var(--text-faint);
    font-size: 11px;
    line-height: 1.5;
  }
  .es-action {
    margin-top: 6px;
    background: none;
    border: none;
    color: var(--amber);
    cursor: pointer;
    font-family: inherit;
    font-size: 12px;
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
    transition: opacity 0.15s;
  }
  .es-action:hover { opacity: 0.75; }
</style>
