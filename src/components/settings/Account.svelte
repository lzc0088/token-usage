<script lang="ts">
  // 账号额度: 折叠面板式账号绑定 + 额度查询.
  import { listen } from "@tauri-apps/api/event";
  import { api, type Config } from "../../lib/api";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { VENDOR_PANEL } from "../../lib/meta/panels";
  import { VENDORS, fieldsFor, CAT_ORDER, GROUPS, type VendorDef } from "../../lib/meta/vendors";
  import DeviceFlow from "./DeviceFlow.svelte";

  function resolvePanelUrl(id: string): string {
    const panel = VENDOR_PANEL[id];
    if (!panel) return "";
    if (typeof panel.url === "string") return panel.url;
    const val = getField(id, panel.url.field);
    return panel.url.map[val] ?? Object.values(panel.url.map)[0] ?? "";
  }
  function openKeyUrl(id: string): void {
    const url = resolvePanelUrl(id);
    if (url) api.openExternal(url).catch(() => {});
  }

  let bound = $state<Record<string, boolean>>({});
  let expanded = $state<Set<string>>(new Set());
  let inputs = $state<Record<string, Record<string, string>>>({});
  let config = $state<Config | null>(null);
  let saveError = $state<Record<string, string>>({});
  let saving = $state<Record<string, boolean>>({});
  let ordered = $state<string[]>([]);
  let active = $state<Set<string>>(new Set());

  // Derived: vendors sorted by category order (subscription → api-key → cookie).
  const sortedVendors = $derived([...VENDORS].sort((a, b) => CAT_ORDER[a.cat] - CAT_ORDER[b.cat]));

  // Load config and restore vendor order + active state.
  $effect(() => {
    api.getConfig().then(c => {
      config = c;
      // Restore custom order, appending any new vendors not in the saved list.
      const saved = c?.quota_vendor_order;
      if (saved && saved.length > 0) {
        const savedSet = new Set(saved);
        ordered = [...saved, ...VENDORS.map(v => v.id).filter(id => !savedSet.has(id))];
      } else {
        ordered = VENDORS.map(v => v.id);
      }
      if (c?.quota_active_vendors && c.quota_active_vendors.length > 0) {
        active = new Set(c.quota_active_vendors);
      }
    }).catch(() => {
      ordered = VENDORS.map(v => v.id);
    });
  });

  /** Persist the current vendor order to config. */
  function saveOrder(): void {
    if (ordered.length > 0) {
      const order = [...ordered];
      updateConfig({ quota_vendor_order: order });
    }
  }

  /** Map category to display label. */
  function authTypeLabel(cat: string): string {
    if (cat === "subscription") return "OAuth";
    if (cat === "api-key") return "API Key";
    if (cat === "cookie") return "Cookie";
    return cat;
  }

  /** Auth-type-aware right-side status badge(s) for an account row.
   *  subscription (OAuth): 未登录 / 已登录
   *  api-key / cookie:     未设定 / 已连接 (+ Cookie 无效 when stale) */
  type BadgeKind = "ok" | "dim" | "warn";
  function accountBadges(
    v: VendorDef,
    isBound: boolean,
    cookieErr: boolean,
  ): Array<{ label: string; kind: BadgeKind }> {
    const isOAuth = v.authType === "detect" || v.authType === "login" || v.id === "claude";
    if (!isBound) {
      return [{ label: isOAuth ? "未登录" : "未设定", kind: "dim" }];
    }
    const connected = isOAuth ? "已登录" : "已连接";
    if (cookieErr) {
      // Bound but cookie stale — keep the connection hint greyed + flag the error.
      return [
        { label: connected, kind: "dim" },
        { label: "Cookie 无效", kind: "warn" },
      ];
    }
    return [{ label: connected, kind: "ok" }];
  }

  /** "Clear" button label reflects the vendor's actual credential type:
   * subscription → 清除登录; pure cookie → 清除 Cookie; pure API Key → 清除 API Key;
   * mixed (key+cookie like Volcengine, or key+ids like GLM Team) → 清除凭证. */
  function clearButtonLabel(v: VendorDef): string {
    const hasCookie = fieldsFor(v).some((f) => f.key === "cookie");
    if (v.cat === "subscription") return "清除登录";
    if (v.cat === "cookie") return hasCookie ? "清除 Cookie" : "清除凭证";
    // cat === "api-key"
    return hasCookie ? "清除凭证" : "清除 API Key";
  }

  interface ClearAction { label: string; fields?: string[]; all?: boolean; }

  /** Per-vendor clear buttons. Mixed vendors (defined with BOTH key + cookie
   * fields, e.g. Volcengine) get one button per FILLED field group so the user
   * can clear/update Cookie and API Key independently. Single-type vendors get
   * one whole-credential clear. */
  function clearActions(v: VendorDef): ClearAction[] {
    const def = fieldsFor(v);
    const defHasKey = def.some((f) => f.key === "key");
    const defHasCookie = def.some((f) => f.key === "cookie");
    if (!(defHasKey && defHasCookie)) {
      return [{ label: clearButtonLabel(v), all: true }];
    }
    const filled = credFields[v.id] ?? [];
    const acts: ClearAction[] = [];
    if (filled.includes("key")) {
      const fields = def.some((f) => f.key === "secret") ? ["key", "secret"] : ["key"];
      acts.push({ label: "清除 API Key", fields });
    }
    if (filled.includes("cookie")) {
      acts.push({ label: "清除 Cookie", fields: ["cookie"] });
    }
    if (acts.length === 0) acts.push({ label: clearButtonLabel(v), all: true });
    return acts;
  }

  /** Execute a clear action: whole-credential remove, or partial field clear. */
  async function doClear(vendor: string, act: ClearAction): Promise<void> {
    if (act.all) {
      await remove(vendor);
      return;
    }
    try {
      await api.clearCredentialFields(vendor, act.fields ?? []);
      const fields = await api.getCredentialFields(vendor);
      credFields = { ...credFields, [vendor]: fields };
      bound = { ...bound, [vendor]: fields.length > 0 };
      void loadQuotaErrors();
    } catch (e) {
      /* clear fields failed */
    }
  }

  // ── Inline cookie update for mixed vendors (Volcengine key + cookie) ──
  // The cookie can be refreshed independently of the API Key/AK+SK.
  let editingCookieVendor = $state<string | null>(null);
  let cookieDraft = $state("");
  let cookieSaving = $state(false);

  /** True when a vendor's credential defines BOTH key + cookie fields — the
   *  cookie is an optional add-on (e.g. Volcengine expiry) manageable alone. */
  function isMixedVendor(v: VendorDef): boolean {
    const def = fieldsFor(v);
    return def.some((f) => f.key === "key") && def.some((f) => f.key === "cookie");
  }
  function startEditCookie(vendor: string): void {
    editingCookieVendor = vendor;
    cookieDraft = "";
  }
  function cancelEditCookie(): void {
    editingCookieVendor = null;
    cookieDraft = "";
  }
  /** Persist a new cookie (preserving key/secret) + reload fields + hints. */
  async function saveCookie(vendor: string): Promise<void> {
    const draft = cookieDraft.trim();
    if (!draft) return;
    cookieSaving = true;
    try {
      await api.updateCookie(vendor, draft);
      const fields = await api.getCredentialFields(vendor);
      credFields = { ...credFields, [vendor]: fields };
      bound = { ...bound, [vendor]: fields.length > 0 };
      editingCookieVendor = null;
      cookieDraft = "";
      void loadQuotaErrors();
    } catch (e) {
      saveError = { ...saveError, [vendor]: e instanceof Error ? e.message : String(e) };
    } finally {
      cookieSaving = false;
    }
  }
  function move(i: number, dir: -1 | 1): void {
    const j = i + dir;
    if (j < 0 || j >= ordered.length) return;
    const arr = [...ordered];
    [arr[i], arr[j]] = [arr[j]!, arr[i]!];
    ordered = arr;
    saveOrder();
  }

  // Per-vendor filled credential fields (e.g. ["key","secret","cookie"]).
  let credFields = $state<Record<string, string[]>>({});
  $effect(() => {
    let cancelled = false;
    (async () => {
      const bmap: Record<string, boolean> = {};
      const fmap: Record<string, string[]> = {};
      for (const v of VENDORS) {
        if (cancelled) return;
        try {
          const fields = await api.getCredentialFields(v.id);
          fmap[v.id] = fields;
          bmap[v.id] = fields.length > 0;
        } catch {
          fmap[v.id] = [];
          bmap[v.id] = false;
        }
      }
      if (!cancelled) {
        bound = bmap;
        credFields = fmap;
      }
    })();
    return () => { cancelled = true; };
  });

  function toggle(id: string): void {
    // Accordion: only one vendor panel open at a time.
    if (expanded.has(id)) {
      expanded = new Set();
    } else {
      expanded = new Set([id]);
    }
  }

  function getField(id: string, fieldKey: string): string {
    const vendor = VENDORS.find(x => x.id === id);
    const field = vendor ? fieldsFor(vendor).find(f => f.key === fieldKey) : undefined;
    return inputs[id]?.[fieldKey] ?? field?.default ?? "";
  }
  function setField(id: string, fieldKey: string, val: string): void {
    inputs = { ...inputs, [id]: { ...(inputs[id] ?? {}), [fieldKey]: val } };
  }

  async function save(vendor: string): Promise<void> {
    const fields = fieldsFor(VENDORS.find(x => x.id === vendor)!);
    const values = fields.map(f => getField(vendor, f.key)).filter(Boolean);
    if (values.length === 0) return;
    if (saving[vendor]) return;
    saving = { ...saving, [vendor]: true };
    saveError = { ...saveError, [vendor]: "" };
    // 序列化为 JSON 字符串存入 keyring（后端按厂商解析）
    const payload = JSON.stringify(
      Object.fromEntries(fields.map(f => [f.key, getField(vendor, f.key)]))
    );
    try {
      await api.setCredential(vendor, payload);
      bound = { ...bound, [vendor]: true };
      // Reload authoritative field list from the backend (handles multi-field
      // vendors), rather than inferring from the just-typed inputs.
      try {
        credFields = { ...credFields, [vendor]: await api.getCredentialFields(vendor) };
      } catch {
        credFields = { ...credFields, [vendor]: fields.map((f) => f.key).filter((k) => getField(vendor, k) !== "") };
      }
      inputs = { ...inputs, [vendor]: {} };
      saveError = { ...saveError, [vendor]: "" };
      // Immediately refresh this vendor's quota so the 额度 page and 总览
      // reflect the newly-bound credential without waiting for the scheduler.
      // Failures are non-fatal — the scheduler will retry on its next tick.
      api.refreshQuota(vendor).catch(() => {});
      void loadQuotaErrors();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      saveError = { ...saveError, [vendor]: msg };
    } finally {
      saving = { ...saving, [vendor]: false };
    }
  }
  async function remove(vendor: string): Promise<void> {
    try {
      await api.deleteCredential(vendor);
      bound = { ...bound, [vendor]: false };
      credFields = { ...credFields, [vendor]: [] };
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      saveError = { ...saveError, [vendor]: msg };
    }
  }

  // Per-vendor cookie-expired hint (sourced from the quota cache's cookie_error).
  let cookieErrorOf = $state<Record<string, string>>({});
  async function loadQuotaErrors(): Promise<void> {
    try {
      const qs = await api.getQuotas();
      const map: Record<string, string> = {};
      for (const q of qs) {
        if (q.cookie_error) map[q.vendor] = q.cookie_error;
      }
      cookieErrorOf = map;
    } catch {
      /* ignore — cookie hints are best-effort */
    }
  }
  $effect(() => {
    void loadQuotaErrors();
    const un = listen<void>("quota:updated", () => void loadQuotaErrors());
    return () => {
      un.then((u) => u());
    };
  });

  // Per-vendor refresh state: "loading" | "ok" | "fail" + message.
  let refreshState = $state<Record<string, { status: "loading" | "ok" | "fail"; msg?: string }>>({});

  // Track pending timeouts so they can be cleared on unmount to prevent
  // state updates on a destroyed component.
  let timeouts = $state<Set<number>>(new Set());
  $effect(() => {
    return () => {
      for (const id of timeouts) clearTimeout(id);
    };
  });

  async function refreshQuota(vendor: string): Promise<void> {
    refreshState = { ...refreshState, [vendor]: { status: "loading" } };
    try {
      await api.refreshQuota(vendor);
      refreshState = { ...refreshState, [vendor]: { status: "ok", msg: "刷新成功" } };
      void loadQuotaErrors();
      const id = window.setTimeout(() => {
        timeouts.delete(id);
        const next = { ...refreshState };
        delete next[vendor];
        refreshState = next;
      }, 3000);
      timeouts.add(id);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      refreshState = { ...refreshState, [vendor]: { status: "fail", msg } };
    }
  }

  // Dispatch login by vendor id. Returns true if a login was started.
  async function startLogin(vendor: string): Promise<boolean> {
    if (vendor === "claude") {
      await api.refreshQuota(vendor).then(() => {
        bound = { ...bound, [vendor]: true };
      }).catch(() => {
        /* no CLI credentials found — expected */
      });
      return true;
    }
    return false;
  }

  async function updateConfig(partial: Partial<Config>): Promise<void> {
    if (!config) return;
    const next = { ...config, ...partial };
    try {
      await api.setConfig(next);
      config = next;
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      saveError = { ...saveError, _global: msg };
    }
  }
