// Component test: the tracked-tools list search box filters rows by
// label/id substring (case-insensitive), composes with the status filter,
// and falls back to the empty hint when nothing matches.
import { describe, it, expect, vi, beforeEach } from "vitest";
import { tick, mount } from "svelte";
import Collection from "./Collection.svelte";

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
  getCurrentWindow: () => ({ label: "settings" }),
}));

const FIXTURE_TOOLS = [
  { client: "claude", label: "Claude Code", status: "active", message_count: 120 },
  { client: "codex", label: "Codex", status: "waiting", message_count: 0 },
  { client: "opencode", label: "OpenCode", status: "active", message_count: 30 },
  { client: "qoder", label: "Qoder", status: "missing", message_count: 0 },
];

vi.mock("../../lib/api", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/api")>();
  return {
    ...orig,
    api: {
      ...orig.api,
      getToolsStatus: async () => {
        invokeMock("get_tools_status");
        return FIXTURE_TOOLS;
      },
      getTokscaleStatus: async () => ({ installed: true, version: "4.15.1" }),
      getArchivedSessionCount: async () => 0,
    },
  };
});

async function settle(n = 5): Promise<void> {
  for (let i = 0; i < n; i++) {
    await tick();
    await new Promise((r) => setTimeout(r, 8));
  }
}

function rowLabels(target: HTMLElement): string[] {
  return [...target.querySelectorAll(".trow .tname")].map(
    (el) => (el.textContent ?? "").trim(),
  );
}

describe("Collection tools search", () => {
  beforeEach(() => {
    invokeMock.mockClear();
  });

  it("shows every tool with an empty query", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Collection, {
      target,
      props: { config: { currency: "both" }, onUpdate: () => {} },
    });
    await settle();
    expect(rowLabels(target)).toEqual(
      expect.arrayContaining(["Claude Code", "Codex", "OpenCode", "Qoder"]),
    );
    expect(target.querySelectorAll(".trow")).toHaveLength(4);
    target.remove();
  });

  it("filters by label substring, case-insensitive", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Collection, {
      target,
      props: { config: { currency: "both" }, onUpdate: () => {} },
    });
    await settle();

    const input = target.querySelector<HTMLInputElement>(".tool-search");
    expect(input).toBeTruthy();
    input!.value = "CLAUDE";
    input!.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();

    expect(rowLabels(target)).toEqual(["Claude Code"]);
    target.remove();
  });

  it("matches the raw client id too", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Collection, {
      target,
      props: { config: { currency: "both" }, onUpdate: () => {} },
    });
    await settle();

    const input = target.querySelector<HTMLInputElement>(".tool-search")!;
    input.value = "codex";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();

    expect(rowLabels(target)).toEqual(["Codex"]);
    target.remove();
  });

  it("composes with the status filter and shows the empty hint", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(Collection, {
      target,
      props: { config: { currency: "both" }, onUpdate: () => {} },
    });
    await settle();

    const input = target.querySelector<HTMLInputElement>(".tool-search")!;
    input.value = "zzz-no-match";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();

    expect(target.querySelectorAll(".trow")).toHaveLength(0);
    expect(target.textContent).toContain("该筛选下暂无工具");
    target.remove();
  });
});
