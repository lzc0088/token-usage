// Global popover period (DAY / MONTH / TOTAL). Svelte 5 runes module.

import type { Period } from "../lib/api";

let period = $state<Period>("day");

// Listener set for components that need imperative notification of period
// changes (bypasses Svelte 5's static-analysis limitation: the compiler
// cannot track cross-module function calls inside $effect).
const periodListeners = new Set<(p: Period) => void>();

export function getPeriod(): Period {
  return period;
}

export function setPeriod(p: Period): void {
  period = p;
  periodListeners.forEach((fn) => fn(p));
}

export function periodValue(): Period {
  return period;
}

/// Register a callback invoked synchronously whenever `setPeriod` is called.
/// Returns an unsubscribe function. Use this in `$effect` when you need to
/// react to period changes but the compiler cannot track `periodValue()`.
export function onPeriodChange(fn: (p: Period) => void): () => void {
  periodListeners.add(fn);
  return () => periodListeners.delete(fn);
}
