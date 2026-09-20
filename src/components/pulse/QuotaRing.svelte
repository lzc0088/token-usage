<script lang="ts">
  import { vendorDisplayName, vendorIconMarkup } from "../../lib/vendorIcons";

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
    subUnit,
    subValue,
    plain = false,
  }: Props = $props();

  let displayName = $derived(vendorDisplayName(vendor));

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
  let offset = $derived(2 * Math.PI * radius * (1 - displayPct / 100));
  let arcColor = $derived(ringColor(usedPct));

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
      <!-- ── Halo glow (wider blurred arc around the MAIN arc only) ──── -->
      <circle
        cx={diameter / 2}
        cy={diameter / 2}
        r={radius}
        fill="none"
        stroke={arcColor}
        stroke-width={strokeWidth * 2}
        stroke-linecap="round"
        stroke-dasharray={2 * Math.PI * radius}
        stroke-dashoffset={offset}
        transform="rotate(-90 {diameter / 2} {diameter / 2})"
        opacity="0.15"
        filter="url(#{filterId})"
      />

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
         (smaller currency unit), otherwise the usage percentage. ───────── -->
  <span class="pct-label">
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

  /* Percentage under the ring — fixed 21px block (11px line + 10px margin)
     so the panel height math (Rust LABEL_H = 21) stays in sync. Solid text
     color (near-black on light theme) per user preference. */
  .pct-label {
    margin-top: 10px;
    height: 11px;
    line-height: 11px;
    font-size: 11px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    color: var(--pulse-text);
    letter-spacing: -0.03em;
    pointer-events: none;
    -webkit-user-select: none;
    user-select: none;
  }

  /* Currency unit (¥/$) renders smaller than the digits. No CSS margin —
     the glyph's natural side bearing provides the ~1px optical gap; a
     margin on top of it double-spaced the pair (user-tuned 1px). */
  .pct-label .sub-unit {
    font-size: 8px;
    margin-right: 0;
  }

  .pct-label .pct-sign {
    margin-left: 0;
  }

  .usage-arc {
    transition: stroke-dashoffset 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
</style>
