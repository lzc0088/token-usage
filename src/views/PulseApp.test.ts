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

  it("expands the card on pulse:expand with lift + cardLeft", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    emit("pulse:expand", { vendor: "codex", cardLeft: true, lift: 120 });
    await flush();

    const tooltip = target.querySelector(".detail-tooltip");
    expect(tooltip).toBeTruthy();
    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    expect(panel?.style.marginTop).toBe("120px");
    expect(panel?.classList.contains("card-left")).toBe(true);
    expect(target.querySelector(".card-pointer")?.getAttribute("class")).toContain(
      "flip"
    );
  });

  it("clears expansion on pulse:collapse", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    emit("pulse:expand", { vendor: "codex", cardLeft: true, lift: 120 });
    await flush();
    emit("pulse:collapse", undefined);
    await flush();

    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    expect(panel?.style.marginTop).toBe("0px");
    expect(target.querySelector(".detail-tooltip")).toBeFalsy();
  });

  it("flush swoop carves the inner corner low and rises flat to the edge", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();

    const style =
      target.querySelector<HTMLElement>(".panel-surface")?.getAttribute("style") ?? "";
    expect(style).toContain("clip-path");
    // path("M 0 c C c1x c1y c2x c2y W 0 L W H C ...") — parse the TOP cap.
    const m = style.match(
      /M 0 ([\d.]+) C ([\d.]+) ([\d.]+) ([\d.]+) ([\d.]+) ([\d.]+) 0/
    );
    expect(m).toBeTruthy();
    const [c, c1x, c1y, c2x, c2y, w] = m!.slice(1).map(Number);

    // Leaf cap: inner corner carved deep, soft fillet (c1y just under c),
    // early lift (c2y small, c2x past mid-width), flat arrival at the edge.
    expect(c).toBeGreaterThan(36);
    expect(c1y).toBeLessThan(c);
    expect(c1x).toBeLessThan(w * 0.2);
    expect(c2y).toBeLessThan(c * 0.2);
    expect(c2x).toBeGreaterThan(w * 0.5);

    // Sample the cubic: the top cap must rise monotonically toward the edge.
    const bez = (t: number) =>
      (1 - t) ** 3 * c + 3 * (1 - t) ** 2 * t * c1y + 3 * (1 - t) * t * t * c2y;
    let prev = c;
    for (let i = 1; i <= 10; i++) {
      const y = bez(i / 10);
      expect(y).toBeLessThanOrEqual(prev + 0.001);
      prev = y;
    }
  });
});
