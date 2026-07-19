<script lang="ts">
  // 额度 segment (T4.5). Loads get_quotas() — only vendors with a stored
  // credential (keyring) appear. Empty state guides to 设置→账号.
  import { api, type Quota, type QuotaStatus } from "../../lib/api";

  let quotas = $state<Quota[] | null>(null);

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const q = await api.getQuotas();
        if (!cancelled) quotas = q;
      } catch (e) {
        console.error("quotas failed", e);
        if (!cancelled) quotas = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  function statusColor(s: QuotaStatus): string {
    if (s === "danger") return "var(--coral)";
    if (s === "low") return "var(--amber)";
    return "var(--lime)";
  }
  function resetLabel(secs: number | null): string {
    if (secs === null || secs <= 0) return "";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (h > 0) return `${h}h${m}m 后重置`;
    return `${m}m 后重置`;
  }
  const VENDOR_LABELS: Record<string, string> = {
    deepseek: "DeepSeek",
    claude: "Claude",
    codex: "Codex",
    glm: "GLM / Z.ai",
    minimax: "MiniMax",
    kimi: "Kimi",
    volcengine: "火山引擎",
  };
</script>

<div class="seg-body">
  {#if quotas === null}
    <p class="loading">加载中…</p>
  {:else if quotas.length === 0}
    <div class="empty">
      <p>未绑定厂商账号</p>
      <p class="hint">在「设置 → 账号」绑定 API Key / OAuth 后，额度将显示在此</p>
    </div>
  {:else}
    {#each quotas as q (q.vendor)}
      <div class="qcard" data-status={q.status}>
        <div class="q-head">
          <span class="dot" style="background:{statusColor(q.status)}"></span>
          <span class="q-vendor">{VENDOR_LABELS[q.vendor] ?? q.vendor}</span>
          <span class="q-kind">{q.kind === "balance" ? "余额" : "套餐"}</span>
        </div>
        <div class="q-value" style="color:{statusColor(q.status)}">{q.display}</div>
        {#if q.kind === "plan" && q.used_pct !== null}
          <div class="q-bar"><i style="width:{q.used_pct.toFixed(0)}%;background:{statusColor(q.status)}"></i></div>
          <div class="q-meta">已用 {q.used_pct.toFixed(0)}%</div>
        {/if}
        {#if resetLabel(q.reset_in_secs)}
          <div class="q-meta dim">{resetLabel(q.reset_in_secs)}</div>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .seg-body {
    padding: 14px 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .loading {
    color: var(--text-faint);
    font-size: 12px;
    padding: 24px 0;
    text-align: center;
  }
  .empty {
    text-align: center;
    padding: 32px 16px;
  }
  .empty p {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
  }
  .empty .hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-faint);
  }
  .qcard {
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 9px;
    padding: 11px 13px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .q-head {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .q-vendor {
    font-size: 12.5px;
    color: var(--text);
    flex: 1;
  }
  .q-kind {
    font-size: 9.5px;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .q-value {
    font-size: 20px;
    font-weight: 500;
    font-family: var(--font-mono);
  }
  .q-bar {
    height: 5px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 3px;
    overflow: hidden;
  }
  .q-bar i {
    display: block;
    height: 100%;
    border-radius: 3px;
  }
  .q-meta {
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .q-meta.dim {
    color: var(--text-faint);
  }
</style>
