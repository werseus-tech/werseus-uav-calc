// SPDX-License-Identifier: AGPL-3.0-or-later
//! Semi-empirical propeller / rotor noise estimate.
//!
//! # Model
//!
//! The tonal (rotational, loading + thickness) level follows the *form* of the Hamilton
//! Standard far-field rotational-noise estimate described in SAE AIR 1407 and reproduced in
//! Hubbard, "Aeroacoustics of Flight Vehicles", vol. 1:
//!
//! ```text
//! L_tonal(r) = L_REF
//!            + 15.3 log10(P / P_REF)          shaft power per rotor
//!            - 20   log10(D / D_REF)          diameter
//!            + 38.5 (M_h - M_REF)             helical tip Mach number
//!            - 3 (B - 2)                      blade count
//!            + 10 log10(N)                    number of rotors (incoherent addition)
//!            + 10 log10(rho / 1.225)          air density (first-order)
//!            - 20 log10(r / 1 m)              spherical spreading
//!            + 3 (optional)                   ground reflection (hard ground, observer near it)
//! ```
//!
//! The published coefficients (15.3, 20, 38.5, 3 per blade) come from large-aircraft propellers
//! at 300 ft. Evaluated directly for a small UAV propeller they over-predict by tens of dB, so
//! this module keeps the scaling exponents and re-anchors the absolute level with the
//! small-UAS reference below. The anchor is an engineering choice, not part of AIR 1407.
//!
//! The broadband (vortex shedding / turbulent boundary layer) level uses the usual
//! tip-speed law:
//!
//! ```text
//! L_broadband(r) = LB_REF + 50 log10(V_h / V_REF) + 10 log10(B / 2) + 20 log10(D / D_REF)
//!                  + 10 log10(N) + 10 log10(rho / 1.225) - 20 log10(r) + ground
//! ```
//!
//! # Constants (reference rotor: 10 inch, 2 blades, 100 W shaft power, 1 m)
//!
//! * `L_REF` = 79 dB tonal, `LB_REF` = 75 dB broadband (re 20 uPa, unweighted, 1 m)
//! * `P_REF` = 100 W, `D_REF` = 0.254 m, `M_REF` = 0.45, `V_REF` = 130 m/s
//!
//! These give about 80.5 dB (roughly 75-78 dBA) at 1 m for the reference rotor, in line with
//! published small-rotor measurements of that size class.
//!
//! # Spectrum
//!
//! Tonal lines sit at `k * BPF`, k = 1..5. Each harmonic rolls off as `-30 log10(k)`
//! (about -9 dB per doubling of harmonic number) and the five lines are normalised so their
//! energy sum equals `L_tonal`. The broadband part is a Gaussian hump on a log-frequency
//! axis centred at `clamp(10 * BPF, 300, 5000)` Hz with sigma = 0.45 decade, normalised so
//! its energy sum equals `L_broadband`. Everything is gathered into 1/3-octave bands from
//! 20 Hz to 20 kHz; overall SPL and dBA are the energy sums of those bands (A-weighting per
//! IEC 61672-1, see [`super::weighting`]).
//!
//! # Validity range
//!
//! Tip Mach (helical) up to about 0.7, observer distance of at least 1 m and about two
//! rotor diameters, 2-6 blades, 0.1-1 m diameter, shaft power 5 W - 20 kW per rotor. Static or
//! slow-moving rotors with the observer in the far field; no directivity, no installation
//! effects, no motor/ESC/airframe noise, no atmospheric absorption. Expect several dB of
//! error. Treat results as a comparison tool between designs, not as a measurement.

use super::weighting::{a_weight_db, band_index, db_sum, third_octave_centres};

