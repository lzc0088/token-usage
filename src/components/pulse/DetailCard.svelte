<script lang="ts">
  import {
    windowLabel,
    fmtCredits,
    formatShortExpiry,
    formatReset,
    splitBalance,
  } from "../../lib/quota-format";
  import { vendorIconMarkup, vendorDisplayName } from "../../lib/vendorIcons";
  import { vendorColor } from "../../lib/pulse-colors";
  import { TAIL_W, TAIL_H, TAIL_PATH, TAIL_OUTLINE } from "../../lib/pulse-shapes";

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
    refreshed_at?: string;
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
    /** Dock-order index — same per-vendor hue as the panel ring. */
    colorIndex?: number;
  }

  const {
    quota,
    cardSide,
    dark = false,
    pulseAlpha = 1,
    arrowY,
    colorIndex = 0,
  }: Props = $props();
  const nowMs = Date.now();

  // Bar color = the vendor's ring hue (theme-aware lightness) — the card
  // is that ring's expansion, so they share one color identity.
  let barColor = $derived(vendorColor(colorIndex, dark));

  // First window reporting absolute credits (plan-less vendors).
  let creditsWin = $derived(
    quota.windows.find(
      (w) => w.total_value != null && w.used_value != null
    ) ?? null
  );

  // "N分钟前刷新" — shown below the vendor name when we have a timestamp.
  function refreshLabel(ts?: string): string {
    if (!ts) return "";
    const ms = Date.now() - new Date(ts).getTime();
    const mins = Math.floor(ms / 60_000);
    if (mins < 1) return "刚刚刷新";
    if (mins < 60) return `${mins}分钟前刷新`;
    const hrs = Math.floor(mins / 60);
    if (hrs < 24) return `${hrs}小时前刷新`;
    const days = Math.floor(hrs / 24);
    return `${days}天前刷新`;
  }

  // Fixed column widths (px) — every row's title/bar/pct boxes start at
  // the same x, so multi-window vendors (e.g. GLM 5h/周/月/MCP) read as
  // aligned columns. 54px fits the longest zh label ("MCP 每月") and
  // "5h · sonnet". Inline (not class CSS) — it is a layout contract.
  const TITLE_W = 54;
  const PCT_W = 44;

  // Remaining pct for a window (clamped) — the card mirrors the ring's
  // 剩余量 mode: the bar fill AND the number both show what is LEFT.
  function remainPct(used: number): number {
    return Math.min(100, Math.max(0, 100 - used));
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
    <!-- Tail: token-monitor bubbleCommands arrowhead protruding from the
         card's panel-facing edge, centered on --arrow-y (the hovered ring).
         ONE silhouette with the card: the fill overlaps 2px INTO the card,
         covering the border line across the neck; the open outline path
         strokes the tail's curves in the SAME border color, so the border
         visually flows around the tail instead of cutting through it. -->
    <svg
      class="card-tail"
      class:card-tail-right={cardSide === "right"}
      style:top="var(--arrow-y, 50%)"
      width={TAIL_W}
      height={TAIL_H}
      viewBox="0 0 {TAIL_W} {TAIL_H}"
      aria-hidden="true"
    >
      <path d={TAIL_PATH} fill="var(--card-solid)" pointer-events="none" />
      <path
        d={TAIL_OUTLINE}
        fill="none"
        stroke="var(--card-border)"
        stroke-width="1"
        stroke-linecap="round"
        pointer-events="none"
      />
    </svg>
  </div>

  <div class="card-body">
    <!-- Header: LEFT region (logo + vendor name) · RIGHT region (plan
         badge + expiry, stacked) — both centered on the same horizontal
         midline — closed by a dashed divider with even spacing above
         (7px) and below (card-body gap 7px). -->
    <div class="card-header">
      <div class="header-row">
        <div class="header-left">
          <span class="vendor-icon">{@html vendorIconMarkup(quota.vendor)}</span>
          <div class="header-left-text">
            <span class="vendor-name">{vendorDisplayName(quota.vendor)}</span>
            {#if quota.refreshed_at}
              <span class="refresh-label">{refreshLabel(quota.refreshed_at)}</span>
            {/if}
          </div>
        </div>
        {#if quota.plan || quota.expires_at}
          <div class="header-right">
            {#if quota.plan}
              <span class="plan-badge">{quota.plan}</span>
            {/if}
            {#if quota.expires_at}
              <span class="expiry-text">到期 {formatShortExpiry(quota.expires_at)}</span>
            {/if}
          </div>
        {/if}
      </div>
      <div class="header-divider" aria-hidden="true"></div>
    </div>

    {#snippet balanceRows()}
      <!-- Balance + consumption rows. Rendered ABOVE the plan section when
           the vendor has both (user-specified ordering); for planless
           vendors they follow the credits row. -->
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
    {/snippet}

    {#if !quota.planless && quota.windows.length > 0}
      <!-- Plan vendors: balance/consumption rows first, then the plan
           sections — 三行左对齐: 标题 · 进度条+百分比 · 剩余(居中) -->
      {@render balanceRows()}
      <div class="window-bars">
        {#each quota.windows as win}
          <div class="window-section">
            <!-- One line: 套餐名称 · 进度条 · 百分比 (fixed column widths);
                 fill + number show the REMAINING share, mirroring the ring. -->
            <div class="ws-bar-row">
              <div class="ws-title" style="width:{TITLE_W}px">{windowLabel(win.label)}</div>
              <div class="ws-track">
                <div
                  class="ws-fill"
                  style="width:{remainPct(win.used_pct)}%;background:{barColor}"
                ></div>
              </div>
              <span class="ws-pct" style="width:{PCT_W}px">{remainPct(win.used_pct).toFixed(2)}<span class="pct-sign">%</span></span>
            </div>
            <!-- Reset countdown below the bar, centered. -->
            {#if win.resets_at}
              <div class="ws-reset">{formatReset(win.resets_at, nowMs)}</div>
            {/if}
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
      {@render balanceRows()}
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
    /* top right bottom left — generous breath, matches token-monitor card density. */
    padding: 14px 16px 12px 14px;
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

  /* ── Tail: token-monitor bubbleCommands arrowhead ────────────────
     The SVG is 12×36px (neck 18, tail 12). It sits 2px INTO the card
     (local x=12 lands at card x=+2), so the opaque fill covers the card's
     1px border across the neck — the border line never crosses the tail;
     the stroked outline path continues the border around the curves. The
     tip at x=1 reaches 9px outside the card toward the panel.
     "card-tail-right" flips horizontally for the other side. */
  .card-tail {
    position: absolute;
    left: -10px;
    transform: translateY(-50%);
    overflow: visible;
  }
  .card-tail.card-tail-right {
    left: auto;
    right: -10px;
    transform: translateY(-50%) scaleX(-1);
  }

  .card-body {
    position: relative; /* paints above the positioned surface layer */
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── Header ──────────────────────────────────────────────────── */
  .card-header {
    display: flex;
    flex-direction: column;
  }

  .header-row {
    display: flex;
    align-items: center; /* left/right regions share one horizontal midline */
    justify-content: space-between;
    gap: 10px;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .header-left-text {
    display: flex;
    flex-direction: column;
    line-height: 1.3;
    min-width: 0;
  }

  .header-right {
    display: flex;
    flex-direction: column; /* plan badge over expiry, two lines */
    align-items: flex-end;
    gap: 2px;
  }

  .vendor-icon {
    width: 13px;
    height: 13px;
    flex-shrink: 0;
    color: var(--card-text); /* follows the theme: light icon on dark card */
  }
  .vendor-icon :global(svg) {
    width: 100%;
    height: 100%;
  }

  .vendor-name {
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .refresh-label {
    font-size: 8px;
    font-weight: 400;
    opacity: 0.55;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .plan-badge {
    font-size: 10px;
    padding: 0 4px;
    border-radius: 3px;
    background: var(--card-badge);
    font-weight: 500;
  }

  .expiry-text {
    font-size: 10px;
    opacity: 0.75;
    white-space: nowrap;
  }

  /* Dashed divider closing the header — margin-top matches the card-body
     gap (10px) for even spacing above and below. */
  .header-divider {
    margin-top: 10px;
    border-top: 1px dashed var(--card-border);
  }

  /* ── Window section: 三行左对齐 ─────────────────────────────── */
  .window-bars {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .window-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* Title column — fixed width comes inline (TITLE_W) as a layout
     contract; the class handles typography + overflow. */
  .ws-title {
    font-size: 9px;
    font-weight: 500;
    flex-shrink: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ws-bar-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .ws-track {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--card-track);
    overflow: hidden;
  }

  .ws-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 420ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .ws-pct {
    font-size: 9px;
    font-weight: 600;
    text-align: right;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", monospace;
    flex-shrink: 0;
  }

  /* Reset countdown — below the bar row, centered under it. */
  .ws-reset {
    text-align: center;
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

  /* Currency unit (¥/$) smaller than the digits — 2px margin creates the
     visible gap between symbol and number (e.g. ¥ 10.0). */
  .stat-amount .stat-unit {
    font-size: 7px;
    margin-right: 2px;
  }

  /* Percent sign: 2px gap before the number (e.g. 10 %). */
  .ws-pct .pct-sign {
    margin-left: 2px;
  }
</style>