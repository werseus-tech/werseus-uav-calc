// SPDX-License-Identifier: AGPL-3.0-or-later
//! 3D chase view of the simulated aircraft, drawn with the shared viewport renderer.
//!
//! The renderer draws its scene in the scene frame with one view-projection matrix, so the
//! aircraft's motion is folded into that matrix (model transform) and the ground grid is
//! transformed into the aircraft frame on the CPU each frame (a few hundred vertices).
use crate::geom3d::{Mat4, Scene, Vec3};
use crate::sim::Sim;
use crate::sim::math::V3;
use crate::ui::theme::Tokens;
use crate::ui::viewport3d::{self, DisplayStyle, gl};
use egui::{Rect, Ui};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct ChaseView {
    pub scene: Arc<Scene>,
    pub scene_version: u64,
    renderer: Arc<Mutex<Option<gl::Renderer>>>,
    gl_failed: Arc<AtomicBool>,
    grid_version: u64,
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
            cam_heading: 0.0,
            yaw_offset: 0.0,
            pitch: 0.28,
            zoom: 1.0,
        }
    }
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
        self.scene = Arc::new(scene);
        self.scene_version = self.scene_version.wrapping_add(1);
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
        eye.z = eye.z.max(0.15);
        let target = pos + Vec3::Z * (radius * 0.15);
        let view = Mat4::look_at(eye, target, Vec3::Z);
        let proj = Mat4::perspective(50f32.to_radians(), aspect, 0.05, 3000.0);
        let vp = proj * view * model;

        // --- Ground grid around the aircraft (world), transformed into the scene frame.
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
        let step = 5.0_f32;
        let half = 40;
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
                    Vec3::new(cx + o, cy - e, 0.0),
                    Vec3::new(cx + o, cy + e, 0.0),
                ),
                (
                    Vec3::new(cx - e, cy + o, 0.0),
                    Vec3::new(cx + e, cy + o, 0.0),
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
        // Runway outline (60 m x 8 m, starting just behind the start point).
        let rw = [
            Vec3::new(-10.0, -4.0, 0.0),
            Vec3::new(50.0, -4.0, 0.0),
            Vec3::new(50.0, 4.0, 0.0),
            Vec3::new(-10.0, 4.0, 0.0),
        ];
        for i in 0..4 {
            grid.major.push(to_arr(rw[i]));
            grid.major.push(to_arr(rw[(i + 1) % 4]));
        }

        let world_light = (Vec3::new(-0.4, 0.3, 1.0)).normalized();
        let world_fill = (Vec3::new(0.6, -0.5, 0.3)).normalized();
        let eye_m = to_model(eye);
        let ld = inv_rot(&r, world_light);
        let fd = inv_rot(&r, world_fill);
        let (sky, ground) = if tk.dark {
            ([0.55, 0.62, 0.74], [0.20, 0.21, 0.25])
        } else {
            ([0.80, 0.85, 0.94], [0.46, 0.44, 0.42])
        };
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
            grid: Arc::new(grid),
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
}
