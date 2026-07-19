<script lang="ts">
  // 外观 (T5.2): 主题 + 密度 + 数字格式 + 强调色 + 货币.
  import type { Config } from "../../lib/api";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();
  const cy = $derived(config.currency ?? "both");
</script>
<div class="sh"><h3>外观</h3><div class="desc">主题、信息密度与显示格式</div></div>
<div class="sc">
  <div class="row"><div class="lab">主题<div class="hint">浅色 / 深色 / 跟随系统</div></div>
    <div class="sw-row">
      <button class="sw on">深色</button><button class="sw">浅色</button><button class="sw">跟随系统</button>
    </div>
  </div>
  <div class="row"><div class="lab">信息密度</div>
    <div class="sw-row">
      <button class="sw">舒适</button><button class="sw on">紧凑</button>
    </div>
  </div>
  <div class="row"><div class="lab">数字格式</div>
    <select class="sel"><option>1.84M（万进制）</option><option>184 万</option><option>1,840,000</option></select>
  </div>
  <div class="gtitle">配色与视觉效果</div>
  <div class="row"><div class="lab">强调色</div>
    <div class="clr-row"><span class="clr on" style="background:#e8b04b"></span><span class="clr" style="background:#b4e34c"></span><span class="clr" style="background:#7fd1d3"></span><span class="clr" style="background:#b69be0"></span><span class="clr" style="background:#f08e6a"></span></div>
  </div>
  <div class="row"><div class="lab">工具图标使用厂商色</div><div class="tg on"></div></div>
  <div class="row"><div class="lab">毛玻璃透明度</div><span class="rng"><span class="val">60%</span></span></div>
  <div class="row"><div class="lab">圆角半径</div><select class="sel"><option>中等</option><option>大</option><option>直角</option></select></div>
  <div class="row"><div class="lab">动画</div><div class="tg on"></div></div>

  <div class="gtitle">货币</div>
  <div class="desc">成本显示币种</div>
  <div class="sw-row" style="margin-top:6px">
    <button class="sw" class:on={cy==="usd"} onclick={()=>onUpdate({currency:"usd"})}>USD</button>
    <button class="sw" class:on={cy==="cny"} onclick={()=>onUpdate({currency:"cny"})}>CNY</button>
    <button class="sw" class:on={cy==="both"} onclick={()=>onUpdate({currency:"both"})}>双显</button>
  </div>
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 11px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12.5px; }
  .lab .hint { font-size: 10.5px; color: var(--text-faint); margin-top: 1px; }
  .sw-row { display: inline-flex; background: var(--glass-2); border: 1px solid var(--border); border-radius: 8px; padding: 2px; gap: 1px; }
  .sw { background: transparent; border: none; color: var(--text-faint); padding: 4px 9px; border-radius: 6px; font-family: inherit; font-size: 11.5px; cursor: pointer; }
  .sw.on { background: var(--amber); color: #1a1408; }
  .sel { background: var(--glass-2); border: 1px solid var(--border); color: var(--text); padding: 5px 10px; border-radius: 7px; font-family: inherit; font-size: 12px; }
  .tg { width: 36px; height: 20px; background: var(--amber); border: 1px solid var(--amber); border-radius: 11px; position: relative; flex-shrink: 0; }
  .tg::after { content:""; position:absolute; top:2px; left:18px; width:14px; height:14px; background:#1a1408; border-radius:50%; }
  .gtitle { font-size: 13px; color: var(--text); margin: 16px 0 4px; }
  .clr-row { display: flex; gap: 7px; }
  .clr { width: 24px; height: 24px; border-radius: 7px; cursor: pointer; border: 2px solid transparent; }
  .clr.on { border-color: var(--text); }
  .rng { font-family: var(--font-mono); font-size: 11px; color: var(--text-dim); }
</style>
