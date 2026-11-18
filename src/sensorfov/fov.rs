// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kamera görüş açısı, yer ayak izi, GSD ve gövde/pervane/kol/iniş takımı görüntü kapanması.
//!
//! Gövde ekseni: +x ileri, +y sağ, +z yukarı; birim mm. Kamera ışını ileriden aşağı doğru
//! `tilt` kadar eğilir (0° = ileri, 90° = nadir).
use crate::cad::freecad::{MotorMountCadParams, mr_arm_angles};
use crate::cad::outputs::LandingLegCadParams;
use crate::calc::Model;
use crate::project::{DroneType, Project};

pub type V3 = [f64; 3];

/// Tipik sensör örnekleri (genişlik, yükseklik mm).
pub struct SensorPreset {
    pub name: &'static str,
    pub w_mm: f64,
    pub h_mm: f64,
}

pub const SENSOR_PRESETS: &[SensorPreset] = &[
    SensorPreset {
        name: "1/2.3 in (6.17 x 4.55)",
        w_mm: 6.17,
        h_mm: 4.55,
    },
    SensorPreset {
        name: "1/2.0 in (6.4 x 4.8)",
        w_mm: 6.4,
        h_mm: 4.8,
    },
    SensorPreset {
        name: "1 in (13.2 x 8.8)",
        w_mm: 13.2,
        h_mm: 8.8,
    },
    SensorPreset {
        name: "Four Thirds (17.3 x 13)",
        w_mm: 17.3,
        h_mm: 13.0,
    },
    SensorPreset {
        name: "APS-C (23.5 x 15.6)",
        w_mm: 23.5,
        h_mm: 15.6,
    },
    SensorPreset {
        name: "Full frame (36 x 24)",
        w_mm: 36.0,
        h_mm: 24.0,
    },
];

/// Tam görüş açısı [°]: 2·atan(boyut / 2f).
pub fn fov_deg(size_mm: f64, focal_mm: f64) -> f64 {
    if !(size_mm > 0.0 && focal_mm > 0.0) {
        return 0.0;
    }
    2.0 * (size_mm / (2.0 * focal_mm)).atan().to_degrees()
}

/// Verilen tam görüş açısını veren odak uzaklığı [mm].
pub fn focal_from_fov(size_mm: f64, fov_deg: f64) -> f64 {
    let half = (fov_deg.clamp(1.0, 179.0) / 2.0).to_radians().tan();
    if size_mm > 0.0 {
        size_mm / (2.0 * half)
    } else {
        0.0
    }
}

/// Nadirde yer örnekleme aralığı [cm/piksel]: irtifa · piksel adımı / odak.
pub fn gsd_cm_px(alt_m: f64, sensor_w_mm: f64, focal_mm: f64, px_w: f64) -> f64 {
    if !(alt_m > 0.0 && sensor_w_mm > 0.0 && focal_mm > 0.0 && px_w > 0.0) {
        return 0.0;
    }
    alt_m * (sensor_w_mm / px_w) / focal_mm * 100.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    /// Nadir noktasından ileri doğru en yakın/en uzak yer mesafesi [m] (uzak: ufuk üstüyse yok).
    pub near_m: f64,
    pub far_m: Option<f64>,
    pub width_near_m: f64,
    pub width_far_m: Option<f64>,
    /// Işın merkezinde çapraz-iz GSD [cm/px] ve iz boyunca GSD [cm/px] (ufukta/üzerinde None).
    pub gsd_cross_cm: Option<f64>,
    pub gsd_along_cm: Option<f64>,
    /// Yaklaşık yer alanı [m²] (yamuk; uzak kenar yoksa None).
    pub area_m2: Option<f64>,
}

