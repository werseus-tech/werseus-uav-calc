// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yörünge kamerası (saf matematik, birim testli).
//!
//! Dünya Z ekseni her zaman yukarıdır; kamera hedef nokta etrafında `yaw` (Z çevresinde) ve
//! `pitch` (yükseliş) ile döner. Pitch ±89.5°'de sınırlanır: model asla ters dönmez ve kutup
//! noktalarında `look_at` tekilliği oluşmaz (Üst görünüm 89.5° kullanır).
//! Ortografik izdüşümde görünür yükseklik mesafeye bağlıdır; böylece yakınlaştırma iki
//! modda da aynı hissi verir.
use crate::geom3d::{Mat4, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const PITCH_LIMIT: f32 = 89.5 * PI / 180.0;
pub const DEFAULT_FOVY: f32 = 35.0 * PI / 180.0;
/// Fare ile döndürme hassasiyeti [rad/piksel].
pub const ORBIT_SPEED: f32 = 0.008;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Projection {
    #[default]
    Perspective,
    Orthographic,
}

/// Hazır görünümler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Iso,
    Top,
    Front,
    Side,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Iso, Preset::Top, Preset::Front, Preset::Side];

    /// (yaw, pitch). Üst: burun ekranda yukarı; Ön: burundan bakış; Yan: sağ taraftan, burun sağda.
    pub fn angles(self) -> (f32, f32) {
        match self {
            Preset::Iso => (-0.95, 0.45),
            Preset::Top => (PI, PITCH_LIMIT),
            Preset::Front => (0.0, 0.0),
            Preset::Side => (-FRAC_PI_2, 0.0),
        }
    }
}

/// Fare sürükleme türü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Orbit,
    Pan,
}

/// Sol tuş = döndür; Shift+sol, orta ya da sağ tuş = kaydır.
pub fn drag_mode(primary: bool, middle: bool, secondary: bool, shift: bool) -> Option<DragMode> {
    if middle || secondary || (primary && shift) {
        Some(DragMode::Pan)
    } else if primary {
        Some(DragMode::Orbit)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fovy: f32,
    pub projection: Projection,
    /// Sahne yarıçapı: yakınlaştırma sınırları ve kırpma düzlemleri buna göre.
    pub scene_radius: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let (yaw, pitch) = Preset::Iso.angles();
        Self {
            target: Vec3::ZERO,
            yaw,
            pitch,
            distance: 3.0,
            fovy: DEFAULT_FOVY,
            projection: Projection::Perspective,
            scene_radius: 1.0,
        }
    }
}

/// Açıyı (-π, π] aralığına sarar.
pub fn wrap_angle(a: f32) -> f32 {
    if !a.is_finite() {
        return 0.0;
    }
    let mut x = (a + PI).rem_euclid(TAU) - PI;
    if x <= -PI {
        x += TAU;
    }
    x
}

