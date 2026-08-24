<script lang="ts">
  // 常规 (T5.2): 启动 + 应用更新 + 关于. 参考 wireframe #general.
  import { untrack } from "svelte";
  import type { Config, UpdateInfo, InstallEvent } from "../../lib/api";
  import { api } from "../../lib/api";
  import { t } from "../../lib/i18n.svelte";
  import Select from "../../components/common/Select.svelte";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();

  // Git hosting repo from Vite env (full string, e.g. "gitee.com/owner/repo").
  const UPDATE_REPO = (import.meta.env.VITE_UPDATE_REPO as string) || "";

  // App version from Rust backend (Cargo.toml → env!("CARGO_PKG_VERSION")).
  let appVersion = $state("1.0.0");
  $effect(() => { api.getAppVersion().then(v => { appVersion = v; }).catch(() => {}); });

  // Repo info for the About links.
  const REPO_PATH = $derived.by(() => {
    // Extract "owner/repo" from e.g. "github.com/owner/repo"
    const parts = UPDATE_REPO.split("/").filter(Boolean);
    // Skip the host part (e.g. "github.com")
    if (parts.length >= 3 && parts[0].includes(".")) {
      return parts.slice(1, 3).join("/");
    }
    return parts.slice(0, 2).join("/");
  });

  /** Open an external URL in the system's default browser. */
  function openExternal(url: string): void {
    if (url) api.openExternal(url).catch(() => {});
  }

  let checking = $state(false);
  let updateStatus = $state<UpdateInfo | null>(null);
  let showLatestAlert = $state(false);
  // Install state machine (application-internal auto-update via updater plugin).
  type InstallState = "idle" | "downloading" | "installing" | "installed" | "relaunching" | "error";
  let installState = $state<InstallState>("idle");
  let installError = $state<string>("");
  let downloadProgress = $state<{ downloaded: number; total: number }>({ downloaded: 0, total: 0 });
  let rateMode = $state<"auto" | "manual">(
    untrack(() => (config.rate_mode === "manual" ? "manual" : "auto")),
  );
  let manualRate = $state<string>("7.2500");
  let savingRate = $state(false);
  let currentRate = $state<number>(7.25);
  let refreshingRate = $state(false);
  let showRateAlert = $state(false);
  let rateAlertMessage = $state("");
  let autoStartEnabled = $state<boolean>(untrack(() => !!config.auto_start));
  let autoStartToggling = $state(false);

  // 加载当前汇率
  const timeouts = new Set<number>();
  function sTimeout(fn: () => void, ms: number): number {
    const id = window.setTimeout(fn, ms);
    timeouts.add(id);
    return id;
  }
  // Cleanup tracked timeouts on unmount.
  $effect(() => {
    return () => { for (const id of timeouts) clearTimeout(id); };
  });
  function loadCurrentRate(): void {
    api.getExchangeRate().then((info) => {
      currentRate = info.rate;
      manualRate = info.rate.toFixed(4);
    }).catch(() => {
      // 失败时使用默认值
      currentRate = 7.25;
    });
  }

  // 加载开机启动真实状态（以系统为准，避免配置与实际不一致）
  function loadAutoStart(): void {
    api.getAutoStart().then((enabled) => {
      autoStartEnabled = enabled;
      // 同步到父组件 config，保证其他引用 config.auto_start 的地方一致
      if (config.auto_start !== enabled) {
        onUpdate({ auto_start: enabled });
      }
    }).catch(() => {
      /* 获取开机启动状态失败 */
    });
  }

  // 切换开机启动：乐观更新 + 失败回滚
  async function toggleAutoStart(): Promise<void> {
    if (autoStartToggling) return;
    const next = !autoStartEnabled;
    autoStartEnabled = next;
    autoStartToggling = true;
    try {
      const actual = await api.setAutoStart(next);
      autoStartEnabled = actual;
      onUpdate({ auto_start: actual });
    } catch (err) {
      // 失败回滚
      autoStartEnabled = !next;
    } finally {
      autoStartToggling = false;
    }
  }

  // ── 数据导出 ──
  let exportMessage = $state("");
  const exportTimeouts = new Set<number>();

  function showExportMsg(msg: string): void {
    exportMessage = msg;
    const id = window.setTimeout(() => { exportMessage = ""; }, 3000);
    exportTimeouts.add(id);
  }

  async function copyJson(): Promise<void> {
    try {
      const json = await api.exportJson();
      await api.copyToClipboard(json);
      showExportMsg(t("export.copied"));
    } catch (e) {
      showExportMsg(String(e));
    }
  }

  async function copyCsv(): Promise<void> {
    try {
      const csv = await api.exportCsv();
      await api.copyToClipboard(csv);
      showExportMsg(t("export.copied"));
    } catch (e) {
      showExportMsg(String(e));
    }
  }

  // 组件挂载时加载
  $effect(() => {
    loadCurrentRate();
    loadAutoStart();
    return () => { for (const id of exportTimeouts) clearTimeout(id); };
  });

  function checkUpdate(): void {
    checking = true;
    updateStatus = null;
    showLatestAlert = false;

    if (!UPDATE_REPO) {
      updateStatus = {
        has_update: false, version: "", name: "", changelog: "",
        url: "", published_at: null,
        error: "未配置更新仓库地址（VITE_UPDATE_REPO）",
        error_kind: "api_error",
        download_url: null,
      };
      checking = false;
      return;
    }

    api.checkUpdate(UPDATE_REPO, appVersion, true).then((info) => {
      // Normalize: only treat as update when versions actually differ.
      const normalized = {
        ...info,
        has_update: info.has_update && info.version.replace(/^v/, '') !== appVersion,
      };
      updateStatus = normalized;
      checking = false;
      if (normalized.error) {
        return;
      }
      if (!normalized.has_update) {
        showLatestAlert = true;
        sTimeout(() => { showLatestAlert = false; }, 3000);
      }
    }).catch((e) => {
      updateStatus = {
        has_update: false, version: "", name: "", changelog: "",
        url: "", published_at: null,
        error: "调用失败：" + (e instanceof Error ? e.message : String(e)),
        error_kind: "network",
        download_url: null,
      };
      checking = false;
    });
  }

  // Trigger the in-app auto-update: download + verify + install + relaunch.
  // Progress events from the Rust backend drive `installState`.
  async function installUpdate(): Promise<void> {
    if (installState === "downloading" || installState === "installing") return;
    installState = "downloading";
    installError = "";
    downloadProgress = { downloaded: 0, total: 0 };
    try {
      await api.installUpdate((e: InstallEvent) => {
        switch (e.event) {
          case "Started":
            downloadProgress = { downloaded: 0, total: e.data.content_length };
            break;
          case "Progress":
            downloadProgress = { ...downloadProgress, downloaded: downloadProgress.downloaded + e.data.chunk_length };
            break;
          case "Finished":
            installState = "installing";
            break;
          case "Installed":
            // Update is staged; DON'T auto-restart. Wait for the user to click
            // "Restart now" so the app doesn't vanish without explanation.
            installState = "installed";
            break;
          case "Error":
            installState = "error";
            installError = e.data.message;
            break;
        }
      });
    } catch (e) {
      installState = "error";
      installError = e instanceof Error ? e.message : String(e);
    }
  }

  // User explicitly chose to restart and finish applying the update.
  function restartNow(): void {
    installState = "relaunching";
    api.restartApp().catch(() => {
      // restartApp should kill this process; if we're still here it failed
      // (macOS bug #11392) — hint the user to reopen manually.
      installState = "error";
      installError = t("general.relaunchHint");
    });
  }

  // Fallback when the in-app updater fails: open the release page in a browser
  // so the user can download the installer manually.
  function fallbackDownload(): void {
    const url = updateStatus?.download_url ?? updateStatus?.url;
    if (url) openExternal(url);
  }

  function resetInstall(): void {
    installState = "idle";
    installError = "";
    downloadProgress = { downloaded: 0, total: 0 };
  }

  function saveRate(): void {
    const num = parseFloat(manualRate);
    if (!validateRate(manualRate) || isNaN(num) || num <= 0) {
      rateAlertMessage = "请输入有效的正数汇率";
      showRateAlert = true;
      sTimeout(() => { showRateAlert = false; }, 4000);
      return;
    }
    savingRate = true;
    api.setManualRate(num).then(() => {
      currentRate = num;
      rateAlertMessage = `手动汇率已保存：1 USD = ${num.toFixed(4)} CNY`;
      showRateAlert = true;
      sTimeout(() => { showRateAlert = false; }, 3000);
    }).catch((err) => {
      rateAlertMessage = `保存失败：${err}`;
      showRateAlert = true;
      sTimeout(() => { showRateAlert = false; }, 5000);
    }).finally(() => {
      savingRate = false;
    });
  }

  function refreshRate(): void {
    refreshingRate = true;
    api.refreshExchangeRate().then((info) => {
      currentRate = info.rate;
      manualRate = info.rate.toFixed(4);

      // 显示成功提示
      rateAlertMessage = `汇率已更新：1 USD ≈ ${info.rate.toFixed(4)} CNY${info.cached ? "（缓存）" : ""}`;
      showRateAlert = true;
      sTimeout(() => { showRateAlert = false; }, 3000);
    }).catch((err) => {
      /* 刷新汇率失败 — shown in rateAlertMessage */
      rateAlertMessage = `汇率刷新失败：${err}`;
      showRateAlert = true;
      sTimeout(() => { showRateAlert = false; }, 5000);
    }).finally(() => {
      refreshingRate = false;
    });
  }

  function handleRateInput(e: Event): void {
    const input = e.target as HTMLInputElement;
    if (!input) return;

    let value = input.value;

    // Remove non-numeric characters except decimal point
    value = value.replace(/[^\d.]/g, '');

    // Ensure only one decimal point
    const parts = value.split('.');
    if (parts.length > 2) {
      value = parts[0] + '.' + parts.slice(1).join('');
    }

    // Limit to 2 decimal places
    if (parts.length === 2 && parts[1].length > 2) {
      value = parts[0] + '.' + parts[1].slice(0, 2);
    }

    manualRate = value;
  }

  function validateRate(value: string): boolean {
    const num = parseFloat(value);
    return !isNaN(num) && num > 0 && /^\d+(\.\d{1,2})?$/.test(value);
  }
