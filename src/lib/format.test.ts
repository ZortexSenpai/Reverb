import { describe, expect, it } from "vitest";
import { formatListeningTime } from "./format";
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
