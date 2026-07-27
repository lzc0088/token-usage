// Settings window active partition (nav state). The settings window is a
// separate Tauri window with its own JS context, so this state is scoped to
// that window only — no cross-window sync needed.
let partition = $state("general");

export function getSettingsPartition(): string { return partition; }
export function setSettingsPartition(p: string): void { partition = p; }
