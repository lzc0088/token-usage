<script lang="ts">
  // 窗口 (T5.2): 菜单栏图标 + 行为 + 快捷键 (V1 display-only).
</script>
<div class="sh"><h3>窗口</h3><div class="desc">菜单栏图标与弹窗行为</div></div>
<div class="sc">
  <div class="row">
    <div class="lab">菜单栏图标显示内容</div>
    <select class="sel"><option>今日 token + 成本</option><option>今日 token</option><option>今日成本</option><option>总 token + 成本</option><option>总 token</option><option>总成本</option><option>仅图标</option></select>
  </div>
  <div class="row"><div class="lab">行为</div>
    <select class="sel"><option>单击弹出窗口</option><option>鼠标移上弹出窗口</option></select>
  </div>
  <div class="row"><div class="lab">弹窗失焦自动关闭</div><div class="tg on"></div></div>
  <div class="gtitle">快捷键</div>
  <div class="row"><div class="lab">显示 / 隐藏窗口</div><span class="key">⌥ ⌘ T</span></div>
  <div class="row"><div class="lab">立即刷新采集</div><span class="key">⌥ ⌘ R</span></div>
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 11px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12.5px; }
  .sel { background: var(--glass-2); border: 1px solid var(--border); color: var(--text); padding: 5px 10px; border-radius: 7px; font-family: inherit; font-size: 12px; }
  .tg { width: 36px; height: 20px; background: var(--amber); border: 1px solid var(--amber); border-radius: 11px; position: relative; flex-shrink: 0; }
  .tg::after { content:""; position:absolute; top:2px; left:18px; width:14px; height:14px; background:#1a1408; border-radius:50%; }
  .gtitle { font-size: 13px; color: var(--text); margin: 16px 0 4px; }
  .key { background: var(--glass-2); border: 1px solid var(--border); color: var(--amber); padding: 4px 10px; border-radius: 7px; font-family: "JetBrains Mono", var(--font-mono); font-size: 11px; }
</style>
