// Global popover period (DAY / MONTH / TOTAL). Svelte 5 runes module —
// `period` is reactive across all components that import it. Changing it is
// the single source of "re-query everything for the new range".

import type { Period } from "../lib/api";

let period = $state<Period>("day");

export function getPeriod(): Period {
  return period;
}

export function setPeriod(p: Period): void {
  period = p;
}

// Re-export a reactive getter for use in $derived/$effect in components.
export function periodValue(): Period {
  return period;
}
