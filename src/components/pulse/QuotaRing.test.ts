// QuotaRing render tests — reproduce the "ring shows but no content/icon"
// regression in isolation (no Tauri APIs involved; pure component).
import { mount } from "svelte";
import { describe, expect, it } from "vitest";
import QuotaRing from "./QuotaRing.svelte";

interface RingProps {
  vendor?: string;
  pct?: number;
  label?: string;
  diameter?: number;
}

function renderRing(props: RingProps = {}): HTMLElement {
  const target = document.createElement("div");
  document.body.appendChild(target);
  mount(QuotaRing, {
    target,
    props: {
      vendor: props.vendor ?? "claude",
      pct: props.pct ?? 40,
      label: props.label ?? "5h",
      diameter: props.diameter ?? 48,
      onHover: () => {},
      onLeave: () => {},
    },
  });
  return target;
}

describe("QuotaRing", () => {
  it("renders the ring svg with a usage arc", () => {
    const target = renderRing();
    expect(target.querySelector(".ring-svg")).toBeTruthy();
    expect(target.querySelectorAll(".ring-svg circle").length).toBeGreaterThan(0);
  });

  it("renders the shared brand icon inside the ring", () => {
    const target = renderRing({ vendor: "claude" });
    const icon = target.querySelector(".ring-icon svg");
    expect(icon).toBeTruthy();
    const glyph = target.querySelector(
      ".ring-icon svg path, .ring-icon svg circle, .ring-icon svg ellipse"
    );
    expect(glyph).toBeTruthy();
  });

  it("falls back to a generic glyph for unknown vendors", () => {
    const target = renderRing({ vendor: "openrouter" });
    expect(target.querySelector(".ring-icon svg ellipse")).toBeTruthy();
  });

  it("renders the pct label below the ring", () => {
    const target = renderRing({ pct: 73 });
    expect(target.querySelector(".pct-label")?.textContent).toContain("73%");
  });

  it("shows the credits sub-label instead of pct for plan-less vendors", () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    mount(QuotaRing, {
      target,
      props: {
        vendor: "workbuddy",
        pct: 40,
        label: "credits",
        diameter: 48,
        onHover: () => {},
        onLeave: () => {},
        subLabel: "1,234",
      },
    });
    expect(target.querySelector(".pct-label")?.textContent).toBe("1,234");
  });

  it("sizes the icon inside the ring hole", () => {
    const target = renderRing({ diameter: 48 });
    const icon = target.querySelector<HTMLElement>(".ring-icon");
    expect(icon).toBeTruthy();
    expect(icon?.style.width).toBe("19px");
  });
});
