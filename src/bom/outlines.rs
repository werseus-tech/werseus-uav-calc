// SPDX-License-Identifier: AGPL-3.0-or-later
//! Levhadan kesilecek düz parçaların ana hatları. Ölçüler `cad::layout::Layout` parametrelerinden
//! alınır (CAD betikleriyle aynı kaynak); kaburga hatları profil noktalarından, plaka/kol/yuva
//! ana hatları basit parametrik şekillerdir. Tüm uzunluklar mm.

use super::SheetMaterial;
use crate::cad::freecad::mr_arm_angles;
use crate::cad::layout::Layout;
use crate::cad::outputs::{PartId, part_quantity};
use crate::calc::Model;
use crate::i18n::tf;
use crate::project::{DroneType, Project};

/// Dış hat: çokgen (kapalı, son nokta ilkini tekrarlamaz) ya da daire (merkez orijinde).
#[derive(Clone, Debug, PartialEq)]
pub enum Outline {
    Polygon(Vec<[f64; 2]>),
    Circle(f64),
}

/// Bir kesim parçası: dış hat, delikler (merkez, çap), malzeme, kalınlık ve adet.
#[derive(Clone, Debug, PartialEq)]
pub struct FlatPart {
    pub name: String,
    pub qty: u32,
    pub material: SheetMaterial,
    pub thickness_mm: f64,
    pub outline: Outline,
    /// Delikler: (merkez [x, y], çap).
    pub holes: Vec<([f64; 2], f64)>,
}

/// Yerleştirilmiş şekil (DXF ve çizim ortak).
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Polygon(Vec<[f64; 2]>),
    Circle { c: [f64; 2], r: f64 },
}

impl FlatPart {
    /// (min, max) sınır kutusu; geçersizse `None`.
    pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
        let b = match &self.outline {
            Outline::Circle(r) => ([-r, -r], [*r, *r]),
            Outline::Polygon(pts) => {
                if pts.len() < 3 || pts.iter().flatten().any(|v| !v.is_finite()) {
                    return None;
                }
                let mut lo = [f64::INFINITY; 2];
                let mut hi = [f64::NEG_INFINITY; 2];
                for p in pts {
                    for k in 0..2 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                (lo, hi)
            }
        };
        let ok = b.0.iter().chain(b.1.iter()).all(|v| v.is_finite())
            && b.1[0] - b.0[0] > 0.0
            && b.1[1] - b.0[1] > 0.0;
        ok.then_some(b)
    }

    /// Sınır kutusu genişliği ve yüksekliği.
    pub fn size(&self) -> Option<(f64, f64)> {
        self.bounds().map(|(lo, hi)| (hi[0] - lo[0], hi[1] - lo[1]))
    }

    /// Net alan [mm²] (dış hat - delikler).
    pub fn net_area_mm2(&self) -> f64 {
        let outer = match &self.outline {
            Outline::Circle(r) => std::f64::consts::PI * r * r,
            Outline::Polygon(pts) => {
                let n = pts.len();
                (0..n)
                    .map(|i| {
                        let (a, b) = (pts[i], pts[(i + 1) % n]);
                        a[0] * b[1] - b[0] * a[1]
                    })
                    .sum::<f64>()
                    .abs()
                    / 2.0
            }
        };
        let holes: f64 = self
            .holes
            .iter()
            .map(|(_, d)| std::f64::consts::PI * d * d / 4.0)
            .sum();
        (outer - holes).max(0.0)
    }

    /// Sınır kutusunun sol-alt köşesi `origin` olacak biçimde (isteğe bağlı 90° döndürülmüş)
    /// yerleştirilmiş şekiller: önce dış hat, sonra delikler.
    pub fn placed(&self, origin: [f64; 2], rotated: bool) -> Vec<Entity> {
        let Some((lo, hi)) = self.bounds() else {
            return Vec::new();
        };
        let h = hi[1] - lo[1];
        let map = |p: [f64; 2]| -> [f64; 2] {
            let (x, y) = (p[0] - lo[0], p[1] - lo[1]);
            if rotated {
                // 90° saat yönünün tersi: (x, y) -> (h - y, x); kutu (h x w) olur.
                [origin[0] + h - y, origin[1] + x]
            } else {
                [origin[0] + x, origin[1] + y]
            }
        };
        let mut v = Vec::new();
        match &self.outline {
            Outline::Circle(r) => v.push(Entity::Circle {
                c: map([0.0, 0.0]),
                r: *r,
            }),
            Outline::Polygon(pts) => v.push(Entity::Polygon(pts.iter().map(|p| map(*p)).collect())),
        }
        for (c, d) in &self.holes {
            v.push(Entity::Circle {
                c: map(*c),
                r: d / 2.0,
            });
        }
        v
    }
}

