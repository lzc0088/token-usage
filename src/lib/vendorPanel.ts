// Shared vendor panel metadata: console URLs, page labels, and cookie-hint text.
// Used by Account.svelte (settings) and quota pages (Limits, Overview) so the
// "open console" / "how to get cookie" guidance stays consistent.

export type PanelUrlSpec = string | { field: string; map: Record<string, string> };

export interface VendorPanel {
  url: PanelUrlSpec;
  pageLabel: string;
  hint: string;
  extraUrl?: string;
  extraLabel?: string;
}

export const VENDOR_PANEL: Record<string, VendorPanel> = {
  deepseek:   { url: "https://platform.deepseek.com/api_keys", pageLabel: "DeepSeek API Keys", hint: "在 DeepSeek 开放平台 → API Keys 页面创建或复制 API 密钥。" },
  minimax:    { url: "https://platform.minimaxi.com/console/plan", pageLabel: "MiniMax 控制台", hint: "在 MiniMax 开放平台 → Token Plan 页面获取专用 Coding API 密钥。" },
  glm:        { url: "https://bigmodel.cn/apikey/platform", pageLabel: "智谱 API Key", hint: "在智谱开放平台 → API Key 页面获取 GLM / Z.ai API 密钥（区分国际区 / 国内区）。" },
  kimi:       { url: "https://www.kimi.com/code/console", pageLabel: "Kimi Code 控制台", hint: "在浏览器打开 Kimi Code 控制台并登录，打开 DevTools（F12）→ Application → Cookies → 复制 kimi-auth 的值粘贴到下方。" },
  volcengine: { url: "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey", pageLabel: "火山方舟 - API Key", hint: "在火山方舟控制台获取 Ark Key（ark-…）或 AK+SK（AKLT…+Secret）。区域默认 cn-beijing。", extraUrl: "https://console.volcengine.com/iam/keymanage", extraLabel: "火山方舟 - 密钥管理" },
  stepfun:   { url: "https://platform.stepfun.com/account-overview", pageLabel: "阶跃星辰 - 账户总览", hint: "在浏览器打开 platform.stepfun.com 并登录，打开 DevTools（F12）→ Network，刷新页面，找到 QueryAccountBalance 请求，复制其请求头 Cookie 字段完整值粘贴到下方。需包含 Oasis-Token 和 Oasis-Webid。注意：Cookie 有效期较短，过期后需重新获取。" },
  iflytek:   { url: "https://maas.xfyun.cn/packageSubscription", pageLabel: "讯飞星辰 - 套餐订阅", hint: "在浏览器打开 maas.xfyun.cn 并登录，打开 DevTools（F12）→ Network，刷新套餐页面，找到 coding-plan/list 请求，复制其请求头 Cookie 字段完整值粘贴到下方。需包含 ssoSessionId。" },
  mimo:       { url: "https://platform.xiaomimimo.com/#/console/balance", pageLabel: "MiMo 控制台", hint: "在浏览器打开 MiMo 控制台并登录，打开 DevTools（F12）→ Network，刷新页面，找到 `balance` 请求，在 Request Headers 中找到 Cookie 字段，复制其完整值粘贴到下方。需包含 api-platform_serviceToken 和 userId。" },
  opencode:   { url: "https://opencode.ai/", pageLabel: "OpenCode", hint: "在 OpenCode Web 面板获取会话 Cookie 后粘贴到下方。" },
  zai_team:   { url: "https://bigmodel.cn/coding-plan/team/usage", pageLabel: "智谱团队控制台", hint: "粘贴团队 Key、组织 ID、项目 ID。它们只保存在本机，仅用于团队 Coding Plan 额度。" },
  qoder:      { url: { field: "site", map: { global: "https://qoder.com", cn: "https://qoder.com.cn" } }, pageLabel: "Qoder 仪表盘", hint: "在 Qoder 仪表盘获取 Cookie（区分国际站 / 中国站）后粘贴到下方。" },
  ollama:     { url: "https://ollama.com/", pageLabel: "Ollama", hint: "在 Ollama Cloud 获取会话 Cookie 后粘贴到下方，用于查询周用量。" },
};

export function resolvePanelUrl(id: string): string {
  const panel = VENDOR_PANEL[id];
  if (!panel) return "";
  if (typeof panel.url === "string") return panel.url;
  // For dynamic URLs (e.g. qoder global/cn), fall back to the first map entry.
  const firstValue = Object.values(panel.url.map)[0];
  return firstValue ?? "";
}
