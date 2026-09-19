<script lang="ts">
  import {
    windowLabel,
    fmtCredits,
    formatExpiryTime,
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
  }

  const { quota, cardSide }: Props = $props();
  const nowMs = Date.now();

  // Bar color thresholds on the light card background.
  function barColor(pct: number): string {
    if (pct >= 80) return "#cc2200";
    if (pct >= 50) return "#cc8800";
    return "#00b36b";
  }
</script>

<div class="detail-card" class:card-right={cardSide === "right"}>
  <!-- Arrow: big triangle protruding from the card's panel-facing edge,
       positioned at --arrow-y (the hovered ring's line). -->
  <div class="card-arrow"></div>

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
          <span>到期 {formatExpiryTime(quota.expires_at)}</span>
        </div>
      {/if}
    </div>

    <!-- Window sections — 三行左对齐: 标题 · 进度条+百分比 · 剩余(居中) -->
    {#if quota.windows.length > 0}
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
              <span class="ws-pct">{win.used_pct.toFixed(2)}%</span>
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
    {/if}

    <!-- Balance + consumption rows -->
    {#if quota.balance}
      {@const { unit, value } = splitBalance(
        quota.balance.currency,
        quota.balance.amount
      )}
      <div class="stat-row">
        <span class="stat-label">账户余额</span>
        <span class="stat-amount">{unit}{value}</span>
      </div>
      {#if quota.balance.today_consumption != null}
        <div class="stat-row">
          <span class="stat-label">今日消费</span>
          <span class="stat-amount">{unit}{quota.balance.today_consumption.toFixed(2)}</span>
        </div>
      {/if}
      {#if quota.balance.month_consumption != null}
        <div class="stat-row">
          <span class="stat-label">月度消费</span>
          <span class="stat-amount">{unit}{quota.balance.month_consumption.toFixed(2)}</span>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  /* Light, solid tooltip card — no glass/backdrop-filter, no theme variance.
     All text near-black (#1a1610) at a uniform 9px per user preference. */
  .detail-card {
    position: relative;
    min-width: 220px;
    max-width: 270px;
    background: #f7f5f1;
    border: 1px solid rgba(0, 0, 0, 0.12);
    border-radius: 10px;
    padding: 10px 12px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.28);
    font-size: 9px;
    line-height: 1.45;
    color: #1a1610;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  /* ── Arrow: big triangle on the card edge, pointing at the ring ── */
  .card-arrow {
    position: absolute;
    top: var(--arrow-y, 50%);
    left: -9px;
    transform: translateY(-50%);
    width: 0;
    height: 0;
    border-top: 9px solid transparent;
    border-bottom: 9px solid transparent;
    border-right: 9px solid #f7f5f1;
    filter: drop-shadow(-1px 0 0 rgba(0, 0, 0, 0.12));
  }
  .detail-card.card-right .card-arrow {
    left: auto;
    right: -9px;
    border-right: none;
    border-left: 9px solid #f7f5f1;
    filter: drop-shadow(1px 0 0 rgba(0, 0, 0, 0.12));
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  /* ── Header ──────────────────────────────────────────────────── */
  .card-header {
    display: flex;
    flex-direction: column;
    gap: 0; /* expiry hugs the plan line — tight per user preference */
  }

  .header-line {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .vendor-icon {
    width: 11px;
    height: 11px;
    flex-shrink: 0;
    color: #1a1610;
  }
  .vendor-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .vendor-name {
    font-size: 9px;
    font-weight: 600;
    flex: 1;
  }

  .plan-badge {
    font-size: 9px;
    padding: 0 4px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.07);
    font-weight: 500;
  }

  .expiry-line {
    font-size: 9px;
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
    background: rgba(0, 0, 0, 0.08);
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
</style>