<script lang="ts">
  // 常规 (T5.2): 启动 + 应用更新 + 关于. 参考 wireframe #general.
  import { untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import type { Config, UpdateInfo } from "../../lib/api";
  import { api } from "../../lib/api";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();

  // Git hosting repo from Vite env (full string, e.g. "gitee.com/owner/repo").
  const UPDATE_REPO = (import.meta.env.VITE_UPDATE_REPO as string) || "";

  // App version from package.json (injected by Vite env).
  const VERSION = (import.meta.env.VITE_APP_VERSION as string) || "0.1.0";

  // Derived repo info for the About links.
  const REPO_IS_GITEE = UPDATE_REPO.includes("gitee.com");
  const REPO_LABEL = REPO_IS_GITEE ? "Gitee" : "GitHub";
  const REPO_BASE_URL = REPO_IS_GITEE ? "https://gitee.com" : "https://github.com";
  const REPO_PATH = $derived.by(() => {
    const parts = UPDATE_REPO.split("/").filter(Boolean);
    if (parts.includes("gitee.com")) {
      return `${parts[parts.length - 2] ?? ""}/${parts[parts.length - 1] ?? ""}`;
    }
    return parts.slice(0, 2).join("/");
  });

  /** Open an external URL in the system's default browser. */
  function openExternal(url: string): void {
    if (url) open(url).catch(() => {});
  }

  let checking = $state(false);
  let updateStatus = $state<UpdateInfo | null>(null);
  let showLatestAlert = $state(false);
  let rateMode = $state<"auto" | "manual">(
    untrack(() => (config.rate_mode === "manual" ? "manual" : "auto")),
  );
  let manualRate = $state<string>("7.25");
  let savingRate = $state(false);
  let currentRate = $state<number>(7.25);
  let refreshingRate = $state(false);
  let showRateAlert = $state(false);
  let rateAlertMessage = $state("");
  let autoStartEnabled = $state<boolean>(untrack(() => !!config.auto_start));
  let autoStartToggling = $state(false);

  // 加载当前汇率
  function loadCurrentRate(): void {
    api.getExchangeRate().then((info) => {
      currentRate = info.rate;
      manualRate = info.rate.toFixed(2);
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
    }).catch((err) => {
      console.error("获取开机启动状态失败:", err);
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
      console.error("设置开机启动失败:", err);
    } finally {
      autoStartToggling = false;
    }
  }

  // 组件挂载时加载
  $effect(() => {
    loadCurrentRate();
    loadAutoStart();
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
        download_url: null,
      };
      checking = false;
      return;
    }

    api.checkUpdate(UPDATE_REPO, VERSION).then((info) => {
      updateStatus = info;
      checking = false;
      if (info.error) {
        return;
      }
      if (!info.has_update) {
        showLatestAlert = true;
        setTimeout(() => { showLatestAlert = false; }, 3000);
      }
    }).catch((e) => {
      updateStatus = {
        has_update: false, version: "", name: "", changelog: "",
        url: "", published_at: null,
        error: "调用失败：" + (e instanceof Error ? e.message : String(e)),
        download_url: null,
      };
      checking = false;
    });
  }

  function saveRate(): void {
    const num = parseFloat(manualRate);
    if (!validateRate(manualRate) || isNaN(num) || num <= 0) {
      rateAlertMessage = "请输入有效的正数汇率";
      showRateAlert = true;
      setTimeout(() => { showRateAlert = false; }, 4000);
      return;
    }
    savingRate = true;
    api.setManualRate(num).then(() => {
      currentRate = num;
      rateAlertMessage = `手动汇率已保存：1 USD = ${num.toFixed(2)} CNY`;
      showRateAlert = true;
      setTimeout(() => { showRateAlert = false; }, 3000);
    }).catch((err) => {
      console.error("保存汇率失败:", err);
      rateAlertMessage = `保存失败：${err}`;
      showRateAlert = true;
      setTimeout(() => { showRateAlert = false; }, 5000);
    }).finally(() => {
      savingRate = false;
    });
  }

  function refreshRate(): void {
    refreshingRate = true;
    api.refreshExchangeRate().then((info) => {
      currentRate = info.rate;
      manualRate = info.rate.toFixed(2);

      // 显示成功提示
      rateAlertMessage = `汇率已更新：1 USD ≈ ${info.rate.toFixed(2)} CNY${info.cached ? "（缓存）" : ""}`;
      showRateAlert = true;

      // 3秒后自动隐藏提示
      setTimeout(() => { showRateAlert = false; }, 3000);
    }).catch((err) => {
      console.error("刷新汇率失败:", err);
      rateAlertMessage = `汇率刷新失败：${err}`;
      showRateAlert = true;

      // 错误提示5秒后隐藏
      setTimeout(() => { showRateAlert = false; }, 5000);
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

<div class="sh"><h3>基本设置</h3><div class="desc">应用启动与数据基础行为</div></div>
<div class="sc">

  <!-- ══ 启动 ══ -->
  <div class="section-title">启动</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">开机自启动<div class="hint">登录时自动启动并驻留菜单栏</div></div>
      <button class="tg" class:on={autoStartEnabled} role="switch" aria-checked={autoStartEnabled} aria-label="开机自启动" onclick={toggleAutoStart} disabled={autoStartToggling}></button>
    </div>
  </div>

  <!-- ══ 币种 ══ -->
  <div class="section-title">币种</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">费用显示<div class="hint">项目中费用的显示币种</div></div>
      <select class="sel" onchange={(e) => {
        const target = e.target as HTMLSelectElement;
        onUpdate({ currency: target.value as 'cny' | 'usd' | 'both' });
      }}>
        <option value="cny" selected={config.currency === 'cny'}>CNY（人民币）</option>
        <option value="usd" selected={config.currency === 'usd'}>USD（美元）</option>
        <option value="both" selected={config.currency === 'both'}>同时显示</option>
      </select>
    </div>
    <div class="box-row">
      <div class="lab">汇率模式<div class="hint">USD ↔ CNY 汇率来源</div></div>
      <select class="sel" onchange={(e) => {
        const target = e.target as HTMLSelectElement;
        rateMode = target.value as 'auto' | 'manual';
        onUpdate({ rate_mode: target.value as "auto" | "manual" });
      }}>
        <option value="auto" selected={rateMode === 'auto'}>自动获取</option>
        <option value="manual" selected={rateMode === 'manual'}>手动输入</option>
      </select>
    </div>
    {#if rateMode === 'auto'}
      <div class="box-row" style="padding-top: 4px">
        <div class="lab">当前汇率<div class="hint">USD ↔ CNY 实时汇率</div></div>
        <div style="display: flex; align-items: center; gap: 8px;">
          <span class="rate-display">1 USD ≈ {currentRate.toFixed(2)} CNY</span>
          <button class="btn-outline" onclick={refreshRate} disabled={refreshingRate}>
            {refreshingRate ? "刷新中…" : "刷新"}
          </button>
        </div>
      </div>
    {:else}
      <div class="box-row" style="padding-top: 4px">
        <div class="lab">手动汇率<div class="hint">输入 1 USD 对应的 CNY 金额</div></div>
        <div style="display: flex; align-items: center; gap: 8px;">
          <input
            type="text"
            class="rate-input"
            value={manualRate}
            oninput={handleRateInput}
            placeholder="7.25"
            inputmode="decimal"
          />
          <button class="btn-save" onclick={saveRate} disabled={savingRate || !validateRate(manualRate)}>
            {savingRate ? "保存中…" : "保存"}
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
  <div class="section-title">更新</div>
  <div class="section-box no-divider">
    <div class="box-row">
      <div class="lab">
        当前版本：
        <span class="ver-num">{VERSION}</span>
      </div>
      <button class="btn-outline" onclick={checkUpdate} disabled={checking}>
        {checking ? "检查中…" : "检查更新"}
      </button>
    </div>

    {#if updateStatus}
      <div class="update-status" class:has-update={updateStatus.has_update} class:has-error={!!updateStatus.error}>
        {#if updateStatus.error}
          <div class="update-error">
            <span class="update-err-icon">⚠</span>
            <span class="update-err-text">{updateStatus.error}</span>
          </div>
        {:else if updateStatus.has_update}
          <div class="update-info">
            <div class="update-top">
              <span class="status-text">新版本：<span class="new-ver">{updateStatus.version}</span></span>
              {#if updateStatus.published_at}
                <span class="update-date">{new Date(updateStatus.published_at).toLocaleDateString("zh-CN")}</span>
              {/if}
            </div>
            {#if updateStatus.changelog}
              <div class="update-changelog">{updateStatus.changelog}</div>
            {/if}
            {#if updateStatus.download_url}
              <button class="btn-download" onclick={() => openExternal(updateStatus!.download_url!)}>
                下载更新
              </button>
            {:else}
              <button class="btn-download" onclick={() => openExternal(updateStatus!.url)}>
                前往下载
              </button>
            {/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if showLatestAlert}
      <div class="latest-alert">
        你当前使用的是最新版本
      </div>
    {/if}
  </div>

  <!-- ══ 关于 ══ -->
  <div class="section-title">关于</div>
  <div class="section-box">
    <p class="about-text">
      Token Usage 是一款跨平台菜单栏应用，用于实时统计本机各 AI 编码助手
      （Claude Code、Codex / ZCode / WorkBuddy …）的 token 用量与费用。
      数据 100% 本地存储，不上传云端。
    </p>
    <div class="about-links">
      {#if UPDATE_REPO}
        <a class="alink" href="{REPO_BASE_URL}/{REPO_PATH}" target="_blank" rel="noopener" onclick={(e) => { e.preventDefault(); openExternal(`${REPO_BASE_URL}/${REPO_PATH}`); }}>{REPO_LABEL}</a>
        <a class="alink" href="{REPO_BASE_URL}/{REPO_PATH}/issues/new" target="_blank" rel="noopener" onclick={(e) => { e.preventDefault(); openExternal(`${REPO_BASE_URL}/${REPO_PATH}/issues/new`); }}>报告问题</a>
      {/if}
    </div>
  </div>

</div>

<style>
  .sc { display: flex; flex-direction: column; }

  /* ── section title ── */
  .section-title {
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 15px;
    color: var(--amber);
    margin-top: 20px;
    margin-bottom: 8px;
  }
  .section-title:first-of-type {
    margin-top: 24px;
  }

  /* ── section box ── */
  .section-box {
    background: rgba(0,0,0,0.02);
    border: 1px solid var(--border-dim);
    border-radius: 10px;
    padding: 12px 14px;
  }

  /* ── box row ── */
  .box-row { display: flex; justify-content: space-between; align-items: center; padding: 9px 0; border-bottom: 1px dashed var(--border); gap: 16px; }
  .box-row:first-child { padding-top: 2px; }
  .box-row:last-child { border-bottom: none; padding-bottom: 2px; }
  .no-divider .box-row { border-bottom: none; }
  .no-divider .box-row:first-child { padding-top: 0; }
  .no-divider .box-row:last-child { padding-bottom: 0; }

  .lab { font-size: 13px; color: var(--text); }
  .lab .hint { font-size: 11px; color: var(--text-faint); margin-top: 2px; }

  /* ── select ── */
  .sel { background: rgba(255,255,255,.03); border: 1px solid var(--border-dim); color: var(--text); padding: 6px 10px; border-radius: 7px; font-size: 13px; cursor: pointer; font-family: inherit; min-width: 150px; height: 32px; }
  .sel:hover { border-color: var(--amber); }
  .sel:focus { outline: none; border-color: var(--amber); }

  /* ── rate display ── */
  .rate-display { font-family: var(--font-mono); font-size: 12px; color: var(--text); font-weight: 500; background: rgba(0,0,0,0.04); padding: 3px 8px; border-radius: 5px; }

  /* ── rate input ── */
  .rate-input { background: rgba(255,255,255,.03); border: 1px solid var(--border-dim); color: var(--text); padding: 6px 10px; border-radius: 7px; font-size: 13px; font-family: var(--font-mono); width: 80px; height: 32px; text-align: right; }
  .rate-input:focus { outline: none; border-color: var(--amber); background: rgba(255,255,255,.05); }
  .rate-input:hover { border-color: var(--amber); }
  .rate-input::placeholder { color: var(--text-faint); }

  /* ── save button ── */
  .btn-save { background: var(--amber); border: none; color: #1a1408; padding: 6px 14px; border-radius: 7px; font-size: 12px; font-weight: 600; cursor: pointer; font-family: inherit; min-width: 60px; height: 32px; }
  .btn-save:hover { opacity: 0.9; }
  .btn-save:disabled { opacity: 0.5; cursor: default; }

  /* ── rate alert ── */
  .rate-alert { padding: 8px 12px; margin-top: 6px; background: rgba(108, 199, 116, 0.08); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 6px; font-size: 11.5px; color: var(--text); animation: slideIn 0.3s ease-out; text-align: center; }
  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-5px); }
    to { opacity: 1; transform: translateY(0); }
  }

  /* ── toggle ── */
  .tg { width: 38px; height: 22px; background: var(--glass-3); border: 1px solid var(--border); border-radius: 12px; position: relative; cursor: pointer; flex-shrink: 0; display: block; }
  .tg.on { background: var(--amber); border-color: var(--amber); }
  .tg::after { content:""; position:absolute; top:2px; left:2px; width:16px; height:16px; background:var(--text); border-radius:50%; transition:.18s; }
  .tg.on::after { left:18px; background:#1a1408; }

  /* ── version number ── */
  .ver-num { font-family: var(--font-mono); font-size: 12px; color: var(--text); font-weight: 500; background: var(--glass-3); border: 1px solid var(--border-dim); padding: 2px 8px; border-radius: 5px; }

  /* ── update status ── */
  .update-status { padding: 10px 12px; margin: 6px 0; background: rgba(108, 199, 116, 0.08); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 7px; }
  .update-info { display: flex; flex-direction: column; gap: 6px; }
  .update-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .status-text { font-size: 12px; color: var(--text); }
  .new-ver { font-family: var(--font-mono); font-weight: 600; color: var(--lime); }
  .update-date { font-size: 10.5px; color: var(--text-faint); }
  .update-changelog {
    font-size: 11px; color: var(--text-dim); line-height: 1.7;
    max-height: 160px; overflow-y: auto;
    background: rgba(0,0,0,0.03); border-radius: 6px; padding: 8px 10px;
    white-space: pre-wrap; word-break: break-word;
  }
  .update-changelog::-webkit-scrollbar { width: 4px; }
  .update-changelog::-webkit-scrollbar-thumb { background: var(--glass-3); border-radius: 2px; }
  .btn-download {
    display: inline-flex; align-items: center; justify-content: center;
    background: var(--lime); border: none; color: #1a3a1a;
    padding: 5px 14px; border-radius: 6px; font-size: 11.5px; font-weight: 600;
    cursor: pointer; font-family: inherit; text-decoration: none;
    align-self: flex-start;
  }
  .btn-download:hover { opacity: 0.88; }

  /* ── update error state ── */
  .update-status.has-error {
    background: rgba(234, 84, 85, 0.08);
    border: 1px solid rgba(234, 84, 85, 0.3);
  }
  .update-error {
    display: flex; align-items: center; gap: 8px;
    font-size: 12px; color: var(--coral);
  }
  .update-err-icon { font-size: 14px; flex-shrink: 0; }
  .update-err-text { line-height: 1.5; }

  /* ── latest alert ── */
  .latest-alert { padding: 10px 12px; margin: 6px 0; background: rgba(108, 199, 116, 0.08); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 7px; font-size: 12px; color: var(--text); text-align: center; animation: slideIn 0.3s ease-out; }
  @keyframes slideIn {
    from { opacity: 0; transform: translateY(-10px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .btn-outline { background: rgba(255,255,255,.04); border: 1px solid var(--border-dim); color: var(--text-dim); padding: 5px 12px; border-radius: 7px; font-family: inherit; font-size: 11.5px; cursor: pointer; transition: all 0.15s; }
  .btn-outline:hover:not(:disabled) { border-color: var(--amber); color: var(--amber); background: rgba(232,176,75,0.08); }
  .btn-outline:disabled { opacity: 0.5; cursor: default; }
  .btn-outline:disabled:hover { border-color: var(--border-dim); color: var(--text-dim); background: rgba(255,255,255,.04); }

  /* ── update status ── */
  .update-status { padding: 10px 12px; margin: 6px 0; background: rgba(108, 199, 116, 0.08); border: 1px solid rgba(108, 199, 116, 0.3); border-radius: 7px; }
  .about-text { font-size: 13px; color: var(--text); line-height: 1.7; margin: 0 0 10px; }
  .about-links { display: flex; gap: 20px; }
  .alink { font-size: 12px; color: var(--amber); text-decoration: none; transition: color 0.15s; }
  .alink:hover { text-decoration: underline; color: #d4a030; }
</style>
