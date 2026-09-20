// railPath / TAIL_PATH — silhouette geometry contracts.
import { describe, expect, it } from "vitest";
import { TAIL_H, TAIL_PATH, TAIL_W, railPath } from "./pulse-shapes";

function coords(d: string): number[] {
  return [...d.matchAll(/-?\d+(?:\.\d+)?/g)].map((m) => parseFloat(m[0]));
}

describe("railPath", () => {
  it("draws the right-edge rail starting on the screen edge", () => {
    const d = railPath(80, 240, "right", 26, 20);
    // First point: (w, 0) — the surface is born ON the screen edge.
    expect(d.startsWith("M80 0")).toBe(true);
    // ...and ends back on it at the bottom.
    expect(d.endsWith("80 240")).toBe(true);
    // Concave shoulder control point pulls INTO the screen edge (x = w).
    expect(d).toContain("C80 19.76");
  });

  it("mirrors for the left edge", () => {
    const right = coords(railPath(80, 240, "right", 26, 20));
    const left = coords(railPath(80, 240, "left", 26, 20));
    // Every x is mirrored (w − x), every y identical.
    expect(left.length).toBe(right.length);
    for (let i = 0; i < right.length; i += 2) {
      expect(left[i]).toBeCloseTo(80 - right[i], 2);
      expect(left[i + 1]).toBeCloseTo(right[i + 1], 2);
    }
  });

  it("never closes the path along the display edge", () => {
    expect(railPath(80, 240, "right", 26, 20)).not.toContain("Z");
  });

  it("clamps the shoulder to half the height", () => {
    // shoulder 500 on a 100-tall rail → s = 50; radius squeezed to ≥ 0.
    const d = railPath(80, 100, "right", 500, 20);
    expect(d).toMatch(/^M80 0/);
  });
});

describe("TAIL_PATH", () => {
  it("spans the fixed local box with smooth S-curves", () => {
    const nums = coords(TAIL_PATH);
    const xs = nums.filter((_, i) => i % 2 === 0);
    const ys = nums.filter((_, i) => i % 2 === 1);
    expect(Math.max(...xs)).toBeLessThanOrEqual(TAIL_W);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...ys)).toBeLessThanOrEqual(TAIL_H);
    expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
    // Card edge at x=18 (neck top y=7, neck bottom y=33), tip at x=1, center y=20.
    expect(TAIL_PATH).toContain("M 18 7");
    expect(TAIL_PATH.endsWith("18 33 Z")).toBe(true);
  });
});
