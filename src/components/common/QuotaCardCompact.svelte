<!--! Compact one-line quota row for the Overview module — icon + vendor +
   plan tag + a single worst-window progress bar + percentage. The full card
   (windows, sub-items, cookie editor, balance detail) lives on the Limits
   segment; clicking this row navigates there. Cookie errors stay visible
   (they're actionable) as a coral chip in place of the bar. -->
<script lang="ts">
  import { VENDOR_LABELS } from "../../lib/meta/vendors";
  import { translateCookieError, formatBalance } from "../../lib/quota-format";
  import { getLang } from "../../lib/i18n.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Quota } from "../../lib/api";
  import ToolIcon from "../ui/ToolIcon.svelte";

  let {
    quota,
    onOpen,
  }: {
    quota: Quota;
    /** Navigate to the Limits segment (full cards). */
    onOpen: () => void;
  } = $props();

  let _lang = $derived(getLang());

  /** Worst window used% — the number worth glancing at. */
  const worstPct = $derived(
    quota.windows.length > 0
      ? Math.round(Math.max(...quota.windows.map((w) => w.used_pct)))
      : null,
  );
  const statusClass = $derived(
    quota.status === "danger" ? "f-danger" : quota.status === "low" ? "f-low" : "f-ok",
  );
</script>

<button
  type="button"
  class="qcc"
  onclick={onOpen}
  title={t("overview.quotaDetail")}
  aria-label="{VENDOR_LABELS[quota.vendor] ?? quota.vendor} — {t('overview.quotaDetail')}"
>
  <span class="qcc-vendor">
    <ToolIcon vendor={quota.vendor} badge={false} size={12} />
    {VENDOR_LABELS[quota.vendor] ?? quota.vendor}
  </span>
  {#if quota.plan_label}
    <span class="qcc-plan">{quota.plan_label}</span>
  {/if}

  <span class="qcc-mid">
    {#if quota.cookie_error}
      <span class="qcc-err">⚠ {translateCookieError(quota.cookie_error, _lang)}</span>
    {:else if worstPct !== null}
      <span class="qcc-bar">
        <span class="qcc-fill {statusClass}" style="width:{Math.min(100, Math.max(2, worstPct))}%"></span>
      </span>
    {:else if quota.balance}
      <span class="qcc-balance">{formatBalance(quota.balance.currency ?? "USD", quota.balance.amount)}</span>
    {/if}
  </span>

  {#if worstPct !== null && !quota.cookie_error}
    <span class="qcc-pct {statusClass}">{worstPct}%</span>
  {/if}
  <span class="qcc-arr" aria-hidden="true">→</span>
</button>

<style>
  .qcc {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 10px;
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    width: 100%;
    transition: border-color 0.15s;
  }
  .qcc:hover {
    border-color: var(--amber-soft);
  }
  .qcc-vendor {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text);
    font-weight: 500;
    flex-shrink: 0;
    max-width: 120px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qcc-plan {
    font-size: 10px;
    font-weight: 500;
    color: var(--violet);
    background: rgba(182, 155, 224, 0.12);
    padding: 1px 7px;
    border-radius: 5px;
    white-space: nowrap;
    flex-shrink: 0;
    line-height: 1.7;
  }
  .qcc-mid {
    flex: 1;
    min-width: 60px;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    overflow: hidden;
  }
  .qcc-bar {
    width: 100%;
    max-width: 130px;
    height: 5px;
    background: var(--bar-track);
    border-radius: 3px;
    overflow: hidden;
  }
  .qcc-fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    transition: width 0.3s;
  }
  .qcc-fill.f-ok { background: var(--lime); }
  .qcc-fill.f-low { background: var(--amber); }
  .qcc-fill.f-danger { background: var(--coral); }
  .qcc-err {
    font-size: 10px;
    color: var(--coral);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qcc-balance {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-dim);
    white-space: nowrap;
  }
  .qcc-pct {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    flex-shrink: 0;
    width: 34px;
    text-align: right;
  }
  .qcc-pct.f-ok { color: var(--lime); }
  .qcc-pct.f-low { color: var(--amber); }
  .qcc-pct.f-danger { color: var(--coral); }
  .qcc-arr {
    color: var(--text-faint);
    font-size: 13px;
    flex-shrink: 0;
    transition: color 0.15s;
  }
  .qcc:hover .qcc-arr { color: var(--amber); }
</style>
