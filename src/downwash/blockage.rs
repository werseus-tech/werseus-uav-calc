// SPDX-License-Identifier: AGPL-3.0-or-later
//! Askıda rotor izinin altında kalan parçaların kapattığı alan ve aşağı yük (download).
//!
//! Model (Leishman 2006, §6 "rotor–airframe interaction / download"; helikopter gövde ve
//! tiltrotor kanadı aşağı yükü): her parça, rotor diskinden `z` kadar aşağıda, düzgün hızlı
//! eyleyici-disk izinin içinde düz bir engel gibi davranır:
//!
//!   D = C_D · ½ρ v(z)² · A_kapalı,  v(z) = v_i (1 + z/√(R² + z²)),
//!
//! iz yarıçapı r(z) süreklilikten daralır (`momentum::slipstream_radius_at`). Göbek bölgesi
//! (r < `hub_cutout`·r(z)) neredeyse akışsız kabul edilip dışarıda bırakılır (gerçek rotorlarda
//! kök bölgesi az itki üretir). Kapalı alan, iz dairesi kutupsal ızgarayla örneklenerek bulunur;
//! bir örnek noktası birden çok parçanın içindeyse yalnız listede ilk gelen parçaya yazılır
//! (üstteki parça alttakini gölgeler).
//!
//! Komşu rotor etkileşimi (uç açıklığı): uç açıklığı oranı δ = (s − D)/D. δ < `tip_warn` için
//! ampirik itki kaybı L₀(1 − δ/δ_warn)² (yan yana rotor deneylerinin büyüklük mertebesi, ör.
//! Yoon vd., NASA 2016; Shukla & Komerath 2018 — ayarlanabilir varsayılan). Diskler örtüşüyorsa
//! ek olarak Leishman (2006, §6 tandem rotor) etkin alan yaklaşımı: aynı güçte itki ∝ A^(1/3),
//! A_etkin = 2A − A_örtüşme ⇒ kayıp = 1 − ((2A − A_ö)/2A)^(1/3).

use super::momentum::{added_velocity_at, disk_area, lens_area, slipstream_radius_at};
use std::f64::consts::PI;

/// Üstten görünümde parça izdüşümü. Koordinatlar: x ileri +, y sağ + [m].
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// a'dan b'ye `w` genişliğinde şerit (kol, bom).
    Segment { a: [f64; 2], b: [f64; 2], w: f64 },
    /// Eksenlere paralel dikdörtgen.
    Rect { x0: f64, x1: f64, y0: f64, y1: f64 },
    /// Daire (merkez plaka).
    Circle { c: [f64; 2], r: f64 },
    /// Simetrik trapez kanat (iki yarı). `x_le_root`: kök hücum kenarı (x ileri +, yani burnun
    /// gerisi negatif); `y_min`: gövde yarı genişliği (gövde içindeki kısım kanat sayılmaz).
    Wing {
        x_le_root: f64,
        c_root: f64,
        taper: f64,
        half_span: f64,
        tan_sweep_le: f64,
        y_min: f64,
    },
}

