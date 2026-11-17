// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canlı rüzgâr tüneli için uçağın 2D kesitleri ve kafes maskesine dökülmesi.
//!
//! Kesitler, 3D önizlemenin kullandığı aynı [`AircraftSpec`]'ten türetilir: kanat/kuyruk
//! yüzeyleri (gerçek profil, kord, ok, dihedral, burulma), gövde (üst-elips kesitli dönel
//! gövde, aynı yarıçap profili), kollar/bomlar/iniş takımı (silindir), motorlar ve plaka
//! (silindir), kutular. Her parça için kapalı biçimli bir "nokta içeride mi" testi yazılmıştır;
//! mesh dilimlemeye gerek kalmaz, açık mesh kenarları ve üst üste binen parçalar sorun olmaz.
//!
//! Pervaneler: dönen bir pervane katı bir duvar değildir; disk, ince bir GÖZENEKLİ bölge olarak
//! modellenir (hava geçer ama yavaşlar). Motor çanı katıdır.
//!
//! Kesit düzlemleri (görüntüde hava hep soldan sağa akar):
//! - `Section`: kanadın seçilen açıklık istasyonunda, açıklık eksenine dik kesit (profil);
//!   kanatsız araçta (multirotor) bir kolun eksenine dik kesit.
//! - `Side`: yandan (x–z) kesit; varsayılan gövde ekseni (y = 0). Burun solda.
//! - `Top`: üstten (x–y) kesit; varsayılan kanat (multirotorda kol) düzlemi. Burun solda.
//! - `Front`: önden (y–z) kesit; hava yandan gelir (yan rüzgâr).
//!
//! Hücum açısı α, cismi kesit düzleminde döndürür (yan/kesit: burun yukarı; üst: yana kayma;
//! ön: yan rüzgârın eğimi). Çerçeveleme (ölçek ve konum) α'dan bağımsızdır; böylece α
//! değiştirilirken cisim yalnız döner, büyüyüp küçülmez ya da kaymaz.
use super::lbm::{FLUID, POROUS, SOLID};
use crate::geom3d::Vec3;
use crate::geom3d::aircraft::{AircraftSpec, PartKind, SurfaceSpec, fuselage_profile};

/// Kesit türü.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CutKind {
    #[default]
    Section,
    Side,
    Top,
    Front,
}

impl CutKind {
    pub const ALL: [CutKind; 4] = [
        CutKind::Section,
        CutKind::Side,
        CutKind::Top,
        CutKind::Front,
    ];
}

/// İzin verilen hücum açısı aralığı [derece].
pub const ALPHA_MIN: f64 = -15.0;
pub const ALPHA_MAX: f64 = 20.0;
/// Çerçevelemede hesaba katılan en büyük dönüş (|α|).
const ALPHA_FRAME: f64 = 20.0;

/// Kesit seçimi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutParams {
    pub kind: CutKind,
    /// `Section`: açıklık oranı 0..1 (kanat) ya da kol boyu oranı (multirotor).
    /// Diğerleri: başvuru düzleminden kayma, yarı genişliğe oranla −1..1.
    pub station: f64,
    pub alpha_deg: f64,
}

impl Default for CutParams {
    fn default() -> Self {
        CutParams {
            kind: CutKind::Section,
            station: 0.35,
            alpha_deg: 4.0,
        }
    }
}

impl CutParams {
    /// Geçersiz/aralık dışı değerleri kırpar.
    pub fn sanitized(mut self) -> CutParams {
        if !self.station.is_finite() {
            self.station = 0.0;
        }
        self.station = match self.kind {
            CutKind::Section => self.station.clamp(0.0, 0.98),
            _ => self.station.clamp(-0.98, 0.98),
        };
        if !self.alpha_deg.is_finite() {
            self.alpha_deg = 0.0;
        }
        self.alpha_deg = self.alpha_deg.clamp(ALPHA_MIN, ALPHA_MAX);
        self
    }
}

type P3 = [f64; 3];

fn v3(v: Vec3) -> P3 {
    [v.x as f64, v.y as f64, v.z as f64]
}
fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: P3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: P3) -> Option<P3> {
    let l = norm(a);
    (l.is_finite() && l > 1e-12).then(|| mul(a, 1.0 / l))
}
fn finite3(a: P3) -> bool {
    a.iter().all(|v| v.is_finite())
}
fn pos(x: f32) -> bool {
    x.is_finite() && x > 0.0
}

/// Profil: x'e göre sıralı üst ve alt yüzey (kord = 1).
#[derive(Clone, Debug)]
struct Profile {
    upper: Vec<[f64; 2]>,
    lower: Vec<[f64; 2]>,
    t_max: f64,
}

fn interp(curve: &[[f64; 2]], x: f64) -> Option<f64> {
    let n = curve.len();
    if n == 0 || x < curve[0][0] || x > curve[n - 1][0] {
        return None;
    }
    let i = curve.partition_point(|p| p[0] < x);
    if i == 0 {
        return Some(curve[0][1]);
    }
    if i >= n {
        return Some(curve[n - 1][1]);
    }
    let (a, b) = (curve[i - 1], curve[i]);
    let dx = b[0] - a[0];
    Some(if dx > 1e-12 {
        a[1] + (b[1] - a[1]) * (x - a[0]) / dx
    } else {
        0.5 * (a[1] + b[1])
    })
}

