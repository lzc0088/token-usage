<script lang="ts">
  // 采集追踪: 工具状态列表，来自 tokscale clients 报告。
  import { listen } from "@tauri-apps/api/event";
  import type { Config } from "../../lib/api";
  import { api, type ClientStatus, type TokscaleStatus } from "../../lib/api";
  import { COLLECTION_UPDATED } from "../../lib/events";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();

  let tools = $state<ClientStatus[] | null>(null);
  let tok = $state<TokscaleStatus | null>(null);
  let tracked = $state<Set<string>>(new Set());
  let visible = $state<Set<string>>(new Set());
  let ordered = $state<string[]>([]);

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [t, s] = await Promise.all([api.getToolsStatus(), api.getTokscaleStatus()]);
        if (cancelled) return;
        tools = t;
        tok = s;
        // Default order is alphabetical by label; a saved custom order wins.
        const allClients = [...t]
          .sort((a, b) => a.label.localeCompare(b.label, "zh-Hans-CN"))
          .map((c) => c.client);
        const allSet = new Set(allClients);
        // Restore from config, fall back to all/auto
        tracked = new Set(config?.collection_tracked ?? allClients);
        visible = new Set(config?.collection_visible ?? allClients);
        ordered = [...(config?.collection_ordered ?? allClients)];
        // Append newly discovered tools (e.g. from a tokscale upgrade) that are
        // not yet in the saved order so they appear at the bottom of the list.
        for (const c of allClients) {
          if (!ordered.includes(c)) ordered.push(c);
        }
        // Prune tools that no longer exist (e.g. renamed/removed by tokscale).
        for (let i = ordered.length - 1; i >= 0; i--) {
          if (!allSet.has(ordered[i]!)) ordered.splice(i, 1);
        }
      } catch { /* handled by UI state */ }
    })();
    return () => { cancelled = true; };
  });

  
  function toggleTracked(key: string): void {
    const next = new Set(tracked);
    if (next.has(key)) next.delete(key); else next.add(key);
    tracked = next;
    onUpdate({ collection_tracked: [...next] });
  }
  function toggleVisible(key: string): void {
    const next = new Set(visible);
    if (next.has(key)) next.delete(key); else next.add(key);
    visible = next;
    onUpdate({ collection_visible: [...next] });
  }
  function move(i: number, dir: -1 | 1): void {
    const j = i + dir;
    if (j < 0 || j >= ordered.length) return;
    const arr = [...ordered];
    [arr[i], arr[j]] = [arr[j]!, arr[i]!];
    ordered = arr;
    onUpdate({ collection_ordered: arr });
  }

  // tokscale 版本号纯展示
  let tokscaleVersion = $derived(tok?.installed ? (tok.version ?? "") : "");
  let totalCount = $derived(tools?.length ?? 0);

  // ── 筛选 ──
  type FilterMode = "all" | "tracking" | "waiting" | "missing" | "disabled";
  let filter = $state<FilterMode>("all");

  /** Classify a tool into a filter category. 已停用 = installed but not tracked. */
  function toolFilter(t: ClientStatus): FilterMode {
    if (t.status === "missing") return "missing";
    if (!tracked.has(t.client)) return "disabled";
    return t.status === "active" ? "tracking" : "waiting";
  }

  let trackingCount = $derived(tools?.filter(t => tracked.has(t.client) && t.status === "active").length ?? 0);
  let waitingCount = $derived(tools?.filter(t => tracked.has(t.client) && t.status === "waiting").length ?? 0);
  let missingCount = $derived(tools?.filter(t => t.status === "missing").length ?? 0);
  let disabledCount = $derived(tools?.filter(t => t.status !== "missing" && !tracked.has(t.client)).length ?? 0);

  let filteredOrdered = $derived.by(() => {
    const list = tools;
    if (filter === "all" || !list) return ordered;
    return ordered.filter(key => {
      const t = list.find(c => c.client === key);
      return t && toolFilter(t) === filter;
    });
  });

  // 会话保留：从 config 读取，开关写入 config（持久化）。
  let keepDeleted = $derived(config?.session_archive_enabled ?? true);
  // 归档会话计数：来自后端（sessions 表中 tool 不在已安装列表的行数）。
  let archivedCount = $state<number | null>(null);
  let clearing = $state(false);

  async function loadArchivedCount(): Promise<void> {
    try {
      archivedCount = await api.getArchivedSessionCount();
    } catch (e) {
      /* archived count load failed — non-critical */
    }
  }

  async function clearArchived(): Promise<void> {
    if (archivedCount === 0) return;
    if (!window.confirm("要清除所有保留的会话用量吗？此操作无法撤销。")) return;
    clearing = true;
    try {
      await api.clearArchivedSessions();
      await loadArchivedCount();
    } catch (e) {
      /* clear archived failed — UI state handles this */
    } finally {
      clearing = false;
    }
  }

  $effect(() => {
    void loadArchivedCount();
    const un = listen<void>(COLLECTION_UPDATED, () => void loadArchivedCount());
    return () => { un.then((u) => u()); };
  });
</script>

