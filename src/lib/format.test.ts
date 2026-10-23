import { describe, expect, it } from "vitest";
import { formatChange, formatListeningTime, movementMarker } from "./format";
import { translator } from "./i18n/core";

const t = translator("en");
const HOUR = 3600;

describe("formatListeningTime", () => {
  it("rounds a long listening time to whole hours", () => {
    expect(formatListeningTime(142 * HOUR + 20 * 60, t)).toBe("≈ 142 h");
    expect(formatListeningTime(1234 * HOUR, t)).toBe("≈ 1,234 h");
  });

  it("keeps a tenth of an hour below ten hours", () => {
    expect(formatListeningTime(2.5 * HOUR, t)).toBe("≈ 2.5 h");
    expect(formatListeningTime(3 * HOUR, t)).toBe("≈ 3 h");
  });

  it("counts minutes below an hour", () => {
    expect(formatListeningTime(45 * 60 + 10, t)).toBe("≈ 45 min");
    // 59 min 50 s is about an hour, not "60 min".
    expect(formatListeningTime(59 * 60 + 50, t)).toBe("≈ 1 h");
  });

  it("writes the number and unit the way the language does", () => {
    expect(formatListeningTime(2.5 * HOUR, translator("de"))).toBe("≈ 2,5 Std.");
    expect(formatListeningTime(45 * 60, translator("fr"))).toBe("≈ 45 min");
  });
});

describe("formatChange", () => {
  it("signs a change in whole percent", () => {
    expect(formatChange(0.12, t)).toBe("+12%");
    expect(formatChange(-0.08, t)).toBe("-8%");
    expect(formatChange(2.5, t)).toBe("+250%");
  });

  it("leaves the sign off a change that rounds to nothing", () => {
    expect(formatChange(0.004, t)).toBe("0%");
    expect(formatChange(-0.004, t)).toBe("0%");
  });

  it("writes the percent the way the language does", () => {
    expect(formatChange(0.12, translator("de"))).toBe("+12 %");
  });
});

describe("movementMarker", () => {
  it("points up or down by the places moved", () => {
    expect(movementMarker({ kind: "moved", places: 37 }, t)).toEqual({
      direction: "up",
      text: "▲37",
      title: "up 37 places since the previous period",
    });
    expect(movementMarker({ kind: "moved", places: -1 }, t)).toEqual({
      direction: "down",
      text: "▼1",
      title: "down 1 place since the previous period",
    });
  });

  it("shows nothing for an entry that kept its place", () => {
    expect(movementMarker({ kind: "moved", places: 0 }, t)).toBeNull();
  });

  it("marks an entry the Previous period didn't have as new", () => {
    expect(movementMarker({ kind: "new" }, t)).toEqual({
      direction: "new",
      text: "new",
      title: "not played in the previous period",
    });
    expect(movementMarker({ kind: "new" }, translator("de"))?.text).toBe("neu");
  });
});
