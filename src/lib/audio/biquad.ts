// RBJ Audio EQ Cookbook filter design — mirrors crates/reverb-core/src/audio/eq.rs
// exactly (same formulas, same clamps) so the curve you see is the curve you hear.

import type { EqBand } from "../subsonic/types";

export const GAIN_DB_LIMIT = 24;
export const Q_MIN = 0.1;
export const Q_MAX = 10;
export const FREQ_MIN = 20;
export const FREQ_MAX = 20000;

export interface BiquadCoeffs {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

export const IDENTITY: BiquadCoeffs = { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };

export function designBiquad(band: EqBand, sampleRate: number): BiquadCoeffs {
  if (!band.enabled) return IDENTITY;
  const freq = Math.min(Math.max(band.frequency, 10), 0.45 * sampleRate);
  const q = Math.min(Math.max(band.q, Q_MIN), Q_MAX);
  const gainDb = Math.min(Math.max(band.gainDb, -GAIN_DB_LIMIT), GAIN_DB_LIMIT);

  const w0 = (2 * Math.PI * freq) / sampleRate;
  const cos = Math.cos(w0);
  const sin = Math.sin(w0);
  const alpha = sin / (2 * q);
  const a = Math.pow(10, gainDb / 40);

  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;

  switch (band.type) {
    case "peaking": {
      if (Math.abs(gainDb) < 1e-9) return IDENTITY;
      b0 = 1 + alpha * a;
      b1 = -2 * cos;
      b2 = 1 - alpha * a;
      a0 = 1 + alpha / a;
      a1 = -2 * cos;
      a2 = 1 - alpha / a;
      break;
    }
    case "lowshelf": {
      const s = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 - (a - 1) * cos + s);
      b1 = 2 * a * (a - 1 - (a + 1) * cos);
      b2 = a * (a + 1 - (a - 1) * cos - s);
      a0 = a + 1 + (a - 1) * cos + s;
      a1 = -2 * (a - 1 + (a + 1) * cos);
      a2 = a + 1 + (a - 1) * cos - s;
      break;
    }
    case "highshelf": {
      const s = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 + (a - 1) * cos + s);
      b1 = -2 * a * (a - 1 + (a + 1) * cos);
      b2 = a * (a + 1 + (a - 1) * cos - s);
      a0 = a + 1 - (a - 1) * cos + s;
      a1 = 2 * (a - 1 - (a + 1) * cos);
      a2 = a + 1 - (a - 1) * cos - s;
      break;
    }
    case "lowpass": {
      b0 = (1 - cos) / 2;
      b1 = 1 - cos;
      b2 = (1 - cos) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    }
    case "highpass": {
      b0 = (1 + cos) / 2;
      b1 = -(1 + cos);
      b2 = (1 + cos) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    }
  }

  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

/** Magnitude response of one a0-normalized biquad at `freq`, in dB. */
export function magnitudeDb(c: BiquadCoeffs, freq: number, sampleRate: number): number {
  const w = (2 * Math.PI * freq) / sampleRate;
  const cw = Math.cos(w);
  const c2w = Math.cos(2 * w);
  const sw = Math.sin(w);
  const s2w = Math.sin(2 * w);
  const num = (c.b0 + c.b1 * cw + c.b2 * c2w) ** 2 + (c.b1 * sw + c.b2 * s2w) ** 2;
  const den = (1 + c.a1 * cw + c.a2 * c2w) ** 2 + (c.a1 * sw + c.a2 * s2w) ** 2;
  return 10 * Math.log10(num / den);
}

/** Log-spaced frequency points for curve rendering. */
export function logFrequencies(count: number): number[] {
  const out: number[] = [];
  const logMin = Math.log10(FREQ_MIN);
  const logMax = Math.log10(FREQ_MAX);
  for (let i = 0; i < count; i++) {
    out.push(Math.pow(10, logMin + ((logMax - logMin) * i) / (count - 1)));
  }
  return out;
}

/** Composite response: cascaded filters sum in dB. */
export function compositeResponseDb(
  bands: EqBand[],
  freqs: number[],
  sampleRate: number,
): number[] {
  const coeffs = bands.map((b) => designBiquad(b, sampleRate));
  return freqs.map((f) => coeffs.reduce((sum, c) => sum + magnitudeDb(c, f, sampleRate), 0));
}
