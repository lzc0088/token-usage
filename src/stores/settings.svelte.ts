// Settings window active partition (nav state). The settings window is a
// separate Tauri window with its own JS context, so this state is scoped to
// that window only — no cross-window sync needed.
//
// Cross-window navigation (e.g. a quota empty-state link in the main popover
// opening the settings window on the "account" page) is bridged through the
// Rust `open_settings(target)` / `consume_settings_target` commands, NOT here
// (JS module state cannot cross between the two webviews).
let partition = $state("general");

export function getSettingsPartition(): string { return partition; }
export function setSettingsPartition(p: string): void { partition = p; }