impl Shape {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        match *self {
            Shape::Segment { a, b, w } => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l2 = dx * dx + dy * dy;
                if l2 <= 1e-18 || w <= 0.0 {
                    return false;
                }
                let t = ((x - a[0]) * dx + (y - a[1]) * dy) / l2;
                if !(0.0..=1.0).contains(&t) {
                    return false;
                }
                let (px, py) = (a[0] + t * dx - x, a[1] + t * dy - y);
                px * px + py * py <= 0.25 * w * w
            }
            Shape::Rect { x0, x1, y0, y1 } => {
                x >= x0.min(x1) && x <= x0.max(x1) && y >= y0.min(y1) && y <= y0.max(y1)
            }
            Shape::Circle { c, r } => {
                let (dx, dy) = (x - c[0], y - c[1]);
                dx * dx + dy * dy <= r * r
            }
            Shape::Wing {
                x_le_root,
                c_root,
                taper,
                half_span,
                tan_sweep_le,
                y_min,
            } => {
                let ay = y.abs();
                if half_span <= 0.0 || ay < y_min || ay > half_span {
                    return false;
                }
                let (x_le, c) = wing_station(x_le_root, c_root, taper, half_span, tan_sweep_le, ay);
                x <= x_le && x >= x_le - c
            }
        }
    }

    /// Sınır kutusu [x_min, x_max, y_min, y_max] (ön eleme için).
    pub fn bbox(&self) -> [f64; 4] {
        let mut b = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        for poly in self.outline() {
            for q in poly {
                b[0] = b[0].min(q[0]);
                b[1] = b[1].max(q[0]);
                b[2] = b[2].min(q[1]);
                b[3] = b[3].max(q[1]);
            }
        }
        if let Shape::Circle { c, r } = *self {
            b = [c[0] - r, c[0] + r, c[1] - r, c[1] + r];
        }
        b
    }

    /// Çizim için çokgen köşeleri (daire için 32 kenar). Kanat iki çokgen verir.
    pub fn outline(&self) -> Vec<Vec<[f64; 2]>> {
        match *self {
            Shape::Segment { a, b, w } => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let l = (dx * dx + dy * dy).sqrt();
                if l <= 1e-12 {
                    return Vec::new();
                }
                let (nx, ny) = (-dy / l * 0.5 * w, dx / l * 0.5 * w);
                vec![vec![
                    [a[0] + nx, a[1] + ny],
                    [b[0] + nx, b[1] + ny],
                    [b[0] - nx, b[1] - ny],
                    [a[0] - nx, a[1] - ny],
                ]]
            }
            Shape::Rect { x0, x1, y0, y1 } => vec![vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]],
            Shape::Circle { c, r } => vec![
                (0..32)
                    .map(|i| {
                        let a = 2.0 * PI * i as f64 / 32.0;
                        [c[0] + r * a.cos(), c[1] + r * a.sin()]
                    })
                    .collect(),
            ],
            Shape::Wing {
                x_le_root,
                c_root,
                taper,
                half_span,
                tan_sweep_le,
                y_min,
            } => {
                let st =
                    |y: f64| wing_station(x_le_root, c_root, taper, half_span, tan_sweep_le, y);
                let (x0, c0) = st(y_min);
                let (x1, c1) = st(half_span);
                [1.0, -1.0]
                    .iter()
                    .map(|s| {
                        vec![
                            [x0, s * y_min],
                            [x1, s * half_span],
                            [x1 - c1, s * half_span],
                            [x0 - c0, s * y_min],
                        ]
                    })
                    .collect()
            }
        }
    }
}

/// Kanat istasyonu: (hücum kenarı x'i [ileri +], kord) |y| konumunda.
pub fn wing_station(
    x_le_root: f64,
    c_root: f64,
    taper: f64,
    half_span: f64,
    tan_sweep_le: f64,
    ay: f64,
) -> (f64, f64) {
    let eta = if half_span > 0.0 {
        (ay / half_span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let c = c_root * (1.0 - (1.0 - taper) * eta);
    (x_le_root - ay * tan_sweep_le, c.max(0.0))
}

/// Parça türü (tablo gruplaması ve varsayılan C_D için).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Arm,
    Body,
    Component,
    Wing,
    Fuselage,
    Boom,
    Tail,
}

/// Rotor izinin altında kalabilecek bir parça.
#[derive(Clone, Debug)]
pub struct Item {
    /// Tablo satırı adı (aynı adlı parçalar tek satırda toplanır).
    pub name: String,
    pub kind: ItemKind,
    pub shape: Shape,
    /// Rotor düzleminin altındaki düşey uzaklık [m].
    pub z: f64,
    /// Akışa dik sürükleme katsayısı (ayarlanabilir varsayılan).
    pub cd: f64,
}

/// Askıdaki bir rotor.
#[derive(Clone, Copy, Debug)]
pub struct Rotor {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    /// Bu rotorun (koaksiyelde çiftin) askı itkisi [N].
    pub thrust: f64,
}

/// Bir rotorun bir parçaya uyguladığı aşağı yük.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hit {
    pub rotor: usize,
    pub item: usize,
    /// Kapalı alan [m²].
    pub area: f64,
    /// Aşağı yük [N].
    pub force: f64,
}

