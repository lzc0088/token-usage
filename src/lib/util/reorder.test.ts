import { describe, it, expect } from "vitest";
import { moveTo } from "./reorder";

describe("moveTo", () => {
  it("moves to a middle index", () => {
    expect(moveTo(["A", "B", "C", "D"], 0, 2)).toEqual(["B", "C", "A", "D"]);
    expect(moveTo(["A", "B", "C", "D"], 1, 2)).toEqual(["A", "C", "B", "D"]);
  });

  it("moves to the end", () => {
    expect(moveTo(["A", "B", "C", "D"], 0, 3)).toEqual(["B", "C", "D", "A"]);
    expect(moveTo(["A", "B", "C", "D"], 1, 3)).toEqual(["A", "C", "D", "B"]);
  });

  it("moves to the start", () => {
    expect(moveTo(["A", "B", "C", "D"], 2, 0)).toEqual(["C", "A", "B", "D"]);
    expect(moveTo(["A", "B", "C", "D"], 3, 0)).toEqual(["D", "A", "B", "C"]);
  });

  it("is a no-op when from === to (returns a copy)", () => {
    const arr = ["A", "B", "C", "D"];
    const out = moveTo(arr, 1, 1);
    expect(out).toEqual(["A", "B", "C", "D"]);
    expect(out).not.toBe(arr); // fresh copy, not same reference
  });

  it("ignores out-of-range indices (returns a copy)", () => {
    const arr = ["A", "B", "C"];
    expect(moveTo(arr, -1, 1)).toEqual(["A", "B", "C"]);
    expect(moveTo(arr, 1, 5)).toEqual(["A", "B", "C"]);
    expect(moveTo(arr, 5, 1)).toEqual(["A", "B", "C"]);
  });

  it("does not mutate the input array", () => {
    const arr = ["A", "B", "C", "D"];
    const before = [...arr];
    moveTo(arr, 0, 2);
    expect(arr).toEqual(before);
  });
});
