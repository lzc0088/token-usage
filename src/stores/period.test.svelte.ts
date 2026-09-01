// Verifies the Svelte 5 reactivity contract the whole popover relies on:
// reading the cross-module period `$state` via `periodValue()` — both
// directly inside `$effect` and through a `$derived(...)` wrapper — must
// re-run downstream effects when `setPeriod` writes the store.
import { describe, it, expect, beforeEach } from "vitest";
import { tick } from "svelte";
import { getPeriod, setPeriod, periodValue } from "./period.svelte";

describe("period store reactivity", () => {
  beforeEach(() => {
    setPeriod("day");
  });

  it("periodValue() read inside $effect re-runs on setPeriod", async () => {
    let runs = 0;
    let seen: string = "";

    $effect.root(() => {
      $effect(() => {
        seen = periodValue();
        runs++;
      });
    });

    await tick();
    expect(runs).toBe(1);
    expect(seen).toBe("day");

    setPeriod("total");
    await tick();
    // Core assertion: the effect re-runs on a cross-module $state write.
    expect(runs).toBe(2);
    expect(seen).toBe("total");

    setPeriod("month");
    await tick();
    expect(runs).toBe(3);
    expect(seen).toBe("month");
  });

  it("$derived(periodValue()) wrapper re-runs effects on setPeriod", async () => {
    let runs = 0;
    let seen: string = "";

    $effect.root(() => {
      const activePeriod = $derived(periodValue());
      $effect(() => {
        seen = activePeriod;
        runs++;
      });
    });

    await tick();
    expect(runs).toBe(1);
    expect(seen).toBe("day");

    setPeriod("total");
    await tick();
    expect(runs).toBe(2);
    expect(seen).toBe("total");
  });

  it("getPeriod() reflects writes", () => {
    setPeriod("month");
    expect(getPeriod()).toBe("month");
    setPeriod("day");
    expect(getPeriod()).toBe("day");
  });
});
