<script lang="ts">
  // 主画面 (T5.2): 默认时段 + 分段导航 tree + 总览模块 tree (visual only, V1).
  import type { Config } from "../../lib/api";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();
</script>
<div class="sh"><h3>主画面</h3><div class="desc">弹窗界面布局与模块</div></div>
<div class="sc">
  <div class="row"><div class="lab">默认时段</div>
    <select class="sel" value={config.default_period ?? "day"} onchange={(e) => onUpdate({ default_period: (e.target as HTMLSelectElement).value as "day"|"month"|"total" })}>
      <option value="day">DAY（今日）</option><option value="month">MONTH（本月）</option><option value="total">TOTAL（全部）</option>
    </select>
  </div>

  <div class="gtitle">分段导航</div>
  <div class="desc">拖拽排序，👁 控制显示/隐藏</div>
  <div class="tree">
    <div class="tp"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">总览</span><span class="arrow">▾</span></div>
    <div class="tc">
      <div class="ti"><span class="eye on">👁</span><span class="nm">输入 / 输出 / 缓存分项</span></div>
      <div class="ti"><span class="eye on">👁</span><span class="nm">缓存命中率</span></div>
      <div class="ti"><span class="eye on">👁</span><span class="nm">工具</span></div>
      <div class="ti"><span class="eye on">👁</span><span class="nm">模型</span></div>
      <div class="ti"><span class="eye on">👁</span><span class="nm">额度</span></div>
      <div class="ti"><span class="eye">👁</span><span class="nm dim">趋势 mini</span></div>
    </div>
    <div class="ti"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">工具</span></div>
    <div class="ti"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">模型</span></div>
    <div class="ti"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">项目</span></div>
    <div class="ti"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">趋势</span></div>
    <div class="ti"><span class="drag">⠿</span><span class="eye">👁</span><span class="nm dim">会话</span></div>
    <div class="ti"><span class="drag">⠿</span><span class="eye on">👁</span><span class="nm">额度</span></div>
  </div>
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 12px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12.5px; }
  .sel { background: var(--glass-2); border: 1px solid var(--border); color: var(--text); padding: 5px 10px; border-radius: 7px; font-family: inherit; font-size: 12px; }
  .gtitle { font-size: 13px; color: var(--text); margin: 16px 0 2px; }
  .tree { margin-top: 6px; display: flex; flex-direction: column; gap: 2px; }
  .tp { display: flex; align-items: center; gap: 6px; padding: 5px 0; }
  .tc { padding-left: 20px; display: flex; flex-direction: column; gap: 2px; }
  .ti { display: flex; align-items: center; gap: 6px; padding: 4px 0; }
  .drag { color: var(--text-faint); cursor: grab; font-size: 13px; }
  .eye { font-size: 12px; opacity: 0.35; }
  .eye.on { opacity: 1; color: var(--amber); }
  .nm { font-size: 12px; color: var(--text); }
  .nm.dim { color: var(--text-faint); }
  .arrow { color: var(--amber); margin-left: auto; }
</style>