</script>

<div class="sh"><h3>{t("general.title")}</h3><div class="desc">{t("general.desc")}</div></div>
<div class="sc">

  <!-- ══ 启动 ══ -->
  <div class="section-title">{t("general.startup")}</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">{t("general.autoStart")}<div class="hint">{t('general.autoStartHint')}</div></div>
      <button type="button" class="tg" class:on={autoStartEnabled} role="switch" aria-checked={autoStartEnabled} aria-label={t("general.autoStart")} onclick={toggleAutoStart} disabled={autoStartToggling}></button>
    </div>
  </div>

  <!-- ══ 币种 ══ -->
  <div class="section-title">{t("general.currency")}</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">{t('general.costDisplay')}<div class="hint">{t('general.costDisplayHint')}</div></div>
      <Select
        class="sel"
        value={config.currency ?? "cny"}
        options={[
          { value: "cny", label: t("general.cnyLabel") },
          { value: "usd", label: t("general.usdLabel") },
          { value: "both", label: t("general.bothLabel") },
        ]}
        onchange={(v) => onUpdate({ currency: v as "cny" | "usd" | "both" })}
      />
    </div>
    <div class="box-row">
      <div class="lab">{t('general.rateMode')}<div class="hint">{t('general.rateModeHint')}</div></div>
      <Select
        class="sel"
        value={rateMode}
        options={[
          { value: "auto", label: t("general.rateAuto") },
          { value: "manual", label: t("general.rateManual") },
        ]}
        onchange={(v) => {
          rateMode = v as "auto" | "manual";
          onUpdate({ rate_mode: v as "auto" | "manual" });
        }}
      />
    </div>
    {#if rateMode === 'auto'}
      <div class="box-row" style="padding-top: 4px">
        <div class="lab">{t('general.currentRate')}<div class="hint">{t("general.usdCnyRate")}</div></div>
        <div style="display: flex; align-items: center; gap: 8px;">
          <span class="rate-display">1 USD ≈ {currentRate.toFixed(4)} CNY</span>
          <button type="button" class="btn-outline" onclick={refreshRate} disabled={refreshingRate}>
            {refreshingRate ? t('general.refreshingRate') : t('general.refreshRate')}
          </button>
        </div>
      </div>
    {:else}
      <div class="box-row" style="padding-top: 4px">
        <div class="lab">{t('general.manualRate')}<div class="hint">{t("general.manualRateHint")}</div></div>
        <div style="display: flex; align-items: center; gap: 8px;">
          <input
            type="text"
            class="rate-input"
            value={manualRate}
            oninput={handleRateInput}
            placeholder="7.2500"
            inputmode="decimal"
          />
          <button type="button" class="btn-save" onclick={saveRate} disabled={savingRate || !validateRate(manualRate)}>
            {savingRate ? t('general.saving') : t('general.rateSavedHint')}
          </button>
        </div>
      </div>
    {/if}
    {#if showRateAlert}
      <div class="rate-alert" style="margin-top: 6px;">
        {rateAlertMessage}
      </div>
    {/if}
  </div>

  <!-- ══ 更新 ══ -->
  <div class="section-title">{t("general.update")}</div>
  <div class="section-box no-divider">
    <div class="box-row">
      <div class="lab">
        {t('general.currVersion')}：
        <span class="ver-num">{appVersion}</span>
      </div>
      <button type="button" class="btn-outline" onclick={checkUpdate} disabled={checking} class:checking>
        {#if checking}
          <span class="spin"></span>{t('general.checking')}
        {:else}
          {t('general.checkUpdate')}
        {/if}
      </button>
    </div>

    {#if updateStatus?.error}
      <div class="update-status has-error">
        <div class="update-error">
          <span class="update-err-icon">⚠</span>
          <span class="update-err-text">{updateStatus.error}</span>
        </div>
      </div>
    {:else if updateStatus?.has_update}
      <div class="update-status has-update">
        <div class="update-info">
          <div class="update-top">
            <span class="status-text">{t('general.newVersion')}：<span class="new-ver">{updateStatus.version}</span></span>
            {#if updateStatus.published_at}
              <span class="update-date">{new Date(updateStatus.published_at).toLocaleDateString("zh-CN")}</span>
            {/if}
          </div>
          {#if updateStatus.changelog}
            <div class="update-changelog">{updateStatus.changelog}</div>
          {/if}
          {#if installState === "idle"}
            <div class="install-actions">
              <button type="button" class="btn-download" onclick={installUpdate}>
                {t("general.installNow")}
              </button>
            </div>
          {:else if installState === "downloading"}
            <div class="install-progress">
              <div class="progress-bar">
                <div class="progress-fill" style="width: {downloadProgress.total > 0 ? Math.min(100, downloadProgress.downloaded / downloadProgress.total * 100) : 0}%"></div>
              </div>
              <span class="progress-text">
                {downloadProgress.total > 0
                  ? `${(downloadProgress.downloaded / 1048576).toFixed(1)} / ${(downloadProgress.total / 1048576).toFixed(1)} MB`
                  : `${(downloadProgress.downloaded / 1048576).toFixed(1)} MB`}
              </span>
            </div>
          {:else if installState === "installing"}
            <div class="install-status">{t("general.installing")}</div>
          {:else if installState === "installed"}
            <div class="install-ready">
              <span class="install-ready-text">{t("general.updateReady")}</span>
              <button type="button" class="btn-download" onclick={restartNow}>
                {t("general.restartNow")}
              </button>
            </div>
          {:else if installState === "relaunching"}
            <div class="install-status">{t("general.relaunching")}</div>
          {:else if installState === "error"}
            <div class="install-error">
              <span class="install-err-text">{installError}</span>
              <div class="install-actions">
                <button type="button" class="btn-download" onclick={() => { resetInstall(); installUpdate(); }}>
                  {t("general.retry")}
                </button>
                <button type="button" class="btn-outline-sm" onclick={fallbackDownload}>
                  {t("general.browserDownload")}
                </button>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else if showLatestAlert}
      <div class="latest-alert">{t('general.latestVersion')}</div>
    {/if}
  </div>

  <!-- ══ 数据导出 ══ -->
  <div class="section-title">{t("export.title")}</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">{t('export.title')}<div class="hint">{t('export.desc')}</div></div>
      <div class="export-actions">
        <button type="button" class="btn-export-json" onclick={copyJson}>
          {t('export.copyJson')}
        </button>
        <button type="button" class="btn-export-csv" onclick={copyCsv}>
          {t('export.copyCsv')}
        </button>
      </div>
    </div>
    {#if exportMessage}
      <div class="export-alert">{exportMessage}</div>
    {/if}
  </div>

  <!-- ══ 关于 ══ -->
  <div class="section-title">{t("general.about")}</div>
  <div class="section-box">
    <p class="about-text">
      {t("general.aboutText")}
    </p>
    <div class="about-links">
      {#if UPDATE_REPO}
        <a class="alink" href="https://github.com/{REPO_PATH}" target="_blank" rel="noopener" onclick={(e) => { e.preventDefault(); openExternal(`https://github.com/${REPO_PATH}`); }}>GitHub</a>
        <a class="alink" href="https://github.com/{REPO_PATH}/issues/new" target="_blank" rel="noopener" onclick={(e) => { e.preventDefault(); openExternal(`https://github.com/${REPO_PATH}/issues/new`); }}>{t('general.reportIssue')}</a>
      {/if}
    </div>
  </div>

</div>

<style>

  .sc { display: flex; flex-direction: column; }

  /* ── section title (override shared) ── */
  .section-title {
    margin-top: 20px;
    margin-bottom: 8px;
  }
  .section-title:first-of-type {
    margin-top: 24px;
  }

  /* ── section box (override shared) ── */
  .section-box {
    padding: 12px 14px;
  }

  /* ── select (override shared min-width) ── */


  /* ── rate display ── */
  .rate-display { font-family: var(--font-mono); font-size: 12px; color: var(--lime); font-weight: 500; background: rgba(108, 199, 116, 0.10); border: 1px solid rgba(108, 199, 116, 0.25); padding: 3px 8px; border-radius: 5px; }

  /* ── save button (local variant) ── */
  .btn-save { background: var(--amber); border: none; color: var(--badge-text); padding: 6px 14px; border-radius: 7px; font-size: 12px; font-weight: 600; cursor: pointer; font-family: inherit; min-width: 60px; height: 32px; }
  .btn-save:hover { opacity: 0.9; }
  .btn-save:disabled { opacity: 0.5; cursor: default; }

  /* ── rate alert ── */
  .rate-alert { padding: 8px 12px; margin-top: 6px; background: var(--lime-bg-soft); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 6px; font-size: 11.5px; color: var(--text); animation: slideIn 0.3s ease-out; text-align: center; }
  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-5px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── version number ── */
  .ver-num { font-family: var(--font-mono); font-size: 12px; color: var(--lime); font-weight: 600; background: rgba(108, 199, 116, 0.10); border: 1px solid rgba(108, 199, 116, 0.25); padding: 2px 8px; border-radius: 5px; }

  /* ── update status ── */
  .update-status { padding: 10px 12px; margin: 6px 0; border-radius: 7px; }
  .update-status.has-update { background: var(--lime-bg-soft); border: 1px solid rgba(108, 199, 116, 0.3); }
  .update-status.has-error { background: var(--coral-bg-soft); border: 1px solid rgba(234, 84, 85, 0.3); }
  .update-info { display: flex; flex-direction: column; gap: 6px; }
  .update-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .status-text { font-size: 12px; color: var(--text); }
  .new-ver { font-family: var(--font-mono); font-weight: 600; color: var(--lime); }
  .update-date { font-size: 10.5px; color: var(--text-faint); }
  .update-changelog {
    font-size: 11px; color: var(--text-dim); line-height: 1.7;
    max-height: 160px; overflow-y: auto;
    background: var(--surface-tint); border-radius: 6px; padding: 8px 10px;
    white-space: pre-wrap; word-break: break-word;
  }
  .update-changelog::-webkit-scrollbar { width: 4px; }
  .update-changelog::-webkit-scrollbar-thumb { background: var(--glass-3); border-radius: 2px; }
  .btn-download {
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--lime); border: none; color: var(--badge-text);
    padding: 5px 14px; border-radius: 6px; font-size: 11.5px; font-weight: 600;
    cursor: pointer; font-family: inherit; text-decoration: none;
    align-self: flex-start;
  }
  .btn-download:hover { opacity: 0.88; }

  /* ── install actions / progress (auto-update UI) ── */
  .install-actions { display: flex; gap: 8px; margin-top: 4px; align-self: flex-start; }
  .btn-outline-sm {
    background: transparent; border: 1px solid var(--glass-3); color: var(--text-dim);
    padding: 5px 12px; border-radius: 6px; font-size: 11.5px; font-weight: 500;
    cursor: pointer; font-family: inherit;
  }
  .btn-outline-sm:hover { border-color: var(--text-dim); color: var(--text); }

  .install-progress { display: flex; align-items: center; gap: 8px; margin-top: 4px; width: 100%; }
  .progress-bar {
    flex: 1; height: 6px; background: var(--bar-track); border-radius: 3px; overflow: hidden; min-width: 120px;
  }
  .progress-fill { height: 100%; background: var(--lime); transition: width 0.2s; }
  .progress-text { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-dim); white-space: nowrap; }

  .install-status { font-size: 12px; color: var(--lime); font-weight: 500; margin-top: 4px; }
  .install-ready { display: flex; flex-direction: column; gap: 6px; margin-top: 4px; align-items: flex-start; }
  .install-ready-text { font-size: 12px; color: var(--lime); font-weight: 500; }
  .install-error { display: flex; flex-direction: column; gap: 6px; margin-top: 4px; }
  .install-err-text { font-size: 11.5px; color: var(--coral); line-height: 1.5; }

  .update-error {
    display: flex; align-items: center; gap: 8px;
    font-size: 12px; color: var(--coral);
  }
  .update-err-icon { font-size: 14px; flex-shrink: 0; }
  .update-err-text { line-height: 1.5; }

  /* ── latest alert ── */
  .latest-alert { padding: 10px 12px; margin: 6px 0; background: var(--lime-bg-soft); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 7px; font-size: 12px; color: var(--text); text-align: center; animation: slideIn 0.3s ease-out; }
  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-10px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── export ── */
  .export-actions { display: flex; gap: 8px; }
  .btn-export-json {
    background: var(--amber); border: none; color: var(--badge-text);
    padding: 6px 14px; border-radius: 7px; font-size: 12px; font-weight: 600;
    cursor: pointer; font-family: inherit; min-width: 80px; height: 32px;
  }
  .btn-export-json:hover { opacity: 0.9; }
  .btn-export-csv {
    background: transparent; border: 1px solid var(--amber); color: var(--amber);
    padding: 6px 14px; border-radius: 7px; font-size: 12px; font-weight: 600;
    cursor: pointer; font-family: inherit; min-width: 80px; height: 32px;
  }
  .btn-export-csv:hover { background: var(--amber-hover); }
  .export-alert { padding: 8px 12px; margin-top: 6px; background: var(--lime-bg-soft); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 6px; font-size: 11.5px; color: var(--text); animation: slideIn 0.3s ease-out; text-align: center; }

  .about-text { font-size: 13px; color: var(--text); line-height: 1.7; margin: 0 0 10px; }
  .about-links { display: flex; gap: 20px; }
  .alink { font-size: 12px; color: var(--amber); text-decoration: none; transition: color 0.15s; }
  .alink:hover { text-decoration: underline; color: var(--amber-soft); }

  /* ── check-update button spinner ── */
  .spin {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 1.5px solid var(--text-faint);
    border-top-color: var(--amber);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    margin-right: 5px;
    vertical-align: -1px;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