impl Profile {
    fn new(pts: &[[f32; 2]]) -> Option<Profile> {
        let clean: Vec<[f64; 2]> = pts
            .iter()
            .filter(|q| q[0].is_finite() && q[1].is_finite())
            .map(|q| [q[0] as f64, q[1] as f64])
            .collect();
        if clean.len() < 3 {
            return None;
        }
        let le = clean
            .iter()
            .enumerate()
            .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .map(|(i, _)| i)?;
        let mut upper: Vec<[f64; 2]> = clean[..=le].iter().rev().copied().collect();
        let mut lower: Vec<[f64; 2]> = clean[le..].to_vec();
        upper.sort_by(|a, b| a[0].total_cmp(&b[0]));
        lower.sort_by(|a, b| a[0].total_cmp(&b[0]));
        if upper.len() < 2 || lower.len() < 2 {
            return None;
        }
        let t_max = clean.iter().fold(0.0f64, |m, p| m.max(p[1].abs()));
        Some(Profile {
            upper,
            lower,
            t_max,
        })
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        match (interp(&self.upper, x), interp(&self.lower, x)) {
            (Some(a), Some(b)) => y >= a.min(b) && y <= a.max(b),
            _ => false,
        }
    }

    /// Kord = 1 için profil alanı (çokgen alanı, shoelace).
    #[cfg(test)]
    fn area(&self) -> f64 {
        let mut ring: Vec<[f64; 2]> = self.upper.iter().rev().copied().collect();
        ring.extend(self.lower.iter().skip(1).copied());
        let n = ring.len();
        let mut s = 0.0;
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            s += a[0] * b[1] - b[0] * a[1];
        }
        0.5 * s.abs()
    }
}

#[derive(Clone, Debug)]
struct Surf {
    root: P3,
    semi: f64,
    c_root: f64,
    c_tip: f64,
    tan_sweep: f64,
    cg: f64,
    sg: f64,
    twist: f64,
    prof: Profile,
    mirrored: bool,
}

impl Surf {
    fn new(s: &SurfaceSpec) -> Option<Surf> {
        if !(pos(s.semi_span) && pos(s.c_root) && s.root_le.is_finite()) {
            return None;
        }
        let c_tip = if pos(s.c_tip) { s.c_tip } else { s.c_root };
        let fin = |v: f32, lim: f32| {
            if v.is_finite() {
                v.clamp(-lim, lim) as f64
            } else {
                0.0
            }
        };
        let sweep = fin(s.sweep_le_deg, 60.0).to_radians();
        let gamma = fin(s.dihedral_deg, 90.0).to_radians();
        let twist = fin(s.twist_tip_deg, 20.0).to_radians();
        let prof = Profile::new(&s.profile)
            .or_else(|| Profile::new(&crate::geom3d::aircraft::fallback_profile()))?;
        Some(Surf {
            root: v3(s.root_le),
            semi: s.semi_span as f64,
            c_root: s.c_root as f64,
            c_tip: c_tip as f64,
            tan_sweep: sweep.tan(),
            cg: gamma.cos(),
            sg: gamma.sin(),
            twist,
            prof,
            mirrored: s.mirrored,
        })
    }

    fn span_dir(&self) -> P3 {
        [0.0, self.cg, self.sg]
    }
    fn up_dir(&self) -> P3 {
        [0.0, -self.sg, self.cg]
    }
    fn chord_at(&self, s: f64) -> f64 {
        self.c_root + (self.c_tip - self.c_root) * (s / self.semi)
    }

    /// `surface_half` mesh üreticisinin tam tersi: noktayı yerel (açıklık, kord, kalınlık)
    /// koordinatlarına çevirir ve profilin içinde mi bakar.
    fn contains(&self, p: P3) -> bool {
        let p = if self.mirrored {
            [p[0], p[1].abs(), p[2]]
        } else {
            p
        };
        let d = sub(p, self.root);
        let s = d[1] * self.cg + d[2] * self.sg;
        if !(0.0..=self.semi).contains(&s) {
            return false;
        }
        let n = -d[1] * self.sg + d[2] * self.cg;
        let c = self.chord_at(s);
        if !(c > 0.0) {
            return false;
        }
        let a2 = -d[0] - s * self.tan_sweep - 0.25 * c;
        let (st, ct) = (self.twist * s / self.semi).sin_cos();
        let a = a2 * ct - n * st;
        let h = a2 * st + n * ct;
        self.prof.contains(a / c + 0.25, h / c)
    }

    fn aabb(&self) -> (P3, P3) {
        let mut pts = Vec::with_capacity(16);
        let up = self.up_dir();
        let span = self.span_dir();
        for s in [0.0, self.semi] {
            let c = self.chord_at(s);
            let le = add(
                add(self.root, mul(span, s)),
                [-s * self.tan_sweep, 0.0, 0.0],
            );
            let margin = c * (self.prof.t_max + self.twist.sin().abs() + 0.02);
            for x in [le, add(le, [-c, 0.0, 0.0])] {
                pts.push(add(x, mul(up, margin)));
                pts.push(add(x, mul(up, -margin)));
            }
        }
        if self.mirrored {
            let m: Vec<P3> = pts.iter().map(|p| [p[0], -p[1], p[2]]).collect();
            pts.extend(m);
        }
        bounds_of(&pts)
    }
}

