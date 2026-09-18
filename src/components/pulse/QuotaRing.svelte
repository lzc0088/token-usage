<script lang="ts">
  interface Props {
    vendor: string;
    pct: number;
    label: string;
    diameter: number;
    color: string;
    onHover: () => void;
    onLeave: () => void;
  }

  let { vendor, pct, label, diameter, color, onHover, onLeave }: Props = $props();

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

  // SVG ring math.
  let strokeWidth = $derived(Math.max(3, diameter * 0.08));
  let radius = $derived((diameter - strokeWidth) / 2);
  let circumference = $derived(2 * Math.PI * radius);
  let offset = $derived(circumference * (1 - Math.min(pct, 100) / 100));

  // Initials for the center badge.
  let initials = $derived(
    displayName
      .split(/[\s·-]+/)
      .map((w) => w[0])
      .join("")
      .slice(0, 2)
      .toUpperCase()
  );
</script>

<button
  class="ring-container"
  style="width:{diameter}px;height:{diameter}px"
  onmouseenter={onHover}
  onmouseleave={onLeave}
  aria-label="{displayName} {label}: {Math.round(pct)}%"
>
  <svg
    class="ring-svg"
    viewBox="0 0 {diameter} {diameter}"
    width={diameter}
    height={diameter}
  >
    <!-- Background track -->
    <circle
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke="currentColor"
      stroke-width={strokeWidth}
      opacity="0.12"
    />
    <!-- Progress arc -->
    <circle
      class="progress-arc"
      cx={diameter / 2}
      cy={diameter / 2}
      r={radius}
      fill="none"
      stroke={color}
      stroke-width={strokeWidth}
      stroke-linecap="round"
      stroke-dasharray={circumference}
      stroke-dashoffset={offset}
      transform="rotate(-90 {diameter / 2} {diameter / 2})"
    />
  </svg>

  <!-- Center badge: initials -->
  <div class="center-badge" style="font-size:{diameter * 0.26}px">
    {initials}
  </div>

  <!-- Percentage label below ring -->
  <div class="pct-label" style="font-size:{Math.max(10, diameter * 0.22)}px">
    {Math.round(pct)}%
  </div>
</button>

<style>
  .ring-container {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    transition: transform 0.2s ease;
    color: var(--pulse-text);
  }

  .ring-container:hover {
    transform: scale(1.08);
  }

  .ring-svg {
    display: block;
  }

  .progress-arc {
    transition: stroke-dashoffset 0.6s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .center-badge {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--pulse-text-dim);
    pointer-events: none;
  }

  .pct-label {
    font-weight: 700;
    font-family: "SF Mono", "JetBrains Mono", "Consolas", monospace;
    color: var(--pulse-text);
    line-height: 1;
    letter-spacing: -0.01em;
  }
</style>
