// PulseApp mount tests — drive the real event flow (pulse:update) with
// mocked Tauri APIs. The detail card lives in its own window now, so this
// window's hover contract is purely: invoke expand_pulse / collapse_pulse.
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

const SAMPLE = {
  quotas: [
    {
      vendor: "claude",
      plan: "Pro",
      status: "ok",
      windows: [{ label: "5h · sonnet", used_pct: 73 }],
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
      windows: [{ label: "5h", used_pct: 21 }],
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

/** Simulate the pointer entering/leaving the ring rail. */
function fireRail(target: HTMLElement, type: "mouseenter" | "mouseleave") {
  target
    .querySelector(".rail-with-tooltip")
    ?.dispatchEvent(new MouseEvent(type));
}

/** Simulate the pointer moving onto a specific vendor's ring. */
function enterRing(target: HTMLElement, index = 0) {
  target
    .querySelectorAll(".ring-container")
    [index]?.dispatchEvent(new MouseEvent("mouseenter"));
}

/** Let Svelte 5 flush its (microtask-batched) render effects. */
async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  listeners.clear();
  invokeMock.mockClear();
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

  it("hovers a ring → invokes expand_pulse with the vendor", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    fireRail(target, "mouseenter");
    enterRing(target, 1);

    expect(invokeMock).toHaveBeenCalledWith("expand_pulse", { vendor: "codex" });
    // This window never mounts a card itself — the card lives in its own
    // window shown by Rust.
    expect(target.querySelector(".detail-tooltip")).toBeFalsy();
  });

  it("leaves the panel after a real hover → collapses after the linger", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    fireRail(target, "mouseenter");
    enterRing(target, 0);
    await new Promise((r) => setTimeout(r, 100)); // hover a moment
    fireRail(target, "mouseleave");

    // Inside the linger window: no collapse yet (re-entry cancels).
    await new Promise((r) => setTimeout(r, 60));
    expect(invokeMock).not.toHaveBeenCalledWith("collapse_pulse");

    await new Promise((r) => setTimeout(r, 80));
    expect(invokeMock).toHaveBeenCalledWith("collapse_pulse");
  });

  it("fast sweep over a ring → collapses immediately (no card flash)", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    fireRail(target, "mouseenter");
    enterRing(target, 0);
    fireRail(target, "mouseleave");

    // delay 0 — collapse fires on the next macrotask, not after the linger.
    await flush();
    expect(invokeMock).toHaveBeenCalledWith("collapse_pulse");
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
