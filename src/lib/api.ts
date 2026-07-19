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
}

export interface BreakdownEntry {
  key: string;
  tokens: number;
  token_pct: number;
  cost_usd: number;
  cost_pct: number;
  messages: number;
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
  model: string;
  tokens: number;
  cost_usd: number;
}

export interface ProjectVm {
  path: string;
  tokens: number;
  cost_usd: number;
  session_count: number;
}

export type ToolStatus = "active" | "waiting" | "missing";

export interface ClientStatus {
  client: string;
  label: string;
  status: ToolStatus;
  message_count: number;
}

export type Currency = "usd" | "cny" | "both";

export interface Config {
  currency: Currency;
  tokscale_path?: string | null;
}

// ── command wrappers ────────────────────────────────────────────────────────

export const api = {
  getSummary: (period: Period) => invoke<Summary>("get_summary", { period }),

  getBreakdown: (period: Period, dimension: Dimension) =>
    invoke<Breakdown>("get_breakdown", { period, dimension }),

  getTrends: (period: Period) => invoke<Trends>("get_trends", { period }),

  getSessions: () => invoke<SessionVm[]>("get_sessions"),

  getProjects: () => invoke<ProjectVm[]>("get_projects"),

  getToolsStatus: () => invoke<ClientStatus[]>("get_tools_status"),

  getConfig: () => invoke<Config>("get_config"),

  setConfig: (config: Config) => invoke<void>("set_config", { config }),
};
