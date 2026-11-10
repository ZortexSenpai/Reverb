import { describe, expect, it } from "vitest";
import { displacement, dropIndex, edgeScrollSpeed, ROW_HEIGHT } from "./queueDrag";

describe("dropIndex", () => {
  it("stays in its slot until dragged past half a row", () => {
    expect(dropIndex(3, ROW_HEIGHT / 2 - 1, 10)).toBe(3);
    expect(dropIndex(3, -(ROW_HEIGHT / 2 - 1), 10)).toBe(3);
  });

  it("lands a whole number of rows away, either way", () => {
    expect(dropIndex(3, 2 * ROW_HEIGHT, 10)).toBe(5);
    expect(dropIndex(3, -2 * ROW_HEIGHT - 10, 10)).toBe(1);
  });

  it("can't land before the first row or after the last", () => {
    expect(dropIndex(1, -5 * ROW_HEIGHT, 10)).toBe(0);
    expect(dropIndex(8, 5 * ROW_HEIGHT, 10)).toBe(9);
  });
});

describe("displacement", () => {
  it("moves the rows between up when dragging down", () => {
    expect([0, 1, 2, 3, 4, 5].map((i) => displacement(i, 1, 4))).toEqual([0, 0, -1, -1, -1, 0]);
  });

  it("moves the rows between down when dragging up", () => {
    expect([0, 1, 2, 3, 4, 5].map((i) => displacement(i, 4, 1))).toEqual([0, 1, 1, 1, 0, 0]);
  });

  it("moves nothing while the row hangs over its own slot", () => {
    expect([0, 1, 2].map((i) => displacement(i, 1, 1))).toEqual([0, 0, 0]);
  });
});

describe("edgeScrollSpeed", () => {
  it("doesn't scroll away from the edges", () => {
    expect(edgeScrollSpeed(300, 100, 600)).toBe(0);
  });

  it("scrolls up near the top and down near the bottom", () => {
    expect(edgeScrollSpeed(110, 100, 600)).toBeLessThan(0);
    expect(edgeScrollSpeed(590, 100, 600)).toBeGreaterThan(0);
  });

  it("speeds up toward the edge, up to a limit past it", () => {
    expect(Math.abs(edgeScrollSpeed(110, 100, 600))).toBeGreaterThan(
      Math.abs(edgeScrollSpeed(140, 100, 600)),
    );
    expect(edgeScrollSpeed(0, 100, 600)).toBe(-12);
    expect(edgeScrollSpeed(900, 100, 600)).toBe(12);
  });
});
