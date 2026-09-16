// Component test: expanding a breakdown row reveals the price-efficiency
// readout — $ per 1M tokens — alongside the existing detail rows.
import { describe, it, expect, vi } from "vitest";
import { tick, mount } from "svelte";
import BreakdownList from "./BreakdownList.svelte";

const invokeMock = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  Channel: class {},
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const ENTRIES = [
  {
    key: "glm-5.3", tokens: 10_000_000, token_pct: 70, cost_usd: 1.9, cost_pct: 60,
    messages: 100, input: 6_000_000, output: 500_000, cache_read: 3_500_000, cache_write: 0,
  },
  {
    key: "step-3.7-flash", tokens: 0, token_pct: 0, cost_usd: 0, cost_pct: 0,
    messages: 0, input: 0, output: 0, cache_read: 0, cache_write: 0,
  },
];

async function settle(n = 4): Promise<void> {
  for (let i = 0; i < n; i++) {
    await tick();
    await new Promise((r) => setTimeout(r, 8));
  }
}

function mountList(target: HTMLElement): void {
  mount(BreakdownList, {
    target,
    props: { entries: ENTRIES, currency: "usd", dim: "model" },
  });
}

describe("BreakdownList price-efficiency", () => {
  it("shows $ per 1M tokens in the expanded detail", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mountList(target);
    await settle();

    const rows = [...target.querySelectorAll(".bd-row")];
    rows[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await settle();

    const detail = target.querySelector(".bd-detail");
    expect(detail).toBeTruthy();
    // 10M tokens at $1.9 → $0.19 per 1M.
    expect(detail!.textContent).toContain("0.19");
    target.remove();
  });

  it("shows — for zero-token entries", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mountList(target);
    await settle();

    const rows = [...target.querySelectorAll(".bd-row")];
    rows[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await settle();

    const detail = target.querySelectorAll(".bd-detail");
    expect(detail.length).toBeGreaterThan(0);
    expect(detail[0]!.textContent).toContain("—");
    target.remove();
  });
});
