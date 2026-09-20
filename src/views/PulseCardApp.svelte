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
  // Fallback card height until bind:clientHeight lands — WKWebView's
  // ResizeObserver is unreliable (known), and without a fallback the card
  // would sit at the window's midline with the arrow stuck at its top.
  const EST_CARD_H = 250;
  // Frontend fade timings — must stay under Rust's FADE_MS safety window.
  const FADE_OUT_MS = 150;

  let data = $state<PulseData>({
    quotas: [],
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
  });
  let card = $state<PulseCardPayload | null>(null);
  let hiding = $state(false);
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  listen<PulseCardPayload>("pulse:card", (e) => {
    // A fresh card event cancels any pending hide (re-hover in flight).
    if (hideTimer !== undefined) {
      clearTimeout(hideTimer);
      hideTimer = undefined;
    }
    hiding = false;
    card = e.payload;
    // Theme/opacity ride the payload (fresher than a full data pull).
    if (e.payload?.theme) {
      data = { ...data, theme: e.payload.theme };
    }
    if (e.payload?.opacity != null) {
      data = { ...data, opacity: e.payload.opacity };
    }
  });

  // Rust asks for a graceful hide (pointer left both pulse windows): fade
  // the content out and clear it — Rust parks the WINDOW off-screen after
  // the fade (parking keeps this webview live, so the next show is
  // instantaneous).
  listen("pulse:card-hide", () => {
    if (!card) return;
    hiding = true;
    if (hideTimer !== undefined) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      hideTimer = undefined;
      hiding = false;
      card = null;
    }, FADE_OUT_MS);
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

  // Dock-order index of the open card's vendor — drives the per-vendor
  // hue shared by the panel ring and this card's bars.
  let colorIndex = $derived(
    card
      ? Math.max(0, data.quotas.findIndex((q) => q.vendor === card!.vendor))
      : 0
  );

  // Card measurement — center it so its arrow sits on the window's
  // vertical midline (which Rust aligned with the hovered ring). The
  // estimate keeps the placement sane even when ResizeObserver never
  // fires (known WKWebView flakiness); the real height refines it.
  let cardH = $state(0);
  let cardTop = $derived(Math.max(0, (CARD_WIN_H - (cardH || EST_CARD_H)) / 2));

  // Report the measured height so the Rust hover poller can hit-test the
  // VISIBLE card — the window is larger than the card, and keeping it
  // alive over its transparent margins made the hide feel sluggish.
  $effect(() => {
    if (card && cardH > 0) {
      invoke("report_card_height", { height: cardH }).catch(() => {});
    }
  });
  // Arrow line inside the card: the window's midline, translated into the
  // card's own coordinates.
  let arrowY = $derived(Math.max(12, CARD_WIN_H / 2 - cardTop));

  // NOTE: hover keep-alive is handled by the Rust cursor poller — this
  // view only renders and animates.
</script>

<div class="pulse-card-root" class:dark={data.theme === "dark"}>
  {#if quota}
    <!-- The card hugs the window's panel-facing edge (never centered —
         centering drifts with card width and unsticks the arrow gap). -->
    <div
      class="card-slot"
      class:out={hiding}
      class:card-left={!!card?.cardOnLeft}
      style="top: {cardTop}px"
      bind:clientHeight={cardH}
    >
      <DetailCard
        quota={quota}
        cardSide={card?.cardOnLeft ? "right" : "left"}
        dark={data.theme === "dark"}
        pulseAlpha={data.opacity ?? 1}
        arrowY={arrowY}
        colorIndex={colorIndex}
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

  /* The inline `top` centers the card vertically (the window never
     resizes). Show/hide animate the CONTENT — the window itself toggles
     invisibly. */
  .card-slot {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    /* Default (cardOnLeft=false): the card window sits RIGHT of the
       panel, so the panel-facing edge is the window's LEFT — the card
       hugs it with 13px of arrow room. The arrow tip then lands 10px
       short of the panel edge (Rust CARD_GAP = -9: tip gap = GAP - 1),
       at a FIXED distance regardless of card width. */
    justify-content: flex-start;
    padding-left: 10px;
    animation: cardIn 0.16s ease-out;
  }

  /* Flush-right panel (cardOnLeft=true): the card window sits LEFT of
     the panel — the card hugs the window's RIGHT edge instead. */
  .card-slot.card-left {
    justify-content: flex-end;
    padding-left: 0;
    padding-right: 10px;
  }

  .card-slot.out {
    animation: cardOut 0.15s ease-in forwards;
  }

  @keyframes cardIn {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes cardOut {
    from {
      opacity: 1;
      transform: translateY(0);
    }
    to {
      opacity: 0;
      transform: translateY(2px);
    }
  }
</style>
