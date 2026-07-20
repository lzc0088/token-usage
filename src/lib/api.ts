// Typed Tauri `invoke` wrappers (M3 T3.1). Signatures mirror the Rust
// #[command] fns in src-tauri/src/commands/. The Rust serde shapes are the
// source of truth; these interfaces keep the frontend end of the contract.

import { invoke } from "@tauri-apps/api/core";

// ── shared enums ────────────────────────────────────────────────────────────

export type Period = "day" | "month" | "total";
export type Dimension = "tool" | "model";

// ── view models (match the Rust Serialize structs) ──────────────────────────

export interface Summary {
  period: string;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  reasoning: number;
  total_tokens: number;
  cost_usd: number;
  messages: number;
  delta_pct: number | null;
  delta_label: string | null;
}

export interface BreakdownEntry {
  key: string;
  tokens: number;
  token_pct: number;
  cost_usd: number;
  cost_pct: number;
  messages: number;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
}

export interface Breakdown {
  dimension: "Tool" | "Model";
  entries: BreakdownEntry[];
  grand_total_tokens: number;
  grand_total_cost: number;
}

export interface TrendPoint {
  date: string;
  tokens: number;
  cost_usd: number;
  messages: number;
}

export interface Trends {
  points: TrendPoint[];
}

export interface SessionVm {
  tool: string;
  session_id: string;
  tokens: number;
  cost_usd: number;
  messages: number;
  model_count: number;
  models: string;
  last_used_at: string | null;
  project_name: string | null;
  project_path: string | null;
}

export interface SessionDetailRow {
  model: string;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  tokens: number;
  cost_usd: number;
  messages: number;
}

export interface SessionRoundVm {
  user_text: string;
  timestamp: string | null;
  turns: number;
  tools: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  total_tokens: number;
  cost_usd: number;
}

export interface ProjectDetailRow {
  key: string;
  tokens: number;
  pct: number;
}

export interface ProjectVm {
  name: string;
  full_path: string | null;
  latest_date: string | null;
  tokens: number;
  cost_usd: number;
  messages: number;
  models: ProjectDetailRow[];
  tools: ProjectDetailRow[];
}

export type ToolStatus = "active" | "waiting" | "missing";

export interface ClientStatus {
  client: string;
  label: string;
  status: ToolStatus;
  message_count: number;
}

export interface TokscaleStatus {
  installed: boolean;
  version: string | null;
}

export type Currency = "usd" | "cny" | "both";

export type QuotaKind = "balance" | "plan";
export type QuotaStatus = "ok" | "low" | "danger";

export interface Quota {
  vendor: string;
  kind: QuotaKind;
  status: QuotaStatus;
  value: number | null;
  display: string;
  reset_in_secs: number | null;
  used_pct: number | null;
  currency: string | null;
}

export interface Config {
  currency: Currency;
  tokscale_path?: string | null;
  auto_start?: boolean;
  language?: "zh" | "en";
  default_period?: "day" | "month" | "total";
}

// ── command wrappers ────────────────────────────────────────────────────────

export const api = {
  getSummary: (period: Period) => invoke<Summary>("get_summary", { period }),

  getBreakdown: (period: Period, dimension: Dimension) =>
    invoke<Breakdown>("get_breakdown", { period, dimension }),

  getDetailBreakdown: (period: Period, dimension: Dimension, filter: string) =>
    invoke<Breakdown>("get_detail_breakdown", { period, dimension, filter }),

  getTrends: (period: Period) => invoke<Trends>("get_trends", { period }),

  getSessions: () => invoke<SessionVm[]>("get_sessions"),

  getSessionDetail: (tool: string, sessionId: string) =>
    invoke<SessionDetailRow[]>("get_session_detail", { tool, sessionId }),

  getSessionRounds: (tool: string, sessionId: string) =>
    invoke<SessionRoundVm[]>("get_session_rounds", { tool, sessionId }),

  getProjects: (period: Period) => invoke<ProjectVm[]>("get_projects", { period }),

  getToolsStatus: () => invoke<ClientStatus[]>("get_tools_status"),

  getQuotas: () => invoke<Quota[]>("get_quotas"),

  getCredentialStatus: (vendor: string) => invoke<boolean>("get_credential_status", { vendor }),

  setCredential: (vendor: string, secret: string) => invoke<void>("set_credential", { vendor, secret }),

  deleteCredential: (vendor: string) => invoke<void>("delete_credential", { vendor }),

  getTokscaleStatus: () => invoke<TokscaleStatus>("get_tokscale_status"),

  getConfig: () => invoke<Config>("get_config"),

  setConfig: (config: Config) => invoke<void>("set_config", { config }),
};