fn bounds_of(pts: &[P3]) -> (P3, P3) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in pts {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

#[derive(Clone, Debug)]
enum Prim {
    Surface(Box<Surf>),
    /// Gövde: (x, r) yarıçap profili, eksen yüksekliği.
    Fuselage {
        prof: Vec<(f64, f64)>,
        z: f64,
    },
    /// Düz uçlu silindir (kol, bom, ayak, motor, plaka, tekerlek).
    Rod {
        a: P3,
        b: P3,
        r: f64,
    },
    Box {
        c: P3,
        h: P3,
    },
    /// Pervane diski (gözenekli): merkez, birim eksen, yarıçap, yarı kalınlık oranı.
    Rotor {
        c: P3,
        n: P3,
        r: f64,
    },
}

#[derive(Clone, Debug)]
struct Body {
    prim: Prim,
    kind: PartKind,
    lo: P3,
    hi: P3,
}

impl Body {
    fn new(prim: Prim, kind: PartKind) -> Option<Body> {
        let (lo, hi) = match &prim {
            Prim::Surface(s) => s.aabb(),
            Prim::Fuselage { prof, z } => {
                let r = prof.iter().fold(0.0f64, |m, q| m.max(q.1));
                let x0 = prof.first()?.0;
                let x1 = prof.last()?.0;
                ([x0.min(x1), -r, z - r], [x0.max(x1), r, z + r])
            }
            Prim::Rod { a, b, r } => {
                let (lo, hi) = bounds_of(&[*a, *b]);
                (sub(lo, [*r; 3]), add(hi, [*r; 3]))
            }
            Prim::Box { c, h } => (sub(*c, *h), add(*c, *h)),
            Prim::Rotor { c, r, .. } => (sub(*c, [*r; 3]), add(*c, [*r; 3])),
        };
        (finite3(lo) && finite3(hi)).then_some(Body { prim, kind, lo, hi })
    }

    /// 0 = dışarıda, `SOLID`, `POROUS`. `porous_half`: pervane diskinin en küçük yarı
    /// kalınlığı [m] (ince diskin ızgarada kaybolmaması için en az bir hücre).
    fn test(&self, p: P3, porous_half: f64) -> u8 {
        for k in 0..3 {
            if p[k] < self.lo[k] - porous_half || p[k] > self.hi[k] + porous_half {
                return FLUID;
            }
        }
        let hit = match &self.prim {
            Prim::Surface(s) => s.contains(p),
            Prim::Fuselage { prof, z } => {
                let n = prof.len();
                if n < 2 || p[0] < prof[0].0 || p[0] > prof[n - 1].0 {
                    false
                } else {
                    let i = prof.partition_point(|q| q.0 < p[0]).clamp(1, n - 1);
                    let ((xa, ra), (xb, rb)) = (prof[i - 1], prof[i]);
                    let r = if xb > xa {
                        ra + (rb - ra) * (p[0] - xa) / (xb - xa)
                    } else {
                        ra.max(rb)
                    }
                    .max(1e-4);
                    // Mesh ile aynı üst-elips kesit: |y/r|^2.6 + |Δz/(0.92 r)|^2.6 ≤ 1.
                    let e = 2.6;
                    (p[1] / r).abs().powf(e) + ((p[2] - z) / (0.92 * r)).abs().powf(e) <= 1.0
                }
            }
            Prim::Rod { a, b, r } => {
                let ab = sub(*b, *a);
                let l2 = dot(ab, ab);
                if l2 < 1e-18 {
                    false
                } else {
                    let t = dot(sub(p, *a), ab) / l2;
                    (0.0..=1.0).contains(&t) && {
                        let q = add(*a, mul(ab, t));
                        let d = sub(p, q);
                        dot(d, d) <= r * r
                    }
                }
            }
            Prim::Box { c, h } => (0..3).all(|k| (p[k] - c[k]).abs() <= h[k]),
            Prim::Rotor { c, n, r } => {
                let v = sub(p, *c);
                let ax = dot(v, *n);
                let rad2 = dot(v, v) - ax * ax;
                let half = (0.04 * r).max(porous_half);
                return if ax.abs() <= half && rad2 <= r * r {
                    POROUS
                } else {
                    FLUID
                };
            }
        };
        if hit { SOLID } else { FLUID }
    }
}

/// `AircraftSpec` → katı/gözenekli parça listesi. Geçersiz parçalar sessizce atlanır.
fn bodies_of(spec: &AircraftSpec) -> Vec<Body> {
    let mut out = Vec::new();
    for (kind, s) in &spec.surfaces {
        if let Some(sf) = Surf::new(s) {
            out.extend(Body::new(Prim::Surface(Box::new(sf)), *kind));
        }
    }
    if let Some(f) = &spec.fuselage {
        if pos(f.length) && pos(f.diameter) && f.x_nose.is_finite() {
            let prof: Vec<(f64, f64)> = fuselage_profile(f, 36)
                .into_iter()
                .map(|(x, r)| (x as f64, r as f64))
                .collect();
            let z = if f.z_axis.is_finite() {
                f.z_axis as f64
            } else {
                0.0
            };
            out.extend(Body::new(Prim::Fuselage { prof, z }, PartKind::Body));
        }
    }
    for (kind, r) in &spec.rods {
        if pos(r.radius) && r.a.is_finite() && r.b.is_finite() {
            out.extend(Body::new(
                Prim::Rod {
                    a: v3(r.a),
                    b: v3(r.b),
                    r: r.radius as f64,
                },
                *kind,
            ));
        }
    }
    if let Some((c, r, t)) = spec.plate {
        if c.is_finite() && pos(r) && pos(t) {
            let c = v3(c);
            out.extend(Body::new(
                Prim::Rod {
                    a: sub(c, [0.0, 0.0, t as f64 * 0.5]),
                    b: add(c, [0.0, 0.0, t as f64 * 0.5]),
                    r: r as f64,
                },
                PartKind::Body,
            ));
        }
    }
    for (c, size) in &spec.boxes {
        if c.is_finite() && size.is_finite() && size.x > 0.0 && size.y > 0.0 && size.z > 0.0 {
            out.extend(Body::new(
                Prim::Box {
                    c: v3(*c),
                    h: mul(v3(*size), 0.5),
                },
                PartKind::Body,
            ));
        }
    }
    for r in &spec.rotors {
        let Some(axis) = unit(v3(r.axis)) else {
            continue;
        };
        if !r.center.is_finite() {
            continue;
        }
        let c = v3(r.center);
        if pos(r.motor_radius) && pos(r.motor_height) {
            out.extend(Body::new(
                Prim::Rod {
                    a: sub(c, mul(axis, r.motor_height as f64)),
                    b: c,
                    r: r.motor_radius as f64,
                },
                PartKind::Motor,
            ));
        }
        if pos(r.radius) {
            out.extend(Body::new(
                Prim::Rotor {
                    c,
                    n: axis,
                    r: r.radius as f64,
                },
                PartKind::Rotor,
            ));
        }
    }
    for w in &spec.wheels {
        if w.center.is_finite() && pos(w.radius) && pos(w.width) {
            let c = v3(w.center);
            let hw = w.width as f64 * 0.5;
            out.extend(Body::new(
                Prim::Rod {
                    a: sub(c, [0.0, hw, 0.0]),
                    b: add(c, [0.0, hw, 0.0]),
                    r: w.radius as f64,
                },
                PartKind::Wheel,
            ));
        }
    }
    out
}

/// Kesit düzlemi: P(a, b) = o + a·ea + b·eb (ea akış yönü, eb görüntüde yukarı; birim, dik).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Plane {
    o: P3,
    ea: P3,
    eb: P3,
}