/// Kapalı alan hesabının sonucu.
#[derive(Clone, Debug, Default)]
pub struct BlockageOut {
    pub hits: Vec<Hit>,
    /// Çizim için örnek noktalar (kapalı bölgeler), seyreltilmiş.
    pub blocked_pts: Vec<[f64; 2]>,
    /// Toplam aşağı yük [N].
    pub total_force: f64,
    /// Toplam rotor itkisi [N].
    pub total_thrust: f64,
}

impl BlockageOut {
    /// Aşağı yükün toplam itkiye oranı (0..1).
    pub fn fraction(&self) -> f64 {
        if self.total_thrust > 0.0 && self.total_force.is_finite() {
            self.total_force / self.total_thrust
        } else {
            0.0
        }
    }
}

const N_RAD: usize = 24;
const N_AZ: usize = 720;

/// Tüm rotorlar × parçalar için kapalı alan ve aşağı yük.
pub fn blockage(rotors: &[Rotor], items: &[Item], rho: f64, hub_cutout: f64) -> BlockageOut {
    let mut out = BlockageOut::default();
    let hub = if hub_cutout.is_finite() {
        hub_cutout.clamp(0.0, 0.9)
    } else {
        0.15
    };
    let boxes: Vec<[f64; 4]> = items.iter().map(|it| it.shape.bbox()).collect();
    for (ri, rot) in rotors.iter().enumerate() {
        let a = disk_area(rot.r);
        if a <= 0.0 || !(rot.thrust.is_finite() && rot.thrust > 0.0) {
            continue;
        }
        out.total_thrust += rot.thrust;
        let vi = super::momentum::induced_velocity_hover(rot.thrust, rho, a);
        // Her parça kendi derinliğindeki iz dairesinde örneklenir.
        let mut areas = vec![0.0_f64; items.len()];
        // Rotor diski (iz her zaman disk içinde kalır) ile kesişen parçalar.
        let near = |b: &[f64; 4]| {
            b[0] <= rot.x + rot.r
                && b[1] >= rot.x - rot.r
                && b[2] <= rot.y + rot.r
                && b[3] >= rot.y - rot.r
        };
        let cand: Vec<usize> = (0..items.len()).filter(|&i| near(&boxes[i])).collect();
        for &ii in &cand {
            let it = &items[ii];
            let z = if it.z.is_finite() { it.z.max(0.0) } else { 0.0 };
            let rs = slipstream_radius_at(0.0, vi, rot.r, z);
            let r0 = hub * rs;
            let dr = (rs - r0) / N_RAD as f64;
            if dr <= 0.0 {
                continue;
            }
            for k in 0..N_RAD {
                let rm = r0 + (k as f64 + 0.5) * dr;
                let da = rm * dr * 2.0 * PI / N_AZ as f64;
                for j in 0..N_AZ {
                    let th = 2.0 * PI * (j as f64 + 0.5) / N_AZ as f64;
                    let (x, y) = (rot.x + rm * th.cos(), rot.y + rm * th.sin());
                    // Örnek bu parçaya mı ait? (Listede önce gelen ve aynı noktayı kapatan bir
                    // parça varsa gölgeler.)
                    if !it.shape.contains(x, y) {
                        continue;
                    }
                    if cand
                        .iter()
                        .take_while(|&&o| o < ii)
                        .any(|&o| items[o].shape.contains(x, y))
                    {
                        continue;
                    }
                    areas[ii] += da;
                    if k % 3 == 0 && j % 12 == 0 && out.blocked_pts.len() < 6000 {
                        out.blocked_pts.push([x, y]);
                    }
                }
            }
        }
        for (ii, it) in items.iter().enumerate() {
            if areas[ii] <= 0.0 {
                continue;
            }
            let z = if it.z.is_finite() { it.z.max(0.0) } else { 0.0 };
            let v = added_velocity_at(vi, rot.r, z);
            let cd = if it.cd.is_finite() {
                it.cd.max(0.0)
            } else {
                0.0
            };
            let f = cd * 0.5 * rho * v * v * areas[ii];
            out.total_force += f;
            out.hits.push(Hit {
                rotor: ri,
                item: ii,
                area: areas[ii],
                force: f,
            });
        }
    }
    out
}

