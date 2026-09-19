<script lang="ts">
  import {
    windowLabel,
    fmtCredits,
    formatShortExpiry,
    formatReset,
    splitBalance,
  } from "../../lib/quota-format";
  import { vendorIconMarkup, vendorDisplayName } from "../../lib/vendorIcons";

  interface PulseWindow {
    label: string;
    used_pct: number;
    resets_at?: string;
    used_value?: number;
    total_value?: number;
  }

  interface PulseBalance {
    amount: number;
    currency: string;
    today_consumption?: number;
    month_consumption?: number;
  }

  interface PulseQuota {
    vendor: string;
    plan?: string;
    /** Credits/balance-only vendor: amount rows only, no bars/reset. */
    planless?: boolean;
    status: string;
    windows: PulseWindow[];
    critical_pct: number;
    critical_label: string;
    balance?: PulseBalance;
    expires_at?: string;
  }

  interface Props {
    quota: PulseQuota;
    cardSide: "left" | "right";
    /** Follow the app theme (dark card + light text when true). */
    dark?: boolean;
    /** Panel opacity (0.2–1.0) — applied to the card background. */
    pulseAlpha?: number;
    /** Arrow's line offset from the card top (px) — points at the ring. */
    arrowY?: number;
  }

  const {
    quota,
    cardSide,
    dark = false,
    pulseAlpha = 1,
    arrowY,
  }: Props = $props();
  const nowMs = Date.now();

  // First window reporting absolute credits (plan-less vendors).
  let creditsWin = $derived(
    quota.windows.find(
      (w) => w.total_value != null && w.used_value != null
    ) ?? null
  );

  // Bar color thresholds, theme-aware.
  function barColor(pct: number): string {
    if (pct >= 80) return dark ? "#ff4f42" : "#cc2200";
    if (pct >= 50) return dark ? "#ffc226" : "#cc8800";
    return dark ? "#00e68a" : "#00b36b";
  }
</script>

<div
  class="detail-card"
  class:card-right={cardSide === "right"}
  class:dark={dark}
  style:--card-alpha={pulseAlpha}
  style:--arrow-y={arrowY != null ? `${arrowY}px` : "50%"}
