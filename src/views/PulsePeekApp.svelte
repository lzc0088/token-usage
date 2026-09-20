<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { railPath } from "../lib/pulse-shapes";
  import type { PulseData } from "../lib/pulse-types";

  // Peek handle window ("pulse-peek", fixed 8×58 logical px). Auto-hide
  // mode's collapsed state: a tiny tinted silhouette with shoulder curves
  // growing out of the screen edge plus a 2×14 accent pill — the visible
  // affordance the cursor targets to reveal the full panel. Ported from
  // token-monitor's edgeDock peek surface. The window is NEVER resized
  // (fixed at creation), so only this static markup ever renders.

  // Layout constants (mirror Rust pulse.rs peek metrics).
  const PEEK_W = 8;
  const PEEK_H = 58; // 34 grip + 2×12 shoulders
  const SHOULDER = 12;
  const RADIUS = 3.5;

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });

  // Data flow mirrors PulseApp: event pushes + a mount/config re-pull (an
  // event-only window goes blank after a webview reload — mount must invoke).
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
  });
  invoke<PulseData>("get_pulse_data")
    .then((d) => {
      if (d) data = d;
    })
    .catch(() => {});
  listen("config:changed", () => {
    invoke<PulseData>("get_pulse_data")
      .then((d) => {
        if (d) data = d;
      })
      .catch(() => {});
  });

  // Side drives the silhouette's mirror (which edge the shoulders grow from).
  let side: "left" | "right" = $derived(data.side === "left" ? "left" : "right");
  let shape = $derived(railPath(PEEK_W, PEEK_H, side, SHOULDER, RADIUS));
</script>

<div
  class="peek-root"
  class:dark={data.theme === "dark"}
  class:light={data.theme !== "dark"}
  style="--pulse-alpha: {data.opacity ?? 1}"
  aria-label="额度侧边栏 — 移入光标展开"
>
  <!-- Silhouette: tinted fill + interior outline, born ON the screen edge
       exactly like the full panel's rail (shared railPath generator). -->
  <svg
    class="peek-shape"
    viewBox="0 0 {PEEK_W} {PEEK_H}"
    width={PEEK_W}
    height={PEEK_H}
    aria-hidden="true"
  >
    <path class="shape-fill" d={shape} />
    <path class="shape-line" d={shape} />
  </svg>
  <span class="peek-pill" class:pill-left={side === "left"}></span>
</div>

<style>
  /* The window is borderless + transparent — strip the popover chrome
     app.css paints on html/body/#app (opaque bg would fill the window). */
  :global(html:has(.peek-root)),
  :global(body:has(.peek-root)),
  :global(#app:has(.peek-root)) {
    background: transparent;
    border: none;
    border-radius: 0;
    box-shadow: none;
  }

  .peek-root {
    --pulse-bg: rgb(0 0 0 / var(--pulse-alpha, 1));
    --pulse-line: rgba(255, 255, 255, 0.15);
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  .peek-root.light {
    --pulse-bg: rgb(240 238 234 / var(--pulse-alpha, 1));
    --pulse-line: rgba(0, 0, 0, 0.1);
  }

  .peek-shape {
    position: absolute;
    inset: 0;
    overflow: visible;
    pointer-events: none;
  }
  .shape-fill {
    fill: var(--pulse-bg);
  }
  .shape-line {
    fill: none;
    stroke: var(--pulse-line);
    stroke-width: 1;
    stroke-linecap: round;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }

  /* Accent pill centered on the handle (token-monitor edge-dock-grip):
     2×14px, anchored to the panel-facing edge, grows on hover. */
  .peek-pill {
    position: absolute;
    top: 50%;
    right: 2px;
    width: 2px;
    height: 14px;
    margin-top: -7px;
    border-radius: 999px;
    background: rgba(120, 160, 255, 0.75);
    transition: transform 140ms ease;
  }
  .peek-pill.pill-left {
    right: auto;
    left: 2px;
  }
  .peek-root:hover .peek-pill {
    transform: scaleY(1.3);
  }
</style>
