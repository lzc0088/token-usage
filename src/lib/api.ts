// Typed Tauri `invoke` wrappers (M3 T3.1). Signatures mirror the Rust
// #[command] fns in src-tauri/src/commands/. The Rust serde shapes are the
// source of truth; these interfaces keep the frontend end of the contract.

import { invoke, Channel } from "@tauri-apps/api/core";

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
  /** Real-time throughput counters (live today path only). Undefined for
   *  month/total or when no session reported a duration. Frontend derives
   *  tokens/s (speed) or tokens/min (burn) from these. */
  timed_output_tokens?: number;
  timed_tokens?: number;
  timed_duration_ms?: number;
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
  rounds: number;
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
  rounds: number;
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
  model: string | null;
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

export interface ClientDiagnostic {
  code: string;
  severity: string;
  message: string;
}

export interface ClientStatus {
  client: string;
  label: string;
  status: ToolStatus;
  message_count: number;
  diagnostics?: ClientDiagnostic[];
}

export interface TokscaleStatus {
  installed: boolean;
  version: string | null;
}

/** Per-tool last-seen info, from the persisted collection health record. */
export interface ClientHealth {
  last_seen_ms: number;
  message_count: number;
}

/** A recorded scan failure (message + unix-ms timestamp). */
export interface HealthError {
  message: string;
  at_ms: number;
}

/** Persisted collection health: global scan timestamps + per-tool last-seen. */
export interface CollectionHealth {
  last_today_ms?: number | null;
  last_history_ms?: number | null;
  last_error?: HealthError | null;
  clients: Record<string, ClientHealth>;
}

/** Phase 1 result from copilot_login: user code + verification URL. */
export interface CopilotLoginStart {
  user_code: string;
  verification_url: string;
  expires_in: number;
}

export type Currency = "usd" | "cny" | "both";

export type QuotaStatus = "ok" | "low" | "danger";

export interface QuotaWindow {
  label: string;
  used_pct: number;
  /** Absolute reset time (RFC3339/ISO-8601). Frontend computes live countdown. */
  resets_at?: string;
  /** Used value (e.g. credits consumed) for "X / Y" display. */
  used_value?: number;
  /** Total value (e.g. credits limit) for "X / Y" display. */
  total_value?: number;
  /** Individual quota items within this window (e.g. each resource package). */
  sub_items?: QuotaWindowSubItem[];
  /** Projected exhaustion time (RFC3339/ISO-8601), computed from burn rate. */
  projected_exhaustion_at?: string;
}

export interface QuotaWindowSubItem {
  name: string;
  used: number;
  total: number;
  pct: number;
  expires_at?: string;
}

export interface QuotaBalance {
  amount: number;
  currency: string;
  today_consumption?: number;
  month_consumption?: number;
}

export interface Quota {
  vendor: string;
  status: QuotaStatus;
  plan_label?: string;
  windows: QuotaWindow[];
  balance: QuotaBalance | null;
  /** RFC3339 timestamp when this quota was last fetched. */
  refreshed_at?: string;
  /** User-actionable error message, e.g. "凭证已失效". */
  error?: string | null;
  /** Optional-cookie expired hint (e.g. Volcengine expiry cookie, or cookie-only
   * vendor fully failing). Frontend shows an inline "更新 Cookie" entry. */
  cookie_error?: string | null;
  /** Subscription plan expiry (RFC3339). Distinct from per-window `resets_at`
   * (rolling quota reset). Drives the "到期" tag. */
  expires_at?: string | null;
  /** Region / site identifier for multi-region vendors (e.g. Qoder "cn"/"global").
   * Used to construct the correct console URL when opening the vendor panel. */
  site?: string | null;
}