fn rect(w: f64, h: f64, cx: f64, cy: f64) -> Vec<[f64; 2]> {
    vec![
        [cx - w / 2.0, cy - h / 2.0],
        [cx + w / 2.0, cy - h / 2.0],
        [cx + w / 2.0, cy + h / 2.0],
        [cx - w / 2.0, cy + h / 2.0],
    ]
}

const CORNERS: [(f64, f64); 4] = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];

struct MotorPlate {
    name: String,
    qty: u32,
    plate: f64,
    thickness: f64,
    pattern: f64,
    bolt: f64,
    shaft: f64,
}

fn motor_plate(a: MotorPlate) -> FlatPart {
    let q = a.pattern / (2.0 * std::f64::consts::SQRT_2);
    let mut holes = vec![([0.0, 0.0], a.shaft)];
    for (sx, sy) in CORNERS {
        holes.push(([sx * q, sy * q], a.bolt));
    }
    FlatPart {
        name: a.name,
        qty: a.qty,
        material: SheetMaterial::CarbonPlate,
        thickness_mm: a.thickness,
        outline: Outline::Polygon(rect(a.plate, a.plate, 0.0, 0.0)),
        holes,
    }
}

pub(crate) fn point_in_polygon(pts: &[[f64; 2]], p: [f64; 2]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        if ((a[1] > p[1]) != (b[1] > p[1]))
            && (p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0])
        {
            inside = !inside;
        }
    }
    inside
}

pub(crate) fn point_to_segment_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 1e-18 {
        ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
}

pub(crate) fn circle_fits_in_polygon(pts: &[[f64; 2]], c: [f64; 2], r: f64) -> bool {
    if !point_in_polygon(pts, c) {
        return false;
    }
    let n = pts.len();
    (0..n).all(|i| point_to_segment_dist(c, pts[i], pts[(i + 1) % n]) >= r)
}

fn push_unique_hole(holes: &mut Vec<([f64; 2], f64)>, c: [f64; 2], d: f64) {
    if d > 0.0
        && holes
            .iter()
            .all(|(o, od)| (o[0] - c[0]).hypot(o[1] - c[1]) >= (od + d) / 2.0)
    {
        holes.push((c, d));
    }
}

