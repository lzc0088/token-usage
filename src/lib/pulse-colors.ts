// Per-vendor colors for the pulse rings and detail-card bars — every
// account gets its own hue (user preference: "每一家都不同"). The hue is
// the dock-order index walked around the color wheel by the GOLDEN ANGLE
// (137.508°): consecutive vendors land maximally far apart, so any 12
// vendors stay ≥20° separated (no collisions, unlike a per-id hash which
// empirically clusters). Usage severity is conveyed by the remaining-arc
// length / bar fill, not by the color.

/** Golden angle in degrees (360 × (1 − 1/φ)). */
const GOLDEN_ANGLE = 137.508;

/** Distinct color for the vendor at dock `index`. `dark` picks a
 *  lighter/darker lightness so the color reads on the card theme; the
 *  theme-neutral mid tone is used by the rings. */
export function vendorColor(index: number, dark?: boolean): string {
  const hue = ((index * GOLDEN_ANGLE) % 360 + 360) % 360;
  const l = dark === undefined ? 55 : dark ? 62 : 45;
  return `hsl(${hue.toFixed(2)} 72% ${l}%)`;
}
