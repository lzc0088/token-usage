<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
  import { PhysicalPosition } from "@tauri-apps/api/dpi";
  import QuotaRing from "../components/pulse/QuotaRing.svelte";
  import { fmtCredits, splitBalance } from "../lib/quota-format";
  import { railPath } from "../lib/pulse-shapes";
  import type { PulseData, PulseQuota } from "../lib/pulse-types";

  // Ring panel window. The detail card lives in its OWN window
  // ("pulse-card", fixed size, only ever MOVED by Rust) — this window
  // NEVER resizes, which is what keeps hover ghost/flash-free: resizing a
  // transparent window whose content re-anchors (flush-right panel) shows
  // one stale web-process frame; pure moves never do.

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });

  // Listen for data updates from Rust.
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
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
  // panel follows the new theme/config without waiting for a push.
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
  const TITLE_BLOCK = 27;
  // Fixed vertical padding (logical px) — matches Rust PAD_V / PAD_V_BOT
  // (fixed, not scale-dependent; bottom is larger for visual breathing room).
  const PAD_V = 36;
  const PAD_V_BOT = 44;
  // Panel height cap fallback — Rust sends the real cap (80% of the
  // current screen) in the payload; the fixed value only covers a stale
  // payload from before the field existed.
  const MAX_PANEL_H_FALLBACK = 520;
  const PAD_FLUSH_INNER = 14;
  const PAD_FLUSH_EDGE = 10;
  const PAD_FLOAT = 16;

  // Layout scale — typography + vertical blocks track the ring preset
  // (mirrors Rust layout_scale = d / 48: 0.75 / 1 / 1.25), so fonts grow
  // and shrink with the size setting instead of clipping at small.
  let layoutS = $derived(data.ring_diameter / 48);

  let ringCount = $derived(Math.max(data.quotas.length, 1));
  let dockNatural = $derived(
    ringCount * (data.ring_diameter + LABEL_H * layoutS) +
      (ringCount - 1) * ITEM_GAP * layoutS
  );
  // Dock height capped by the screen-relative max (scrolls when overflow).
  let dockMax = $derived(
    Math.max(
      60,
      (data.max_panel_h ?? MAX_PANEL_H_FALLBACK) -
        (PAD_V + PAD_V_BOT + TITLE_BLOCK * layoutS)
    )
  );
  let dockH = $derived(Math.min(dockNatural, dockMax));
  let panelH = $derived(PAD_V + PAD_V_BOT + TITLE_BLOCK * layoutS + dockH);

  // Which screen edge the panel is fused to (null = floating pill).
  let flushSide = $state<"left" | "right" | null>(null);

  let padH = $derived(
    flushSide === "left" || flushSide === "right"
      ? PAD_FLUSH_INNER + PAD_FLUSH_EDGE
      : PAD_FLOAT * 2
  );
  let panelW = $derived(data.ring_diameter + padH);

  // Concave-shoulder rail silhouette (token-monitor style) — used for
  // clip-path and outline stroke when the panel is docked to a screen edge.
  const SHOULDER = 26;
  const RAIL_RADIUS = 20;
  let railClip = $derived(
    flushSide ? railPath(panelW, panelH, flushSide, SHOULDER, RAIL_RADIUS) : ""
  );
  // Clip-path needs a CLOSED shape — Z draws a straight line along the
  // screen edge from the path's end back to its start, which is exactly
  // the flat edge we want the fill to cover.
  let railClipClosed = $derived(railClip ? `${railClip} Z` : "");

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

  // Smoothly animate the window to a target position (ease-out cubic).
  // Native windows can't use CSS transitions — position is set by the OS —
  // so we drive it with rAF in small steps.
  function animateToPosition(
    targetX: number,
    targetY: number,
    duration = 180
  ): Promise<void> {
    return win.outerPosition().then((start) => {
      const sx = start.x;
      const sy = start.y;
      const dx = targetX - sx;
      const dy = targetY - sy;
      // Already there — nothing to animate.
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return Promise.resolve();
      const t0 = performance.now();
      return new Promise<void>((resolve) => {
        function frame() {
          const t = Math.min((performance.now() - t0) / duration, 1);
          // ease-out cubic: fast start, gentle settle.
          const e = 1 - (1 - t) * (1 - t) * (1 - t);
          win.setPosition(
            new PhysicalPosition(
              Math.round(sx + dx * e),
              Math.round(sy + dy * e)
            )
          ).catch(() => {});
          if (t < 1) requestAnimationFrame(frame);
          else resolve();
        }
        requestAnimationFrame(frame);
      });
    });
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
        await animateToPosition(monLeft, pos.y);
      } else if (monRight - (pos.x + size.width) <= threshold) {
        await animateToPosition(monRight - size.width, pos.y);
      }
    } catch {
      // ignore — position stays wherever the drag left it
    }
  }

  $effect(() => {
    const onMouseDown = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      // Drag only from panel chrome (title, padding), never from rings.
      if (!target?.closest(".pulse-panel")) return;
      if (target.closest(".ring-container")) return;
      win.startDragging().catch(() => {});
    };
    const onMouseUp = () => {
      // Persist after a drag (the 1s Rust poller is the safety net).
      snapToEdge().then(() => {
        win
          .outerPosition()
          .then((pos) => invoke("set_pulse_position", { x: pos.x, y: pos.y }))
          .catch(() => {});
      });
    };
    // Suppress the WebView2 native context menu (refresh / save as / …) —
    // the panel is a HUD widget, not a document.
    const onContextMenu = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("contextmenu", onContextMenu);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("contextmenu", onContextMenu);
    };
  });

  // Track window moves (fires during native drag) to recompute which edge
  // is fused.
  $effect(() => {
    const unlisten = win.onMoved(() => updateFlushSide());
    return () => {
      unlisten.then((u) => u()).catch(() => {});
    };
  });

  // Initial flush detection.
  updateFlushSide();

  // NOTE: hover show/hide is driven entirely by the Rust-side cursor
  // poller (ui/pulse.rs ensure_hover_poller) — DOM hover events in the
  // non-key webview proved unreliable after the two-window split. This
  // window reports nothing; it only render the rings.

  // Panel-level title explaining the percentage mode — the rings report
  // what is LEFT (usage-remaining mode; plan-less vendors show their
  // balance/credits amount either way).
  let panelTitle = $derived("剩余量");

  // Windows renders differently in two ways we compensate for via a `win`
  // class: WebView2 over a transparent window can't sample the desktop, so
  // backdrop-filter shows as a flat gray glass block instead of a blur
  // (disabled there); classic scrollbars take LAYOUT width, not overlay
  // (dock scrollbar hidden below).
  const isWindows =
    typeof navigator !== "undefined" && /Win/.test(navigator.platform ?? "");

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
  class:win={isWindows}
  style="--ring-size: {data.ring_diameter}px; --pulse-alpha: {data.opacity ?? 1}; --s: {layoutS}; width:{panelW}px; height:{panelH}px"
