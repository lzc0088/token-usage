// Regression: the month-label column dedupe. A grid whose leading partial
// week straddles a month boundary (gridStart 2025-08-31 → 2025-09-01 in
// column 0) used to emit two labels with col=0 — a duplicate each-key that
// crashed the reactive flush and froze the whole popover on 累计.
import { describe, it, expect, vi } from "vitest";
import { tick, mount } from "svelte";
import Heatmap from "./Heatmap.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async () => undefined,
  Channel: class {},
}));

// Real-shape data: sparse daily points whose year window (end 2026-09-01
// minus 364 days → start 2025-09-02, backed up to Sunday 2025-08-31)
// straddles Aug/Sep in the grid's first column.
const STRADDLING_POINTS = [
  "2025-04-09", "2025-10-15", "2025-11-02", "2026-03-11",
  "2026-06-20", "2026-07-05", "2026-08-01", "2026-08-28", "2026-09-01",
].map((date) => ({ date, tokens: 1_000_000, cost_usd: 1, messages: 5 }));

describe("Heatmap month labels", () => {
  it("renders without duplicate-key crash when the grid's first week straddles a month", async () => {
    const errors: string[] = [];
    const onErr = (e: ErrorEvent): void => {
      errors.push(e.error instanceof Error ? e.error.message : e.message);
    };
    window.addEventListener("error", onErr);

    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Heatmap, { target, props: { points: STRADDLING_POINTS, locale: "zh" } });
    await tick();
    await new Promise((r) => setTimeout(r, 20));

    // Month label texts rendered — no each_key_duplicate throw.
    const labels = [...target.querySelectorAll("text")].map((t) => t.textContent);
    expect(labels.length).toBeGreaterThan(0);
    expect(errors).toEqual([]);

    window.removeEventListener("error", onErr);
    target.remove();
  });
});
