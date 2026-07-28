<script lang="ts">
  // 额度 segment. Loads cached quotas from `get_quotas` once on open — no timer.
  // Refresh is driven by the background scheduler (quota_refresh_interval in settings).
  // Countdown timers for reset times still tick locally.
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { api, type Config, type Quota, type QuotaBalance } from "../../lib/api";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { VENDOR_LABELS } from "../../lib/meta/vendors";
  import { VENDOR_PANEL, resolvePanelUrl } from "../../lib/meta/panels";

  let quotas = $state<Quota[] | null>(null);
  let config = $state<Config | null>(null);
  let nowMs = $state(Date.now());
  /** Tracks which windows are expanded (key = window label). */
  let expandedWindows = $state<Map<string, boolean>>(new Map());

  function toggleWindow(label: string): void {
    const cur = expandedWindows.get(label) ?? false;
    expandedWindows.set(label, !cur);
    expandedWindows = new Map(expandedWindows); // trigger reactivity
  }

  // Tick every 30s so reset countdowns stay live (local only, no API calls).
  $effect(() => {
    const t = setInterval(() => { nowMs = Date.now(); }, 30_000);
    return () => clearInterval(t);
  });

  // Load on mount + re-fetch on window focus (e.g. settings changed).
  async function refresh(): Promise<void> {
    try {
      const [q, c] = await Promise.all([api.getQuotas(), api.getConfig()]);
      quotas = q;
      config = c;
    } catch (e) {
      console.error("quotas failed", e instanceof Error ? e.message : String(e));
    }
  }

  $effect(() => {
    // Use void to avoid returning the promise to Svelte's effect cleanup.
    void refresh();
    const onFocus = () => { void refresh(); };
    window.addEventListener("focus", onFocus);
    // Live-update when the background scheduler finishes a refresh cycle.
    const unlisten_promise = listen<void>("quota:updated", () => { void refresh(); });
    return () => {
      window.removeEventListener("focus", onFocus);
      unlisten_promise.then((un) => un());
    };
  });

  // On mount: if cached quota is older than the configured cadence
  // (quota_refresh_interval), trigger a background refresh now so the user
  // never sees stale data on entry. No-op when fresh — the scheduler handles
  // it. The quota:updated listener above reloads once the refresh finishes.
  $effect(() => {
    void api.refreshQuotasIfStale().catch(() => {});
  });

  /** Map window labels to Chinese display names. */
  const WINDOW_LABELS: Record<string, string> = {
    "5h": "5小时",
    "周": "7天",
    "月": "每月",
    "MCP 月": "MCP 每月",
  };
  function windowLabel(raw: string): string {
    return WINDOW_LABELS[raw] ?? raw;
  }

  /** Compute "Updated X ago" from an RFC3339 timestamp. */
  function formatRefreshed(refreshedAt: string | undefined, now: number): string {
    if (!refreshedAt) return "";
    const target = Date.parse(refreshedAt);
    if (!Number.isFinite(target)) return "";
    const secs = Math.floor((now - target) / 1000);
    if (secs < 0) return "Updated just now";
    if (secs < 60) return `Updated ${secs}s ago`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `Updated ${mins}min ago`;
    const hrs = Math.floor(mins / 60);
    return `Updated ${hrs}h ago`;
  }

  function formatBalance(b: QuotaBalance): string {
    const sym = b.currency === "CNY" ? "¥" : b.currency === "USD" ? "$" : "";
    return `${sym}${b.amount.toFixed(2)}`;
  }
  function openPanelUrl(vendor: string): void {
    const url = resolvePanelUrl(vendor);
    if (url) open(url);
  }
  /** ISO reset timestamp → "Reset 3h12min" etc., or "" if absent/past-far. */
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

  /** Format a credits number: integers as "1,500", floats as "1,234.5". */
  function fmtCredits(n: number | undefined | null): string {
    if (n == null) return "—";
    const isInt = n % 1 === 0;
    const s = isInt ? String(n) : n.toFixed(1);
    const parts = s.split(".");
    parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    return parts.join(".");
  }

  /** Full expiry date: "2026-12-31" or "" if absent/unparseable.
   *  Caller appends "到期". */
  function formatShortExpiry(iso: string): string {
    const target = Date.parse(iso);
    if (!Number.isFinite(target)) return "";
    const d = new Date(target);
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, "0");
    const dd = String(d.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
  }
  /** Plan-level subscription expiry. Only `expires_at` is considered — per-window
   *  `resets_at` is a rolling quota reset (already shown in each window row),
   *  not a subscription end date. Returns epoch ms or `undefined`. */
  function nearestExpiry(q: Quota, _now: number): number | undefined {
    if (!q.expires_at) return undefined;
    const t = Date.parse(q.expires_at);
    return (Number.isFinite(t) && t > _now) ? t : undefined;
  }
  /** "2026-10-19到期 · 剩余86d 10h 22m" or "" if no upcoming expiry. */
  function formatExpiry(q: Quota, now: number): string {
    const nearest = nearestExpiry(q, now);
    if (nearest === undefined) return "";
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
  /** Tag color severity based on how close the nearest expiry is.
   *  ≤3d 危急(红) / ≤7d 紧张(橙红) / ≤30d 提醒(黄) / >30d 正常(绿). */
  function expiryUrgency(q: Quota, now: number): string {
    const nearest = nearestExpiry(q, now);
    if (nearest === undefined) return "exp-expired";
    const days = (nearest - now) / 86400000;
    if (days <= 3) return "exp-critical";
    if (days <= 7) return "exp-soon";
    if (days <= 30) return "exp-warn";
    return "exp-ok";
  }

  // ── Inline cookie update (shown when a vendor's cookie has expired) ──
  let editingCookieVendor = $state<string | null>(null);
  let cookieDraft = $state("");
  let cookieSaving = $state(false);

  function startEditCookie(vendor: string): void {
    editingCookieVendor = vendor;
    cookieDraft = "";
  }
  function cancelEditCookie(): void {
    editingCookieVendor = null;
    cookieDraft = "";
  }
  /** Persist the new cookie (preserving key/secret server-side) + refresh. */
  async function saveCookie(vendor: string): Promise<void> {
    const draft = cookieDraft.trim();
    if (!draft) return;
    cookieSaving = true;
    try {
      await api.updateCookie(vendor, draft);
      await api.refreshQuota(vendor);
      editingCookieVendor = null;
      cookieDraft = "";
      await refresh();
    } catch (e) {
      console.error("update cookie failed", e instanceof Error ? e.message : String(e));
    } finally {
      cookieSaving = false;
    }
  }
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
    {@const progressMode = config?.quota_progress_mode ?? "剩余"}
    {#each quotas as q (q.vendor)}
      <div class="qcard" data-status={q.status}>
        <!-- 标题行：厂商名(左) + 套餐名(右) + 刷新时间(左下行) -->
        <div class="qhead">
          <div class="qhead-line">
            <div class="qhead-name-row">
              <span class="q-vendor"><ToolIcon vendor={q.vendor} badge={false} size={11} />{VENDOR_LABELS[q.vendor] ?? q.vendor}</span>
            </div>
            {#if q.plan_label}
              <span class="qplan-tag">{q.plan_label}</span>
            {/if}
          </div>
          <div class="qhead-line">
            <span class="qrefreshed">{formatRefreshed(q.refreshed_at, nowMs) || "Updated just now"}</span>
            {#if q.error}
              <span class="qerror">{q.error}</span>
            {/if}
            {#if !q.cookie_error && formatExpiry(q, nowMs)}
              <span class="qexpiry {expiryUrgency(q, nowMs)}">{formatExpiry(q, nowMs)}</span>
            {/if}
          </div>
        </div>

        {#if q.cookie_error}
          {#if editingCookieVendor === q.vendor}
            <div class="qcookie-edit">
              <textarea
                bind:value={cookieDraft}
                placeholder="粘贴新 Cookie…"
                rows="3"
                disabled={cookieSaving}
                aria-label="Cookie"
              ></textarea>
              <div class="qcookie-actions">
                <button
                  class="qcookie-save"
                  onclick={() => saveCookie(q.vendor)}
                  disabled={cookieSaving || !cookieDraft.trim()}
                >{cookieSaving ? "保存中…" : "保存"}</button>
                <button
                  class="qcookie-cancel"
                  onclick={cancelEditCookie}
                  disabled={cookieSaving}
                >取消</button>
              </div>
              {#if VENDOR_PANEL[q.vendor]}
                <p class="qcookie-hint">{VENDOR_PANEL[q.vendor].hint}</p>
              {/if}
            </div>
          {:else}
            <div class="qcookie-bar">
              <span class="qcookie-text">⚠ {q.cookie_error}</span>
              <div class="qcookie-bar-actions">
                {#if VENDOR_PANEL[q.vendor]}
                  <button class="qcookie-open" onclick={() => openPanelUrl(q.vendor)}>
                    打开控制台
                  </button>
                {/if}
                <button class="qcookie-btn" onclick={() => startEditCookie(q.vendor)}>更新 Cookie</button>
              </div>
            </div>
            {#if VENDOR_PANEL[q.vendor]}
              <p class="qcookie-hint">{VENDOR_PANEL[q.vendor].hint}</p>
            {/if}
          {/if}
        {/if}

        {#if q.balance}
          <!-- 余额：标签(左) + 金额(右)，无进度条 -->
          <div class="qitem-balance">
            <span class="qibl-label">余额</span>
            <span class="qibl-amount">{formatBalance(q.balance)}</span>
          </div>
          {#if q.balance.today_consumption}
            <div class="qconsumption">
              <span>今日: ${(q.balance.today_consumption ?? 0).toFixed(2)}</span>
              {#if q.balance.month_consumption}
                <span>本月: ${(q.balance.month_consumption ?? 0).toFixed(2)}</span>
              {/if}
            </div>
          {/if}
        {/if}

        {#each q.windows as w (w.label)}
          {@const showPct = progressMode === "用量" ? Math.round(w.used_pct) : Math.round(100 - w.used_pct)}
          {@const showLabel = progressMode === "用量" ? "用量" : "剩余"}
          {@const summaryCredits = w.used_value != null && w.total_value != null
            ? `${fmtCredits(w.used_value)} / ${fmtCredits(w.total_value)}`
            : null}
          {@const hasSub = w.sub_items && w.sub_items.length > 0}
          {@const subExpanded = expandedWindows.get(w.label) ?? false}
          <div class="qitem-window">
            <!-- 汇总行：label | 进度条列(进度条 + credits说明) | 类型 | 百分比 -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div class="qiw-row" class:clickable={hasSub} role={hasSub ? "button" : undefined} tabindex={hasSub ? 0 : undefined} onkeydown={hasSub ? (e: KeyboardEvent) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleWindow(w.label); } } : undefined} onclick={hasSub ? () => toggleWindow(w.label) : undefined}>
              <span class="qiw-label">
                {#if hasSub}
                  <span class="qiw-chevron" class:open={subExpanded}>▸</span>
                {/if}
                {windowLabel(w.label)}
              </span>
              <span class="qiw-bar-col">
                <span class="qiw-bar"><span class="qiw-fill f-{showPct <= 20 ? 'danger' : showPct <= 50 ? 'low' : 'ok'}" style="width:{Math.min(100, Math.max(0, showPct))}%"></span></span>
                {#if summaryCredits}
                  <span class="qiw-bar-caption">{summaryCredits}</span>
                {/if}
              </span>
              <span class="qiw-mode-tag" class:tag-remaining={showLabel === "剩余"} class:tag-usage={showLabel === "用量"}>{showLabel}</span>
              <span class="qiw-pct">{showPct}%</span>
            </div>
            <!-- 子项列表（可折叠）：credits | 进度条列(进度条 + 到期日) | 百分比 -->
            {#if hasSub && subExpanded}
              <div class="qiw-sub">
                {#each w.sub_items! as item (item.name + (item.expires_at ?? ''))}
                  {@const itemShowPct = progressMode === "用量" ? Math.round(item.pct) : Math.round(100 - item.pct)}
                  <div class="qsub-row">
                    <span class="qsub-credits">{fmtCredits(item.used)} / {fmtCredits(item.total)}</span>
                    <span class="qiw-bar-col">
                      <span class="qiw-bar qsub-bar"><span class="qiw-fill f-{itemShowPct <= 20 ? 'danger' : itemShowPct <= 50 ? 'low' : 'ok'}" style="width:{Math.min(100, Math.max(0, itemShowPct))}%"></span></span>
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

        {#if q.windows.length === 0 && !q.balance && !q.cookie_error}
          <div class="qpending">额度读取待实现</div>
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
  /* Each header line is its own two-column row (left + right via space-between),
     so the expiry tag on line 2 reaches the card's right edge. */
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
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex-shrink: 0;
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
  .qhead-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
    flex-shrink: 0;
    min-width: 0;
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
  /* Tighten spacing between consecutive windows — the card's 10px gap is too
     large once a window carries a Reset line, making groups feel disconnected. */
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
  /* Chevron for expandable windows */
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
  .qiw-chevron.open {
    transform: rotate(90deg);
  }
  /* Clickable summary row */
  .qiw-row.clickable {
    cursor: pointer;
  }
  .qiw-row.clickable:hover .qiw-label {
    color: var(--text);
  }
  /* Bar column: the bar defines the row's vertical center line so the label,
     bar, tag and pct all sit on the same horizontal axis. The caption (credits
     / expiry) is absolutely positioned below the bar so it doesn't shift the
     bar off that center line. */
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
  .qcookie-btn:hover {
    background: rgba(234, 84, 85, 0.28);
  }
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
  .qcookie-edit textarea::placeholder {
    color: var(--text-faint);
  }
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
  .qcookie-save:hover:not(:disabled) {
    opacity: 0.9;
  }
  .qcookie-save:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .qcookie-cancel {
    background: var(--glass-3);
    color: var(--text);
    border-color: var(--border);
  }
  .qcookie-cancel:hover {
    background: var(--glass-2);
    border-color: var(--text-dim);
  }

  /* ── cookie bar action buttons ── */
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

  /* ── cookie hint text ── */
  .qcookie-hint {
    font-size: 10.5px;
    color: var(--text-faint);
    margin: 6px 0 0;
    line-height: 1.6;
  }

  /* ── Sub-items (individual quota_detail entries) ──
     Column widths mirror the summary row (label/credits 60px, mode-tag/spacer
     52px, pct 34px) so the progress bar's right edge and the percentage both
     right-align with the summary row. A left border + padding gives the
     visual indent without shifting the right edge. */
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
  .qsub-bar {
    height: 3px;
  }
  .qsub-pct {
    flex: 0 0 34px;
    font-size: 9.5px;
    font-family: "JetBrains Mono", var(--font-mono);
    color: var(--text-dim);
    text-align: right;
    flex-shrink: 0;
  }
</style>
