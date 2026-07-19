<script lang="ts">
  // 常规 (T5.2): 开机自启动 + 语言.
  import type { Config } from "../../lib/api";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();
</script>
<div class="sh"><h3>常规</h3><div class="desc">基础行为与偏好</div></div>
<div class="sc">
  <div class="row"><div class="lab">开机自启动</div><button class="tg" class:on={!!config.auto_start} onclick={() => onUpdate({ auto_start: !config.auto_start })} aria-label="开机自启动"></button></div>
  <div class="row">
    <div class="lab">语言<div class="hint">界面语言</div></div>
    <select class="sel" value={config.language ?? "zh"} onchange={(e) => onUpdate({ language: (e.target as HTMLSelectElement).value === "en" ? "en" : "zh" })}>
      <option value="zh">中文</option>
      <option value="en">English</option>
    </select>
  </div>
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; color: var(--text); }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .row { display: flex; justify-content: space-between; align-items: center; padding: 12px 0; border-bottom: 1px solid var(--border-dim); gap: 10px; }
  .lab { font-size: 12.5px; color: var(--text); }
  .lab .hint { font-size: 10.5px; color: var(--text-faint); margin-top: 1px; }
  .tg { width: 36px; height: 20px; background: var(--glass-3); border: 1px solid var(--border); border-radius: 11px; position: relative; cursor: pointer; flex-shrink: 0; display: block; }
  .tg.on { background: var(--amber); border-color: var(--amber); }
  .tg::after { content:""; position:absolute; top:2px; left:2px; width:14px; height:14px; background:var(--text); border-radius:50%; transition:.15s; }
  .tg.on::after { left:18px; background:#1a1408; }
  .sel { background: var(--glass-2); border: 1px solid var(--border); color: var(--text); padding: 5px 10px; border-radius: 7px; font-family: inherit; font-size: 12px; }
</style>
