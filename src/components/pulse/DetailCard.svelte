<script lang="ts">
  import {
    windowLabel,
    fmtCredits,
    formatExpiryTime,
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
</script>

<div class="detail-card" class:card-right={cardSide === "right"}>
  <!-- Pointer beak: circle-arrow, 2px gap from panel edge (16px + 2px) -->
  <div class="card-pointer">
    <svg
      viewBox="0 0 14 14"
      width="14"
      height="14"
      fill="var(--pulse-card-bg, #0c0c0c)"
      stroke="none"
    >
      <circle cx="7" cy="7" r="7" />
      <path
        d="M6 4.5l3 2.5-3 2.5"
        fill="none"
        stroke="var(--pulse-text-dim, #8a857b)"
        stroke-width="1.2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </div>

  <div class="card-body">
    <!-- Header: icon + vendor + plan badge  ……  到期时间 (right-aligned) -->
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

    <!-- Window bars -->
    {#if quota.windows.length > 0}
      <div class="window-bars">
        {#each quota.windows as win}
          <div class="window-row">
            <div class="row-label">
              <span class="row-title">{windowLabel(win.label)}</span>
              <span class="row-meta">
                {#if win.resets_at}
                  <span class="reset-hint">{win.resets_at}</span>
                {/if}
                {#if win.total_value != null && win.used_value != null}
                  <span class="remaining">
                    剩余 {fmtCredits(win.total_value - win.used_value)}
                  </span>
                {/if}
              </span>
            </div>
            <div class="row-track">
              <div
                class="row-fill"
                style="width:{Math.min(100, win.used_pct)}%"
              ></div>
            </div>
            <span class="row-pct">{win.used_pct}%</span>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Balance display -->
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

      <!-- Today / month consumption (same style as balance) -->
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

  /* ── Pointer beak ──────────────────────────────────────────── */
  .card-pointer {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    left: -6px;
    opacity: 0.96;
  }
  .detail-card.card-right .card-pointer {
    left: auto;
    right: -6px;
    transform: translateY(-50%) scaleX(-1);
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  /* ── Header ────────────────────────────────────────────────── */
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

  /* ── Window bars ───────────────────────────────────────────── */
  .window-bars {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .window-row {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 5px;
  }

  .row-label {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .row-title {
    font-size: 9px;
    color: var(--pulse-text-dim, #8a857b);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-meta {
    display: flex;
    gap: 5px;
    align-items: center;
  }

  .reset-hint {
    font-size: 8px;
    color: var(--pulse-text-dim, #8a857b);
    opacity: 0.7;
  }

  .remaining {
    font-size: 9px;
    font-weight: 600;
    color: var(--pulse-text);
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

  .row-pct {
    font-size: 9px;
    font-weight: 600;
    width: 30px;
    text-align: right;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
  }

  /* ── Balance + consumption rows ────────────────────────────── */
  /*  All three (余额 / 今日消费 / 月度消费) share the same layout:
      left label  ……  right mono number — no duplicate selectors. */
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
    font-size: 11px;
    font-weight: 600;
    display: flex;
    align-items: baseline;
    gap: 1px;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
  }

  .balance-unit,
  .cons-unit {
    font-size: 9px;
    opacity: 0.7;
  }
</style>
