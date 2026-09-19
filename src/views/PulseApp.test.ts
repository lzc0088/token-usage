// PulseApp mount tests — reproduce the "panel visible but no content" issue
// by driving the real event flow (pulse:update) with mocked Tauri APIs.
import { mount } from "svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { listeners, invokeMock } = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: unknown }) => void>();
  const invokeMock = vi.fn(async () => {});
  return { listeners, invokeMock };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    listeners.set(name, cb);
    return () => listeners.delete(name);
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

const onMovedCbs: Array<() => void> = [];
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "pulse",
    outerPosition: async () => ({ x: 1432, y: 300 }),
    outerSize: async () => ({ width: 80, height: 300 }),
    scaleFactor: async () => 2,
    startDragging: async () => {},
    onMoved: async (cb: () => void) => {
      onMovedCbs.push(cb);
      return () => {};
    },
  }),
  currentMonitor: async () => ({
    position: { x: 0, y: 0 },
    size: { width: 1512, height: 982 },
  }),
}));

import PulseApp from "./PulseApp.svelte";

// jsdom lacks ResizeObserver (used by bind:clientHeight on the tooltip).
class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}
if (typeof globalThis.ResizeObserver === "undefined") {
  (globalThis as Record<string, unknown>).ResizeObserver = ResizeObserverMock;
}

const SAMPLE = {
  quotas: [
    {
      vendor: "claude",
      plan: "Pro",
      status: "ok",
      windows: [{ label: "5h · sonnet", used_pct: 73, resets_at: null }],
      critical_pct: 73,
      critical_label: "5h",
      second_pct: 21,
      second_label: "week",
      is_running: false,
      is_refreshing: false,
    },
    {
      vendor: "codex",
      status: "ok",
      windows: [{ label: "5h", used_pct: 21, resets_at: null }],
      critical_pct: 21,
      critical_label: "5h",
    },
  ],
  size: "medium",
  ring_diameter: 48,
  theme: "dark",
};

function emit(name: string, payload: unknown) {
  listeners.get(name)?.({ payload });
}

/** Simulate the pointer entering the panel (arms the expand-event guard). */
function enterPanel(target: HTMLElement) {
  target
    .querySelector(".rail-with-tooltip")
    ?.dispatchEvent(new MouseEvent("mouseenter"));
}

/** Let Svelte 5 flush its (microtask-batched) render effects. */
async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  listeners.clear();
});

describe("PulseApp", () => {
  it("renders rings after pulse:update", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();

    const rings = target.querySelectorAll(".ring-container");
    expect(rings.length).toBe(2);
    expect(target.querySelector(".panel-title")?.textContent).toContain("使用量");
    expect(target.querySelector(".ring-icon svg")).toBeTruthy();
    expect(invokeMock).not.toHaveBeenCalledWith("collapse_pulse");
  });

  it("expands the card without shifting the panel position", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    enterPanel(target);
    emit("pulse:expand", { vendor: "codex", cardLeft: true });
    await flush();

    const tooltip = target.querySelector(".detail-tooltip");
    expect(tooltip).toBeTruthy();
    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    // No lift — the panel never moves on hover (top-left anchored window).
    expect(panel?.style.marginTop).toBe("");
    expect(panel?.classList.contains("card-left")).toBe(true);
    // card-right on the parent flips the arrow via CSS scaleX(-1)
    expect(target.querySelector(".detail-card")?.classList.contains("card-right")).toBe(true);
  });

  it("clears expansion on pulse:collapse", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    enterPanel(target);
    emit("pulse:expand", { vendor: "codex", cardLeft: true });
    await flush();
    emit("pulse:collapse", undefined);
    await flush();

    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    expect(panel?.style.marginTop).toBe("");
    expect(target.querySelector(".detail-tooltip")).toBeFalsy();
  });

  it("flush surface rounds the interior side and stays flat on the fused edge", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();

    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    expect(panel?.classList.contains("flush-right")).toBe(true);
    expect(target.querySelector(".panel-surface")).toBeTruthy();
    // Height mirrors the Rust formula: 2×PAD_V(30) + TITLE(24) + dock.
    // SAMPLE = 2 medium rings → dock = 2×(48+21) + 14 = 152 → 236px.
    expect(panel?.style.height).toBe("236px");
  });
});
