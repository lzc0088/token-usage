<script lang="ts">
  // 账号额度: 折叠面板式账号绑定 + 额度查询.
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-shell";
  import { api, type Config } from "../../lib/api";
  import ToolIcon from "../../lib/ToolIcon.svelte";
  import { VENDOR_PANEL } from "../../lib/vendorPanel";

  type AuthType = "detect" | "login" | "key" | "cookie";
  type TagColor = "blue" | "amber" | "purple" | "lime" | "coral" | "gray";
  interface InfoTag { text: string; color: TagColor; }
  interface FieldDef { key: string; label: string; placeholder: string; type?: "text" | "password" | "select" | "textarea"; options?: string[]; default?: string; }
  interface VendorDef {
    id: string; label: string; cat: "subscription" | "api-key" | "cookie";
    // A vendor may support multiple billing types (e.g. Kimi: 按量 + Token Plan).
    billing: string[];  // 按量, Token Plan, Coding Plan, Team Plan, 订阅
    authType: AuthType; desc: string;
    tags: InfoTag[];
    fields?: FieldDef[];
    loginLabel?: string;
  }

  const DEFAULT_KEY_FIELD: FieldDef = { key: "key", label: "API Key", placeholder: "sk-…", type: "password" };
  const DEFAULT_COOKIE_FIELD: FieldDef = { key: "cookie", label: "Cookie", placeholder: "粘贴 Cookie…", type: "textarea" };

  function fieldsFor(v: VendorDef): FieldDef[] {
    if (v.fields) return v.fields;
    if (v.authType === "cookie") return [DEFAULT_COOKIE_FIELD];
    return [DEFAULT_KEY_FIELD];
  }
  function resolvePanelUrl(id: string): string {
    const panel = VENDOR_PANEL[id];
    if (!panel) return "";
    if (typeof panel.url === "string") return panel.url;
    const val = getField(id, panel.url.field);
    return panel.url.map[val] ?? Object.values(panel.url.map)[0] ?? "";
  }
  function openKeyUrl(id: string): void {
    const url = resolvePanelUrl(id);
    if (url) open(url).catch(() => {});
  }

  // ── 厂商清单（基于 token-monitor limitCollector.js 实测）──
  const VENDORS: VendorDef[] = [
    // ① 订阅制 — 自动检测 / 跳转 CLI 登录
    { id: "claude",    label: "Claude Code",        cat: "subscription", billing: ["订阅"], authType: "detect",
      desc: "Anthropic 官方订阅，自动检测本机登录状态",
      tags: [{text:"5h 窗口",color:"amber"},{text:"周窗口",color:"amber"}],
      loginLabel: "运行 claude /login" },
    { id: "codex",     label: "Codex",              cat: "subscription", billing: ["订阅"], authType: "login",
      desc: "OpenAI Codex 订阅，OAuth 授权登录，分主 / 次窗口",
      tags: [{text:"5h 窗口",color:"amber"},{text:"周窗口",color:"amber"}],
      loginLabel: "OAuth 登录" },
    { id: "cursor",    label: "Cursor",             cat: "subscription", billing: ["订阅"], authType: "detect",
      desc: "Cursor IDE 订阅，按账单周期统计用量",
      tags: [{text:"账单周期",color:"amber"}],
      loginLabel: "打开 Cursor 登录" },
    // ② API Key — 表单填入
    { id: "deepseek",  label: "DeepSeek ( 深度求索 )",  cat: "api-key", billing: ["按量"], authType: "key",
      desc: "按量付费，查询账户余额",
      tags: [{text:"余额",color:"lime"}] },
    { id: "minimax",   label: "MiniMax ( 稀宇 )",       cat: "api-key", billing: ["Token Plan", "按量"], authType: "key",
      desc: "Coding Plan，需专用 Coding Key，按 Token 额度统计；亦支持按量付费",
      tags: [{text:"Coding Key",color:"coral"},{text:"Token Plan",color:"amber"}] },
    { id: "glm",       label: "GLM ( 智谱 )",           cat: "api-key", billing: ["Coding Plan", "按量"], authType: "key",
      desc: "Coding Plan，区分国际区 / 国内区，三窗口额度；亦支持按量资源包",
      tags: [{text:"区域",color:"purple"},{text:"5h",color:"amber"},{text:"周",color:"amber"},{text:"MCP月",color:"lime"}],
      fields: [
        { key: "key", label: "API Key", placeholder: "ZAI/GLM Key…", type: "password" },
        { key: "region", label: "区域", placeholder: "", type: "select", options: ["global", "bigmodel-cn"], default: "bigmodel-cn" },
      ] },
    { id: "kimi",      label: "Kimi ( 月之暗面 )",      cat: "cookie", billing: ["按量", "Token Plan"], authType: "cookie",
      desc: "从浏览器 Application → Cookies 复制 kimi-auth 值，获取 5h/周/月完整额度",
      tags: [{text:"5h",color:"amber"},{text:"周",color:"amber"},{text:"月",color:"lime"}],
      fields: [
        { key: "cookie", label: "Cookie", placeholder: "粘贴 kimi-auth 的值…", type: "textarea" },
      ] },
    { id: "volcengine",label: "Volcengine ( 火山方舟 )",  cat: "api-key", billing: ["Coding Plan", "按量"], authType: "key",
      desc: "Ark Key 读取流量限制 · 可选 Cookie 显示订阅到期日期",
      tags: [{text:"AK+SK",color:"purple"},{text:"5h",color:"amber"},{text:"周",color:"amber"},{text:"月",color:"lime"},{text:"区域",color:"blue"}],
      fields: [
        { key: "key", label: "Ark Key / AK", placeholder: "ark-… 或 AKLT…", type: "password" },
        { key: "secret", label: "Secret（AK+SK 时需要）", placeholder: "配合 AKLT 使用", type: "password" },
        { key: "region", label: "区域", placeholder: "", type: "select", options: ["cn-beijing"], default: "cn-beijing" },
        { key: "cookie", label: "控制台 Cookie（可选）", placeholder: "粘贴 console.volcengine.com 的 Cookie（含 csrfToken），用于显示到期日期", type: "textarea" },
      ] },
    { id: "stepfun",   label: "StepFun ( 阶跃星辰 )",    cat: "cookie", billing: ["Step Plan", "按量"], authType: "cookie",
      desc: "阶跃星辰 StepFun，粘贴 platform.stepfun.com 控制台 Cookie，查询账户余额与 Step Plan Credit",
      tags: [{text:"Step Plan",color:"amber"},{text:"余额",color:"lime"}],
      fields: [
        { key: "cookie", label: "Cookie", placeholder: "粘贴 platform.stepfun.com 的 Cookie（含 Oasis-Token、Oasis-Webid）…", type: "textarea" },
      ] },
    { id: "iflytek",   label: "iFlytek ( 讯飞星辰 )",        cat: "cookie", billing: ["Token Plan", "按量"], authType: "cookie",
      desc: "讯飞星辰 MaaS（Astron），粘贴控制台 Cookie（含 ssoSessionId），获取 Coding Plan 套餐到期与用量",
      tags: [{text:"Token Plan",color:"amber"},{text:"余额",color:"lime"}],
      fields: [
        { key: "cookie", label: "Cookie", placeholder: "粘贴 maas.xfyun.cn 控制台 Cookie（含 ssoSessionId）…", type: "textarea" },
      ] },
    { id: "copilot",   label: "GitHub Copilot",     cat: "subscription", billing: ["订阅"], authType: "login",
      desc: "GitHub 账号 OAuth 授权",
      tags: [],
      loginLabel: "GitHub 登录" },
    { id: "mimo",      label: "MiMo ( 小米 )",          cat: "cookie", billing: ["Token Plan", "按量"], authType: "cookie",
      desc: "小米 MiMo，粘贴浏览器 Cookie 获取余额与套餐额度，支持 Token Plan 与按量",
      tags: [{text:"余额",color:"lime"},{text:"Token Plan",color:"amber"}] },
    // ③ Cookie — 粘贴
    { id: "opencode",  label: "OpenCode",           cat: "cookie", billing: ["按量"], authType: "cookie",
      desc: "Go / Zen Web 面板，粘贴会话 Cookie",
      tags: [] },
    { id: "zai_team",  label: "GLM Team ( 智谱团队 )",     cat: "cookie", billing: ["Team Plan"], authType: "cookie",
      desc: "智谱团队计划，需 Key + 组织 ID + 项目 ID",
      tags: [{text:"多字段",color:"coral"}],
      fields: [
        { key: "key", label: "Team API Key", placeholder: "Team Key…", type: "password" },
        { key: "orgid", label: "Organization ID", placeholder: "Bigmodel-Organization", type: "text" },
        { key: "projid", label: "Project ID", placeholder: "Bigmodel-Project", type: "text" },
      ] },
    { id: "qoder",     label: "Qoder",              cat: "cookie", billing: ["按量"], authType: "cookie",
      desc: "仪表盘 Cookie，区分国际站 / 中国站",
      tags: [{text:"区域",color:"amber"}],
      fields: [
        { key: "cookie", label: "Cookie", placeholder: "粘贴仪表盘 Cookie…", type: "text" },
        { key: "site", label: "站点", placeholder: "", type: "select", options: ["global", "cn"], default: "cn" },
      ] },
    { id: "ollama",    label: "Ollama",             cat: "cookie", billing: ["按量"], authType: "cookie",
      desc: "Ollama Cloud，按周统计用量",
      tags: [{text:"周窗口",color:"amber"}] },
  ];

  // 按类别排序：订阅制(OAuth) → API Key → Cookie
  const CAT_ORDER: Record<string, number> = { subscription: 0, "api-key": 1, cookie: 2 };
  const sortedVendors = $derived([...VENDORS].sort((a, b) => CAT_ORDER[a.cat] - CAT_ORDER[b.cat]));
  // 账号区分组
  const GROUPS: Array<{ cat: string; label: string }> = [
    { cat: "subscription", label: "订阅制（OAuth）" },
    { cat: "api-key", label: "API Key" },
    { cat: "cookie", label: "Cookie" },
  ];

  let bound = $state<Record<string, boolean>>({});
  let expanded = $state<Set<string>>(new Set());
  let inputs = $state<Record<string, Record<string, string>>>({});
  let config = $state<Config | null>(null);
  let saveError = $state<Record<string, string>>({});
  // 厂商排列顺序（从 config 恢复，追加新厂商到末尾）
  let ordered = $state<string[]>([]);
  // 厂商启用状态（从 config 恢复，默认全部不启用）
  let active = $state<Set<string>>(new Set());

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
    const isOAuth = v.authType === "detect" || v.authType === "login";
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
      console.error("clear fields failed", e instanceof Error ? e.message : String(e));
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
    const next = new Set(expanded);
    if (next.has(id)) next.delete(id); else next.add(id);
    expanded = next;
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
      void api.refreshQuota(vendor);
      // set_credential re-fetched + re-cached the quota (clearing any stale
      // cookie_error), so refresh the "Cookie 失效" hint immediately.
      void loadQuotaErrors();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      saveError = { ...saveError, [vendor]: msg };
    }
  }
  async function remove(vendor: string): Promise<void> {
    try {
      await api.deleteCredential(vendor);
      bound = { ...bound, [vendor]: false };
      credFields = { ...credFields, [vendor]: [] };
    } catch {}
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

  async function refreshQuota(vendor: string): Promise<void> {
    refreshState = { ...refreshState, [vendor]: { status: "loading" } };
    try {
      await api.refreshQuota(vendor);
      refreshState = { ...refreshState, [vendor]: { status: "ok", msg: "刷新成功" } };
      // A refresh may have resolved (or surfaced) a cookie issue — reload hints.
      void loadQuotaErrors();
      // Auto-clear success message after 3s.
      setTimeout(() => {
        const next = { ...refreshState };
        delete next[vendor];
        refreshState = next;
      }, 3000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      refreshState = { ...refreshState, [vendor]: { status: "fail", msg } };
    }
  }
  function startLogin(vendor: string): void {
    // TODO: 后端 invoke 登录 command（如 codex_login spawn OAuth 流程）
    console.log("start login for", vendor);
  }

  async function updateConfig(partial: Partial<Config>): Promise<void> {
    if (!config) return;
    const next = { ...config, ...partial };
    try {
      await api.setConfig(next);
      config = next;
    } catch {}
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
      <div class="section-box" style="margin-top:12px">
        <div class="group-head">{g.label} <span class="group-count">{items.length}</span></div>
        {#each items as v, rowIdx (v.id)}
          {@const fs = fieldsFor(v)}
          <!-- 主行（可点击展开） -->
          <button class="arow {rowIdx === items.length - 1 ? 'arow-last' : ''}" class:open={expanded.has(v.id)} onclick={() => toggle(v.id)}>
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
            <div class="panel">
              {#if bound[v.id]}
                {#if cookieErrorOf[v.id]}
                  <p class="panel-warn">⚠ {cookieErrorOf[v.id]}，请重新填写并保存</p>
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
                        <button class="btn-primary" onclick={() => saveCookie(v.id)} disabled={cookieSaving || !cookieDraft.trim()}>
                          {cookieSaving ? "保存中…" : "保存 Cookie"}
                        </button>
                        <button class="btn-outline" onclick={cancelEditCookie} disabled={cookieSaving}>取消</button>
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
                        <button class="btn-outline" onclick={() => startEditCookie(v.id)}>
                          {(credFields[v.id] ?? []).includes("cookie") ? "更新 Cookie" : "添加 Cookie"}
                        </button>
                      </div>
                    {/if}
                  </div>
                {/if}
                <div class="panel-actions">
                  {#each clearActions(v) as act (act.label)}
                    <button class="btn-outline" onclick={() => doClear(v.id, act)}>{act.label}</button>
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
                <div class="panel-actions">
                  <button class="btn-outline" onclick={() => startLogin(v.id)}>立即检测</button>
                  <button class="btn-primary" onclick={() => startLogin(v.id)}>{v.loginLabel ?? "登录"}</button>
                </div>
              {:else}
                {#if VENDOR_PANEL[v.id]}
                  <button class="btn-open" onclick={() => openKeyUrl(v.id)}>在浏览器打开 {VENDOR_PANEL[v.id].pageLabel} 页面</button>
                  {#if VENDOR_PANEL[v.id].extraUrl && VENDOR_PANEL[v.id].extraLabel}
                    <button class="btn-open" onclick={() => open(VENDOR_PANEL[v.id].extraUrl!).catch(() => {})}>在浏览器打开 {VENDOR_PANEL[v.id].extraLabel} 页面</button>
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
                  <button class="btn-outline" onclick={() => toggle(v.id)}>取消</button>
                  <button class="btn-primary" onclick={() => save(v.id)}>保存</button>
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

  <div class="section-box" style="margin-top:16px">
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

  <div class="section-box" style="margin-top:16px">
    <div class="group-head">厂商管理</div>
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
            <button class="ibtn" class:on={active.has(id)} title={active.has(id) ? '已启用' : '已停用'}
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
            <button class="ibtn" title="上移" disabled={i === 0} onclick={() => move(i, -1)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
              </svg>
            </button>
            <!-- 下移 -->
            <button class="ibtn" title="下移" disabled={i === ordered.length - 1} onclick={() => move(i, 1)}>
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

  .section-title {
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 15px;
    color: var(--amber);
    margin-top: 20px;
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .section-title:first-of-type { margin-top: 24px; }
  .title-stat {
    font-size: 11px;
    font-weight: 500;
    color: var(--lime);
    background: rgba(108,199,116,0.10);
    padding: 2px 9px;
    border-radius: 5px;
  }

  .section-box {
    background: rgba(0,0,0,0.02);
    border: 1px solid var(--border-dim);
    border-radius: 10px;
    padding: 12px 14px;
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
  .itag.c-blue   { background: rgba(79,195,247,0.14); color: #4fc3f7; }
  .itag.c-amber  { background: rgba(232,176,75,0.14); color: var(--amber); }
  .itag.c-purple { background: rgba(179,136,255,0.14); color: #b388ff; }
  .itag.c-lime   { background: rgba(108,199,116,0.14); color: var(--lime); }
  .itag.c-coral  { background: rgba(224,108,117,0.14); color: var(--coral); }
  .itag.c-gray   { background: var(--surface-tint-strong); color: var(--text-faint); }

  .astate { flex-shrink: 0; min-width: 50px; display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }
  .badge { font-size: 10.5px; font-weight: 500; padding: 2px 7px; border-radius: 5px; }
  .badge.s-ok  { color: var(--lime); background: rgba(108,199,116,0.12); }
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
    border: 1px solid rgba(234,84,85,0.40);
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
    background: rgba(255, 255, 255, 0.02);
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

  .btn-outline {
    background: var(--surface-tint-strong);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    padding: 5px 12px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 11.5px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-outline:hover { border-color: var(--amber); color: var(--amber); background: rgba(232,176,75,0.08); }

  .btn-primary {
    background: var(--amber);
    border: none;
    color: #1a1408;
    padding: 5px 14px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .btn-primary:hover { opacity: 0.88; }

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
  .btn-open:hover { border-color: var(--amber); color: var(--amber); background: rgba(232,176,75,0.08); }

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

  /* ── settings rows (matching Collection.svelte) ── */
  .box-row { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px dashed var(--border); gap: 16px; }
  .box-row:first-child { padding-top: 2px; }
  .box-row:last-child { border-bottom: none; padding-bottom: 2px; }
  .lab { font-size: 13px; color: var(--text); }
  .lab .hint { font-size: 11px; color: var(--text-faint); margin-top: 2px; }

  .sel {
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    color: var(--text);
    padding: 6px 10px;
    border-radius: 7px;
    font-size: 13px;
    cursor: pointer;
    font-family: inherit;
    min-width: 130px;
    height: 32px;
  }
  .sel:hover { border-color: var(--amber); }
  .sel:focus { outline: none; border-color: var(--amber); }

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
  .ttag.tt-active  { background: rgba(108,199,116,0.12); color: var(--lime); }
  .ttag.tt-unconfig { background: rgba(232,176,75,0.10); color: var(--amber); }
  .ttag.tt-inactive { background: rgba(234,84,85,0.12); color: var(--coral); }
  .ttag.tt-auth-subscription { background: rgba(79,195,247,0.14); color: #4fc3f7; }
  .ttag.tt-auth-api-key { background: rgba(232,176,75,0.14); color: var(--amber); }
  .ttag.tt-auth-cookie { background: rgba(179,136,255,0.14); color: #b388ff; }
  .ttag.tt-billing { background: rgba(108,199,116,0.10); color: var(--lime); }

  .tright {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── icon buttons (shared with Collection.svelte) ── */
  .ibtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: none;
    border: 1px solid transparent;
    border-radius: 6px;
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
    color: var(--text-faint);
    transition: all 0.15s;
  }
  .ibtn:hover:not(:disabled) {
    background: var(--surface-tint-strong);
    color: var(--amber);
  }
  .ibtn:disabled { opacity: 0.15; cursor: default; }
  .ibtn.on { color: var(--amber); }
  .ibtn.on:hover { background: rgba(232,176,75,0.08); }

  .loading, .empty { font-size: 11px; color: var(--text-faint); padding: 8px 0; }
</style>