impl Plane {
    fn point(&self, a: f64, b: f64) -> P3 {
        add(self.o, add(mul(self.ea, a), mul(self.eb, b)))
    }
    fn normal(&self) -> P3 {
        cross(self.ea, self.eb)
    }
}

/// Kanatsız araçta `Section` bir kolun kesitine dönüşür; UI bunu bildirir.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionOf {
    Wing,
    Arm,
    /// Ne kanat ne kol var: yandan kesite düşülür.
    Fallback,
}

/// Bir kesitin geometrisi (çerçevelemeden bağımsız).
#[derive(Clone, Debug)]
pub struct CutGeometry {
    bodies: Vec<Body>,
    plane: Plane,
    /// Bölüm kesitinde dönme merkezi (çeyrek kord); diğerlerinde `None` (kesitin ortası).
    pivot: Option<[f64; 2]>,
    pub section_of: SectionOf,
    /// Düzlemin başvuruya göre konumu [m] (bilgi amaçlı).
    pub offset_m: f64,
}

fn main_wing(spec: &AircraftSpec) -> Option<&SurfaceSpec> {
    spec.surfaces
        .iter()
        .find(|(k, s)| *k == PartKind::Wing && pos(s.semi_span) && pos(s.c_root))
        .map(|(_, s)| s)
}