/// Yer ayak izi (düz zemin, kamera `alt_m` irtifada). `tilt` ufuktan aşağı açı.
pub fn footprint(
    alt_m: f64,
    hfov: f64,
    vfov: f64,
    tilt: f64,
    sensor_w: f64,
    f: f64,
    px_w: f64,
) -> Footprint {
    let tilt = tilt.clamp(0.0, 90.0);
    let alt = alt_m.max(0.0);
    let ground = |phi_deg: f64| -> Option<f64> {
        if phi_deg <= 0.01 || phi_deg >= 179.99 {
            None
        } else {
            Some(alt / phi_deg.to_radians().tan())
        }
    };
    let width_at = |d: f64| {
        2.0 * (alt * alt + d * d).sqrt()
            * (vfov / 2.0).to_radians().cos()
            * (hfov / 2.0).to_radians().tan()
    };
    let near_m = ground(tilt + vfov / 2.0).unwrap_or(0.0);
    let far_m = ground(tilt - vfov / 2.0);
    let pitch = if px_w > 0.0 { sensor_w / px_w } else { 0.0 };
    let (gsd_cross_cm, gsd_along_cm) = if tilt > 0.0 {
        let s = tilt.to_radians().sin();
        let g = if f > 0.0 {
            (alt / s) * pitch / f * 100.0
        } else {
            0.0
        };
        (Some(g), Some(g / s))
    } else {
        (None, None)
    };
    let width_near = width_at(near_m);
    let width_far = far_m.map(width_at);
    let area =
        far_m.map(|fm| 0.5 * (width_near + width_far.unwrap_or(0.0)) * (fm - near_m).max(0.0));
    Footprint {
        near_m,
        far_m,
        width_near_m: width_near,
        width_far_m: width_far,
        gsd_cross_cm,
        gsd_along_cm,
        area_m2: area,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mount {
    Nose,
    Belly,
    Gimbal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Prop,
    Arm,
    Gear,
    Fuselage,
}

/// Dışbükey katı: ardışık halkalar (her halka aynı sayıda köşe). Halka içi ve halkalar arası
/// kenarlar derinlik kırpması için kullanılır; izdüşüm bu kenarların dışbükey zarfıdır.
#[derive(Clone, Debug)]
pub struct Solid {
    pub kind: ItemKind,
    pub label: String,
    pub rings: Vec<Vec<V3>>,
}

impl Solid {
    fn edges(&self) -> Vec<(V3, V3)> {
        let mut e = Vec::new();
        for (ri, ring) in self.rings.iter().enumerate() {
            let n = ring.len();
            for i in 0..n {
                e.push((ring[i], ring[(i + 1) % n]));
                if let Some(next) = self.rings.get(ri + 1) {
                    e.push((ring[i], next[i % next.len()]));
                }
            }
        }
        e
    }
    pub fn vertices(&self) -> impl Iterator<Item = V3> + '_ {
        self.rings.iter().flatten().copied()
    }
}

/// `axis` 0: x eksenine dik disk (yz düzleminde); aksi: z eksenine dik (xy düzleminde).
fn circle_ring(c: V3, r: f64, axis: usize, n: usize) -> Vec<V3> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            let (s, co) = a.sin_cos();
            match axis {
                0 => [c[0], c[1] + r * co, c[2] + r * s],
                _ => [c[0] + r * co, c[1] + r * s, c[2]],
            }
        })
        .collect()
}

/// Z eksenine dik dikdörtgen halka: başlangıçtan `ang_deg` yönünde `len`, yanlara ±w/2.
fn bar_ring(ang_deg: f64, len: f64, w: f64, z: f64) -> Vec<V3> {
    let (s, c) = ang_deg.to_radians().sin_cos();
    let p = |a: f64, b: f64| [c * a - s * b, s * a + c * b, z];
    vec![
        p(0.0, -w / 2.0),
        p(len, -w / 2.0),
        p(len, w / 2.0),
        p(0.0, w / 2.0),
    ]
}

fn box_ring_z(x0: f64, x1: f64, y0: f64, y1: f64, z: f64) -> Vec<V3> {
    vec![[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]]
}

#[derive(Clone, Debug)]
pub struct Airframe {
    pub items: Vec<Solid>,
    /// Kamera konumları (mm): Nose, Belly, Gimbal.
    pub cam_pos: [V3; 3],
}

impl Airframe {
    pub fn camera_pos(&self, m: Mount) -> V3 {
        self.cam_pos[match m {
            Mount::Nose => 0,
            Mount::Belly => 1,
            Mount::Gimbal => 2,
        }]
    }