/// Reference tonal level at 1 m for the reference rotor (dB re 20 uPa).
pub const L_TONAL_REF: f64 = 79.0;
/// Reference broadband level at 1 m for the reference rotor (dB re 20 uPa).
pub const L_BROADBAND_REF: f64 = 75.0;
/// Reference shaft power per rotor (W).
pub const P_REF_W: f64 = 100.0;
/// Reference diameter (m), 10 inch.
pub const D_REF_M: f64 = 0.254;
/// Reference helical tip Mach number for the tonal term.
pub const MACH_REF: f64 = 0.45;
/// Reference tip speed for the broadband term (m/s).
pub const V_REF_MS: f64 = 130.0;
/// Reference air density (kg/m^3).
pub const RHO_REF: f64 = 1.225;
/// Power exponent coefficient (Hamilton Standard form).
pub const K_POWER: f64 = 15.3;
/// Tip Mach coefficient (Hamilton Standard form).
pub const K_MACH: f64 = 38.5;
/// dB removed per blade above two.
pub const K_BLADES: f64 = 3.0;
/// Broadband tip-speed exponent (50 log10 V).
pub const K_BROADBAND_V: f64 = 50.0;
/// Ground reflection gain (dB).
pub const GROUND_DB: f64 = 3.0;
/// Number of tonal harmonics reported.
pub const N_HARMONICS: usize = 5;
/// Tonal harmonic roll-off coefficient (dB per decade of harmonic number).
pub const HARMONIC_ROLLOFF: f64 = 30.0;
/// Broadband hump width (decades of frequency).
pub const BROADBAND_SIGMA_DEC: f64 = 0.45;
/// Tip Mach above which the estimate warns (caution band).
pub const MACH_CAUTION: f64 = 0.6;
/// Tip Mach above which noise rises steeply (high band).
pub const MACH_HIGH: f64 = 0.7;
/// Minimum observer distance used in the calculation (m).
pub const MIN_DISTANCE_M: f64 = 0.1;

/// Tip Mach warning band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachBand {
    /// Below 0.6.
    Ok,
    /// 0.6 to 0.7.
    Caution,
    /// 0.7 and above.
    High,
}

/// Classify a tip Mach number.
pub fn mach_band(mach: f64) -> MachBand {
    if mach >= MACH_HIGH {
        MachBand::High
    } else if mach >= MACH_CAUTION {
        MachBand::Caution
    } else {
        MachBand::Ok
    }
}

/// One group of identical rotors at one operating point.
#[derive(Clone, Copy, Debug)]
pub struct RotorSet {
    /// Propeller diameter (m).
    pub diameter_m: f64,
    /// Blades per rotor.
    pub blades: u32,
    /// Number of identical rotors.
    pub n_rotors: u32,
    /// Shaft speed (rev/min).
    pub rpm: f64,
    /// Shaft power per rotor (W).
    pub shaft_power_w: f64,
    /// Forward (flight) speed (m/s), 0 for hover / static.
    pub forward_speed_ms: f64,
}

/// Air properties, taken from the project atmosphere.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    /// Density (kg/m^3).
    pub rho: f64,
    /// Speed of sound (m/s).
    pub a_sound: f64,
}

impl Default for Air {
    fn default() -> Self {
        Air {
            rho: RHO_REF,
            a_sound: 340.294,
        }
    }
}

/// Observer position and ground option.
#[derive(Clone, Copy, Debug)]
pub struct Observer {
    /// Distance from the rotor group (m).
    pub distance_m: f64,
    /// Add the +3 dB ground reflection.
    pub ground_reflection: bool,
}

impl Default for Observer {
    fn default() -> Self {
        Observer {
            distance_m: 10.0,
            ground_reflection: false,
        }
    }
}

/// One 1/3-octave band of the spectrum.
#[derive(Clone, Copy, Debug)]
pub struct Band {
    /// Centre frequency (Hz).
    pub fc: f64,
    /// Band level (dB).
    pub spl: f64,
    /// A-weighted band level (dBA).
    pub spl_a: f64,
}

/// One tonal line.
#[derive(Clone, Copy, Debug)]
pub struct Tone {
    /// Harmonic number (1 = blade-pass frequency).
    pub k: u32,
    /// Frequency (Hz).
    pub f: f64,
    /// Line level (dB).
    pub spl: f64,
    /// A-weighted line level (dBA).
    pub spl_a: f64,
}

/// Spectrum for plotting.
#[derive(Clone, Debug, Default)]
pub struct Spectrum {
    /// 1/3-octave bands, 20 Hz - 20 kHz (tonal + broadband energy).
    pub bands: Vec<Band>,
    /// Tonal lines (harmonics 1..5).
    pub tones: Vec<Tone>,
}

