// SPDX-License-Identifier: AGPL-3.0-or-later
//! Swarm analysis (spec module 15): N identical copies of the current design flying in
//! formation — wake interaction and power, mesh radio network, spacing / collision check.
//!
//! Educational, preliminary-design level for civil uses (light shows, mapping, search and
//! rescue). It reads the project and the evaluated [`Model`] and never changes them; the
//! formation inputs live only in the user interface state.
//!
//! - [`formation`]: geometry and keep-out spacing check
//! - [`wake`]: horseshoe-vortex (fixed wing) and momentum-theory downwash (multirotor) models
//! - [`mesh`]: free-space link budget, hops (BFS), single points of failure (articulation points)
pub mod formation;
pub mod mesh;
pub mod wake;

use crate::calc::Model;
use crate::project::{DroneType, Project};
use formation::{FormationKind, FormationParams, SpacingLevel};
use mesh::{MeshResult, Radio, RadioPreset};
use wake::{RotorEffect, RotorVehicle, WingEffect, WingVehicle};

/// Which lift system the formation analysis uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Wing-borne cruise (horseshoe-vortex model).
    Wing,
    /// Rotor-borne hover (downwash model).
    RotorHover,
    /// Rotor-borne forward flight at the design cruise speed.
    RotorCruise,
}

impl Mode {
    pub fn is_rotor(self) -> bool {
        !matches!(self, Mode::Wing)
    }
}

/// Modes that make sense for a drone type (first = default).
pub fn modes_for(dt: DroneType) -> &'static [Mode] {
    match dt {
        DroneType::Multirotor => &[Mode::RotorHover, Mode::RotorCruise],
        DroneType::VtolQuadplane => &[Mode::Wing, Mode::RotorHover],
        _ => &[Mode::Wing],
    }
}

/// Swarm inputs (UI state; not saved in the project file).
#[derive(Clone, Debug, PartialEq)]
pub struct SwarmInputs {
    pub mode: Mode,
    pub kind: FormationKind,
    pub n: usize,
    /// Lengths below are given in vehicle sizes (`true`) or metres (`false`).
    pub in_sizes: bool,
    pub spacing: f64,
    pub step_x: f64,
    /// Vertical step per rank [m].
    pub step_h: f64,
    /// Position error per vehicle (95 %) [m]. Default 0.3 m ≈ RTK; plain GNSS is 1.5–3 m.
    pub pos_err: f64,
    pub preset: RadioPreset,
    pub radio: Radio,
    /// Formation diameter used for the network analysis [m].
    pub spread: f64,
    /// Horizontal distance from the formation centre to the ground station [m].
    pub gs_distance: f64,
    /// Formation height above the ground station [m].
    pub altitude: f64,
}

impl SwarmInputs {
    /// Sensible defaults for a drone type.
    pub fn for_type(dt: DroneType) -> Self {
        let mode = modes_for(dt)[0];
        let (kind, spacing, step_x, step_h) = if mode.is_rotor() {
            (FormationKind::Grid, 3.0, 3.0, 1.0)
        } else {
            (FormationKind::Vee, 0.9, 2.0, 0.0)
        };
        SwarmInputs {
            mode,
            kind,
            n: 5,
            in_sizes: true,
            spacing,
            step_x,
            step_h,
            pos_err: 0.3,
            preset: RadioPreset::Ghz24,
            radio: RadioPreset::Ghz24.radio(),
            spread: 300.0,
            gs_distance: 1500.0,
            altitude: 60.0,
        }
    }
}

/// Wing model inputs from the evaluated design.
pub fn wing_vehicle(p: &Project, m: &Model) -> WingVehicle {
    WingVehicle {
        span: p.b,
        s_ref: p.s,
        cla: m.aero.cla_w,
        k: m.aero.k,
        weight: m.weight.w,
        rho: m.atmo.rho,
        v: m.perf_cr.v,
        eta: crate::calc::performance::eta_pt(&m.perf_in),
        p_alone: m.perf_cr.p_total,
    }
}

/// Rotor model inputs from the evaluated multirotor part of the design.
pub fn rotor_vehicle(p: &Project, m: &Model, cruise: bool) -> RotorVehicle {
    RotorVehicle {
        thrust: m.mr.w,
        disk_area: m.mr.a_disk_total,
        rho: m.atmo.rho,
        v: if cruise { p.mr_v_cruise.max(0.0) } else { 0.0 },
        p_elec_hover: m.mr.p_elec_hover_total,
        p_alone: if cruise {
            m.mr.p_total_cruise
        } else {
            m.mr.p_total_hover
        },
    }
}

