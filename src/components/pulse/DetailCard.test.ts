// DetailCard structure tests — the card paints its bg/border/tail on ONE
// compositing layer (.card-surface) so the tail fuses with the body and
// the panel-matching alpha applies uniformly via `opacity`.
import { mount } from "svelte";
import { describe, expect, it } from "vitest";
import DetailCard from "./DetailCard.svelte";

const QUOTA = {
  vendor: "claude",
  plan: "Pro",
  status: "ok",
  windows: [{ label: "5h · sonnet", used_pct: 42 }],
  critical_pct: 42,
  critical_label: "5h",
  expires_at: "2026-10-01T00:00:00Z",
};

function renderCard(props: Record<string, unknown> = {}): HTMLElement {
  const target = document.createElement("div");
  document.body.appendChild(target);
  mount(DetailCard, {
    target,
    props: { quota: QUOTA, cardSide: "left", ...props },
  });
  return target;
}

describe("DetailCard", () => {
  it("renders the surface layer containing the fused arrow", () => {
    const target = renderCard();
    const surface = target.querySelector(".card-surface");
    expect(surface).toBeTruthy();
    // The arrow lives INSIDE the surface layer — same compositing layer,
    // no dividing line at the junction.
    expect(surface?.querySelector(".card-arrow")).toBeTruthy();
    expect(target.querySelector(".card-body")).toBeTruthy();
  });

  it("exposes the panel alpha through the --card-alpha var", () => {
    const target = renderCard({ pulseAlpha: 0.5 });
    const style = target
      .querySelector<HTMLElement>(".detail-card")
      ?.getAttribute("style");
    expect(style).toContain("--card-alpha: 0.5");
  });

  it("keeps content readable regardless of alpha (body outside the layer)", () => {
    const target = renderCard({ pulseAlpha: 0.3, dark: true });
    const body = target.querySelector<HTMLElement>(".card-body");
    expect(body).toBeTruthy();
    // The body is a sibling AFTER the surface — it paints above it.
    const surface = target.querySelector(".card-surface");
    expect(
      surface && body && surface.compareDocumentPosition(body) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
  });
});