export interface Config {
  currency: Currency;
  /** Exchange-rate source: "auto" (fetch daily) | "manual" (user-supplied). */
  rate_mode?: "auto" | "manual";
  tokscale_path?: string | null;
  auto_start?: boolean;
  language?: "zh" | "en";
  default_period?: "day" | "month" | "total";
  /** Token-rate readout mode: "speed" (output tokens/s of model-busy time)
   *  | "burn" (total tokens/min). */
  token_rate_mode?: "speed" | "burn";
  auto_close_on_blur?: boolean;
  /** Popover trigger: "click" (tray click) | "hover" (mouse over tray). */
  trigger_mode?: "click" | "hover";
  /** Window display mode: "normal" (draggable) | "fixed" (pinned) | "always_on_top" (floating). */
  window_display_mode?: "normal" | "fixed" | "always_on_top";
  /** Tray display style. */
  tray_display?:
    | "today_tokens"
    | "today_cost"
    | "today_both"
    | "total_tokens"
    | "total_cost"
    | "total_both"
    | "quota_min"
    | "icon_only";
  /** Whether to show the app icon in the Dock. */
  show_in_dock?: boolean;
  /** Global hotkey to toggle the popover (e.g. "Alt+Command+T"). */
  hotkey?: string;
  /** UI theme: "dark" | "light" | "system". */
  theme?: "dark" | "light" | "system";
  /** Animation preference: "system" | "on" | "off". */
  animation?: "system" | "on" | "off";
  /** Font size preset: "small" (13px) | "medium" (15px) | "large" (17px). */
  font_size?: "small" | "medium" | "large";
  /** Font family preset: "app" (Hanken Grotesk) | "system" (system-ui) | "mono" (JetBrains Mono). */
  font_family?: "app" | "system" | "mono";
  /** Data refresh interval. */
  refresh_interval?: "manual" | "30s" | "60s" | "300s";
  /** Collection mode: "live" (file-watch realtime) | "smart" (10min interval, activity-gated)
   * | "interval" (fixed interval only, no file watch). */
  collection_mode?: "live" | "smart" | "interval";
  /** Preserve sessions whose source tool is no longer installed. */
  session_archive_enabled?: boolean;
  /** Quota data refresh interval: fixed cadence, or "adaptive" (5-min baseline
   *  + burn-rate-driven early probes for windows nearing exhaustion). */
  quota_refresh_interval?: "1m" | "3m" | "5m" | "10m" | "15m" | "adaptive";
  /** Quota progress display mode. */
  quota_progress_mode?: "用量" | "剩余";
  /** Enabled vendor IDs for the quota display (undefined = all enabled). */
  quota_active_vendors?: string[] | null;
  /** Custom display order for quota vendor list (all vendor ids). */
  quota_vendor_order?: string[] | null;
  /** Collection: tracked tool names (undefined = all tracked). */
  collection_tracked?: string[] | null;
  /** Collection: visible tool names (undefined = all visible). */
  collection_visible?: string[] | null;
  /** Collection: ordered tool names (undefined = report order). */
  collection_ordered?: string[] | null;
  /** Layout: visible top-level segment keys in order. */
  layout_modules?: string[] | null;
  /** Whether to show system notifications when quota is nearly exhausted. */
  quota_notify_enabled?: boolean;
  /** Layout: visible overview sub-item keys in order. */
  layout_overview_sub?: string[] | null;
  /** Overview: quota vendor IDs to show, in order. */
  overview_quota_vendors?: string[] | null;
  /** Show a floating data widget on the desktop (Windows/Linux only). */
  floating_enabled?: boolean;
  /** Floating widget display mode. */
  floating_display?: "today_tokens" | "today_cost" | "total_tokens" | "total_cost";
  /** Floating widget screen edge: "left" | "right". */
  floating_position?: "left" | "right";
}

// ── exchange rate ───────────────────────────────────────────────────────────

export interface ExchangeRateInfo {
  rate: number;
  cached: boolean;
  date: string;
}

export interface UpdateInfo {
  has_update: boolean;
  version: string;
  name: string;
  changelog: string;
  url: string;
  published_at: string | null;
  /** Error message when the check failed; empty on success. */
  error: string;
  /** Machine-readable failure kind: "" | "rate_limited" | "network" | "api_error" | "parse".
   *  Lets the UI localize + decide whether to retry. Empty on success or when
   *  the last-known-good result is being surfaced despite a transient failure. */
  error_kind: string;
  /** Direct download URL for the release asset (e.g. .dmg). */
  download_url: string | null;
}

/**
 * Progress events streamed from `install_update` during download + install.
 * Drives the install UI state machine in General.svelte.
 */
export type InstallEvent =
  | { event: "Started"; data: { content_length: number } }
  | { event: "Progress"; data: { chunk_length: number } }
  | { event: "Finished"; data: null }
  | { event: "Installed"; data: null }
  | { event: "Error"; data: { message: string } };

// ── command wrappers ────────────────────────────────────────────────────────