/// Largest vehicle dimension [m]: wing span, or rotor tip to rotor tip.
pub fn vehicle_size(p: &Project, mode: Mode) -> f64 {
    let s = if mode.is_rotor() {
        p.mr_frame_size + p.mr_d_prop
    } else {
        p.b
    };
    if s.is_finite() && s > 0.0 { s } else { 1.0 }
}

/// Complete swarm result.
#[derive(Clone, Debug)]
pub struct SwarmResult {
    pub mode: Mode,
    /// Vehicle size [m].
    pub size: f64,
    /// Flight speed [m/s].
    pub v: f64,
    pub positions: Vec<[f64; 3]>,
    pub wing_fx: Vec<WingEffect>,
    pub rotor_fx: Vec<RotorEffect>,
    /// Power of one vehicle alone [W].
    pub p_alone: f64,
    /// Power of each vehicle in the formation [W].
    pub p_each: Vec<f64>,
    /// Hover induced velocity (rotor) or vortex sink rate (wing) [m/s].
    pub wake_speed: f64,
    /// Rotor: equivalent single-disk radius [m] (0 for the wing model).
    pub rotor_r_eq: f64,
    /// Rotor: recommended (vertical, horizontal) separation [m].
    pub safe_sep: Option<(f64, f64)>,
    /// Wing: best lateral spacing (in spans) and its drag fraction at the chosen step.
    pub best_lateral: Option<(f64, f64)>,
    pub required_spacing: f64,
    pub min_pair: Option<(usize, usize, f64)>,
    pub spacing_issues: Vec<(usize, usize, f64, SpacingLevel)>,
    /// Network nodes (vehicles then ground station), metres.
    pub mesh_nodes: Vec<[f64; 3]>,
    pub mesh: MeshResult,
}

