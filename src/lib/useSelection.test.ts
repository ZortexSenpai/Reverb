import { describe, expect, it } from "vitest";
import { pruneSelection, resolveClick } from "./useSelection";

const items = ["a", "b", "c", "d", "e"].map((id) => ({ id }));
const plain = { shift: false, toggle: false };
const toggle = { shift: false, toggle: true };
const shift = { shift: true, toggle: false };

function ids(set: Set<string>) {
  return [...set].sort();
}

describe("resolveClick", () => {
  it("replaces the selection on a plain click and anchors there", () => {
    const first = resolveClick(items, 3, plain, new Set(["a", "b"]), 0);
    expect(ids(first.ids)).toEqual(["d"]);
    expect(first.anchor).toBe(3);
  });

  it("adds and removes single rows with ctrl/meta", () => {
    const added = resolveClick(items, 2, toggle, new Set(["a"]), 0);
    expect(ids(added.ids)).toEqual(["a", "c"]);
    expect(added.anchor).toBe(2);

    const removed = resolveClick(items, 2, toggle, added.ids, added.anchor);
    expect(ids(removed.ids)).toEqual(["a"]);
  });

  it("extends from the anchor with shift, in either direction", () => {
    expect(ids(resolveClick(items, 3, shift, new Set(["b"]), 1).ids)).toEqual(["b", "c", "d"]);
    expect(ids(resolveClick(items, 0, shift, new Set(["c"]), 2).ids)).toEqual(["a", "b", "c"]);
  });

  it("keeps the anchor across repeated shift clicks, so a range can shrink", () => {
    const wide = resolveClick(items, 4, shift, new Set(["b"]), 1);
    expect(ids(wide.ids)).toEqual(["b", "c", "d", "e"]);
    const narrowed = resolveClick(items, 2, shift, wide.ids, wide.anchor);
    expect(ids(narrowed.ids)).toEqual(["b", "c"]);
  });

  it("falls back to a plain click when shift has no anchor", () => {
    const result = resolveClick(items, 2, shift, new Set(), null);
    expect(ids(result.ids)).toEqual(["c"]);
    expect(result.anchor).toBe(2);
  });

  it("ignores clicks on rows that are no longer there", () => {
    const result = resolveClick(items, 99, plain, new Set(["a"]), 0);
    expect(ids(result.ids)).toEqual(["a"]);
  });

  it("survives an anchor left behind by a longer list", () => {
    const result = resolveClick(items, 1, shift, new Set(["b"]), 42);
    expect(ids(result.ids)).toEqual(["b"]);
  });
});

describe("pruneSelection", () => {
  it("drops ids the list no longer contains", () => {
    expect(ids(new Set(pruneSelection(items, new Set(["a", "zz"]))))).toEqual(["a"]);
  });

  it("returns the same set when everything is still present", () => {
    const selected = new Set(["a", "b"]);
    expect(pruneSelection(items, selected)).toBe(selected);
  });
});
