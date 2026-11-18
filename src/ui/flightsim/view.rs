// SPDX-License-Identifier: AGPL-3.0-or-later
//! 3D chase view of the simulated aircraft, drawn with the shared viewport renderer.
//!
//! The renderer draws its scene in the scene frame with one view-projection matrix, so the
//! aircraft's motion is folded into that matrix (model transform) and the ground grid is
//! transformed into the aircraft frame on the CPU each frame (a few hundred vertices).
use crate::geom3d::{ControlPart, ControlRole, Mat4, Scene, Vec3};
use crate::sim::aero::Surfaces;
use crate::sim::math::V3;
use crate::sim::vehicle::Mixing;
use crate::sim::{EnvironmentKind, Sim, WorldMesh};
use crate::ui::theme::Tokens;
use crate::ui::viewport3d::{self, DisplayStyle, gl};
use egui::{Rect, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Visual maximum propeller angular velocity [rad/s] at `motor_n = 1.0`, kept well below the
/// 60 Hz Nyquist aliasing frequency so rotation speed differences remain clearly visible.
const MAX_PROP_OMEGA_RAD_S: f32 = 34.0;

pub struct ChaseView {
    pub scene: Arc<Scene>,
    pub scene_version: u64,
    renderer: Arc<Mutex<Option<gl::Renderer>>>,
    gl_failed: Arc<AtomicBool>,
    grid_version: u64,
    world: Arc<WorldMesh>,
    world_env: EnvironmentKind,
    world_center: (i32, i32),
    world_dark: bool,
    world_version: u64,
    rotor_angles: Vec<f32>,
    last_sim_t: f64,
    /// Camera heading (smoothed) [rad], orbit offsets and zoom set by the mouse.
    cam_heading: f64,
    pub yaw_offset: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl Default for ChaseView {
    fn default() -> Self {
        Self {
            scene: Arc::new(Scene::default()),
            scene_version: 0,
            renderer: Arc::new(Mutex::new(None)),
            gl_failed: Arc::new(AtomicBool::new(false)),
            grid_version: 0,
            world: Arc::new(WorldMesh::default()),
            world_env: EnvironmentKind::default(),
            world_center: (i32::MAX, i32::MAX),
            world_dark: true,
            world_version: 0,
            rotor_angles: Vec::new(),
            last_sim_t: 0.0,
            cam_heading: 0.0,
            yaw_offset: 0.0,
            pitch: 0.28,
            zoom: 1.0,
        }
    }
}

/// Deflection angle [rad] for a control surface part after the mixer.
/// Positive angle rotates the trailing edge toward the surface's `+up` normal
/// (elevator/aileron/elevon up, rudder right).
pub fn control_deflection_rad(
    role: ControlRole,
    is_right: bool,
    surfaces: Surfaces,
    mixing: Mixing,
) -> f32 {
    let m = surfaces.mixed(mixing);
    let frac = match (mixing, role) {
        (Mixing::Elevon, ControlRole::Elevon | ControlRole::Aileron) => {
            if is_right {
                (m.elevator + m.aileron).clamp(-1.0, 1.0)
            } else {
                (m.elevator - m.aileron).clamp(-1.0, 1.0)
            }
        }
        (Mixing::VTail, ControlRole::Ruddervator) => {
            if is_right {
                (m.elevator - m.rudder).clamp(-1.0, 1.0)
            } else {
                (m.elevator + m.rudder).clamp(-1.0, 1.0)
            }
        }
        (_, ControlRole::Aileron) => {
            if is_right {
                m.aileron
            } else {
                -m.aileron
            }
        }
        (_, ControlRole::Elevator) => m.elevator,
        (_, ControlRole::Rudder) => m.rudder,
        (_, ControlRole::Ruddervator) => {
            if is_right {
                (m.elevator - m.rudder).clamp(-1.0, 1.0)
            } else {
                (m.elevator + m.rudder).clamp(-1.0, 1.0)
            }
        }
        (_, ControlRole::Elevon) => {
            if is_right {
                (m.elevator + m.aileron).clamp(-1.0, 1.0)
            } else {
                (m.elevator - m.aileron).clamp(-1.0, 1.0)
            }
        }
    };
    (frac * crate::sim::vehicle::DELTA_MAX) as f32
}

/// Model matrix in aircraft-scene coordinates rotating a control surface about its hinge line.
pub fn control_model_matrix(part: &ControlPart, angle_rad: f32) -> Mat4 {
    Mat4::rotation_around(part.hinge_pivot, part.hinge_axis, angle_rad)
}

/// Matches each drawn [`crate::geom3d::RotorPart`] to its index in `sim.params.rotors` / `sim.motor_n`.
pub fn match_rotor_indices(scene: &Scene, sim: &Sim) -> Vec<Option<usize>> {
    let cg = sim.params.scene_cg;
    scene
        .rotors
        .iter()
        .map(|rp| {
            let is_lift = rp.axis.z.abs() > 0.5;
            let mut best: Option<(usize, f32)> = None;
            for (k, sr) in sim.params.rotors.iter().enumerate() {
                if sr.lift != is_lift {
                    continue;
                }
                let s_pos = Vec3::new(
                    cg[0] + sr.pos.x as f32,
                    cg[1] - sr.pos.y as f32,
                    cg[2] - sr.pos.z as f32,
                );
                let d = if is_lift {
                    (s_pos - rp.center).length()
                } else {
                    (s_pos.x - rp.center.x).hypot(s_pos.y - rp.center.y)
                };
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((k, d));
                }
            }
            best.map(|(k, _)| k)
        })
        .collect()
}

/// Advances a propeller's visual phase angle [rad] proportionally to `motor_n`.
/// Stopped (`motor_n <= 1e-4`) or paused (`dt <= 0`) motors keep their exact angle.
pub fn advance_rotor_angle(angle: f32, motor_n: f64, spin: f64, dt: f32) -> f32 {
    let a = if angle.is_finite() { angle } else { 0.0 };
    if !(motor_n.is_finite() && dt.is_finite()) || motor_n <= 1e-4 || dt <= 0.0 {
        return a;
    }
    let dir = if spin.is_finite() && spin < 0.0 {
        -1.0_f32
    } else {
        1.0_f32
    };
    let n = (motor_n as f32).clamp(0.0, 1.5);
    (a + dir * n * MAX_PROP_OMEGA_RAD_S * dt).rem_euclid(std::f32::consts::TAU)
}

/// Translucent blur-disc opacity multiplier in `[0, 1]` from normalised rotor speed `motor_n`.
pub fn rotor_disc_alpha(motor_n: f64) -> f32 {
    if !motor_n.is_finite() || motor_n <= 0.15 {
        return 0.0;
    }
    (((motor_n as f32) - 0.15) / 0.65).clamp(0.0, 1.0)
}

/// NED -> render world (x north, y west, z up).
fn ned_to_render(v: V3) -> Vec3 {
    Vec3::new(v.x as f32, -v.y as f32, -v.z as f32)
}

/// Rotation (columns) of the scene frame into the render world for the sim attitude.
fn rotation(sim: &Sim) -> [Vec3; 3] {
    // R = C * Rq * C with C = diag(1, -1, -1) (scene frame <-> FRD and NED <-> render).
    let c = |v: V3| Vec3::new(v.x as f32, -v.y as f32, -v.z as f32);
    let q = sim.state.q;
    [
        c(q.rotate(V3::new(1.0, 0.0, 0.0))),
        c(q.rotate(V3::new(0.0, -1.0, 0.0))),
        c(q.rotate(V3::new(0.0, 0.0, -1.0))),
    ]
}

fn mat_from(r: &[Vec3; 3], t: Vec3) -> Mat4 {
    Mat4 {
        cols: [
            [r[0].x, r[0].y, r[0].z, 0.0],
            [r[1].x, r[1].y, r[1].z, 0.0],
            [r[2].x, r[2].y, r[2].z, 0.0],
            [t.x, t.y, t.z, 1.0],
        ],
    }
}

/// R^T * v
fn inv_rot(r: &[Vec3; 3], v: Vec3) -> Vec3 {
    Vec3::new(r[0].dot(v), r[1].dot(v), r[2].dot(v))
}

impl ChaseView {
    pub fn gl_failed(&self) -> bool {
        self.gl_failed.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn force_gl_failure(&self) {
        self.gl_failed.store(true, Ordering::Relaxed);
    }

    pub fn set_scene(&mut self, scene: Scene) {
        self.rotor_angles.resize(scene.rotors.len(), 0.0);
        self.scene = Arc::new(scene);
        self.scene_version = self.scene_version.wrapping_add(1);
    }

    fn dynamic_parts(&mut self, sim: &Sim) -> gl::DynamicParts {
        let mixing = sim
            .params
            .wing
            .as_ref()
            .map_or(Mixing::Conventional, |w| w.mixing);
        let control_models = self
            .scene
            .controls
            .iter()
            .map(|c| {
                let ang = control_deflection_rad(c.role, c.is_right, sim.surfaces, mixing);
                control_model_matrix(c, ang).to_cols_array()
            })
            .collect();

        let sim_dt = if sim.t.is_finite() && sim.t > self.last_sim_t {
            ((sim.t - self.last_sim_t) as f32).clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.last_sim_t = if sim.t.is_finite() { sim.t } else { 0.0 };

        if self.rotor_angles.len() != self.scene.rotors.len() {
            self.rotor_angles.resize(self.scene.rotors.len(), 0.0);
        }
        let matched = match_rotor_indices(&self.scene, sim);
        let mut prop_models = Vec::with_capacity(self.scene.rotors.len());
        let mut disc_alphas = Vec::with_capacity(self.scene.rotors.len());
        for (i, rp) in self.scene.rotors.iter().enumerate() {
            let (n, spin) = matched[i]
                .map(|k| {
                    let n = sim.motor_n.get(k).copied().unwrap_or(0.0);
                    let spin = sim.params.rotors.get(k).map_or(1.0, |r| r.spin);
                    (n, spin)
                })
                .unwrap_or((0.0, 1.0));
            let ang = advance_rotor_angle(self.rotor_angles[i], n, spin, sim_dt);
            self.rotor_angles[i] = ang;
            prop_models.push(Mat4::rotation_around(rp.center, rp.axis, ang).to_cols_array());
            disc_alphas.push(rotor_disc_alpha(n));
        }

        gl::DynamicParts {
            control_models,
            prop_models,
            disc_alphas,
        }
    }

    /// Paints the 3D view into `rect`.
    pub fn paint(&mut self, ui: &mut Ui, rect: Rect, sim: &Sim, tk: &Tokens, dt: f32) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, tk.viewport_bg);
        let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);

        // --- Model transform: scene -> render world.
        let cg = sim.params.scene_cg;
        let cg = Vec3::new(cg[0], cg[1], cg[2]);
        let r = rotation(sim);
        let pos = ned_to_render(sim.state.pos);
        let model = mat_from(&r, pos) * Mat4::translation(-cg);

        // --- Chase camera (follows heading smoothly; mouse adds orbit/zoom).
        let (_, _, psi) = sim.state.q.to_euler();
        let k = 1.0 - (-(dt as f64) * 2.0).exp();
        let d = crate::sim::math::wrap_pi(psi - self.cam_heading);
        self.cam_heading = crate::sim::math::wrap_pi(self.cam_heading + d * k);
        let radius = self.scene.radius().max(0.3);
        let dist = (radius * 3.2).max(1.6) * self.zoom.clamp(0.3, 6.0);
        let hd = self.cam_heading as f32 + self.yaw_offset;
        let fwd = Vec3::new(hd.cos(), -hd.sin(), 0.0);
        let pitch = self.pitch.clamp(-0.2, 1.4);
        let mut eye = pos - fwd * (dist * pitch.cos()) + Vec3::Z * (dist * pitch.sin());
        let eye_ground = sim.env.ground_height(eye.x as f64, -eye.y as f64) as f32;
        eye.z = eye.z.max(eye_ground + 0.35);
        let target = pos + Vec3::Z * (radius * 0.15);
        let view = Mat4::look_at(eye, target, Vec3::Z);
        let proj = Mat4::perspective(50f32.to_radians(), aspect, 0.08, 3000.0);
        let world_vp_mat = proj * view;
        let vp = world_vp_mat * model;

        // --- Procedural environment mesh (rebuilt only when env/theme/tile changes).
        let qx = (sim.state.pos.x / 100.0).round() as i32;
        let qy = (sim.state.pos.y / 100.0).round() as i32;
        if self.world_version == 0
            || self.world_env != sim.env
            || self.world_center != (qx, qy)
            || self.world_dark != tk.dark
        {
            self.world_version = self.world_version.wrapping_add(1).max(1);
            self.world_env = sim.env;
            self.world_center = (qx, qy);
            self.world_dark = tk.dark;
            self.world = Arc::new(WorldMesh::build(
                sim.env,
                qx as f64 * 100.0,
                qy as f64 * 100.0,
                tk.dark,
                self.world_version,
            ));
        }

        // --- Ground grid around the aircraft (subtle reference lines on OpenField only).
        let to_model = |w: Vec3| inv_rot(&r, w - pos) + cg;
        let to_arr = |w: Vec3| {
            let v = to_model(w);
            [v.x, v.y, v.z]
        };
        let mut grid = gl::GridData {
            version: {
                self.grid_version = self.grid_version.wrapping_add(1);
                self.grid_version
            },
            ..Default::default()
        };
        if sim.env == EnvironmentKind::OpenField {
            let step = 20.0_f32;
            let half = 14;
            let (cx, cy) = (
                (pos.x / (step * 5.0)).round() * step * 5.0,
                (pos.y / (step * 5.0)).round() * step * 5.0,
            );
            for i in -half..=half {
                let o = i as f32 * step;
                let major = ((cx + o) / step).round() as i64 % 5 == 0;
                let e = half as f32 * step;
                for (a, b) in [
                    (
                        Vec3::new(cx + o, cy - e, 0.02),
                        Vec3::new(cx + o, cy + e, 0.02),
                    ),
                    (
                        Vec3::new(cx - e, cy + o, 0.02),
                        Vec3::new(cx + e, cy + o, 0.02),
                    ),
                ] {
                    let dst = if major {
                        &mut grid.major
                    } else {
                        &mut grid.minor
                    };
                    dst.push(to_arr(a));
                    dst.push(to_arr(b));
                }
            }
        }

        let world_light = (Vec3::new(-0.48, 0.36, 0.78)).normalized();
        let world_fill = (Vec3::new(0.60, -0.50, 0.32)).normalized();
        let eye_m = to_model(eye);
        let ld = inv_rot(&r, world_light);
        let fd = inv_rot(&r, world_fill);
        let (sky, ground) = if tk.dark {
            ([0.55, 0.62, 0.74], [0.20, 0.21, 0.25])
        } else {
            ([0.80, 0.85, 0.94], [0.46, 0.44, 0.42])
        };
        let (sky_zenith, sky_mid, fog_color) = if tk.dark {
            ([0.07, 0.11, 0.20], [0.14, 0.20, 0.32], [0.24, 0.29, 0.38])
        } else {
            ([0.22, 0.46, 0.78], [0.48, 0.68, 0.90], [0.75, 0.83, 0.92])
        };
        let world_kind = match sim.env {
            EnvironmentKind::OpenField => 0.0,
            EnvironmentKind::Runway => 1.0,
            EnvironmentKind::City => 2.0,
            EnvironmentKind::Mountains => 3.0,
        };
        let fog_range = match sim.env {
            EnvironmentKind::Mountains => [320.0_f32, 1140.0_f32],
            _ => [220.0_f32, 600.0_f32],
        };
        let world_inv_vp = world_vp_mat
            .inverse()
            .unwrap_or(Mat4::IDENTITY)
            .to_cols_array();
        let dynamic = self.dynamic_parts(sim);
        let frame = gl::FrameData {
            view_proj: vp.to_cols_array(),
            eye: [eye_m.x, eye_m.y, eye_m.z],
            light_dir: [ld.x, ld.y, ld.z],
            fill_dir: [fd.x, fd.y, fd.z],
            sky,
            ground,
            materials: viewport3d::part_materials(),
            offsets: Vec::new(),
            shadow: [0.0; 4],
            ground_z: 0.0,
            shadow_alpha: 0.0,
            clear: gl::rgba(tk.viewport_bg, 1.0),
            grid_minor: gl::rgba(tk.viewport_grid, 1.0),
            grid_major: gl::rgba(tk.viewport_grid_major, 1.0),
            colors: viewport3d::part_colors(tk, DisplayStyle::Solid),
            scene: self.scene.clone(),
            scene_version: self.scene_version,
            dynamic: Some(dynamic),
            grid: Arc::new(grid),
            world: Some(self.world.clone()),
            world_kind,
            world_vp: world_vp_mat.to_cols_array(),
            world_inv_vp,
            world_eye: [eye.x, eye.y, eye.z],
            world_light: [world_light.x, world_light.y, world_light.z],
            world_fill: [world_fill.x, world_fill.y, world_fill.z],
            sky_zenith,
            sky_mid,
            fog_color,
            fog_range,
        };
        let renderer = self.renderer.clone();
        let failed_flag = self.gl_failed.clone();
        let ctx = ui.ctx().clone();
        let cb = eframe::egui_glow::CallbackFn::new(move |info, painter| {
            if failed_flag.load(Ordering::Relaxed) {
                return;
            }
            let gl = painter.gl();
            let Ok(mut guard) = renderer.lock() else {
                failed_flag.store(true, Ordering::Relaxed);
                return;
            };
            if guard.is_none() {
                match gl::Renderer::new(gl) {
                    Ok(r) => *guard = Some(r),
                    Err(e) => {
                        crate::support::log_error(&format!("flight sim 3D view: {e}"));
                        failed_flag.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                        return;
                    }
                }
            }
            if let Some(r) = guard.as_mut() {
                if let Err(e) = r.paint(gl, &info, painter.intermediate_fbo(), &frame) {
                    use eframe::glow::HasContext;
                    unsafe {
                        gl.bind_framebuffer(eframe::glow::FRAMEBUFFER, painter.intermediate_fbo())
                    };
                    if !r.disable_msaa(gl) {
                        crate::support::log_error(&format!("flight sim 3D view: {e}"));
                        failed_flag.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint();
                }
            }
        });
        painter.add(egui::PaintCallback {
            rect,
            callback: Arc::new(cb),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_rotation_is_orthonormal_and_level_is_identity() {
        let p = crate::project::Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut sim = crate::sim::sim_for_project(&p, &m, crate::sim::StartPosition::Air);
        sim.state.q = crate::sim::math::Quat::IDENTITY;
        let r = rotation(&sim);
        assert!((r[0] - Vec3::X).length() < 1e-6);
        assert!((r[1] - Vec3::Y).length() < 1e-6);
        assert!((r[2] - Vec3::Z).length() < 1e-6);
        // Banked right: the right wing (scene -Y) goes down in the render world.
        sim.state.q = crate::sim::math::Quat::from_euler(0.5, 0.0, 0.0);
        let r = rotation(&sim);
        let right_wing = r[1] * -1.0;
        assert!(right_wing.z < 0.0);
        for axis in r {
            assert!((axis.length() - 1.0).abs() < 1e-5);
        }
    }

    fn build_scene(dt: crate::project::DroneType) -> (crate::project::Project, Scene) {
        let mut p = crate::project::Project::default();
        p.set_drone_type(dt);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let spec = crate::geom3d::from_project(&p, &m, &crate::geom3d::ViewOptions::default());
        (p, crate::geom3d::build(&spec))
    }

    fn trailing_edge_point(part: &ControlPart) -> Vec3 {
        let p = part
            .mesh
            .positions
            .iter()
            .min_by(|a, b| a[0].total_cmp(&b[0]))
            .expect("non-empty control mesh");
        Vec3::new(p[0], p[1], p[2])
    }

    fn check_hinge_fixed_and_angle(part: &ControlPart, angle: f32) {
        let mat = control_model_matrix(part, angle);
        for s in [-0.15_f32, 0.0, 0.25] {
            let pt = part.hinge_pivot + part.hinge_axis * s;
            let moved = mat.transform_point(pt);
            assert!(
                (moved - pt).length() < 1e-5,
                "hinge point moved: {pt:?} -> {moved:?}"
            );
        }
        let te = trailing_edge_point(part);
        let te_moved = mat.transform_point(te);
        // Component perpendicular to hinge axis rotates by |angle|.
        let v0 = te - part.hinge_pivot;
        let p0 = v0 - part.hinge_axis * v0.dot(part.hinge_axis);
        let v1 = te_moved - part.hinge_pivot;
        let p1 = v1 - part.hinge_axis * v1.dot(part.hinge_axis);
        let cos_a = (p0.dot(p1) / (p0.length() * p1.length())).clamp(-1.0, 1.0);
        assert!(
            (cos_a.acos() - angle.abs()).abs() < 1e-4,
            "angle mismatch: got {} expected {}",
            cos_a.acos(),
            angle.abs()
        );
    }

    #[test]
    fn surface_hinge_transform_for_conventional_vtail_and_flying_wing() {
        use crate::project::DroneType;

        // 1) Conventional: ailerons, elevator, rudder
        let (_, conv) = build_scene(DroneType::FixedWingConventional);
        assert_eq!(conv.controls.len(), 5); // 2 ailerons + 2 elevator halves + 1 rudder
        let s_elev_up = Surfaces {
            elevator: 1.0,
            ..Default::default()
        };
        let s_roll_right = Surfaces {
            aileron: 1.0,
            ..Default::default()
        };
        let s_rud_right = Surfaces {
            rudder: 1.0,
            ..Default::default()
        };
        for c in &conv.controls {
            match c.role {
                ControlRole::Elevator => {
                    let a =
                        control_deflection_rad(c.role, c.is_right, s_elev_up, Mixing::Conventional);
                    assert!((a - crate::sim::vehicle::DELTA_MAX as f32).abs() < 1e-5);
                    check_hinge_fixed_and_angle(c, a);
                    let te = trailing_edge_point(c);
                    let dz = control_model_matrix(c, a).transform_point(te).z - te.z;
                    assert!(dz > 0.01, "elevator up should move TE +Z: {dz}");
                }
                ControlRole::Aileron => {
                    let a = control_deflection_rad(
                        c.role,
                        c.is_right,
                        s_roll_right,
                        Mixing::Conventional,
                    );
                    check_hinge_fixed_and_angle(c, a);
                    let te = trailing_edge_point(c);
                    let dz = control_model_matrix(c, a).transform_point(te).z - te.z;
                    if c.is_right {
                        assert!(dz > 0.005, "right aileron should move UP (+Z): {dz}");
                    } else {
                        assert!(dz < -0.005, "left aileron should move DOWN (-Z): {dz}");
                    }
                }
                ControlRole::Rudder => {
                    let a = control_deflection_rad(
                        c.role,
                        c.is_right,
                        s_rud_right,
                        Mixing::Conventional,
                    );
                    check_hinge_fixed_and_angle(c, a);
                    let te = trailing_edge_point(c);
                    let dy = control_model_matrix(c, a).transform_point(te).y - te.y;
                    assert!(
                        dy < -0.005,
                        "rudder right should move TE to right (-Y): {dy}"
                    );
                }
                _ => panic!("unexpected role {:?}", c.role),
            }
        }

        // 2) V-tail: ailerons + two ruddervators
        let (_, vtail) = build_scene(DroneType::FixedWingVTail);
        let rvs: Vec<_> = vtail
            .controls
            .iter()
            .filter(|c| c.role == ControlRole::Ruddervator)
            .collect();
        assert_eq!(rvs.len(), 2);
        for c in &rvs {
            let ae = control_deflection_rad(c.role, c.is_right, s_elev_up, Mixing::VTail);
            check_hinge_fixed_and_angle(c, ae);
            let te = trailing_edge_point(c);
            let dz = control_model_matrix(c, ae).transform_point(te).z - te.z;
            assert!(dz > 0.005, "v-tail pitch up should raise TE: {dz}");

            let ar = control_deflection_rad(c.role, c.is_right, s_rud_right, Mixing::VTail);
            let dy = control_model_matrix(c, ar).transform_point(te).y - te.y;
            assert!(
                dy < -0.005,
                "v-tail yaw right should shift TE right (-Y): {dy}"
            );
        }
        let s_both_v = Surfaces {
            elevator: 1.0,
            rudder: 1.0,
            ..Default::default()
        };
        let a_l = control_deflection_rad(ControlRole::Ruddervator, false, s_both_v, Mixing::VTail);
        let a_r = control_deflection_rad(ControlRole::Ruddervator, true, s_both_v, Mixing::VTail);
        assert!((a_l - crate::sim::vehicle::DELTA_MAX as f32).abs() < 1e-5);
        assert!(a_r.abs() < 1e-5);

        // 3) Flying wing: two elevons
        let (_, fw) = build_scene(DroneType::FlyingWing);
        assert_eq!(fw.controls.len(), 2);
        for c in &fw.controls {
            assert_eq!(c.role, ControlRole::Elevon);
            let ae = control_deflection_rad(c.role, c.is_right, s_elev_up, Mixing::Elevon);
            check_hinge_fixed_and_angle(c, ae);
            let te = trailing_edge_point(c);
            let dz = control_model_matrix(c, ae).transform_point(te).z - te.z;
            assert!(dz > 0.01, "elevon pitch up should raise TE: {dz}");

            let aa = control_deflection_rad(c.role, c.is_right, s_roll_right, Mixing::Elevon);
            let dz_roll = control_model_matrix(c, aa).transform_point(te).z - te.z;
            if c.is_right {
                assert!(dz_roll > 0.005, "right elevon should go up on roll right");
            } else {
                assert!(dz_roll < -0.005, "left elevon should go down on roll right");
            }
        }
    }

    #[test]
    fn prop_rotation_advances_with_motor_n_and_is_zero_when_stopped() {
        let a0 = 1.23_f32;
        assert_eq!(advance_rotor_angle(a0, 0.0, 1.0, 0.016), a0);
        assert_eq!(rotor_disc_alpha(0.0), 0.0);

        let dt = 0.02_f32;
        let a_half = advance_rotor_angle(0.0, 0.5, 1.0, dt);
        let a_full = advance_rotor_angle(0.0, 1.0, 1.0, dt);
        assert!(a_half > 0.0 && (a_full - 2.0 * a_half).abs() < 1e-5);
        assert!(rotor_disc_alpha(1.0) > rotor_disc_alpha(0.4));

        // Multirotor rotor matching maps every rendered rotor to a distinct sim motor.
        let (p, sc) = build_scene(crate::project::DroneType::Multirotor);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let sim = crate::sim::sim_for_project(&p, &m, crate::sim::StartPosition::Air);
        let matched = match_rotor_indices(&sc, &sim);
        assert_eq!(matched.len(), sim.params.rotors.len());
        let mut idxs: Vec<usize> = matched.into_iter().flatten().collect();
        idxs.sort_unstable();
        idxs.dedup();
        assert_eq!(idxs.len(), sim.params.rotors.len());
    }
}