export const api = {
  getSummary: (period: Period) => invoke<Summary>("get_summary", { period }),

  getBreakdown: (period: Period, dimension: Dimension) =>
    invoke<Breakdown>("get_breakdown", { period, dimension }),

  getDetailBreakdown: (period: Period, dimension: Dimension, filter: string) =>
    invoke<Breakdown>("get_detail_breakdown", { period, dimension, filter }),

  getTrends: (period: Period) => invoke<Trends>("get_trends", { period }),

  getSessions: (limit?: number) =>
    invoke<SessionVm[]>("get_sessions", { limit: limit ?? null }),

  getSessionDetail: (tool: string, sessionId: string) =>
    invoke<SessionDetailRow[]>("get_session_detail", { tool, sessionId }),

  getSessionRounds: (tool: string, sessionId: string) =>
    invoke<SessionRoundVm[]>("get_session_rounds", { tool, sessionId }),

  getProjects: (period: Period, offset?: number, limit?: number) =>
    invoke<ProjectVm[]>("get_projects", { period, offset: offset ?? null, limit: limit ?? null }),

  getToolsStatus: () => invoke<ClientStatus[]>("get_tools_status"),

  getQuotas: () => invoke<Quota[]>("get_quotas"),

  refreshQuotas: () => invoke<void>("refresh_quotas"),

  /** Refresh quotas only if cache is older than `quota_refresh_interval`. Returns true if a refresh ran. */
  refreshQuotasIfStale: () => invoke<boolean>("refresh_quotas_if_stale"),

  /** Force an immediate collector scan (today + history). Fire-and-forget. */
  collectNow: () => invoke<void>("collect_now"),

  refreshQuota: (vendor: string) => invoke<void>("refresh_quota", { vendor }),

  testCredential: (vendor: string, credential: string) => invoke<string>("test_credential", { vendor, credential }),

  getCredentialStatus: (vendor: string) => invoke<boolean>("get_credential_status", { vendor }),

  setCredential: (vendor: string, secret: string) => invoke<void>("set_credential", { vendor, secret }),

  deleteCredential: (vendor: string) => invoke<void>("delete_credential", { vendor }),

  /** Update the cookie field (and optionally region/site) of an existing
   *  credential, preserving key/secret. Triggers a quota:updated event. */
  updateCookie: (vendor: string, cookie: string, extraFields?: Record<string, string>) =>
    invoke<void>("update_cookie", { vendor, cookie, extraFields }),

  /** Non-empty field names in a stored credential (e.g. ["key","secret","cookie"]). */
  getCredentialFields: (vendor: string) =>
    invoke<string[]>("get_credential_fields", { vendor }),

  /** Values of NON-SECRET scalar fields (region, site, …) — secrets excluded. */
  getCredentialFieldValues: (vendor: string) =>
    invoke<Record<string, string>>("get_credential_field_values", { vendor }),

  /** Remove specific fields from a stored credential, keeping the rest. */
  clearCredentialFields: (vendor: string, fields: string[]) =>
    invoke<void>("clear_credential_fields", { vendor, fields }),

  /** Phase 1: request device code from GitHub. Returns user code + URL. */
  copilotLogin: () => invoke<CopilotLoginStart>("copilot_login"),
  /** Phase 2: poll for access token until user authorizes in browser. */
  pollCopilotToken: () => invoke<string>("poll_for_token"),

  /** Dev diagnostics: bridge a frontend log line to the Rust terminal. */
  feLog: (msg: string) => invoke<void>("frontend_log", { msg }).catch(() => {}),

  /** Run `codex login` OAuth flow. Emits `codex:login_status` events as it
   *  progresses; the frontend opens the authorize URL when detected. */
  codexLogin: () => invoke<void>("codex_login"),

  getTokscaleStatus: () => invoke<TokscaleStatus>("get_tokscale_status"),
  getCollectionHealth: () => invoke<CollectionHealth>("get_collection_health"),

  getArchivedSessionCount: () => invoke<number>("get_archived_session_count"),

  clearArchivedSessions: () => invoke<number>("clear_archived_sessions"),

  getConfig: () => invoke<Config>("get_config"),

  setConfig: (config: Config) => invoke<void>("set_config", { config }),

  getExchangeRate: () => invoke<ExchangeRateInfo>("get_exchange_rate"),

  refreshExchangeRate: () => invoke<ExchangeRateInfo>("refresh_exchange_rate"),

  /** Latest stored USD→CNY rate (any date, no API call). For cost conversion. */
  getLatestRate: () => invoke<ExchangeRateInfo>("get_latest_rate"),

  /** Persist a user-supplied rate and switch to manual mode. */
  setManualRate: (rate: number) => invoke<void>("set_manual_rate", { rate }),

  setAutoStart: (enabled: boolean) => invoke<boolean>("set_auto_start", { enabled }),

  getAutoStart: () => invoke<boolean>("get_auto_start"),

  getAppVersion: () => invoke<string>("get_app_version"),

  checkUpdate: (repo: string, currentVersion: string, force: boolean = false) =>
    invoke<UpdateInfo>("check_update", { repo, currentVersion, force }),

  /** Download + verify + install the latest update via tauri-plugin-updater,
   *  then restart. Progress events stream to `onEvent`. Resolves when the
   *  install is done (the app then relaunches). */
  installUpdate: (onEvent: (e: InstallEvent) => void) => {
    const channel = new Channel<InstallEvent>();
    channel.onmessage = onEvent;
    return invoke<void>("install_update", { onEvent: channel });
  },

  /** Open an external URL in the system browser. Only http/https URLs are
   *  allowed — javascript:, file:, and other schemes are rejected server-side. */
  openExternal: (url: string) => invoke<void>("open_external", { url }),

  /** Restart the app to finish applying an installed update. */
  restartApp: () => invoke<void>("restart_app"),

  /** Return the current OS: "macos" | "windows" | "linux". */
  getPlatform: () => invoke<string>("get_platform"),

  /** Export usage data as JSON string. */
  exportJson: () => invoke<string>("export_json"),

  /** Export usage data as CSV string (snapshot breakdown). */
  exportCsv: () => invoke<string>("export_csv"),

  /** Copy text to clipboard. */
  copyToClipboard: (text: string) => invoke<void>("copy_to_clipboard", { text }),
};
