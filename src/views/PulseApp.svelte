<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
  import { PhysicalPosition } from "@tauri-apps/api/dpi";
  import QuotaRing from "../components/pulse/QuotaRing.svelte";
  import DetailCard from "../components/pulse/DetailCard.svelte";
  import { fmtCredits, splitBalance } from "../lib/quota-format";

  interface PulseWindow {
    label: string;
    used_pct: number;
    resets_at?: string;
    /** Absolute used/total (e.g. credits) for "剩余 X" display. */
    used_value?: number;
    total_value?: number;
  }

  interface PulseBalance {
    amount: number;
    currency: string;
    /** Today / month spend (API vendors, e.g. DeepSeek). */
    today_consumption?: number;
    month_consumption?: number;
  }

  interface PulseQuota {
    vendor: string;
    plan?: string;
    /** Credits/balance-only vendor: amount under the ring, no progress arc. */
    planless?: boolean;
    status: string;
    windows: PulseWindow[];
    critical_pct: number;
    critical_label: string;
    balance?: PulseBalance;
    /** Subscription plan expiry (RFC3339) — shown beside the plan badge. */
    expires_at?: string;
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
    /** Surface opacity (0.2–1.0), from config. */
    opacity?: number;
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
    // Fast pass-through guard: the pointer may have already left the panel
    // before this (async) event landed — mounting the card now would flash.
    if (!pointerInPanel) {
      invoke("collapse_pulse").catch(() => {});
      return;
    }
    isExpanded = true;
    expandCardLeft = !!e.payload?.cardLeft;
    // Pre-warm expands carry vendor=null — their only job is growing the
    // window. Entering the panel directly onto a ring fires BOTH the
    // pre-warm and the vendor expand; the two events can arrive out of
    // order, so a null payload must never clobber an already-hovered ring
    // (that unmounts the card → "hover shows nothing").
    const vendor = e.payload?.vendor ?? null;
    if (vendor !== null) hoveredVendor = vendor;
  });

  listen("pulse:collapse", () => {
    isExpanded = false;
    hoveredVendor = null;
    expandCardLeft = false;
    if (collapseTimer !== undefined) {
      clearTimeout(collapseTimer);
      collapseTimer = undefined;
    }
  });

  // Pull data on mount. A webview reload / HMR resets component state, and
  // the next `pulse:update` push may be minutes away (quota refresh
  // interval) — without this the panel would sit empty until then.
  invoke<PulseData>("get_pulse_data")
    .then((d) => {
      if (d) data = d;
    })
    .catch(() => {
      // backend unavailable (e.g. browser dev) — event pushes still work
    });

  // Settings changed (theme, vendors, order, …) → re-pull immediately so the
  // panel + card follow the new theme/config without waiting for a push.
  listen("config:changed", () => {
    invoke<PulseData>("get_pulse_data")
      .then((d) => {
        if (d) data = d;
      })
      .catch(() => {});
  });

  // ── Layout constants (mirror Rust pulse.rs) ────────────────────────────
  // Ring item = ring + LABEL_H (pct label); items separated by ITEM_GAP.
  const LABEL_H = 21;
  const ITEM_GAP = 14;
  const TITLE_BLOCK = 24;
  // Fixed vertical padding (user-specified: padding-top 20px).
  const PAD_V = 30;
  // Panel height cap — the ring dock scrolls beyond this.
  const MAX_PANEL_H = 520;
  const PAD_FLUSH_INNER = 18;
  const PAD_FLUSH_EDGE = 10;
  const PAD_FLOAT = 16;

  let ringCount = $derived(Math.max(data.quotas.length, 1));
  let dockNatural = $derived(
    ringCount * (data.ring_diameter + LABEL_H) + (ringCount - 1) * ITEM_GAP
  );
  // Dock height capped by MAX_PANEL_H (scrolls when overflow).
  let dockMax = $derived(Math.max(60, MAX_PANEL_H - PAD_V * 2 - TITLE_BLOCK));
  let dockH = $derived(Math.min(dockNatural, dockMax));
  let panelH = $derived(PAD_V * 2 + TITLE_BLOCK + dockH);

  // Which screen edge the panel is fused to (null = floating pill).
  let flushSide = $state<"left" | "right" | null>(null);

  let padH = $derived(
    flushSide === "left" || flushSide === "right"
      ? PAD_FLUSH_INNER + PAD_FLUSH_EDGE
      : PAD_FLOAT * 2
  );
  let panelW = $derived(data.ring_diameter + padH);

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
      if (target.closest(".ring-container, .detail-tooltip")) return;
      win.startDragging().catch(() => {});
    };
    const onMouseUp = () => {
      // Persist after a drag (the 1s Rust poller is the safety net). Skip
      // while expanded — the window is lifted, y would be wrong.
      if (isExpanded) return;
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

  // ── Graceful collapse + fast-sweep protection ──────────────────────────
  // Linger: after leaving, do NOTHING for LINGER_MS — sweeping across the
  //   panel (or over the title strip between rings) re-enters within this
  //   window and nothing was ever hidden → zero flicker.
  // After the linger the card unmounts instantly (no animation), and the
  //   window shrinks one painted frame LATER — a same-tick resize squeezes
  //   the still-rendered card and leaves a ghost.
  // `pointerInPanel` additionally guards the async pulse:expand event: a
  // pass-through hover that already left never mounts the card at all.
  const LINGER_MS = 120;
  let collapseTimer: ReturnType<typeof setTimeout> | undefined;
  let pointerInPanel = $state(false);

  function cancelPendingCollapse() {
    if (collapseTimer !== undefined) {
      clearTimeout(collapseTimer);
      collapseTimer = undefined;
    }
  }

  function onWrapperEnter() {
    pointerInPanel = true;
    cancelPendingCollapse();
    // Pre-warm: grow the window BEFORE any ring is hovered, while nothing
    // is visible to animate — the (transparent) resize happens with no card
    // on screen, so entering the panel from outside never flashes. The
    // subsequent ring-hover expand is then a no-op resize (Rust guard).
    invoke("expand_pulse", { vendor: null }).catch(() => {});
  }

  function onRingEnter(vendor: string) {
    pointerInPanel = true;
    cancelPendingCollapse();
    // hoveredVendor + isExpanded are set by the pulse:expand listener once
    // Rust confirms the window is ready — setting them here races with the
    // async event and the pre-warm expand (vendor=null) which would
    // overwrite hoveredVendor with null.
    invoke("expand_pulse", { vendor: vendor }).catch(() => {});
  }

  function onRingLeave() {
    // Intentionally no-op: clearing the vendor here unmounts the card for a
    // frame between adjacent rings. The wrapper's mouseleave collapses.
  }

  function onWrapperLeave() {
    pointerInPanel = false;
    collapseTimer = setTimeout(() => {
      collapseTimer = undefined;
      hoveredVendor = null; // unmount the card first…
      // …then shrink the window only after that unmount has PAINTED —
      // resizing in the same tick squeezes the still-rendered card and
      // leaves a one-frame ghost. Double-rAF waits one painted frame.
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          if (pointerInPanel) return; // re-entered mid-flight: keep expanded
          invoke("collapse_pulse").catch(() => {});
        })
      );
    }, LINGER_MS);
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

  // ── Card vertical placement (window never moves — the card fits inside
  //    the pre-expanded window; the arrow slides along the card edge to
  //    point at the hovered ring). ─────────────────────────────────────────
  // NOTE: coordinates are RELATIVE TO .rail-with-tooltip (the tooltip's
  // positioned ancestor, which already sits below the title block) — do NOT
  // add PAD_V/TITLE_BLOCK here or the card lands ~54px below the ring.
  const EST_CARD_H = 170; // pre-measure fallback for the first frame

  let cardH = $state(0); // measured via bind:clientHeight on the tooltip
  // Hide-until-measured, re-armed on every fresh appearance: a stale
  // height from the previous card would place this one at the wrong `top`
  // for a frame — the appear "ghost". If the measurement never lands
  // (ResizeObserver hiccup), stop hiding after 80ms — a permanently
  // invisible card reads as "hover shows nothing".
  let measureGuaranteed = $state(false);
  $effect(() => {
    if (hoveredVendor === null) {
      cardH = 0;
      measureGuaranteed = false;
      return;
    }
    const t = setTimeout(() => {
      measureGuaranteed = true;
    }, 80);
    return () => clearTimeout(t);
  });

  // Hovered ring's center Y within the rail (index·pitch + half the ring).
  let ringY = $derived(
    hoveredIndex < 0
      ? 0
      : hoveredIndex * (data.ring_diameter + LABEL_H + ITEM_GAP) + data.ring_diameter / 2
  );

  // Card top: centered on the ring; the Rust expand reserves 140px of slack
  // BELOW the panel so no upper clamp is needed beyond ≥ 0.
  let cardTop = $derived.by(() => {
    if (hoveredIndex < 0) return 0;
    return Math.max(0, ringY - (cardH || EST_CARD_H) / 2);
  });

  // Arrow position on the card edge: where the ring actually is.
  let arrowY = $derived.by(() => {
    const h = cardH || EST_CARD_H;
    return Math.max(12, Math.min(ringY - cardTop, h - 12));
  });

  // Panel-level title explaining the percentage mode.
  let panelTitle = $derived("使用量");

  // Extra windows (week / MCP / …) as inner concentric arcs: all windows
  // sorted by usage, minus the most critical one (that's the main arc).
  function extraPcts(q: PulseQuota): number[] {
    return q.windows
      .map((w) => w.used_pct)
      .sort((a, b) => b - a)
      .slice(1);
  }

  // Plan-less vendors (credits/balance only): the label under the ring
  // shows the remaining amount instead of a percentage (unit smaller).
  function ringSubLabel(q: PulseQuota): { unit: string; value: string } | undefined {
    if (!q.planless) return undefined;
    if (q.balance) {
      return splitBalance(q.balance.currency, q.balance.amount);
    }
    const w = q.windows.find(
      (win) => win.total_value != null && win.used_value != null
    );
    return w
      ? { unit: "", value: fmtCredits(w.total_value! - w.used_value!) }
      : undefined;
  }
