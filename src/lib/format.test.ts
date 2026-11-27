import { describe, expect, it } from "vitest";
import { formatDsdRate } from "./format";

describe("formatDsdRate", () => {
  it("names the 44.1 kHz family by its multiple", () => {
    expect(formatDsdRate(2_822_400)).toBe("DSD64");
    expect(formatDsdRate(11_289_600)).toBe("DSD256");
    expect(formatDsdRate(45_158_400)).toBe("DSD1024");
  });

  it("marks the 48 kHz family", () => {
    expect(formatDsdRate(3_072_000)).toBe("DSD64 (48k)");
    expect(formatDsdRate(24_576_000)).toBe("DSD512 (48k)");
  });

  it("is null for PCM rates", () => {
    expect(formatDsdRate(44_100)).toBeNull();
    expect(formatDsdRate(705_600)).toBeNull();
    expect(formatDsdRate(1_536_000)).toBeNull();
    // A multiple, but not a power of two.
    expect(formatDsdRate(44_100 * 96)).toBeNull();
  });
});