impl SwarmResult {
    pub fn p_total(&self) -> f64 {
        self.p_each.iter().sum()
    }
    pub fn p_alone_total(&self) -> f64 {
        self.p_alone * self.positions.len() as f64
    }
    pub fn worst_spacing(&self) -> SpacingLevel {
        self.spacing_issues
            .first()
            .map(|r| r.3)
            .unwrap_or(SpacingLevel::Ok)
    }
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

/// Runs every swarm model for the current design.
pub fn analyse(inp: &SwarmInputs, p: &Project, m: &Model) -> SwarmResult {
    let mode = if modes_for(p.drone_type).contains(&inp.mode) {
        inp.mode
    } else {
        modes_for(p.drone_type)[0]
    };
    let size = vehicle_size(p, mode);
    let unit = if inp.in_sizes { size } else { 1.0 };
    let fp = FormationParams {
        kind: inp.kind,
        n: inp.n.clamp(1, 64),
        spacing: fin(inp.spacing) * unit,
        step_x: fin(inp.step_x) * unit,
        step_h: fin(inp.step_h),
    };
    let positions = formation::positions(&fp);

    let (wing_fx, rotor_fx, p_alone, p_each, v, wake_speed, rotor_r_eq, safe_sep, best_lateral) =
        if mode.is_rotor() {
            let rv = rotor_vehicle(p, m, mode == Mode::RotorCruise);
            let fx = wake::rotor_interactions(&rv, &positions);
            let pe = fx.iter().map(|e| fin(rv.p_alone + e.d_power)).collect();
            (
                Vec::new(),
                fx,
                fin(rv.p_alone),
                pe,
                rv.v,
                fin(rv.v_h()),
                fin(rv.r_eq()),
                Some(wake::safe_separation(&rv)),
                None,
            )
        } else {
            let wv = wing_vehicle(p, m);
            let fx = wake::wing_interactions(&wv, &positions);
            let pe = fx.iter().map(|e| fin(wv.p_alone + e.d_power)).collect();
            let dx = if fp.step_x > 0.0 {
                fp.step_x
            } else {
                2.0 * wv.span
            };
            (
                fx,
                Vec::new(),
                fin(wv.p_alone),
                pe,
                wv.v,
                fin(wv.horseshoe([0.0; 3]).sink_rate()),
                0.0,
                None,
                Some(wake::best_lateral_spacing(&wv, dx)),
            )
        };

    let required_spacing = formation::required_spacing(size, inp.pos_err);
    let min_pair = formation::min_pair(&positions);
    let spacing_issues = formation::spacing_report(&positions, required_spacing);

    // Network: formation scaled to the requested spread, lifted to the altitude.
    let c = formation::centroid(&positions);
    let ext = formation::extent(&positions);
    let k = if ext > 1e-9 {
        fin(inp.spread).max(0.0) / ext
    } else {
        1.0
    };
    let mut mesh_nodes: Vec<[f64; 3]> = positions
        .iter()
        .map(|q| {
            [
                (q[0] - c[0]) * k,
                (q[1] - c[1]) * k,
                (q[2] - c[2]) * k + fin(inp.altitude),
            ]
        })
        .collect();
    mesh_nodes.push([fin(inp.gs_distance), 0.0, 0.0]);
    let mesh = mesh::analyse(&mesh_nodes, &inp.radio);

    SwarmResult {
        mode,
        size,
        v: fin(v),
        positions,
        wing_fx,
        rotor_fx,
        p_alone,
        p_each,
        wake_speed,
        rotor_r_eq,
        safe_sep,
        best_lateral,
        required_spacing,
        min_pair,
        spacing_issues,
        mesh_nodes,
        mesh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    fn all_finite(r: &SwarmResult) -> bool {
        let fx_ok = r.wing_fx.iter().all(|e| {
            e.w_mean.is_finite()
                && e.d_drag.is_finite()
                && e.d_drag_frac.is_finite()
                && e.d_power.is_finite()
                && e.cl_roll.is_finite()
        }) && r
            .rotor_fx
            .iter()
            .all(|e| e.w.is_finite() && e.thrust_loss.is_finite() && e.d_power.is_finite());
        fx_ok
            && r.p_each.iter().all(|x| x.is_finite())
            && r.positions.iter().flatten().all(|x| x.is_finite())
            && r.mesh.links.iter().all(|l| l.margin.is_finite())
            && r.size.is_finite()
            && r.p_alone.is_finite()
            && r.safe_sep
                .is_none_or(|s| s.0.is_finite() && s.1.is_finite())
    }

    #[test]
    fn every_type_mode_and_shape_is_finite() {
        let lib = Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &lib);
            for &mode in modes_for(dt) {
                for kind in FormationKind::ALL {
                    let mut inp = SwarmInputs::for_type(dt);
                    inp.mode = mode;
                    inp.kind = kind;
                    let r = analyse(&inp, &p, &m);
                    assert!(all_finite(&r), "{dt:?} {mode:?} {kind:?}");
                    assert_eq!(r.positions.len(), 5);
                    assert_eq!(r.mesh_nodes.len(), 6);
                    assert!(r.p_alone > 0.0, "{dt:?} {mode:?}");
                }
            }
        }
    }

    #[test]
    fn samples_give_sensible_numbers() {
        let lib = Library::embedded();
        for f in [
            "fixed_wing_2kg_1600mm.json",
            "flying_wing_1200mm.json",
            "quad_250_x.json",
            "quad_275_x.json",
            "hexa_550_x.json",
            "vtol_quadplane.json",
        ] {
            let p = crate::project::load_example(f).expect(f);
            let m = evaluate(&p, &lib);
            let inp = SwarmInputs::for_type(p.drone_type);
            let r = analyse(&inp, &p, &m);
            assert!(all_finite(&r), "{f}");
            if r.mode == Mode::Wing {
                // Default V at 0.9 span overlap: the formation saves power.
                assert!(r.p_total() < r.p_alone_total(), "{f}");
                let (best, frac) = r.best_lateral.unwrap();
                assert!(best > 0.5 && best < 1.1 && frac < 0.0, "{f}: {best} {frac}");
            } else {
                let (zv, dh) = r.safe_sep.unwrap();
                assert!(zv > dh && dh > 0.0 && zv < 200.0, "{f}: {zv} {dh}");
            }
            assert!(r.mesh.connected, "{f}");
        }
    }

    #[test]
    fn analysis_does_not_touch_the_project() {
        let p = Project::default();
        let m = evaluate(&p, &Library::embedded());
        let before = serde_json::to_string(&p).unwrap();
        let _ = analyse(&SwarmInputs::for_type(p.drone_type), &p, &m);
        assert_eq!(before, serde_json::to_string(&p).unwrap());
    }
}
