<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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
  // out, then unmount and hide this window ourselves — Rust's scheduled
  // hard-hide is only the safety net for a dead webview.
  listen("pulse:card-hide", () => {
    if (!card) return;
    hiding = true;
    if (hideTimer !== undefined) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => {
      hideTimer = undefined;
      hiding = false;
      card = null;
      getCurrentWindow().hide().catch(() => {});
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

  // Card measurement — center it so its arrow sits on the window's
  // vertical midline (which Rust aligned with the hovered ring). The
  // estimate keeps the placement sane even when ResizeObserver never
  // fires (known WKWebView flakiness); the real height refines it.
  let cardH = $state(0);
  let cardTop = $derived(Math.max(0, (CARD_WIN_H - (cardH || EST_CARD_H)) / 2));
  // Arrow line inside the card: the window's midline, translated into the
  // card's own coordinates.
  let arrowY = $derived(Math.max(12, CARD_WIN_H / 2 - cardTop));

  // The visible card is part of the hover area — the pointer resting on
  // it (reading the details) keeps it alive. The transparent window
  // margins report nothing (see template note).
  function onCardEnter() {
    invoke("pulse_activity").catch(() => {});
  }
  function onCardLeave() {
    invoke("pulse_idle").catch(() => {});
  }
</script>

<div class="pulse-card-root" class:dark={data.theme === "dark"}>
  {#if quota}
    <!-- Hover tracking lives on the VISIBLE card only: the window's
         transparent margins must stay inert, otherwise they form an
         invisible 340×480 hover surface that keeps the card alive (and
         moves the hide trigger) far beyond the panel's edge. -->
    <div
      class="card-slot"
      class:out={hiding}
      style="top: {cardTop}px"
      role="presentation"
      onmouseenter={onCardEnter}
      onmouseleave={onCardLeave}
      bind:clientHeight={cardH}
    >
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
     (the window never resizes). Show/hide animate the CONTENT — the
     window itself toggles invisibly. */
  .card-slot {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    justify-content: center;
    animation: cardIn 0.16s ease-out;
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
