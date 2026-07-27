<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { api, type Breakdown, type Currency, type Quota, type Summary } from "../../lib/api";
  import { formatCost, splitTokens } from "../../lib/format";
  import { modelVendor } from "../../lib/modelMeta";
  import { toolMeta } from "../../lib/toolMeta";
  import ToolIcon from "../../lib/ToolIcon.svelte";
  import { VENDOR_LABELS } from "../../lib/vendorLabels";
  import { VENDOR_PANEL, resolvePanelUrl } from "../../lib/vendorPanel";
  import { periodValue } from "../../stores/period.svelte";
  import { setSegment } from "../../stores/segment.svelte";

  interface OverviewConfig {
    layout_overview_sub?: string[] | null;
  }

  let {
    summary,
    currency,
    cnyRate = 7.2,
    config,
  }: { summary: Summary | null; currency: Currency; cnyRate?: number; config?: OverviewConfig } = $props();

  let toolB = $state<Breakdown | null>(null);
  let modelB = $state<Breakdown | null>(null);
  let quotas = $state<Quota[]>([]);
  let overviewQuotaVendors = $state<string[] | null>(null);
  let activeVendors = $state<string[] | null>(null);
  let progressMode = $state<"用量" | "剩余">("剩余");
  let nowMs = $state(Date.now());

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    (async () => {
      try {
        const [t, m, q, cfg] = await Promise.all([
          api.getBreakdown(p, "tool"),
          api.getBreakdown(p, "model"),
          api.getQuotas(),
          api.getConfig(),
        ]);
        if (!cancelled) {
          toolB = t;
          modelB = m;
          quotas = q;
          overviewQuotaVendors = cfg?.overview_quota_vendors ?? null;
          activeVendors = cfg?.quota_active_vendors ?? null;
          progressMode = (cfg?.quota_progress_mode as "用量" | "剩余") ?? "剩余";
        }
      } catch {
        if (!cancelled) { toolB = null; modelB = null; quotas = []; }
      }
    })();
    return () => { cancelled = true; };
  });
  $effect(() => {
    const t = setInterval(() => { nowMs = Date.now(); }, 30_000);
    return () => clearInterval(t);
  });

  // Live-update quotas when the background scheduler finishes a refresh cycle.
  $effect(() => {
    const unlisten_promise = listen<void>("quota:updated", () => {
      api.getQuotas().then((q) => { quotas = q; }).catch(console.error);
    });
    return () => { unlisten_promise.then((un) => un()); };
  });

  // On mount: if cached quota is older than the configured cadence
  // (quota_refresh_interval), trigger a background refresh now so the user
  // never sees stale data on entry. No-op when fresh — the scheduler handles
  // it. The quota:updated listener above reloads once the refresh finishes.
  $effect(() => {
    void api.refreshQuotasIfStale().catch(() => {});
  });

  const visibleQuotas = $derived.by(() => {
    if (quotas.length === 0) return [];
    let filtered = quotas;
    if (activeVendors) {
      const set = new Set(activeVendors);
      filtered = filtered.filter(q => set.has(q.vendor));
    }
    if (filtered.length === 0) return [];
    if (overviewQuotaVendors) {
      const set = new Set(overviewQuotaVendors);
      filtered = filtered.filter(q => set.has(q.vendor));
    }
    if (filtered.length === 0) return [];
    if (overviewQuotaVendors) {
      const order = new Map(overviewQuotaVendors.map((v, i) => [v, i]));
      filtered = [...filtered].sort((a, b) => (order.get(a.vendor) ?? 999) - (order.get(b.vendor) ?? 999));
    }
    return filtered;
  });

  const DEFAULT_SUB_ORDER = ["overview_io", "overview_tools", "overview_models", "overview_quotas"] as const;
  // $derived so section order/visibility tracks config prop changes live
  // (not just on component remount).
  const subOrder = $derived(config?.layout_overview_sub ?? DEFAULT_SUB_ORDER);

  // All sections are shown by default (visibility controlled by subOrder)
  // The sections themselves handle empty state internally.

  const palette = ["var(--amber)", "var(--lime)", "var(--cyan)", "var(--violet)", "var(--coral)"];

  const WINDOW_LABELS: Record<string, string> = {
    "5h": "5小时", "周": "7天", "月": "每月", "MCP 月": "MCP 每月",
  };
  function windowLabel(raw: string): string {
    return WINDOW_LABELS[raw] ?? raw;
  }

  /** Nearest future expiry — prefer `expires_at` (plan end), else scan windows. */
  function nearestExpiry(q: Quota, now: number): number | undefined {
    if (q.expires_at) {
      const t = Date.parse(q.expires_at);
      if (Number.isFinite(t) && t > now) return t;
    }
    let nearest: number | undefined;
    for (const w of q.windows) {
      if (!w.resets_at) continue;
      const t = Date.parse(w.resets_at);
      if (!Number.isFinite(t) || t <= now) continue;
      if (nearest === undefined || t < nearest) nearest = t;
    }
    return nearest;
  }
  /** "2026-08-09到期 · 剩余14d 3h 22m" or "" if no upcoming expiry. */
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
  function formatBalance(b: NonNullable<Quota["balance"]>): string {
    const sym = b.currency === "CNY" ? "¥" : b.currency === "USD" ? "$" : "";
    return `${sym}${b.amount.toFixed(2)}`;
  }
  function openPanelUrl(vendor: string): void {
    const url = resolvePanelUrl(vendor);
    if (url) open(url);
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
  async function saveCookie(vendor: string): Promise<void> {
    const draft = cookieDraft.trim();
    if (!draft) return;
    cookieSaving = true;
    try {
      await api.updateCookie(vendor, draft);
      editingCookieVendor = null;
      cookieDraft = "";
      // Refresh this vendor's quota and reload the list.
      await api.refreshQuota(vendor);
      const q = await api.getQuotas();
      quotas = q;
    } catch (e) {
      console.error("update cookie failed", e instanceof Error ? e.message : String(e));
    } finally {
      cookieSaving = false;
    }
  }
</script>
<div class="ov-body">
  {#if !summary && !toolB && !modelB && quotas.length === 0}
    <p class="muted">加载中…</p>
  {/if}

  {#each subOrder as sectionKey (sectionKey)}
    {#if sectionKey === "overview_io"}
      <section class="module">
        <div class="split2">
          {#if summary}
            {@const cells = [
              { k: "输入", v: summary.input, cls: "" },
              { k: "输出", v: summary.output, cls: "amber" },
              { k: "缓存读取", v: summary.cache_read, cls: "lime" },
              { k: "缓存写入", v: summary.cache_write, cls: "coral" },
            ]}
            {#each cells as cell}
              {@const s = splitTokens(cell.v)}
              <div class="scell"><div class="k">{cell.k}</div><div class="v {cell.cls}">{s.value}<span class="u">{s.unit}</span></div></div>
            {/each}
          {:else}
            <p class="empty">暂无数据</p>
          {/if}
        </div>
      </section>
    {/if}

    {#if sectionKey === "overview_tools"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--amber)"></span>工具</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("tools")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("tools")}>全部</span></div>
        {#if toolB && toolB.entries.length > 0}
          {#each toolB.entries.slice(0, 3) as e, i (e.key)}
            {@const s = splitTokens(e.tokens)}
            <div class="crow" role="button" tabindex="0" onclick={() => setSegment("tools")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("tools")}>
              <span class="rk" style="background:{toolMeta(e.key).color};color:#1a1408">{@html toolMeta(e.key).icon}</span>
              <span class="nm">{toolMeta(e.key).label}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {s.value}<span class="su">{s.unit}</span></span></span>
              <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[i % palette.length]}"></i></div>
              <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
              <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
            </div>
          {/each}
        {:else}
          <p class="empty">暂无工具数据 — 等待 tokscale 采集</p>
        {/if}
      </section>
    {/if}

    {#if sectionKey === "overview_models"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--cyan)"></span>模型</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("models")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("models")}>全部</span></div>
        {#if modelB && modelB.entries.length > 0}
          {#each modelB.entries.slice(0, 3) as e, i (e.key)}
            {@const s = splitTokens(e.tokens)}
            {@const mv = modelVendor(e.key)}
            <div class="crow" role="button" tabindex="0" onclick={() => setSegment("models")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("models")}>
              <span class="rk" style="background:{toolMeta(e.key).color};color:#1a1408">{@html toolMeta(e.key).icon}</span>
              <span class="nm">{toolMeta(e.key).label}{#if mv}<span class="mvendor" style="color:{mv.color}">{mv.vendor}</span>{/if}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {e.messages} 会话</span></span>
              <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[(i + 2) % palette.length]}"></i></div>
              <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
              <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
            </div>
          {/each}
        {:else}
          <p class="empty">暂无模型数据 — 等待 tokscale 采集</p>
        {/if}
      </section>
    {/if}

    {#if sectionKey === "overview_quotas"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--violet)"></span>额度</span><span class="more" role="button" tabindex="0" onclick={() => setSegment("limit")} onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && setSegment("limit")}>全部</span></div>
        <div class="qcard-list">
          {#each visibleQuotas as q (q.vendor)}
            <div class="qcard" data-status={q.status}>
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
                {@const resetText = formatReset(w.resets_at, nowMs)}
                <div class="qitem-window">
                  <div class="qiw-row">
                    <span class="qiw-label">{windowLabel(w.label)}</span>
                    <span class="qiw-bar-wrap"><span class="qiw-bar"><span class="qiw-fill f-{showPct <= 20 ? 'danger' : showPct <= 50 ? 'low' : 'ok'}" style="width:{Math.min(100, Math.max(0, showPct))}%"></span></span></span>
                    <span class="qiw-mode-tag" class:tag-remaining={showLabel === "剩余"} class:tag-usage={showLabel === "用量"}>{showLabel}</span>
                    <span class="qiw-pct">{showPct}%</span>
                  </div>
                  {#if resetText}
                    <div class="qiw-reset">{resetText}</div>
                  {/if}
                </div>
              {/each}

              {#if q.windows.length === 0 && !q.balance && !q.cookie_error}
                <div class="qpending">额度读取待实现</div>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}
  {/each}
</div>

<style>
  .ov-body { padding: 14px 18px; display: flex; flex-direction: column; }
  .module {
    margin-bottom: 14px; padding-bottom: 14px;
    border-bottom: 1px solid var(--border-dim);
  }
  .module:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }
  .sec-h {
    font-size: 13px; font-weight: 700;
    color: var(--text);
    margin-bottom: 10px;
    display: flex; justify-content: space-between; align-items: center;
  }
  .sec-h .title-dot {
    display: inline-block; width: 8px; height: 8px; border-radius: 50%;
    margin-right: 6px; flex-shrink: 0;
  }
  .sec-h .more { color: var(--amber); cursor: pointer; font-size: 13px; font-weight: 600; }
  .split2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .scell {
    background: rgba(255,255,255,.025); border: 1px solid var(--border-dim);
    border-radius: 9px; padding: 10px 11px;
  }
  .scell .k { font-size: 11px; color: var(--text-faint); }
  .scell .v {
    font-size: 20px; font-weight: 500; color: var(--text); margin-top: 2px;
    display: flex; align-items: baseline; gap: 2px;
  }
  .scell .v .u { font-size: 11px; color: var(--text-faint); font-weight: 600; }
  .v.amber { color: var(--amber) !important; }
  .v.lime { color: var(--lime) !important; }
  .v.coral { color: var(--coral) !important; }
  .crow {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 0; border-bottom: 1px dashed var(--border-dim); cursor: pointer;
  }
  .crow:last-child { border-bottom: none; }
  .crow:hover .nm { color: var(--amber); }
  .crow .rk {
    width: 24px; height: 24px; border-radius: 6px;
    display: flex; align-items: center; justify-content: center;
    font-size: 11px; font-weight: 700; flex-shrink: 0;
    font-style: normal;
  }
  .crow .nm {
    font-size: 13px; flex: 1; color: var(--text); transition: .15s;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .nm .sub {
    display: block; font-size: 11px; color: var(--text-faint); margin-top: 1px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .nm .mvendor {
    font-size: 10px; font-weight: 600; margin-left: 6px;
    padding: 1px 5px; border-radius: 4px;
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
  }
  .crow .su { font-size: 9px; margin-left: 2px; }
  .crow .br {
    width: 62px; height: 4px; background: var(--bar-track);
    border-radius: 2px; overflow: hidden; flex-shrink: 0;
  }
  .crow .br i { display: block; height: 100%; }
  .crow .pct-label {
    font-family: var(--font-mono); font-size: 12px; color: var(--text-faint);
    width: 42px; text-align: right; flex-shrink: 0;
  }
  .crow .pct-unit { font-size: 8px; margin-left: 2px; }
  .crow .vl {
    font-size: 12px; color: var(--text-dim); width: 50px; text-align: right; flex-shrink: 0;
  }
  .crow .vlu { font-size: 8px; color: var(--text-faint); font-weight: 600; margin-left: 2px; }
  .qcard-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  /* ── cookie bar action buttons (shared with Limits.svelte) ── */
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

  /* ── cookie inline edit (same as Limits.svelte) ── */
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

  /* ── cookie hint text ── */
  .qcookie-hint {
    font-size: 10.5px;
    color: var(--text-faint);
    margin: 6px 0 0;
    line-height: 1.6;
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
    max-width: 50%;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.7;
  }
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
  }
  .qiw-bar-wrap {
    flex: 1;
    display: flex;
    justify-content: center;
    padding: 0 8px;
  }
  .qiw-bar {
    width: 100%;
    max-width: 100%;
    height: 6px;
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
  .empty { margin: 0; font-size: 12px; color: var(--text-faint); }
  .muted { margin: 0; color: var(--text-dim); font-size: 13px; }
</style>
