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

  it("shows balance rows ABOVE the plan section when both exist", () => {
    const target = renderCard({
      quota: {
        ...QUOTA,
        balance: {
          amount: 12.34,
          currency: "CNY",
          today_consumption: 1.5,
          month_consumption: 9.9,
        },
      },
    });
    const balanceRow = target.querySelector(".stat-row");
    const bars = target.querySelector(".window-bars");
    expect(balanceRow).toBeTruthy();
    expect(bars).toBeTruthy();
    // The first stat row (账户余额) must precede the progress-bar section.
    expect(balanceRow?.textContent).toContain("账户余额");
    expect(
      balanceRow &&
        bars &&
        balanceRow.compareDocumentPosition(bars) &
          Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();
    // Consumption rows render too.
    expect(target.textContent).toContain("今日消费");
    expect(target.textContent).toContain("月度消费");
  });

  it("renders title+bar+pct on one line with the reset centered below", () => {
    const target = renderCard({
      quota: {
        ...QUOTA,
        windows: [
          {
            label: "5h · sonnet",
            used_pct: 42,
            resets_at: "2027-01-01T00:00:00Z",
            total_value: 100,
            used_value: 58,
          },
        ],
      },
    });
    // Title, track, pct share ONE row, in order.
    const row = target.querySelector(".ws-bar-row");
    expect(row).toBeTruthy();
    const kids = row ? [...row.children] : [];
    expect(kids[0]?.classList.contains("ws-title")).toBe(true);
    expect(kids[1]?.classList.contains("ws-track")).toBe(true);
    expect(kids[2]?.classList.contains("ws-pct")).toBe(true);
    // REMAINING mode — the card mirrors the ring: 42% used → 58.00% left,
    // and the bar fill width carries the remaining share too.
    expect(kids[2]?.textContent).toContain("58.00");
    const fill = row?.querySelector<HTMLElement>(".ws-fill");
    expect(fill?.getAttribute("style")).toContain("width: 58%");
    // The reset line sits below the bar row, centered.
    const reset = target.querySelector(".ws-reset");
    expect(reset).toBeTruthy();
    expect(reset?.textContent).toContain("重置");
    // The old three-line layout (ws-value with 剩余) is gone.
    expect(target.querySelector(".ws-value")).toBeFalsy();
  });

  it("fixes the title/pct widths so columns align across rows", () => {
    const target = renderCard({
      quota: {
        ...QUOTA,
        windows: [
          { label: "5h", used_pct: 42 },
          { label: "MCP 月", used_pct: 10 },
          { label: "周", used_pct: 88 },
        ],
      },
    });
    // Identical fixed boxes for every row → title/bar/pct columns align
    // (inline widths — a layout contract, assertable without a layout engine).
    const titles = [...target.querySelectorAll<HTMLElement>(".ws-title")];
    expect(titles.map((t) => t.style.width)).toEqual(["54px", "54px", "54px"]);
    const pcts = [...target.querySelectorAll<HTMLElement>(".ws-pct")];
    expect(pcts.map((p) => p.style.width)).toEqual(["44px", "44px", "44px"]);
  });

  it("splits the header into left/right regions closed by a dashed divider", () => {
    const target = renderCard();
    const left = target.querySelector(".header-left");
    const right = target.querySelector(".header-right");
    expect(left).toBeTruthy();
    expect(right).toBeTruthy();
    // Left region: logo + vendor name only.
    expect(left?.querySelector(".vendor-icon")).toBeTruthy();
    expect(left?.querySelector(".vendor-name")?.textContent).toContain("Claude");
    expect(left?.querySelector(".plan-badge")).toBeFalsy();
    // Right region: plan badge on the first line, expiry under it.
    const rightChildren = right ? [...right.children] : [];
    expect(rightChildren[0]?.classList.contains("plan-badge")).toBe(true);
    expect(rightChildren[1]?.textContent).toContain("到期");
    // Dashed divider closes the header block with even spacing.
    expect(target.querySelector(".card-header .header-divider")).toBeTruthy();
  });

  it("keeps credits row first for planless vendors", () => {
    const target = renderCard({
      quota: {
        vendor: "deepseek",
        planless: true,
        status: "ok",
        windows: [{ label: "余额", used_pct: 0, total_value: 100, used_value: 40 }],
        critical_pct: 0,
        critical_label: "",
        balance: { amount: 88.8, currency: "CNY" },
      },
    });
    expect(target.querySelector(".window-bars")).toBeFalsy();
    const firstRow = target.querySelector(".stat-row");
    expect(firstRow?.textContent).toContain("剩余 Credits");
    // Balance rows follow the credits row.
    const labels = [...target.querySelectorAll(".stat-label")].map(
      (el) => el.textContent
    );
    expect(labels.indexOf("剩余 Credits")).toBeLessThan(
      labels.indexOf("账户余额")
    );
  });
});
