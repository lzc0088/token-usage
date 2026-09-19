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
</script>

<div class="detail-card" class:card-right={cardSide === "right"}>
  <!-- Simple arrow pointer: left-pointing triangle, centered, 2px gap -->
  <div class="card-pointer"></div>

  <div class="card-body">
    <!-- Header: icon + vendor + plan badge …… 到期时间 (right) -->
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
          <span class="expiry-label">到期</span>
          <span class="expiry-time">{formatExpiryTime(quota.expires_at)}</span>
        </div>
      {/if}
    </div>

    <!-- Window rows -->
    {#if quota.windows.length > 0}
      <div class="window-bars">
        {#each quota.windows as win}
          <div class="window-row">
            <!-- Row 1: name · bar · remaining · pct (single line) -->
            <div class="row-main">
              <span class="row-title">{windowLabel(win.label)}</span>
              <div class="row-track">
                <div
                  class="row-fill"
                  style="width:{Math.min(100, win.used_pct)}%"
                ></div>
              </div>
              {#if win.total_value != null && win.used_value != null}
                <span class="row-remain">{fmtCredits(win.total_value - win.used_value)}</span>
              {/if}
              <span class="row-pct">{win.used_pct.toFixed(2)}%</span>
            </div>
            <!-- Row 2: reset time, centered vertically with the bar above -->
            {#if win.resets_at}
              <div class="row-reset">{formatReset(win.resets_at, nowMs)}</div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Balance -->
    {#if quota.balance}
      {@const { unit, value } = splitBalance(
        quota.balance.currency,
        quota.balance.amount
      )}
      <div class="balance-row">
        <span class="balance-label">余额</span>
        <span class="balance-amount">
          <span class="balance-unit">{unit}</span>
          <span class="balance-value">{value}</span>
        </span>
      </div>

      {#if quota.balance.today_consumption != null}
        <div class="cons-row">
          <span class="cons-label">今日消费</span>
          <span class="cons-amount">
            <span class="cons-unit">{unit}</span>
            <span class="cons-value">{quota.balance.today_consumption.toFixed(2)}</span>
          </span>
        </div>
      {/if}
      {#if quota.balance.month_consumption != null}
        <div class="cons-row">
          <span class="cons-label">月度消费</span>
          <span class="cons-amount">
            <span class="cons-unit">{unit}</span>
            <span class="cons-value">{quota.balance.month_consumption.toFixed(2)}</span>
          </span>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .detail-card {
    position: relative;
    min-width: 220px;
    max-width: 260px;
    background: var(--pulse-card-bg, #0c0c0c);
    border-radius: 10px;
    padding: 10px 12px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.5);
    font-size: 10px;
    line-height: 1.45;
    color: var(--pulse-text);
  }

  /* ── Simple triangle arrow pointer ──────────────────────────── */
  .card-pointer {
    position: absolute;
    top: 50%;
    left: -6px;
    transform: translateY(-50%);
    width: 0;
    height: 0;
    border-top: 6px solid transparent;
    border-bottom: 6px solid transparent;
    border-right: 6px solid var(--pulse-card-bg, #0c0c0c);
  }
  .detail-card.card-right .card-pointer {
    left: auto;
    right: -6px;
    border-right: none;
    border-left: 6px solid var(--pulse-card-bg, #0c0c0c);
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
    gap: 2px;
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
  }
  .vendor-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .vendor-name {
    font-size: 11px;
    font-weight: 600;
    flex: 1;
  }

  .plan-badge {
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--pulse-text-dim, #8a857b);
    font-weight: 500;
  }

  .expiry-line {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: 4px;
    margin-top: 1px;
  }

  .expiry-label {
    font-size: 9px;
    color: var(--pulse-text-dim, #8a857b);
    flex-shrink: 0;
  }

  .expiry-time {
    font-size: 9px;
    font-weight: 600;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
    color: var(--pulse-text);
    opacity: 0.8;
  }

  /* ── Window rows ───────────────────────────────────────────── */
  .window-bars {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .window-row {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* Row 1: name · bar · remain · pct — all on one line */
  .row-main {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 5px;
  }

  .row-title {
    font-size: 9px;
    color: var(--pulse-text-dim, #8a857b);
    white-space: nowrap;
  }

  .row-track {
    width: 52px;
    height: 3px;
    border-radius: 2px;
    background: var(--pulse-bar-track, rgba(255, 255, 255, 0.1));
    overflow: hidden;
  }

  .row-fill {
    height: 100%;
    border-radius: 2px;
    background: var(--pulse-good, #00e68a);
    transition: width 0.35s ease;
  }

  .row-remain {
    font-size: 9px;
    font-weight: 600;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
    white-space: nowrap;
  }

  .row-pct {
    font-size: 9px;
    font-weight: 600;
    width: 40px;
    text-align: right;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
  }

  /* Row 2: reset time, vertically centered with the bar above */
  .row-reset {
    font-size: 8px;
    color: var(--pulse-text-dim, #8a857b);
    opacity: 0.75;
    padding-left: 1px;
  }

  /* ── Balance + consumption ──────────────────────────────────── */
  .balance-row,
  .cons-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .balance-label,
  .cons-label {
    font-size: 9px;
    color: var(--pulse-text-dim, #8a857b);
    flex-shrink: 0;
  }

  .balance-amount,
  .cons-amount {
    font-size: 9px;
    font-weight: 600;
    display: flex;
    align-items: baseline;
    gap: 1px;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
  }

  .balance-unit,
  .cons-unit {
    font-size: 8px;
    opacity: 0.7;
  }
</style>