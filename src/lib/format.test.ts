import { describe, it, expect } from "vitest";
import {
  formatTokens,
  formatCost,
  splitTokens,
  splitTokensCN,
  formatTokenRate,
} from "./format";

describe("formatTokens", () => {
  it("returns '0' for NaN", () => {
    expect(formatTokens(NaN)).toBe("0");
  });
  it("returns '0' for Infinity", () => {
    expect(formatTokens(Infinity)).toBe("0");
  });
  it("returns '0' for -Infinity", () => {
    expect(formatTokens(-Infinity)).toBe("0");
  });
  it("formats billions as B", () => {
    expect(formatTokens(1_500_000_000)).toBe("1.5B");
  });
  it("formats millions as M", () => {
    expect(formatTokens(1_840_000)).toBe("1.84M");
  });
  it("formats thousands as K", () => {
    expect(formatTokens(5_200)).toBe("5.2K");
  });
  it("formats sub-thousand as plain number", () => {
    expect(formatTokens(999)).toBe("999");
  });
  it("trims trailing zeros in compact", () => {
    expect(formatTokens(1_000_000)).toBe("1M");
  });
  it("plain style uses en-US locale", () => {
    expect(formatTokens(1_234_567, "plain")).toBe("1,234,567");
  });
  it("wan style formats 万 for 10k-100M", () => {
    expect(formatTokens(12_000, "wan")).toBe("1.2万");
    expect(formatTokens(350_000, "wan")).toBe("35万");
    // >= 100k → 0 decimals (rounded)
    expect(formatTokens(12_345_678, "wan")).toBe("1235万");
  });
  it("wan style formats 亿 for >= 100M", () => {
    expect(formatTokens(100_000_000, "wan")).toBe("1.00亿");
    expect(formatTokens(123_456_789, "wan")).toBe("1.23亿");
  });
  it("wan style falls back to plain for sub-10k", () => {
    expect(formatTokens(9_000, "wan")).toBe("9,000");
  });
});

describe("formatCost", () => {
  it("returns $ prefixed for usd", () => {
    expect(formatCost(10.5, "usd")).toBe("$10.50");
  });
  it("returns ¥ prefixed for cny", () => {
    expect(formatCost(10, "cny", 7.2)).toBe("¥72.00");
  });
  it("both mode shows CNY first then USD", () => {
    expect(formatCost(10, "both", 7.2)).toBe("¥72.00 / $10.00");
  });
  it("defaults usd to 0 for NaN", () => {
    expect(formatCost(NaN, "usd")).toBe("$0.00");
  });
  it("defaults cnyRate to 7.2 and usd to 0 when cnyRate is NaN", () => {
    // Guard resets BOTH usd and cnyRate when either is non-finite
    expect(formatCost(10, "cny", NaN)).toBe("¥0.00");
  });
  it("formats zero correctly", () => {
    expect(formatCost(0, "usd")).toBe("$0.00");
    expect(formatCost(0, "cny", 7.2)).toBe("¥0.00");
  });
});

describe("splitTokens", () => {
  it("returns {value, unit} for billions", () => {
    expect(splitTokens(1_500_000_000)).toEqual({ value: "1.5", unit: "B" });
  });
  it("returns {value, unit} for millions", () => {
    expect(splitTokens(1_840_000)).toEqual({ value: "1.84", unit: "M" });
  });
  it("returns {value, unit} for thousands", () => {
    expect(splitTokens(5_200)).toEqual({ value: "5.2", unit: "K" });
  });
  it("returns empty unit for sub-thousand", () => {
    expect(splitTokens(999)).toEqual({ value: "999", unit: "" });
  });
  it("respects custom decimals", () => {
    expect(splitTokens(1_500_000, 1)).toEqual({ value: "1.5", unit: "M" });
    expect(splitTokens(1_500_000, 0)).toEqual({ value: "2", unit: "M" });
  });
  it("handles NaN", () => {
    expect(splitTokens(NaN)).toEqual({ value: "0", unit: "" });
  });
});

describe("splitTokensCN", () => {
  // Only three units survive (千/万/亿) — larger magnitudes scale the value
  // instead of introducing compound units like 百亿/千万.
  it("scales 亿 upward for >= 1亿 (no 百亿/十亿)", () => {
    expect(splitTokensCN(10_000_000_000)).toEqual({ value: "100", unit: "亿" });
    expect(splitTokensCN(1_000_000_000)).toEqual({ value: "10", unit: "亿" });
  });
  it("formats 亿 for >= 100M", () => {
    expect(splitTokensCN(100_000_000)).toEqual({ value: "1", unit: "亿" });
  });
  it("scales 万 upward for >= 1万 (no 千万/百万/十万)", () => {
    expect(splitTokensCN(10_000_000)).toEqual({ value: "1000", unit: "万" });
    expect(splitTokensCN(1_000_000)).toEqual({ value: "100", unit: "万" });
    expect(splitTokensCN(100_000)).toEqual({ value: "10", unit: "万" });
  });
  it("formats 万 for >= 10k", () => {
    expect(splitTokensCN(10_000)).toEqual({ value: "1", unit: "万" });
  });
  it("formats 千 for >= 1k", () => {
    expect(splitTokensCN(1_000)).toEqual({ value: "1", unit: "千" });
  });
  it("returns plain number for sub-1k", () => {
    expect(splitTokensCN(999)).toEqual({ value: "999", unit: "" });
  });
  it("handles negative values", () => {
    expect(splitTokensCN(-1_500_000)).toEqual({ value: "-150", unit: "万" });
  });
  it("handles NaN", () => {
    expect(splitTokensCN(NaN)).toEqual({ value: "0", unit: "" });
  });
});

describe("formatTokenRate", () => {
  it("returns empty string for zero duration", () => {
    expect(formatTokenRate("speed", 100, 200, 0)).toBe("");
    expect(formatTokenRate("burn", 100, 200, 0)).toBe("");
  });
  it("returns empty string for undefined duration", () => {
    expect(formatTokenRate("speed", 100, 200, undefined)).toBe("");
  });
  it("formats speed mode (output tok/s)", () => {
    expect(formatTokenRate("speed", 100, 200, 5000)).toBe("20 tok/s");
  });
  it("formats burn mode (total tok/min)", () => {
    expect(formatTokenRate("burn", undefined, 1000, 60000)).toBe("1K tok/min");
  });
  it("speed mode uses 0 for undefined output", () => {
    expect(formatTokenRate("speed", undefined, 200, 5000)).toBe("0 tok/s");
  });
  it("burn mode uses 0 for undefined total", () => {
    expect(formatTokenRate("burn", undefined, undefined, 5000)).toBe("0 tok/min");
  });
});
