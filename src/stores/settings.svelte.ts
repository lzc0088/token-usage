// Settings modal open/close + active partition.
let open = $state(false);
let partition = $state("general");

export function isSettingsOpen(): boolean { return open; }
export function openSettings(): void { open = true; }
export function closeSettings(): void { open = false; }
export function getSettingsPartition(): string { return partition; }
export function setSettingsPartition(p: string): void { partition = p; }
