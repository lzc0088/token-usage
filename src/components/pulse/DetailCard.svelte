<script lang="ts">
  import { vendorDisplayName, vendorIconMarkup } from "../../lib/vendorIcons";

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
  }

  interface Props {
    quota: PulseQuota;
    cardSide?: "left" | "right";
  }

  let { quota, cardSide = "left" }: Props = $props();

  let displayName = $derived(vendorDisplayName(quota.vendor));
  // Real brand SVG from the app's shared icon set (currentColor fill).
  let iconMarkup = $derived(vendorIconMarkup(quota.vendor));

  function barColor(pct: number): string {
    if (pct >= 80) return "var(--pulse-warning)";
    if (pct >= 50) return "var(--pulse-caution)";
    return "var(--pulse-good)";
  }

  // Format relative reset time.
  function relativeReset(iso?: string): string | null {
    if (!iso) return null;
    try {
      const reset = new Date(iso);
      const now = new Date();
      const diff = reset.getTime() - now.getTime();
      if (diff <= 0) return "Resets now";
      const hours = Math.floor(diff / 3_600_000);
      const mins = Math.floor((diff % 3_600_000) / 60_000);
      const days = Math.floor(hours / 24);
      if (days > 0) return `Resets in ${days}d ${hours % 24}h`;
      if (hours > 0) return `Resets in ${hours}h${mins > 0 ? " " + mins + "m" : ""}`;
      return `Resets in ${mins}m`;
    } catch {
      return null;
    }
  }

  // Split label into quota type + model name subtitle.
  function splitLabel(raw: string): { type: string; subtitle: string } {
    const idx = raw.indexOf("·");
    if (idx >= 0) {
      return {
        type: raw.slice(0, idx).trim(),
        subtitle: raw.slice(idx + 1).trim(),
      };
    }
    // Fallback: try " - " separator
    const dash = raw.indexOf(" - ");
    if (dash >= 0) {
      return {
        type: raw.slice(0, dash).trim(),
        subtitle: raw.slice(dash + 3).trim(),
      };
    }
    return { type: raw, subtitle: "" };
  }

  </script>

<div class="detail-card">
  <!-- Pointer shape (curved tail pointing toward the ring) -->
  <svg
    class="card-pointer"
    viewBox="0 0 20 40"
    preserveAspectRatio="none"
    aria-hidden="true"
    class:flip={cardSide === "right"}
  >
    <path
      d="M 20,0 C 14,10 6,15 0,20 C 6,25 14,30 20,40 Z"
      fill="var(--pulse-card-bg)"
    />
  </svg>

  <!-- Card body -->
  <div class="card-content">
    <div class="card-header">
      <span class="vendor-icon" aria-hidden="true">{@html iconMarkup}</span>
      <span class="vendor-name">{displayName}</span>
      {#if quota.plan}
        <span class="plan-badge">{quota.plan}</span>
      {/if}
    </div>

    <div class="window-list">
      {#each quota.windows as win (win.label)}
        {@const { type: winType, subtitle } = splitLabel(win.label)}
        <div class="window-row">
          <div class="window-top">
            <span class="window-label">{winType}</span>
            <span class="window-pct" style="color:{barColor(win.used_pct)}">
              {Math.round(win.used_pct)}%
            </span>
          </div>
          {#if subtitle}
            <span class="model-name">{subtitle}</span>
          {/if}
          <div class="bar-track">
            <div
              class="bar-fill"
              style="width:{Math.min(win.used_pct, 100)}%;background:{barColor(win.used_pct)}"
            ></div>
          </div>
          {#if relativeReset(win.resets_at)}
            <span class="reset-time">{relativeReset(win.resets_at)}</span>
          {/if}
        </div>
      {/each}
    </div>

    {#if quota.balance}
      <div class="balance-row">
        <span class="balance-label">余额</span>
        <span class="balance-amount">
          {quota.balance.currency} {quota.balance.amount.toFixed(2)}
        </span>
      </div>
    {/if}
  </div>
</div>

<style>
  .detail-card {
    position: relative;
    display: flex;
    background: var(--pulse-card-bg);
    border-radius: 14px;
    min-width: 200px;
    max-width: 236px;
    animation: cardIn 0.22s cubic-bezier(0.34, 1.56, 0.64, 1);
    color: var(--pulse-text);
    backdrop-filter: blur(16px) saturate(180%);
    -webkit-backdrop-filter: blur(16px) saturate(180%);
    border: 1px solid rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
  }

  @keyframes cardIn {
    from {
      opacity: 0;
      transform: translateY(-5px) scale(0.95);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* ── Pointer ────────────────────────────────────────────────────────
     Vertically centered on the card: the card is centered on the hovered
     ring, so the centered beak points straight at the ring. */

  .card-pointer {
    position: absolute;
    top: 50%;
    left: -17px;
    width: 18px;
    height: 36px;
    transform: translateY(-50%);
    flex-shrink: 0;
    pointer-events: none;
  }

  .card-pointer.flip {
    left: auto;
    right: -17px;
    transform: translateY(-50%) scaleX(-1);
  }

  /* ── Card content ──────────────────────────────────────────────────── */

  .card-content {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 200px;
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 0;
  }

  .vendor-icon {
    width: 15px;
    height: 15px;
    color: var(--pulse-text);
    flex-shrink: 0;
  }

  .vendor-icon :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }

  .vendor-name {
    font-size: 13px;
    font-weight: 600;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
    letter-spacing: -0.01em;
  }

  .plan-badge {
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--pulse-text-dim);
    font-weight: 500;
    letter-spacing: 0.01em;
  }

  .window-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    /* Cap at ~3 rows so tall vendors scroll instead of overflowing the
       window (Rust sizes the expanded window for CARD_HALF_H = 128). */
    max-height: 154px;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .window-list::-webkit-scrollbar {
    width: 4px;
  }

  .window-list::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 2px;
  }

  .window-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .window-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .window-label {
    font-size: 11px;
    color: var(--pulse-text-dim);
    font-weight: 500;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  .model-name {
    font-size: 10px;
    color: var(--pulse-text-dim);
    opacity: 0.7;
    font-weight: 400;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
    margin-top: -1px;
  }

  .window-pct {
    font-size: 11px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    letter-spacing: -0.02em;
  }

  .bar-track {
    height: 4px;
    background: var(--pulse-bar-track);
    border-radius: 2px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .reset-time {
    font-size: 9px;
    color: var(--pulse-text-dim);
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    opacity: 0.6;
    margin-top: 1px;
  }

  .balance-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-top: 2px;
    padding-top: 6px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .balance-label {
    font-size: 10px;
    color: var(--pulse-text-dim);
    font-weight: 400;
  }

  .balance-amount {
    font-size: 12px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    letter-spacing: -0.01em;
  }
</style>