/// Result of one estimate.
#[derive(Clone, Debug, Default)]
pub struct Estimate {
    /// False when the inputs cannot describe a rotor; all numbers are then zero.
    pub valid: bool,
    /// Tip speed `pi D n` (m/s), without forward speed.
    pub tip_speed: f64,
    /// Static tip Mach number.
    pub tip_mach: f64,
    /// Helical tip Mach number, including forward speed (used by the model).
    pub helical_mach: f64,
    /// Rotational speed (rev/s).
    pub rev_per_s: f64,
    /// Blade-pass frequency (Hz).
    pub bpf: f64,
    /// First five harmonics `k * BPF` (Hz).
    pub harmonics: [f64; N_HARMONICS],
    /// Tonal overall level at the observer (dB).
    pub tonal_spl: f64,
    /// Broadband overall level at the observer (dB).
    pub broadband_spl: f64,
    /// Overall sound pressure level at the observer (dB).
    pub oaspl: f64,
    /// A-weighted overall level at the observer (dBA).
    pub oaspl_a: f64,
    /// Plot data.
    pub spectrum: Spectrum,
    /// Observer closer than 1 m or two rotor diameters (far-field assumption violated).
    pub near_field: bool,
    /// Helical tip Mach above the 0.7 validity limit.
    pub mach_beyond_range: bool,
}

/// Tip speed `pi * D * n` (m/s), `n` in rev/s.
pub fn tip_speed(diameter_m: f64, rpm: f64) -> f64 {
    std::f64::consts::PI * diameter_m * rpm / 60.0
}

/// Blade-pass frequency `rpm / 60 * blades` (Hz).
pub fn blade_pass_frequency(rpm: f64, blades: u32) -> f64 {
    rpm / 60.0 * blades as f64
}

