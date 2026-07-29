// Shared vendor display-name mapping. Single source of truth for all
// components that render quota vendor names (Limits, Overview, MainView, …).
// Keys match the raw vendor IDs returned by Rust quota adapters.

export const VENDOR_LABELS: Record<string, string> = {
  deepseek: "DeepSeek ( 深度求索 )",
  glm: "GLM ( 智谱 )",
  minimax: "MiniMax ( 稀宇 )",
  kimi: "Kimi ( 月之暗面 )",
  volcengine: "Volcengine ( 火山方舟 )",
  mimo: "MiMo ( 小米 )",
  stepfun: "StepFun ( 阶跃星辰 )",
  iflytek: "iFlytek ( 讯飞星辰 )",
  copilot: "GitHub Copilot",
  zai_team: "GLM Team ( 智谱团队 )",
  claude: "Claude Code ( Anthropic )",
  codex: "Codex ( OpenAI )",
  opencode: "OpenCode ( OpenCode AI )",
  qoder: "Qoder ( 阿里 )",
  ollama: "Ollama ( Ollama Cloud )",
  cursor: "Cursor ( Anysphere )",
};

// ── Full vendor definitions for the Account (账号额度) settings page ────

export type AuthType = "detect" | "login" | "key" | "cookie";
export type TagColor = "blue" | "amber" | "purple" | "lime" | "coral" | "gray";

export interface InfoTag {
  text: string;
  color: TagColor;
}

export interface FieldDef {
  key: string;
  label: string;
  placeholder: string;
  type?: "text" | "password" | "select" | "textarea";
  options?: string[];
  default?: string;
}

export interface VendorDef {
  id: string;
  label: string;
  cat: "subscription" | "api-key" | "cookie";
  billing: string[]; // 按量, Token Plan, Coding Plan, Team Plan, 订阅
  authType: AuthType;
  desc: string;
  tags: InfoTag[];
  fields?: FieldDef[];
  loginLabel?: string;
}

const DEFAULT_KEY_FIELD: FieldDef = {
  key: "key",
  label: "API Key",
  placeholder: "sk-…",
  type: "password",
};

const DEFAULT_COOKIE_FIELD: FieldDef = {
  key: "cookie",
  label: "Cookie",
  placeholder: "粘贴 Cookie…",
  type: "textarea",
};

export function fieldsFor(v: VendorDef): FieldDef[] {
  if (v.fields) return v.fields;
  if (v.authType === "cookie") return [DEFAULT_COOKIE_FIELD];
  return [DEFAULT_KEY_FIELD];
}