fn overall_bounds(bodies: &[Body]) -> Option<(P3, P3)> {
    if bodies.is_empty() {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for b in bodies {
        for k in 0..3 {
            lo[k] = lo[k].min(b.lo[k]);
            hi[k] = hi[k].max(b.hi[k]);
        }
    }
    Some((lo, hi))
}

/// Kesit geometrisini kurar. Hiçbir koşulda panik yapmaz; parça yoksa boş kesit döner.
pub fn cut_geometry(spec: &AircraftSpec, cut: &CutParams) -> CutGeometry {
    let cut = cut.sanitized();
    let bodies = bodies_of(spec);
    let (lo, hi) = overall_bounds(&bodies).unwrap_or(([-0.5; 3], [0.5; 3]));
    let side_plane = |y0: f64| Plane {
        o: [0.0, y0, 0.0],
        ea: [-1.0, 0.0, 0.0],
        eb: [0.0, 0.0, 1.0],
    };
    let half = |r: f64, a: f64, b: f64| (a - r).abs().max((b - r).abs()).max(1e-3);
    let wing = main_wing(spec).and_then(Surf::new);
    let mut geom = CutGeometry {
        bodies: Vec::new(),
        plane: side_plane(0.0),
        pivot: None,
        section_of: SectionOf::Wing,
        offset_m: 0.0,
    };
    match cut.kind {
        CutKind::Section => {
            if let Some(w) = &wing {
                let s0 = cut.station * w.semi;
                let o = add(w.root, mul(w.span_dir(), s0));
                geom.plane = Plane {
                    o,
                    ea: [-1.0, 0.0, 0.0],
                    eb: w.up_dir(),
                };
                geom.pivot = Some([s0 * w.tan_sweep + 0.25 * w.chord_at(s0), 0.0]);
                geom.offset_m = s0;
                geom.bodies = Body::new(Prim::Surface(Box::new(w.clone())), PartKind::Wing)
                    .into_iter()
                    .collect();
                return geom;
            }
            let arm = bodies.iter().find_map(|b| match (&b.prim, b.kind) {
                (Prim::Rod { a, b: bb, .. }, PartKind::Arm | PartKind::Boom) => Some((*a, *bb)),
                _ => None,
            });
            if let Some((a, b)) = arm {
                if let Some(d) = unit(sub(b, a)) {
                    let t = cut.station.clamp(0.05, 0.95);
                    let o = add(a, mul(sub(b, a), t));
                    let z = [0.0, 0.0, 1.0];
                    let eb = unit(sub(z, mul(d, dot(z, d))))
                        .or_else(|| unit(cross(d, [1.0, 0.0, 0.0])))
                        .unwrap_or(z);
                    let mut ea = cross(eb, d);
                    if dot(ea, [-1.0, 0.0, 0.0]) < 0.0 {
                        ea = mul(ea, -1.0);
                    }
                    geom.plane = Plane { o, ea, eb };
                    geom.section_of = SectionOf::Arm;
                    geom.offset_m = t * norm(sub(b, a));
                    // Yalnız kollar/bomlar ve motorlar: kol dairesi çerçeveyi doldurur (pervane
                    // diski kesite girip ölçeği küçültmesin).
                    geom.bodies = bodies
                        .into_iter()
                        .filter(|b| {
                            matches!(b.kind, PartKind::Arm | PartKind::Boom | PartKind::Motor)
                        })
                        .collect();
                    return geom;
                }
            }
            geom.section_of = SectionOf::Fallback;
            geom.bodies = bodies;
        }
        CutKind::Side => {
            let y0 = cut.station * half(0.0, lo[1], hi[1]);
            geom.plane = side_plane(y0);
            geom.offset_m = y0;
            geom.bodies = bodies;
        }
        CutKind::Top => {
            // Başvuru: kanat kökünün orta kalınlığı; kanatsızsa kol ekseni; yoksa kutu ortası.
            let z_ref = if let Some(w) = &wing {
                let mid = 0.5
                    * (interp(&w.prof.upper, 0.3).unwrap_or(0.0)
                        + interp(&w.prof.lower, 0.3).unwrap_or(0.0));
                w.root[2] + mid * w.c_root
            } else if let Some(z) = bodies.iter().find_map(|b| match (&b.prim, b.kind) {
                (Prim::Rod { a, b: bb, .. }, PartKind::Arm) => Some(0.5 * (a[2] + bb[2])),
                _ => None,
            }) {
                z
            } else {
                0.5 * (lo[2] + hi[2])
            };
            let z0 = z_ref + cut.station * half(z_ref, lo[2], hi[2]);
            geom.plane = Plane {
                o: [0.0, 0.0, z0],
                ea: [-1.0, 0.0, 0.0],
                eb: [0.0, -1.0, 0.0],
            };
            geom.offset_m = z0 - z_ref;
            geom.bodies = bodies;
        }
        CutKind::Front => {
            let x_ref = match &wing {
                Some(w) => w.root[0] - 0.3 * w.c_root,
                None => 0.5 * (lo[0] + hi[0]),
            };
            let x0 = x_ref + cut.station * half(x_ref, lo[0], hi[0]);
            geom.plane = Plane {
                o: [x0, 0.0, 0.0],
                ea: [0.0, 1.0, 0.0],
                eb: [0.0, 0.0, 1.0],
            };
            geom.offset_m = x0 - x_ref;
            geom.bodies = bodies;
        }
    }
    geom
}

/// Kafes maskesi ve çerçeveleme.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub nx: usize,
    pub ny: usize,
    pub cells: Vec<u8>,
    /// Hücre boyu [m].
    pub dx: f64,
    /// Akış yönündeki cisim boyu [m] (Re için başvuru uzunluğu).
    pub l_ref: f64,
    /// Kesitte cisim yok (düzlem hiçbir parçaya değmiyor).
    pub empty: bool,
    pub solid_cells: usize,
    pub porous_cells: usize,
}

/// Çerçeveleme: α'dan bağımsız ölçek ve dönme merkezinin kafesteki konumu.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    dx: f64,
    pivot: [f64; 2],
    pivot_lat: [f64; 2],
    l_ref: f64,
}

/// Düzlemi kesen parçaların düzlemdeki (a, b) sınır dikdörtgenleri.
fn plane_rects(geom: &CutGeometry, margin: f64) -> Vec<(usize, [f64; 4])> {
    let n = geom.plane.normal();
    let mut out = Vec::new();
    for (i, b) in geom.bodies.iter().enumerate() {
        let mut nmin = f64::INFINITY;
        let mut nmax = f64::NEG_INFINITY;
        let mut r = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        for c in 0..8 {
            let p = [
                if c & 1 == 0 { b.lo[0] } else { b.hi[0] },
                if c & 2 == 0 { b.lo[1] } else { b.hi[1] },
                if c & 4 == 0 { b.lo[2] } else { b.hi[2] },
            ];
            let d = sub(p, geom.plane.o);
            let dn = dot(d, n);
            nmin = nmin.min(dn);
            nmax = nmax.max(dn);
            let (a, bb) = (dot(d, geom.plane.ea), dot(d, geom.plane.eb));
            r[0] = r[0].min(a);
            r[1] = r[1].max(a);
            r[2] = r[2].min(bb);
            r[3] = r[3].max(bb);
        }
        if nmin <= margin && nmax >= -margin {
            out.push((
                i,
                [r[0] - margin, r[1] + margin, r[2] - margin, r[3] + margin],
            ));
        }
    }
    out
}

