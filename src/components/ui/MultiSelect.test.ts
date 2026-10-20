import { describe, expect, it } from "vitest";
import { nextSelection, type MultiSelectOption } from "./MultiSelect";

const options: MultiSelectOption[] = ["a", "b", "c", "d"].map((value) => ({
  value,
  label: value.toUpperCase(),
}));

describe("nextSelection", () => {
  it("adds a value that was not selected", () => {
    expect(nextSelection(options, ["a"], "c")).toEqual(["a", "c"]);
  });

  it("removes a value that was selected", () => {
    expect(nextSelection(options, ["a", "c"], "a")).toEqual(["c"]);
  });

  it("only ever changes the one value — unlike a native multiple select, where a\
 plain click replaces the whole selection", () => {
    expect(nextSelection(options, ["a", "b", "c", "d"], "b")).toEqual(["a", "c", "d"]);
  });

  it("keeps the declared option order, not the click order", () => {
    const clicked = ["d", "b", "a"].reduce(
      (selected, value) => nextSelection(options, selected, value),
      [] as string[],
    );
    expect(clicked).toEqual(["a", "b", "d"]);
  });

  it("can empty the selection and fill it again", () => {
    expect(nextSelection(options, ["a"], "a")).toEqual([]);
    expect(nextSelection(options, [], "a")).toEqual(["a"]);
  });
});
