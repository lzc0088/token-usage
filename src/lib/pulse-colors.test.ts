// vendorColor — golden-angle hue per dock index: distinct, ordered spread.
import { describe, expect, it } from "vitest";
import { vendorColor } from "./pulse-colors";

function hueOf(color: string): number {
  const m = color.match(/^hsl\(([\d.]+) /);
  return m ? parseFloat(m[1]) : NaN;
}

describe("vendorColor", () => {
  it("is stable for the same index", () => {
    expect(vendorColor(3)).toBe(vendorColor(3));
    expect(vendorColor(3)).toMatch(/^hsl\(/);
  });

  it("keeps any 12 dock slots ≥20° apart (每一家都不同)", () => {
    const hues = Array.from({ length: 12 }, (_, i) => hueOf(vendorColor(i)));
    for (let i = 0; i < hues.length; i++) {
      for (let j = i + 1; j < hues.length; j++) {
        const d = Math.abs(hues[i] - hues[j]);
        expect(Math.min(d, 360 - d)).toBeGreaterThanOrEqual(20);
      }
    }
    // Adjacent vendors land maximally far apart (golden angle).
    expect(Math.abs(hueOf(vendorColor(0)) - hueOf(vendorColor(1)))).toBeCloseTo(
      137.508,
      1
    );
  });

  it("adapts lightness for the card theme", () => {
    expect(vendorColor(0, true)).not.toBe(vendorColor(0, false));
    expect(vendorColor(0, true)).toBe(vendorColor(0, true));
  });

  it("handles negative indices defensively", () => {
    expect(vendorColor(-1)).toMatch(/^hsl\(/);
  });
});
