// PulseApp mount tests — drive the real event flow (pulse:update) with
// mocked Tauri APIs. Hover show/hide is owned by the Rust cursor poller,
// so this window has no hover contract at all — it just renders rings.
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
    expect(target.querySelector(".panel-title")?.textContent).toContain("剩余量");
    expect(target.querySelector(".ring-icon svg")).toBeTruthy();
    // Remaining-amount mode: the label under a 73%-used ring shows 27%.
    expect(target.querySelector(".pct-label")?.textContent).toContain("27");
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
    // Height mirrors the Rust formula: 2×PAD_V(30) + TITLE(27) + dock.
    // SAMPLE = 2 medium rings → dock = 2×(48+21) + 14 = 152 → 239px.
    expect(panel?.style.height).toBe("239px");
  });

  it("caps the dock at the payload's screen-relative max (80%)", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    // Many vendors + a small screen cap → the dock scrolls inside the cap.
    const many = {
      ...SAMPLE,
      quotas: Array.from({ length: 12 }, (_, i) => ({
        ...SAMPLE.quotas[0],
        vendor: `v${i}`,
      })),
      max_panel_h: 200,
    };
    emit("pulse:update", many);
    await flush();

    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    // Panel height = max_panel_h (dock capped at 200 - 87 = 113).
    expect(panel?.style.height).toBe("200px");
    const dock = target.querySelector<HTMLElement>(".ring-dock");
    expect(dock?.style.maxHeight).toBe("113px");
  });

  it("scales typography and blocks with the size preset", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseApp, { target });

    // Large: s = 60/48 = 1.25 → dock = 2×(60+26.25) + 17.5 = 190,
    // height = 87×1.25 + 190 = 298.75.
    emit("pulse:update", { ...SAMPLE, size: "large", ring_diameter: 60 });
    await flush();

    const panel = target.querySelector<HTMLElement>(".pulse-panel");
    expect(panel?.style.height).toBe("298.75px");
    // The --s scale var drives the CSS block metrics (padding/title/gaps).
    expect(panel?.getAttribute("style")).toContain("--s: 1.25");
  });
});
