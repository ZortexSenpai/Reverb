//! Roon-style parametric EQ.
//!
//! Coefficient design happens on the control plane (RBJ Audio EQ Cookbook);
//! the real-time callback only runs the biquad chain and two click-suppression
//! mechanisms: per-block one-pole coefficient smoothing (for slider drags) and
//! an equal-power dry/wet crossfade (for enable/bypass/preset switches).
//!
//! Everything published to the callback is `Copy` and fixed-size — the
//! real-time path never allocates.

use serde::{Deserialize, Serialize};

pub const MAX_BANDS: usize = 10;
pub const GAIN_DB_LIMIT: f32 = 24.0;
pub const Q_MIN: f32 = 0.1;
pub const Q_MAX: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterType {
    Peaking,
    Lowshelf,
    Highshelf,
    Lowpass,
    Highpass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub filter_type: FilterType,
    /// Center / corner frequency in Hz.
    pub frequency: f32,
    /// Gain in dB (ignored for lowpass/highpass).
    pub gain_db: f32,
    pub q: f32,
}

impl Default for EqBand {
    fn default() -> Self {
        Self { enabled: true, filter_type: FilterType::Peaking, frequency: 1000.0, gain_db: 0.0, q: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum PreampMode {
    /// -max(0, max enabled band gain) — always click-safe headroom.
    Auto,
    Manual { db: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqState {
    pub enabled: bool,
    pub bands: Vec<EqBand>,
    pub preamp: PreampMode,
}

impl Default for EqState {
    fn default() -> Self {
        Self { enabled: false, bands: Vec::new(), preamp: PreampMode::Auto }
    }
}

/// a0-normalized biquad coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl BiquadCoeffs {
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// RBJ Audio EQ Cookbook designs (Q-parameterized shelves).
    pub fn design(band: &EqBand, sample_rate: f64) -> Self {
        if !band.enabled {
            return Self::IDENTITY;
        }
        let freq = (band.frequency as f64).clamp(10.0, 0.45 * sample_rate);
        let q = (band.q as f64).clamp(Q_MIN as f64, Q_MAX as f64);
        let gain_db = (band.gain_db as f64).clamp(-(GAIN_DB_LIMIT as f64), GAIN_DB_LIMIT as f64);

        let w0 = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let cos = w0.cos();
        let sin = w0.sin();
        let alpha = sin / (2.0 * q);
        let a = 10f64.powf(gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match band.filter_type {
            FilterType::Peaking => {
                if gain_db.abs() < 1e-9 {
                    return Self::IDENTITY;
                }
                (
                    1.0 + alpha * a,
                    -2.0 * cos,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cos,
                    1.0 - alpha / a,
                )
            }
            FilterType::Lowshelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            FilterType::Highshelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
            FilterType::Lowpass => (
                (1.0 - cos) / 2.0,
                1.0 - cos,
                (1.0 - cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
            FilterType::Highpass => (
                (1.0 + cos) / 2.0,
                -(1.0 + cos),
                (1.0 + cos) / 2.0,
                1.0 + alpha,
                -2.0 * cos,
                1.0 - alpha,
            ),
        };

        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Magnitude response in dB at `freq` — used by tests (the frontend
    /// mirrors this math in TypeScript for the curve display).
    pub fn magnitude_db(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / sample_rate;
        let (cw, c2w, sw, s2w) = (w.cos(), (2.0 * w).cos(), w.sin(), (2.0 * w).sin());
        let num = (self.b0 + self.b1 * cw + self.b2 * c2w).powi(2)
            + (self.b1 * sw + self.b2 * s2w).powi(2);
        let den =
            (1.0 + self.a1 * cw + self.a2 * c2w).powi(2) + (self.a1 * sw + self.a2 * s2w).powi(2);
        10.0 * (num / den).log10()
    }
}

/// The full EQ configuration published to the audio callback. Fixed size,
/// `Copy`, no heap — safe to hand over via a triple buffer.
#[derive(Clone, Copy)]
pub struct EqCoeffSet {
    pub bands: [BiquadCoeffs; MAX_BANDS],
    /// Linear preamp gain applied before the band chain.
    pub preamp: f64,
    /// When true the callback crossfades to the dry signal.
    pub bypass: bool,
}

impl EqCoeffSet {
    pub const IDENTITY: Self =
        Self { bands: [BiquadCoeffs::IDENTITY; MAX_BANDS], preamp: 1.0, bypass: true };

    pub fn from_state(state: &EqState, sample_rate: f64) -> Self {
        let mut bands = [BiquadCoeffs::IDENTITY; MAX_BANDS];
        for (slot, band) in bands.iter_mut().zip(state.bands.iter()) {
            *slot = BiquadCoeffs::design(band, sample_rate);
        }
        let preamp_db = match state.preamp {
            PreampMode::Auto => {
                let max_gain = state
                    .bands
                    .iter()
                    .filter(|b| {
                        b.enabled
                            && matches!(
                                b.filter_type,
                                FilterType::Peaking | FilterType::Lowshelf | FilterType::Highshelf
                            )
                    })
                    .map(|b| b.gain_db.max(0.0))
                    .fold(0.0f32, f32::max);
                -f64::from(max_gain)
            }
            PreampMode::Manual { db } => f64::from(db.clamp(-12.0, 12.0)),
        };
        Self { bands, preamp: 10f64.powf(preamp_db / 20.0), bypass: !state.enabled }
    }
}

/// Transposed direct form II biquad state (one per band per channel).
#[derive(Clone, Copy, Default)]
struct BiquadState {
    z1: f64,
    z2: f64,
}

impl BiquadState {
    #[inline(always)]
    fn process(&mut self, c: &BiquadCoeffs, x: f64) -> f64 {
        let y = c.b0 * x + self.z1;
        self.z1 = c.b1 * x - c.a1 * y + self.z2;
        self.z2 = c.b2 * x - c.a2 * y;
        y
    }
}

/// Time constants, expressed as one-pole smoothing factors computed per block.
const COEFF_SMOOTH_SECS: f64 = 0.010;
const BYPASS_FADE_SECS: f64 = 0.020;

/// The real-time side. Lives inside the cpal callback closure.
/// No allocation, no locking.
pub struct EqProcessor {
    sample_rate: f64,
    target: EqCoeffSet,
    current: [BiquadCoeffs; MAX_BANDS],
    preamp_current: f64,
    state: [[BiquadState; 2]; MAX_BANDS],
    /// 0.0 = fully dry (bypassed), 1.0 = fully wet.
    mix: f64,
    fade_step: f64,
}

impl EqProcessor {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = f64::from(sample_rate);
        Self {
            sample_rate,
            target: EqCoeffSet::IDENTITY,
            current: [BiquadCoeffs::IDENTITY; MAX_BANDS],
            preamp_current: 1.0,
            state: [[BiquadState::default(); 2]; MAX_BANDS],
            mix: 0.0,
            fade_step: 1.0 / (BYPASS_FADE_SECS * sample_rate),
        }
    }

    /// Adopt a new coefficient set (called at block rate when the triple
    /// buffer has an update).
    pub fn set_target(&mut self, set: &EqCoeffSet) {
        // If we're coming out of full bypass the chain state is stale — zero
        // it so the crossfade masks the filter warm-up rather than a click.
        if self.target.bypass && !set.bypass && self.mix <= f64::EPSILON {
            self.reset_state();
            self.current = set.bands;
            self.preamp_current = set.preamp;
        }
        self.target = *set;
    }

    /// Zero all filter state (after seeks/flushes).
    pub fn reset_state(&mut self) {
        self.state = [[BiquadState::default(); 2]; MAX_BANDS];
    }

    /// Advance coefficient smoothing; call once per callback block.
    pub fn block_update(&mut self, block_frames: usize) {
        let alpha =
            1.0 - (-(block_frames as f64) / (COEFF_SMOOTH_SECS * self.sample_rate)).exp();
        for (cur, tgt) in self.current.iter_mut().zip(self.target.bands.iter()) {
            cur.b0 += (tgt.b0 - cur.b0) * alpha;
            cur.b1 += (tgt.b1 - cur.b1) * alpha;
            cur.b2 += (tgt.b2 - cur.b2) * alpha;
            cur.a1 += (tgt.a1 - cur.a1) * alpha;
            cur.a2 += (tgt.a2 - cur.a2) * alpha;
        }
        self.preamp_current += (self.target.preamp - self.preamp_current) * alpha;
    }

    /// Process one stereo frame in place.
    #[inline(always)]
    pub fn process_frame(&mut self, left: &mut f32, right: &mut f32) {
        let target_mix = if self.target.bypass { 0.0 } else { 1.0 };
        if self.mix < target_mix {
            self.mix = (self.mix + self.fade_step).min(1.0);
        } else if self.mix > target_mix {
            self.mix = (self.mix - self.fade_step).max(0.0);
        }

        let dry_l = f64::from(*left);
        let dry_r = f64::from(*right);

        if self.mix <= f64::EPSILON {
            return;
        }

        let mut l = dry_l * self.preamp_current;
        let mut r = dry_r * self.preamp_current;
        for (band, st) in self.current.iter().zip(self.state.iter_mut()) {
            l = st[0].process(band, l);
            r = st[1].process(band, r);
        }

        // Equal-power crossfade between dry and wet.
        let theta = self.mix * std::f64::consts::FRAC_PI_2;
        let (wet_gain, dry_gain) = (theta.sin(), theta.cos());
        *left = (dry_l * dry_gain + l * wet_gain) as f32;
        *right = (dry_r * dry_gain + r * wet_gain) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(filter_type: FilterType, frequency: f32, gain_db: f32, q: f32) -> EqBand {
        EqBand { enabled: true, filter_type, frequency, gain_db, q }
    }

    #[test]
    fn peaking_magnitude_at_center() {
        let c = BiquadCoeffs::design(&band(FilterType::Peaking, 1000.0, 6.0, 1.0), 48000.0);
        assert!((c.magnitude_db(1000.0, 48000.0) - 6.0).abs() < 0.01);
        // Far away from the peak the response returns to ~0 dB.
        assert!(c.magnitude_db(20.0, 48000.0).abs() < 0.1);
        assert!(c.magnitude_db(20000.0, 48000.0).abs() < 0.3);
    }

    #[test]
    fn low_shelf_boosts_bass_only() {
        let c = BiquadCoeffs::design(&band(FilterType::Lowshelf, 200.0, 9.0, 0.707), 48000.0);
        assert!((c.magnitude_db(20.0, 48000.0) - 9.0).abs() < 0.2);
        assert!(c.magnitude_db(10000.0, 48000.0).abs() < 0.1);
    }

    #[test]
    fn highpass_rolls_off_dc() {
        let c = BiquadCoeffs::design(&band(FilterType::Highpass, 100.0, 0.0, 0.707), 48000.0);
        assert!(c.magnitude_db(10.0, 48000.0) < -30.0);
        assert!(c.magnitude_db(1000.0, 48000.0).abs() < 0.1);
    }

    #[test]
    fn eq_state_parses_wire_format() {
        let json = r#"{
            "enabled": true,
            "bands": [
                {"enabled": true, "type": "lowshelf", "frequency": 90, "gainDb": 4.5, "q": 0.707},
                {"enabled": true, "type": "peaking", "frequency": 2200, "gainDb": -3.5, "q": 1.4}
            ],
            "preamp": {"mode": "auto"}
        }"#;
        let state: EqState = serde_json::from_str(json).expect("wire format must parse");
        assert!(state.enabled);
        assert_eq!(state.bands.len(), 2);
        assert_eq!(state.bands[0].filter_type, FilterType::Lowshelf);
        assert_eq!(state.preamp, PreampMode::Auto);
        // And it round-trips.
        let back = serde_json::to_string(&state).unwrap();
        let state2: EqState = serde_json::from_str(&back).unwrap();
        assert_eq!(state, state2);
    }

    #[test]
    fn disabled_band_is_identity() {
        let mut b = band(FilterType::Peaking, 1000.0, 6.0, 1.0);
        b.enabled = false;
        assert_eq!(BiquadCoeffs::design(&b, 48000.0), BiquadCoeffs::IDENTITY);
    }

    #[test]
    fn auto_preamp_matches_max_boost() {
        let state = EqState {
            enabled: true,
            bands: vec![
                band(FilterType::Peaking, 100.0, 4.0, 1.0),
                band(FilterType::Lowshelf, 80.0, 7.5, 0.707),
                band(FilterType::Peaking, 5000.0, -3.0, 2.0),
            ],
            preamp: PreampMode::Auto,
        };
        let set = EqCoeffSet::from_state(&state, 48000.0);
        let preamp_db = 20.0 * set.preamp.log10();
        assert!((preamp_db + 7.5).abs() < 0.01, "got {preamp_db}");
    }

    #[test]
    fn processor_identity_when_bypassed() {
        let mut eq = EqProcessor::new(48000);
        eq.set_target(&EqCoeffSet::IDENTITY);
        eq.block_update(512);
        let (mut l, mut r) = (0.5f32, -0.25f32);
        eq.process_frame(&mut l, &mut r);
        assert_eq!((l, r), (0.5, -0.25));
    }

    #[test]
    fn processor_applies_gain_when_active() {
        // A wide +6 dB peak at 1 kHz should boost a mid-band signal. Feed a
        // 1 kHz sine and compare RMS in and out after the fade settles.
        let state = EqState {
            enabled: true,
            bands: vec![band(FilterType::Peaking, 1000.0, 6.0, 0.5)],
            preamp: PreampMode::Manual { db: 0.0 },
        };
        let set = EqCoeffSet::from_state(&state, 48000.0);
        let mut eq = EqProcessor::new(48000);
        eq.set_target(&set);
        // Settle smoothing + fade.
        for _ in 0..100 {
            eq.block_update(512);
        }
        let mut in_rms = 0.0f64;
        let mut out_rms = 0.0f64;
        let n = 48000;
        for i in 0..n {
            let x = (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin() as f32 * 0.1;
            let (mut l, mut r) = (x, x);
            eq.process_frame(&mut l, &mut r);
            if i > 4800 {
                in_rms += f64::from(x * x);
                out_rms += f64::from(l * l);
            }
        }
        let gain_db = 10.0 * (out_rms / in_rms).log10();
        assert!((gain_db - 6.0).abs() < 0.3, "got {gain_db} dB");
    }
}
