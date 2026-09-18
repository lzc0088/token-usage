<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
  import { PhysicalPosition } from "@tauri-apps/api/dpi";
  import QuotaRing from "../components/pulse/QuotaRing.svelte";
  import DetailCard from "../components/pulse/DetailCard.svelte";

  interface PulseWindow {
    label: string;
    used_pct: number;
    resets_at?: string;
  }

  interface PulseBalance {
    amount: number;
    currency: string;
  }

  interface PulseQuota {
    vendor: string;
    plan?: string;
    status: string;
    windows: PulseWindow[];
    critical_pct: number;
    critical_label: string;
    balance?: PulseBalance;
    second_pct?: number;
    second_label?: string;
    is_running?: boolean;
    is_refreshing?: boolean;
  }

  interface PulseData {
    quotas: PulseQuota[];
    size: string;
    ring_diameter: number;
    theme: string;
  }

  // Expand payload from Rust (ui/pulse.rs expand_pulse).
  interface ExpandPayload {
    vendor: string | null;
    /** true → window grew leftward, so the card opens LEFT of the rings. */
    cardLeft?: boolean;
  }

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });

  let hoveredVendor = $state<string | null>(null);
  let isExpanded = $state(false);
  // Which side the detail card opens on — decided by Rust (it knows which
  // way the window grew) so the card never lands outside the window.
  let expandCardLeft = $state(false);

  // Listen for data updates from Rust.
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
  });

  listen<ExpandPayload>("pulse:expand", (e) => {
    isExpanded = true;
    hoveredVendor = e.payload?.vendor ?? null;
    expandCardLeft = !!e.payload?.cardLeft;
  });

  listen("pulse:collapse", () => {
    isExpanded = false;
    hoveredVendor = null;
    expandCardLeft = false;
  });

  // ── Layout constants (mirror Rust pulse.rs) ────────────────────────────
  // Ring item = ring + LABEL_H (pct label); items separated by ITEM_GAP.
  const PAD_V = 14;
  const TITLE_BLOCK = 24;
  const LABEL_H = 13;
  const ITEM_GAP = 4;
  const PAD_FLUSH_INNER = 18;
  const PAD_FLUSH_EDGE = 10;
  const PAD_FLOAT = 16;
  // Teardrop cap height for the flush (fused-edge) silhouette.
  const CAP = 24;

  let ringCount = $derived(Math.max(data.quotas.length, 1));
  let dockH = $derived(
    ringCount * (data.ring_diameter + LABEL_H) + (ringCount - 1) * ITEM_GAP
  );
  let panelH = $derived(PAD_V * 2 + TITLE_BLOCK + dockH);

  // Which screen edge the panel is fused to (null = floating pill).
  let flushSide = $state<"left" | "right" | null>(null);

  let padH = $derived(
    flushSide === "left" || flushSide === "right"
      ? PAD_FLUSH_INNER + PAD_FLUSH_EDGE
      : PAD_FLOAT * 2
  );
  let panelW = $derived(data.ring_diameter + padH);

  // Flush silhouette: inverted-S (倒S) teardrop caps. The top/bottom edges
  // sweep from the inner side toward the fused edge, arriving smoothly
  // (vertical tangent) at a tip flush with the screen border — matching the
  // Pulse dock reference. Floating panels fall back to a rounded pill.
  let surfaceStyle = $derived.by(() => {
    const w = panelW;
    const h = Math.max(panelH, CAP * 3);
    const c = Math.min(CAP, h / 3);
    const cTop = (c * 0.35).toFixed(1);
    const cBot = (c * 0.6).toFixed(1);
    if (flushSide === "right") {
      return `clip-path: path("M 0 ${c.toFixed(1)} C 0 ${cTop} ${w} ${cBot} ${w} 0 L ${w} ${h} C ${w} ${(h - c * 0.6).toFixed(1)} 0 ${(h - c * 0.35).toFixed(1)} 0 ${(h - c).toFixed(1)} Z")`;
    }
    if (flushSide === "left") {
      return `clip-path: path("M ${w} ${c.toFixed(1)} C ${w} ${cTop} 0 ${cBot} 0 0 L 0 ${h} C 0 ${(h - c * 0.6).toFixed(1)} ${w} ${(h - c * 0.35).toFixed(1)} ${w} ${(h - c).toFixed(1)} Z")`;
    }
    return "";
  });

  // ── Drag + berth fusion ─────────────────────────────────────────────
  // The panel is dragged with the native titlebar-style drag (startDragging),
  // so it keeps following the cursor even when the pointer outruns the tiny
  // webview. Near an edge the window snaps flush so the flat side fuses with
  // the screen border (inverted-S teardrop silhouette).
  const win = getCurrentWindow();

  // Distance (logical px) from a screen edge within which the panel snaps.
  const SNAP_THRESHOLD = 24;

  async function readGeometry() {
    const [pos, size, mon, scale] = await Promise.all([
      win.outerPosition(),
      win.outerSize(),
      currentMonitor(),
      win.scaleFactor(),
    ]);
    return { pos, size, mon, scale };
  }

  async function updateFlushSide() {
    try {
      const { pos, size, mon, scale } = await readGeometry();
      if (!mon) {
        flushSide = null;
        return;
      }
      const thr = 8 * scale;
      const gapL = pos.x - mon.position.x;
      const gapR = mon.position.x + mon.size.width - (pos.x + size.width);
      flushSide = gapR <= thr ? "right" : gapL <= thr ? "left" : null;
    } catch {
      flushSide = null;
    }
  }

  // Snap flush to the nearest screen edge when dropped close to it.
  async function snapToEdge() {
    try {
      const { pos, size, mon, scale } = await readGeometry();
      if (!mon) return;
      const threshold = SNAP_THRESHOLD * scale;
      const monLeft = mon.position.x;
      const monRight = mon.position.x + mon.size.width;
      if (pos.x - monLeft <= threshold) {
        await win.setPosition(new PhysicalPosition(monLeft, pos.y));
      } else if (monRight - (pos.x + size.width) <= threshold) {
        await win.setPosition(new PhysicalPosition(monRight - size.width, pos.y));
      }
    } catch {
      // ignore — position stays wherever the drag left it
    }
  }

  $effect(() => {
    const onMouseDown = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      // Drag only from panel chrome (title, padding), never from interactive bits.
      if (!target?.closest(".pulse-panel")) return;
      if (target.closest(".ring-container, .close-btn, .detail-tooltip")) return;
      win.startDragging().catch(() => {});
    };
    const onMouseUp = () => {
      // Persist immediately after a drag; the 1s Rust poller is the safety net.
      snapToEdge().then(() => {
        win
          .outerPosition()
          .then((pos) => invoke("set_pulse_position", { x: pos.x, y: pos.y }))
          .catch(() => {});
      });
    };
    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("mouseup", onMouseUp);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("mouseup", onMouseUp);
    };
  });

  // Track window moves (fires during native drag + expand/collapse shifts)
  // to recompute which edge is fused.
  $effect(() => {
    const unlisten = win.onMoved(() => updateFlushSide());
    return () => {
      unlisten.then((u) => u()).catch(() => {});
    };
  });

  // Initial flush detection.
  updateFlushSide();

  function onRingEnter(vendor: string) {
    hoveredVendor = vendor;
    invoke("expand_pulse", { vendor: vendor }).catch(() => {});
  }

  function onRingLeave() {
    hoveredVendor = null;
    // Don't collapse here — let the wrapper handle collapse on full leave.
    // This prevents flashing when moving from ring to tooltip card.
  }

  function onWrapperLeave() {
    hoveredVendor = null;
    invoke("collapse_pulse").catch(() => {});
  }

  async function dismissPulse() {
    await invoke("dismiss_pulse").catch(() => {});
  }

  let hoveredQuota = $derived(
    hoveredVendor
      ? data.quotas.find((q) => q.vendor === hoveredVendor) ?? null
      : null
  );

  // Which ring index is hovered (for card positioning).
  let hoveredIndex = $derived(
    hoveredVendor ? data.quotas.findIndex((q) => q.vendor === hoveredVendor) : -1
  );

  // Panel-level title explaining the percentage mode.
  let panelTitle = $derived("使用量");