>
  <!-- Visual surface: bg + blur.  Docked edges use the concave-shoulder
       rail clip-path (token-monitor style); floating pills use border-radius.
       The auto-hide peek handle lives in its OWN tiny window ("pulse-peek"),
       so this window never renders a collapsed state. -->
  <!-- Hidden SVG: defines the <clipPath> for docked panel edges. -->
  {#if flushSide && railClip}
    <svg
      class="rail-defs"
      width="0"
      height="0"
      aria-hidden="true"
    >
      <defs>
        <clipPath id="rail-clip" clipPathUnits="userSpaceOnUse">
          <path d={railClipClosed} />
        </clipPath>
      </defs>
    </svg>
  {/if}

  <div class="panel-surface" aria-hidden="true">
    {#if flushSide && railClip}
      <!-- Outline traces the interior (concave shoulders + rounded
           corners); the screen edge is left open for a clean docked look. -->
      <svg
        class="rail-outline"
        viewBox="0 0 {panelW} {panelH}"
        width={panelW}
        height={panelH}
        aria-hidden="true"
      >
        <path
          d={railClip}
          fill="none"
          stroke="var(--rail-stroke, rgba(255,255,255,0.15))"
          stroke-width="1"
          vector-effect="non-scaling-stroke"
          pointer-events="none"
        />
      </svg>
    {/if}
  </div>

  <!-- Panel title (mode indicator) -->
  <div class="panel-title">{panelTitle}</div>

  <!-- Ring rail -->
  <div class="rail-with-tooltip" role="group">
    <div class="ring-dock" style="max-height: {dockMax}px">
      {#each data.quotas as quota, index (quota.vendor)}
        <QuotaRing
          vendor={quota.vendor}
          pct={quota.critical_pct}
          label={quota.critical_label}
          diameter={data.ring_diameter}
          colorIndex={index}
          isRunning={quota.is_running ?? false}
          isRefreshing={quota.is_refreshing ?? false}
          extraPcts={extraPcts(quota)}
          showsRemaining={true}
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
  </div>
</div>

<style>
  /* The pulse window is borderless + transparent. Strip the popover chrome
     app.css puts on html/body/#app — the opaque bg, border and 15px radius
     would paint a rounded opaque rect over the whole window and clip the
     fused flat edge. */
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
    /* rgb() / syntax (CSS Color Module Level 4): rgba() with a var() alpha
       is unreliable in WKWebView and can silently fall back to opaque —
       the "sometimes transparent, sometimes not" symptom. */
    --pulse-bg: rgb(0 0 0 / var(--pulse-alpha, 1));
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
    --pulse-bg: rgb(240 238 234 / var(--pulse-alpha, 1));
    --pulse-text: #1a1610;
    --pulse-text-dim: #7a756c;
    --pulse-track: rgba(0, 0, 0, 0.10);
    --pulse-bar-track: rgba(0, 0, 0, 0.07);
  }

  /* ── Panel shell ──────────────────────────────────────────────────────
     Visual surface (bg + blur + shape) lives on .panel-surface; the shell
     itself stays simple. */

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
  }

  /* Windows: WebView2 over a transparent window can't sample the desktop
     behind it, so the blur renders as a flat gray glass block — drop it
     and let the rings float without a visible background panel. */
  .pulse-panel.win .panel-surface {
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
    background: transparent;
  }
  /* Rail outline is also hidden on Windows — with a transparent bg the
     stroke would draw an empty pill silhouette around nothing. */
  .pulse-panel.win .panel-surface .rail-outline {
    display: none;
  }

  /* Rail silhouette — clip-path replaces border-radius for docked edges;
     the concave-shoulder S-curve grows out of the screen border. */
  .pulse-panel.flush-right .panel-surface,
  .pulse-panel.flush-left .panel-surface {
    border-radius: 0;
    clip-path: url(#rail-clip);
  }

  /* Hidden SVG holding the <clipPath> definition — zero-size, no visual. */
  .rail-defs {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
  }

  .rail-outline {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  /* Light theme: darker outline. */
  .pulse-panel.light .rail-outline {
    --rail-stroke: rgba(0, 0, 0, 0.1);
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
    /* Fixed vertical padding — does not scale with the ring preset so the
       spacing feels consistent across small / medium / large. */
    padding: 36px 16px 44px 16px;
    gap: calc(8px * var(--s, 1));
  }

  /* Fused berths: flat edge kisses the screen border (-1px overlap), rings
     biased toward the edge (smaller padding on the fused side). */
  .pulse-panel.flush-right {
    padding: 36px 10px 44px 14px;
    margin-left: auto;
    margin-right: -1px;
  }

  .pulse-panel.flush-left {
    padding: 36px 14px 44px 10px;
    margin-left: -1px;
  }

  /* ── Ring dock ─────────────────────────────────────────────────────── */

  .rail-with-tooltip {
    position: relative;
  }

  .ring-dock {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: calc(14px * var(--s, 1));
    /* Scroll when the vendor count would exceed the panel height cap —
       but the BAR is hidden: on Windows classic scrollbars take ~17px of
       LAYOUT width (macOS overlay bars take none), covering the rings.
       Wheel/drag still scrolls. */
    overflow-y: auto;
    scrollbar-width: none;
  }
  .ring-dock::-webkit-scrollbar {
    display: none;
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
    /* Pinned block (× --s) so the Rust RAIL_TOP (PAD_V + TITLE_BLOCK = 33
       + 27, scaled) arrow math matches the rendered layout exactly. */
    height: calc(15px * var(--s, 1));
    line-height: calc(15px * var(--s, 1));
    font-size: calc(10px * var(--s, 1));
    font-weight: 600;
    color: var(--pulse-text);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    margin-bottom: calc(4px * var(--s, 1));
    align-self: center;
    text-align: center;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
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