/// Tasarımın düz kesim parçaları: multirotorda merkez plaka, kollar ve motor yuvaları; sabit
/// kanatta kanat kaburgaları (içi boş kabukta) ve motor yuvaları; VTOL'de kaldırma yuvaları da.
pub fn flat_parts(p: &Project, m: &Model) -> Vec<FlatPart> {
    let lay = Layout::new(p, m);
    let mut out = Vec::new();
    if p.drone_type == DroneType::Multirotor {
        let pl = &lay.mr_plate;
        let mut holes = vec![([0.0, 0.0], pl.center_hole_dia_mm)];
        for a in mr_arm_angles(pl.layout, pl.arm_count) {
            let (s, c) = a.to_radians().sin_cos();
            for r in pl.arm_hole_r_mm {
                push_unique_hole(&mut holes, [r * c, r * s], pl.chassis_bolt_dia_mm);
            }
        }
        let q = pl.fc_mount_pattern_mm / 2.0;
        for (sx, sy) in CORNERS {
            push_unique_hole(&mut holes, [sx * q, sy * q], pl.chassis_bolt_dia_mm);
        }
        out.push(FlatPart {
            name: PartId::MrPlate.title().to_string(),
            qty: part_quantity(PartId::MrPlate, p),
            material: SheetMaterial::CarbonPlate,
            thickness_mm: pl.thickness_mm,
            outline: Outline::Circle(pl.plate_radius_mm),
            holes,
        });
        let a = &lay.mr_arm;
        let x_motor = a.arm_length_mm - a.width_mm / 2.0;
        let mut holes = vec![([x_motor, 0.0], a.motor_shaft_dia_mm)];
        let q = a.motor_mount_pattern_mm / (2.0 * std::f64::consts::SQRT_2);
        for (sx, sy) in CORNERS {
            push_unique_hole(&mut holes, [x_motor + sx * q, sy * q], a.motor_bolt_dia_mm);
        }
        for k in 0..2 {
            push_unique_hole(
                &mut holes,
                [
                    a.chassis_hole_x0_mm + k as f64 * a.chassis_bolt_spacing_mm,
                    0.0,
                ],
                a.chassis_bolt_dia_mm,
            );
        }
        out.push(FlatPart {
            name: PartId::MrArm.title().to_string(),
            qty: part_quantity(PartId::MrArm, p),
            material: SheetMaterial::CarbonPlate,
            thickness_mm: a.thickness_mm,
            outline: Outline::Polygon(rect(
                a.arm_length_mm,
                a.width_mm,
                a.arm_length_mm / 2.0,
                0.0,
            )),
            holes,
        });
        let mm = &lay.mr_mount;
        out.push(motor_plate(MotorPlate {
            name: PartId::MrMotorMount.title().to_string(),
            qty: part_quantity(PartId::MrMotorMount, p),
            plate: mm.plate_size_mm,
            thickness: mm.thickness_mm,
            pattern: mm.motor_bolt_pattern_mm,
            bolt: mm.motor_bolt_dia_mm,
            shaft: mm.shaft_hole_dia_mm,
        }));
        return out;
    }
    // Kanat kaburgaları (sol/sağ çift): kök kaburgası + ara kaburgalar (kabuk et kalınlığı kadar içe çekilmiş).
    let wing = &lay.wing;
    let st = wing.structure();
    if st.is_hollow() {
        let g = wing.geom();
        let mut stations = vec![(0.0, st.root_rib_mm)];
        stations.extend(st.rib_stations_mm().into_iter().map(|z| (z, st.rib_t_mm)));
        for (z, thick) in stations {
            let c = g.chord_at(z);
            if !(c.is_finite() && c > 1.0) {
                continue;
            }
            let Some(pts) = g.profile.inner_polygon(
                c,
                st.wall_mm,
                crate::cad::structure::INNER_SEGMENTS_PER_SIDE,
            ) else {
                continue;
            };
            let f = wing.spar_x_frac;
            let local_th = (g.profile.yu(f) - g.profile.yl(f)) * c;
            let hole_c = [f * c, (g.profile.yu(f) + g.profile.yl(f)) / 2.0 * c];
            let mut holes = Vec::new();
            let spar_reaches = st
                .spar
                .map(|a| z <= a.start[2] + a.dir[2] * a.length + 1e-6)
                .unwrap_or(false);
            let min_wall = st.wall_mm.max(1.0);
            if wing.spar_od_mm > 0.0
                && spar_reaches
                && local_th >= wing.spar_od_mm + 2.0 * st.wall_mm + 2.0 * min_wall
                && circle_fits_in_polygon(&pts, hole_c, wing.spar_od_mm / 2.0 + min_wall)
            {
                holes.push((hole_c, wing.spar_od_mm));
            }
            out.push(FlatPart {
                name: tf("bom.part.rib_fmt", &[format!("{z:.0}")]),
                qty: 2,
                material: SheetMaterial::Plywood,
                thickness_mm: thick,
                outline: Outline::Polygon(pts),
                holes,
            });
        }
    }
    let mm = &lay.motor_mount;
    out.push(motor_plate(MotorPlate {
        name: PartId::MotorMount.title().to_string(),
        qty: part_quantity(PartId::MotorMount, p),
        plate: mm.plate_size_mm,
        thickness: mm.thickness_mm,
        pattern: mm.motor_bolt_pattern_mm,
        bolt: mm.motor_bolt_dia_mm,
        shaft: mm.shaft_hole_dia_mm,
    }));
    if p.drone_type == DroneType::VtolQuadplane {
        let lm = &lay.lift_mount;
        out.push(motor_plate(MotorPlate {
            name: PartId::LiftMotorMount.title().to_string(),
            qty: part_quantity(PartId::LiftMotorMount, p),
            plate: lm.plate_size_mm,
            thickness: lm.thickness_mm,
            pattern: lm.motor_bolt_pattern_mm,
            bolt: lm.motor_bolt_dia_mm,
            shaft: lm.shaft_hole_dia_mm,
        }));
    }
    out
}
