// PulseCardApp tests — the card window view reacts to "pulse:card" events
// (vendor + side + theme/opacity) and renders the shared DetailCard.
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

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ label: "pulse-card" }),
  currentMonitor: async () => null,
}));

import PulseCardApp from "./PulseCardApp.svelte";

// jsdom lacks ResizeObserver (used by bind:clientHeight on the card slot).
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
      windows: [{ label: "5h · sonnet", used_pct: 73 }],
      critical_pct: 73,
      critical_label: "5h",
      expires_at: "2026-10-01T00:00:00Z",
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
  opacity: 0.7,
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

describe("PulseCardApp", () => {
  it("renders the hovered vendor's card on pulse:card", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseCardApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    emit("pulse:card", { vendor: "codex", cardOnLeft: false, theme: "dark", opacity: 0.7 });
    await flush();

    const card = target.querySelector<HTMLElement>(".detail-card");
    expect(card).toBeTruthy();
    expect(target.querySelector(".vendor-name")?.textContent).toContain("Codex");
    // Card right of the panel → arrow on the card's LEFT edge.
    expect(card?.classList.contains("card-right")).toBe(false);
  });

  it("flips the arrow side for a flush-right panel (cardOnLeft)", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseCardApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    emit("pulse:card", { vendor: "claude", cardOnLeft: true, theme: "light", opacity: 0.5 });
    await flush();

    const card = target.querySelector<HTMLElement>(".detail-card");
    expect(card?.classList.contains("card-right")).toBe(true);
    // Theme + opacity ride the payload.
    expect(card?.classList.contains("dark")).toBe(false);
    expect(card?.getAttribute("style")).toContain("--card-alpha: 0.5");
  });

  it("shows nothing until a vendor is selected", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseCardApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();

    expect(target.querySelector(".detail-card")).toBeFalsy();
  });

  it("keeps the card mounted on pulse:update while open", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(PulseCardApp, { target });

    emit("pulse:update", SAMPLE);
    await flush();
    emit("pulse:card", { vendor: "claude", cardOnLeft: true });
    await flush();
    // Live usage refresh re-pushes data — the card must stay.
    emit("pulse:update", { ...SAMPLE, quotas: SAMPLE.quotas });
    await flush();

    expect(target.querySelector(".detail-card")).toBeTruthy();
  });
});