>
  <!-- Surface layer: bg + border + tail in ONE compositing layer. The
       panel-matching alpha is applied via `opacity` here, so the tail's
       overlap with the body never double-stacks translucency, and the
       card border can't show through the junction (no dividing line). -->
  <div class="card-surface" aria-hidden="true">
    <!-- Arrow: large triangle protruding from the card's panel-facing
         edge, positioned at --arrow-y (the hovered ring's line). -->
    <div class="card-arrow"></div>
  </div>

  <div class="card-body">
    <!-- Header: icon + vendor + plan badge; expiry hugs the line above. -->
    <div class="card-header">
      <div class="header-line">
        <span class="vendor-icon">{@html vendorIconMarkup(quota.vendor)}</span>
        <span class="vendor-name">{vendorDisplayName(quota.vendor)}</span>
        {#if quota.plan}
          <span class="plan-badge">{quota.plan}</span>
        {/if}
      </div>
      {#if quota.expires_at}
        <div class="expiry-line">
          <span>到期 {formatShortExpiry(quota.expires_at)}</span>
        </div>
      {/if}
    </div>

    {#if !quota.planless && quota.windows.length > 0}
      <!-- Plan vendors — 三行左对齐: 标题 · 进度条+百分比 · 剩余(居中) -->
      <div class="window-bars">
        {#each quota.windows as win}
          <div class="window-section">
            <div class="ws-title">{windowLabel(win.label)}</div>
            <div class="ws-bar-row">
              <div class="ws-track">
                <div
                  class="ws-fill"
                  style="width:{Math.min(100, win.used_pct)}%;background:{barColor(win.used_pct)}"
                ></div>
              </div>
              <span class="ws-pct">{win.used_pct.toFixed(2)}<span class="pct-sign">%</span></span>
            </div>
            <div class="ws-value">
              {#if win.total_value != null && win.used_value != null}
                <span>剩余 {fmtCredits(win.total_value - win.used_value)}</span>
              {/if}
              {#if win.resets_at}
                <span>{formatReset(win.resets_at, nowMs)}</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {:else if quota.planless}
      <!-- Credits/balance-only vendors (no plan): just the remaining credits —
           no progress bars, no reset times. -->
      {#if creditsWin}
        <div class="stat-row">
          <span class="stat-label">剩余 Credits</span>
          <span class="stat-amount">{fmtCredits(creditsWin.total_value! - creditsWin.used_value!)}</span>
        </div>
      {/if}
    {/if}

    <!-- Balance + consumption rows -->
    {#if quota.balance}
      {@const { unit, value } = splitBalance(
        quota.balance.currency,
        quota.balance.amount
      )}
      <div class="stat-row">
        <span class="stat-label">账户余额</span>
        <span class="stat-amount">{#if unit}<span class="stat-unit">{unit}</span>{/if}{value}</span>
      </div>
      {#if quota.balance.today_consumption != null}
        <div class="stat-row">
          <span class="stat-label">今日消费</span>
          <span class="stat-amount">{#if unit}<span class="stat-unit">{unit}</span>{/if}{quota.balance.today_consumption.toFixed(2)}</span>
        </div>
      {/if}
      {#if quota.balance.month_consumption != null}
        <div class="stat-row">
          <span class="stat-label">月度消费</span>
          <span class="stat-amount">{#if unit}<span class="stat-unit">{unit}</span>{/if}{quota.balance.month_consumption.toFixed(2)}</span>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  /* Solid tooltip card — no glass/backdrop-filter, no shadows (a dark
     shadow reads as a translucent black background). Theme-aware via CSS
     vars; all text uniform size per user preference. The bg/border/tail
     live on .card-surface (one compositing layer) so the tail fuses
     seamlessly with the body at every opacity. */
  .detail-card {
    --card-solid: #f7f5f1;
    --card-text: #1a1610;
    --card-border: rgba(0, 0, 0, 0.12);
    --card-track: rgba(0, 0, 0, 0.08);
    --card-badge: rgba(0, 0, 0, 0.07);
    position: relative;
    min-width: 242px;
    max-width: 297px;
    padding: 14px 14px;
    font-size: 9px;
    line-height: 1.45;
    color: var(--card-text);
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  .detail-card.dark {
    --card-solid: #16140f;
    --card-text: #f2ede3;
    --card-border: rgba(255, 255, 255, 0.1);
    --card-track: rgba(255, 255, 255, 0.1);
    --card-badge: rgba(255, 255, 255, 0.1);
  }

  /* ── Surface layer: bg + border + tail, one compositing layer ──────
     Solid fills inside; the panel-matching alpha is applied once via
     `opacity` — overlapping regions can't double-stack translucency. */
  .card-surface {
    position: absolute;
    inset: 0;
    background: var(--card-solid);
    border: 1px solid var(--card-border);
    border-radius: 20px;
    opacity: var(--card-alpha, 1);
  }

  /* ── Arrow: large triangle on the card edge, pointing at the ring ──
     14px wide with a 2px overlap INTO the card — opaque-over-opaque
     covers the border at the junction so the tail reads as part of the
     body (no dividing line), at every opacity. */
  .card-arrow {
    position: absolute;
    top: var(--arrow-y, 50%);
    left: -12px;
    transform: translateY(-50%);
    width: 0;
    height: 0;
    border-top: 12px solid transparent;
    border-bottom: 12px solid transparent;
    border-right: 14px solid var(--card-solid);
  }
  .detail-card.card-right .card-arrow {
    left: auto;
    right: -12px;
    border-right: none;
    border-left: 14px solid var(--card-solid);
  }

  .card-body {
    position: relative; /* paints above the positioned surface layer */
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  /* ── Header ──────────────────────────────────────────────────── */
  .card-header {
    display: flex;
    flex-direction: column;
    gap: 4px; /* plan name ↔ expiry spacing (user-specified) */
  }

  .header-line {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .vendor-icon {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--card-text); /* follows the theme: light icon on dark card */
  }
  .vendor-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .vendor-name {
    font-size: 10px;
    font-weight: 600;
    flex: 1;
  }

  .plan-badge {
    font-size: 10px;
    padding: 0 4px;
    border-radius: 3px;
    background: var(--card-badge);
    font-weight: 500;
  }

  .expiry-line {
    display: flex;
    justify-content: flex-end;
    font-size: 10px;
    margin-top: 0;
    opacity: 0.75;
  }

  /* ── Window section: 三行左对齐 ─────────────────────────────── */
  .window-bars {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .window-section {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .ws-title {
    font-size: 9px;
    font-weight: 500;
  }

  .ws-bar-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .ws-track {
    flex: 1;
    height: 5px;
    border-radius: 3px;
    background: var(--card-track);
    overflow: hidden;
  }

  .ws-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.35s ease;
  }

  .ws-pct {
    font-size: 9px;
    font-weight: 600;
    width: 44px;
    text-align: right;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", monospace;
    flex-shrink: 0;
  }

  /* Reset/remaining line — centered. */
  .ws-value {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 9px;
    font-weight: 500;
  }

  /* ── Balance + consumption rows (uniform) ───────────────────── */
  .stat-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    font-size: 9px;
  }

  .stat-label {
    font-weight: 500;
  }

  .stat-amount {
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", monospace;
  }

  /* Currency unit (¥/$) renders smaller than the digits. */
  .stat-amount .stat-unit {
    font-size: 8px;
    margin-right: 1px;
  }

  .ws-pct .pct-sign {
    margin-left: 1px;
  }
</style>