export const VENDORS: VendorDef[] = [
  // ① 订阅制 — 自动检测 / Cookie 粘贴
  {
    id: "claude",
    label: "Claude Code ( Anthropic )",
    cat: "cookie",
    billing: ["订阅"],
    authType: "detect",
    desc: "未设置 Web 登录时，会自动检测 Claude Code OAuth 与 CLI；添加 Web 登录后，本机 Claude 会改用此来源。Cookie 只会保存在本机。",
    tags: [
      { text: "5h 窗口", color: "amber" },
      { text: "周窗口", color: "amber" },
    ],
    fields: [
      { key: "cookie", label: "Cookie", placeholder: "粘贴 sessionKey 的值…", type: "textarea" },
    ],
    loginLabel: "运行 claude /login",
  },
  {
    id: "codex",
    label: "Codex ( OpenAI )",
    cat: "subscription",
    billing: ["订阅"],
    authType: "detect",
    desc: "未设置 Codex CLI 登录时，会自动检测本地凭证；添加 Web 登录后，本机会改用此来源。",
    tags: [
      { text: "5h 窗口", color: "amber" },
      { text: "周窗口", color: "amber" },
    ],
    loginLabel: "运行 codex /login",
  },
  {
    id: "cursor",
    label: "Cursor ( Anysphere )",
    cat: "cookie",
    billing: ["订阅"],
    authType: "cookie",
    desc: "Cursor IDE 订阅，粘贴浏览器 WorkosCursorSessionToken Cookie 值查询用量",
    tags: [{ text: "账单周期", color: "amber" }],
    fields: [
      {
        key: "cookie",
        label: "Session Token",
        placeholder: "粘贴 WorkosCursorSessionToken 的值…",
        type: "textarea",
      },
    ],
  },
  // ② API Key — 表单填入
  {
    id: "deepseek",
    label: "DeepSeek ( 深度求索 )",
    cat: "api-key",
    billing: ["按量"],
    authType: "key",
    desc: "按量付费，查询账户余额",
    tags: [{ text: "余额", color: "lime" }],
  },
  {
    id: "minimax",
    label: "MiniMax ( 稀宇 )",
    cat: "api-key",
    billing: ["Token Plan", "按量"],
    authType: "key",
    desc: "Coding Plan，需专用 Coding Key，按 Token 额度统计；亦支持按量付费",
    tags: [
      { text: "Coding Key", color: "coral" },
      { text: "Token Plan", color: "amber" },
    ],
  },
  {
    id: "glm",
    label: "GLM ( 智谱 )",
    cat: "api-key",
    billing: ["Coding Plan", "按量"],
    authType: "key",
    desc: "Coding Plan，区分国际区 / 国内区，三窗口额度；亦支持按量资源包",
    tags: [
      { text: "区域", color: "purple" },
      { text: "5h", color: "amber" },
      { text: "周", color: "amber" },
      { text: "MCP月", color: "lime" },
    ],
    fields: [
      { key: "key", label: "API Key", placeholder: "ZAI/GLM Key…", type: "password" },
      {
        key: "region",
        label: "区域",
        placeholder: "",
        type: "select",
        options: ["global", "bigmodel-cn"],
        default: "bigmodel-cn",
      },
    ],
  },
  {
    id: "kimi",
    label: "Kimi ( 月之暗面 )",
    cat: "cookie",
    billing: ["按量", "Token Plan"],
    authType: "cookie",
    desc: "从浏览器 Application → Cookies 复制 kimi-auth 值，获取 5h/周/月完整额度",
    tags: [
      { text: "5h", color: "amber" },
      { text: "周", color: "amber" },
      { text: "月", color: "lime" },
    ],
    fields: [
      { key: "cookie", label: "Cookie", placeholder: "粘贴 kimi-auth 的值…", type: "textarea" },
    ],
  },
  {
    id: "volcengine",
    label: "Volcengine ( 火山方舟 )",
    cat: "api-key",
    billing: ["Coding Plan", "按量"],
    authType: "key",
    desc: "Ark Key 读取流量限制 · 可选 Cookie 显示订阅到期日期",
    tags: [
      { text: "AK+SK", color: "purple" },
      { text: "5h", color: "amber" },
      { text: "周", color: "amber" },
      { text: "月", color: "lime" },
      { text: "区域", color: "blue" },
    ],
    fields: [
      { key: "key", label: "Ark Key / AK", placeholder: "ark-… 或 AKLT…", type: "password" },
      {
        key: "secret",
        label: "Secret（AK+SK 时需要）",
        placeholder: "配合 AKLT 使用",
        type: "password",
      },
      {
        key: "region",
        label: "区域",
        placeholder: "",
        type: "select",
        options: ["cn-beijing"],
        default: "cn-beijing",
      },
      {
        key: "cookie",
        label: "控制台 Cookie（可选）",
        placeholder: "粘贴 console.volcengine.com 的 Cookie（含 csrfToken），用于显示到期日期",
        type: "textarea",
      },
    ],
  },
  {
    id: "stepfun",
    label: "StepFun ( 阶跃星辰 )",
    cat: "cookie",
    billing: ["Step Plan", "按量"],
    authType: "cookie",
    desc: "阶跃星辰 StepFun，粘贴 platform.stepfun.com 控制台 Cookie，查询账户余额与 Step Plan Credit",
    tags: [
      { text: "Step Plan", color: "amber" },
      { text: "余额", color: "lime" },
    ],
    fields: [
      {
        key: "cookie",
        label: "Cookie",
        placeholder: "粘贴 platform.stepfun.com 的 Cookie（含 Oasis-Token、Oasis-Webid）…",
        type: "textarea",
      },
    ],
  },
  {
    id: "iflytek",
    label: "iFlytek ( 讯飞星辰 )",
    cat: "cookie",
    billing: ["Token Plan", "按量"],
    authType: "cookie",
    desc: "讯飞星辰 MaaS（Astron），粘贴控制台 Cookie（含 ssoSessionId），获取 Coding Plan 套餐到期与用量",
    tags: [
      { text: "Token Plan", color: "amber" },
      { text: "余额", color: "lime" },
    ],
    fields: [
      {
        key: "cookie",
        label: "Cookie",
        placeholder: "粘贴 maas.xfyun.cn 控制台 Cookie（含 ssoSessionId）…",
        type: "textarea",
      },
    ],
  },
  {
    id: "copilot",
    label: "GitHub Copilot",
    cat: "subscription",
    billing: ["订阅"],
    authType: "login",
    desc: "GitHub 账号 OAuth 授权，显示 Premium / Chat 额度",
    tags: [
      { text: "Premium", color: "amber" },
      { text: "Chat", color: "blue" },
    ],
    loginLabel: "GitHub 登录",
  },
  {
    id: "mimo",
    label: "MiMo ( 小米 )",
    cat: "cookie",
    billing: ["Token Plan", "按量"],
    authType: "cookie",
    desc: "小米 MiMo，粘贴浏览器 Cookie 获取余额与套餐额度，支持 Token Plan 与按量",
    tags: [
      { text: "余额", color: "lime" },
      { text: "Token Plan", color: "amber" },
    ],
  },
  // ③ Cookie — 粘贴
  {
    id: "opencode",
    label: "OpenCode ( OpenCode AI )",
    cat: "cookie",
    billing: ["按量"],
    authType: "cookie",
    desc: "Go / Zen Web 面板，粘贴会话 Cookie，支持 5h / 周 / 月额度与余额",
    tags: [
      { text: "5h 窗口", color: "amber" },
      { text: "周窗口", color: "amber" },
      { text: "月窗口", color: "amber" },
      { text: "余额", color: "lime" },
    ],
  },
  {
    id: "zai_team",
    label: "GLM Team ( 智谱团队 )",
    cat: "cookie",
    billing: ["Team Plan"],
    authType: "cookie",
    desc: "智谱团队计划，需 Key + 组织 ID + 项目 ID",
    tags: [{ text: "多字段", color: "coral" }],
    fields: [
      { key: "key", label: "Team API Key", placeholder: "Team Key…", type: "password" },
      { key: "orgid", label: "Organization ID", placeholder: "Bigmodel-Organization", type: "text" },
      { key: "projid", label: "Project ID", placeholder: "Bigmodel-Project", type: "text" },
    ],
  },
  {
    id: "qoder",
    label: "Qoder ( 阿里 )",
    cat: "cookie",
    billing: ["按量"],
    authType: "cookie",
    desc: "仪表盘 Cookie，区分国际站 / 中国站",
    tags: [{ text: "区域", color: "amber" }],
    fields: [
      { key: "site", label: "站点", placeholder: "", type: "select", options: ["global", "cn"], default: "cn" },
      { key: "cookie", label: "Cookie", placeholder: "粘贴仪表盘 Cookie…", type: "textarea" },
    ],
  },
  {
    id: "ollama",
    label: "Ollama ( Ollama Cloud )",
    cat: "cookie",
    billing: ["按量"],
    authType: "cookie",
    desc: "Ollama Cloud，按周统计用量",
    tags: [{ text: "周窗口", color: "amber" }],
  },
];

// 按类别排序：订阅制(OAuth) → API Key → Cookie
export const CAT_ORDER: Record<string, number> = {
  subscription: 0,
  "api-key": 1,
  cookie: 2,
};

export const GROUPS: Array<{ cat: string; label: string }> = [
  { cat: "subscription", label: "OAuth" },
  { cat: "api-key", label: "API Key" },
  { cat: "cookie", label: "Cookie" },
];
