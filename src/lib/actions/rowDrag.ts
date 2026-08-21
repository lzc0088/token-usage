// Svelte action: pointer-drag reorder for a flat list row.
//
// Imports the Tauri shim so the action works in any webview, including
// non-Tauri contexts (vitest, plain web) — the shim no-ops there.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Svelte action: pointer-drag reorder for a flat list row.
//
// Usage:
//   <div use:rowDrag={{ id, onReorder: (newIndex) => reorder(id, newIndex) }}
//        data-row-id={id}>
//     …row content (with a button class included in `excludeSelector` if the
//     button must stay clickable for short presses).
//
// Behavior (matches the heuristics token-monitor hardened in practice):
//   - pointerdown starts a "pre-drag". The click is allowed if the pointer
//     doesn't travel past `threshold` (default 4px) — so buttons inside the
//     row (e.g. the enable toggle) keep working for short presses.
//   - Past the threshold, pointer capture is set on the row, a translucent
//     ghost follows the pointer, and the source row dims. Native window
//     dragging (`MovableByWindowBackground`) is suspended via the
//     `set_drag_suspended` Tauri command — critically on HOVER (pointerenter),
//     not on pointerdown: WKWebView runs this JS in a separate process, so an
//     IPC issued at pointerdown lands after AppKit has already claimed the
//     press+move as a window drag. Suspending while the cursor merely hovers
//     the row removes the race entirely; pointerleave resumes.
//   - On pointerup, the target index is the count of rows (in current DOM
//     order, including self) whose vertical midpoint is strictly below the
//     pointer — clamped to `[0, n-1]`. The parent then applies the reorder
//     via its own `moveTo(from, to)` (see `src/lib/util/reorder.ts`).
//   - The next `click` after a successful drop is suppressed (otherwise the
//     drop would also toggle the row's button).
//   - Aborts cleanly on Escape, window blur, or pointercancel. Window-drag
//     suspension is released on every abort path.

interface RowDragOptions {
  /** Stable identifier for this row (currently informational; the action looks
   *  up siblings via `data-row-id` and the parent applies the reorder). */
  id: string;
  /** Called once on drop. The argument is the desired FINAL index of this row
   *  in the list after the drop. */
  onReorder: (newIndex: number) => void;
  /** Selector for elements inside the row that should NOT trigger a drag
   *  (e.g. the enable toggle button). Pointerdown on these is ignored. */
  excludeSelector?: string;
  /** Distance (manhattan px) before a press is treated as a drag. Default 4. */
  threshold?: number;
  /** CSS selector to scope siblings when computing the drop target index.
   *  Only rows matching this selector are counted as siblings — useful for
   *  tree lists where each level forms its own reorder group (e.g.
   *  `[aria-level="2"]` for second-level children). When omitted, all
   *  sibling `[data-row-id]` elements are considered. */
  siblingSelector?: string;
}

const THRESHOLD_PX = 4;