    /// Projeden basitleştirilmiş gövde: pervane diskleri, kollar/bomlar, iniş takımı, gövde.
    pub fn from_project(p: &Project, m: &Model, gimbal_drop_mm: f64) -> Self {
        let mut items: Vec<Solid> = Vec::new();
        let leg = LandingLegCadParams::from_project(p);
        let leg_solids = |items: &mut Vec<Solid>, x: f64, z_top: f64, y_top: f64| {
            let sp = leg.splay_deg.to_radians();
            for (k, sgn) in [1.0_f64, -1.0].into_iter().enumerate() {
                let y1 = sgn * (y_top.abs() + leg.leg_len_mm * sp.sin());
                let z1 = z_top - leg.leg_len_mm * sp.cos();
                let (y0a, y0b) = (sgn * y_top - 3.0, sgn * y_top + 3.0);
                let top = box_ring_z(x - 3.0, x + 3.0, y0a, y0b, z_top);
                let foot = box_ring_z(
                    x - leg.foot_len_mm / 2.0,
                    x + leg.foot_len_mm / 2.0,
                    y1 - leg.foot_w_mm / 2.0,
                    y1 + leg.foot_w_mm / 2.0,
                    z1,
                );
                items.push(Solid {
                    kind: ItemKind::Gear,
                    label: format!("Gear {}", k + 1),
                    rings: vec![top, foot],
                });
            }
        };
        let cam_pos = if p.drone_type == DroneType::Multirotor {
            let n = p.mr_arm_count.max(1);
            let angles = mr_arm_angles(p.mr_layout, n);
            let plate_r = (p.mr_r_plate * 1000.0).clamp(25.0, 150.0);
            let arm_l = (p.mr_frame_size * 500.0).max(60.0);
            let r_prop = (p.mr_d_prop * 500.0).max(20.0);
            for (i, a) in angles.iter().enumerate() {
                items.push(Solid {
                    kind: ItemKind::Arm,
                    label: format!("Arm {}", i + 1),
                    rings: vec![
                        bar_ring(*a, arm_l, 24.0, 0.0),
                        bar_ring(*a, arm_l, 24.0, 4.0),
                    ],
                });
                let (s, c) = a.to_radians().sin_cos();
                let (cx, cy) = (arm_l * c, arm_l * s);
                items.push(Solid {
                    kind: ItemKind::Prop,
                    label: format!("Prop {}", i + 1),
                    rings: vec![
                        circle_ring([cx, cy, 28.0], r_prop, 2, 24),
                        circle_ring([cx, cy, 32.0], r_prop, 2, 24),
                    ],
                });
                if p.mr_coaxial {
                    items.push(Solid {
                        kind: ItemKind::Prop,
                        label: format!("Prop {}b", i + 1),
                        rings: vec![
                            circle_ring([cx, cy, -22.0], r_prop, 2, 24),
                            circle_ring([cx, cy, -18.0], r_prop, 2, 24),
                        ],
                    });
                }
            }
            leg_solids(&mut items, 0.0, 0.0, 30.0);
            [
                [plate_r, 0.0, -10.0],
                [0.0, 0.0, -3.0],
                [plate_r * 0.5, 0.0, -gimbal_drop_mm],
            ]
        } else {
            let len = (p.l_fuse * 1000.0).max(100.0);
            let r = (p.d_fuse * 500.0).max(15.0);
            let mount = MotorMountCadParams::from_project(p, m);
            let prof = [(0.0, 0.3), (0.25, 1.0), (0.65, 1.0), (1.0, 0.3)];
            items.push(Solid {
                kind: ItemKind::Fuselage,
                label: "Fuselage".into(),
                rings: prof
                    .iter()
                    .map(|(xf, rf)| circle_ring([-len * xf, 0.0, 0.0], r * rf, 0, 12))
                    .collect(),
            });
            let r_main = (p.d_prop * 500.0).max(20.0);
            let x_main = if mount.mount_type == "pusher" {
                -len - 15.0
            } else {
                20.0
            };
            items.push(Solid {
                kind: ItemKind::Prop,
                label: "Prop".into(),
                rings: vec![
                    circle_ring([x_main, 0.0, 0.0], r_main, 0, 24),
                    circle_ring([x_main + 4.0, 0.0, 0.0], r_main, 0, 24),
                ],
            });
            let x_wle = -p.x_root_le * 1000.0;
            let c_mean = if p.b > 0.0 { p.s / p.b * 1000.0 } else { 150.0 };
            if p.drone_type == DroneType::VtolQuadplane {
                let r_l = (p.mr_d_prop * 500.0).max(20.0);
                let y_b = (r + r_l + 15.0).min(0.3 * p.b * 1000.0).max(r + 10.0);
                let diag = p.mr_frame_size * 1000.0;
                let l_diag = if diag > 2.1 * y_b {
                    (diag * diag - 4.0 * y_b * y_b).sqrt()
                } else {
                    0.0
                };
                let l_m = l_diag.max(2.0 * r_l + 30.0);
                let xc = x_wle - 0.25 * c_mean;
                for (si, sgn) in [1.0_f64, -1.0].into_iter().enumerate() {
                    let y = sgn * y_b;
                    items.push(Solid {
                        kind: ItemKind::Arm,
                        label: format!("Boom {}", si + 1),
                        rings: vec![
                            box_ring_z(xc - l_m / 2.0, xc + l_m / 2.0, y - 8.0, y + 8.0, -8.0),
                            box_ring_z(xc - l_m / 2.0, xc + l_m / 2.0, y - 8.0, y + 8.0, 8.0),
                        ],
                    });
                    for (fi, dx) in [l_m / 2.0, -l_m / 2.0].into_iter().enumerate() {
                        items.push(Solid {
                            kind: ItemKind::Prop,
                            label: format!("Lift {}{}", si + 1, if fi == 0 { "F" } else { "A" }),
                            rings: vec![
                                circle_ring([xc + dx, y, 22.0], r_l, 2, 24),
                                circle_ring([xc + dx, y, 26.0], r_l, 2, 24),
                            ],
                        });
                    }
                }
            }
            leg_solids(&mut items, x_wle, -r, leg.tab_w_mm / 2.0 + 0.5);
            [
                [1.0, 0.0, 0.0],
                [-0.35 * len, 0.0, -r - 3.0],
                [-0.35 * len, 0.0, -r - gimbal_drop_mm],
            ]
        };
        Airframe { items, cam_pos }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: V3,
    pub tilt_deg: f64,
    pub hfov_deg: f64,
    pub vfov_deg: f64,
}

impl Camera {
    /// (optik eksen, sağ, yukarı).
    fn axes(&self) -> (V3, V3, V3) {
        let (s, c) = self.tilt_deg.to_radians().sin_cos();
        ([c, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, c])
    }
    /// Görüntü köşe ışınlarının (sol-alt, sağ-alt, sağ-üst, sol-üst) yön vektörleri (derinlik 1).
    pub fn corner_dirs(&self) -> [V3; 4] {
        let (a, r, u) = self.axes();
        let th = (self.hfov_deg / 2.0).to_radians().tan();
        let tv = (self.vfov_deg / 2.0).to_radians().tan();
        let d = |su: f64, sv: f64| {
            [
                a[0] + su * th * r[0] + sv * tv * u[0],
                a[1] + su * th * r[1] + sv * tv * u[1],
                a[2] + su * th * r[2] + sv * tv * u[2],
            ]
        };
        [d(-1.0, -1.0), d(1.0, -1.0), d(1.0, 1.0), d(-1.0, 1.0)]
    }
}

const GRID_W: usize = 64;
const GRID_H: usize = 48;
const DEPTH_EPS: f64 = 1.0;

fn dotv(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Dışbükey zarf (saat yönü tersi), monoton zincir.
pub fn hull(mut pts: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut h: Vec<[f64; 2]> = Vec::new();
    for pass in 0..2 {
        let start = h.len();
        let iter: Vec<[f64; 2]> = if pass == 0 {
            pts.clone()
        } else {
            pts.iter().rev().copied().collect()
        };
        for p in iter {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], p) <= 0.0 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
    }
    h
}

/// Katının kamera görüntü düzlemindeki (u/derinlik, v/derinlik) dışbükey zarfı.
fn project(s: &Solid, cam: &Camera) -> Vec<[f64; 2]> {
    let (a, r, up) = cam.axes();
    let rel = |q: V3| [q[0] - cam.pos[0], q[1] - cam.pos[1], q[2] - cam.pos[2]];
    let depth = |q: V3| dotv(rel(q), a);
    let mut pts: Vec<V3> = Vec::new();
    for (p, q) in s.edges() {
        let (dp, dq) = (depth(p), depth(q));
        match (dp >= DEPTH_EPS, dq >= DEPTH_EPS) {
            (true, true) => {
                pts.push(p);
                pts.push(q);
            }
            (true, false) | (false, true) => {
                let (inn, out, di, dout) = if dp >= DEPTH_EPS {
                    (p, q, dp, dq)
                } else {
                    (q, p, dq, dp)
                };
                let t = (di - DEPTH_EPS) / (di - dout);
                pts.push(inn);
                pts.push([
                    inn[0] + (out[0] - inn[0]) * t,
                    inn[1] + (out[1] - inn[1]) * t,
                    inn[2] + (out[2] - inn[2]) * t,
                ]);
            }
            _ => {}
        }
    }
    let uv: Vec<[f64; 2]> = pts
        .into_iter()
        .map(|q| {
            let d = rel(q);
            let z = dotv(d, a).max(DEPTH_EPS);
            [dotv(d, r) / z, dotv(d, up) / z]
        })
        .collect();
    hull(uv)
}

fn in_convex(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    (0..poly.len()).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0
    })
}

