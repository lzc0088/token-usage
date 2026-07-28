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
