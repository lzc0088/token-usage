<script lang="ts">
  // 账号额度: 折叠面板式账号绑定 + 额度查询.
  import { listen } from "@tauri-apps/api/event";
  import { api, type Config } from "../../lib/api";
  import { QUOTA_UPDATED } from "../../lib/events";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { VENDOR_PANEL,panelHint,panelLabel } from "../../lib/meta/panels";
  import { VENDORS, fieldsFor, CAT_ORDER, GROUPS, type VendorDef, type FieldDef, tv,vl } from "../../lib/meta/vendors";
  import { t,getLang } from "../../lib/i18n.svelte";
  import { moveTo } from "../../lib/util/reorder";
  import { rowDrag } from "../../lib/actions/rowDrag";
  import DeviceFlow, { type OAuthState } from "./DeviceFlow.svelte";

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
  // Per-field validation errors: fieldErrors[vendor][fieldKey] = "error message"
  let fieldErrors = $state<Record<string, Record<string, string>>>({});
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

  // Language — synced from config.language via $effect so it tracks
  // config changes (initial load, language switch) without depending on
  // module-level currentLang timing or $derived null-config short-circuit.
  let lang = $state(getLang());
  $effect(() => {
    const next = config?.language ?? getLang();
    if (next !== lang) lang = next;
  });

  /** Field label with i18n: uses enLabel when language is English. */
  function fl(f: { label: string; enLabel?: string }): string {
    return lang === "en" && f.enLabel ? f.enLabel : f.label;
  }
  /** Field placeholder with i18n: uses enPlaceholder when language is English. */
  function fp(f: { placeholder: string; enPlaceholder?: string }): string {
    return lang === "en" && f.enPlaceholder ? f.enPlaceholder : f.placeholder;
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
      return [{ label: isOAuth ? t("account.unbound") : t("account.unset"), kind: "dim" }];
    }
    const connected = isOAuth ? t("account.connected") : t("account.connectedKey");
    if (cookieErr) {
      // Bound but cookie stale — keep the connection hint greyed + flag the error.
      return [
        { label: connected, kind: "dim" },
        { label: t("account.cookieInvalid"), kind: "warn" },
      ];
    }
    return [{ label: connected, kind: "ok" }];
  }

  /** "Clear" button label reflects the vendor's actual credential type:
   * subscription → 清除登录; pure cookie → 清除 Cookie; pure API Key → 清除 API Key;
   * mixed (key+cookie like Volcengine, or key+ids like GLM Team) → 清除凭证. */
  function clearButtonLabel(v: VendorDef): string {
    const hasCookie = fieldsFor(v).some((f) => f.key === "cookie");
    if (v.cat === "subscription") return t("account.clearLogin");
    if (v.cat === "cookie") return hasCookie ? t("account.clearCookie") : t("account.clearCred");
    // cat === "api-key"
    return hasCookie ? t("account.clearCred") : t("account.clearApiKey");
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
      acts.push({ label: t("account.clearApiKey"), fields });
    }
    if (filled.includes("cookie")) {
      acts.push({ label: t("account.clearCookie"), fields: ["cookie"] });
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
      // Immediately refresh this vendor's quota and AWAIT it — the cache must
      // be updated BEFORE we reload (and before the user returns to the
      // popover / 额度页). If refresh fails, the cookie_error badge stays but
      // the cookie save itself succeeded; the scheduler will retry.
      try {
        await api.refreshQuota(vendor);
      } catch {
        /* refresh failed — cookie is saved, will retry */
      }
      await loadQuotaErrors();
    } catch (e) {
      saveError = { ...saveError, [vendor]: e instanceof Error ? e.message : String(e) };
    } finally {
      cookieSaving = false;
    }
  }
  /** Move the vendor `id` to the new absolute index in the ordered list, then
   *  persist. Used by the row-drag action (see `use:rowDrag`). */
  function moveToIndex(id: string, newIndex: number): void {
    const myIdx = ordered.indexOf(id);
    if (myIdx < 0) return;
    const next = moveTo(ordered, myIdx, newIndex);
    if (next === ordered) return;
    ordered = next;
    saveOrder();
  }

  // Per-vendor filled credential fields (e.g. ["key","secret","cookie"]).
  let credFields = $state<Record<string, string[]>>({});
  $effect(() => {
    let cancelled = false;
    (async () => {
      const bmap: Record<string, boolean> = {};
      const fmap: Record<string, string[]> = {};
      // Fetch all vendors' credential fields in parallel.
      const results = await Promise.all(
        VENDORS.map(async (v) => {
          if (cancelled) return null;
          try {
            const fields = await api.getCredentialFields(v.id);
            return { id: v.id, fields, bound: fields.length > 0 };
          } catch {
            return { id: v.id, fields: [] as string[], bound: false };
          }
        }),
      );
      if (cancelled) return;
      for (const r of results) {
        if (!r) continue;
        fmap[r.id] = r.fields;
        bmap[r.id] = r.bound;
      }
      bound = bmap;
      credFields = fmap;
    })();
    return () => { cancelled = true; };
  });

  function toggle(id: string): void {
    // Accordion: only one vendor panel open at a time.
    if (expanded.has(id)) {
      expanded = new Set();
    } else {
      expanded = new Set([id]);
      if (id === "copilot" || id === "codex") {
        api.feLog(`expand ${id}: bound=${bound[id]}, authType=${VENDORS.find(v => v.id === id)?.authType}, cpState=${copilotLoginState.phase}, cdState=${codexLoginState.phase}, credFields=${JSON.stringify(credFields[id])}`);
      }
    }
  }

  function getField(id: string, fieldKey: string): string {
    const vendor = VENDORS.find(x => x.id === id);
    const field = vendor ? fieldsFor(vendor).find(f => f.key === fieldKey) : undefined;
    return inputs[id]?.[fieldKey] ?? field?.default ?? "";
  }
  function setField(id: string, fieldKey: string, val: string): void {
    inputs = { ...inputs, [id]: { ...(inputs[id] ?? {}), [fieldKey]: val } };
    // Clear field-level error on user input.
    if (fieldErrors[id]?.[fieldKey]) {
      const next = { ...(fieldErrors[id] ?? {}), [fieldKey]: "" };
      fieldErrors = { ...fieldErrors, [id]: next };
    }
  }

  /** Validate all fields for a vendor before save. Returns true if all pass. */
  function validateFields(vendor: string): boolean {
    const v = VENDORS.find(x => x.id === vendor);
    const fields = v ? fieldsFor(v) : [];
    const errors: Record<string, string> = {};
    for (const f of fields) {
      if (f.type === "select") continue; // selects always have a value
      if ((f as FieldDef).optional) continue; // optional add-ons (e.g. Volcengine console cookie)
      const val = (inputs[vendor]?.[f.key] ?? "").trim();
      if (!val) {
        errors[f.key] = t("account.fieldRequired");
        continue;
      }
      // API key format: should look like a key (alphanumeric + common separators), min 8 chars.
      if (f.type === "password" && (f.key === "key" || f.key === "secret")) {
        if (val.length < 8) {
          errors[f.key] = t("account.fieldTooShort");
        }
      }
      // Cookie: minimum length check.
      if (f.type === "textarea" && f.key === "cookie") {
        if (val.length < 10) {
          errors[f.key] = t("account.fieldTooShort");
        }
      }
    }
    if (Object.keys(errors).length > 0) {
      fieldErrors = { ...fieldErrors, [vendor]: errors };
      return false;
    }
    // Clear errors for this vendor.
    if (fieldErrors[vendor]) {
      const next = { ...fieldErrors };
      delete next[vendor];
      fieldErrors = next;
    }
    return true;
  }

  async function save(vendor: string): Promise<void> {
    if (saving[vendor]) return;
    if (!validateFields(vendor)) return;

    const fields = fieldsFor(VENDORS.find(x => x.id === vendor)!);
    saving = { ...saving, [vendor]: true };
    saveError = { ...saveError, [vendor]: "" };
    const payload = JSON.stringify(
      Object.fromEntries(fields.map(f => [f.key, getField(vendor, f.key)]))
    );
    try {
      await api.setCredential(vendor, payload);
      bound = { ...bound, [vendor]: true };
      try {
        credFields = { ...credFields, [vendor]: await api.getCredentialFields(vendor) };
      } catch {
        credFields = { ...credFields, [vendor]: fields.map((f) => f.key).filter((k) => getField(vendor, k) !== "") };
      }
      inputs = { ...inputs, [vendor]: {} };
      saveError = { ...saveError, [vendor]: "" };
      // Auto-refresh quota after save so the user sees updated data immediately.
      try {
        await api.refreshQuota(vendor);
      } catch {
        /* quota refresh failed — credential is saved, scheduler will retry */
      }
      await loadQuotaErrors();
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
    const un = listen<void>(QUOTA_UPDATED, () => { api.feLog("QUOTA_UPDATED received"); void loadQuotaErrors(); });
    return () => {
      un.then((u) => u());
    };
  });

  // Per-vendor refresh state tracking removed — save now auto-refreshes
  // quota after successful credential update (see save() below).


  // ── OAuth state (lives here so event listeners match the settings window lifetime) ──

  let copilotLoginState = $state<OAuthState>({ phase: "idle" });
  let codexLoginState = $state<OAuthState>({ phase: "idle" });

  // OAuth flow is now handled entirely via two-step IPC calls (no events):
  // 1. copilotLogin() → returns device code info
  // 2. pollForToken() → blocks until user authorizes, returns access token
  // This matches the token-monitor reference pattern and avoids Tauri event delivery issues.

  async function startCopilotLogin(): Promise<void> {
    api.feLog("startCopilotLogin() -> requesting device code");
    copilotLoginState = { phase: "requesting" };
    try {
      // Phase 1: request device code from GitHub (returns user code + URL)
      const info = await api.copilotLogin();
      api.feLog(`copilot_login ok: code=${info.user_code}, url=${info.verification_url}`);
      // Show the user code and open the browser for authorization
      copilotLoginState = {
        phase: "authorize",
        userCode: info.user_code,
        verificationUrl: info.verification_url,
      };
      await api.openExternal(info.verification_url);
      // Phase 2: poll for access token (blocks until user authorizes)
      api.feLog("copilot_poll: polling for access token...");
      copilotLoginState = { ...copilotLoginState, phase: "polling" };
      const token = await api.pollCopilotToken();
      api.feLog("copilot_poll ok: token received");
      await api.setCredential("copilot", JSON.stringify({ key: token }));
      copilotLoginState = { phase: "success" };
      bindAndReloadCredential("copilot");
    } catch (e: unknown) {
      api.feLog("copilot ERROR: " + (e instanceof Error ? e.message : String(e)));
      copilotLoginState = { phase: "error", error: e instanceof Error ? e.message : String(e) };
    }
  }

  async function startCodexLogin(): Promise<void> {
    api.feLog("startCodexLogin() -> calling api.codexLogin()");
    codexLoginState = { phase: "requesting" };
    try {
      await api.codexLogin();
      api.feLog("api.codexLogin() resolved");
      bindAndReloadCredential("codex");
    } catch (e: unknown) {
      api.feLog("codex ERROR: " + (e instanceof Error ? e.message : String(e)));
      codexLoginState = { phase: "error", error: e instanceof Error ? e.message : String(e) };
    }
  }
  /** After a successful OAuth login, mark bound + reload credential fields. */
  function bindAndReloadCredential(vendor: string): void {
    bound = { ...bound, [vendor]: true };
    api.getCredentialFields(vendor)
      .then((f) => { credFields = { ...credFields, [vendor]: f }; })
      .catch(() => {});
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

<div class="sh"><h3>{t("account.title2")}</h3><div class="desc">{t("account.desc")}</div></div>
<div class="sc">

  <!-- ══ 账号 ══ -->
  <div class="section-title">
    {t("account.accountsSection")}
    <span class="title-stat">{Object.values(bound).filter(Boolean).length} / {VENDORS.length} {t("account.connectedCount")}</span>
  </div>

  {#each GROUPS as g}
    {@const items = sortedVendors.filter(v => v.cat === g.cat)}
    {#if items.length > 0}
      <div class="section-box" style="margin-top:8px">
        <div class="group-head">{g.label} <span class="group-count">{items.length}</span></div>
        {#each items as v, rowIdx (v.id)}
          {@const fs = fieldsFor(v)}
          <!-- 主行（可点击展开） -->
          <button type="button" class="arow {rowIdx === items.length - 1 ? 'arow-last' : ''}" class:open={expanded.has(v.id)} onclick={() => toggle(v.id)} aria-expanded={expanded.has(v.id)} aria-controls="panel-{v.id}" id="accordion-btn-{v.id}">
            <ToolIcon vendor={v.id} size={22} />
            <span class="ainfo">
              <span class="aname">{v.label}</span>
              <span class="atags">
                {#each v.tags as tag (tag.text)}
                  <span class="itag c-{tag.color}">{tv(tag.text, lang)}</span>
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
            <div class="panel {rowIdx === items.length - 1 ? 'panel-last' : ''}" role="region" aria-labelledby="accordion-btn-{v.id}" id="panel-{v.id}">
              {#if bound[v.id]}
                {#if cookieErrorOf[v.id]}
                  <p class="panel-warn">⚠ {cookieErrorOf[v.id]}{t("account.cookieErrorRetry")}</p>
                  {#if VENDOR_PANEL[v.id]}
                    <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>{t("account.openInBrowser")} {panelLabel(VENDOR_PANEL[v.id], lang)} {t("account.pageLabel")}</button>
                    <p class="panel-note">{panelHint(VENDOR_PANEL[v.id], lang)}</p>
                  {/if}
                {:else if v.id === "stepfun"}
                  <p class="panel-note" style="margin:0 0 8px">{t("account.cookieNote")}</p>
                {/if}
                <p class="panel-hint">
                  {#if v.authType === "detect" || v.authType === "login"}
                    {t("account.boundHintSub")}
                  {:else}
                    {t("account.boundHintCookie")}
                  {/if}
                </p>
                {#if isMixedVendor(v)}
                  <div class="cookie-mgr">
                    {#if editingCookieVendor === v.id}
                      <textarea class="finp finp-textarea" bind:value={cookieDraft} placeholder={t("account.pasteCookie")} rows="4" disabled={cookieSaving}></textarea>
                      <div class="cookie-mgr-actions">
                        <button type="button" class="btn-primary" onclick={() => saveCookie(v.id)} disabled={cookieSaving || !cookieDraft.trim()}>
                          {cookieSaving ? t("account.checking") : t("account.saveCookieBtn")}
                        </button>
                        <button type="button" class="btn-outline" onclick={cancelEditCookie} disabled={cookieSaving}>{t("account.cancel")}</button>
                      </div>
                    {:else}
                      <div class="cookie-mgr-row">
                        <span class="cookie-mgr-status">
                          {#if cookieErrorOf[v.id] && (credFields[v.id] ?? []).includes("cookie")}
                            <span class="cs-err">⚠ {t("account.cookieExpired")}</span>
                          {:else if (credFields[v.id] ?? []).includes("cookie")}
                            <span class="cs-ok">✓ {t("account.cookieBound")}</span>
                          {:else}
                            <span class="cs-none">{t("account.cookieNotBound")}</span>
                          {/if}
                        </span>
                        <button type="button" class="btn-outline" onclick={() => startEditCookie(v.id)}>
                          {(credFields[v.id] ?? []).includes("cookie") ? t("account.updateCookieBtn") : t("account.addCookieBtn")}
                        </button>
                      </div>
                    {/if}
                  </div>
                {/if}
                <div class="panel-actions">
                  {#each clearActions(v) as act (act.label)}
                    <button type="button" class="btn-outline" onclick={() => doClear(v.id, act)}>{act.label}</button>
                  {/each}
                </div>
              {:else if v.authType === "detect" || v.authType === "login"}
                <p class="panel-hint">
                  {#if v.authType === "detect"}
                    {t("account.detectHint")}
                  {:else}
                    {t("account.oauthHint")}
                  {/if}
                </p>
                {#if v.id === "claude" && VENDOR_PANEL.claude}
                  <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>{t("account.openInBrowser")} {panelLabel(VENDOR_PANEL.claude, lang)} {t("account.pageLabel")}</button>
                  <p class="panel-note">{panelHint(VENDOR_PANEL.claude, lang)}</p>
                  <div class="fields">
                    {#each fieldsFor(v) as f (f.key)}
                      {@const hasErr = !!fieldErrors[v.id]?.[f.key]}
                      <label class="field">
                        <span class="flabel">{fl(f)}</span>
                        {#if f.type === "textarea"}
                          <textarea class="finp finp-textarea" class:field-invalid={hasErr} placeholder={fp(f)} rows="4" oninput={(e) => setField(v.id, f.key, (e.target as HTMLTextAreaElement).value)}>{getField(v.id, f.key)}</textarea>
                        {/if}
                        {#if hasErr}
                          <p class="field-err">{fieldErrors[v.id][f.key]}</p>
                        {/if}
                      </label>
                    {/each}
                  </div>
                  {#if saveError[v.id]}
                    <p class="save-err">{saveError[v.id]}</p>
                  {/if}
                  <div class="panel-actions">
                    <button type="button" class="btn-outline" onclick={() => toggle(v.id)} disabled={saving[v.id]}>{t("account.cancel")}</button>
                    <button type="button" class="btn-primary" onclick={() => save(v.id)} disabled={saving[v.id]}>
                      {saving[v.id] ? t("account.checking") : t("account.save")}
                    </button>
                  </div>
                {:else if (v.id === "copilot")}
                  {#if copilotLoginState.phase === "idle"}
                    <div class="panel-actions">
                      <button type="button" class="btn-primary" onclick={() => { api.feLog("copilot 登录按钮点击"); startCopilotLogin(); }}>{vl(v, lang) || t("account.login")}</button>
                    </div>
                  {:else}
                    <DeviceFlow vendor="copilot" state={copilotLoginState} onRetry={() => startCopilotLogin()} />
                  {/if}
                {:else if (v.id === "codex")}
                  {#if codexLoginState.phase === "idle"}
                    <div class="panel-actions">
                      <button type="button" class="btn-primary" onclick={() => { api.feLog("codex 登录按钮点击"); startCodexLogin(); }}>{vl(v, lang) || t("account.login")}</button>
                    </div>
                  {:else}
                    <DeviceFlow vendor="codex" state={codexLoginState} onRetry={() => startCodexLogin()} />
                  {/if}
                {:else}
                  <div class="panel-actions">
                    {#if v.authType === "detect"}
                      <button type="button" class="btn-outline" onclick={() => startLogin(v.id)}>{t("account.detect")}</button>
                    {/if}
                    <button type="button" class="btn-primary" onclick={() => startLogin(v.id)}>{vl(v, lang) || t("account.login")}</button>
                  </div>
                {/if}
              {:else}
                {#if VENDOR_PANEL[v.id]}
                  <button type="button" class="btn-open" onclick={() => openKeyUrl(v.id)}>{t("account.openInBrowser")} {panelLabel(VENDOR_PANEL[v.id], lang)} {t("account.pageLabel")}</button>
                  {#if VENDOR_PANEL[v.id].extraUrl && VENDOR_PANEL[v.id].extraLabel}
                    <button type="button" class="btn-open" onclick={() => api.openExternal(VENDOR_PANEL[v.id].extraUrl!).catch(() => {})}>{t("account.openInBrowser")} {panelLabel(VENDOR_PANEL[v.id], lang)} {t("account.pageLabel")}</button>
                  {/if}
                  <p class="panel-note">{panelHint(VENDOR_PANEL[v.id], lang)}</p>
                {:else}
                  <p class="panel-note">
                    {#if v.authType === "cookie"}
                      {t("account.cookieCopyHint")}
                    {:else}
                      {t("account.apiKeyHint")}
                    {/if}
                  </p>
                {/if}
                <div class="fields">
                  {#each fs as f (f.key)}
                    {@const hasErr = !!fieldErrors[v.id]?.[f.key]}
                    <label class="field">
                      <span class="flabel">{fl(f)}</span>
                      {#if f.type === "select"}
                        <select class="fsel" class:field-invalid={hasErr} value={getField(v.id, f.key)} onchange={(e) => setField(v.id, f.key, (e.target as HTMLSelectElement).value)}>
                          {#each f.options ?? [] as opt (opt)}
                            <option value={opt}>{opt}</option>
                          {/each}
                        </select>
                      {:else if f.type === "textarea"}
                        <textarea class="finp finp-textarea" class:field-invalid={hasErr} placeholder={fp(f)} rows="4" oninput={(e) => setField(v.id, f.key, (e.target as HTMLTextAreaElement).value)}>{getField(v.id, f.key)}</textarea>
                      {:else}
                        <input class="finp" class:field-invalid={hasErr} type={f.type ?? "text"} placeholder={fp(f)} value={getField(v.id, f.key)} oninput={(e) => setField(v.id, f.key, (e.target as HTMLInputElement).value)} />
                      {/if}
                      {#if hasErr}
                        <p class="field-err">{fieldErrors[v.id][f.key]}</p>
                      {/if}
                    </label>
                  {/each}
                </div>
                <div class="panel-actions">
                  <button type="button" class="btn-outline" onclick={() => toggle(v.id)} disabled={saving[v.id]}>{t("account.cancel")}</button>
                  <button type="button" class="btn-primary" onclick={() => save(v.id)} disabled={saving[v.id]}>
                    {saving[v.id] ? t("account.checking") : t("account.save")}
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
    {t("account.quotasSection")}
    <span class="title-stat">{active.size} / {VENDORS.length} {t("account.enabledCount")}</span>
  </div>

  <div class="section-box" style="margin-top:8px">
    <div class="group-head">{t("account.globalSettings")}</div>
    <div class="box-row">
      <div class="lab">{t("account.refreshFreq")}<div class="hint">{t("account.refreshFreqHint")}</div></div>
      <select class="sel" value={config?.quota_refresh_interval ?? "5m"}
        onchange={(e) => updateConfig({ quota_refresh_interval: (e.target as HTMLSelectElement).value as Config["quota_refresh_interval"] })}>
        <option value="1m">{t("account.1m")}</option>
        <option value="3m">{t("account.3m")}</option>
        <option value="5m">{t("account.5m")}</option>
        <option value="10m">{t("account.10m")}</option>
        <option value="15m">{t("account.15m")}</option>
      </select>
    </div>
    <div class="box-row">
      <div class="lab">{t("account.progressMode")}<div class="hint">{t("account.progressModeHint")}</div></div>
      <select class="sel" value={config?.quota_progress_mode ?? "剩余"}
        onchange={(e) => updateConfig({ quota_progress_mode: (e.target as HTMLSelectElement).value as Config["quota_progress_mode"] })}>
        <option value="用量">{t("account.usage")}</option>
        <option value="剩余">{t("account.remaining")}</option>
      </select>
    </div>
  </div>

  <div class="section-box" style="margin-top:12px">
    <div class="group-head">{t("account.vendorMgmt")}</div>
    <div class="icon-legend">
      <span class="legend-text">{t("account.dragReorder")}</span>
      <div class="legend-actions">
        <span class="legend-item">{t("account.enable")}</span>
      </div>
    </div>
    {#each ordered as id (id)}
      {@const v = VENDORS.find(x => x.id === id)}
      {#if v}
        <div
          class="trow"
          data-row-id={id}
          use:rowDrag={{ id, onReorder: (newIndex) => moveToIndex(id, newIndex), excludeSelector: ".ibtn-toggle" }}
        >
          <ToolIcon vendor={id} size={22} />
          <span class="tleft">
            <span class="tname">{v.label}</span>
            <span class="ttags">
              {#if active.has(id)}
                <span class="ttag" class:tt-active={bound[v.id]} class:tt-unconfig={!bound[v.id]}>
                  {bound[v.id] ? t("account.detected") : t("account.notConfigured")}
                </span>
              {:else}
                <span class="ttag tt-inactive">{t("account.disabledTag")}</span>
              {/if}
              {#each v.billing as b (b)}
                <span class="ttag tt-billing">{tv(b, lang)}</span>
              {/each}
              <span class="ttag tt-auth-{v.cat}">{authTypeLabel(v.cat)}</span>
            </span>
          </span>
          <span class="tright">
            <!-- 启用 toggle：选中→验证绑定状态，未选中→{t("account.disabledTag")}（状态持久化） -->
            <button class="ibtn ibtn-toggle" class:on={active.has(id)} title={active.has(id) ? t("account.enabled") : t("account.disabledTag")} aria-label={active.has(id) ? t("account.disable") : t("account.enable")}
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
  .sel { min-width: 150px; }

  .panel-actions { display: flex; align-items: center; gap: 8px; row-gap: 4px; flex-wrap: wrap; }
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
    min-width: 64px;
    text-align: center;
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
  .field-err { font-size: 10.5px; color: var(--coral); margin: 3px 0 0; line-height: 1.4; }
  .finp.field-invalid { border-color: var(--coral-border); }

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
    padding: 8px 4px 8px 6px;
    border-bottom: 1px dashed var(--border);
    gap: 12px;
    border-radius: 4px;
    cursor: grab;
    transition: background 0.12s;
  }
  .trow:hover { background: var(--surface-tint); }
  .trow:active { cursor: grabbing; }
  .trow:last-child { border-bottom: none; }
  .trow.row-drag-source { opacity: 0.35; }
  /* The ghost is a clone of the row, fixed-positioned, with its own transform
     offset (set by the rowDrag action). It is a child of <body>, not the
     .trow, so its styles must be standalone (not :global-scoped to .trow). */
  :global(.row-drag-ghost) {
    pointer-events: none;
    opacity: 0.75;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    background: var(--surface-tint-strong);
    border-radius: 5px;
    will-change: transform;
  }

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
    justify-content: space-between;
    margin-top: 0;
    margin-bottom: 0;
    padding: 0;
    gap: 4px;
  }
  .icon-legend .legend-actions {
    display: flex;
    align-items: center;
    gap: 2px; /* match .tright button gap */
  }
  .icon-legend .legend-item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px; /* match .ibtn width → aligns with the button column below */
    height: 28px;
    font-size: 10.5px;
    color: var(--text-faint);
    line-height: 1;
  }
  .icon-legend .legend-text {
    font-size: 10.5px;
    color: var(--text-faint);
    white-space: nowrap;
  }
</style>