export function rowDrag(node: HTMLElement, opts: RowDragOptions) {
  let options = opts;
  let startX = 0;
  let startY = 0;
  let dragging = false;
  let capturedPointerId: number | null = null;
  let ghost: HTMLElement | null = null;
  let suppressClick = false;

  /** Collect rows in the same reorder group as `node`. When
   *  `siblingSelector` is set, only rows matching that selector are
   *  included (e.g. `[aria-level="2"]` for tree level-2 items). */
  function collectGroup(): HTMLElement[] {
    const parent = node.parentElement;
    if (!parent) return [];
    const sel = options.siblingSelector;
    return Array.from(parent.children).filter(
      (c): c is HTMLElement =>
        c instanceof HTMLElement &&
        c.hasAttribute("data-row-id") &&
        (!sel || c.matches(sel)),
    );
  }

  function currentIndex(): number {
    return collectGroup().indexOf(node);
  }

  function computeTargetIndex(clientY: number): number {
    const rows = collectGroup();
    const n = rows.length;
    if (n === 0) return 0;
    let count = 0;
    for (const r of rows) {
      const mid = r.getBoundingClientRect().top + r.getBoundingClientRect().height / 2;
      if (mid < clientY) count += 1;
    }
    return Math.min(count, n - 1);
  }

  // Lazily resolved window label (the OS window this action is mounted in).
  // Cached after first call. Returns "" in non-Tauri contexts.
  let windowLabel: string | null = null;
  function getWindowLabel(): string {
    if (windowLabel !== null) return windowLabel;
    try {
      windowLabel = getCurrentWindow().label;
    } catch {
      windowLabel = "";
    }
    return windowLabel;
  }

  /** Suspend the OS-level window drag for the current webview so the row
   *  drag doesn't get hijacked by the window manager. No-op on web or
   *  non-Tauri contexts. */
  function suspendWindowDrag(): void {
    const label = getWindowLabel();
    if (!label) return;
    invoke("set_drag_suspended", { label, suspended: true }).catch(() => {
      /* drag suspension is a UX enhancement; failure is non-fatal */
    });
  }

  /** Release the OS-level window drag (paired with suspendWindowDrag). */
  function resumeWindowDrag(): void {
    const label = getWindowLabel();
    if (!label) return;
    invoke("set_drag_suspended", { label, suspended: false }).catch(() => {
      /* see suspendWindowDrag */
    });
  }

  function createGhost(): void {
    const r = node.getBoundingClientRect();
    const g = node.cloneNode(true) as HTMLElement;
    g.style.position = "fixed";
    g.style.left = `${r.left}px`;
    g.style.top = `${r.top}px`;
    g.style.width = `${r.width}px`;
    g.style.pointerEvents = "none";
    g.style.opacity = "0.7";
    g.style.zIndex = "9999";
    g.classList.add("row-drag-ghost");
    document.body.appendChild(g);
    ghost = g;
    node.classList.add("row-drag-source");
    suspendWindowDrag();
  }

  function cleanupGhost(): void {
    if (ghost) {
      ghost.remove();
      ghost = null;
    }
    node.classList.remove("row-drag-source");
    // NOTE: window-drag resume is deliberately NOT here — callers decide
    // based on where the pointer ended up (see endDrag / onPointerLeave).
  }

  /** True while the given point sits inside the row's box. Used to decide
   *  whether a resume is safe (pointer left → resume now) or whether the
   *  hover suspend should stand (pointer still over the row → the upcoming
   *  pointerleave will resume). */
  function isInsideRow(x: number, y: number): boolean {
    const r = node.getBoundingClientRect();
    return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
  }

  function teardownListeners(): void {
    document.removeEventListener("pointermove", onPointerMove);
    document.removeEventListener("pointerup", onPointerUp, true);
    document.removeEventListener("pointercancel", onPointerUp, true);
    document.removeEventListener("keydown", onKeyDown, true);
    window.removeEventListener("blur", onWindowBlur);
  }

  function startTracking(e: PointerEvent): void {
    startX = e.clientX;
    startY = e.clientY;
    dragging = false;
    capturedPointerId = e.pointerId;
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp, true);
    document.addEventListener("pointercancel", onPointerUp, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", onWindowBlur);
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return; // primary button only
    if (options.excludeSelector) {
      const t = e.target as Element | null;
      if (t && t.closest && t.closest(options.excludeSelector)) return;
    }
    startTracking(e);
    // Belt-and-suspenders: the hover suspend (pointerenter) has normally
    // already turned window dragging off by now; re-issuing costs nothing
    // (the Rust side dedupes) and covers pointers that press down without a
    // prior enter (e.g. touch).
    suspendWindowDrag();
  }

  function onPointerMove(e: PointerEvent): void {
    if (capturedPointerId !== e.pointerId) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (!dragging) {
      const threshold = options.threshold ?? THRESHOLD_PX;
      if (Math.abs(dx) + Math.abs(dy) < threshold) return;
      dragging = true;
      // Suppress the click that would follow pointerup so the drop doesn't
      // also fire a button-toggle inside the row.
      suppressClick = true;
      try {
        (e.target as Element).setPointerCapture?.(e.pointerId);
      } catch {
        // setPointerCapture may throw if the target was removed; safe to ignore.
      }
      createGhost();
    }
    if (ghost) {
      const r = node.getBoundingClientRect();
      ghost.style.transform = `translate(${e.clientX - startX}px, ${e.clientY - startY}px)`;
      // Keep the ghost anchored to the row's left edge (so the visual
      // ghost slides as the pointer moves, but its width stays the row's).
      void r;
    }
  }

  function endDrag(e: PointerEvent, commit: boolean): void {
    if (dragging) {
      const myIdx = currentIndex();
      if (commit && myIdx >= 0) {
        const target = computeTargetIndex(e.clientY);
        if (target !== myIdx) {
          options.onReorder(target);
        }
      }
      try {
        (e.target as Element).releasePointerCapture?.(e.pointerId);
      } catch {
        // ignore
      }
    }
    cleanupGhost();
    teardownListeners();
    capturedPointerId = null;
    // Defer clearing suppressClick so the click event (which fires after
    // pointerup) sees the flag and is cancelled.
    if (dragging) {
      setTimeout(() => {
        suppressClick = false;
      }, 0);
    }
    // Resume window dragging only when the pointer ended OUTSIDE the row;
    // if it's still over the row the hover suspend stands and the eventual
    // pointerleave resumes — resuming here would re-arm the AppKit race
    // for the very next press on this row.
    if (!isInsideRow(e.clientX, e.clientY)) {
      resumeWindowDrag();
    }
  }

  function onPointerUp(e: PointerEvent): void {
    endDrag(e, /* commit */ true);
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === "Escape" && capturedPointerId !== null) {
      // Abort. The pointer is (almost certainly) still over the row, so the
      // hover suspend stands — pointerleave will resume window dragging.
      cleanupGhost();
      teardownListeners();
      capturedPointerId = null;
    }
  }

  function onWindowBlur(): void {
    if (capturedPointerId !== null) {
      cleanupGhost();
      teardownListeners();
      capturedPointerId = null;
    }
    // The window lost focus (possibly hidden mid-hover). A lingering hover
    // suspend would keep dragging off after the next show, so resume now —
    // the Rust side no-ops when nothing is suspended.
    resumeWindowDrag();
  }

  // ── hover-scoped window-drag suspension ──
  // Suspending at pointerdown loses the race against AppKit's background
  // drag (WKWebView JS is a process hop away); suspending on hover wins by
  // a wide margin because no button is involved yet.
  function onPointerEnter(): void {
    suspendWindowDrag();
  }
  function onPointerLeave(): void {
    // Only resume when no press is active — a press's own end path decides.
    if (capturedPointerId === null) {
      resumeWindowDrag();
    }
  }

  function onClickCapture(e: MouseEvent): void {
    if (suppressClick) {
      e.stopPropagation();
      e.preventDefault();
      suppressClick = false;
    }
  }

  node.addEventListener("pointerdown", onPointerDown);
  node.addEventListener("pointerenter", onPointerEnter);
  node.addEventListener("pointerleave", onPointerLeave);
  // Suppress the post-drop click in the capture phase so it doesn't reach
  // inner buttons (which would toggle on drop).
  node.addEventListener("click", onClickCapture, true);

  return {
    update(newOpts: RowDragOptions): void {
      options = newOpts;
    },
    destroy(): void {
      teardownListeners();
      cleanupGhost();
      // Node leaving the DOM never sees a pointerleave — always resume.
      resumeWindowDrag();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointerenter", onPointerEnter);
      node.removeEventListener("pointerleave", onPointerLeave);
      node.removeEventListener("click", onClickCapture, true);
    },
  };
}
