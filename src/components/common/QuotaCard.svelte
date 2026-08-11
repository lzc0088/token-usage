<!--! Shared quota card — used by Overview and Limits segments.
   Renders a single vendor's quota: header (name + plan + expiry),
   cookie editor (inline when expired), balance, and usage windows
   with expandable sub-items. -->

<script lang="ts">
  import { VENDOR_PANEL, panelHint } from "../../lib/meta/panels";
  import ToolIcon from "../ui/ToolIcon.svelte";
  import { VENDOR_LABELS, VENDORS, fieldsFor, type FieldDef } from "../../lib/meta/vendors";
  import { api, type Currency } from "../../lib/api";
  import Select from "./Select.svelte";
  import { formatRefreshed, formatExpiry, expiryUrgency, translateCookieError, fmtCredits, formatReset, formatShortExpiry, windowLabel, formatBalance, openPanelUrl } from "../../lib/quota-format";
  import { formatCost } from "../../lib/format";
  import { getLang } from "../../lib/i18n.svelte";
  import type { Quota } from "../../lib/api";

  type ProgressMode = "用量" | "剩余";

  let {
    quota,
    progressMode = $bindable("用量"),
    nowMs = 0,
    currency = "cny" as Currency,
    cnyRate = 7.2,
    onQuotaChanged,
  }: {
    quota: Quota;
    progressMode: ProgressMode;
    nowMs: number;
    currency?: Currency;
    cnyRate?: number;
    /** Called after a credential change so the parent can reload fresh quota
     *  data and pass it down as a new `quota` prop. More reliable than relying
     *  on the `quota:updated` Tauri event, which can be missed by a hidden /
     *  backgrounded webview. */
    onQuotaChanged?: () => Promise<void> | void;
  } = $props();

  
  function l(zh: string, en: string): string { return getLang() === "en" ? en : zh; }
  let _lang = $derived(getLang());

  // ── Inline cookie editor (per-card state) ──
  // `null` = not editing, string = vendor being edited (supports per-vendor
  // editing when multiple cards share the same component instance).
  let editingCookie = $state<string | null>(null);
  let cookieDraft = $state("");
  let cookieSaving = $state(false);
  /** Validation/refresh error surfaced to the user (e.g. invalid cookie). */
  let cookieError = $state<string>("");

  // Region/site selector: shown when the vendor credential defines a `select`
  // field other than the cookie (e.g. Volcengine region, Qoder site, GLM region).
  /** Vendor's editable non-cookie select fields (region/site). */
  const regionFields: FieldDef[] = $derived.by(() => {
    const v = VENDORS.find(x => x.id === quota.vendor);
    if (!v) return [];
    return fieldsFor(v).filter(f => f.type === "select");
  });
  /** Current values of those select fields (region/site/projid). */
  let regionValues = $state<Record<string, string>>({});
  /** Snapshot at edit-start so we only persist changed fields. */
  let regionInitial = $state<Record<string, string>>({});

  function startEditCookie(vendor: string): void {
    editingCookie = vendor;
    cookieDraft = "";
    cookieError = "";
    // Pre-fill region fields from the stored credential (non-secret values).
    if (regionFields.length > 0) {
      api.getCredentialFieldValues(vendor)
        .then((vals) => {
          const picked: Record<string, string> = {};
          for (const f of regionFields) {
            picked[f.key] = vals[f.key] ?? f.default ?? f.options?.[0] ?? "";
          }
          regionValues = picked;
          regionInitial = { ...picked };
        })
        .catch(() => {
          const picked: Record<string, string> = {};
          for (const f of regionFields) {
            picked[f.key] = f.default ?? f.options?.[0] ?? "";
          }
          regionValues = picked;
          regionInitial = { ...picked };
        });
    }
  }
  function cancelEditCookie(): void {
    editingCookie = null;
    cookieDraft = "";
    cookieError = "";
  }
  async function saveCookie(): Promise<void> {
    const draft = cookieDraft.trim();
    if (!draft) return;
    cookieSaving = true;
    cookieError = "";
    // Only send region fields that the user actually changed.
    const extra: Record<string, string> = {};
    for (const f of regionFields) {
      const cur = regionValues[f.key] ?? "";
      if (cur !== (regionInitial[f.key] ?? "")) {
        extra[f.key] = cur;
      }
    }
    try {
      await api.updateCookie(quota.vendor, draft, Object.keys(extra).length > 0 ? extra : undefined);
      // refreshQuota re-validates with the new cookie. If it errors, the
      // cookie is invalid (or the region is wrong) — surface the message.
      try {
        await api.refreshQuota(quota.vendor);
      } catch (refreshErr) {
        cookieError = refreshErr instanceof Error ? refreshErr.message : String(refreshErr);
        cookieSaving = false;
        return;
      }
      // Success: ask the parent to reload fresh quota data and pass it down.
      // More reliable than the `quota:updated` Tauri event, which a
      // backgrounded popover webview can miss.
      if (onQuotaChanged) {
        await onQuotaChanged();
      }
      // Parent has now passed a fresh `quota` prop (cookie_error cleared).
      // Close the editor — the cookie section hides naturally.
      editingCookie = null;
      cookieDraft = "";
    } catch (e) {
      cookieError = e instanceof Error ? e.message : String(e);
    } finally {
      cookieSaving = false;
    }
  }

  // ── Window expand/collapse (bubble up) ──
  let expandedWindows = $state<Map<string, boolean>>(new Map());

  function toggleWindow(label: string): void {
    const cur = expandedWindows.get(label) ?? false;
    expandedWindows.set(label, !cur);
    expandedWindows = new Map(expandedWindows);
  }