fn finite_pos(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

/// Estimate the noise of a rotor group. Never returns NaN: invalid inputs give
/// `Estimate { valid: false, .. }` with zeros; extreme but finite inputs are clamped inside
/// the level formula (power 1 W - 1 MW, diameter 1 cm - 10 m, helical Mach <= 1.2,
/// distance >= 0.1 m, blades 1-12, rotors <= 256).
pub fn estimate(rs: &RotorSet, air: &Air, obs: &Observer) -> Estimate {
    let ok = finite_pos(rs.diameter_m)
        && finite_pos(rs.rpm)
        && rs.blades > 0
        && rs.n_rotors > 0
        && finite_pos(air.rho)
        && finite_pos(air.a_sound)
        && rs.shaft_power_w.is_finite()
        && rs.forward_speed_ms.is_finite()
        && obs.distance_m.is_finite();
    if !ok {
        return Estimate::default();
    }
    let vt = tip_speed(rs.diameter_m, rs.rpm);
    let v_fwd = rs.forward_speed_ms.max(0.0);
    let v_hel = (vt * vt + v_fwd * v_fwd).sqrt();
    let tip_mach = vt / air.a_sound;
    let helical_mach = v_hel / air.a_sound;
    let rev_per_s = rs.rpm / 60.0;
    let bpf = blade_pass_frequency(rs.rpm, rs.blades);
    let mut harmonics = [0.0; N_HARMONICS];
    for (i, h) in harmonics.iter_mut().enumerate() {
        *h = bpf * (i as f64 + 1.0);
    }
    if !(vt.is_finite() && v_hel.is_finite() && bpf.is_finite() && harmonics[4].is_finite()) {
        return Estimate::default();
    }

    // Clamped quantities for the level formula.
    let d = rs.diameter_m.clamp(0.01, 10.0);
    let p = rs.shaft_power_w.clamp(1.0, 1.0e6);
    let b = rs.blades.clamp(1, 12) as f64;
    let n = rs.n_rotors.clamp(1, 256) as f64;
    let m = helical_mach.clamp(0.0, 1.2);
    let v = (m * air.a_sound).max(1.0);
    let r = obs.distance_m.max(MIN_DISTANCE_M);
    let common = 10.0 * n.log10() + 10.0 * (air.rho / RHO_REF).log10() - 20.0 * r.log10()
        + if obs.ground_reflection {
            GROUND_DB
        } else {
            0.0
        };

    let tonal = L_TONAL_REF + K_POWER * (p / P_REF_W).log10() - 20.0 * (d / D_REF_M).log10()
        + K_MACH * (m - MACH_REF)
        - K_BLADES * (b - 2.0)
        + common;
    let broadband = L_BROADBAND_REF
        + K_BROADBAND_V * (v / V_REF_MS).log10()
        + 10.0 * (b / 2.0).log10()
        + 20.0 * (d / D_REF_M).log10()
        + common;

    let spectrum = build_spectrum(tonal, broadband, &harmonics);
    let oaspl = db_sum(spectrum.bands.iter().map(|x| x.spl));
    let oaspl_a = db_sum(spectrum.bands.iter().map(|x| x.spl_a));
    let out = Estimate {
        valid: true,
        tip_speed: vt,
        tip_mach,
        helical_mach,
        rev_per_s,
        bpf,
        harmonics,
        tonal_spl: tonal,
        broadband_spl: broadband,
        oaspl,
        oaspl_a,
        spectrum,
        near_field: obs.distance_m < 1.0_f64.max(2.0 * rs.diameter_m),
        mach_beyond_range: helical_mach > MACH_HIGH,
    };
    if [out.tonal_spl, out.broadband_spl, out.oaspl, out.oaspl_a]
        .iter()
        .all(|x| x.is_finite())
    {
        out
    } else {
        Estimate::default()
    }
}

fn build_spectrum(tonal: f64, broadband: f64, harmonics: &[f64; N_HARMONICS]) -> Spectrum {
    let centres = third_octave_centres();
    let mut energy = vec![0.0_f64; centres.len()];
    let e = |l: f64| 10f64.powf(l / 10.0);

    // Broadband hump, normalised to the broadband overall level.
    let f_c = (harmonics[0] * 10.0).clamp(300.0, 5000.0).log10();
    let w: Vec<f64> = centres
        .iter()
        .map(|fc| {
            let z = (fc.log10() - f_c) / BROADBAND_SIGMA_DEC;
            (-0.5 * z * z).exp()
        })
        .collect();
    let wsum: f64 = w.iter().sum();
    for (en, wi) in energy.iter_mut().zip(&w) {
        *en += e(broadband) * wi / wsum;
    }

    // Tonal lines, normalised so the five lines sum to the tonal overall level.
    let norm: f64 = (1..=N_HARMONICS)
        .map(|k| 10f64.powf(-HARMONIC_ROLLOFF * (k as f64).log10() / 10.0))
        .sum();
    let l1 = tonal - 10.0 * norm.log10();
    let mut tones = Vec::new();
    for (i, f) in harmonics.iter().enumerate() {
        let k = i as u32 + 1;
        let spl = l1 - HARMONIC_ROLLOFF * (k as f64).log10();
        tones.push(Tone {
            k,
            f: *f,
            spl,
            spl_a: spl + a_weight_db(*f),
        });
        if let Some(bi) = band_index(*f) {
            energy[bi] += e(spl);
        }
    }

    let bands = centres
        .iter()
        .zip(&energy)
        .map(|(fc, en)| {
            let spl = if *en > 0.0 { 10.0 * en.log10() } else { -200.0 };
            Band {
                fc: *fc,
                spl,
                spl_a: spl + a_weight_db(*fc),
            }
        })
        .collect();
    Spectrum { bands, tones }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RotorSet {
        RotorSet {
            diameter_m: 0.254,
            blades: 2,
            n_rotors: 4,
            rpm: 9000.0,
            shaft_power_w: 80.0,
            forward_speed_ms: 0.0,
        }
    }

    fn est(rs: &RotorSet, r: f64, ground: bool) -> Estimate {
        estimate(
            rs,
            &Air::default(),
            &Observer {
                distance_m: r,
                ground_reflection: ground,
            },
        )
    }

    #[test]
    fn tip_speed_is_pi_d_n() {
        let e = est(&base(), 10.0, false);
        let expect = std::f64::consts::PI * 0.254 * 9000.0 / 60.0;
        assert!((e.tip_speed - expect).abs() < 1e-9);
        assert!((e.tip_mach - expect / 340.294).abs() < 1e-9);
    }

    #[test]
    fn bpf_and_harmonics() {
        let mut rs = base();
        rs.blades = 3;
        let e = est(&rs, 10.0, false);
        assert!((e.bpf - 450.0).abs() < 1e-9);
        for k in 0..5 {
            assert!((e.harmonics[k] - 450.0 * (k as f64 + 1.0)).abs() < 1e-9);
        }
        assert_eq!(e.spectrum.tones.len(), 5);
    }

    #[test]
    fn doubling_distance_is_minus_6db() {
        let a = est(&base(), 5.0, false);
        let b = est(&base(), 10.0, false);
        assert!((a.oaspl - b.oaspl - 6.0206).abs() < 1e-6);
        assert!((a.oaspl_a - b.oaspl_a - 6.0206).abs() < 1e-6);
    }

    #[test]
    fn doubling_rotor_count_is_plus_3db() {
        let mut rs = base();
        rs.n_rotors = 2;
        let a = est(&rs, 10.0, false);
        rs.n_rotors = 4;
        let b = est(&rs, 10.0, false);
        assert!((b.oaspl - a.oaspl - 3.0103).abs() < 1e-6);
        assert!((b.oaspl_a - a.oaspl_a - 3.0103).abs() < 1e-6);
    }

    #[test]
    fn ground_reflection_adds_3db() {
        let a = est(&base(), 10.0, false);
        let b = est(&base(), 10.0, true);
        assert!((b.oaspl - a.oaspl - 3.0).abs() < 1e-6);
    }

    #[test]
    fn higher_tip_mach_is_louder() {
        let mut last = f64::NEG_INFINITY;
        for rpm in [4000.0, 6000.0, 8000.0, 10_000.0, 12_000.0, 14_000.0] {
            let mut rs = base();
            rs.rpm = rpm;
            let e = est(&rs, 10.0, false);
            assert!(e.oaspl > last && e.oaspl_a.is_finite(), "{rpm}");
            last = e.oaspl;
        }
    }

    #[test]
    fn tonal_energy_is_conserved_in_bands() {
        let e = est(&base(), 3.0, false);
        let tones = db_sum(e.spectrum.tones.iter().map(|t| t.spl));
        assert!((tones - e.tonal_spl).abs() < 1e-6);
        let total = db_sum([e.tonal_spl, e.broadband_spl]);
        assert!((total - e.oaspl).abs() < 0.05, "{total} {}", e.oaspl);
        assert!(e.oaspl_a < e.oaspl);
    }

    #[test]
    fn reference_rotor_level_is_plausible() {
        let rs = RotorSet {
            diameter_m: D_REF_M,
            blades: 2,
            n_rotors: 1,
            rpm: MACH_REF * 340.294 * 60.0 / (std::f64::consts::PI * D_REF_M),
            shaft_power_w: P_REF_W,
            forward_speed_ms: 0.0,
        };
        let e = est(&rs, 1.0, false);
        assert!(e.oaspl > 78.0 && e.oaspl < 84.0, "{}", e.oaspl);
        assert!(e.oaspl_a > 65.0 && e.oaspl_a < 82.0, "{}", e.oaspl_a);
    }

    #[test]
    fn mach_bands() {
        assert_eq!(mach_band(0.59), MachBand::Ok);
        assert_eq!(mach_band(0.6), MachBand::Caution);
        assert_eq!(mach_band(0.69), MachBand::Caution);
        assert_eq!(mach_band(0.7), MachBand::High);
    }

    #[test]
    fn extreme_inputs_never_give_nan() {
        let vals = [
            0.0,
            -1.0,
            1e-12,
            1e12,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        for &d in &vals {
            for &rpm in &vals {
                for &p in &vals {
                    for &r in &vals {
                        for blades in [0u32, 2, u32::MAX] {
                            for n in [0u32, 4, u32::MAX] {
                                let rs = RotorSet {
                                    diameter_m: d,
                                    blades,
                                    n_rotors: n,
                                    rpm,
                                    shaft_power_w: p,
                                    forward_speed_ms: d,
                                };
                                let e = est(&rs, r, true);
                                for x in [
                                    e.tip_speed,
                                    e.tip_mach,
                                    e.bpf,
                                    e.oaspl,
                                    e.oaspl_a,
                                    e.tonal_spl,
                                    e.broadband_spl,
                                ] {
                                    assert!(!x.is_nan(), "{rs:?} {r}");
                                }
                                if e.valid {
                                    assert!(e.oaspl.is_finite() && e.oaspl_a.is_finite());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
