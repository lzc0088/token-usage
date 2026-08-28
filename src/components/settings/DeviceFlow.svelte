<script module lang="ts">
  /** Shared OAuth state used by both DeviceFlow display and Account login fns. */
  type OAuthPhase = "idle" | "requesting" | "authorize" | "polling" | "success" | "error";

  export interface OAuthState {
    phase: OAuthPhase;
    userCode?: string;
    verificationUrl?: string;
    loginUrl?: string;
    error?: string;
  }
</script>

<script lang="ts">
  import { api } from "../../lib/api";

  // Pure display component — only renders when OAuth is active (non-idle).
  // The idle "登录" button lives inline in Account.svelte so its onclick
  // calls the function directly without prop indirection.

  let {
    vendor,
    state,
    onRetry,
  }: {
    vendor: "copilot" | "codex";
    state: OAuthState;
    onRetry?: () => void;
  } = $props();
</script>

<div class="device-flow">
  {#if vendor === "copilot"}
    {#if state.phase === "requesting"}
      <p class="df-status">正在向 GitHub 请求设备码…</p>
    {:else if state.phase === "authorize" || state.phase === "polling"}
      <p class="df-status">请在浏览器中输入以下验证码完成授权：</p>
      <div class="df-code">{state.userCode ?? "…"}</div>
      <p class="df-hint">
        浏览器未自动打开？
        <button type="button" class="df-link" onclick={() => state.verificationUrl && api.openExternal(state.verificationUrl).catch(() => {})}>手动打开授权页面</button>
      </p>
      {#if state.phase === "polling"}
        <p class="df-polling">等待授权中…</p>
      {/if}
    {:else if state.phase === "success"}
      <p class="df-status df-ok">✓ 登录成功，已保存凭证</p>
    {:else if state.phase === "error"}
      <p class="df-status df-err">登录失败：{state.error}</p>
    {/if}
  {:else}
    {#if state.phase === "requesting"}
      <p class="df-status">正在启动 codex login…</p>
    {:else if state.phase === "authorize"}
      <p class="df-status">请在浏览器中完成 OpenAI 授权：</p>
      <div class="df-code">{state.loginUrl ?? "…"}</div>
      <p class="df-hint">
        浏览器未自动打开？
        <button type="button" class="df-link" onclick={() => state.loginUrl && api.openExternal(state.loginUrl).catch(() => {})}>手动打开授权页面</button>
      </p>
      <p class="df-polling">等待授权完成…</p>
    {:else if state.phase === "success"}
      <p class="df-status df-ok">✓ 登录成功，已保存凭证</p>
    {:else if state.phase === "error"}
      <p class="df-status df-err">登录失败：{state.error}</p>
    {/if}
  {/if}

  {#if state.phase === "error" && onRetry}
    <div class="panel-actions">
      <button type="button" class="btn-primary" onclick={onRetry}>重新登录</button>
    </div>
  {/if}
</div>

<style>
  .device-flow {
    padding: 10px 12px;
    margin: 0 0 10px;
    border: 1px dashed var(--border-dim);
    border-radius: 8px;
    background: var(--surface-tint);
  }
  .df-status { font-size: 0.8rem; color: var(--text); margin: 0 0 8px; line-height: 1.5; }
  .df-status.df-ok { color: var(--lime); margin: 0; }
  .df-status.df-err { color: var(--coral); margin: 0; }
  .df-code {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 1.467rem;
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
  .df-hint { font-size: 0.7333rem; color: var(--text-faint); margin: 0 0 6px; }
  .df-link {
    background: none;
    border: none;
    color: var(--amber);
    cursor: pointer;
    font-family: inherit;
    font-size: 0.7333rem;
    padding: 0;
    text-decoration: underline;
  }
  .df-polling {
    font-size: 0.7333rem;
    color: var(--text-dim);
    margin: 0;
    animation: df-pulse 1.4s ease-in-out infinite;
  }
  @keyframes df-pulse {
    0%, 100% { opacity: 0.5; }
    50% { opacity: 1; }
  }
</style>
