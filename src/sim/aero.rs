// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fixed-wing aerodynamics: lift / drag / moment versus angle of attack including stall,
//! sideslip, control surfaces and damping derivatives.
//!
//! Lift: the attached-flow curve has the design lift slope and saturates smoothly at the
//! design CLmax; past the stall angle it blends (sigmoid) into a flat-plate model, which
//! keeps every angle from -180 to 180 deg finite (tail slides, spins, backwards flight).
use super::math::V3;
use super::vehicle::{DELTA_MAX, Mixing, WingAero};

/// Pilot-sense surface commands in [-1, 1]: elevator + = nose up, aileron + = roll right,
/// rudder + = nose right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Surfaces {
    pub elevator: f64,
    pub aileron: f64,
    pub rudder: f64,
}

impl Surfaces {
    pub fn clamped(self) -> Surfaces {
        let c = |v: f64| {
            if v.is_finite() {
                v.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        };
        Surfaces {
            elevator: c(self.elevator),
            aileron: c(self.aileron),
            rudder: c(self.rudder),
        }
    }

    /// Effective surface deflections after the servo mixer: on a V-tail or flying wing the
    /// two panels share their travel, so a large pitch command leaves less for yaw / roll.
    pub fn mixed(self, mixing: Mixing) -> Surfaces {
        let s = self.clamped();
        match mixing {
            Mixing::Conventional => s,
            Mixing::VTail => {
                let l = (s.elevator + s.rudder).clamp(-1.0, 1.0);
                let r = (s.elevator - s.rudder).clamp(-1.0, 1.0);
                Surfaces {
                    elevator: 0.5 * (l + r),
                    aileron: s.aileron,
                    rudder: 0.5 * (l - r),
                }
            }
            Mixing::Elevon => {
                let l = (s.elevator + s.aileron).clamp(-1.0, 1.0);
                let r = (s.elevator - s.aileron).clamp(-1.0, 1.0);
                Surfaces {
                    elevator: 0.5 * (l + r),
                    aileron: 0.5 * (l - r),
                    rudder: 0.0,
                }
            }
        }
    }
}

const STALL_SHARPNESS: f64 = 36.0;

/// Stall blending factor (0 = attached, 1 = fully separated), symmetric in alpha.
pub fn stall_sigma(alpha: f64, alpha0: f64) -> f64 {
    let m = STALL_SHARPNESS;
    let a = (-m * (alpha - alpha0)).clamp(-60.0, 60.0).exp();
    let b = (m * (alpha + alpha0)).clamp(-60.0, 60.0).exp();
    ((1.0 + a + b) / ((1.0 + a) * (1.0 + b))).clamp(0.0, 1.0)
}

/// Attached-flow lift coefficient: design slope, smooth saturation near CLmax.
fn cl_attached(w: &WingAero, alpha_w: f64) -> f64 {
    let lin = w.cla * alpha_w;
    let x = lin / (w.cl_att_scale * w.cl_max);
    lin / (1.0 + x.abs().powi(8)).powf(0.125)
}

/// (CL, CD, stall sigma) at wing angle of attack `alpha_w` [rad].
pub fn cl_cd(w: &WingAero, alpha_w: f64) -> (f64, f64, f64) {
    let sigma = stall_sigma(alpha_w, w.alpha_stall);
    let cl_att = cl_attached(w, alpha_w);
    let cd_att = w.cd0 + w.k * cl_att * cl_att;
    let (s, c) = alpha_w.sin_cos();
    let cl_fp = 0.9 * 2.0 * s * c;
    let cd_fp = w.cd0 + 1.8 * s * s;
    (
        (1.0 - sigma) * cl_att + sigma * cl_fp,
        (1.0 - sigma) * cd_att + sigma * cd_fp,
        sigma,
    )
}

/// Stall angle (sigmoid centre) for which the blended lift curve peaks exactly at CLmax.
pub fn calibrate_stall_angle(w: &WingAero) -> f64 {
    let a_lin = w.cl_max / w.cla.max(0.1);
    let peak_for = |alpha_stall: f64| {
        let mut w = w.clone();
        w.alpha_stall = alpha_stall;
        (0..=800)
            .map(|i| cl_cd(&w, i as f64 * 0.001).0)
            .fold(f64::MIN, f64::max)
            / w.cl_max
    };
    // The peak grows monotonically with the stall angle: bisection.
    let (mut lo, mut hi) = (0.8 * a_lin, 2.5 * a_lin);
    if !(peak_for(lo) < 1.0 && peak_for(hi) > 1.0) {
        return 1.25 * a_lin;
    }
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if peak_for(mid) < 1.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Wing angle of attack for a given CL on the attached branch (bisection).
pub fn alpha_for_cl(w: &WingAero, cl: f64) -> f64 {
    let a_peak = (0..=400)
        .map(|i| i as f64 * 0.0015)
        .fold((0.0, f64::MIN), |acc, a| {
            let c = cl_cd(w, a).0;
            if c > acc.1 { (a, c) } else { acc }
        })
        .0;
    let (mut lo, mut hi) = (-0.3_f64, a_peak.max(0.05));
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if cl_cd(w, mid).0 < cl {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AeroOut {
    pub force: V3,
    pub moment: V3,
    pub alpha: f64,
    pub beta: f64,
    pub cl: f64,
    pub stall: f64,
}

/// Aerodynamic force and moment (body FRD, about the design CG) for air-relative body
/// velocity `v` [m/s], body rates `w` [rad/s], surface commands `s` (already mixed).
pub fn wing_forces(wa: &WingAero, v: V3, w: V3, s: Surfaces, rho: f64) -> AeroOut {
    let speed = v.norm();
    if !(speed.is_finite() && speed > 1e-3) {
        return AeroOut::default();
    }
    let alpha = v.z.atan2(v.x);
    let beta = (v.y / speed).clamp(-1.0, 1.0).asin();
    let alpha_w = super::math::wrap_pi(alpha + wa.i_w);
    let (cl_2d, cd_2d, sigma) = cl_cd(wa, alpha_w);
    // Only the chordwise (x-z) flow makes lift, induced drag and pitching moment: scaling
    // by cos^2(beta) makes them vanish smoothly in pure spanwise flow (e.g. a hovering
    // VTOL in a crosswind), where alpha is undefined.
    let chordwise = ((v.x * v.x + v.z * v.z) / (speed * speed)).clamp(0.0, 1.0);
    let mut cl = cl_2d * chordwise;
    let cd = wa.cd0 + (cd_2d - wa.cd0) * chordwise;
    let qbar = 0.5 * rho * speed * speed;
    // Rate non-dimensionalisation with a floor speed keeps damping finite at rest.
    let v_d = speed.max(3.0);
    let (ph, qh, rh) = (
        w.x * wa.b / (2.0 * v_d),
        w.y * wa.mac / (2.0 * v_d),
        w.z * wa.b / (2.0 * v_d),
    );
    let de = s.elevator * DELTA_MAX;
    let da = s.aileron * DELTA_MAX;
    let dr = s.rudder * DELTA_MAX;
    cl += wa.clq * qh - wa.cl_de * de * (1.0 - sigma) * chordwise;
    let sb = beta.sin();
    let cy = wa.cyb * sb - wa.cy_dr * dr;
    let attached = 1.0 - sigma;
    let cm = (wa.cma * (alpha_w - wa.alpha_w_trim).sin() - 0.08 * sigma * alpha_w.signum()
        + wa.cm_de * de)
        * chordwise
        + wa.cmq * qh;
    let cll = wa.clb * sb
        + wa.clp * ph * (1.0 - 1.5 * sigma)
        + 0.25 * cl * rh
        + wa.clda * da * (0.4 + 0.6 * attached)
        + wa.cl_dr * dr;
    let cn = wa.cnb * sb + wa.cnr * rh - 0.125 * cl * ph - 0.15 * cl * wa.clda * da + wa.cn_dr * dr;
    // Lift acts perpendicular to the airflow in the body x-z plane, drag against it.
    let lift_dir = V3::new(v.z, 0.0, -v.x).normalized_or(V3::ZERO);
    let drag_dir = v * (-1.0 / speed);
    let side_dir = V3::new(0.0, 1.0, 0.0);
    let cd_total = cd + 0.3 * sb * sb;
    let force = (lift_dir * cl + drag_dir * cd_total + side_dir * cy) * (qbar * wa.s);
    let moment = V3::new(wa.b * cll, wa.mac * cm, wa.b * cn) * (qbar * wa.s);
    AeroOut {
        force,
        moment,
        alpha,
        beta,
        cl,
        stall: sigma,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{DroneType, Project};
    use crate::sim::vehicle::VehicleParams;

    fn wing(dt: DroneType) -> WingAero {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        VehicleParams::from_project(&p, &m).wing.unwrap()
    }

    #[test]
    fn lift_curve_peaks_at_design_clmax_and_drops_after_stall() {
        for dt in [
            DroneType::FixedWingConventional,
            DroneType::FlyingWing,
            DroneType::FixedWingVTail,
        ] {
            let w = wing(dt);
            let curve: Vec<(f64, f64)> = (0..=600)
                .map(|i| {
                    let a = i as f64 * 0.001;
                    (a, cl_cd(&w, a).0)
                })
                .collect();
            let (a_peak, cl_peak) = curve
                .iter()
                .copied()
                .fold((0.0, f64::MIN), |acc, x| if x.1 > acc.1 { x } else { acc });
            assert!((cl_peak / w.cl_max - 1.0).abs() < 0.01, "{dt:?} {cl_peak}");
            // Small-angle slope is the design slope.
            let slope = cl_cd(&w, 0.01).0 / 0.01;
            assert!((slope / w.cla - 1.0).abs() < 0.02, "{dt:?} {slope}");
            // Clear lift loss 6 deg past the peak.
            let after = cl_cd(&w, a_peak + 0.105).0;
            assert!(after < 0.85 * cl_peak, "{dt:?} {after} vs {cl_peak}");
            // Drag rises strongly after the stall.
            assert!(cl_cd(&w, a_peak + 0.2).1 > 3.0 * cl_cd(&w, 0.05).1);
            // Finite for every angle.
            for i in -360..=360 {
                let (cl, cd, s) = cl_cd(&w, i as f64 * 0.01);
                assert!(cl.is_finite() && cd.is_finite() && s.is_finite());
                assert!(cd > 0.0);
            }
        }
    }

    #[test]
    fn mixers_share_travel() {
        let s = Surfaces {
            elevator: 1.0,
            aileron: 1.0,
            rudder: 1.0,
        };
        let v = s.mixed(Mixing::VTail);
        assert!((v.elevator - 0.5).abs() < 1e-12 && (v.rudder - 0.5).abs() < 1e-12);
        let e = s.mixed(Mixing::Elevon);
        assert_eq!(e.rudder, 0.0);
        assert!((e.elevator - 0.5).abs() < 1e-12);
        assert_eq!(s.mixed(Mixing::Conventional), s);
    }

    #[test]
    fn damping_and_control_signs() {
        let w = wing(DroneType::FixedWingConventional);
        let v = V3::new(18.0, 0.0, 18.0 * (w.alpha_w_trim - w.i_w).tan());
        let base = wing_forces(&w, v, V3::ZERO, Surfaces::default(), 1.2);
        // Trimmed: almost no pitching moment with neutral elevator.
        assert!(base.moment.y.abs() < 0.02, "{}", base.moment.y);
        let up = Surfaces {
            elevator: 0.5,
            ..Default::default()
        };
        assert!(wing_forces(&w, v, V3::ZERO, up, 1.2).moment.y > 0.0);
        let right = Surfaces {
            aileron: 0.5,
            ..Default::default()
        };
        assert!(wing_forces(&w, v, V3::ZERO, right, 1.2).moment.x > 0.0);
        let rud = Surfaces {
            rudder: 0.5,
            ..Default::default()
        };
        assert!(wing_forces(&w, v, V3::ZERO, rud, 1.2).moment.z > 0.0);
        // Rates are damped.
        let rolling = wing_forces(&w, v, V3::new(1.0, 0.0, 0.0), Surfaces::default(), 1.2);
        assert!(rolling.moment.x < 0.0);
        let pitching = wing_forces(&w, v, V3::new(0.0, 1.0, 0.0), Surfaces::default(), 1.2);
        assert!(pitching.moment.y < base.moment.y);
        // Sideslip from the right yaws the nose into the wind.
        let slip = wing_forces(
            &w,
            V3::new(18.0, 2.0, v.z),
            V3::ZERO,
            Surfaces::default(),
            1.2,
        );
        assert!(slip.moment.z > 0.0);
        // Static pitch stability: more alpha -> nose down.
        let high = wing_forces(
            &w,
            V3::new(18.0, 0.0, 2.0),
            V3::ZERO,
            Surfaces::default(),
            1.2,
        );
        assert!(high.moment.y < base.moment.y);
    }

    #[test]
    fn spanwise_flow_makes_no_lift_and_is_continuous() {
        let w = wing(DroneType::FixedWingConventional);
        let rho = 1.2;
        let q_s = 0.5 * rho * 10.0 * 10.0 * w.s;
        // Pure crosswind on a hovering VTOL: tiny chordwise components of either sign.
        for (vx, vz) in [
            (1e-3, 0.0),
            (-1e-3, 0.0),
            (0.0, 1e-3),
            (0.0, -1e-3),
            (0.0, 0.0),
        ] {
            let out = wing_forces(
                &w,
                V3::new(vx, 10.0, vz),
                V3::ZERO,
                Surfaces::default(),
                rho,
            );
            assert!(out.force.z.abs() < 1e-4 * q_s, "lift {:?}", out.force);
            assert!(
                out.moment.y.abs() < 1e-4 * q_s * w.mac,
                "cm {:?}",
                out.moment
            );
        }
        // At a fixed alpha and speed, lift falls off smoothly with sideslip (cos^2 beta).
        let a = w.alpha_w_trim - w.i_w;
        let lift_at = |beta: f64| {
            let (sb, cb) = beta.sin_cos();
            let v = V3::new(10.0 * cb * a.cos(), 10.0 * sb, 10.0 * cb * a.sin());
            -wing_forces(&w, v, V3::ZERO, Surfaces::default(), rho)
                .force
                .z
        };
        let (l0, l30, l60) = (lift_at(0.0), lift_at(0.52), lift_at(1.05));
        assert!(l0 > l30 && l30 > l60 && l60 > 0.0, "{l0} {l30} {l60}");
        assert!(l60 < 0.4 * l0, "{l60} vs {l0}");
    }
}