fn sane(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

impl OrbitCamera {
    pub fn min_distance(&self) -> f32 {
        (self.scene_radius.max(0.01) * 0.05).max(0.005)
    }

    pub fn max_distance(&self) -> f32 {
        self.scene_radius.max(0.01) * 40.0
    }

    /// Bozuk değerleri onarır ve sınırları uygular (her işlemden sonra çağrılır).
    pub fn sanitize(&mut self) {
        if !self.target.is_finite() {
            self.target = Vec3::ZERO;
        }
        self.scene_radius = sane(self.scene_radius, 1.0).max(0.01);
        self.yaw = wrap_angle(self.yaw);
        self.pitch = sane(self.pitch, 0.3).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        self.fovy = sane(self.fovy, DEFAULT_FOVY).clamp(0.1, 2.5);
        let d = sane(self.distance, self.scene_radius * 3.0);
        self.distance = d.clamp(self.min_distance(), self.max_distance());
    }

    /// Hedeften kameraya birim yön.
    pub fn back_dir(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.back_dir() * self.eye_distance()
    }

    /// Ortografikte göz, sahnenin dışında kalacak kadar geride durur (kırpma olmasın).
    fn eye_distance(&self) -> f32 {
        match self.projection {
            Projection::Perspective => self.distance,
            Projection::Orthographic => self.distance + self.scene_radius * 6.0,
        }
    }

    /// Kamera sağ ve yukarı birim vektörleri (ekran düzlemi).
    pub fn basis(&self) -> (Vec3, Vec3) {
        let f = -self.back_dir();
        let right = f.cross(Vec3::Z).normalized();
        let right = if right == Vec3::ZERO { Vec3::Y } else { right };
        let up = right.cross(f);
        (right, up)
    }

    /// Hedef düzleminde görünür yarı yükseklik.
    pub fn half_height(&self) -> f32 {
        self.distance * (self.fovy * 0.5).tan()
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.eye(), self.target, Vec3::Z)
    }

    pub fn near_far(&self) -> (f32, f32) {
        let r = self.scene_radius;
        let d = self.eye_distance();
        let far = d + r * 4.0;
        let near = (d - r * 4.0).max(d * 0.01).max(0.001);
        (near, far)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 1e-3 {
            aspect
        } else {
            1.0
        };
        let (near, far) = self.near_far();
        match self.projection {
            Projection::Perspective => Mat4::perspective(self.fovy, aspect, near, far),
            Projection::Orthographic => {
                let h = self.half_height();
                let w = h * aspect;
                Mat4::orthographic(-w, w, -h, h, near, far)
            }
        }
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj(aspect) * self.view()
    }

    /// Dünya noktasını [0,1]² ekran kesrine izdüşürür (y aşağı). Kameranın arkasındaysa `None`.
    pub fn project(&self, p: Vec3, aspect: f32) -> Option<(f32, f32)> {
        let view = self.view();
        let pv = view.transform_point(p);
        if self.projection == Projection::Perspective && pv.z >= -1e-4 {
            return None;
        }
        let ndc = self.proj(aspect).transform_point(pv);
        if !ndc.is_finite() {
            return None;
        }
        Some(((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5))
    }

    /// Fare sürüklemesiyle döndürme (piksel cinsinden).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= sane(dx, 0.0) * ORBIT_SPEED;
        self.pitch += sane(dy, 0.0) * ORBIT_SPEED;
        self.sanitize();
    }

    /// Kaydırma: imlecin altındaki nokta imleçle birlikte hareket eder.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_h: f32) {
        if !(viewport_h.is_finite() && viewport_h > 1.0) {
            return;
        }
        let per_px = 2.0 * self.half_height() / viewport_h;
        let (right, up) = self.basis();
        self.target =
            self.target - right * (sane(dx, 0.0) * per_px) + up * (sane(dy, 0.0) * per_px);
        self.sanitize();
    }

    /// İmlecin olduğu noktaya doğru yakınlaştırma. `factor` < 1 yaklaştırır. `ndc` imlecin
    /// görünüm içindeki konumu (-1..1, y yukarı).
    pub fn zoom_at(&mut self, factor: f32, ndc: (f32, f32), aspect: f32) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let old = self.distance;
        let new = (old * factor).clamp(self.min_distance(), self.max_distance());
        let k = new / old;
        if (k - 1.0).abs() < 1e-6 {
            return;
        }
        let aspect = if aspect.is_finite() && aspect > 1e-3 {
            aspect
        } else {
            1.0
        };
        let (right, up) = self.basis();
        let h = self.half_height();
        let (nx, ny) = (
            sane(ndc.0, 0.0).clamp(-1.0, 1.0),
            sane(ndc.1, 0.0).clamp(-1.0, 1.0),
        );
        let p = self.target + right * (nx * h * aspect) + up * (ny * h);
        self.target = p + (self.target - p) * k;
        self.distance = new;
        self.sanitize();
    }

    /// Sahneyi (merkez + yarıçap) görünüme sığdırır; açıyı değiştirmez.
    pub fn fit(&mut self, center: Vec3, radius: f32, aspect: f32) {
        let r = sane(radius, 1.0).max(0.01);
        self.scene_radius = r;
        self.target = if center.is_finite() {
            center
        } else {
            Vec3::ZERO
        };
        self.distance = fit_distance(r, self.fovy, aspect);
        self.sanitize();
    }

    pub fn with_preset(mut self, p: Preset) -> Self {
        let (y, pt) = p.angles();
        self.yaw = y;
        self.pitch = pt;
        self.sanitize();
        self
    }
}

