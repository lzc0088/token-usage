// PulsePeekApp mount tests — the auto-hide handle window renders a fixed
// 8×58 silhouette (shared railPath generator) plus a centered accent pill.
// The window itself never resizes; show/hide is owned by the Rust poller.
import { mount } from "svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { listeners, invokeMock } = vi.hoisted(() => {
  const listeners = new Map<string, (e: { payload: unknown }) => void>();
  // Resolve undefined: the component's `if (d)` guard must not clobber
  // event-pushed state with a truthy-but-empty mount response.
  const invokeMock = vi.fn(async () => undefined);
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

import PulsePeekApp from "./PulsePeekApp.svelte";

const SAMPLE = {
  quotas: [],
  size: "medium",
  ring_diameter: 48,
  theme: "dark",
  side: "right",
  opacity: 0.9,
};

function emit(name: string, payload: unknown) {
  listeners.get(name)?.({ payload });
}

async function flush() {
  await new Promise((r) => setTimeout(r, 0));
}

beforeEach(() => {
  listeners.clear();
  invokeMock.mockClear();
});

describe("PulsePeekApp", () => {
  it("renders the edge silhouette and accent pill after pulse:update", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulsePeekApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();

    const root = target.querySelector<HTMLElement>(".peek-root");
    expect(root).toBeTruthy();
    // Silhouette drawn from the shared rail generator (shoulder curves).
    const fill = root?.querySelector<SVGPathElement>(".shape-fill");
    expect(fill?.getAttribute("d")).toBeTruthy();
    // The pill is the cursor affordance.
    expect(root?.querySelector(".peek-pill")).toBeTruthy();
  });

  it("mirrors the silhouette for the left edge", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulsePeekApp, { target });

    emit("pulse:update", { ...SAMPLE, side: "left" });
    await flush();

    const pill = target.querySelector<HTMLElement>(".peek-pill");
    expect(pill?.classList.contains("pill-left")).toBe(true);
  });

  it("pulls initial data on mount (event-only windows blank after reload)", () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulsePeekApp, { target });
    expect(invokeMock).toHaveBeenCalledWith("get_pulse_data");
  });
});