/// İki rotorun etkileşimi.
#[derive(Clone, Copy, Debug)]
pub struct Pair {
    pub i: usize,
    pub j: usize,
    /// Uç açıklığı oranı δ = (s − D)/D (negatif: örtüşme).
    pub tip_ratio: f64,
    /// Her iki rotorun itki kaybı oranı (0..1).
    pub loss: f64,
}

impl Pair {
    pub fn overlap(&self) -> bool {
        self.tip_ratio < 0.0
    }
    pub fn warn(&self, tip_warn: f64) -> bool {
        self.tip_ratio < tip_warn
    }
}

/// Bir çiftin itki kaybı oranı (bkz. modül açıklaması).
pub fn pair_loss(r: f64, s: f64, tip_warn: f64, loss_touch: f64) -> f64 {
    if !(r.is_finite() && s.is_finite()) || r <= 0.0 {
        return 0.0;
    }
    let d = 2.0 * r;
    let delta = (s - d) / d;
    let warn = if tip_warn.is_finite() {
        tip_warn.max(1e-6)
    } else {
        0.1
    };
    let l0 = if loss_touch.is_finite() {
        loss_touch.clamp(0.0, 0.5)
    } else {
        0.03
    };
    if delta >= warn {
        0.0
    } else if delta >= 0.0 {
        l0 * (1.0 - delta / warn).powi(2)
    } else {
        let a = disk_area(r);
        let a_ov = lens_area(r, s.max(0.0));
        let eff = ((2.0 * a - a_ov) / (2.0 * a)).clamp(0.0, 1.0);
        l0 + (1.0 - eff.powf(1.0 / 3.0))
    }
}

/// Uç açıklığı oranı `report_below` altındaki tüm rotor çiftleri.
pub fn pairs(rotors: &[Rotor], tip_warn: f64, loss_touch: f64, report_below: f64) -> Vec<Pair> {
    let mut v = Vec::new();
    for i in 0..rotors.len() {
        for j in (i + 1)..rotors.len() {
            let (a, b) = (rotors[i], rotors[j]);
            let r = 0.5 * (a.r + b.r);
            if !(r.is_finite() && r > 0.0) {
                continue;
            }
            let s = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
            let tip_ratio = (s - 2.0 * r) / (2.0 * r);
            if tip_ratio.is_finite() && tip_ratio < report_below {
                v.push(Pair {
                    i,
                    j,
                    tip_ratio,
                    loss: pair_loss(r, s, tip_warn, loss_touch),
                });
            }
        }
    }
    v
}

