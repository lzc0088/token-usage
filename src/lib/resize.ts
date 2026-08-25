// JS-driven window resize for the main popover.
//
// Why not plain `win.setSize`: on macOS `setContentSize:` pins the window's
// BOTTOM-LEFT origin (AppKit coordinates), so dragging the bottom edge grows
// the window upward instead of the bottom following the mouse — the window
// visibly "moves" while resizing. The Rust command `resize_main_anchored`
// resizes AND compensates the origin per drag direction, so the grabbed edge
// follows the mouse and every other edge stays fixed. Resizing therefore
// never moves the window; header-drag is the only thing that repositions it.
//
// The native edge hotspots are disabled on the Rust side
// (set_resizable(false) in apply_window_size_constraints) so only these
// handles drive resizing.

import { invoke } from "@tauri-apps/api/core";

// Must mirror the Rust constraints in ui/window.rs (apply_window_size_constraints)
// and tauri.conf.json — programmatic setSize is not clamped by AppKit.
export const MIN_W = 340;
export const MIN_H = 400;
export const MAX_W = 500;
export const MAX_H = 1100;

const clamp = (v: number, lo: number, hi: number): number =>
  Math.min(hi, Math.max(lo, v));

/**
 * Start resizing the current window from a pointerdown on a resize handle.
 *
 * @param e      the pointerdown event (main button only)
 * @param dir    edge/corner direction: "n" | "s" | "e" | "w" | "ne" | …
 */
export function startWindowResize(e: PointerEvent, dir: string): void {
  if (e.button !== 0) return;
  e.preventDefault();

  const handle = e.currentTarget;
  if (!(handle instanceof HTMLElement)) return;
  handle.setPointerCapture(e.pointerId);

  const startX = e.clientX;
  const startY = e.clientY;
  // CSS px are logical px — matches the LogicalSize the Rust side expects.
  const startW = window.innerWidth;
  const startH = window.innerHeight;

  // Suppress blur-hide while the user is mid-resize (same as header drag).
  invoke("set_main_interacting", { interacting: true }).catch(() => {});

  const onMove = (ev: PointerEvent): void => {
    const dx = ev.clientX - startX;
    const dy = ev.clientY - startY;
    let w = startW;
    let h = startH;
    if (dir.includes("e")) w += dx;
    if (dir.includes("w")) w -= dx;
    if (dir.includes("s")) h += dy;
    if (dir.includes("n")) h -= dy;
    invoke("resize_main_anchored", {
      dir,
      width: clamp(w, MIN_W, MAX_W),
      height: clamp(h, MIN_H, MAX_H),
    }).catch(() => {});
  };

  const onEnd = (): void => {
    handle.removeEventListener("pointermove", onMove);
    handle.removeEventListener("pointerup", onEnd);
    handle.removeEventListener("pointercancel", onEnd);
    if (handle.hasPointerCapture(e.pointerId)) {
      handle.releasePointerCapture(e.pointerId);
    }
    invoke("set_main_interacting", { interacting: false }).catch(() => {});
  };

  handle.addEventListener("pointermove", onMove);
  handle.addEventListener("pointerup", onEnd);
  handle.addEventListener("pointercancel", onEnd);
}
