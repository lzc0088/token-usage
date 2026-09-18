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

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });

  let hoveredVendor = $state<string | null>(null);
  let isExpanded = $state(false);

  // Listen for data updates from Rust.
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
  });

  listen<string | null>("pulse:expand", (e) => {
    isExpanded = true;
    hoveredVendor = e.payload;
  });

  listen("pulse:collapse", () => {
    isExpanded = false;
    hoveredVendor = null;
  });

  // Initial data fetch.
  async function init() {
    // Data is pushed via event on sync; no separate invoke needed.
  }
  init();

  // ── Drag + berth fusion ─────────────────────────────────────────────
  // The panel is dragged with the native titlebar-style drag (startDragging),
  // so it keeps following the cursor even when the pointer outruns the tiny
  // webview. Berth side (which edge is flat) is derived from the live window
  // position; near an edge the window snaps flush so the flat side fuses
  // with the screen border.
  let berthClass = $state("pos-right");

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

  async function updateBerthClass() {
    try {
      const { pos, size, mon } = await readGeometry();
      if (!mon) {
        berthClass = "pos-right";
        return;
      }
      const centerX = pos.x + size.width / 2;
      const monCenter = mon.position.x + mon.size.width / 2;
      berthClass = centerX < monCenter ? "pos-left" : "pos-right";
    } catch {
      berthClass = "pos-right";
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

  // Track window moves (fires during native drag) to switch the berth side.
  $effect(() => {
    const unlisten = win.onMoved(() => updateBerthClass());
    return () => {
      unlisten.then((u) => u()).catch(() => {});
    };
  });

  // Initial berth detection.
  updateBerthClass();

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
  class:expanded={isExpanded}
  class:pos-right={berthClass === "pos-right"}
  class:pos-left={berthClass === "pos-left"}
  style="--ring-size: {data.ring_diameter}px"
>
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
        style="--hover-index: {hoveredIndex}; --ring-gap: 10px;"
      >
        <DetailCard quota={hoveredQuota} cardSide={berthClass === "pos-right" ? "right" : "left"} />
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

  /* ── Panel surface (berth fusion) ──────────────────────────────────── */

  .pulse-panel {
    position: relative;
    display: flex;
    width: fit-content;
    background: var(--pulse-bg);
    color: var(--pulse-text);
    user-select: none;
    -webkit-user-select: none;
    overflow: visible;
    cursor: grab; /* draggable via native titlebar-style drag */
    transition: border-radius 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
    /* Berth shape: fully rounded on the outer edge, flat on the screen-fused
       edge. Pulse fuses to the screen edge, so the edge touching the border
       is flat — no gap, no rounded corner showing wallpaper. */
    border-radius: 20px;
    backdrop-filter: blur(24px) saturate(180%);
    -webkit-backdrop-filter: blur(24px) saturate(180%);
  }

  /* Position-dependent berth: right edge is flat (fused), left is rounded.
     margin-left:auto right-anchors the panel inside the (wider, expanded)
     window so the rings don't jump when the window grows leftward. */
  .pulse-panel.pos-right {
    border-top-left-radius: 20px;
    border-bottom-left-radius: 20px;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    margin-left: auto;
    margin-right: -1px; /* fuse to screen edge */
  }

  .pulse-panel.pos-left {
    border-top-right-radius: 20px;
    border-bottom-right-radius: 20px;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    margin-left: -1px;
  }

  /* ── Layout ────────────────────────────────────────────────────────── */

  .vertical {
    flex-direction: column;
    align-items: center;
    padding: 14px 16px;
    gap: 8px;
  }

  /* ── Close button ──────────────────────────────────────────────────── */

  .close-btn {
    position: absolute;
    top: 6px;
    right: 6px;
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
    gap: 10px;
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

  /* ── Detail tooltip card ────────────────────────────────────────────── */

  .detail-tooltip {
    position: absolute;
    pointer-events: auto;
    z-index: 20;
    animation: tooltipIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  /* Card centers on the hovered ring (offsets are rail-relative — the rail's
     top-left is the first ring's top-left). max() floors the position so a
     tall card hovering the top ring stays inside the window. */
  .vertical .detail-tooltip {
    left: calc(var(--ring-size) + 12px);
    top: max(
      calc(
        var(--hover-index) * (var(--ring-size) + var(--ring-gap)) +
        var(--ring-size) / 2
      ),
      96px
    );
    transform: translateY(-50%);
  }

  /* Right-berth: panel sits at the right screen edge, so the card must open
     to the LEFT of the rings (into the screen interior). */
  .vertical.pos-right .detail-tooltip {
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

  /* ── Empty state ───────────────────────────────────────────────────── */

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
