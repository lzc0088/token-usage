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

    // Month labels render as yy-mm (centered) with a month-total line — no
    // duplicate each-key throw.
    const labelEls = target.querySelectorAll(".month-label");
    const labels = [...labelEls].map((t) => t.textContent?.trim());
    expect(labels.length).toBeGreaterThan(0);
    expect(labels.every((l) => l && /^\d{2}-\d{2}$/.test(l))).toBe(true);
    // Centered over each month block.
    expect([...labelEls].every((t) => t.getAttribute("text-anchor") === "middle")).toBe(true);

    // Month totals: the "2026-08" block sums its fixture days (8/01 + 8/28,
    // 1M each → （200万）in zh units), also centered.
    const totalEls = target.querySelectorAll(".month-total");
    const totals = [...totalEls].map((t) => t.textContent?.trim());
    expect(totals.length).toBe(labels.length);
    expect(totals).toContain("（200万）");
    expect([...totalEls].every((t) => t.getAttribute("text-anchor") === "middle")).toBe(true);

    expect(errors).toEqual([]);

    window.removeEventListener("error", onErr);
    target.remove();
  });

  it("lays out per-month blocks: horizontal 7-day rows", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Heatmap, { target, props: { points: STRADDLING_POINTS, locale: "zh" } });
    await tick();
    await new Promise((r) => setTimeout(r, 20));

    const cell = (date: string): SVGGElement | null =>
      target.querySelector(`[aria-label^="${date}:"]`);

    const aug1 = cell("2026-08-01");
    const aug7 = cell("2026-08-07");
    const aug8 = cell("2026-08-08");
    const aug15 = cell("2026-08-15");
    expect(aug1 && aug7 && aug8 && aug15).toBeTruthy();

    const y = (el: Element): number => Number(el.getAttribute("y"));
    const x = (el: Element): number => Number(el.getAttribute("x"));

    // Horizontal layout: days 1-7 across the first row, 8-14 second row, etc.
    // Default props: cellSize=11, gap=2 → step=13.
    const step = 13;

    // 1号 and 7号 on the same row, 6 columns apart.
    expect(y(aug7!)).toBe(y(aug1!));
    expect(x(aug7!)).toBe(x(aug1!) + 6 * step);

    // 8号 wraps to the next row (same column as 1号, one row down).
    expect(x(aug8!)).toBe(x(aug1!));
    expect(y(aug8!)).toBe(y(aug1!) + step);

    // 15号 is on the third row (same column as 1号, two rows down).
    expect(x(aug15!)).toBe(x(aug1!));
    expect(y(aug15!)).toBe(y(aug1!) + 2 * step);

    target.remove();
  });
});
