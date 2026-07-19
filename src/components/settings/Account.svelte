<script lang="ts">
  // 账号 (T5.4 placeholder). V1: vendor bindings (subscription / API key / cookie).
</script>
<div class="sh"><h3>账号</h3><div class="desc">大模型厂商账号关联与管理</div></div>
<div class="sc">
  <div class="desc" style="margin-bottom:12px">绑定厂商账号以获取额度信息；👁 控制前端可见</div>
  {#each [
    { n: "Claude Code / Anthropic", h: "OAuth · 已登录 ✓", btn: "已登录 ✓", c: "lime" },
    { n: "Codex / OpenAI", h: "OAuth · 已登录 ✓", btn: "已登录 ✓", c: "lime" },
    { n: "GLM / Z.ai", h: "API Key · 配置 →", btn: "配置 Key →", c: "amber" },
    { n: "DeepSeek", h: "API Key · 配置 →", btn: "配置 Key →", c: "amber" },
    { n: "Gemini / Google", h: "API Key · 配置 →", btn: "配置 Key →", c: "amber" },
  ] as v (v.n)}
    <div class="arow">
      <span class="lico">{v.n.slice(0,1)}</span>
      <div class="ainfo"><b>{v.n}</b><span class="h">{v.h}</span></div>
      <span class="eye on">👁</span>
      <button class="btn" style="color:var(--{v.c})">{v.btn}</button>
    </div>
  {/each}
</div>
<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .arow { display: flex; align-items: center; gap: 8px; padding: 9px 0; border-bottom: 1px solid var(--border-dim); }
  .lico { width: 24px; height: 24px; border-radius: 6px; background: rgba(232,176,75,0.12); color: var(--amber); display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 600; flex-shrink: 0; }
  .ainfo { flex: 1; min-width: 0; }
  .ainfo b { font-size: 11.5px; color: var(--text); display: block; }
  .ainfo .h { font-size: 10px; color: var(--text-faint); margin-top: 1px; }
  .eye { font-size: 12px; opacity: 0.35; }
  .eye.on { opacity: 1; color: var(--amber); }
  .btn { padding: 4px 10px; border-radius: 6px; font-family: inherit; font-size: 10.5px; cursor: pointer; background: transparent; border: 1px solid var(--border-dim); color: var(--text-faint); }
</style>