<div class="sh"><h3>采集追踪</h3><div class="desc">本机 AI 工具发现、追踪状态与排序</div></div>
<div class="sc">

  <!-- ══ 基本 ══ -->
  <div class="section-title">基本</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">采集频率<div class="hint">数据采集与更新的时间间隔</div></div>
      <select class="sel" value={config.refresh_interval || "manual"}
        onchange={(e) => onUpdate({ refresh_interval: (e.target as HTMLSelectElement).value as Config["refresh_interval"] })}>
        <option value="manual">实时采集</option>
        <option value="30s">每 30 秒</option>
        <option value="60s">每 1 分钟</option>
        <option value="300s">每 5 分钟</option>
        <option value="600s">每 10 分钟</option>
      </select>
    </div>

    <div class="box-row">
      <div class="lab">会话保留<div class="hint">来源工具删除或清除会话后，仍保留会话总量与已观测的每日活动</div></div>
      <button type="button" class="tg" class:on={keepDeleted} role="switch" aria-checked={keepDeleted} aria-label="会话保留" onclick={() => onUpdate({ session_archive_enabled: !keepDeleted })}></button>
    </div>

    <div class="box-row">
      <div>
        <div class="lab">归档会话</div>
        <div class="hint" style="margin-top:2px">目前保留 <strong>{archivedCount ?? "…"}</strong> 个已归档会话</div>
      </div>
      <button type="button" class="btn-outline" onclick={clearArchived} disabled={clearing || !archivedCount}>
        {clearing ? "清除中…" : "清除保留数据"}
      </button>
    </div>

    <!-- Tokscale 状态（原独立 section，现汇总到基本） -->
    <div class="box-row tok-row">
      <div class="lab">Tokscale<div class="hint">token 采集引擎</div></div>
      {#if tok === null}
        <span class="tok-loading">…</span>
      {:else if tok.installed}
        <div class="tok-inline">
          <span class="tok-tag s-active">已安装</span>
          <span class="tok-ver">v{tokscaleVersion}</span>
          <span class="tok-badge">(随此 app 内置)</span>
        </div>
      {:else}
        <span class="tok-tag s-missing">未安装</span>
      {/if}
    </div>
  </div>

  <!-- ══ 工具 ══ -->
  <div class="section-title">工具
    <span class="fbar">
      <button type="button" class="fbtn" class:on={filter === "all"} onclick={() => (filter = "all")}>全部 {totalCount}</button>
      <button type="button" class="fbtn" class:on={filter === "tracking"} onclick={() => (filter = "tracking")}>追踪中 {trackingCount}</button>
      <button type="button" class="fbtn" class:on={filter === "waiting"} onclick={() => (filter = "waiting")}>等待数据 {waitingCount}</button>
      <button type="button" class="fbtn" class:on={filter === "disabled"} onclick={() => (filter = "disabled")}>已停用 {disabledCount}</button>
      <button type="button" class="fbtn" class:on={filter === "missing"} onclick={() => (filter = "missing")}>未安装 {missingCount}</button>
    </span>
  </div>
  <div class="section-box">
    <div class="icon-legend">
      <span class="legend-item">追踪</span>
      <span class="legend-item">显示</span>
      <span class="legend-item">上移</span>
      <span class="legend-item">下移</span>
    </div>

    {#if tools === null}
      <div class="skel-list">
        {#each [1,2,3,4] as _}
          <div class="skel-row">
            <div class="skel-icon"></div>
            <div class="skel-main">
              <div class="skel-line skel-w40"></div>
              <div class="skel-line skel-w60"></div>
            </div>
            <div class="skel-right">
              <div class="skel-dot"></div>
              <div class="skel-dot"></div>
              <div class="skel-dot"></div>
              <div class="skel-dot"></div>
            </div>
          </div>
        {/each}
      </div>
    {:else if tools.length === 0}
      <p class="empty">未检测到任何工具</p>
    {:else if filteredOrdered.length === 0}
      <p class="empty">该筛选下暂无工具</p>
    {:else}
      {#each filteredOrdered as key (key)}
        {@const t = tools.find(c => c.client === key)}
        {@const i = ordered.indexOf(key)}
        {#if t}
          <div class="trow">
            <div class="tleft">
              <ToolIcon vendor={t.client} size={22} color="var(--text-dim)" />
              <div class="tinfo">
                <span class="tname">{t.label}{#if t.diagnostics?.length}
                  <span class="t-diag" title={t.diagnostics.map(d => d.message).join("\n")}>ℹ</span>
                {/if}</span>
                <div class="trow-meta">
                  <span class="tstatus" class:s-active={t.status === "active"} class:s-waiting={t.status === "waiting"} class:s-missing={t.status !== "active" && t.status !== "waiting"}>{t.status === "active" ? "追踪中" : t.status === "waiting" ? "等待数据" : "未安装"}</span>
                  <span class="tmsg">{t.message_count} 条</span>
                </div>
              </div>
            </div>
            <span class="tright">
              <!-- 追踪 toggle -->
              <button type="button" class="ibtn ibtn-toggle" title={tracked.has(t.client) ? '已追踪' : '未追踪'} aria-label={tracked.has(t.client) ? '取消追踪' : '开始追踪'} onclick={() => toggleTracked(t.client)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  {#if tracked.has(t.client)}
                    <rect x="3" y="3" width="18" height="18" rx="4"/><polyline points="9 12 11 14 16 8"/>
                  {:else}
                    <rect x="3" y="3" width="18" height="18" rx="4"/>
                  {/if}
                </svg>
              </button>
              <!-- 可见 eye -->
              <button class="ibtn ibtn-vis" class:on={visible.has(t.client)} title={visible.has(t.client) ? '显示中' : '已隐藏'} aria-label={visible.has(t.client) ? '隐藏' : '显示'} onclick={() => toggleVisible(t.client)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  {#if visible.has(t.client)}
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
                  {:else}
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/>
                  {/if}
                </svg>
              </button>
              <!-- 排序 -->
              <button type="button" class="ibtn" title="上移" aria-label="上移" disabled={i === 0} onclick={() => move(i, -1)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
                </svg>
              </button>
              <button type="button" class="ibtn" title="下移" aria-label="下移" disabled={i === ordered.length - 1} onclick={() => move(i, 1)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>
                </svg>
              </button>
            </span>
          </div>
        {/if}
      {/each}
    {/if}
  </div>


</div>

<style>

  .sc { display: flex; flex-direction: column; }

  .section-title {
    font-size: 15px;
    margin-top: 20px;
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    row-gap: 6px;
  }
  .section-title:first-of-type { margin-top: 24px; }

  .fbar { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; justify-content: flex-end; }
  .fbtn {
    font-size: 10px;
    font-weight: 500;
    padding: 2px 8px;
    border-radius: 5px;
    border: 1px solid var(--border-dim);
    background: var(--surface-tint);
    color: var(--text-dim);
    cursor: pointer;
    font-family: inherit;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .fbtn:hover { border-color: var(--amber); color: var(--amber); }
  .fbtn.on {
    background: rgba(232,176,75,0.14);
    border-color: var(--amber);
    color: var(--amber);
  }

  /* ── skeleton loading ── */
  .skel-list { display: flex; flex-direction: column; width: 100%; }
  .skel-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px dashed var(--border);
  }
  .skel-row:last-child { border-bottom: none; }
  .skel-icon { width: 22px; height: 22px; border-radius: 6px; background: var(--surface-tint-strong); flex-shrink: 0; }
  .skel-main { display: flex; flex-direction: column; gap: 5px; flex: 1; }
  .skel-right { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
  .skel-dot { width: 22px; height: 22px; border-radius: 6px; background: var(--surface-tint); }
  .skel-line { height: 10px; border-radius: 4px; background: var(--surface-tint-strong); }
  .skel-w40 { width: 40%; }
  .skel-w60 { width: 60%; }
  .skel-icon, .skel-line, .skel-dot { animation: skel-pulse 1.4s ease-in-out infinite; }
  @keyframes skel-pulse { 0%,100% { opacity: 0.5; } 50% { opacity: 1; } }

  /* ── tool row ── */
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
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }
  .tname { font-size: 13px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t-diag { font-size: 11px; color: var(--amber); margin-left: 4px; cursor: help; }
  .tinfo { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .trow-meta { display: flex; align-items: center; gap: 6px; }
  .tstatus {
    font-size: 10.5px;
    font-weight: 500;
    padding: 1px 7px;
    border-radius: 5px;
    flex-shrink: 0;
    line-height: 1.4;
    background: var(--surface-tint);
    color: var(--text-faint);
  }
  .tstatus.s-active  { background: rgba(108,199,116,0.12); color: var(--lime); }
  .tstatus.s-waiting { background: rgba(232,176,75,0.12); color: var(--amber); }
  .tstatus.s-missing { background: rgba(234,84,85,0.12); color: var(--coral); }
  .tmsg {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 10.5px;
    font-weight: 500;
    padding: 2px 7px;
    border-radius: 5px;
    flex-shrink: 0;
    background: var(--surface-tint-strong);
    color: var(--text-dim);
  }

  .tright {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
  }

  /* ── icon legend ── */

  /* ── basic section ── */
  .box-row.tok-row { align-items: center; }
  .tok-inline { display: flex; align-items: center; gap: 8px; }
  .tok-loading { font-size: 11px; color: var(--text-faint); }
  .tok-tag { font-size: 10.5px; font-weight: 500; padding: 2px 7px; border-radius: 5px; }
  .tok-tag.s-active  { background: rgba(108,199,116,0.12); color: var(--lime); }
  .tok-tag.s-missing { background: rgba(224,108,117,0.12); color: var(--coral); }
  .tok-ver { font-family: "JetBrains Mono", var(--font-mono); font-size: 12px; color: var(--lime); }
  .tok-badge { font-size: 10.5px; color: var(--text-dim); background: rgba(255,255,255,0.05); padding: 2px 7px; border-radius: 5px; }

  .empty { font-size: 11px; color: var(--text-faint); padding: 12px 0; }

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