#[derive(Clone, Debug)]
pub struct ItemBlock {
    pub index: usize,
    pub kind: ItemKind,
    pub label: String,
    /// Görüntünün yüzde kaçını kapattığı (tek başına).
    pub pct: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ObstructionReport {
    /// Birleşim: görüntünün yüzde kaçı kapalı (üst üste binenler bir kez sayılır).
    pub total_pct: f64,
    /// %0,5 üzeri kapatanlar, büyükten küçüğe.
    pub items: Vec<ItemBlock>,
}

/// Kamera görüntüsünün yaklaşık yüzde kaçının hangi parçalarca kapandığı (64x48 ızgara).
pub fn obstruction(items: &[Solid], cam: &Camera) -> ObstructionReport {
    let ok = |a: f64| a > 0.0 && a < 179.0;
    if !(ok(cam.hfov_deg) && ok(cam.vfov_deg)) {
        return ObstructionReport::default();
    }
    let th = (cam.hfov_deg / 2.0).to_radians().tan();
    let tv = (cam.vfov_deg / 2.0).to_radians().tan();
    let mut union = vec![false; GRID_W * GRID_H];
    let mut out = Vec::new();
    for (index, s) in items.iter().enumerate() {
        let poly = project(s, cam);
        if poly.len() < 3 {
            continue;
        }
        let mut cnt = 0usize;
        for j in 0..GRID_H {
            for i in 0..GRID_W {
                let u = ((i as f64 + 0.5) / GRID_W as f64 * 2.0 - 1.0) * th;
                let v = (1.0 - (j as f64 + 0.5) / GRID_H as f64 * 2.0) * tv;
                if in_convex(&poly, [u, v]) {
                    cnt += 1;
                    union[j * GRID_W + i] = true;
                }
            }
        }
        let pct = 100.0 * cnt as f64 / (GRID_W * GRID_H) as f64;
        if pct >= 0.5 {
            out.push(ItemBlock {
                index,
                kind: s.kind,
                label: s.label.clone(),
                pct,
            });
        }
    }
    out.sort_by(|a, b| b.pct.total_cmp(&a.pct));
    let total = 100.0 * union.iter().filter(|b| **b).count() as f64 / union.len() as f64;
    ObstructionReport {
        total_pct: total,
        items: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn hfov_from_sensor_and_focal() {
        let h = fov_deg(6.17, 4.5);
        assert!((h - 68.9).abs() < 0.2, "{h}");
        let f = focal_from_fov(6.17, h);
        assert!((f - 4.5).abs() < 1e-9);
        assert_eq!(fov_deg(0.0, 4.5), 0.0);
    }

    #[test]
    fn gsd_formula() {
        // 100 m, 6.17 mm / 4000 px, 4.5 mm odak => 100 * 0.0015425 / 4.5 = 3.43 cm/px.
        let g = gsd_cm_px(100.0, 6.17, 4.5, 4000.0);
        assert!((g - 3.428).abs() < 0.01, "{g}");
        assert_eq!(gsd_cm_px(0.0, 6.17, 4.5, 4000.0), 0.0);
    }

    #[test]
    fn nadir_footprint_matches_fov() {
        let hfov = fov_deg(6.17, 4.5);
        let f = footprint(100.0, hfov, 55.0, 90.0, 6.17, 4.5, 4000.0);
        let w = 100.0 * 6.17 / 4.5;
        let far = f.far_m.unwrap();
        let wf = f.width_far_m.unwrap();
        let area = f.area_m2.unwrap();
        assert!((f.near_m + far).abs() < 1e-9, "{} vs {}", f.near_m, far);
        assert!(
            (f.width_near_m - w).abs() < 1e-9,
            "{} vs {w}",
            f.width_near_m
        );
        assert!((wf - w).abs() < 1e-9, "{wf} vs {w}");
        assert!(
            (area - w * (far - f.near_m)).abs() < 1e-6,
            "{area} vs {}",
            w * (far - f.near_m)
        );
        let gc = f.gsd_cross_cm.unwrap();
        let ga = f.gsd_along_cm.unwrap();
        assert!((gc - ga).abs() < 1e-9);

        let oblique = footprint(100.0, hfov, 55.0, 20.0, 6.17, 4.5, 4000.0);
        assert!(oblique.far_m.is_none(), "uzak kenar ufuk üstünde");

        let horiz = footprint(100.0, hfov, 55.0, 0.0, 6.17, 4.5, 4000.0);
        let expected_near = 100.0 / (55.0_f64 / 2.0).to_radians().tan();
        assert!(
            (horiz.near_m - expected_near).abs() < 1e-9,
            "{} vs {expected_near}",
            horiz.near_m
        );
        assert!(horiz.far_m.is_none());
        assert!(horiz.gsd_cross_cm.is_none() && horiz.gsd_along_cm.is_none());
    }

    fn disc(x: f64, y: f64, z: f64, r: f64) -> Solid {
        Solid {
            kind: ItemKind::Prop,
            label: "p".into(),
            rings: vec![
                circle_ring([x, y, z], r, 0, 24),
                circle_ring([x + 4.0, y, z], r, 0, 24),
            ],
        }
    }

    fn cam(tilt: f64) -> Camera {
        Camera {
            pos: [0.0; 3],
            tilt_deg: tilt,
            hfov_deg: 60.0,
            vfov_deg: 45.0,
        }
    }

    #[test]
    fn prop_inside_wedge_is_detected_outside_is_not() {
        let inside = obstruction(&[disc(300.0, 0.0, 0.0, 60.0)], &cam(0.0));
        assert!(inside.total_pct > 3.0, "{}", inside.total_pct);
        assert_eq!(inside.items.len(), 1);
        let outside = obstruction(&[disc(300.0, 900.0, 0.0, 60.0)], &cam(0.0));
        assert_eq!(outside.total_pct, 0.0);
        assert!(outside.items.is_empty());
        let behind = obstruction(&[disc(-300.0, 0.0, 0.0, 60.0)], &cam(0.0));
        assert_eq!(behind.total_pct, 0.0);
    }

    #[test]
    fn tilt_moves_obstruction_out_of_view() {
        let item = [disc(300.0, 0.0, 0.0, 40.0)];
        assert!(obstruction(&item, &cam(0.0)).total_pct > 1.0);
        assert_eq!(obstruction(&item, &cam(90.0)).total_pct, 0.0);
    }

    #[test]
    fn all_vehicle_types_are_finite() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            let af = Airframe::from_project(&p, &m, 60.0);
            assert!(!af.items.is_empty());
            for mount in [Mount::Nose, Mount::Belly, Mount::Gimbal] {
                for tilt in [0.0, 30.0, 60.0, 90.0] {
                    let c = Camera {
                        pos: af.camera_pos(mount),
                        tilt_deg: tilt,
                        hfov_deg: 68.9,
                        vfov_deg: 55.0,
                    };
                    let r = obstruction(&af.items, &c);
                    assert!(
                        r.total_pct.is_finite() && (0.0..=100.0).contains(&r.total_pct),
                        "{dt:?} {r:?}"
                    );
                    assert!(r.items.iter().all(|i| i.pct.is_finite()));
                    if mount == Mount::Gimbal && tilt == 90.0 {
                        assert!(r.total_pct < 100.0, "{dt:?} gimbal 90: {}", r.total_pct);
                    }
                    if mount == Mount::Nose && tilt == 0.0 {
                        assert!(
                            !r.items.iter().any(|i| i.kind == ItemKind::Fuselage),
                            "{dt:?} nose 0° fuselage self-occlusion: {:?}",
                            r.items
                        );
                        if matches!(dt, DroneType::Multirotor | DroneType::FlyingWing) {
                            assert!(r.total_pct < 100.0, "{dt:?} nose 0: {}", r.total_pct);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn multirotor_plus_nose_is_below_forward_arm() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        p.mr_layout = crate::calc::multirotor::MultirotorLayout::Plus;
        let m = evaluate(&p, &Library::embedded());
        let af = Airframe::from_project(&p, &m, 60.0);
        let c0 = Camera {
            pos: af.camera_pos(Mount::Nose),
            tilt_deg: 0.0,
            hfov_deg: 68.9,
            vfov_deg: 55.0,
        };
        let r0 = obstruction(&af.items, &c0);
        let arm1_pct = r0
            .items
            .iter()
            .find(|i| i.label == "Arm 1")
            .map_or(0.0, |i| i.pct);
        assert!(
            arm1_pct < 30.0,
            "arm should not block upper half of the view: {r0:?}"
        );
        let r30 = obstruction(
            &af.items,
            &Camera {
                tilt_deg: 30.0,
                ..c0
            },
        );
        assert!(
            !r30.items.iter().any(|i| i.label == "Arm 1"),
            "arm should be above 30° downward view: {r30:?}"
        );
    }
}
