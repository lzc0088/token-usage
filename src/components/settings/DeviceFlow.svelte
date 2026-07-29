<script module>
  // Copilot OAuth Device Flow types (exported for Account.svelte).
  export interface CopilotLoginState {
    phase: "requesting" | "authorize" | "polling" | "success" | "error";
    userCode?: string;
    verificationUrl?: string;
    error?: string;
  }

  // Codex OAuth Login Flow types (exported for Account.svelte).
  export interface CodexLoginState {
    phase: "requesting" | "authorize" | "success" | "error";
    loginUrl?: string;
    error?: string;
  }
</script>

<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../../lib/api";

  // ── Timer cleanup (prevent setState on unmounted component) ──
  const timeouts = new Set<number>();
  $effect(() => {
    return () => { for (const id of timeouts) clearTimeout(id); };
  });

  // ── Copilot OAuth Device Flow ──
  let copilotLogin: CopilotLoginState | null = $state(null);

  export async function startCopilotLogin(): Promise<void> {
    copilotLogin = { phase: "requesting" };
    const un = await listen<{
      phase: string;
      user_code?: string;
      verification_url?: string;
    }>("copilot:login_status", (e) => {
      const p = e.payload;
      if (p.phase === "authorize") {
        copilotLogin = {
          phase: "authorize",
          userCode: p.user_code,
          verificationUrl: p.verification_url,
        };
        if (p.verification_url) api.openExternal(p.verification_url).catch(() => {});
      } else if (p.phase === "polling") {
        copilotLogin = { ...copilotLogin!, phase: "polling" };
      }
    });
    try {
      const token = await api.copilotLogin();
      await api.setCredential("copilot", JSON.stringify({ key: token }));
      copilotLogin = { phase: "success" };
      const id = window.setTimeout(() => { copilotLogin = null; }, 2500);
      timeouts.add(id);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      copilotLogin = { phase: "error", error: msg };
    } finally {
      un();
    }
  }

  // ── Codex OAuth Login Flow ──
  let codexLogin: CodexLoginState | null = $state(null);

  export async function startCodexLogin(): Promise<void> {
    codexLogin = { phase: "requesting" };
    const un = await listen<{
      phase: string;
      login_url?: string;
      message?: string;
    }>("codex:login_status", (e) => {
      const p = e.payload;
      if (p.phase === "authorize" && p.login_url) {
        codexLogin = {
          phase: "authorize",
          loginUrl: p.login_url,
        };
        api.openExternal(p.login_url).catch(() => {});
      } else if (p.phase === "success") {
        codexLogin = { phase: "success" };
        const id = window.setTimeout(() => { codexLogin = null; }, 2500);
        timeouts.add(id);
      } else if (p.phase === "error") {
        codexLogin = { phase: "error", error: p.message };
      }
    });
    try {
      await api.codexLogin();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      codexLogin = { phase: "error", error: msg };
    } finally {
      un();
    }
  }
</script>

{#if copilotLogin}
  <div class="device-flow">
    {#if copilotLogin.phase === "requesting"}
      <p class="df-status">正在向 GitHub 请求设备码…</p>
    {:else if copilotLogin.phase === "authorize" || copilotLogin.phase === "polling"}
      <p class="df-status">请在浏览器中输入以下验证码完成授权：</p>
      <div class="df-code">{copilotLogin.userCode ?? "…"}</div>
      <p class="df-hint">
        浏览器未自动打开？
        <button type="button" class="df-link" onclick={() => copilotLogin?.verificationUrl && api.openExternal(copilotLogin.verificationUrl).catch(() => {})}>手动打开授权页面</button>
      </p>
      <p class="df-polling">等待授权中…</p>
    {:else if copilotLogin.phase === "success"}
      <p class="df-status df-ok">✓ 登录成功，已保存凭证</p>
    {:else if copilotLogin.phase === "error"}
      <p class="df-status df-err">登录失败：{copilotLogin.error}</p>
    {/if}
    {#if copilotLogin.phase === "error"}
      <div class="panel-actions">
        <button type="button" class="btn-primary" onclick={() => startCopilotLogin()}>重新登录</button>
      </div>
    {/if}
  </div>
{/if}

{#if codexLogin}
  <div class="device-flow">
    {#if codexLogin.phase === "requesting"}
      <p class="df-status">正在启动 codex login…</p>
    {:else if codexLogin.phase === "authorize"}
      <p class="df-status">请在浏览器中完成 OpenAI 授权：</p>
      <div class="df-code">{codexLogin.loginUrl ?? "…"}</div>
      <p class="df-hint">
        浏览器未自动打开？
        <button type="button" class="df-link" onclick={() => codexLogin?.loginUrl && api.openExternal(codexLogin.loginUrl).catch(() => {})}>手动打开授权页面</button>
      </p>
      <p class="df-polling">等待授权完成…</p>
    {:else if codexLogin.phase === "success"}
      <p class="df-status df-ok">✓ 登录成功，已保存凭证</p>
    {:else if codexLogin.phase === "error"}
      <p class="df-status df-err">登录失败：{codexLogin.error}</p>
    {/if}
    {#if codexLogin.phase === "error"}
      <div class="panel-actions">
        <button type="button" class="btn-primary" onclick={() => startCodexLogin()}>重新登录</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .device-flow {
    padding: 10px 12px;
    margin: 0 0 10px;
    border: 1px dashed var(--border-dim);
    border-radius: 8px;
    background: var(--surface-tint);
  }
  .df-status { font-size: 12px; color: var(--text); margin: 0 0 8px; line-height: 1.5; }
  .df-status.df-ok { color: var(--lime); margin: 0; }
  .df-status.df-err { color: var(--coral); margin: 0; }
  .df-code {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 4px;
    color: var(--amber);
    text-align: center;
    padding: 8px 0;
    background: var(--amber-hover);
    border-radius: 6px;
    margin-bottom: 8px;
    user-select: all;
  }
  .df-hint { font-size: 11px; color: var(--text-faint); margin: 0 0 6px; }
  .df-link {
    background: none;
    border: none;
    color: var(--amber);
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    padding: 0;
    text-decoration: underline;
  }
  .df-polling {
    font-size: 11px;
    color: var(--text-dim);
    margin: 0;
    animation: df-pulse 1.4s ease-in-out infinite;
  }
  @keyframes df-pulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }
</style>
