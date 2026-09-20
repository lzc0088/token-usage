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

/** Detail-card tail — an EXACT port of token-monitor's bubbleCommands tail
 *  (same control-point formula, same metrics: neck 18, tail 12). The local
 *  box is 12 × 36 with the card edge at x = 12 and the tip at x = 1 (1px
 *  inside so antialiasing never clips); the center line is y = 18. The
 *  S-curve's first control point SUCKS UP TO the card edge (x = 12,
 *  y = ±0.4·neck) before the second control (x = tail/2, y ∓ 1.5) sweeps
 *  out to the tip — the curve leaves the edge vertically then bulges,
 *  which is what gives token-monitor's arrow its pronounced curvature. */
export const TAIL_W = 12;
export const TAIL_H = 36;
export const TAIL_PATH =
  "M 12 0 C 12 7.2 6 16.5 1 18 C 6 19.5 12 25.2 12 36 Z";
