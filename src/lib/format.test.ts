import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, splitTokens, splitTokensCN } from "./format";

describe("formatTokens", () => {
  it("returns '0' for non-finite input", () => {
    expect(formatTokens(NaN)).toBe("0");
    expect(formatTokens(Infinity)).toBe("0");
    expect(formatTokens(-Infinity)).toBe("0");
  });

  it("formats compact style (default)", () => {
    expect(formatTokens(500)).toBe("500");
    expect(formatTokens(1_500)).toBe("1.5K");
    expect(formatTokens(1_500_000)).toBe("1.5M");
    expect(formatTokens(1_500_000_000)).toBe("1.5B");
    expect(formatTokens(42_000)).toBe("42K");
  });

  it("formats compact edge values", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1_000)).toBe("1K");
    expect(formatTokens(999_999)).toBe("1000K");
    expect(formatTokens(1_000_000)).toBe("1M");
  });

  it("formats plain style as locale string", () => {
    expect(formatTokens(1_234_567, "plain")).toBe("1,234,567");
    expect(formatTokens(0, "plain")).toBe("0");
  });

  it("formats wan (万) style", () => {
    expect(formatTokens(10_000, "wan")).toBe("1.0万");
    expect(formatTokens(350_000, "wan")).toBe("35万");
    expect(formatTokens(1_200_000, "wan")).toBe("120万");
    expect(formatTokens(100_000_000, "wan")).toBe("1.00亿");
    expect(formatTokens(150_000_000, "wan")).toBe("1.50亿");
    expect(formatTokens(500, "wan")).toBe("500");
  });
});

describe("formatCost", () => {
  it("guards non-finite values", () => {
    expect(formatCost(NaN, "usd")).toBe("$ 0.00");
    expect(formatCost(Infinity, "usd")).toBe("$ 0.00");
  });

  it("formats USD only", () => {
    expect(formatCost(5.0, "usd")).toBe("$ 5.00");
    expect(formatCost(0.015, "usd")).toBe("$ 0.01");
  });

  it("formats CNY using rate", () => {
    expect(formatCost(1.0, "cny", 7.2)).toBe("¥ 7.20");
    expect(formatCost(10.0, "cny", 7.0)).toBe("¥ 70.00");
  });

  it("formats both (CNY first)", () => {
    const result = formatCost(1.0, "both", 7.0);
    expect(result).toContain("¥ 7.00");
    expect(result).toContain("$ 1.00");
    expect(result.startsWith("¥")).toBe(true);
  });

  it("guards non-finite rate", () => {
    // NaN rate means usd is set to 0, giving ¥ 0.00
    expect(formatCost(1.0, "cny", NaN)).toBe("¥ 0.00");
  });
});

describe("splitTokens", () => {
  it("handles non-finite input", () => {
    expect(splitTokens(NaN)).toEqual({ value: "0", unit: "" });
  });

  it("splits into value and unit", () => {
    expect(splitTokens(500)).toEqual({ value: "500", unit: "" });
    expect(splitTokens(1_500)).toEqual({ value: "1.5", unit: "K" });
    expect(splitTokens(1_500_000)).toEqual({ value: "1.5", unit: "M" });
    expect(splitTokens(1_500_000_000)).toEqual({ value: "1.5", unit: "B" });
  });

  it("respects decimals parameter", () => {
    expect(splitTokens(1_234_567, 1)).toEqual({ value: "1.2", unit: "M" });
    expect(splitTokens(1_234_567, 4)).toEqual({ value: "1.2346", unit: "M" });
  });
});

describe("splitTokensCN", () => {
  it("handles non-finite input", () => {
    expect(splitTokensCN(NaN)).toEqual({ value: "0", unit: "" });
  });

  it("splits Chinese scale thresholds", () => {
    expect(splitTokensCN(500)).toEqual({ value: "500", unit: "" });
    expect(splitTokensCN(1_500)).toEqual({ value: "1.5", unit: "千" });
    expect(splitTokensCN(15_000)).toEqual({ value: "1.5", unit: "万" });
    expect(splitTokensCN(150_000)).toEqual({ value: "1.5", unit: "十万" });
    expect(splitTokensCN(1_500_000)).toEqual({ value: "1.5", unit: "百万" });
    expect(splitTokensCN(15_000_000)).toEqual({ value: "1.5", unit: "千万" });
    expect(splitTokensCN(150_000_000)).toEqual({ value: "1.5", unit: "亿" });
    expect(splitTokensCN(1_500_000_000)).toEqual({ value: "1.5", unit: "十亿" });
    expect(splitTokensCN(15_000_000_000)).toEqual({ value: "1.5", unit: "百亿" });
  });

  it("handles negative values correctly", () => {
    const result = splitTokensCN(-15_000);
    expect(result.unit).toBe("万");
  });
});
