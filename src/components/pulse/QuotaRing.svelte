<script lang="ts">
  import { vendorDisplayName, vendorIconMarkup } from "../../lib/vendorIcons";
  import { vendorColor } from "../../lib/pulse-colors";

  interface Props {
    vendor: string;
    pct: number;
    label: string;
    diameter: number;
    onHover?: () => void;
    onLeave?: () => void;
    isRunning?: boolean;
    isRefreshing?: boolean;
    /** Additional windows (week / MCP / …) rendered as inner concentric arcs. */
    extraPcts?: number[];
    showsRemaining?: boolean;
    /** Dock-order index — drives the per-vendor hue (每一家都不同). */
    colorIndex?: number;
    /** Plan-less vendors: currency unit + amount shown under the ring
     *  instead of a percentage (unit renders smaller, e.g. ¥). */
    subUnit?: string;
    subValue?: string;
    /** Plan-less vendors: track ring only — icon + amount, no progress arcs. */
    plain?: boolean;
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
    extraPcts,
    showsRemaining = false,
    colorIndex = 0,
    subUnit,
    subValue,
    plain = false,
  }: Props = $props();

  let displayName = $derived(vendorDisplayName(vendor));

  // Label metrics scale with the ring preset (mirrors Rust layout_scale =
  // d / 48): LABEL_H × s = 11px line + 10px top margin, font tracks too.
  let labelS = $derived(diameter / 48);

  // ── Ring geometry ──────────────────────────────────────────────────────

  let strokeWidth = $derived(Math.max(3, diameter * 0.10));
  let radius = $derived((diameter - strokeWidth) / 2);
  let circumference = $derived(2 * Math.PI * radius);
  let centerRadius = $derived(Math.max(4, (diameter - strokeWidth * 2 - 8) / 2));

  // All arcs, outer → inner: the critical window + extras. Each inner arc
  // steps inward and thins so multi-window vendors (5h/week/MCP/…) keep
  // every arc visible.
  let arcs = $derived([
    Math.min(100, Math.max(0, pct)),
    ...(extraPcts ?? []).map((p) => Math.min(100, Math.max(0, p))),
  ]);
  const ARC_STEP = 1.18; // × strokeWidth between concentric arcs

  // Per-arc geometry (radius / stroke / dash offset), skipping arcs that
  // would collapse into the center.
  let arcSpecs = $derived(
    arcs
      .map((p, i) => {
        const r = radius - i * strokeWidth * ARC_STEP;
        const sw = Math.max(2.0, strokeWidth * (1 - i * 0.22));
        const c = 2 * Math.PI * r;
        return { r, sw, c, offset: c * (1 - p / 100), isMain: i === 0, p };
      })
      .filter((a) => a.r >= 3)
  );

  // Center glyph — shrinks when inner arcs crowd the middle.
  let iconSize = $derived.by(() => {
    const innermost = arcSpecs.length > 0 ? arcSpecs[arcSpecs.length - 1] : null;
    const defaultSize = Math.round(centerRadius * 1.25);
    if (!innermost) return defaultSize;
    const room = (innermost.r - innermost.sw / 2 - 1.2) * 2;
    return Math.max(8, Math.min(defaultSize, Math.round(room)));
  });
  // Real brand SVG markup from the app's shared icon set (currentColor fill).
  let iconMarkup = $derived(vendorIconMarkup(vendor));
  let iconOffset = $derived((diameter - iconSize) / 2);

  // ── Computed display values ────────────────────────────────────────────

  let usedPct = $derived(Math.min(100, Math.max(0, pct)));
  let displayPct = $derived(showsRemaining ? 100 - usedPct : usedPct);
  // Per-vendor hue from the dock order (golden-angle spread) — every
  // account reads as a distinct color; the remaining arc length conveys
  // severity, so the color is free to identify the vendor.
  let arcColor = $derived(vendorColor(colorIndex));

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
    <!-- No halo glow: the blurred arc smeared color across the ring's
         interior, reading as a colored disc background on high-usage
         vendors (GLM / StepFun). Only the arc strokes carry color. -->

    <!-- ── Background track — always visible (plain vendors keep the ring
         outline, just without progress arcs). ─────────────────────────── -->
    <circle
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke="var(--pulse-track)"
      stroke-width={strokeWidth}
    />

    <!-- ── Progress arcs — hidden for plan-less vendors. One concentric
         arc per window (5h outer, week/MCP/… inner), thinning inward. ── -->
    {#if !plain}
      {#each arcSpecs as spec, i (i)}
        {#if !spec.isMain}
          <!-- Per-layer track: keeps every layer visible even at 0% usage
               (e.g. GLM's MCP window) — multi-layer rings read as rings. -->
          <circle
            cx={diameter / 2}
            cy={diameter / 2}
            r={spec.r}
            fill="none"
            stroke="var(--pulse-track)"
            stroke-width={spec.sw}
            opacity="0.7"
          />
        {/if}
        <circle
          class="usage-arc"
          cx={diameter / 2}
          cy={diameter / 2}
          r={spec.r}
          fill="none"
          stroke={arcColor}
          stroke-width={spec.sw}
          stroke-linecap="round"
          stroke-dasharray={spec.c}
          stroke-dashoffset={spec.offset}
          transform="rotate(-90 {diameter / 2} {diameter / 2})"
          opacity={spec.isMain ? 1 : 0.5}
        />
      {/each}
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

    <!-- No center disc — the glyph floats over the transparent hole. -->
  </svg>

  <!-- ── Center glyph: real brand mark only (pct lives below the ring) ── -->
  <span
    class="ring-icon"
    style="width:{iconSize}px;height:{iconSize}px;left:{iconOffset}px;top:{iconOffset}px"
    aria-hidden="true">{@html iconMarkup}</span
  >

  <!-- ── Label under the ring: credits/balance for plan-less vendors
         (smaller currency unit), otherwise the usage percentage. Metrics
         scale with the ring preset (inline — layout contract). ───────── -->
  <span
    class="pct-label"
    style="margin-top:{10 * labelS}px;height:{11 * labelS}px;line-height:{11 * labelS}px;font-size:{11 * labelS}px"
  >
    {#if subValue}
      {#if subUnit}<span class="sub-unit">{subUnit}</span>{/if}{subValue}
    {:else}{Math.round(displayPct)}<span class="pct-sign">%</span>
    {/if}
  </span>
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

  /* Percentage under the ring — metrics come inline (labelS-scaled) so
     the panel height math (Rust LABEL_H = 21 × s) stays in sync. Solid
     text color (near-black on light theme) per user preference. */
  .pct-label {
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    color: var(--pulse-text);
    letter-spacing: -0.03em;
    pointer-events: none;
    -webkit-user-select: none;
    user-select: none;
  }

  /* Currency unit (¥/$) renders smaller than the digits (em → scales with
     the label font). A small margin-right separates the unit glyph from
     the numeric value for visual clarity. */
  .pct-label .sub-unit {
    font-size: 0.73em;
    margin-right: 2px;
  }

  .pct-label .pct-sign {
    margin-left: 2px;
  }

  .usage-arc {
    transition: stroke-dashoffset 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
</style>
