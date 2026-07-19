<script lang="ts">
  // 采集 (T5.3 placeholder). V1 shows tool status list (static) + tokscale install.
</script>
<div class="sh"><h3>采集</h3><div class="desc">tokscale 数据采集 · 工具追踪状态</div></div>
<div class="sc">
  <div class="gtitle">已发现工具</div>
  <div class="desc" style="margin-bottom:8px">● 已追踪 · ◐ 等待数据 · ○ 未安装</div>
  <!-- placeholder tool list — real data from get_tools_status in T5.3 full impl -->
  {#each [{ n: "Claude Code", s: "●" }, { n: "Codex", s: "●" }, { n: "Cursor", s: "◐" }, { n: "ZCode", s: "●" }, { n: "OpenCode", s: "●" }, { n: "Qoder", s: "○" }] as t (t.n)}
    <div class="trow"><span class="tdot" class:g={t.s==="●"} class:w={t.s==="◐"}>{t.s}</span><span class="tnm">{t.n}</span><span class="eye on">👁</span><span class="tg"></span></div>
  {/each}
  <div class="gtitle" style="margin-top:20px">tokscale</div>
  <div class="row"><div class="lab">状态</div><span style="color:var(--lime)">已安装 v4.5.3</span></div>
  <div class="row"><div class="lab">安装/更新</div><button class="btn">重试安装</button></div>
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .gtitle { font-size: 13px; color: var(--text); margin: 16px 0 6px; }
  .trow { display: flex; align-items: center; gap: 8px; padding: 7px 0; border-bottom: 1px solid var(--border-dim); }
  .tdot { font-family: "JetBrains Mono", monospace; font-size: 11px; color: var(--lime); width: 16px; }
  .tdot.g { color: var(--lime); } .tdot.w { color: var(--amber); }
  .tnm { flex: 1; font-size: 12px; }
  .eye { font-size: 12px; opacity: 0.35; }
  .eye.on { opacity: 1; color: var(--amber); }
  .tg { width: 26px; height: 14px; background: var(--amber); border-radius: 8px; position: relative; }
  .tg::after { content:""; position:absolute; top:1px; left:12px; width:10px; height:10px; background:#1a1408; border-radius:50%; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 10px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12.5px; }
  .btn { background: rgba(232,176,75,0.12); border: 1px solid var(--amber-soft); color: var(--amber); padding: 4px 12px; border-radius: 7px; font-family: inherit; font-size: 11.5px; cursor: pointer; }
</style>
