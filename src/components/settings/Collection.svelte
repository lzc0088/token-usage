<script lang="ts">
  // 采集 (T5.3): tool status list from get_tools_status + tokscale version.
  import { api, type ClientStatus, type TokscaleStatus } from "../../lib/api";

  let tools = $state<ClientStatus[] | null>(null);
  let tok = $state<TokscaleStatus | null>(null);

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [t, s] = await Promise.all([api.getToolsStatus(), api.getTokscaleStatus()]);
        if (!cancelled) { tools = t; tok = s; }
      } catch (e) { console.error("collection load", e); if (!cancelled) { tools = null; tok = null; } }
    })();
    return () => { cancelled = true; };
  });

  function dot(status: string): string {
    if (status === "active") return "●";
    if (status === "waiting") return "◐";
    return "○";
  }
  function dotColor(status: string): string {
    if (status === "active") return "var(--lime)";
    if (status === "waiting") return "var(--amber)";
    return "var(--text-faint)";
  }
</script>

<div class="sh"><h3>采集</h3><div class="desc">tokscale 数据采集 · 工具追踪状态</div></div>
<div class="sc">
  <div class="gtitle">已发现工具 · {tools?.length ?? "—"}</div>
  <div class="desc" style="margin-bottom:6px">● 已追踪 · ◐ 等待数据 · ○ 未安装 · 消息数</div>

  {#if tools === null}
    <p class="loading">加载中…</p>
  {:else if tools.length === 0}
    <p class="empty">未检测到任何工具</p>
  {:else}
    {#each tools as t (t.client)}
      <div class="trow">
        <span class="tdot" style="color:{dotColor(t.status)}">{dot(t.status)}</span>
        <span class="tnm">{t.label}</span>
        <span class="tmsg">{t.message_count}</span>
        <span class="eye on">👁</span>
        <span class="tg"></span>
      </div>
    {/each}
  {/if}

  <div class="gtitle" style="margin-top:20px">tokscale</div>
  {#if tok === null}
    <span class="loading">…</span>
  {:else if tok.installed}
    <div class="row"><span class="lab">状态</span><span style="color:var(--lime)">已安装 {tok.version ?? ""}</span></div>
  {:else}
    <div class="row"><span class="lab">状态</span><span style="color:var(--coral)">未安装</span></div>
  {/if}
</div>

<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .gtitle { font-size: 13px; color: var(--text); margin: 14px 0 4px; }
  .trow { display: flex; align-items: center; gap: 7px; padding: 6px 0; border-bottom: 1px solid var(--border-dim); }
  .tdot { font-family:"JetBrains Mono",monospace; font-size: 10px; width: 16px; }
  .tnm { flex: 1; font-size: 12px; }
  .tmsg { font-family:"JetBrains Mono",monospace; font-size: 10px; color: var(--text-faint); }
  .eye { font-size: 12px; opacity: 0.35; }
  .eye.on { opacity: 1; color: var(--amber); }
  .tg { width: 26px; height: 14px; background: var(--amber); border: none; border-radius: 8px; position: relative; flex-shrink: 0; }
  .tg::after { content:""; position:absolute; top:1px; left:12px; width:10px; height:10px; background:#1a1408; border-radius:50%; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 8px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12px; }
  .loading, .empty { font-size: 11px; color: var(--text-faint); padding: 12px 0; }
</style>