fn test_at(geom: &CutGeometry, rects: &[(usize, [f64; 4])], a: f64, b: f64, ph: f64) -> u8 {
    let p = geom.plane.point(a, b);
    let mut res = FLUID;
    for (i, r) in rects {
        if a < r[0] || a > r[1] || b < r[2] || b > r[3] {
            continue;
        }
        match geom.bodies[*i].test(p, ph) {
            SOLID => return SOLID,
            POROUS => res = POROUS,
            _ => {}
        }
    }
    res
}

/// Kesitin gerçek (dönmemiş) sınırını kaba bir taramayla bulur: (a0, a1, b0, b1).
fn scan_extent(geom: &CutGeometry, rects: &[(usize, [f64; 4])]) -> Option<[f64; 4]> {
    let mut u = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for (_, r) in rects {
        u[0] = u[0].min(r[0]);
        u[1] = u[1].max(r[1]);
        u[2] = u[2].min(r[2]);
        u[3] = u[3].max(r[3]);
    }
    if !u.iter().all(|v| v.is_finite()) || u[1] <= u[0] || u[3] <= u[2] {
        return None;
    }
    let (wa, wb) = (u[1] - u[0], u[3] - u[2]);
    let step = (wa.max(wb) / 480.0).max(1e-6);
    let na = ((wa / step).ceil() as usize).clamp(2, 960);
    let nb = ((wb / step).ceil() as usize).clamp(2, 960);
    let mut e = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for j in 0..nb {
        let b = u[2] + (j as f64 + 0.5) * wb / nb as f64;
        for i in 0..na {
            let a = u[0] + (i as f64 + 0.5) * wa / na as f64;
            if test_at(geom, rects, a, b, step) != FLUID {
                e[0] = e[0].min(a);
                e[1] = e[1].max(a);
                e[2] = e[2].min(b);
                e[3] = e[3].max(b);
            }
        }
    }
    if !e.iter().all(|v| v.is_finite()) {
        return None;
    }
    // Örnek aralığı kadar pay (tarama noktaları hücre ortalarıdır).
    let (ha, hb) = (0.5 * wa / na as f64, 0.5 * wb / nb as f64);
    Some([e[0] - ha, e[1] + ha, e[2] - hb, e[3] + hb])
}

fn frame_for(geom: &CutGeometry, ext: [f64; 4], nx: usize, ny: usize) -> Frame {
    let la = (ext[1] - ext[0]).max(1e-4);
    let lb = (ext[3] - ext[2]).max(1e-4);
    let (s, c) = ALPHA_FRAME.to_radians().sin_cos();
    let la_eff = la * c + lb * s;
    let lb_eff = la * s + lb * c;
    // Cisim kanal uzunluğunun en çok ~%32'sini, yüksekliğinin ~%42'sini kaplar.
    let dx = (la_eff / (0.32 * nx as f64)).max(lb_eff / (0.42 * ny as f64));
    let pivot = geom
        .pivot
        .unwrap_or([0.5 * (ext[0] + ext[1]), 0.5 * (ext[2] + ext[3])]);
    let lead = 0.22 * nx as f64 + (la_eff - la) * 0.5 / dx;
    let pivot_lat = [
        lead + (pivot[0] - ext[0]) / dx,
        0.5 * ny as f64 + (pivot[1] - 0.5 * (ext[2] + ext[3])) / dx,
    ];
    Frame {
        dx,
        pivot,
        pivot_lat,
        l_ref: la,
    }
}

/// Kesiti `nx × ny` kafese döker. α derece cinsinden. Hiçbir koşulda panik yapmaz.
pub fn rasterize(geom: &CutGeometry, alpha_deg: f64, nx: usize, ny: usize) -> Mask {
    let nx = nx.clamp(8, 4096);
    let ny = ny.clamp(8, 4096);
    let alpha = if alpha_deg.is_finite() {
        alpha_deg.clamp(ALPHA_MIN, ALPHA_MAX).to_radians()
    } else {
        0.0
    };
    let mut cells = vec![FLUID; nx * ny];
    let rects0 = plane_rects(geom, 0.0);
    let Some(ext) = scan_extent(geom, &rects0) else {
        return Mask {
            nx,
            ny,
            cells,
            dx: 1.0 / nx as f64,
            l_ref: 1.0,
            empty: true,
            solid_cells: 0,
            porous_cells: 0,
        };
    };
    let fr = frame_for(geom, ext, nx, ny);
    let dx = fr.dx;
    // Gözenekli diskin en küçük yarı kalınlığı: bir hücre.
    let ph = dx;
    let rects = plane_rects(geom, ph);
    let (sa, ca) = alpha.sin_cos();
    // Hücre başına 3×3 alt örnek; çoğunluk (≥ 5/9) içerideyse hücre o türdendir.
    const SUB: [f64; 3] = [1.0 / 6.0, 0.5, 5.0 / 6.0];
    let (mut ns, mut np) = (0usize, 0usize);
    for j in 0..ny {
        for i in 0..nx {
            let (mut s_hits, mut p_hits) = (0, 0);
            for oy in SUB {
                for ox in SUB {
                    let wa = (i as f64 + ox - fr.pivot_lat[0]) * dx;
                    let wb = (j as f64 + oy - fr.pivot_lat[1]) * dx;
                    // Görüntü → cisim: obj = R(α)·world (cisim R(−α) ile döndürülmüştür;
                    // yan kesitte burun yukarı = pozitif α).
                    let a = fr.pivot[0] + wa * ca - wb * sa;
                    let b = fr.pivot[1] + wa * sa + wb * ca;
                    match test_at(geom, &rects, a, b, ph) {
                        SOLID => s_hits += 1,
                        POROUS => p_hits += 1,
                        _ => {}
                    }
                }
            }
            let k = j * nx + i;
            if s_hits >= 5 {
                cells[k] = SOLID;
                ns += 1;
            } else if s_hits + p_hits >= 5 {
                cells[k] = POROUS;
                np += 1;
            }
        }
    }
    // Giriş/çıkış sütunları ve alt/üst satırlar her zaman akışkan (sınır koşulları için).
    for j in 0..ny {
        for i in [0, 1, nx - 2, nx - 1] {
            clear(&mut cells, j * nx + i, &mut ns, &mut np);
        }
    }
    for i in 0..nx {
        for j in [0, ny - 1] {
            clear(&mut cells, j * nx + i, &mut ns, &mut np);
        }
    }
    Mask {
        nx,
        ny,
        cells,
        dx,
        l_ref: fr.l_ref,
        empty: ns + np == 0,
        solid_cells: ns,
        porous_cells: np,
    }
}

