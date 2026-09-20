<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
  import { PhysicalPosition } from "@tauri-apps/api/dpi";
  import QuotaRing from "../components/pulse/QuotaRing.svelte";
  import { fmtCredits, splitBalance } from "../lib/quota-format";
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
    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("mouseup", onMouseUp);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("mouseup", onMouseUp);
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
  style="--ring-size: {data.ring_diameter}px; --pulse-alpha: {data.opacity ?? 1}; width:{panelW}px; height:{panelH}px"
>
  <!-- Visual surface: bg + blur + radius. -->
  <div class="panel-surface" aria-hidden="true"></div>

  <!-- Panel title (mode indicator) -->
  <div class="panel-title">{panelTitle}</div>

  <!-- Ring rail -->
  <div class="rail-with-tooltip" role="group">
    <div class="ring-dock" style="max-height: {dockMax}px">
      {#each data.quotas as quota (quota.vendor)}
        <QuotaRing
          vendor={quota.vendor}
          pct={quota.critical_pct}
          label={quota.critical_label}
          diameter={data.ring_diameter}
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
     biased toward the edge (smaller padding on the fused side). */
  .pulse-panel.flush-right {
    padding: 30px 10px 30px 18px;
    margin-left: auto;
    margin-right: -1px;
  }

  .pulse-panel.flush-left {
    padding: 30px 18px 30px 10px;
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
    /* Pinned so the Rust RAIL_TOP (PAD_V + TITLE_BLOCK = 30 + 27) arrow
       math matches the rendered layout exactly. */
    height: 15px;
    line-height: 15px;
    font-size: 10px;
    font-weight: 600;
    color: var(--pulse-text);
    letter-spacing: 0.03em;
    text-transform: uppercase;
    margin-bottom: 4px;
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