/// Çift kayıplarından araç düzeyinde itki kaybı oranı (eşit itkili rotorlar): Σ 2·kayıp / n.
pub fn interaction_fraction(pairs: &[Pair], n_rotors: usize) -> f64 {
    if n_rotors == 0 {
        return 0.0;
    }
    let s: f64 = pairs.iter().map(|p| 2.0 * p.loss).sum();
    // Boş toplam −0.0 verir; ekranda "−0.0" görünmesin.
    if s > 0.0 {
        (s / n_rotors as f64).min(0.9)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm(w: f64) -> Item {
        Item {
            name: "arm".into(),
            kind: ItemKind::Arm,
            shape: Shape::Segment {
                a: [0.0, 0.0],
                b: [0.2, 0.0],
                w,
            },
            z: 0.03,
            cd: 1.2,
        }
    }

    fn rotor() -> Rotor {
        Rotor {
            x: 0.2,
            y: 0.0,
            r: 0.127,
            thrust: 8.0,
        }
    }

    #[test]
    fn zero_blockage_when_nothing_is_under_the_rotor() {
        let out = blockage(&[rotor()], &[], 1.225, 0.15);
        assert_eq!(out.total_force, 0.0);
        assert_eq!(out.fraction(), 0.0);
        let far = Item {
            shape: Shape::Rect {
                x0: 2.0,
                x1: 2.2,
                y0: 2.0,
                y1: 2.2,
            },
            ..arm(0.02)
        };
        let out = blockage(&[rotor()], &[far], 1.225, 0.15);
        assert_eq!(out.total_force, 0.0);
        assert!(out.hits.is_empty());
    }

    #[test]
    fn blockage_rises_with_arm_width() {
        let mut last = 0.0;
        for w in [0.01, 0.016, 0.024, 0.032] {
            let f = blockage(&[rotor()], &[arm(w)], 1.225, 0.15).fraction();
            assert!(f > last, "w={w}: {f} <= {last}");
            last = f;
        }
        // Elle kontrol: kapalı alan ≈ w·(r_s − r_hub); oran = C_D·½v²·A_b / (2v_i²·A).
        let rot = rotor();
        let out = blockage(&[rot], &[arm(0.024)], 1.225, 0.15);
        let a = disk_area(rot.r);
        let vi = super::super::momentum::induced_velocity_hover(rot.thrust, 1.225, a);
        let rs = slipstream_radius_at(0.0, vi, rot.r, 0.03);
        let ab = 0.024 * 0.85 * rs;
        assert!(
            (out.hits[0].area - ab).abs() / ab < 0.08,
            "{} vs {ab}",
            out.hits[0].area
        );
        let v = added_velocity_at(vi, rot.r, 0.03);
        let exp = 1.2 * 0.5 * v * v * ab / (2.0 * vi * vi * a);
        assert!((out.fraction() - exp).abs() / exp < 0.08);
    }

    #[test]
    fn upper_item_shadows_lower_item() {
        let plate = Item {
            name: "plate".into(),
            kind: ItemKind::Body,
            shape: Shape::Circle {
                c: [0.2, 0.0],
                r: 0.2,
            },
            z: 0.03,
            cd: 1.0,
        };
        let out = blockage(&[rotor()], &[plate, arm(0.024)], 1.225, 0.15);
        assert!(out.hits.iter().all(|h| h.item == 0));
    }

    #[test]
    fn tip_clearance_warning_threshold() {
        let r = 0.127;
        let rot = |x: f64| Rotor {
            x,
            y: 0.0,
            r,
            thrust: 5.0,
        };
        // δ = 0.12 > 0.10: uyarı yok, kayıp yok.
        let p = pairs(&[rot(0.0), rot(2.0 * r * 1.12)], 0.10, 0.03, 0.5);
        assert_eq!(p.len(), 1);
        assert!(!p[0].warn(0.10) && p[0].loss == 0.0);
        // δ = 0.05 < 0.10: uyarı, kayıp L0·(0.5)² = 0.0075.
        let p = pairs(&[rot(0.0), rot(2.0 * r * 1.05)], 0.10, 0.03, 0.5);
        assert!(p[0].warn(0.10) && !p[0].overlap());
        assert!((p[0].loss - 0.0075).abs() < 1e-9, "{}", p[0].loss);
        // Örtüşme: kayıp L0'dan büyük.
        let p = pairs(&[rot(0.0), rot(1.8 * r)], 0.10, 0.03, 0.5);
        assert!(p[0].overlap() && p[0].loss > 0.03);
        assert!((interaction_fraction(&p, 2) - p[0].loss).abs() < 1e-12);
        // Uzak çift raporlanmaz.
        assert!(pairs(&[rot(0.0), rot(10.0)], 0.10, 0.03, 0.5).is_empty());
    }

    #[test]
    fn shapes_contain_and_outline() {
        let w = Shape::Wing {
            x_le_root: -0.2,
            c_root: 0.25,
            taper: 0.6,
            half_span: 0.9,
            tan_sweep_le: 0.0,
            y_min: 0.05,
        };
        assert!(w.contains(-0.3, 0.3) && w.contains(-0.3, -0.3));
        assert!(!w.contains(-0.3, 0.01) && !w.contains(-0.3, 1.0) && !w.contains(0.0, 0.3));
        assert_eq!(w.outline().len(), 2);
        let c = Shape::Circle {
            c: [0.0, 0.0],
            r: 1.0,
        };
        assert!(c.contains(0.5, 0.5) && !c.contains(0.9, 0.9));
        assert_eq!(c.outline()[0].len(), 32);
    }
}