</script>

<div
  class="pulse-panel vertical"
  class:dark={data.theme === "dark"}
  class:light={data.theme !== "dark"}
  class:flush-right={flushSide === "right"}
  class:flush-left={flushSide === "left"}
  class:grow-left={expandCardLeft}
  class:card-left={expandCardLeft}
  style="--ring-size: {data.ring_diameter}px; width:{panelW}px; height:{panelH}px"
>
  <!-- Shaped surface: bg + blur + inverted-S teardrop clip. Kept on its own
       layer so the tooltip card can overflow the (clipped) panel body. -->
  <div class="panel-surface" style={surfaceStyle} aria-hidden="true"></div>

  <!-- Close / dismiss button -->
  <button
    class="close-btn"
    onclick={dismissPulse}
    aria-label="关闭 Pulse"
    title="关闭 Pulse"
  >
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
      <path
        d="M1 1L9 9M9 1L1 9"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
      />
    </svg>
  </button>

  <!-- Panel title (mode indicator) -->
  <div class="panel-title">{panelTitle}</div>

  <!-- Ring rail + tooltip container -->
  <div class="rail-with-tooltip" onmouseleave={onWrapperLeave} role="group">
    <div class="ring-dock">
      {#each data.quotas as quota (quota.vendor)}
        <QuotaRing
          vendor={quota.vendor}
          pct={quota.critical_pct}
          label={quota.critical_label}
          diameter={data.ring_diameter}
          onHover={() => onRingEnter(quota.vendor)}
          onLeave={onRingLeave}
          isRunning={quota.is_running ?? false}
          isRefreshing={quota.is_refreshing ?? false}
          secondPct={quota.second_pct}
          showsRemaining={false}
        />
      {/each}
      {#if data.quotas.length === 0}
        <div
          class="empty-ring"
          style="width:{data.ring_diameter}px;height:{data.ring_diameter}px"
        >
          <span class="empty-icon">📊</span>
        </div>
      {/if}
    </div>

    <!-- Detail tooltip card (anchored to hovered ring) -->
    {#if isExpanded && hoveredQuota && hoveredIndex >= 0}
      <div
        class="detail-tooltip"
        style="--hover-index: {hoveredIndex}; --ring-gap: 17px;"
      >
        <DetailCard quota={hoveredQuota} cardSide={expandCardLeft ? "right" : "left"} />
      </div>
    {/if}
  </div>
</div>

<style>
  /* The pulse window is borderless + transparent. Strip the popover chrome
     app.css puts on html/body/#app — the opaque bg, border and 15px radius
     would paint a rounded opaque rect over the whole (expanded) window and
     clip the fused flat edge. */
  :global(html:has(.pulse-panel)),
  :global(body:has(.pulse-panel)),
  :global(#app:has(.pulse-panel)) {
    background: transparent;
    border: none;
    border-radius: 0;
    box-shadow: none;
  }

  :root {
    --pulse-good: #00e68a;
    --pulse-caution: #ffc226;
    --pulse-warning: #ff4f42;
    --pulse-exhausted: #d92027;
    --pulse-bg: #000000;
    --pulse-ring-bg: #000000;
    --pulse-card-bg: rgba(12, 12, 12, 0.96);
    --pulse-text: #f2ede3;
    --pulse-text-dim: #8a857b;
    --pulse-track: rgba(255, 255, 255, 0.12);
    --pulse-bar-track: rgba(255, 255, 255, 0.10);
  }

  .light {
    --pulse-good: #00b36b;
    --pulse-caution: #cc8800;
    --pulse-warning: #cc2200;
    --pulse-exhausted: #a81820;
    --pulse-bg: rgba(240, 238, 234, 0.96);
    --pulse-ring-bg: rgba(240, 238, 234, 0.96);
    --pulse-card-bg: rgba(250, 248, 244, 0.96);
    --pulse-text: #1a1610;
    --pulse-text-dim: #7a756c;
    --pulse-track: rgba(0, 0, 0, 0.10);
    --pulse-bar-track: rgba(0, 0, 0, 0.07);
  }

  /* ── Panel shell ──────────────────────────────────────────────────────
     Visual surface (bg + blur + shape) lives on .panel-surface; the shell
     itself stays unclipped so the detail tooltip can overflow it. */

  .pulse-panel {
    position: relative;
    display: flex;
    color: var(--pulse-text);
    user-select: none;
    -webkit-user-select: none;
    overflow: visible;
    cursor: grab; /* draggable via native titlebar-style drag */
  }

  .panel-surface {
    position: absolute;
    inset: 0;
    z-index: 0;
    background: var(--pulse-bg);
    border-radius: 20px;
    backdrop-filter: blur(24px) saturate(180%);
    -webkit-backdrop-filter: blur(24px) saturate(180%);
    pointer-events: none;
  }

  .pulse-panel.flush-right .panel-surface,
  .pulse-panel.flush-left .panel-surface {
    border-radius: 0; /* clip-path draws the teardrop instead */
  }

  /* Content rides above the surface layer. */
  .panel-title,
  .rail-with-tooltip {
    position: relative;
    z-index: 1;
  }

  /* ── Layout ────────────────────────────────────────────────────────── */

  .vertical {
    flex-direction: column;
    align-items: center;
    padding: 14px 16px;
    gap: 8px;
  }

  /* Fused berths: flat edge kisses the screen border (-1px overlap), rings
     biased toward the edge (smaller padding on the fused side). The window
     keeps a few px of slack, so right-fusing also right-anchors the panel. */
  .pulse-panel.flush-right {
    padding: 14px 10px 14px 18px;
    margin-left: auto;
    margin-right: -1px;
  }

  .pulse-panel.flush-left {
    padding: 14px 18px 14px 10px;
    margin-left: -1px;
  }

  /* Right-fused expand: macOS anchors resizes top-left, so Rust shifts the
     window leftward — right-anchor the panel so the rings don't jump. */
  .pulse-panel.grow-left {
    margin-left: auto;
    margin-right: -1px;
  }

  /* ── Close button ──────────────────────────────────────────────────── */

  .close-btn {
    position: absolute;
    top: 10px;
    right: 10px;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(128, 128, 128, 0.2);
    border: none;
    border-radius: 50%;
    color: var(--pulse-text-dim);
    cursor: pointer;
    padding: 0;
    transition: all 0.15s ease;
    opacity: 0;
    pointer-events: none;
    z-index: 10;
  }

  .pulse-panel:hover .close-btn {
    opacity: 1;
    pointer-events: auto;
  }

  /* Right-fused: the teardrop tip is top-right, park the button left. */
  .pulse-panel.flush-right .close-btn {
    left: 14px;
    right: auto;
    top: 14px;
  }

  .close-btn:hover {
    background: rgba(255, 79, 66, 0.3);
    color: var(--pulse-warning);
    transform: scale(1.1);
  }

  .close-btn:active {
    transform: scale(0.92);
  }

  /* ── Ring dock ─────────────────────────────────────────────────────── */

  .rail-with-tooltip {
    position: relative;
  }

  .ring-dock {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* ── Panel title ────────────────────────────────────────────────────── */

  .panel-title {
    font-size: 10px;
    font-weight: 500;
    color: var(--pulse-text-dim);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    margin-bottom: 4px;
    align-self: flex-start;
    margin-left: 2px;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  /* Left-fused: keep the title clear of the top-left teardrop tip. */
  .pulse-panel.flush-left .panel-title {
    align-self: flex-end;
    margin-left: 0;
    margin-right: 2px;
  }

  /* ── Detail tooltip card ────────────────────────────────────────────── */

  .detail-tooltip {
    position: absolute;
    pointer-events: auto;
    z-index: 20;
    animation: tooltipIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  /* Card centers on the hovered ring (offsets are rail-relative — the rail's
     top-left is the first ring's top-left). max() floors the position so a
     max-height card (half ≤ 128px, see Rust CARD_HALF_H) hovering the top
     ring stays inside the window. */
  .vertical .detail-tooltip {
    left: calc(var(--ring-size) + 12px);
    top: max(
      calc(
        var(--hover-index) * (var(--ring-size) + var(--ring-gap)) +
        var(--ring-size) / 2
      ),
      90px
    );
    transform: translateY(-50%);
  }

  /* Window grew leftward (fused right / overflow clamped): the card opens
     to the LEFT of the rings, into the screen interior. */
  .vertical.card-left .detail-tooltip {
    left: auto;
    right: calc(var(--ring-size) + 12px);
  }

  @keyframes tooltipIn {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(-50%) scale(1);
    }
  }

  /* ── Empty state ────────────────────────────────────────────────────── */

  .empty-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--pulse-text-dim);
    border-radius: 50%;
    opacity: 0.3;
  }

  .empty-icon {
    font-size: calc(var(--ring-size) * 0.4);
  }
</style>
