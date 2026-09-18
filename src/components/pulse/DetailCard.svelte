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

<style>
  .detail-card {
    background: var(--pulse-card-bg);
    border-radius: 12px;
    padding: 14px;
    min-width: 200px;
    max-width: 240px;
    margin-top: 8px;
    animation: cardIn 0.2s ease-out;
    color: var(--pulse-text);
  }

  .detail-card.left {
    margin-top: 0;
    margin-left: 8px;
  }

  @keyframes cardIn {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }

  .vendor-name {
    font-size: 13px;
    font-weight: 600;
  }

  .plan-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--pulse-text-dim);
  }

  .window-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .window-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .window-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .window-label {
    font-size: 11px;
    color: var(--pulse-text-dim);
  }

  .window-pct {
    font-size: 12px;
    font-weight: 700;
    font-family: "SF Mono", "JetBrains Mono", monospace;
  }

  .bar-track {
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 2px;
    transition: width 0.4s ease;
  }

  .reset-time {
    font-size: 9px;
    color: var(--pulse-text-dim);
    font-family: "SF Mono", "JetBrains Mono", monospace;
  }

  .balance-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .balance-label {
    font-size: 11px;
    color: var(--pulse-text-dim);
  }

  .balance-amount {
    font-size: 13px;
    font-weight: 600;
    font-family: "SF Mono", "JetBrains Mono", monospace;
  }
</style>
