<script lang="ts">
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

  // Vendor display names.
  const vendorNames: Record<string, string> = {
    glm: "GLM",
    kimi: "Kimi",
    minimax: "Minimax",
    volcengine: "火山引擎",
    bailian: "百炼",
    stepfun: "阶跃",
    openrouter: "OpenRouter",
    ollama: "Ollama",
    deepseek: "DeepSeek",
    workbuddy: "WorkBuddy",
    qoder: "Qoder",
  };

  let displayName = $derived(vendorNames[vendor] ?? vendor);

  // ── Vendor icon paths (24x24 viewBox, minimal geometric marks) ────────
  // Simple distinctive shapes recognizable at 14–18px rendered size.
  const vendorIcons: Record<string, { d: string; stroke?: boolean }> = {
    claude:      { d: "M7 17V7h6v10H7zm2-5.5h2v3H9z" },
    deepseek:    { d: "M7 17V7h5.5A3.5 3.5 0 0116 10.5v6.5A3.5 3.5 0 0112.5 17H7z" },
    kimi:        { d: "M8.5 17V7l6.5 5-6.5 5z" },
    minimax:     { d: "M12 7.5l4.5 9h-9z" },
    volcengine:  { d: "M12 7.5l4 4.5-4 4.5-4-4.5z" },
    bailian:     { d: "M7.5 7.5h9v9h-9z" },
    stepfun:     { d: "M7.5 16.5q4.5-9 9 0", stroke: true },
    openrouter:  { d: "M12 7.5a4.5 4.5 0 100 9 4.5 4.5 0 000-9z" },
    ollama:      { d: "M12 7.5a4.5 4.5 0 114.5 4.5 4.5 4.5 0 01-4.5-4.5z", stroke: true },
    workbuddy:   { d: "M8 17l7-9.5", stroke: true },
    qoder:       { d: "M8.5 8.5l7 7m0-7l-7 7", stroke: true },
  };

  let iconData = $derived(vendorIcons[vendor] ?? { d: "", stroke: false });

  // ── Ring geometry ──────────────────────────────────────────────────────

  let strokeWidth = $derived(Math.max(3, diameter * 0.10));
  let radius = $derived((diameter - strokeWidth) / 2);
  let circumference = $derived(2 * Math.PI * radius);
  let centerRadius = $derived(Math.max(4, (diameter - strokeWidth * 2 - 8) / 2));

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
  style="width:{diameter}px;height:{diameter}px"
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

    <!-- ── Center disc (dark, creates separation) ──────────────────────── -->
    <circle
      cx={diameter / 2}
      cy={diameter / 2}
      r={centerRadius}
      fill="var(--pulse-ring-bg)"
    />

    <!-- ── Center icon: vendor SVG mark ─────────────────────────────────── -->
    {#if iconData.d}
      <path
        d={iconData.d}
        fill={iconData.stroke ? "none" : "var(--pulse-text-dim)"}
        stroke={iconData.stroke ? "var(--pulse-text-dim)" : "none"}
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        transform="translate({diameter / 2 - 12}, {diameter / 2 - 12}) scale(0.7)"
        pointer-events="none"
      />
    {/if}

    <!-- ── Percentage text (inside ring center) ──────────────────────── -->
    <text
      x={diameter / 2}
      y={diameter / 2 + centerRadius * 0.55}
      text-anchor="middle"
      dominant-baseline="central"
      fill="var(--pulse-text)"
      font-size={Math.max(7, centerRadius * 0.5)}
      font-family="'SF Mono', 'JetBrains Mono', 'Menlo', 'Consolas', monospace"
      font-weight="600"
      letter-spacing="-0.02em"
      pointer-events="none"
      style="opacity: 0.7"
    >
      {Math.round(displayPct)}%
    </text>
  </svg>
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
    transition: transform 0.22s cubic-bezier(0.34, 1.56, 0.64, 1);
    color: var(--pulse-text);
    -webkit-tap-highlight-color: transparent;
  }

  .ring-container:hover {
    transform: scale(1.08);
  }

  .ring-container:active {
    transform: scale(1.04);
  }

  .ring-svg {
    display: block;
  }

  .usage-arc {
    transition: stroke-dashoffset 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
  }
</style>
