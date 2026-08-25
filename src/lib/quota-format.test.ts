import { describe, it, expect } from "vitest";
import {
  windowLabel,
  formatRefreshed,
  formatShortExpiry,
  nearestExpiry,
  formatExpiry,
  expiryUrgency,
  fmtCredits,
  formatReset,
  splitBalance,
} from "./quota-format";

describe("windowLabel", () => {
  it("returns Chinese label for known windows", () => {
    expect(windowLabel("5h","zh")).toBe("5小时");
    expect(windowLabel("周","zh")).toBe("7天");
    expect(windowLabel("月","zh")).toBe("每月");
    expect(windowLabel("MCP 月","zh")).toBe("MCP 每月");
  });
  it("falls back to raw value for unknown", () => {
    expect(windowLabel("unknown","zh")).toBe("unknown");
  });
});

describe("formatRefreshed", () => {
  const now = Date.parse("2026-07-28T12:00:00Z");

  it("returns empty for undefined", () => {
    expect(formatRefreshed(undefined, now)).toBe("");
  });
  it("returns empty for invalid date", () => {
    expect(formatRefreshed("not-a-date", now)).toBe("");
  });
  it("returns '刚刚刷新' for future dates", () => {
    expect(formatRefreshed("2026-07-28T12:00:01Z", now)).toBe("刚刚刷新");
  });
  it("returns seconds format with 刷新 suffix", () => {
    expect(formatRefreshed("2026-07-28T11:59:30Z", now)).toBe("30秒前刷新");
  });
  it("returns minutes format with 刷新 suffix", () => {
    expect(formatRefreshed("2026-07-28T11:30:00Z", now)).toBe("30分钟前刷新");
  });
  it("returns hours format with 刷新 suffix", () => {
    expect(formatRefreshed("2026-07-28T06:00:00Z", now)).toBe("6小时前刷新");
  });
});

describe("formatShortExpiry", () => {
  it("formats ISO date to YYYY-MM-DD", () => {
    expect(formatShortExpiry("2026-12-31T00:00:00Z")).toBe("2026-12-31");
  });
  it("returns empty for invalid input", () => {
    expect(formatShortExpiry("")).toBe("");
  });
});

describe("nearestExpiry", () => {
  const now = Date.parse("2026-07-28T12:00:00Z");
  it("returns undefined for empty input", () => {
    expect(nearestExpiry(undefined, now)).toBeUndefined();
  });
  it("returns undefined for past dates", () => {
    expect(nearestExpiry("2026-01-01T00:00:00Z", now)).toBeUndefined();
  });
  it("returns epoch ms for future dates", () => {
    const t = Date.parse("2026-12-31T00:00:00Z");
    expect(nearestExpiry("2026-12-31T00:00:00Z", now)).toBe(t);
  });
});

describe("formatExpiry", () => {
  const now = Date.parse("2026-07-28T12:00:00Z");
  it("returns empty for no expiry", () => {
    expect(formatExpiry(undefined, now)).toBe("");
  });
  it("formats future expiry with remaining time", () => {
    const result = formatExpiry("2026-12-31T00:00:00Z", now);
    expect(result).toContain("2026-12-31");
    expect(result).toContain("到期");
  });
});

describe("expiryUrgency", () => {
  const now = Date.parse("2026-07-28T12:00:00Z");
  it("returns 'exp-expired' for no expiry", () => {
    expect(expiryUrgency(undefined, now)).toBe("exp-expired");
  });
  it("returns 'exp-critical' for ≤3 days", () => {
    expect(expiryUrgency("2026-07-30T00:00:00Z", now)).toBe("exp-critical");
  });
  it("returns 'exp-soon' for ≤7 days", () => {
    expect(expiryUrgency("2026-08-03T00:00:00Z", now)).toBe("exp-soon");
  });
  it("returns 'exp-warn' for ≤30 days", () => {
    expect(expiryUrgency("2026-08-20T00:00:00Z", now)).toBe("exp-warn");
  });
  it("returns 'exp-ok' for >30 days", () => {
    expect(expiryUrgency("2026-09-30T00:00:00Z", now)).toBe("exp-ok");
  });
});

describe("fmtCredits", () => {
  it("returns '—' for null/undefined", () => {
    expect(fmtCredits(null)).toBe("—");
    expect(fmtCredits(undefined)).toBe("—");
  });
  it("formats integers with commas", () => {
    expect(fmtCredits(1500)).toBe("1,500");
  });
  it("formats floats with one decimal", () => {
    expect(fmtCredits(1234.5)).toBe("1,234.5");
  });
  it("formats small numbers", () => {
    expect(fmtCredits(99)).toBe("99");
  });
});

describe("formatReset", () => {
  const now = Date.parse("2026-07-28T12:00:00Z");
  it("returns empty for no reset time", () => {
    expect(formatReset(undefined, now)).toBe("");
  });
  it("returns '即将重置' for past/present", () => {
    expect(formatReset("2026-07-28T00:00:00Z", now)).toBe("即将重置");
  });
  it("formats hours+minutes with 后重置 suffix", () => {
    expect(formatReset("2026-07-28T15:30:00Z", now)).toBe("3小时30分钟后重置");
  });
  it("formats days+hours with 后重置 suffix", () => {
    expect(formatReset("2026-07-30T00:00:00Z", now)).toBe("1天12小时后重置");
  });
  it("formats minutes only with 后重置 suffix", () => {
    expect(formatReset("2026-07-28T12:45:00Z", now)).toBe("45分钟后重置");
  });
});

describe("splitBalance", () => {
  it("formats CNY with ¥ symbol", () => {
    expect(splitBalance("CNY", 1234.56)).toEqual({ unit: "¥", value: "1234.56" });
  });
  it("formats USD with $ symbol", () => {
    expect(splitBalance("USD", 99.99)).toEqual({ unit: "$", value: "99.99" });
  });
  it("returns bare number for unknown currency", () => {
    expect(splitBalance("EUR", 50)).toEqual({ unit: "", value: "50.00" });
  });
});