fn clear(cells: &mut [u8], k: usize, ns: &mut usize, np: &mut usize) {
    match cells[k] {
        SOLID => *ns -= 1,
        POROUS => *np -= 1,
        _ => {}
    }
    cells[k] = FLUID;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom3d::aircraft::{FuselageSpec, naca4_profile};

    fn wing_only(code: &str, sweep: f32, mirrored: bool) -> AircraftSpec {
        AircraftSpec {
            surfaces: vec![(
                PartKind::Wing,
                SurfaceSpec {
                    root_le: Vec3::ZERO,
                    semi_span: 0.8,
                    c_root: 0.25,
                    c_tip: 0.25,
                    sweep_le_deg: sweep,
                    dihedral_deg: 0.0,
                    twist_tip_deg: 0.0,
                    structure: None,
                    profile: naca4_profile(code).expect("naca"),
                    mirrored,
                },
            )],
            ..Default::default()
        }
    }

    /// Profil maskesinin alanı, profilin analitik (çokgen) alanına yakın.
    #[test]
    fn airfoil_mask_area_matches_analytic_area() {
        for code in ["0012", "2412", "4415"] {
            let spec = wing_only(code, 0.0, true);
            let geom = cut_geometry(
                &spec,
                &CutParams {
                    kind: CutKind::Section,
                    station: 0.5,
                    alpha_deg: 0.0,
                },
            );
            let m = rasterize(&geom, 0.0, 480, 240);
            assert!(!m.empty);
            let prof = Profile::new(&naca4_profile(code).unwrap()).unwrap();
            let exact = prof.area() * 0.25 * 0.25;
            let got = m.solid_cells as f64 * m.dx * m.dx;
            let rel = (got - exact) / exact;
            println!(
                "{code}: mask {got:.6} m² / analitik {exact:.6} m² ({:+.2} %)",
                rel * 100.0
            );
            assert!(rel.abs() < 0.04, "{code}: {rel}");
            // Başvuru uzunluğu kord olmalı.
            assert!((m.l_ref - 0.25).abs() < 0.01, "l_ref {}", m.l_ref);
        }
    }

    /// Simetrik profil α = 0'da yatay eksene göre simetrik maske verir; α döndürür.
    #[test]
    fn symmetric_shape_gives_symmetric_mask() {
        let spec = wing_only("0012", 0.0, true);
        let cut = CutParams {
            kind: CutKind::Section,
            station: 0.3,
            alpha_deg: 0.0,
        };
        let geom = cut_geometry(&spec, &cut);
        let (nx, ny) = (360, 180);
        let m = rasterize(&geom, 0.0, nx, ny);
        let mut mismatch = 0;
        for j in 0..ny {
            for i in 0..nx {
                if m.cells[j * nx + i] != m.cells[(ny - 1 - j) * nx + i] {
                    mismatch += 1;
                }
            }
        }
        assert!(
            mismatch * 200 <= m.solid_cells,
            "simetri bozuk: {mismatch} / {}",
            m.solid_cells
        );
        // Pozitif α'da hücum kenarı (sol) yukarı kalkar: sol yarıdaki katı hücrelerin
        // ortalama yüksekliği sağ yarıdakinden büyük.
        let m2 = rasterize(&geom, 10.0, nx, ny);
        let (mut lsum, mut ln, mut rsum, mut rn) = (0.0, 0.0, 0.0, 0.0);
        let cols: Vec<usize> = (0..nx)
            .filter(|i| (0..ny).any(|j| m2.cells[j * nx + i] == SOLID))
            .collect();
        let mid = (cols[0] + cols[cols.len() - 1]) / 2;
        for j in 0..ny {
            for i in 0..nx {
                if m2.cells[j * nx + i] == SOLID {
                    if i < mid {
                        lsum += j as f64;
                        ln += 1.0;
                    } else {
                        rsum += j as f64;
                        rn += 1.0;
                    }
                }
            }
        }
        assert!(lsum / ln > rsum / rn + 1.0, "α burun yukarı döndürmedi");
        // Aynı alan (dönme alanı korur), küçük kafes hatası payıyla.
        let r = m2.solid_cells as f64 / m.solid_cells as f64;
        assert!((r - 1.0).abs() < 0.05, "{r}");
    }

    /// Gövdenin yan kesiti y = 0'a göre yukarı-aşağı simetrik (z_axis = 0), ön kesiti de
    /// sol-sağ simetrik.
    #[test]
    fn fuselage_cuts_are_symmetric() {
        let spec = AircraftSpec {
            fuselage: Some(FuselageSpec {
                x_nose: 0.3,
                length: 1.0,
                diameter: 0.12,
                z_axis: 0.0,
                structure: None,
            }),
            ..Default::default()
        };
        for kind in [CutKind::Side, CutKind::Top] {
            let g = cut_geometry(
                &spec,
                &CutParams {
                    kind,
                    station: 0.0,
                    alpha_deg: 0.0,
                },
            );
            let (nx, ny) = (240, 120);
            let m = rasterize(&g, 0.0, nx, ny);
            assert!(m.solid_cells > 100, "{kind:?}");
            let mut mismatch = 0;
            for j in 0..ny {
                for i in 0..nx {
                    if m.cells[j * nx + i] != m.cells[(ny - 1 - j) * nx + i] {
                        mismatch += 1;
                    }
                }
            }
            assert!(mismatch * 100 <= m.solid_cells, "{kind:?}: {mismatch}");
        }
    }

    /// Bozuk/boş tasarımlarda panik yok; boş kesit `empty` olarak işaretlenir.
    #[test]
    fn degenerate_designs_do_not_panic() {
        let mut specs = vec![AircraftSpec::default()];
        let mut s = wing_only("0012", 0.0, true);
        s.surfaces[0].1.semi_span = 0.0;
        specs.push(s);
        let mut s = wing_only("0012", f32::NAN, true);
        s.surfaces[0].1.c_root = f32::NAN;
        specs.push(s);
        let mut s = wing_only("0012", 0.0, true);
        s.surfaces[0].1.profile.clear();
        specs.push(s);
        specs.push(AircraftSpec {
            fuselage: Some(FuselageSpec {
                x_nose: 0.0,
                length: 0.0,
                diameter: 0.1,
                z_axis: f32::INFINITY,
                structure: None,
            }),
            ..Default::default()
        });
        for spec in &specs {
            for kind in CutKind::ALL {
                for st in [-5.0, 0.0, 0.5, 1.0, f64::NAN] {
                    let g = cut_geometry(
                        spec,
                        &CutParams {
                            kind,
                            station: st,
                            alpha_deg: f64::NAN,
                        },
                    );
                    let m = rasterize(&g, 99.0, 64, 32);
                    assert_eq!(m.cells.len(), 64 * 32);
                    assert!(m.dx.is_finite() && m.dx > 0.0);
                }
            }
        }
        // Boş profil yedek profile düşer (kesit boş değildir).
        let g = cut_geometry(&specs[3], &CutParams::default());
        assert!(!rasterize(&g, 0.0, 64, 32).empty);
        // Tamamen boş spec → boş maske.
        let g = cut_geometry(&specs[0], &CutParams::default());
        assert!(rasterize(&g, 0.0, 64, 32).empty);
    }

    /// Gerçek tasarımlar: her araç türünde her kesit bir şey gösterir; multirotorda bölüm
    /// kesiti kol kesitidir ve üstten kesitte pervane diskleri gözenekli görünür.
    #[test]
    fn every_vehicle_type_has_non_empty_cuts() {
        use crate::geom3d::{ViewOptions, from_project};
        use crate::project::{DroneType, Project};
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
            let spec = from_project(&p, &m, &ViewOptions::default());
            for kind in CutKind::ALL {
                let station = if kind == CutKind::Section { 0.4 } else { 0.0 };
                let g = cut_geometry(
                    &spec,
                    &CutParams {
                        kind,
                        station,
                        alpha_deg: 5.0,
                    },
                );
                let mk = rasterize(&g, 5.0, 240, 120);
                println!(
                    "{dt:?} {kind:?}: katı {} gözenekli {} dx {:.4} m",
                    mk.solid_cells, mk.porous_cells, mk.dx
                );
                assert!(!mk.empty, "{dt:?} {kind:?} boş");
                if kind == CutKind::Section {
                    let expect = if dt == DroneType::Multirotor {
                        SectionOf::Arm
                    } else {
                        SectionOf::Wing
                    };
                    assert_eq!(g.section_of, expect, "{dt:?}");
                }
            }
        }
        // Multirotor: pervane diski düzleminden geçen üst kesitte gözenekli hücreler.
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let spec = from_project(&p, &m, &ViewOptions::default());
        let z_rotor = spec.rotors[0].center.z as f64;
        let hit = (-49..=49).any(|k| {
            let g = cut_geometry(
                &spec,
                &CutParams {
                    kind: CutKind::Top,
                    station: k as f64 / 50.0,
                    alpha_deg: 0.0,
                },
            );
            let near = (g.plane.o[2] - z_rotor).abs() < 0.01;
            near && rasterize(&g, 0.0, 240, 120).porous_cells > 50
        });
        assert!(hit, "pervane diski üst kesitte görünmedi");
    }

    #[test]
    fn rasterize_is_deterministic() {
        let spec = wing_only("2412", 20.0, true);
        let g = cut_geometry(&spec, &CutParams::default());
        assert_eq!(rasterize(&g, 7.0, 200, 100), rasterize(&g, 7.0, 200, 100));
    }
}
