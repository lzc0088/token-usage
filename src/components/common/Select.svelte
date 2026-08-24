<script lang="ts" module>
  // Shared option shape used by Select.svelte.
  export interface SelectOption {
    value: string;
    label: string;
  }
</script>

<script lang="ts">
  // Drop-in <select> replacement with a dark popup on Windows.
  //
  // Why: Windows WebView2 renders the native <select> dropdown *popup* via the
  // OS comctl32 control, which ignores the page theme — so in dark mode the
  // option list shows a white background with unreadable text. `color-scheme:
  // dark` does not reliably fix this across WebView2 versions. macOS (WKWebView)
  // and Linux (GTK) follow the system theme, so there we keep the native
  // <select> unchanged.

  let {
    value,
    options,
    onchange,
    class: klass = "sel",
    style: sty = "",
    disabled = false,
    ariaLabel: aria_label,
  }: {
    value: string;
    options: SelectOption[];
    onchange: (value: string) => void;
    class?: string;
    style?: string;
    disabled?: boolean;
    ariaLabel?: string;
  } = $props();

  // WebView2 is Windows-only; Mac/Linux keep the native control.
  const isWindows =
    typeof navigator !== "undefined" && /Win/.test(navigator.platform ?? "");

  let open = $state(false);
  let triggerEl: HTMLButtonElement | undefined = $state();
  // Popup coordinates (fixed positioning, computed on open to escape any
  // overflow-clipped scroll container).
  let popStyle = $state<{ top: string; left: string; width: string }>({
    top: "0px",
    left: "0px",
    width: "0px",
  });

  const selectedLabel = $derived(
    options.find((o) => o.value === value)?.label ?? "",
  );

  function placePopup(): void {
    if (!triggerEl) return;
    const r = triggerEl.getBoundingClientRect();
    const POP_H = Math.min(options.length * 34 + 8, 260);
    const flipUp = r.bottom + POP_H + 4 > window.innerHeight;
    const top = flipUp ? r.top - POP_H - 4 : r.bottom + 4;
    popStyle = {
      top: `${Math.max(4, top)}px`,
      left: `${r.left}px`,
      width: `${r.width}px`,
    };
  }

  function toggle(): void {
    if (disabled) return;
    if (!open) placePopup();
    open = !open;
  }

  function pick(v: string): void {
    onchange(v);
    open = false;
  }

  function onDocClick(e: MouseEvent): void {
    // Close when clicking outside both the trigger and the popup.
    const target = e.target as Node;
    if (triggerEl?.contains(target)) return;
    if (popEl?.contains(target)) return;
    open = false;
  }

  function onDocKey(e: KeyboardEvent): void {
    if (e.key === "Escape") open = false;
  }

  let popEl: HTMLDivElement | undefined = $state();

  // Manage document-level listeners only while open.
  $effect(() => {
    if (!open) return;
    document.addEventListener("click", onDocClick, true);
    document.addEventListener("keydown", onDocKey);
    const onClose = () => {
      open = false;
    };
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      document.removeEventListener("click", onDocClick, true);
      document.removeEventListener("keydown", onDocKey);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  });
</script>

{#if !isWindows}
  <!-- Native <select> for macOS / Linux: theme follows the OS, no fix needed. -->
  <select
    class={klass}
    style={sty}
    {disabled}
    aria-label={aria_label}
    {value}
    onchange={(e) => onchange((e.target as HTMLSelectElement).value)}
  >
    {#each options as opt (opt.value)}
      <option value={opt.value}>{opt.label}</option>
    {/each}
  </select>
{:else}
  <!-- Windows: custom trigger + dark floating popup. -->
  <div class="tu-select">
    <button
      type="button"
      bind:this={triggerEl}
      class="{klass} tu-trigger"
      style={sty}
      class:tu-open={open}
      class:tu-disabled={disabled}
      {disabled}
      aria-label={aria_label}
      onclick={toggle}
    >
      <span class="tu-label">{selectedLabel}</span>
      <span class="tu-caret" aria-hidden="true"></span>
    </button>

    {#if open}
      <!-- position: fixed so it is never clipped by a scrolling panel. -->
      <div
        bind:this={popEl}
        class="tu-popup"
        style="top:{popStyle.top};left:{popStyle.left};width:{popStyle.width}"
        role="listbox"
      >
        {#each options as opt (opt.value)}
          <button
            type="button"
            class="tu-opt"
            class:tu-sel={opt.value === value}
            onclick={() => pick(opt.value)}
            role="option"
            aria-selected={opt.value === value}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .tu-select {
    position: relative;
    display: inline-flex;
  }

  /* Trigger: layered on top of the passed class (.sel / .fsel / .qregion-select)
     so the closed appearance is identical to the native control. We only add the
     flex layout (label + caret) and reset native button chrome. */
  .tu-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    cursor: pointer;
    text-align: left;
    width: 100%;
    /* Reset user-agent button styles; the passed class supplies the look. */
    -webkit-appearance: none;
    appearance: none;
    outline: none;
  }
  .tu-trigger.tu-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .tu-trigger.tu-open {
    border-color: var(--amber) !important;
  }

  .tu-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* CSS-only caret (no icon dependency). */
  .tu-caret {
    flex-shrink: 0;
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
    border-top: 5px solid var(--text-faint);
    transition: transform 0.15s;
  }
  .tu-open .tu-caret {
    transform: rotate(180deg);
    border-top-color: var(--amber);
  }

  /* Dark floating popup — the whole point of this component. */
  .tu-popup {
    position: fixed;
    z-index: 100000;
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(0, 0, 0, 0.25);
    padding: 4px;
    max-height: 260px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    /* Ensure popup is opaque even on transparent windows. */
    backdrop-filter: blur(8px);
  }

  .tu-opt {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    color: var(--text);
    padding: 7px 10px;
    border-radius: 5px;
    font-family: inherit;
    font-size: 0.8667rem;
    cursor: pointer;
    transition: background 0.1s;
  }
  .tu-opt:hover {
    background: var(--surface-tint-strong);
  }
  .tu-opt.tu-sel {
    color: var(--amber);
    font-weight: 600;
    background: var(--amber-bg);
  }
</style>