/// Yarıçapı `r` olan küreyi dikey ve yatay görüş açısına sığdıran mesafe (%12 pay).
pub fn fit_distance(r: f32, fovy: f32, aspect: f32) -> f32 {
    let aspect = if aspect.is_finite() && aspect > 1e-3 {
        aspect
    } else {
        1.0
    };
    let fovy = if fovy.is_finite() {
        fovy.clamp(0.1, 2.5)
    } else {
        DEFAULT_FOVY
    };
    let fovx = 2.0 * ((fovy * 0.5).tan() * aspect).atan();
    let half = fovy.min(fovx) * 0.5;
    (r / half.sin()) * 1.12
}

/// Hedefe doğru yumuşak geçiş: üstel yaklaşım, yaw en kısa yoldan. Bittiyse `true`.
pub fn approach(cur: &mut OrbitCamera, goal: &OrbitCamera, dt: f32) -> bool {
    let dt = if dt.is_finite() {
        dt.clamp(0.0, 0.1)
    } else {
        0.016
    };
    let a = 1.0 - (-dt * 14.0).exp();
    let dyaw = wrap_angle(goal.yaw - cur.yaw);
    cur.yaw += dyaw * a;
    cur.pitch += (goal.pitch - cur.pitch) * a;
    cur.distance += (goal.distance - cur.distance) * a;
    cur.target = cur.target.lerp(goal.target, a);
    cur.projection = goal.projection;
    cur.scene_radius = goal.scene_radius;
    cur.sanitize();
    let done = wrap_angle(goal.yaw - cur.yaw).abs() < 1e-3
        && (goal.pitch - cur.pitch).abs() < 1e-3
        && (goal.distance - cur.distance).abs() < goal.distance.abs() * 1e-3 + 1e-5
        && (goal.target - cur.target).length() < goal.scene_radius * 1e-3 + 1e-6;
    if done {
        *cur = *goal;
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, t: f32) -> bool {
        (a - b).abs() <= t
    }

    #[test]
    fn orbit_never_flips_over_poles() {
        let mut c = OrbitCamera::default();
        for _ in 0..500 {
            c.orbit(3.0, 40.0);
        }
        assert!(approx(c.pitch, PITCH_LIMIT, 1e-6));
        let (_, up) = c.basis();
        assert!(
            up.z > 0.0,
            "ekran yukarısı dünya yukarısıyla aynı yarıda kalmalı"
        );
        for _ in 0..1000 {
            c.orbit(-7.0, -40.0);
        }
        assert!(approx(c.pitch, -PITCH_LIMIT, 1e-6));
        assert!(c.yaw > -PI - 1e-6 && c.yaw <= PI + 1e-6);
        assert!(c.view().to_cols_array().iter().all(|v| v.is_finite()));
    }

    #[test]
    fn presets_look_from_expected_sides() {
        let base = OrbitCamera::default();
        let top = base.with_preset(Preset::Top);
        assert!(top.back_dir().z > 0.99);
        // Üst görünümde burun (+X) ekranda yukarı.
        let (_, up) = top.basis();
        assert!(up.x > 0.99, "{up:?}");
        let front = base.with_preset(Preset::Front);
        assert!(approx(front.back_dir().x, 1.0, 1e-5));
        let side = base.with_preset(Preset::Side);
        assert!(approx(side.back_dir().y, -1.0, 1e-5));
        let (right, _) = side.basis();
        assert!(right.x > 0.99, "yan görünümde burun sağda: {right:?}");
    }

    #[test]
    fn target_projects_to_screen_center() {
        let mut c = OrbitCamera::default();
        c.target = Vec3::new(0.3, -0.2, 0.1);
        for proj in [Projection::Perspective, Projection::Orthographic] {
            c.projection = proj;
            let (x, y) = c.project(c.target, 1.6).unwrap();
            assert!(approx(x, 0.5, 1e-4) && approx(y, 0.5, 1e-4));
        }
        // Kameranın arkasındaki nokta perspektifte görünmez.
        c.projection = Projection::Perspective;
        let behind = c.eye() + c.back_dir() * 1.0;
        assert!(c.project(behind, 1.6).is_none());
    }

    #[test]
    fn pan_moves_target_in_screen_plane() {
        let mut c = OrbitCamera::default();
        let d0 = c.distance;
        let t0 = c.target;
        c.pan(100.0, 0.0, 500.0);
        let moved = c.target - t0;
        let (right, up) = c.basis();
        assert!(moved.dot(right) < 0.0);
        assert!(approx(moved.dot(up), 0.0, 1e-5));
        assert!(approx(
            moved.length(),
            2.0 * c.half_height() * 100.0 / 500.0,
            1e-4
        ));
        assert_eq!(c.distance, d0);
        // Bozuk görünüm yüksekliği yok sayılır.
        let t1 = c.target;
        c.pan(10.0, 10.0, 0.0);
        assert_eq!(c.target, t1);
    }

    #[test]
    fn zoom_keeps_point_under_cursor_and_respects_limits() {
        for proj in [Projection::Perspective, Projection::Orthographic] {
            let mut c = OrbitCamera {
                projection: proj,
                ..Default::default()
            };
            let aspect = 1.5;
            let ndc = (0.6, -0.4);
            // İmlecin altındaki hedef-düzlemi noktası
            let (right, up) = c.basis();
            let h = c.half_height();
            let p = c.target + right * (ndc.0 * h * aspect) + up * (ndc.1 * h);
            let (sx, sy) = c.project(p, aspect).unwrap();
            c.zoom_at(0.8, ndc, aspect);
            let (sx2, sy2) = c.project(p, aspect).unwrap();
            assert!(approx(sx, sx2, 1e-3) && approx(sy, sy2, 1e-3), "{proj:?}");
        }
        let mut c = OrbitCamera::default();
        for _ in 0..200 {
            c.zoom_at(0.5, (0.0, 0.0), 1.0);
        }
        assert!(approx(c.distance, c.min_distance(), 1e-6));
        for _ in 0..200 {
            c.zoom_at(2.0, (0.0, 0.0), 1.0);
        }
        assert!(approx(c.distance, c.max_distance(), 1e-3));
        let d = c.distance;
        c.zoom_at(f32::NAN, (0.0, 0.0), 1.0);
        c.zoom_at(-1.0, (0.0, 0.0), 1.0);
        assert_eq!(c.distance, d);
    }

    #[test]
    fn fit_contains_whole_sphere() {
        let mut c = OrbitCamera::default();
        for aspect in [0.5_f32, 1.0, 2.5] {
            c.fit(Vec3::new(1.0, 2.0, 0.5), 0.8, aspect);
            assert_eq!(c.target, Vec3::new(1.0, 2.0, 0.5));
            // Kürenin en uç noktaları ekranın içinde.
            let (right, up) = c.basis();
            for d in [right, -right, up, -up] {
                let (x, y) = c.project(c.target + d * 0.8, aspect).unwrap();
                assert!(
                    (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
                    "{aspect} {x} {y}"
                );
            }
        }
        c.fit(Vec3::new(f32::NAN, 0.0, 0.0), f32::NAN, f32::NAN);
        assert!(c.target.is_finite() && c.distance.is_finite());
    }

    #[test]
    fn approach_converges_via_shortest_yaw() {
        let mut cur = OrbitCamera {
            yaw: 3.0,
            ..Default::default()
        };
        let goal = OrbitCamera {
            yaw: -3.0,
            pitch: 1.0,
            distance: 5.0,
            ..Default::default()
        };
        // İlk adımda yaw, π sınırının üzerinden (artarak) gitmeli.
        let before = cur.yaw;
        approach(&mut cur, &goal, 0.016);
        assert!(wrap_angle(cur.yaw - before) > 0.0);
        let mut done = false;
        for _ in 0..400 {
            if approach(&mut cur, &goal, 0.016) {
                done = true;
                break;
            }
        }
        assert!(done);
        assert_eq!(cur, goal);
    }

    #[test]
    fn drag_mode_mapping() {
        assert_eq!(drag_mode(true, false, false, false), Some(DragMode::Orbit));
        assert_eq!(drag_mode(true, false, false, true), Some(DragMode::Pan));
        assert_eq!(drag_mode(false, true, false, false), Some(DragMode::Pan));
        assert_eq!(drag_mode(false, false, true, false), Some(DragMode::Pan));
        assert_eq!(drag_mode(false, false, false, false), None);
    }

    #[test]
    fn wrap_angle_range() {
        assert!(approx(wrap_angle(3.0 * PI), PI, 1e-5) || approx(wrap_angle(3.0 * PI), -PI, 1e-5));
        assert!(approx(wrap_angle(-0.5), -0.5, 1e-6));
        assert_eq!(wrap_angle(f32::NAN), 0.0);
    }
}
