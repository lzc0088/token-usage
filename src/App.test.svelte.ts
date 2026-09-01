// Full-App repro: mount the real App shell with mocked Tauri IPC and
// period-shaped data (mirrors the real get_summary/get_trends responses),
// then click 本月 → 累计 like the user does. The real app throws
// `each_key_duplicate` on this transition — this test must surface it.
import { describe, it, expect, vi, beforeEach } from "vitest";
import { tick, mount } from "svelte";
import App from "./App.svelte";
import { setPeriod } from "./stores/period.svelte";
import { setSegment } from "./stores/segment.svelte";

const invokeMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "main", hide: () => {}, onResized: async () => () => {} }),
}));
vi.mock("./lib/resize", () => ({ startWindowResize: () => {} }));
vi.mock("./lib/update", () => ({ checkForUpdate: async () => {} }));

// Real-shaped responses per period.
const SUMMARY_MONTH = {
  period: "month", input: 1, output: 2, cache_read: 3, cache_write: 4, reasoning: 0,
  total_tokens: 3_940_145_168, cost_usd: 123.4, messages: 999, active_days: 28,
  delta_pct: 42.5, delta_label: "较上月",
};
const SUMMARY_TOTAL = {
  period: "total", input: 1, output: 2, cache_read: 3, cache_write: 4, reasoning: 0,
  total_tokens: 10_350_156_580, cost_usd: 456.7, messages: 9999, active_days: 88,
  delta_pct: null, delta_label: null,
};
const SUMMARY_DAY = {
  period: "day", input: 1, output: 2, cache_read: 3, cache_write: 4, reasoning: 0,
  total_tokens: 172_182_679, cost_usd: 12.3, messages: 99, active_days: 1,
  delta_pct: -5.1, delta_label: "较昨日",
  timed_output_tokens: 1000, timed_tokens: 2000, timed_duration_ms: 5000,
};
const TRENDS_MONTH = Array.from({ length: 28 }, (_, i) => ({
  date: `2026-08-${String(i + 1).padStart(2, "0")}`,
  tokens: 140_000_000, cost_usd: 4.4, messages: 35,
}));
const TRENDS_TOTAL = [
  ["2025-04", 1], ["2025-10", 2], ["2025-11", 3], ["2026-03", 4],
  ["2026-06", 5], ["2026-07", 6], ["2026-08", 7],
].flatMap(([m, seed]) =>
  Array.from({ length: 13 }, (_, i) => ({
    date: `${m}-${String(i + 1).padStart(2, "0")}`,
    tokens: 100_000_000 * (seed as number), cost_usd: 1.5, messages: 20,
  })),
);

vi.mock("./lib/api", async (importOriginal) => {
  const orig = await importOriginal<typeof import("./lib/api")>();
  return {
    ...orig,
    api: {
      ...orig.api,
      getConfig: async () => {
        invokeMock("get_config");
        return { currency: "both", language: "zh", default_period: "day" };
      },
      getSummary: async (p: string) => {
        invokeMock("get_summary", { period: p });
        return p === "month" ? SUMMARY_MONTH : p === "total" ? SUMMARY_TOTAL : SUMMARY_DAY;
      },
      getTrends: async (p: string) => {
        invokeMock("get_trends", { period: p });
        return { points: p === "month" ? TRENDS_MONTH : p === "total" ? TRENDS_TOTAL : TRENDS_MONTH.slice(0, 7) };
      },
      getLatestRate: async () => ({ rate: 6.72, date: "2026-09-01" }),
      getAppVersion: async () => "1.0.14",
      checkUpdate: async () => ({ has_update: false }),
      getBreakdown: async () => ({ entries: [] }),
      getQuotas: async () => [],
      refreshQuotasIfStale: async () => false,
    },
  };
});

class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
(globalThis as Record<string, unknown>).ResizeObserver = ResizeObserverStub;
// jsdom lacks matchMedia (used by the appearance module's theme listeners).
window.matchMedia = window.matchMedia ?? (() => ({
  matches: false,
  addEventListener: () => {},
  removeEventListener: () => {},
  addListener: () => {},
  removeListener: () => {},
  dispatchEvent: () => false,
}));

async function settle(n = 6): Promise<void> {
  for (let i = 0; i < n; i++) {
    await tick();
    await new Promise((r) => setTimeout(r, 8));
  }
}

describe("App 累计 transition (real repro)", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    setPeriod("day");
    setSegment("ov");
  });

  it("month → total keeps all pages reactive", async () => {
    const errors: string[] = [];
    const onErr = (e: ErrorEvent): void => {
      errors.push(e.error instanceof Error ? `${e.error.message}\n${e.error.stack}` : e.message);
    };
    window.addEventListener("error", onErr);

    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(App, { target });
    await settle();

    // Month first.
    setPeriod("month");
    await settle();
    // Then the fatal transition.
    setPeriod("total");
    await settle(10);

    // Hero must now show the TOTAL summary.
    const hero = target.querySelector('[data-testid="hero-section"]');
    expect(hero).toBeTruthy();
    expect(hero!.textContent).toContain("1.035百亿"); // 10_350_156_580 (zh format)

    // Reactivity must survive further clicks.
    setPeriod("month");
    await settle();
    expect(hero!.textContent).toContain("3.94"); // 3_940_145_168 → 3.94十亿

    window.removeEventListener("error", onErr);
    expect(errors).toEqual([]);
    target.remove();
  });

  it("on TREND tab: month → total must not kill reactivity (the real repro)", async () => {
    const errors: string[] = [];
    const onErr = (e: ErrorEvent): void => {
      errors.push(e.error instanceof Error ? `${e.error.message}\n${e.error.stack}` : e.message);
    };
    window.addEventListener("error", onErr);

    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(App, { target });
    await settle();

    // Switch to the trend tab first (the real crash context).
    setSegment("trend");
    await settle();

    setPeriod("month");
    await settle();
    // The fatal transition.
    setPeriod("total");
    await settle(12);

    // The trend chart must render monthly nodes (7 months fixture).
    const nodes = target.querySelectorAll("svg .node");
    expect(nodes.length).toBe(7);

    // Hero must show total data.
    const hero = target.querySelector('[data-testid="hero-section"]');
    expect(hero!.textContent).toContain("1.035百亿");

    // Reactivity survives: back to month re-renders daily nodes.
    setPeriod("month");
    await settle();
    expect(target.querySelectorAll("svg .node").length).toBe(28);
    expect(hero!.textContent).toContain("3.94");

    window.removeEventListener("error", onErr);
    expect(errors).toEqual([]);
    target.remove();
  });
});