</script>

<div class="qcard" data-status={quota.status}>
  <div class="qhead">
    <div class="qhead-line">
      <div class="qhead-name-row">
        <span class="q-vendor"><ToolIcon vendor={quota.vendor} badge={false} size={11} />{VENDOR_LABELS[quota.vendor] ?? quota.vendor}</span>
      </div>
      {#if quota.plan_label}
        <span class="qplan-tag">{quota.plan_label}</span>
      {/if}
    </div>
    <div class="qhead-line">
      <span class="qrefreshed">{formatRefreshed(quota.refreshed_at, nowMs, _lang) || l("刚刚刷新","Just now")}</span>
      {#if quota.error}
        <span class="qerror">{quota.error}</span>
      {/if}
      {#if !quota.cookie_error && formatExpiry(quota.expires_at ?? undefined, nowMs)}
        <span class="qexpiry {expiryUrgency(quota.expires_at ?? undefined, nowMs)}">{formatExpiry(quota.expires_at ?? undefined, nowMs, _lang)}</span>
      {/if}
    </div>
  </div>

  {#if quota.cookie_error}
  {#if editingCookie === quota.vendor}
    <div class="qcookie-edit">
      <textarea
        bind:value={cookieDraft}
        placeholder={l("粘贴新 Cookie…","Paste new Cookie…")}
        rows="4"
        disabled={cookieSaving}
        aria-label="Cookie"
      ></textarea>
      {#if regionFields.length > 0}
        <div class="qcookie-regions">
          {#each regionFields as f (f.key)}
            <label class="qregion-field">
              <span class="qregion-label">{f.label}</span>
              <Select
                class="qregion-select"
                value={regionValues[f.key] ?? ""}
                options={(f.options ?? []).map((o) => ({ value: o, label: o }))}
                disabled={cookieSaving}
                onchange={(v) => { regionValues = { ...regionValues, [f.key]: v }; }}
              />
            </label>
          {/each}
        </div>
      {/if}
      {#if cookieError}
        <p class="qcookie-err">⚠ {cookieError}</p>
      {/if}
      <div class="qcookie-actions">
        <button type="button" class="qcookie-save" onclick={saveCookie} disabled={cookieSaving || !cookieDraft.trim()}>
          {cookieSaving ? l("保存中…","Saving…") : l("保存","Save")}
        </button>
        <button type="button" class="qcookie-cancel" onclick={cancelEditCookie} disabled={cookieSaving}>{l("取消","Cancel")}</button>
      </div>
      {#if VENDOR_PANEL[quota.vendor]}
        <p class="qcookie-hint">{panelHint(VENDOR_PANEL[quota.vendor], _lang)}</p>
      {/if}
    </div>
  {:else}
    <div class="qcookie-bar">
      <span class="qcookie-text">⚠ {translateCookieError(quota.cookie_error, _lang)}</span>
      <div class="qcookie-bar-actions">
        {#if VENDOR_PANEL[quota.vendor]}
          <button type="button" class="qcookie-open" onclick={() => openPanelUrl(quota.vendor)}>{l("打开控制台","Open Console")}</button>
        {/if}
        <button type="button" class="qcookie-btn" onclick={() => startEditCookie(quota.vendor)}>{l("更新 Cookie","Update Cookie")}</button>
      </div>
    </div>
    {#if VENDOR_PANEL[quota.vendor]}
      <p class="qcookie-hint">{panelHint(VENDOR_PANEL[quota.vendor], _lang)}</p>
    {/if}
  {/if}
{/if}

{#if quota.balance}
  <div class="qitem-balance">
    <span class="qibl-label">{l("余额","Balance")}</span>
    <span class="qibl-amount">{formatBalance(quota.balance.currency ?? "USD", quota.balance.amount)}</span>
  </div>
  {#if quota.balance.today_consumption != null}
    <div class="qconsumption">
      <span class="qcons-label">{l("今日","Today")}</span>
      <span class="qcons-value">{formatCost(quota.balance.today_consumption ?? 0, currency, cnyRate)}</span>
      {#if quota.balance.month_consumption != null}
        <span class="qcons-sep">·</span>
        <span class="qcons-label">{l("本月","Month")}</span>
        <span class="qcons-value">{formatCost(quota.balance.month_consumption ?? 0, currency, cnyRate)}</span>
      {/if}
    </div>
  {/if}
{/if}

{#each quota.windows as w (w.label)}
  {@const showPct = progressMode === "用量" ? Math.round(w.used_pct) : Math.round(100 - w.used_pct)}
  {@const showLabel = progressMode === "用量" ? l("用量","Usage") : l("剩余","Remaining")}
  {@const summaryCredits = w.used_value != null && w.total_value != null ? `${fmtCredits(w.used_value)} / ${fmtCredits(w.total_value)}` : null}
  {@const hasSub = w.sub_items && w.sub_items.length > 0}
  {@const subExpanded = expandedWindows.get(w.label) ?? false}
  <div class="qitem-window">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="qiw-row"
      class:clickable={hasSub}
      role={hasSub ? "button" : undefined}
      tabindex={hasSub ? 0 : undefined}
      aria-expanded={hasSub ? subExpanded : undefined}
      onkeydown={hasSub ? (e: KeyboardEvent) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleWindow(w.label); } } : undefined}
      onclick={hasSub ? () => toggleWindow(w.label) : undefined}
    >
      <span class="qiw-label">
        {#if hasSub}
          <span class="qiw-chevron" class:open={subExpanded}>▸</span>
        {/if}
        {windowLabel(w.label, _lang)}
      </span>
      <span class="qiw-bar-col">
        <span class="qiw-bar">
          <span class="qiw-fill f-{showPct <= 20 ? 'danger' : showPct <= 50 ? 'low' : 'ok'}" style="width:{Math.min(100, Math.max(0, showPct))}%"></span>
        </span>
        {#if summaryCredits}
          <span class="qiw-bar-caption">{summaryCredits}</span>
        {/if}
      </span>
      <span class="qiw-mode-tag" class:tag-remaining={showLabel === "剩余"} class:tag-usage={showLabel === "用量"}>{showLabel}</span>
      <span class="qiw-pct">{showPct}%</span>
    </div>
    {#if hasSub && subExpanded}
      <div class="qiw-sub">
        {#each w.sub_items! as item (item.name + (item.expires_at ?? ''))}
          {@const itemShowPct = progressMode === "用量" ? Math.round(item.pct) : Math.round(100 - item.pct)}
          <div class="qsub-row">
            <span class="qsub-credits">{fmtCredits(item.used)} / {fmtCredits(item.total)}</span>
            <span class="qiw-bar-col">
              <span class="qiw-bar qsub-bar">
                <span class="qiw-fill f-{itemShowPct <= 20 ? 'danger' : itemShowPct <= 50 ? 'low' : 'ok'}" style="width:{Math.min(100, Math.max(0, itemShowPct))}%"></span>
              </span>
              {#if item.expires_at}
                <span class="qiw-bar-caption">{formatShortExpiry(item.expires_at)}{l("到期","expires")}</span>
              {/if}
            </span>
            <span class="qsub-tag-spacer"></span>
            <span class="qsub-pct">{itemShowPct}%</span>
          </div>
        {/each}
      </div>
    {:else if w.resets_at}
      <div class="qiw-reset">{formatReset(w.resets_at, nowMs, _lang)}</div>
    {/if}
  </div>
{/each}

{#if quota.windows.length === 0 && !quota.balance && !quota.cookie_error}
  <div class="qpending">{l("额度读取待实现","Quota fetch pending")}</div>
{/if}
</div>

<style>
  /* ── card shell ── */
  .qcard {
    background: var(--glass-2);
    border: 1px solid var(--border-dim);
    border-radius: 9px;
    padding: 11px 13px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── 标题行 ── */
  .qhead {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 8px;
    margin-bottom: 6px;
    border-bottom: 1px dashed var(--border);
  }
  .qhead-line {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .qhead-name-row {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .q-vendor {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text);
    font-weight: 500;
    flex-shrink: 0;
  }
  .qrefreshed {
    font-size: 10px;
    color: var(--text-dim);
    font-family: "JetBrains Mono", var(--font-mono);
    background: var(--glass-subtle);
    border: 1px solid var(--border-dim);
    padding: 1px 7px;
    border-radius: 5px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .qerror {
    font-size: 10px;
    color: var(--coral);
  }
  .qplan-tag {
    font-size: 10px;
    font-weight: 500;
    color: var(--violet);
    background: rgba(182, 155, 224, 0.12);
    padding: 1px 8px;
    border-radius: 5px;
    white-space: nowrap;
    flex-shrink: 0;
    line-height: 1.7;
  }
  .qexpiry {
    font-size: 10px;
    color: var(--text-dim);
    font-family: "JetBrains Mono", var(--font-mono);
    background: var(--glass-subtle);
    border: 1px solid var(--border-dim);
    padding: 1px 7px;
    border-radius: 5px;
    white-space: nowrap;
  }
  .qexpiry.exp-critical {
    color: var(--coral);
    border-color: var(--coral);
    background: var(--coral-bg-strong);
  }
  .qexpiry.exp-soon {
    color: var(--orange);
    border-color: var(--orange);
    background: var(--orange-bg);
  }
  .qexpiry.exp-warn {
    color: var(--amber);
    border-color: var(--amber);
    background: var(--amber-bg);
  }
  .qexpiry.exp-ok {
    color: var(--lime);
    border-color: var(--lime);
    background: var(--lime-bg);
  }

  /* ── 余额 ── */
  .qitem-balance {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .qibl-label { font-size: 11.5px; color: var(--text-dim); }
  .qibl-amount {
    font-size: 11.5px;
    color: var(--text);
    font-weight: 600;
    font-family: "JetBrains Mono", var(--font-mono);
    text-align: right;
  }
  .qconsumption {
    display: flex;
    justify-content: flex-end;
    align-items: baseline;
    gap: 4px;
    font-size: 10.5px;
  }
  .qcons-label { color: var(--text-faint); }
  .qcons-value {
    font-family: "JetBrains Mono", var(--font-mono);
    color: var(--text-dim);
  }
  .qcons-sep { color: var(--text-faint); margin: 0 2px; }

  /* ── 窗口 ── */
  .qitem-window {
    display: flex;
    flex-direction: column;
  }
  .qitem-window + .qitem-window {
    margin-top: -4px;
  }
  .qiw-row {
    display: flex;
    align-items: center;
    gap: 0;
  }
  .qiw-label {
    font-size: 11.5px;
    color: var(--text-dim);
    white-space: nowrap;
    flex-shrink: 0;
    width: 60px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .qiw-chevron {
    display: inline-block;
    font-size: 13px;
    color: var(--text-faint);
    transition: transform 0.15s;
    width: 12px;
    text-align: center;
    flex-shrink: 0;
    line-height: 1;
  }
  .qiw-chevron.open { transform: rotate(90deg); }
  .qiw-row.clickable { cursor: pointer; }
  .qiw-row.clickable:hover .qiw-label { color: var(--text); }
  .qiw-bar-col {
    flex: 1;
    min-width: 160px;
    position: relative;
    display: flex;
    align-items: center;
    padding: 0 6px;
  }
  .qiw-bar-caption {
    position: absolute;
    top: 100%;
    left: 6px;
    right: 6px;
    margin-top: 2px;
    font-size: 9px;
    color: var(--text-faint);
    font-family: "JetBrains Mono", var(--font-mono);
    text-align: center;
    line-height: 1.2;
    white-space: nowrap;
  }
  .qiw-bar {
    width: 100%;
    height: 5px;
    background: var(--bar-track);
    border-radius: 3px;
    overflow: hidden;
  }
  .qiw-fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    transition: width 0.3s;
  }
  .qiw-fill.f-ok    { background: var(--lime); }
  .qiw-fill.f-low   { background: var(--amber); }
  .qiw-fill.f-danger { background: var(--coral); }
  .qiw-mode-tag {
    flex: 0 0 52px;
    font-size: 10px;
    font-weight: 500;
    padding: 1px 5px;
    border-radius: 4px;
    text-align: center;
    flex-shrink: 0;
    line-height: 1.6;
  }
  .qiw-pct {
    flex: 0 0 34px;
    font-size: 11px;
    font-family: "JetBrains Mono", var(--font-mono);
    color: var(--text);
    font-weight: 600;
    text-align: right;
  }
  .qiw-mode-tag.tag-remaining {
    background: var(--lime-bg);
    color: var(--lime);
  }
  .qiw-mode-tag.tag-usage {
    background: var(--coral-bg);
    color: var(--coral);
  }
  .qiw-reset {
    font-size: 10.5px;
    color: var(--text-faint);
    text-align: center;
  }
  .qpending {
    font-size: 10.5px;
    color: var(--text-faint);
    font-style: italic;
  }

  /* ── Cookie 过期提示 + inline 更新 ── */
  .qcookie-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    border-radius: 6px;
    background: var(--coral-bg-soft);
    border: 1px solid var(--coral-border);
  }
  .qcookie-text {
    font-size: 10.5px;
    color: var(--coral);
    flex: 1;
    min-width: 0;
    line-height: 1.4;
  }
  .qcookie-btn {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--coral);
    background: var(--coral-bg-strong);
    border: 1px solid rgba(234, 84, 85, 0.45);
    padding: 2px 9px;
    border-radius: 5px;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .qcookie-btn:hover { background: rgba(234, 84, 85, 0.28); }
  .qcookie-edit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 9px;
    border-radius: 6px;
    background: var(--coral-bg-soft);
    border: 1px solid var(--coral-border);
  }
  .qcookie-edit textarea {
    width: 100%;
    box-sizing: border-box;
    font-size: 10.5px;
    font-family: "JetBrains Mono", var(--font-mono);
    color: var(--text);
    background: var(--glass-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 5px 6px;
    resize: vertical;
  }
  .qcookie-edit textarea::placeholder { color: var(--text-faint); }
  .qcookie-edit textarea:focus {
    outline: none;
    border-color: var(--amber);
    background: var(--glass-3);
  }
  .qcookie-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .qcookie-save,
  .qcookie-cancel {
    font-size: 10.5px;
    font-weight: 600;
    padding: 4px 14px;
    border-radius: 5px;
    cursor: pointer;
    border: 1px solid var(--border-dim);
  }
  .qcookie-save {
    background: var(--lime);
    color: var(--lime-text-on-bg);
    border-color: var(--lime);
  }
  .qcookie-save:hover:not(:disabled) { opacity: 0.9; }
  .qcookie-save:disabled { opacity: 0.5; cursor: not-allowed; }
  .qcookie-cancel {
    background: var(--glass-3);
    color: var(--text);
    border-color: var(--border);
  }
  .qcookie-cancel:hover {
    background: var(--glass-2);
    border-color: var(--text-dim);
  }
  .qcookie-bar-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .qcookie-open {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--amber);
    background: var(--amber-hover);
    border: 1px solid rgba(232, 176, 75, 0.35);
    padding: 2px 10px;
    border-radius: 5px;
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: all 0.15s;
  }
  .qcookie-open:hover {
    background: rgba(232, 176, 75, 0.18);
    border-color: var(--amber);
  }
  .qcookie-hint {
    font-size: 10.5px;
    color: var(--text-faint);
    margin: 6px 0 0;
    line-height: 1.6;
  }

  /* ── Cookie validation error ── */
  .qcookie-err {
    font-size: 10.5px;
    color: var(--coral);
    margin: 4px 0 0;
    line-height: 1.5;
  }

  /* ── Region/site selectors inside the cookie editor ── */
  .qcookie-regions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .qregion-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .qregion-label {
    font-size: 10px;
    color: var(--text-faint);
  }

  /* ── Sub-items (individual quota_detail entries) ── */
  .qiw-sub {
    margin-top: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
  }
  .qsub-row {
    display: flex;
    align-items: center;
    gap: 0;
    padding-left: 10px;
    border-left: 1px solid var(--border-dim);
    width: 100%;
    box-sizing: border-box;
  }
  .qsub-credits {
    flex: 0 0 60px;
    font-size: 10px;
    color: var(--text-dim);
    font-family: "JetBrains Mono", var(--font-mono);
    text-align: right;
    white-space: nowrap;
    flex-shrink: 0;
    user-select: text;
    -webkit-user-select: text;
  }
  .qsub-tag-spacer {
    flex: 0 0 52px;
    flex-shrink: 0;
  }
  .qsub-bar { height: 3px; }
  .qsub-pct {
    flex: 0 0 34px;
    font-size: 9.5px;
    font-family: "JetBrains Mono", var(--font-mono);
    color: var(--text-dim);
    text-align: right;
    flex-shrink: 0;
  }
</style>
