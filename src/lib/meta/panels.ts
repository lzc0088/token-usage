// Shared vendor panel metadata: console URLs, page labels, and cookie-hint text.
// Used by Account.svelte (settings) and quota pages (Limits, Overview) so the
// "open console" / "how to get cookie" guidance stays consistent.

export type PanelUrlSpec = string | { field: string; map: Record<string, string> };

export interface VendorPanel {
  url: PanelUrlSpec;
  pageLabel: string;
  enPageLabel?: string;
  hint: string;
  enHint?: string;
  extraUrl?: string;
  extraLabel?: string;
}

export function panelHint(p: VendorPanel, lang = "zh"): string {
  return lang === "en" && p.enHint ? p.enHint : p.hint;
}

export function panelLabel(p: VendorPanel, lang = "zh"): string {
  return lang === "en" && p.enPageLabel ? p.enPageLabel : p.pageLabel;
}

export const VENDOR_PANEL: Record<string, VendorPanel> = {
  deepseek:   { url: "https://platform.deepseek.com/api_keys", pageLabel: "DeepSeek API Keys", enPageLabel: "DeepSeek API Keys", hint: "在 DeepSeek 开放平台 → API Keys 页面创建或复制 API 密钥。", enHint: "Copy your API key from the DeepSeek Platform → API Keys page." },
  minimax:    { url: "https://platform.minimaxi.com/console/plan", pageLabel: "MiniMax 控制台", enPageLabel: "MiniMax Console", hint: "在 MiniMax 开放平台 → Token Plan 页面获取专用 Coding API 密钥。", enHint: "Get your coding API key from the MiniMax Platform → Token Plan page." },
  glm:        { url: "https://bigmodel.cn/apikey/platform", pageLabel: "智谱 API Key", enPageLabel: "ZhipuAI API Key", hint: "在智谱开放平台 → API Key 页面获取 GLM / Z.ai API 密钥（区分国际区 / 国内区）。", enHint: "Get your GLM / Z.ai API key from the ZhipuAI Platform → API Key page (distinguish international / domestic regions)." },
  kimi:       {
    url: "https://www.kimi.com/code/console",
    pageLabel: "Kimi Code 控制台", enPageLabel: "Kimi Code Console",
    hint: "连接 Kimi 网站登录状态，以显示 5 小时、每周和每月额度。Cookie 只会保存在本机。获取步骤：1) 在浏览器打开 Kimi Code 控制台（上方按钮）并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Cookies → https://www.kimi.com；3) 找到名为 kimi-auth 的 Cookie，复制它的 Value；4) 粘贴到下方后点击保存。",
    enHint: "Connect to Kimi website to display 5h/weekly/monthly limits. Steps: 1) Open Kimi Code Console (button above) in browser and log in; 2) DevTools (F12) → Application → Cookies → kimi.com; 3) Copy the Value of the cookie named kimi-auth; 4) Paste below and save.",
  },
  volcengine: { url: "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey", pageLabel: "火山方舟 - API Key", enPageLabel: "Volcengine Ark - API Key", hint: "在火山方舟控制台获取 Ark Key（ark-…）或 AK+SK（AKLT…+Secret）。区域默认 cn-beijing。", enHint: "Get your Ark Key (ark-…) or AK+SK (AKLT…+Secret) from the Volcengine Ark console. Region defaults to cn-beijing.", extraUrl: "https://console.volcengine.com/iam/keymanage", extraLabel: "火山方舟 - 密钥管理" },
  stepfun:   {
    url: "https://platform.stepfun.com/account-overview",
    pageLabel: "阶跃星辰 - 账户总览", enPageLabel: "StepFun Account",
    hint: "在浏览器打开 platform.stepfun.com 并登录，打开 DevTools（F12 或 Cmd+Opt+I）→ Network，刷新页面，找到 QueryAccountBalance 请求，复制其请求头 Cookie 字段完整值粘贴到下方。需包含 Oasis-Token 和 Oasis-Webid。注意：Cookie 有效期较短，过期后需重新获取。",
    enHint: "Open platform.stepfun.com in browser, log in, DevTools (F12) → Network, refresh, find QueryAccountBalance request, copy the full Cookie header. Must include Oasis-Token and Oasis-Webid. Note: cookies expire quickly, re-fetch when expired.",
  },
  iflytek:   {
    url: "https://maas.xfyun.cn/packageSubscription",
    pageLabel: "讯飞星辰 - 套餐订阅", enPageLabel: "iFlytek Subscription",
    hint: "在浏览器打开 maas.xfyun.cn 并登录，打开 DevTools（F12 或 Cmd+Opt+I）→ Network，刷新套餐页面，找到 coding-plan/list 请求，复制其请求头 Cookie 字段完整值粘贴到下方。需包含 ssoSessionId。",
    enHint: "Open maas.xfyun.cn in browser, log in, DevTools (F12) → Network, refresh, find coding-plan/list request, copy the full Cookie header. Must include ssoSessionId.",
  },
  mimo:       {
    url: "https://platform.xiaomimimo.com/#/console/balance",
    pageLabel: "MiMo 控制台", enPageLabel: "MiMo Console",
    hint: "在浏览器打开 MiMo 控制台并登录，打开 DevTools（F12 或 Cmd+Opt+I）→ Network，刷新页面，找到 `balance` 请求，在 Request Headers 中找到 Cookie 字段，复制其完整值粘贴到下方。需包含 api-platform_serviceToken 和 userId。",
    enHint: "Open MiMo Console in browser, log in, DevTools (F12) → Network, refresh, find `balance` request, copy the Cookie header. Must include api-platform_serviceToken and userId.",
  },
  opencode:   {
    url: "https://opencode.ai/auth",
    pageLabel: "OpenCode", enPageLabel: "OpenCode",
    hint: "OpenCode 额度通过网页会话读取，只需提供 auth 登录令牌（不是完整 Cookie 字符串）。获取步骤：1) 在浏览器打开 opencode.ai/auth（上方按钮）并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Cookies → https://opencode.ai；3) 找到名为 auth 的 Cookie，复制其 Value；4) 粘贴到下方后点击保存。注意：auth 令牌有效期较短，过期后需重新获取。",
    enHint: "OpenCode quota via web session (auth token only). Steps: 1) Open opencode.ai/auth (button above) and log in; 2) DevTools (F12) → Application → Cookies → opencode.ai; 3) Copy the Value of cookie named auth; 4) Paste below and save. Note: token expires quickly.",
  },
  zai_team:   {
    url: "https://bigmodel.cn/coding-plan/team/usage",
    pageLabel: "智谱团队控制台", enPageLabel: "ZhipuAI Team Console",
    hint: "团队套餐仅国内版（open.bigmodel.cn）提供，z.ai 国际版无团队套餐。粘贴 Z.ai API 密钥，用于查询 GLM 团队 Coding Plan 额度。获取组织 / 项目 ID：1) 在浏览器打开 BigModel（上方按钮）并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Local Storage → https://bigmodel.cn；3) 复制 Bigmodel-Organization 与 Bigmodel-Project 的值；4) 将它们与 API 密钥一起粘贴到下方，点击保存。",
    enHint: "Team plan is China-only (open.bigmodel.cn). Steps: 1) Open BigModel (button above) and log in; 2) DevTools (F12) → Application → Local Storage → bigmodel.cn; 3) Copy Bigmodel-Organization and Bigmodel-Project values; 4) Paste with API key below and save.",
  },
  qoder:      {
    url: { field: "site", map: { global: "https://qoder.com/account/usage", cn: "https://qoder.com.cn/account/usage" } },
    pageLabel: "Qoder 用量", enPageLabel: "Qoder Usage",
    hint: "在浏览器打开 Qoder 用量页面并登录（区分国际站 qoder.com / 中国站 qoder.com.cn），打开 DevTools（F12 或 Cmd+Opt+I）→ Network，刷新页面，找到 big_model_credits 请求，复制其请求头 Cookie 字段完整值粘贴到下方。",
    enHint: "Open Qoder usage page and log in (global: qoder.com / China: qoder.com.cn). DevTools (F12) → Network, refresh, find big_model_credits request, copy the full Cookie header.",
  },
  ollama:     {
    url: "https://ollama.com/settings",
    pageLabel: "Ollama Cloud", enPageLabel: "Ollama Cloud",
    hint: "Ollama Cloud 按量计费（无订阅套餐），额度通过网页 Cookie 读取。获取步骤：1) 浏览器打开 ollama.com 并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Cookies → https://ollama.com；3) 复制 session 相关 Cookie 的完整值（通常包含 ollama_session 或 connect.sid）粘贴到下方。注意：Cookie 有效期较短，过期后需重新获取；本地运行的 Ollama（无 Cloud）无需绑定，不计额度。",
    enHint: "Ollama Cloud is pay-as-you-go (no subscription). Steps: 1) Open ollama.com and log in; 2) DevTools (F12) → Application → Cookies → ollama.com; 3) Copy session cookie values. Note: local Ollama (no Cloud) does not need binding.",
  },
  cursor:     {
    url: "https://cursor.com/settings",
    pageLabel: "Cursor 设置", enPageLabel: "Cursor Settings",
    hint: "连接 Cursor 网站登录状态，以显示 Plan 用量。Cookie 只会保存在本机。获取步骤：1) 在浏览器打开 Cursor 设置页面（上方按钮）并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Cookies → https://cursor.com；3) 找到名为 WorkosCursorSessionToken 的 Cookie，复制它的 Value；4) 粘贴到下方后点击保存。注意：该令牌有效期较长，过期后需重新获取。",
    enHint: "Connect to Cursor login state to display Plan usage. Steps: 1) Open Cursor Settings (button above) and log in; 2) DevTools (F12) → Application → Cookies → cursor.com; 3) Copy the Value of cookie named WorkosCursorSessionToken; 4) Paste below and save.",
  },
  claude:     {
    url: "https://claude.ai/settings",
    pageLabel: "Claude 用量", enPageLabel: "Claude Usage",
    hint: "未设置 Web 登录时，会自动检测 Claude Code OAuth 与 CLI；添加 Web 登录后，本机 Claude 会改用此来源。Cookie 只会保存在本机。获取步骤：1) 在浏览器打开 Claude 用量页面（上方按钮）并登录；2) 打开 DevTools（F12 或 Cmd+Opt+I）→ Application（或 Storage）→ Cookies → https://claude.ai；3) 找到名为 sessionKey 的 Cookie，复制它的值；4) 粘贴到下方后点击保存。注意：Web 会话 Cookie 有效期较短，过期后需重新获取。",
    enHint: "Without Web login, Claude Code OAuth & CLI are auto-detected. Steps: 1) Open Claude Usage page (button above) and log in; 2) DevTools (F12) → Application → Cookies → claude.ai; 3) Copy the Value of cookie named sessionKey; 4) Paste below and save. Note: session cookie expires quickly.",
  },
  codex:      {
    url: "https://chatgpt.com/settings",
    pageLabel: "ChatGPT 设置", enPageLabel: "ChatGPT Settings",
    hint: "Codex 额度通过本地 CLI 凭证读取。获取步骤：1) 在终端运行 codex login 完成 OAuth 登录；2) 点击下方「立即检测」确认本机凭证已就绪；3) 额度数据将通过本地 CLI RPC + ChatGPT API 自动获取。",
    enHint: "Codex quota via local CLI credentials. Steps: 1) Run `codex login` in terminal; 2) Click below to detect; 3) Quota is fetched automatically via local CLI RPC + ChatGPT API.",
  },
  grok:       {
    url: "https://console.x.ai/",
    pageLabel: "xAI Console", enPageLabel: "xAI Console",
    hint: "在 xAI 控制台获取 API Key。",
    enHint: "Get your API key from the xAI Console.",
  },
  openrouter: {
    url: "https://openrouter.ai/keys",
    pageLabel: "OpenRouter Keys", enPageLabel: "OpenRouter Keys",
    hint: "在 OpenRouter 控制台 → Keys 页面创建或复制 API 密钥。",
    enHint: "Create or copy your API key from the OpenRouter Console → Keys page.",
  },
};

export function resolvePanelUrl(id: string): string {
  const panel = VENDOR_PANEL[id];
  if (!panel) return "";
  if (typeof panel.url === "string") return panel.url;
  // For dynamic URLs (e.g. qoder global/cn), fall back to the first map entry.
  const firstValue = Object.values(panel.url.map)[0];
  return firstValue ?? "";
}
