// railPath / TAIL_PATH — silhouette geometry contracts.
import { describe, expect, it } from "vitest";
import { TAIL_H, TAIL_OUTLINE, TAIL_PATH, TAIL_W, railPath } from "./pulse-shapes";

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
  it("ports token-monitor's bubbleCommands tail with its edge-hugging S-curve", () => {
    const nums = coords(TAIL_PATH);
    const xs = nums.filter((_, i) => i % 2 === 0);
    const ys = nums.filter((_, i) => i % 2 === 1);
    expect(Math.max(...xs)).toBeLessThanOrEqual(TAIL_W);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...ys)).toBeLessThanOrEqual(TAIL_H);
    expect(Math.min(...ys)).toBeGreaterThanOrEqual(0);
    // Neck anchors on the card edge (x = TAIL_W) at both ends, tip at x = 1.
    expect(TAIL_PATH).toContain(`M ${TAIL_W} 0`);
    expect(TAIL_PATH.endsWith(`${TAIL_W} ${TAIL_H} Z`)).toBe(true);
    // The FIRST control point of each curve sits ON the card edge
    // (x = TAIL_W, y = 0.4·neck from the end) — the curve departs
    // vertically before the mid control (x = TAIL_W/2, center ∓1.0 for
    // the sharp ~14° tip vertex) sweeps to the tip.
    expect(TAIL_PATH).toContain(`C ${TAIL_W} 7.2 ${TAIL_W / 2} 17 1 18`);
    expect(TAIL_PATH).toContain(`C ${TAIL_W / 2} 19 ${TAIL_W} 25.2`);
  });

  it("TAIL_OUTLINE is the tail without the card-edge closing segment", () => {
    // Same curves, NO Z and no closing line back up the card edge — the
    // stroke never draws along the fused junction.
    expect(TAIL_OUTLINE).toBe(TAIL_PATH.slice(0, -2));
    expect(TAIL_OUTLINE).not.toContain("Z");
  });
});
