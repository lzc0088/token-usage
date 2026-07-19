<script lang="ts">
  // 账号 (T5.4): vendor credential bindings — subscription (auto-detected),
  // API Key (keyring form), Cookie (guidance link). V1 ships DeepSeek form.
  import { api } from "../../lib/api";

  type VCat = "subscription" | "api-key" | "cookie";
  interface VendorDef { id: string; label: string; cat: VCat; hint: string; }

  const VENDORS: VendorDef[] = [
    { id: "claude", label: "Claude Code / Anthropic",  cat: "subscription", hint: "OAuth · 本地 CLI 凭证自动读取" },
    { id: "codex",  label: "Codex / OpenAI",           cat: "subscription", hint: "OAuth · 本地 CLI 凭证自动读取" },
    { id: "grok",   label: "Grok / xAI",               cat: "subscription", hint: "OAuth · 本地 CLI 凭证自动读取" },
    { id: "deepseek", label: "DeepSeek",                cat: "api-key",      hint: "API Key · 余额查询" },
    { id: "glm",     label: "GLM / Z.ai",              cat: "api-key",      hint: "API Key · Coding Plan" },
    { id: "minimax", label: "MiniMax",                  cat: "api-key",      hint: "专用 Coding API Key" },
    { id: "kimi",    label: "Kimi",                     cat: "api-key",      hint: "API Key · 计费" },
    { id: "volcengine", label: "火山引擎 Volcengine",    cat: "api-key",      hint: "Ark API Key 或 AK+SK" },
    { id: "copilot", label: "Copilot / GitHub",         cat: "api-key",      hint: "OAuth Token" },
    { id: "qoder",   label: "Qoder",                    cat: "cookie",       hint: "仪表盘 Cookie · 配置指引 →" },
    { id: "ollama",  label: "Ollama Cloud",             cat: "cookie",       hint: "会话 Cookie · 配置指引 →" },
  ];

  let bound = $state<Record<string, boolean>>({});
  let inputs = $state<Record<string, string>>({});
  let open = $state<Record<string, boolean>>({});

  // Fetch all vendors' binding status.
  $effect(() => {
    let cancelled = false;
    (async () => {
      const map: Record<string, boolean> = {};
      for (const v of VENDORS) {
        if (cancelled) return;
        try { map[v.id] = await api.getCredentialStatus(v.id); } catch { map[v.id] = false; }
      }
      if (!cancelled) bound = map;
    })();
    return () => { cancelled = true; };
  });

  async function save(vendor: string) {
    const key = (inputs[vendor] || "").trim();
    if (!key) return;
    try {
      await api.setCredential(vendor, key);
      bound = { ...bound, [vendor]: true };
      open = { ...open, [vendor]: false };
      inputs = { ...inputs, [vendor]: "" };
    } catch {}
  }
  async function remove(vendor: string) {
    try {
      await api.deleteCredential(vendor);
      bound = { ...bound, [vendor]: false };
    } catch {}
  }
</script>

<div class="sh"><h3>账号</h3><div class="desc">厂商账号关联 · 额度数据来源</div></div>
<div class="sc">
  {#each VENDORS as v (v.id)}
    <div class="arow">
      <span class="lico">{v.label.slice(0,1)}</span>
      <div class="ainfo">
        <b>{v.label}</b>
        <span class="h">{v.hint}</span>
      </div>
      {#if v.cat === "subscription"}
        <span class="badge lime">已检测 ✓</span>
      {:else if v.cat === "cookie"}
        <span class="badge dim">配置指引 →</span>
      {:else}
        {#if bound[v.id]}
          <span class="badge lime">已绑定</span>
          <button class="btn sm" onclick={() => remove(v.id)}>清除</button>
        {:else if open[v.id]}
          <div class="inp-wrap">
            <input class="inp" type="password" placeholder="API Key..." bind:value={() => inputs[v.id] ?? "", (val) => inputs = { ...inputs, [v.id]: val }} />
            <button class="btn sm" onclick={() => save(v.id)}>保存</button>
            <button class="btn sm" onclick={() => (open = { ...open, [v.id]: false })}>取消</button>
          </div>
        {:else}
          <button class="btn" onclick={() => (open = { ...open, [v.id]: true })}>配置 Key →</button>
        {/if}
      {/if}
    </div>
  {/each}
</div>

<style>
  .sh { padding: 18px 16px 12px; position: sticky; top: 0; background: var(--bg); z-index: 10; border-bottom: 1px solid var(--border-dim); }
  .sh h3 { font-size: 18px; margin: 0 0 2px; }
  .desc { font-size: 11.5px; color: var(--text-faint); }
  .sc { padding: 5px 16px 18px; display: flex; flex-direction: column; }
  .arow { display: flex; align-items: center; gap: 8px; padding: 8px 0; border-bottom: 1px solid var(--border-dim); flex-wrap: wrap; }
  .lico { width: 24px; height: 24px; border-radius: 6px; background: rgba(232,176,75,0.12); color: var(--amber); display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 600; flex-shrink: 0; }
  .ainfo { flex: 1; min-width: 0; }
  .ainfo b { font-size: 11.5px; color: var(--text); display: block; }
  .ainfo .h { font-size: 10px; color: var(--text-faint); margin-top: 1px; }
  .badge { font-size: 10px; padding: 2px 7px; border-radius: 5px; flex-shrink: 0; }
  .badge.lime { color: var(--lime); background: rgba(180,227,76,0.08); border: 1px solid rgba(180,227,76,0.2); }
  .badge.dim { color: var(--text-faint); }
  .btn { padding: 4px 10px; border-radius: 6px; font-family: inherit; font-size: 10.5px; cursor: pointer; border: 1px solid var(--amber-soft); color: var(--amber); background: rgba(232,176,75,0.08); }
  .btn.sm { font-size: 10px; padding: 3px 8px; }
  .inp-wrap { display: flex; gap: 4px; align-items: center; }
  .inp { background: var(--glass-2); border: 1px solid var(--border); color: var(--text); padding: 4px 8px; border-radius: 6px; font-family: var(--font-mono); font-size: 11px; width: 130px; }
</style>
