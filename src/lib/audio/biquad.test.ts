// Mirrors the Rust unit tests in crates/reverb-core/src/audio/eq.rs — both
// implementations must produce the same responses.

import { describe, expect, it } from "vitest";
import { designBiquad, IDENTITY, magnitudeDb } from "./biquad";
import type { EqBand } from "../subsonic/types";

function band(type: EqBand["type"], frequency: number, gainDb: number, q: number): EqBand {
  return { enabled: true, type, frequency, gainDb, q };
}

describe("designBiquad", () => {
  it("peaking filter has the requested gain at its center", () => {
    const c = designBiquad(band("peaking", 1000, 6, 1), 48000);
    expect(magnitudeDb(c, 1000, 48000)).toBeCloseTo(6, 2);
    expect(Math.abs(magnitudeDb(c, 20, 48000))).toBeLessThan(0.1);
    expect(Math.abs(magnitudeDb(c, 20000, 48000))).toBeLessThan(0.3);
  });

  it("low shelf boosts bass only", () => {
    const c = designBiquad(band("lowshelf", 200, 9, 0.707), 48000);
    expect(magnitudeDb(c, 20, 48000)).toBeCloseTo(9, 1);
    expect(Math.abs(magnitudeDb(c, 10000, 48000))).toBeLessThan(0.1);
  });

  it("highpass rolls off DC", () => {
    const c = designBiquad(band("highpass", 100, 0, 0.707), 48000);
    expect(magnitudeDb(c, 10, 48000)).toBeLessThan(-30);
    expect(Math.abs(magnitudeDb(c, 1000, 48000))).toBeLessThan(0.1);
  });

  it("disabled band is identity", () => {
    const b = { ...band("peaking", 1000, 6, 1), enabled: false };
    expect(designBiquad(b, 48000)).toEqual(IDENTITY);
  });

  it("zero-gain peaking is identity", () => {
    expect(designBiquad(band("peaking", 1000, 0, 1), 48000)).toEqual(IDENTITY);
  });
});
