// Silhouette path generators for the pulse panel, ported from
// token-monitor's edgeDock/shapes.js: the flush rail grows out of the
// screen edge through two concave shoulders (an S-curve at each end), and
// the detail card's tail is a broad-necked arrowhead that blends into the
// card edge instead of a sharp triangle. Commands are emitted in CSS/SVG
// path syntax; the panel uses them for BOTH the clip-path (tint + blur)
// and the stroked outline, so the fill and the line share one edge.

/** Cubic control offset that best approximates a quarter circle. */
const ARC = 0.448;

type Cmd = ["M" | "L" | "C", ...number[]];

function mirrorX(cmds: Cmd[], width: number): Cmd[] {
  return cmds.map(([op, ...pts]) => {
    const next: Cmd = [op];
    for (let i = 0; i < pts.length; i += 2) {
      next.push(width - pts[i], pts[i + 1]);
    }
    return next;
  });
}

function toPath(cmds: Cmd[]): string {
  const r = (v: number) => Math.round(v * 100) / 100;
  return cmds.map(([op, ...pts]) => `${op}${pts.map(r).join(" ")}`).join(" ");
}

/** Rail silhouette flush with a screen edge: the body flows into the edge
 *  through two concave shoulders (top + bottom), interior corners rounded
 *  by `radius`. Drawn for the right edge (screen edge at x = w) and
 *  mirrored for the left. `shoulder` is the vertical run of each S-curve.
 *  Deliberately left OPEN (no Z): fills auto-close along the screen edge,
 *  while a stroked outline never draws a line along the display border. */
export function railPath(
  w: number,
  h: number,
  side: "left" | "right",
  shoulder: number,
  radius: number
): string {
  const s = Math.max(0, Math.min(shoulder, h / 2));
  const spread = Math.min(w * 0.53, w - radius);
  const r = Math.max(0, Math.min(radius, (h - 2 * s) / 2, w - spread));
  const cmds: Cmd[] = [
    ["M", w, 0],
    ["C", w, s * 0.76, w - w * 0.25, s, w - spread, s],
    ["L", r, s],
    ["C", r * ARC, s, 0, s + r * ARC, 0, s + r],
    ["L", 0, h - s - r],
    ["C", 0, h - s - r * ARC, r * ARC, h - s, r, h - s],
    ["L", w - spread, h - s],
    ["C", w - w * 0.25, h - s, w, h - s * 0.76, w, h],
  ];
  return toPath(side === "left" ? mirrorX(cmds, w) : cmds);
}

/** Detail-card tail — token-monitor's bubbleCommands tail, user-tuned:
 *  deeper (tail 18 vs 12) and a SHARP tip (ctrl offset ∓0.8 → vertex
 *  ≈11°). The local box is 18 × 36 with the card edge at x = 18
 *  and the tip at x = 1 (1px inside so antialiasing never clips); the
 *  center line is y = 18. The S-curve's first control point SUCKS UP TO
 *  the card edge (x = 18, y = ±0.4·neck) before the second control
 *  (x = tail/2, y ∓ 0.8) sweeps out to the tip — the curve leaves the
 *  edge vertically then bulges, giving the arrow its pronounced
 *  curvature. */
export const TAIL_W = 18;
export const TAIL_H = 36;
export const TAIL_PATH =
  "M 18 0 C 18 7.2 9 17.2 1 18 C 9 18.8 18 25.2 18 36 Z";

/** Outline variant of the tail: the same two curves WITHOUT the closing
 *  segment along the card edge (no Z) — stroking it draws the arrow's
 *  silhouette in the card's border color while the neck stays open, so the
 *  border appears to flow around the tail rather than cut through it.
 *  Same open-path convention as token-monitor's railCommands(outline).
 *  Renderers need stroke-miterlimit ≳ 10 for the ~11° tip — the default
 *  miterlimit of 4 silently bevels (blunts) it. */
export const TAIL_OUTLINE = "M 18 0 C 18 7.2 9 17.2 1 18 C 9 18.8 18 25.2 18 36";
