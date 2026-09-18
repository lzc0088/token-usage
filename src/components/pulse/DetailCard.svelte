<script lang="ts">
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
    position: string;
  }

  let { quota, position }: Props = $props();

  // Vendor display names.
  const vendorNames: Record<string, string> = {
    glm: "GLM",
    kimi: "Kimi",
    minimax: "Minimax",
    volcengine: "火山引擎",
    bailian: "百炼",
    stepfun: "阶跃",
    openrouter: "OpenRouter",
    ollama: "Ollama",
    deepseek: "DeepSeek",
    workbuddy: "WorkBuddy",
    qoder: "Qoder",
  };

  let displayName = $derived(vendorNames[quota.vendor] ?? quota.vendor);

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
      if (diff <= 0) return "已重置";
      const hours = Math.floor(diff / 3_600_000);
      const mins = Math.floor((diff % 3_600_000) / 60_000);
      if (hours >= 24) return `${Math.floor(hours / 24)}天后`;
      if (hours > 0) return `${hours}h${mins > 0 ? mins + "m" : ""}后`;
      return `${mins}m后`;
    } catch {
      return null;
    }
  }

  </script>

<div class="detail-card" class:left={position === "left"}>
  <!-- Pointer shape (curved tail pointing toward the ring) -->
  <svg
    class="card-pointer"
    viewBox="0 0 20 40"
    preserveAspectRatio="none"
    aria-hidden="true"
  >
    {#if position === "left"}
      <!-- Pointer on right side, pointing right -->
      <path
        d="M 0,0 C 6,10 14,15 20,20 C 14,25 6,30 0,40 Z"
        fill="var(--pulse-card-bg)"
      />
    {:else}
      <!-- Pointer on left side, pointing left -->
      <path
        d="M 20,0 C 14,10 6,15 0,20 C 6,25 14,30 20,40 Z"
        fill="var(--pulse-card-bg)"
      />
    {/if}
  </svg>

  <!-- Card body -->
  <div class="card-content">
    <div class="card-header">
      <span class="vendor-name">{displayName}</span>
      {#if quota.plan}
        <span class="plan-badge">{quota.plan}</span>
      {/if}
    </div>

    <div class="window-list">
      {#each quota.windows as win (win.label)}
        <div class="window-row">
          <div class="window-top">
            <span class="window-label">{win.label}</span>
            <span class="window-pct" style="color:{barColor(win.used_pct)}">
              {Math.round(win.used_pct)}%
            </span>
          </div>
          <div class="bar-track">
            <div
              class="bar-fill"
              style="width:{Math.min(win.used_pct, 100)}%;background:{barColor(win.used_pct)}"
            ></div>
          </div>
          {#if relativeReset(win.resets_at)}
            <span class="reset-time">⏱ {relativeReset(win.resets_at)}</span>
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
    border-radius: 16px;
    min-width: 210px;
    max-width: 250px;
    animation: cardIn 0.25s cubic-bezier(0.34, 1.56, 0.64, 1);
    color: var(--pulse-text);
    backdrop-filter: blur(16px) saturate(180%);
    -webkit-backdrop-filter: blur(16px) saturate(180%);
    border: 1px solid rgba(255, 255, 255, 0.06);
    /* Prevent the card from overflowing the panel */
    flex-shrink: 0;
  }

  .detail-card.left {
    flex-direction: row-reverse;
  }

  @keyframes cardIn {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.94);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* ── Pointer ──────────────────────────────────────────────────────── */

  .card-pointer {
    position: absolute;
    top: 50%;
    width: 20px;
    height: 40px;
    transform: translateY(-50%);
    flex-shrink: 0;
    pointer-events: none;
  }

  .detail-card:not(.left) .card-pointer {
    left: -19px;
  }

  .detail-card.left .card-pointer {
    right: -19px;
  }

  /* ── Card content ──────────────────────────────────────────────────── */

  .card-content {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 210px;
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 2px;
  }

  .vendor-name {
    font-size: 14px;
    font-weight: 600;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
    letter-spacing: -0.01em;
  }

  .plan-badge {
    font-size: 10px;
    padding: 1px 7px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--pulse-text-dim);
    font-weight: 500;
    letter-spacing: 0.01em;
  }

  .window-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .window-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .window-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .window-label {
    font-size: 11.5px;
    color: var(--pulse-text-dim);
    font-weight: 400;
    font-family: "SF Pro Rounded", "SF Rounded", "Helvetica Neue Rounded",
      -apple-system, sans-serif;
  }

  .window-pct {
    font-size: 12px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    letter-spacing: -0.02em;
  }

  .bar-track {
    height: 5px;
    background: var(--pulse-bar-track);
    border-radius: 3px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  .reset-time {
    font-size: 10px;
    color: var(--pulse-text-dim);
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    opacity: 0.7;
  }

  .balance-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-top: 4px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .balance-label {
    font-size: 11px;
    color: var(--pulse-text-dim);
    font-weight: 400;
  }

  .balance-amount {
    font-size: 13px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", "Menlo", "Consolas", monospace;
    letter-spacing: -0.01em;
  }
</style>