</script>

<div class="sh"><h3>账号额度</h3><div class="desc">厂商账号绑定与额度查询 · 点击展开配置</div></div>
<div class="sc">

  <!-- ══ 账号 ══ -->
  <div class="section-title">
    账号
    <span class="title-stat">{Object.values(bound).filter(Boolean).length} / {VENDORS.length} 已连接</span>
  </div>

  {#each GROUPS as g}
    {@const items = sortedVendors.filter(v => v.cat === g.cat)}
    {#if items.length > 0}
      <div class="section-box" style="margin-top:8px">
        <div class="group-head">{g.label} <span class="group-count">{items.length}</span></div>
        {#each items as v, rowIdx (v.id)}
          {@const fs = fieldsFor(v)}
          <!-- 主行（可点击展开） -->
          <button type="button" class="arow {rowIdx === items.length - 1 ? 'arow-last' : ''}" class:open={expanded.has(v.id)} onclick={() => toggle(v.id)}>
            <ToolIcon vendor={v.id} size={22} />
            <span class="ainfo">
              <span class="aname">{v.label}</span>
              <span class="atags">
                {#each v.tags as t (t.text)}
                  <span class="itag c-{t.color}">{t.text}</span>
                {/each}
              </span>
            </span>
            <span class="astate">
              {#each accountBadges(v, !!bound[v.id], !!cookieErrorOf[v.id]) as b (b.label)}
                <span class="badge s-{b.kind}">{b.label}</span>
              {/each}
            </span>
            <span class="chev">{expanded.has(v.id) ? "▾" : "▸"}</span>
          </button>

          <!-- 展开配置面板 -->
          {#if expanded.has(v.id)}
            <div class="panel {rowIdx === items.length - 1 ? 'panel-last' : ''}">
              {#if bound[v.id]}
                {#if cookieErrorOf[v.id]}
                  <p class="panel-warn">⚠ {cookieErrorOf[v.id]}，请重新填写并保存</p>
                  {#if VENDOR_PANEL[v.id]}
                    <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>在浏览器打开 {VENDOR_PANEL[v.id].pageLabel} 页面</button>
                    <p class="panel-note">{VENDOR_PANEL[v.id].hint}</p>
                  {/if}
                {:else if v.id === "stepfun"}
                  <p class="panel-note" style="margin:0 0 8px">💡 Cookie 有效期较短，过期后需重新从浏览器获取。如遇额度刷新失败，请更新 Cookie。</p>
                {/if}
                <p class="panel-hint">
                  {#if v.authType === "detect" || v.authType === "login"}
                    账号已绑定，可清除凭证后重新绑定，或刷新额度数据。
                  {:else}
                    账号已绑定，可清除后重新填写，或刷新额度数据。
                  {/if}
                </p>
                {#if isMixedVendor(v)}
                  <div class="cookie-mgr">
                    {#if editingCookieVendor === v.id}
                      <textarea class="finp finp-textarea" bind:value={cookieDraft} placeholder="粘贴新 Cookie…" rows="3" disabled={cookieSaving}></textarea>
                      <div class="cookie-mgr-actions">
                        <button type="button" class="btn-primary" onclick={() => saveCookie(v.id)} disabled={cookieSaving || !cookieDraft.trim()}>
                          {cookieSaving ? "检查中…" : "保存 Cookie"}
                        </button>
                        <button type="button" class="btn-outline" onclick={cancelEditCookie} disabled={cookieSaving}>取消</button>
                      </div>
                    {:else}
                      <div class="cookie-mgr-row">
                        <span class="cookie-mgr-status">
                          {#if cookieErrorOf[v.id] && (credFields[v.id] ?? []).includes("cookie")}
                            <span class="cs-err">⚠ Cookie 已失效</span>
                          {:else if (credFields[v.id] ?? []).includes("cookie")}
                            <span class="cs-ok">✓ Cookie 已绑定（显示到期日期）</span>
                          {:else}
                            <span class="cs-none">Cookie 未绑定（可选，用于显示到期日期）</span>
                          {/if}
                        </span>
                        <button type="button" class="btn-outline" onclick={() => startEditCookie(v.id)}>
                          {(credFields[v.id] ?? []).includes("cookie") ? "更新 Cookie" : "添加 Cookie"}
                        </button>
                      </div>
                    {/if}
                  </div>
                {/if}
                <div class="panel-actions">
                  {#each clearActions(v) as act (act.label)}
                    <button type="button" class="btn-outline" onclick={() => doClear(v.id, act)}>{act.label}</button>
                  {/each}
                  <button
                    class="btn-primary"
                    disabled={refreshState[v.id]?.status === "loading"}
                    onclick={() => refreshQuota(v.id)}
                  >{refreshState[v.id]?.status === "loading" ? "刷新中…" : "刷新"}</button>
                  {#if refreshState[v.id]?.status === "ok"}
                    <span class="refresh-msg ok">{refreshState[v.id].msg}</span>
                  {:else if refreshState[v.id]?.status === "fail"}
                    <span class="refresh-msg fail">刷新失败：{refreshState[v.id].msg}</span>
                  {/if}
                </div>
              {:else if v.authType === "detect" || v.authType === "login"}
                <p class="panel-hint">
                  {#if v.authType === "detect"}
                    自动检测本机已安装的 CLI 凭证，无需手动填写。如未检测到，可打开对应客户端登录后重新检测。
                  {:else}
                    通过浏览器 OAuth 授权完成登录，授权后凭证自动保存到本机。
                  {/if}
                </p>
                {#if v.id === "claude" && VENDOR_PANEL.claude}
                  <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>在浏览器打开 {VENDOR_PANEL.claude.pageLabel} 页面</button>
                  <p class="panel-note">{VENDOR_PANEL.claude.hint}</p>
                  <div class="fields">
                    {#each fieldsFor(v) as f (f.key)}
                      <label class="field">
                        <span class="flabel">{f.label}</span>
                        {#if f.type === "textarea"}
                          <textarea class="finp finp-textarea" placeholder={f.placeholder} rows="2" oninput={(e) => setField(v.id, f.key, (e.target as HTMLTextAreaElement).value)}>{getField(v.id, f.key)}</textarea>
                        {/if}
                      </label>
                    {/each}
                  </div>
                  {#if saveError[v.id]}
                    <p class="save-err">{saveError[v.id]}</p>
                  {/if}
                  <div class="panel-actions">
                    <button type="button" class="btn-outline" onclick={() => toggle(v.id)} disabled={saving[v.id]}>取消</button>
                    <button type="button" class="btn-primary" onclick={() => save(v.id)} disabled={saving[v.id]}>
                      {saving[v.id] ? "检查中…" : "保存"}
                    </button>
                  </div>
                {:else if (v.id === "copilot" || v.id === "codex")}
                  <DeviceFlow />
                {:else}
                  <div class="panel-actions">
                    {#if v.authType === "detect"}
                      <button type="button" class="btn-outline" onclick={() => startLogin(v.id)}>立即检测</button>
                    {/if}
                    <button type="button" class="btn-primary" onclick={() => startLogin(v.id)}>{v.loginLabel ?? "登录"}</button>
                  </div>
                {/if}
              {:else}
                {#if VENDOR_PANEL[v.id]}
                  <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>在浏览器打开 {VENDOR_PANEL[v.id].pageLabel} 页面</button>
                  {#if VENDOR_PANEL[v.id].extraUrl && VENDOR_PANEL[v.id].extraLabel}
                    <button type="button" class="btn-open" onclick={() => api.openExternal(VENDOR_PANEL[v.id].extraUrl!).catch(() => {})}>在浏览器打开 {VENDOR_PANEL[v.id].extraLabel} 页面</button>
                  {/if}
                  <p class="panel-note">{VENDOR_PANEL[v.id].hint}</p>
                {:else}
                  <p class="panel-note">
                    {#if v.authType === "cookie"}
                      从浏览器控制台复制对应 Cookie，粘贴到下方。Cookie 仅保存在本机，不上传任何服务器。
                    {:else}
                      在对应厂商控制台获取 API Key，填入下方。Key 仅保存在本机，不上传任何服务器。
                    {/if}
                  </p>
                {/if}
                <div class="fields">
                  {#each fs as f (f.key)}
                    <label class="field">
                      <span class="flabel">{f.label}</span>
                      {#if f.type === "select"}
                        <select class="fsel" value={getField(v.id, f.key)} onchange={(e) => setField(v.id, f.key, (e.target as HTMLSelectElement).value)}>
                          {#each f.options ?? [] as opt (opt)}
                            <option value={opt}>{opt}</option>
                          {/each}
                        </select>
                      {:else if f.type === "textarea"}
                        <textarea class="finp finp-textarea" placeholder={f.placeholder} rows="3" oninput={(e) => setField(v.id, f.key, (e.target as HTMLTextAreaElement).value)}>{getField(v.id, f.key)}</textarea>
                      {:else}
                        <input class="finp" type={f.type ?? "text"} placeholder={f.placeholder} value={getField(v.id, f.key)} oninput={(e) => setField(v.id, f.key, (e.target as HTMLInputElement).value)} />
                      {/if}
                    </label>
                  {/each}
                </div>
                <div class="panel-actions">
                  <button type="button" class="btn-outline" onclick={() => toggle(v.id)} disabled={saving[v.id]}>取消</button>
                  <button type="button" class="btn-primary" onclick={() => save(v.id)} disabled={saving[v.id]}>
                    {saving[v.id] ? "检查中…" : "保存"}
                  </button>
                </div>
                {#if saveError[v.id]}
                  <p class="save-err">{saveError[v.id]}</p>
                {/if}
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  {/each}

  <!-- ══ 额度 ══ -->
  <div class="section-title">
    额度
    <span class="title-stat">{active.size} / {VENDORS.length} 已启用</span>
  </div>

  <div class="section-box" style="margin-top:8px">
    <div class="group-head">全局设置</div>
    <div class="box-row">
      <div class="lab">刷新频率<div class="hint">额度数据刷新间隔</div></div>
      <select class="sel" value={config?.quota_refresh_interval ?? "5m"}
        onchange={(e) => updateConfig({ quota_refresh_interval: (e.target as HTMLSelectElement).value as Config["quota_refresh_interval"] })}>
        <option value="1m">1 分钟</option>
        <option value="3m">3 分钟</option>
        <option value="5m">5 分钟</option>
        <option value="10m">10 分钟</option>
        <option value="15m">15 分钟</option>
      </select>
    </div>
    <div class="box-row">
      <div class="lab">进度显示<div class="hint">进度条与百分比显示方式</div></div>
      <select class="sel" value={config?.quota_progress_mode ?? "剩余"}
        onchange={(e) => updateConfig({ quota_progress_mode: (e.target as HTMLSelectElement).value as Config["quota_progress_mode"] })}>
        <option value="用量">用量</option>
        <option value="剩余">剩余</option>
      </select>
    </div>
  </div>

  <div class="section-box" style="margin-top:12px">
    <div class="group-head">厂商管理</div>
    <div class="icon-legend">
      <span class="legend-item">启用</span>
      <span class="legend-item">上移</span>
      <span class="legend-item">下移</span>
    </div>
    {#each ordered as id, i (id)}
      {@const v = VENDORS.find(x => x.id === id)}
      {#if v}
        <div class="trow">
          <ToolIcon vendor={id} size={22} />
          <span class="tleft">
            <span class="tname">{v.label}</span>
            <span class="ttags">
              {#if active.has(id)}
                <span class="ttag" class:tt-active={bound[v.id]} class:tt-unconfig={!bound[v.id]}>
                  {bound[v.id] ? "已检出" : "未配置"}
                </span>
              {:else}
                <span class="ttag tt-inactive">已停用</span>
              {/if}
              {#each v.billing as b (b)}
                <span class="ttag tt-billing">{b}</span>
              {/each}
              <span class="ttag tt-auth-{v.cat}">{authTypeLabel(v.cat)}</span>
            </span>
          </span>
          <span class="tright">
            <!-- 启用 toggle：选中→验证绑定状态，未选中→已停用（状态持久化） -->
            <button class="ibtn ibtn-toggle" class:on={active.has(id)} title={active.has(id) ? '已启用' : '已停用'} aria-label={active.has(id) ? '停用' : '启用'}
              onclick={() => {
                const next = new Set(active);
                if (active.has(id)) next.delete(id); else next.add(id);
                active = next;
                updateConfig({
                  quota_active_vendors: ordered.filter(x => next.has(x)),
                  quota_vendor_order: [...ordered],
                });
              }}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                {#if active.has(id)}
                  <rect x="3" y="3" width="18" height="18" rx="4"/><polyline points="9 12 11 14 16 8"/>
                {:else}
                  <rect x="3" y="3" width="18" height="18" rx="4"/>
                {/if}
              </svg>
            </button>
            <!-- 上移 -->
            <button type="button" class="ibtn" title="上移" aria-label="上移" disabled={i === 0} onclick={() => move(i, -1)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
              </svg>
            </button>
            <!-- 下移 -->
            <button type="button" class="ibtn" title="下移" aria-label="下移" disabled={i === ordered.length - 1} onclick={() => move(i, 1)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>
              </svg>
            </button>
          </span>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>

  .sc { display: flex; flex-direction: column; }

  /* section-title override: larger + flex layout for stat badge */
  .section-title {
    font-size: 15px;
    margin-bottom: 2px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .title-stat {
    font-size: 11px;
    font-weight: 500;
    color: var(--lime);
    background: var(--lime-bg-soft);
    padding: 2px 9px;
    border-radius: 5px;
  }

  /* ── account row（可点击） ── */
  .arow {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 4px;
    background: none;
    border: none;
    border-bottom: 1px dashed var(--border);
    cursor: pointer;
    text-align: left;
    font-family: inherit;
    transition: background 0.12s;
    border-radius: 4px;
  }
  .arow:hover { background: var(--surface-tint); }
  .arow-last { border-bottom: none !important; }
  .panel-last { border-bottom: none !important; }
  .arow.open { border-bottom: none; }

  .ainfo { flex: 1; min-width: 0; }
  .aname { font-size: 13px; color: var(--text); display: block; }
  .atags { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; margin-top: 3px; }

  /* ── info tags（厂商特性标签，6 色）── */
  .itag {
    font-size: 9.5px;
    font-weight: 500;
    padding: 1px 5px;
    border-radius: 3px;
    line-height: 1.5;
    white-space: nowrap;
  }
  .itag.c-blue   { background: rgba(79,195,247,0.14); color: var(--cyan-text); }
  .itag.c-amber  { background: rgba(232,176,75,0.14); color: var(--amber); }
  .itag.c-purple { background: rgba(179,136,255,0.14); color: var(--violet-text); }
  .itag.c-lime   { background: rgba(108,199,116,0.14); color: var(--lime); }
  .itag.c-coral  { background: rgba(224,108,117,0.14); color: var(--coral); }
  .itag.c-gray   { background: var(--surface-tint-strong); color: var(--text-faint); }

  .astate { flex-shrink: 0; min-width: 50px; display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
  .badge { font-size: 10.5px; font-weight: 500; padding: 2px 7px; border-radius: 5px; }
  .badge.s-ok  { color: var(--lime); background: var(--lime-bg); }
  .badge.s-dim { color: var(--text-faint); background: var(--surface-tint); }
  .badge.s-warn { color: var(--coral); background: rgba(234,84,85,0.14); }

  .chev { color: var(--text-dim); font-size: 14px; flex-shrink: 0; width: 18px; text-align: center; transition: transform 0.15s; }
  .arow.open .chev { color: var(--amber); }

  /* ── 展开面板 ── */
  .panel {
    padding: 4px 4px 10px 40px;
    margin-bottom: 6px;
    border-bottom: 1px dashed var(--border);
  }
  .panel-warn {
    font-size: 11.5px;
    color: var(--coral);
    background: rgba(234,84,85,0.10);
    border: 1px solid var(--coral-border);
    padding: 6px 9px;
    border-radius: 6px;
    margin: 0 0 8px;
    line-height: 1.5;
  }
  .cookie-mgr {
    margin: 0 0 10px;
    padding: 8px 9px;
    border: 1px dashed var(--border-dim);
    border-radius: 6px;
    background: var(--surface-tint);
  }
  /* Match the full-width inputs of the key/secret/cookie fields above — the
     textarea lives directly in .cookie-mgr (no flex parent to stretch it), so
     width must be explicit. */
  .cookie-mgr textarea {
    width: 100%;
    box-sizing: border-box;
  }
  .cookie-mgr-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .cookie-mgr-status { font-size: 11px; min-width: 0; flex: 1; line-height: 1.5; }
  .cs-ok { color: var(--lime); }
  .cs-err { color: var(--coral); }
  .cs-none { color: var(--text-faint); }
  .cookie-mgr-actions { display: flex; gap: 6px; margin-top: 6px; }
  .panel-hint { font-size: 11px; color: var(--text-faint); margin: 4px 0 10px; line-height: 1.6; }

  /* ── Copilot device-flow login ── */
  .device-flow {
    padding: 10px 12px;
    margin: 0 0 10px;
    border: 1px dashed var(--border-dim);
    border-radius: 8px;
    background: var(--surface-tint);
  }
  .df-status { font-size: 12px; color: var(--text); margin: 0 0 8px; line-height: 1.5; }
  .df-status.df-ok { color: var(--lime); margin: 0; }
  .df-status.df-err { color: var(--coral); margin: 0; }
  .df-code {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 4px;
    color: var(--amber);
    text-align: center;
    padding: 8px 0;
    background: var(--amber-hover);
    border-radius: 6px;
    margin-bottom: 8px;
    user-select: all;
  }
  .df-hint { font-size: 11px; color: var(--text-faint); margin: 0 0 6px; }
  .df-link {
    background: none;
    border: none;
    color: var(--amber);
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    padding: 0;
    text-decoration: underline;
  }
  .df-polling {
    font-size: 11px;
    color: var(--text-dim);
    margin: 0;
    animation: df-pulse 1.4s ease-in-out infinite;
  }
  @keyframes df-pulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }

  .fields { display: flex; flex-direction: column; gap: 8px; margin-bottom: 8px; }
  .field { display: flex; flex-direction: column; gap: 3px; }
  .flabel { font-size: 10.5px; color: var(--text-faint); }
  .finp {
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    color: var(--text);
    padding: 6px 10px;
    border-radius: 7px;
    font-family: var(--font-mono);
    font-size: 11.5px;
    height: 32px;
    box-sizing: border-box;
  }
  .finp:focus { outline: none; border-color: var(--amber); }
  .finp-textarea {
    height: auto;
    min-height: 60px;
    resize: vertical;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .fsel {
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    color: var(--text);
    padding: 6px 10px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 12px;
    height: 32px;
    box-sizing: border-box;
  }
  .fsel:focus { outline: none; border-color: var(--amber); }

  .panel-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .ok-text { font-size: 11px; color: var(--lime); }

  .btn-primary {
    background: var(--amber);
    border: none;
    color: var(--badge-text);
    padding: 5px 14px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .btn-primary:hover { opacity: 0.88; }
  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-primary:disabled:hover { opacity: 0.5; }
  .btn-outline:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-outline:disabled:hover {
    border-color: var(--border-dim);
    color: var(--text-dim);
    background: var(--surface-tint-strong);
  }

  .btn-open {
    display: inline-flex;
    align-items: center;
    background: var(--surface-tint-strong);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 6px 12px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 11.5px;
    cursor: pointer;
    margin-bottom: 6px;
    transition: all 0.15s;
  }
  .btn-open:hover { border-color: var(--amber); color: var(--amber); background: var(--amber-hover); }

  .cookie-steps {
    margin: 8px 0;
    padding: 8px 10px;
    background: var(--surface-tint);
    border: 1px solid var(--surface-tint-strong);
    border-radius: 7px;
  }
  .cs-title { font-size: 11px; font-weight: 600; color: var(--text); margin-bottom: 4px; }

  .panel-note { font-size: 11px; color: var(--text-faint); margin: 0 0 10px; line-height: 1.6; }
  .save-err { font-size: 11px; color: var(--coral); margin: 6px 0 0; line-height: 1.5; }
  .refresh-msg { font-size: 11px; line-height: 1.5; align-self: center; }
  .refresh-msg.ok { color: var(--lime); }
  .refresh-msg.fail { color: var(--coral); word-break: break-word; }

  /* ── account group header ── */
  .group-head {
    font-family: var(--font-ui);
    font-size: 12px;
    font-weight: 700;
    color: var(--amber);
    letter-spacing: 0.03em;
    margin: 14px 0 6px;
  }
  .group-head:first-of-type { margin-top: 4px; }
  .group-count {
    font-size: 10px;
    font-weight: 500;
    color: var(--text-faint);
    background: var(--surface-tint-strong);
    padding: 1px 7px;
    border-radius: 4px;
    vertical-align: middle;
  }

  .stat-group { display: flex; align-items: center; gap: 6px; }
  .stat {
    font-family: var(--font-ui);
    font-size: 10.5px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--surface-tint-strong);
    color: var(--text-dim);
  }
  .stat.s-on { background: rgba(108,199,116,0.1); color: var(--lime); }

  /* ── vendor row (matching Collection.svelte trow) ── */
  .trow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    border-bottom: 1px dashed var(--border);
    gap: 12px;
  }
  .trow:last-child { border-bottom: none; }

  .tleft {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .tname { font-size: 13px; color: var(--text); font-weight: 500; }
  .ttags { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; }
  .ttag {
    font-size: 10px;
    font-weight: 500;
    padding: 1px 7px;
    border-radius: 4px;
    line-height: 1.6;
  }
  .ttag.tt-active  { background: var(--lime-bg); color: var(--lime); }
  .ttag.tt-unconfig { background: rgba(232,176,75,0.10); color: var(--amber); }
  .ttag.tt-inactive { background: rgba(234,84,85,0.12); color: var(--coral); }
  .ttag.tt-auth-subscription { background: rgba(79,195,247,0.14); color: var(--cyan-text); }
  .ttag.tt-auth-api-key { background: rgba(232,176,75,0.14); color: var(--amber); }
  .ttag.tt-auth-cookie { background: rgba(179,136,255,0.14); color: var(--violet-text); }
  .ttag.tt-billing { background: var(--lime-bg-soft); color: var(--lime); }

  .tright {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── icon legend ── */

  .loading, .empty { font-size: 11px; color: var(--text-faint); padding: 8px 0; }

  .icon-legend {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    margin-top: 0;
    margin-bottom: 0;
    padding: 0;
  }
  .icon-legend .legend-item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    font-size: 10px;
    color: var(--text-faint);
    line-height: 1;
  }
</style>