</script>

<div
  class="pulse-panel vertical"
  class:dark={data.theme === "dark"}
  class:light={data.theme !== "dark"}
  class:flush-right={flushSide === "right"}
  class:flush-left={flushSide === "left"}
  class:grow-left={expandCardLeft}
  class:card-left={expandCardLeft}
  style="--ring-size: {data.ring_diameter}px; --pulse-alpha: {data.opacity ?? 1}; width:{panelW}px; height:{panelH}px"
>
  <!-- Visual surface: bg + blur + radius. Its own layer so the tooltip
       card can overflow the panel body. -->
  <div class="panel-surface" aria-hidden="true"></div>

  <!-- Panel title (mode indicator) -->
  <div class="panel-title">{panelTitle}</div>

  <!-- Ring rail + tooltip container -->
  <div
    class="rail-with-tooltip"
    onmouseenter={onWrapperEnter}
    onmouseleave={onWrapperLeave}
    role="group"
  >
    <div class="ring-dock" style="max-height: {dockMax}px">
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
          extraPcts={extraPcts(quota)}
          showsRemaining={false}
          subUnit={ringSubLabel(quota)?.unit}
          subValue={ringSubLabel(quota)?.value}
          plain={quota.planless ?? false}
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

    <!-- Detail tooltip card — vertically centered on the hovered ring and
         clamped inside the (pre-expanded) window; the arrow slides along
         the card edge to point straight at the ring. -->
    {#if isExpanded && hoveredQuota && hoveredIndex >= 0}
      <div
        class="detail-tooltip"
        class:measuring={cardH === 0 && !measureGuaranteed}
        bind:clientHeight={cardH}
        style="top: {cardTop}px; --arrow-y: {arrowY}px"
      >
        <DetailCard quota={hoveredQuota} cardSide={expandCardLeft ? "right" : "left"} dark={data.theme === "dark"} pulseAlpha={data.opacity ?? 1} />
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
    --pulse-bg: rgba(0, 0, 0, var(--pulse-alpha, 1));
    --pulse-ring-bg: rgba(0, 0, 0, var(--pulse-alpha, 1));
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
    --pulse-bg: rgba(240, 238, 234, var(--pulse-alpha, 1));
    --pulse-ring-bg: rgba(240, 238, 234, var(--pulse-alpha, 1));
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
    border-radius: 50px;
    backdrop-filter: blur(24px) saturate(180%);
    -webkit-backdrop-filter: blur(24px) saturate(180%);
    pointer-events: none;
    transition: border-radius 0.3s ease;
  }

  /* Fused berths: rounded on the screen-interior side, flat on the edge. */
  .pulse-panel.flush-right .panel-surface {
    border-radius: 50px 0 0 50px;
  }

  .pulse-panel.flush-left .panel-surface {
    border-radius: 0 50px 50px 0;
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
    padding: 30px 16px;
    gap: 8px;
  }

  /* Fused berths: flat edge kisses the screen border (-1px overlap), rings
     biased toward the edge (smaller padding on the fused side). The window
     keeps a few px of slack, so right-fusing also right-anchors the panel. */
  .pulse-panel.flush-right {
    padding: 30px 10px 30px 18px;
    margin-left: auto;
    margin-right: -1px;
  }

  .pulse-panel.flush-left {
    padding: 30px 18px 30px 10px;
    margin-left: -1px;
  }

  /* Right-fused expand: macOS anchors resizes top-left, so Rust shifts the
     window leftward — right-anchor the panel so the rings don't jump. */
  .pulse-panel.grow-left {
    margin-left: auto;
    margin-right: -1px;
  }

  /* ── Ring dock ─────────────────────────────────────────────────────── */

  .rail-with-tooltip {
    position: relative;
  }

  .ring-dock {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 14px;
    /* Scroll when the vendor count would exceed the panel height cap. */
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .ring-dock::-webkit-scrollbar {
    width: 3px;
  }

  .ring-dock::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.18);
    border-radius: 2px;
  }

  /* ── Panel title ────────────────────────────────────────────────────── */

  .panel-title {
    font-size: 10px;
    font-weight: 500;
    color: var(--pulse-text);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    margin-bottom: 4px;
    align-self: center;
    text-align: center;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  /* ── Detail tooltip card ────────────────────────────────────────────── */

  .detail-tooltip {
    position: absolute;
    pointer-events: auto;
    z-index: 20;
  }

  /* Pre-measure frame: the estimated height can differ from the real card,
     so stay invisible until bind:clientHeight lands (one frame) — this
     kills the position-jump flash on first hover. */
  .detail-tooltip.measuring {
    opacity: 0;
  }

  /* Card sits 30px clear of the panel's INNER edge (= pad_inner 18 + 30 =
     48px from the ring rail). Vertical position comes from inline `top`
     (ring-centered, clamped inside the window) — the window itself never
     moves, so there is nothing to compensate for. */
  .vertical .detail-tooltip {
    left: calc(var(--ring-size) + 48px);
  }

  /* Window grew leftward (fused right / overflow clamped): the card opens
     to the LEFT of the rings, into the screen interior. */
  .vertical.card-left .detail-tooltip {
    left: auto;
    right: calc(var(--ring-size) + 48px);
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
