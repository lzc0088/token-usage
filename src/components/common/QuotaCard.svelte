<!--! Shared quota card — used by Overview and Limits segments.
   Renders a single vendor's quota: header (name + plan + expiry),
   cookie editor (inline when expired), balance, and usage windows
   with expandable sub-items. -->

<script lang="ts">
  import { VENDOR_PANEL } from "../../lib/meta/panels";
  import ToolIcon from "../ui/ToolIcon.svelte";
  import { VENDOR_LABELS } from "../../lib/meta/vendors";
  import { open } from "@tauri-apps/plugin-shell";
  import { api } from "../../lib/api";
  import type { Quota } from "../../lib/api";

  type ProgressMode = "用量" | "剩余";

  let {
    quota,
    progressMode = $bindable("用量"),
    nowMs = 0,
  }: {
    quota: Quota;
    progressMode: ProgressMode;
    nowMs: number;
  } = $props();

  // ── Inline cookie editor (per-card state) ──
  let editingCookie = $state(false);
  let cookieDraft = $state("");
  let cookieSaving = $state(false);

  function startEditCookie(): void {
    editingCookie = true;
    cookieDraft = "";
  }
  function cancelEditCookie(): void {
    editingCookie = false;
    cookieDraft = "";
  }
  async function saveCookie(): Promise<void> {
    const draft = cookieDraft.trim();
    if (!draft) return;
    cookieSaving = true;
    try {
      await api.updateCookie(quota.vendor, draft);
      editingCookie = false;
      cookieDraft = "";
      await api.refreshQuota(quota.vendor);
    } catch (e) {
      console.error("update cookie failed", e instanceof Error ? e.message : String(e));
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

  // ── Helpers (passed from parent or computed here) ──
  function formatRefreshed(iso: string | undefined, now: number): string {
    if (!iso) return "";
    const then = Date.parse(iso);
    if (isNaN(then)) return "";
    const secs = Math.floor((now - then) / 1000);
    if (secs < 0) return "Updated just now";
    if (secs < 60) return `Updated ${secs}s ago`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `Updated ${mins}min ago`;
    const hrs = Math.floor(mins / 60);
    return `Updated ${hrs}h ago`;
  }
  function formatExpiry(q: Quota, now: number): string {
    if (!q.expires_at) return "";
    const nearest = Date.parse(q.expires_at);
    if (!Number.isFinite(nearest)) return "";
    if (nearest <= now) return "已到期";
    const d = new Date(nearest);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    const secs = Math.floor((nearest - now) / 1000);
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    let remain: string;
    if (days > 0) remain = `${days}d ${hours}h ${mins}m`;
    else if (hours > 0) remain = `${hours}h ${mins}m`;
    else remain = `${mins}m`;
    return `${yyyy}-${mm}-${dd}到期 · 剩余${remain}`;
  }
  function expiryUrgency(q: Quota, now: number): string {
    if (!q.expires_at) return "";
    const nearest = Date.parse(q.expires_at);
    if (!Number.isFinite(nearest)) return "";
    const days = (nearest - now) / 86400000;
    if (days <= 3) return "exp-critical";
    if (days <= 7) return "exp-soon";
    if (days <= 30) return "exp-warn";
    return "exp-ok";
  }
  function fmtCredits(n: number | undefined | null): string {
    if (n == null) return "—";
    const isInt = n % 1 === 0;
    const s = isInt ? String(n) : n.toFixed(1);
    const parts = s.split(".");
    parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    return parts.join(".");
  }
  function formatReset(resetsAt: string | undefined, now: number): string {
    if (!resetsAt) return "";
    const target = Date.parse(resetsAt);
    if (!Number.isFinite(target)) return "";
    const secs = Math.floor((target - now) / 1000);
    if (secs <= 0) return "Reset now";
    const days = Math.floor(secs / 86400);
    const hours = Math.floor((secs % 86400) / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    if (days > 0) return `Reset ${days}d ${hours}h`;
    if (hours > 0) return `Reset ${hours}h ${mins}min`;
    return `Reset ${mins}min`;
  }
  function formatShortExpiry(iso: string): string {
    const target = Date.parse(iso);
    if (!Number.isFinite(target)) return "";
    const d = new Date(target);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
  }
  const WINDOW_LABELS: Record<string, string> = {
    "5h": "5小时", "周": "7天", "月": "每月", "MCP 月": "MCP 每月",
  };
  function windowLabel(raw: string): string {
    return WINDOW_LABELS[raw] ?? raw;
  }
  function openPanelUrl(vendor: string): void {
    const panel = VENDOR_PANEL[vendor];
    if (!panel) return;
    const url = typeof panel.url === "string" ? panel.url : Object.values(panel.url.map)[0] ?? "";
    if (url) open(url).catch(() => {});
  }
  function formatBalance(b: NonNullable<Quota["balance"]>): string {
    const sym = b.currency === "CNY" ? "¥" : b.currency === "USD" ? "$" : "";
    return `${sym}${b.amount.toFixed(2)}`;
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
      <span class="qrefreshed">{formatRefreshed(quota.refreshed_at, nowMs) || "Updated just now"}</span>
      {#if quota.error}
        <span class="qerror">{quota.error}</span>
      {/if}
      {#if !quota.cookie_error && formatExpiry(quota, nowMs)}
        <span class="qexpiry {expiryUrgency(quota, nowMs)}">{formatExpiry(quota, nowMs)}</span>
      {/if}
    </div>
  </div>

  {#if quota.cookie_error}
  {#if editingCookie}
    <div class="qcookie-edit">
      <textarea
        bind:value={cookieDraft}
        placeholder="粘贴新 Cookie…"
        rows="3"
        disabled={cookieSaving}
        aria-label="Cookie"
      ></textarea>
      <div class="qcookie-actions">
        <button class="qcookie-save" onclick={saveCookie} disabled={cookieSaving || !cookieDraft.trim()}>
          {cookieSaving ? "保存中…" : "保存"}
        </button>
        <button class="qcookie-cancel" onclick={cancelEditCookie} disabled={cookieSaving}>取消</button>
      </div>
      {#if VENDOR_PANEL[quota.vendor]}
        <p class="qcookie-hint">{VENDOR_PANEL[quota.vendor].hint}</p>
      {/if}
    </div>
  {:else}
    <div class="qcookie-bar">
      <span class="qcookie-text">⚠ {quota.cookie_error}</span>
      <div class="qcookie-bar-actions">
        {#if VENDOR_PANEL[quota.vendor]}
          <button class="qcookie-open" onclick={() => openPanelUrl(quota.vendor)}>打开控制台</button>
        {/if}
        <button class="qcookie-btn" onclick={startEditCookie}>更新 Cookie</button>
      </div>
    </div>
    {#if VENDOR_PANEL[quota.vendor]}
      <p class="qcookie-hint">{VENDOR_PANEL[quota.vendor].hint}</p>
    {/if}
  {/if}
{/if}

{#if quota.balance}
  <div class="qitem-balance">
    <span class="qibl-label">余额</span>
    <span class="qibl-amount">{formatBalance(quota.balance)}</span>
  </div>
  {#if quota.balance.today_consumption != null}
    <div class="qconsumption">
      <span>今日: ${(quota.balance.today_consumption ?? 0).toFixed(2)}</span>
      {#if quota.balance.month_consumption != null}
        <span>本月: ${(quota.balance.month_consumption ?? 0).toFixed(2)}</span>
      {/if}
    </div>
  {/if}
{/if}

{#each quota.windows as w (w.label)}
  {@const showPct = progressMode === "用量" ? Math.round(w.used_pct) : Math.round(100 - w.used_pct)}
  {@const showLabel = progressMode === "用量" ? "用量" : "剩余"}
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
        {windowLabel(w.label)}
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
                <span class="qiw-bar-caption">{formatShortExpiry(item.expires_at)}到期</span>
              {/if}
            </span>
            <span class="qsub-tag-spacer"></span>
            <span class="qsub-pct">{itemShowPct}%</span>
          </div>
        {/each}
      </div>
    {:else if w.resets_at}
      <div class="qiw-reset">{formatReset(w.resets_at, nowMs)}</div>
    {/if}
  </div>
{/each}

{#if quota.windows.length === 0 && !quota.balance && !quota.cookie_error}
  <div class="qpending">额度读取待实现</div>
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
    background: rgba(255, 255, 255, 0.04);
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
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--border-dim);
    padding: 1px 7px;
    border-radius: 5px;
    white-space: nowrap;
  }
  .qexpiry.exp-critical {
    color: var(--coral);
    border-color: var(--coral);
    background: rgba(234, 84, 85, 0.14);
  }
  .qexpiry.exp-soon {
    color: #e8834a;
    border-color: #e8834a;
    background: rgba(232, 131, 74, 0.12);
  }
  .qexpiry.exp-warn {
    color: var(--amber);
    border-color: var(--amber);
    background: rgba(232, 176, 75, 0.12);
  }
  .qexpiry.exp-ok {
    color: var(--lime);
    border-color: var(--lime);
    background: rgba(108, 199, 116, 0.12);
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
    gap: 14px;
    font-size: 10.5px;
    color: var(--text-faint);
  }

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
    background: rgba(108, 199, 116, 0.12);
    color: var(--lime);
  }
  .qiw-mode-tag.tag-usage {
    background: rgba(224, 108, 117, 0.12);
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
    background: rgba(234, 84, 85, 0.10);
    border: 1px solid rgba(234, 84, 85, 0.40);
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
    background: rgba(234, 84, 85, 0.18);
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
    background: rgba(234, 84, 85, 0.08);
    border: 1px solid rgba(234, 84, 85, 0.40);
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
    color: #14310f;
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
    background: rgba(232, 176, 75, 0.10);
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
