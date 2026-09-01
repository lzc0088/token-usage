// Component test: mount the real Trend segment with mocked Tauri IPC and
// assert the rendered chart for the TOTAL period aggregates daily points
// into monthly buckets (7 nodes), while the heatmap keeps daily data.
import { describe, it, expect, vi, beforeEach } from "vitest";
import { tick, mount } from "svelte";
import Trend from "./Trend.svelte";
import { setPeriod } from "../../stores/period.svelte";

// ── Mocks (must precede imports that touch them) ───────────────────────────
const invokeMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {},
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({}),
}));

vi.mock("../../lib/api", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/api")>();
  return {
    ...orig,
    api: {
      ...orig.api,
      getTrends: async (p: string) => {
        invokeMock("get_trends", { period: p });
        // Real-shape data: month = 28 single-month daily points (August),
        // total = 90 daily points across 6 months — mirrors the backend.
        return { points: p === "month" ? FIXTURE_MONTH : FIXTURE_DAILY };
      },
      getSummary: async (p: string) => {
        invokeMock("get_summary", { period: p });
        return { ...FIXTURE_SUMMARY, period: p };
      },
    },
  };
});

// 28 daily points all in ONE month (2026-08) — the month-period dataset.
const FIXTURE_MONTH = Array.from({ length: 28 }, (_, i) => ({
  date: `2026-08-${String(i + 1).padStart(2, "0")}`,
  tokens: 1000,
  cost_usd: 0.1,
  messages: 1,
}));

// 90-ish daily points across 7 distinct months, mirroring real data shape.
const FIXTURE_DAILY = (() => {
  const pts: { date: string; tokens: number; cost_usd: number; messages: number }[] = [];
  const months = ["2025-10", "2025-11", "2026-03", "2026-06", "2026-07", "2026-08"];
  // 15 days per month × 6 months = 90 daily points.
  for (const m of months) {
    for (let d = 1; d <= 15; d++) {
      const day = String(d).padStart(2, "0");
      pts.push({ date: `${m}-${day}`, tokens: 1000, cost_usd: 0.1, messages: 1 });
    }
  }
  return pts;
})();

const FIXTURE_SUMMARY = {
  period: "total",
  input: 0,
  output: 0,
  cache_read: 0,
  cache_write: 0,
  reasoning: 0,
  total_tokens: 90_000,
  cost_usd: 9,
  messages: 90,
  active_days: 90,
  delta_pct: null,
  delta_label: null,
};

// jsdom lacks ResizeObserver (used by bind:clientWidth) — stub it.
class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
(globalThis as Record<string, unknown>).ResizeObserver = ResizeObserverStub;

describe("Trend TOTAL period rendering", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    setPeriod("day");
  });

  it("aggregates 90 daily points into 6 monthly chart nodes for total", async () => {
    setPeriod("total");
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Trend, {
      target,
      props: { currency: "usd" as const, cnyRate: 7.2 },
    });

    // Let the fetch effect + render settle.
    for (let i = 0; i < 5; i++) {
      await tick();
      await new Promise((r) => setTimeout(r, 5));
    }

    // The backend was asked for total.
    expect(invokeMock).toHaveBeenCalledWith("get_trends", { period: "total" });

    // Chart nodes: one SVG circle per chartPoint → 6 months, NOT 90 days.
    const nodes = target.querySelectorAll("svg .node");
    expect(nodes.length).toBe(6);

    // Monthly x-tick labels are YYYY-MM shaped.
    const ticks = [...target.querySelectorAll(".xtick")].map((el) => el.textContent?.trim());
    expect(ticks.length).toBeGreaterThan(0);
    expect(ticks.every((tk) => tk && /^\d{4}-\d{2}$/.test(tk))).toBe(true);

    target.remove();
  });

  it("renders daily nodes for month period (no aggregation)", async () => {
    setPeriod("month");
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Trend, {
      target,
      props: { currency: "usd" as const, cnyRate: 7.2 },
    });

    for (let i = 0; i < 5; i++) {
      await tick();
      await new Promise((r) => setTimeout(r, 5));
    }

    const nodes = target.querySelectorAll("svg .node");
    expect(nodes.length).toBe(28); // daily granularity for month period

    target.remove();
  });

  it("month → total transition re-renders and keeps reactivity alive", async () => {
    // Mirrors the user's exact repro: on trend tab with month selected,
    // click 累计. In the real app this silently killed ALL period effects.
    const errors: unknown[] = [];
    const onErr = (e: ErrorEvent) => errors.push(e.error ?? e.message);
    window.addEventListener("error", onErr);

    setPeriod("month");
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Trend, {
      target,
      props: { currency: "usd" as const, cnyRate: 7.2 },
    });
    for (let i = 0; i < 5; i++) {
      await tick();
      await new Promise((r) => setTimeout(r, 5));
    }
    expect(target.querySelectorAll("svg .node").length).toBe(28);

    // The transition under test.
    setPeriod("total");
    for (let i = 0; i < 8; i++) {
      await tick();
      await new Promise((r) => setTimeout(r, 10));
    }

    // Chart must converge to the total data's 6 monthly buckets (the stale
    // month data may briefly show as 1 bucket — timing-dependent, so assert
    // the settled state).
    const nodesAfter = target.querySelectorAll("svg .node");
    expect(nodesAfter.length).toBe(6);

    // Reactivity must survive: a further month click still re-renders.
    setPeriod("month");
    for (let i = 0; i < 5; i++) {
      await tick();
      await new Promise((r) => setTimeout(r, 10));
    }
    expect(target.querySelectorAll("svg .node").length).toBe(28);

    window.removeEventListener("error", onErr);
    expect(errors).toEqual([]);
    target.remove();
  });
});
