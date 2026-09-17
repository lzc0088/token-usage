// Component test: the desktop widget card renders today's usage, the two
// tightest quota windows, and the 7-day sparkline from the existing IPC
// surface; the close button disables the widget in config.
import { describe, it, expect, vi, beforeEach } from "vitest";
import { tick, mount } from "svelte";
import WidgetApp from "./WidgetApp.svelte";

const invokeMock = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "widget", hide: vi.fn() }),
}));

vi.mock("../lib/api", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/api")>();
  return {
    ...orig,
    api: {
      ...orig.api,
      getConfig: async () => ({ currency: "both", language: "zh" }),
      getSummary: async () => ({
        period: "day", input: 1, output: 2, cache_read: 3, cache_write: 0, reasoning: 0,
        total_tokens: 79_072_426, cost_usd: 17.2, messages: 583,
        delta_pct: null, delta_label: null, live_rate_speed: 182.5,
      }),
      getQuotas: async () => [
        { vendor: "glm", status: "ok", refreshed_at: null, windows: [
          { label: "5h", used_pct: 34, resets_at: null },
        ] },
        { vendor: "kimi", status: "ok", refreshed_at: null, windows: [
          { label: "weekly", used_pct: 82, resets_at: null },
          { label: "monthly", used_pct: 68, resets_at: null },
        ] },
        { vendor: "qoder", status: "ok", refreshed_at: null, windows: [] },
      ],
      getTrends: async () => ({ points: Array.from({ length: 7 }, (_, i) => ({
        date: `2026-09-${i + 10}`, tokens: 40_000_000 + i * 5_000_000, cost_usd: 1, messages: 1,
      })) }),
      getLatestRate: async () => ({ rate: 7.16, date: "2026-09-17" }),
    },
  };
});

async function settle(n = 5): Promise<void> {
  for (let i = 0; i < n; i++) {
    await tick();
    await new Promise((r) => setTimeout(r, 8));
  }
}

describe("WidgetApp desktop card", () => {
  beforeEach(() => {
    invokeMock.mockClear();
  });

  it("renders today's usage, tightest quotas (desc) and sparkline", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(WidgetApp, { target });
    await settle();

    // Today's tokens (zh format: 万) and cost.
    expect(target.textContent).toContain("7907.24万");
    expect(target.textContent).toContain("17.20");
    // Rate readout from the differenced live rate.
    expect(target.textContent).toContain("183");
    // Quota rows: kimi weekly (82%) ranks above its monthly (68%); glm 5h
    // (34%) last — only the two tightest show.
    const rows = [...target.querySelectorAll(".wq-row")];
    expect(rows).toHaveLength(2);
    expect(rows[0]!.textContent).toContain("82");
    expect(rows[1]!.textContent).toContain("68");
    // 7-point sparkline.
    expect(target.querySelectorAll("svg polyline").length).toBeGreaterThanOrEqual(1);
    target.remove();
  });

  it("close button disables the widget in config", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(WidgetApp, { target });
    await settle();

    invokeMock.mockClear();
    const btn = target.querySelector<HTMLButtonElement>(".w-close");
    expect(btn).toBeTruthy();
    btn!.click();
    await settle();

    const call = invokeMock.mock.calls.find((c) => c[0] === "set_config");
    expect(call).toBeTruthy();
    expect(JSON.stringify(call![1])).toContain('"widget_enabled":false');
    target.remove();
  });
});
