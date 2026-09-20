// Shared payload types for the pulse windows (ring panel + detail card).
// Mirrors src-tauri/src/ui/pulse.rs (PulseData / PulseQuota / …).

export interface PulseWindow {
  label: string;
  used_pct: number;
  resets_at?: string;
  /** Absolute used/total (e.g. credits) for "剩余 X" display. */
  used_value?: number;
  total_value?: number;
}

export interface PulseBalance {
  amount: number;
  currency: string;
  /** Today / month spend (API vendors, e.g. DeepSeek). */
  today_consumption?: number;
  month_consumption?: number;
}

export interface PulseQuota {
  vendor: string;
  plan?: string;
  /** Credits/balance-only vendor: amount under the ring, no progress arc. */
  planless?: boolean;
  status: string;
  windows: PulseWindow[];
  critical_pct: number;
  critical_label: string;
  balance?: PulseBalance;
  /** Subscription plan expiry (RFC3339) — shown beside the plan badge. */
  expires_at?: string;
  second_pct?: number;
  second_label?: string;
  is_running?: boolean;
  is_refreshing?: boolean;
  /** Server timestamp (RFC3339) when the data was fetched. */
  refreshed_at?: string;
}

export interface PulseData {
  quotas: PulseQuota[];
  size: string;
  ring_diameter: number;
  theme: string;
  /** Surface opacity (0.2–1.0), from config. */
  opacity?: number;
  /** Panel height cap (logical px) = 80% of the current screen, computed
   *  by Rust so the window size and the CSS dock cap agree exactly. */
  max_panel_h?: number;
  /** Panel is collapsed to a narrow peek grip at the screen edge. */
  peek?: boolean;
  /** Panel side: "left" | "right" — tells the peek grip which edge to anchor to. */
  side?: string;
}

/** Card-show payload from Rust (ui/pulse.rs expand_pulse → "pulse:card"). */
export interface PulseCardPayload {
  vendor: string;
  /** true → the card window sits LEFT of the panel (flush-right panel). */
  cardOnLeft?: boolean;
  theme?: string;
  opacity?: number;
}
