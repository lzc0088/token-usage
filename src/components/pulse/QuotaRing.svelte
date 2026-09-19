<script lang="ts">
  import { vendorDisplayName, vendorIconMarkup } from "../../lib/vendorIcons";

  interface Props {
    vendor: string;
    pct: number;
    label: string;
    diameter: number;
    onHover: () => void;
    onLeave: () => void;
    isRunning?: boolean;
    isRefreshing?: boolean;
    secondPct?: number;
    showsRemaining?: boolean;
  }

  let {
    vendor,
    pct,
    label,
    diameter,
    onHover,
    onLeave,
    isRunning = false,
    isRefreshing = false,
    secondPct,
    showsRemaining = false,
  }: Props = $props();

  let displayName = $derived(vendorDisplayName(vendor));

  // ── Ring geometry ──────────────────────────────────────────────────────

  let strokeWidth = $derived(Math.max(3, diameter * 0.10));
  let radius = $derived((diameter - strokeWidth) / 2);
  let circumference = $derived(2 * Math.PI * radius);
  let centerRadius = $derived(Math.max(4, (diameter - strokeWidth * 2 - 8) / 2));
  // Center glyph size — small mark floating in the (transparent) hole.
  let iconSize = $derived(Math.round(centerRadius * 1.25));
  // Real brand SVG markup from the app's shared icon set (currentColor fill).
  let iconMarkup = $derived(vendorIconMarkup(vendor));
  let iconOffset = $derived((diameter - iconSize) / 2);

  // ── Computed display values ────────────────────────────────────────────

  let usedPct = $derived(Math.min(100, Math.max(0, pct)));
  let displayPct = $derived(showsRemaining ? 100 - usedPct : usedPct);
  let offset = $derived(circumference * (1 - displayPct / 100));
  let arcColor = $derived(ringColor(usedPct));

  // Second (inner) ring) — null when not shown.
  let secondOffset = $derived(
    secondPct == null
      ? null
      : Math.max(0, Math.min(100, secondPct)) === 100
        ? 0
        : circumference * (1 - Math.max(0, Math.min(100, secondPct)) / 100)
  );

  // Unique filter ID per vendor.
  let filterId = $derived(`halo-${vendor}`);

  function ringColor(pct: number): string {
    if (pct >= 100) return "var(--pulse-exhausted)";
    if (pct >= 75) return "var(--pulse-warning)";
    if (pct >= 50) return "var(--pulse-caution)";
    return "var(--pulse-good)";
  }

  // ── Busy / refresh arc animation ────────────────────────────────────────

  let busyRotation = $state(0);
  let refreshRotation = $state(0);

  // Animate busy mark (slow constant rotation).
  $effect(() => {
    if (!isRunning) return;
    let raf: number;
    const tick = () => {
      busyRotation = (busyRotation + 0.8) % 360;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  // Animate refresh mark (faster, constant).
  $effect(() => {
    if (!isRefreshing) return;
    let raf: number;
    const tick = () => {
      refreshRotation = (refreshRotation + 1.5) % 360;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
</script>

<button
  class="ring-container"
  style="width:{diameter}px"
  onmouseenter={onHover}
  onmouseleave={onLeave}
  aria-label="{displayName} {label}: {Math.round(usedPct)}%"
>
  <svg
    class="ring-svg"
    viewBox="0 0 {diameter} {diameter}"
    width={diameter}
    height={diameter}
  >
    <defs>
      <!-- Halo glow: blur the arc and let the center disc mask it out. -->
      <filter
        id={filterId}
        x="-50%"
        y="-50%"
        width="200%"
        height="200%"
      >
        <feGaussianBlur
          in="SourceGraphic"
          stdDeviation="{Math.max(2, diameter * 0.07)}"
          result="blur"
        />
        <feMerge>
          <feMergeNode in="blur" />
          <feMergeNode in="SourceGraphic" />
        </feMerge>
      </filter>
    </defs>

    <!-- ── Background track ──────────────────────────────────────────── -->
    <circle
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke="var(--pulse-track)"
      stroke-width={strokeWidth}
    />

    <!-- ── Halo glow (wider blurred arc) ─────────────────────────────── -->
    <circle
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke={arcColor}
      stroke-width={strokeWidth * 2}
      stroke-linecap="round"
      stroke-dasharray={circumference}
      stroke-dashoffset={offset}
      transform="rotate(-90 {diameter / 2} {diameter / 2})"
      opacity="0.15"
      filter="url(#{filterId})"
    />

    <!-- ── Second inner ring ──────────────────────────────────────────── -->
    {#if secondPct != null && secondOffset != null}
      <circle
        cx={diameter / 2}
        cy={diameter / 2}
        r={Math.max(4, radius - strokeWidth * 0.8)}
        fill="none"
        stroke={arcColor}
        stroke-width={strokeWidth * 0.45}
        stroke-linecap="round"
        stroke-dasharray={2 * Math.PI * Math.max(4, radius - strokeWidth * 0.8)}
        stroke-dashoffset={secondOffset}
        transform="rotate(-90 {diameter / 2} {diameter / 2})"
        opacity="0.35"
      />
    {/if}

    <!-- ── Busy travelling mark ────────────────────────────────────────── -->
    {#if isRunning}
      <circle
        cx={diameter / 2}
        cy={diameter / 2}
        r={radius}
        fill="none"
        stroke={arcColor}
        stroke-width={strokeWidth * 0.35}
        stroke-linecap="round"
        stroke-dasharray="{circumference * 0.22} {circumference * 0.78}"
        transform="rotate({busyRotation} {diameter / 2} {diameter / 2})"
        opacity="0.8"
      />
    {/if}

    <!-- ── Refresh spinning arc ────────────────────────────────────────── -->
    {#if isRefreshing}
      <circle
        cx={diameter / 2}
        cy={diameter / 2}
        r={radius}
        fill="none"
        stroke="var(--pulse-text-dim)"
        stroke-width={strokeWidth * 0.3}
        stroke-linecap="round"
        stroke-dasharray="{circumference * 0.35} {circumference * 0.65}"
        transform="rotate({refreshRotation} {diameter / 2} {diameter / 2})"
        opacity="0.6"
      />
    {/if}

    <!-- ── Usage arc (main progress) ───────────────────────────────────── -->
    <circle
      class="usage-arc"
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke={arcColor}
      stroke-width={strokeWidth}
      stroke-linecap="round"
      stroke-dasharray={circumference}
      stroke-dashoffset={offset}
      transform="rotate(-90 {diameter / 2} {diameter / 2})"
    />

    <!-- No center disc — the glyph floats over the transparent hole. -->
  </svg>

  <!-- ── Center glyph: real brand mark only (pct lives below the ring) ── -->
  <span
    class="ring-icon"
    style="width:{iconSize}px;height:{iconSize}px;left:{iconOffset}px;top:{iconOffset}px"
    aria-hidden="true">{@html iconMarkup}</span
  >

  <!-- ── Percentage label under the ring ──────────────────────────────── -->
  <span class="pct-label">{Math.round(displayPct)}%</span>
</button>

<style>
  .ring-container {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    color: var(--pulse-text);
    -webkit-tap-highlight-color: transparent;
  }

  .ring-svg {
    display: block;
  }

  /* Brand glyph overlay — sized/positioned inline in px, colored via
     currentColor. No opacity/transform: this WKWebView runs in a transparent
     window where compositing layers can drop content. */
  .ring-icon {
    position: absolute;
    color: var(--pulse-text);
    pointer-events: none;
  }

  .ring-icon :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }

  /* Percentage under the ring — a fixed 13px block (11px line + 2px margin)
     so the panel height math (Rust LABEL_H = 13) stays in sync. */
  .pct-label {
    margin-top: 2px;
    height: 11px;
    line-height: 11px;
    font-size: 9px;
    font-weight: 500;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    color: var(--pulse-text-dim);
    letter-spacing: -0.03em;
    pointer-events: none;
    -webkit-user-select: none;
    user-select: none;
  }

  .usage-arc {
    transition: stroke-dashoffset 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
</style>
