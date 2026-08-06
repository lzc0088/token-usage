// Pure array reorder: move the element at `from` to position `to` in the
// returned array. Both indices are 0-based and must be in `[0, arr.length)`.
// Out-of-range or from===to returns a copy of the input unchanged.
//
// Semantics verified by `moveTo.test.ts`:
//   moveTo([A,B,C,D], 0, 2) === [B,C,A,D]  (A → index 2)
//   moveTo([A,B,C,D], 1, 3) === [A,C,D,B]  (B → end)
//   moveTo([A,B,C,D], 2, 0) === [C,A,B,D]  (C → start)
//   moveTo([A,B,C,D], 1, 1) === [A,B,C,D]  (no-op)
export function moveTo<T>(arr: readonly T[], from: number, to: number): T[] {
  if (from < 0 || from >= arr.length) return arr.slice();
  if (to < 0 || to >= arr.length) return arr.slice();
  if (from === to) return arr.slice();
  const out = arr.slice();
  const [item] = out.splice(from, 1);
  out.splice(to, 0, item);
  return out;
}
