<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import DetailCard from "../components/pulse/DetailCard.svelte";
  import type { PulseCardPayload, PulseData } from "../lib/pulse-types";

  // Card window root — the window itself is created hidden at a FIXED size
  // (tauri.conf.json "pulse-card") and only ever MOVED by Rust on hover.
  // This view renders the vendor card centered; the window's vertical
  // center sits on the hovered ring's line, so the arrow (rendered at the
  // card's ring-facing edge, on that same line) points straight at it.

  // Fixed window size — mirrors CARD_WIN_W/H in ui/pulse.rs.
  const CARD_WIN_H = 480;

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });
  let card = $state<PulseCardPayload | null>(null);

  listen<PulseCardPayload>("pulse:card", (e) => {
    card = e.payload;
    // Theme/opacity ride the payload (fresher than a full data pull).
    if (e.payload?.theme) {
      data = { ...data, theme: e.payload.theme };
    }
    if (e.payload?.opacity != null) {
      data = { ...data, opacity: e.payload.opacity };
    }
  });

  // Live usage / theme updates while the card happens to be open.
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
  });

  // Pull on mount: a webview reload / HMR resets component state, and the
  // next push may be minutes away — the card would sit empty until then.
  invoke<PulseData>("get_pulse_data")
    .then((d) => {
      if (d) data = d;
    })
    .catch(() => {
      // backend unavailable (e.g. browser dev) — event pushes still work
    });

  let quota = $derived(
    card ? data.quotas.find((q) => q.vendor === card!.vendor) ?? null : null
  );

  // Card measurement — center it so its arrow sits on the window's
  // vertical midline (which Rust aligned with the hovered ring).
  let cardH = $state(0);
  let cardTop = $derived(Math.max(0, (CARD_WIN_H - cardH) / 2));
  // Arrow line inside the card: the window's midline, translated into the
  // card's own coordinates.
  let arrowY = $derived(Math.max(12, CARD_WIN_H / 2 - cardTop));
</script>

<div class="pulse-card-root" class:dark={data.theme === "dark"}>
  {#if quota}
    <div class="card-slot" style="top: {cardTop}px" bind:clientHeight={cardH}>
      <DetailCard
        quota={quota}
        cardSide={card?.cardOnLeft ? "right" : "left"}
        dark={data.theme === "dark"}
        pulseAlpha={data.opacity ?? 1}
        arrowY={arrowY}
      />
    </div>
  {/if}
</div>

<style>
  /* Borderless transparent card window — strip the popover chrome
     app.css puts on html/body/#app (opaque bg would paint a rounded
     opaque rect over the whole window). */
  :global(html:has(.pulse-card-root)),
  :global(body:has(.pulse-card-root)),
  :global(#app:has(.pulse-card-root)) {
    background: transparent;
    border: none;
    border-radius: 0;
    box-shadow: none;
  }

  .pulse-card-root {
    width: 100vw;
    height: 100vh;
    user-select: none;
    -webkit-user-select: none;
  }

  /* The card centers horizontally; the inline `top` centers it vertically
     once the measured height arrives (the window never resizes). */
  .card-slot {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
  }
</style>
