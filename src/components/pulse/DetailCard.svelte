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
    /** Panel opacity (0.2–1.0), applied to card background. */
    alpha?: number;
    dark?: boolean;
  }

  const { quota, cardSide, alpha = 1, dark = false }: Props = $props();
  const nowMs = Date.now();

  // Solid card background — no glass/backdrop-filter, just opacity.
  let cardBg = $derived(
    dark
      ? `rgba(12,12,12,${alpha.toFixed(2)})`
      : `rgba(250,248,244,${alpha.toFixed(2)})`
  );
</script>

<div
  class="detail-card"
  class:card-right={cardSide === "right"}
  style="background:{cardBg}"
>
  <!-- Circle-arrow pointer: small circle with chevron, 2px gap from panel -->
  <div class="card-pointer">
    <svg
      viewBox="0 0 14 14"
      width="14"
      height="14"
      fill={cardBg}
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
    <!-- Header: icon + vendor + plan badge -->
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

    <!-- Window sections — 三行左对齐: 标题 · 进度条+百分比 · 数量 -->
    {#if quota.windows.length > 0}
      <div class="window-bars">
        {#each quota.windows as win}
          <div class="window-section">
            <div class="ws-title">{windowLabel(win.label)}</div>
            <div class="ws-bar-row">
              <div class="ws-track">
                <div
                  class="ws-fill"
                  style="width:{Math.min(100, win.used_pct)}%"
                ></div>
              </div>
              <span class="ws-pct">{win.used_pct.toFixed(2)}%</span>
            </div>
            <div class="ws-value">
              {#if win.total_value != null && win.used_value != null}
                <span class="ws-remain">
                  剩余 {fmtCredits(win.total_value - win.used_value)}
                </span>
              {/if}
              {#if win.resets_at}
                <span class="ws-reset">{formatReset(win.resets_at, nowMs)}</span>
              {/if}
            </div>
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
    max-width: 270px;
    /* background set inline via JS — solid, no glass/backdrop-filter */
    border-radius: 10px;
    padding: 10px 12px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.5);
    font-size: 10px;
    line-height: 1.45;
    color: var(--pulse-text);
  }

  /* ── Circle-arrow pointer ───────────────────────────────────── */
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
    gap: 8px;
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

  /* ── Window section: 三行左对齐 ─────────────────────────────── */
  .window-bars {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .window-section {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .ws-title {
    font-size: 9px;
    font-weight: 500;
    color: var(--pulse-text-dim, #8a857b);
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
    background: var(--pulse-bar-track, rgba(255, 255, 255, 0.1));
    overflow: hidden;
  }

  .ws-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--pulse-good, #00e68a);
    transition: width 0.35s ease;
  }

  .ws-pct {
    font-size: 9px;
    font-weight: 600;
    width: 44px;
    text-align: right;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
    flex-shrink: 0;
  }

  .ws-value {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ws-remain {
    font-size: 9px;
    font-weight: 600;
    font-family: "SF Mono", "Fira Code", "Cascadia Code", monospace;
  }

  .ws-reset {
    font-size: 8px;
    color: var(--pulse-text-dim, #8a857b);
    opacity: 0.75;
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