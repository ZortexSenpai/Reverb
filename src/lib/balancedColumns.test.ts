import { describe, expect, it } from "vitest";
import { balancedColumns } from "./balancedColumns";

describe("balancedColumns", () => {
  it("keeps everything in one row when it fits", () => {
    expect(balancedColumns(7, 9)).toBe(7);
    expect(balancedColumns(7, 7)).toBe(7);
  });

  it("splits a row that doesn't fit into rows of nearly equal length", () => {
    // 5 + 4, not 7 + 2.
    expect(balancedColumns(9, 7)).toBe(5);
    // 4 + 3, not 5 + 2.
    expect(balancedColumns(7, 5)).toBe(4);
    // Three rows of three, not 4 + 4 + 1.
    expect(balancedColumns(9, 4)).toBe(3);
  });

  it("shows at least one column, even with no room measured yet", () => {
    expect(balancedColumns(9, 0)).toBe(1);
    expect(balancedColumns(0, 5)).toBe(1);
  });
});
