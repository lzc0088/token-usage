/// Shared event names for frontend-backend communication via Tauri events.
/// Keep in sync with Rust emitter calls in src-tauri/src/commands/.

/** Emitted after the quota scheduler finishes a refresh cycle (one or all vendors). */
export const QUOTA_UPDATED = "quota:updated";
/** Emitted when the user saves config in the settings window. */
export const CONFIG_CHANGED = "config:changed";
/** Emitted by the collector when today's summary is refreshed. */
export const TODAY_UPDATED = "today:updated";
/** Emitted after the exchange rate is refreshed (auto-fetch or manual). */
export const RATE_UPDATED = "rate:updated";
/** Emitted when the user clicks "立即刷新" in the tray context menu. */
export const TRAY_REFRESH = "tray:refresh";
/** Emitted when collection data is refreshed (history ingest, archive clear, client list change). */
export const COLLECTION_UPDATED = "collection:updated";
/** Emitted when a collection scan or data-ingest step fails so the UI can
 *  show a degraded-state warning instead of silently stale data. */
export const COLLECTION_ERROR = "collection:error";
/** Emitted after collector health record is updated (scan success/failure). */
export const COLLECTION_HEALTH = "collection:health";
/** Emitted during the GitHub Copilot OAuth Device Flow. */
export const COPILOT_LOGIN_STATUS = "copilot:login_status";
/** Emitted during the Codex OAuth login flow. */
export const CODEX_LOGIN_STATUS = "codex:login_status";
