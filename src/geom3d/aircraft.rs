// SPDX-License-Identifier: AGPL-3.0-or-later
//! Uçak parametrelerinden saf 3D sahne üretimi.
//!
//! Koordinatlar (metre): X ileri (burun), Y sol, Z yukarı (sağ el kuralı). Orijin kanat kökü
//! hücum kenarı; bu, 2D şemayla (`ui::schematic`) aynı başvuru noktasıdır, böylece iki görünüm
//! aynı ölçüleri gösterir. Bu dosya `Project`/`Model` okumaz: girdi [`AircraftSpec`]'tir,
//! dönüşüm `spec.rs` içindedir. Böylece mesh üretimi bağımsız test edilir.
use super::math::Vec3;
use super::mesh::{Mesh, push_box, push_cylinder, push_disc, push_lathe_x};

/// Taşıyıcı yüzey (kanat, yatay/V kuyruk, dikey kuyruk, winglet) — tek yarı tanımlanır.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    /// Kök hücum kenarı konumu.
    pub root_le: Vec3,
    /// Yarı açıklık (yüzey boyunca, kökten uca).
    pub semi_span: f32,
    pub c_root: f32,
    pub c_tip: f32,
    /// Hücum kenarı ok açısı [derece].
    pub sweep_le_deg: f32,
    /// Dihedral [derece]; 90 = dikey yüzey (yukarı).
    pub dihedral_deg: f32,
    /// Normalize profil noktaları (x 0..1 kord, y kalınlık/kord), firar üst → hücum → firar alt.
    pub profile: Vec<[f32; 2]>,
    /// Y = 0 düzlemine göre aynası da üretilsin mi (sol + sağ yarı)?
    pub mirrored: bool,
}

/// Dönel gövde.
#[derive(Clone, Debug, PartialEq)]
pub struct FuselageSpec {
    pub x_nose: f32,
    pub length: f32,
    pub diameter: f32,
    /// Gövde ekseninin yüksekliği (kanat düzlemine göre).
    pub z_axis: f32,
}

/// Pervane diski (ve isteğe bağlı motor gövdesi).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotorSpec {
    pub center: Vec3,
    /// Disk normali (kaldırma pervanesi: +Z, itici/çekici: ±X).
    pub axis: Vec3,
    pub radius: f32,
    /// Motor çanı (silindir) yarıçapı; 0 ise motor çizilmez.
    pub motor_radius: f32,
    pub motor_height: f32,
}

/// Kol, bom gibi silindirik çubuk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RodSpec {
    pub a: Vec3,
    pub b: Vec3,
    pub radius: f32,
}

/// Sahnenin tamamı için girdiler.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AircraftSpec {
    pub surfaces: Vec<(PartKind, SurfaceSpec)>,
    pub fuselage: Option<FuselageSpec>,
    pub rods: Vec<(PartKind, RodSpec)>,
    pub rotors: Vec<RotorSpec>,
    /// Multirotor merkez plakası: merkez, yarıçap, kalınlık.
    pub plate: Option<(Vec3, f32, f32)>,
    /// Kutular (batarya/elektronik yığını): merkez, boyut.
    pub boxes: Vec<(Vec3, Vec3)>,
    pub cg: Option<Vec3>,
    pub np: Option<Vec3>,
}

/// Parça türü: renk/malzeme ve görünürlük düğmeleri buna göre seçilir.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum PartKind {
    #[default]
    Wing,
    Tail,
    Body,
    Boom,
    Arm,
    Motor,
    /// Yarı saydam pervane diskleri.
    Rotor,
}

impl PartKind {
    pub const ALL: [PartKind; 7] = [
        PartKind::Wing,
        PartKind::Tail,
        PartKind::Body,
        PartKind::Boom,
        PartKind::Arm,
        PartKind::Motor,
        PartKind::Rotor,
    ];

    pub fn translucent(self) -> bool {
        self == PartKind::Rotor
    }
}

#[derive(Clone, Debug, Default)]
pub struct Part {
    pub kind: PartKind,
    pub mesh: Mesh,
}

/// Üretilen sahne: türe göre birleştirilmiş mesh'ler + işaret noktaları + sınır kutusu.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub parts: Vec<Part>,
    pub cg: Option<Vec3>,
    pub np: Option<Vec3>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|p| p.mesh.is_empty())
    }

    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }

    /// Sınır kutusunu çevreleyen kürenin yarıçapı (en az 0.05 m).
    pub fn radius(&self) -> f32 {
        let r = (self.bounds_max - self.bounds_min).length() * 0.5;
        if r.is_finite() { r.max(0.05) } else { 0.5 }
    }

    pub fn part(&self, kind: PartKind) -> Option<&Part> {
        self.parts.iter().find(|p| p.kind == kind)
    }

    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.mesh.triangle_count()).sum()
    }
}

fn finite_pos(x: f32) -> bool {
    x.is_finite() && x > 0.0
}

/// Profili yaklaşık `n` noktaya seyreltir; uçlar (firar kenarı) ve hücum kenarı korunur.
pub fn resample_profile(pts: &[[f32; 2]], n: usize) -> Vec<[f32; 2]> {
    let clean: Vec<[f32; 2]> = pts
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if clean.len() < 3 {
        return fallback_profile();
    }
    let n = n.max(8);
    if clean.len() <= n {
        return clean;
    }
    let le = clean
        .iter()
        .enumerate()
        .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut out = Vec::with_capacity(n + 2);
    let half = n / 2;
    // üst yüzey: 0..=le, alt yüzey: le..end
    for k in 0..=half {
        let i = (k * le) / half.max(1);
        out.push(clean[i]);
    }
    let last = clean.len() - 1;
    for k in 1..=half {
        let i = le + (k * (last - le)) / half.max(1);
        out.push(clean[i]);
    }
    out.dedup();
    out
}

/// Profil verisi kullanılamazsa simetrik %12 kalınlıkta yedek profil (NACA 0012 yaklaşık).
pub fn fallback_profile() -> Vec<[f32; 2]> {
    let n = 16;
    let yt = |x: f32| {
        0.6 * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x * x * x
            - 0.1036 * x * x * x * x)
    };
    let mut v = Vec::new();
    for i in 0..=n {
        let b = std::f32::consts::PI * i as f32 / n as f32;
        let x = 0.5 * (1.0 + b.cos()); // 1 → 0
        v.push([x, yt(x)]);
    }
    for i in 1..=n {
        let b = std::f32::consts::PI * i as f32 / n as f32;
        let x = 0.5 * (1.0 - b.cos()); // 0 → 1
        v.push([x, -yt(x)]);
    }
    v
}

/// Üçgeni verilen yaklaşık dış normale göre doğru sarımla ekler.
fn tri_oriented(mesh: &mut Mesh, a: u32, b: u32, c: u32, outward: Vec3) {
    let p = |i: u32| {
        let v = mesh.positions[i as usize];
        Vec3::new(v[0], v[1], v[2])
    };
    let n = (p(b) - p(a)).cross(p(c) - p(a));
    if n.dot(outward) >= 0.0 {
        mesh.push_triangle(a, b, c);
    } else {
        mesh.push_triangle(a, c, b);
    }
}

/// Tek yarı yüzey mesh'i (aynalama çağıran tarafta).
pub fn surface_half(s: &SurfaceSpec) -> Mesh {
    let mut mesh = Mesh::new();
    if !(finite_pos(s.semi_span) && finite_pos(s.c_root) && s.root_le.is_finite()) {
        return mesh;
    }
    let c_tip = if finite_pos(s.c_tip) {
        s.c_tip
    } else {
        s.c_root
    };
    let sweep = if s.sweep_le_deg.is_finite() {
        s.sweep_le_deg.clamp(-60.0, 60.0).to_radians()
    } else {
        0.0
    };
    let gamma = if s.dihedral_deg.is_finite() {
        s.dihedral_deg.clamp(-90.0, 90.0).to_radians()
    } else {
        0.0
    };
    let prof = resample_profile(&s.profile, 48);
    let chord_dir = Vec3::new(-1.0, 0.0, 0.0);
    let span_dir = Vec3::new(0.0, gamma.cos(), gamma.sin());
    let up_dir = Vec3::new(0.0, -gamma.sin(), gamma.cos());

    let n = prof.len();
    // 2B normaller (profil teğetine dik, dışa doğru): n = (dy, -dx)
    let normals2: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let a = prof[if i == 0 { 0 } else { i - 1 }];
            let b = prof[if i + 1 >= n { n - 1 } else { i + 1 }];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 1e-9 {
                [dy / l, -dx / l]
            } else {
                [0.0, 0.0]
            }
        })
        .collect();

    let section = |mesh: &mut Mesh, span: f32, chord: f32| -> u32 {
        let le = s.root_le + span_dir * span + chord_dir * (span * sweep.tan());
        let first = mesh.vertex_count() as u32;
        for (i, p) in prof.iter().enumerate() {
            let pos = le + chord_dir * (p[0] * chord) + up_dir * (p[1] * chord);
            let nrm = (chord_dir * normals2[i][0] + up_dir * normals2[i][1]).normalized();
            mesh.push_vertex(pos, nrm);
        }
        first
    };
    let r0 = section(&mut mesh, 0.0, s.c_root);
    let t0 = section(&mut mesh, s.semi_span, c_tip);
    for i in 0..n - 1 {
        let (a, b) = (r0 + i as u32, r0 + i as u32 + 1);
        let (c, d) = (t0 + i as u32 + 1, t0 + i as u32);
        let out = {
            let v = mesh.normals[a as usize];
            Vec3::new(v[0], v[1], v[2])
        };
        tri_oriented(&mut mesh, a, b, c, out);
        tri_oriented(&mut mesh, a, c, d, out);
    }
    // Kök ve uç kapakları (düz normal, merkezden yelpaze).
    for (span, chord, dir) in [(0.0, s.c_root, -1.0_f32), (s.semi_span, c_tip, 1.0)] {
        let le = s.root_le + span_dir * span + chord_dir * (span * sweep.tan());
        let cap_n = span_dir * dir;
        let center = le + chord_dir * (0.35 * chord);
        let ci = mesh.push_vertex(center, cap_n);
        let first = mesh.vertex_count() as u32;
        for p in &prof {
            let pos = le + chord_dir * (p[0] * chord) + up_dir * (p[1] * chord);
            mesh.push_vertex(pos, cap_n);
        }
        for i in 0..n as u32 - 1 {
            tri_oriented(&mut mesh, ci, first + i, first + i + 1, cap_n);
        }
    }
    mesh
}

/// Y = 0 düzlemine göre ayna kopyası (sarım ters çevrilir).
pub fn mirror_y(m: &Mesh) -> Mesh {
    let mut out = m.clone();
    for p in &mut out.positions {
        p[1] = -p[1];
    }
    for n in &mut out.normals {
        n[1] = -n[1];
    }
    for t in out.indices.as_chunks_mut::<3>().0 {
        t.swap(1, 2);
    }
    out
}

pub fn translate(m: &mut Mesh, d: Vec3) {
    for p in &mut m.positions {
        p[0] += d.x;
        p[1] += d.y;
        p[2] += d.z;
    }
}

/// Gövde yarıçap profili (x, r): burun → kuyruk, 2D şemadaki biçime benzer yumuşak ogive.
pub fn fuselage_profile(f: &FuselageSpec, n: usize) -> Vec<(f32, f32)> {
    let l = f.length;
    let r = f.diameter * 0.5;
    let n = n.max(6);
    // t: 0 = kuyruk, 1 = burun
    let shape = |t: f32| -> f32 {
        let from_nose = 1.0 - t;
        if from_nose < 0.18 {
            // burun: yarım elips
            let u = from_nose / 0.18;
            (1.0 - (1.0 - u) * (1.0 - u)).max(0.0).sqrt()
        } else if from_nose < 0.6 {
            1.0
        } else {
            // kuyruğa doğru incelme (r → 0.22 r)
            let u = (from_nose - 0.6) / 0.4;
            1.0 - 0.78 * u * u * (3.0 - 2.0 * u)
        }
    };
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let x = f.x_nose - l + t * l;
            let rr = if i == n { 0.0 } else { r * shape(t) };
            (x, rr)
        })
        .collect()
}

/// Sahneyi üretir. Geçersiz/NaN girdiler sessizce atlanır; çıktı her zaman geçerli mesh'tir.
pub fn build(spec: &AircraftSpec) -> Scene {
    let mut by_kind: Vec<(PartKind, Mesh)> =
        PartKind::ALL.iter().map(|k| (*k, Mesh::new())).collect();
    let mut add = |kind: PartKind, m: &Mesh| {
        if let Some(slot) = by_kind.iter_mut().find(|(k, _)| *k == kind) {
            slot.1.append(m);
        }
    };

    for (kind, s) in &spec.surfaces {
        let half = surface_half(s);
        add(*kind, &half);
        if s.mirrored {
            add(*kind, &mirror_y(&half));
        }
    }

    if let Some(f) = &spec.fuselage {
        if finite_pos(f.length) && finite_pos(f.diameter) && f.x_nose.is_finite() {
            let mut m = Mesh::new();
            push_lathe_x(&mut m, &fuselage_profile(f, 28), 28);
            translate(
                &mut m,
                Vec3::new(0.0, 0.0, if f.z_axis.is_finite() { f.z_axis } else { 0.0 }),
            );
            add(PartKind::Body, &m);
        }
    }

    for (kind, r) in &spec.rods {
        let mut m = Mesh::new();
        push_cylinder(&mut m, r.a, r.b, r.radius, 14, true);
        add(*kind, &m);
    }

    if let Some((c, r, t)) = spec.plate {
        let mut m = Mesh::new();
        push_cylinder(
            &mut m,
            c - Vec3::Z * (t * 0.5),
            c + Vec3::Z * (t * 0.5),
            r,
            32,
            true,
        );
        add(PartKind::Body, &m);
    }
    for (c, size) in &spec.boxes {
        if c.is_finite() && size.is_finite() && size.x > 0.0 && size.y > 0.0 && size.z > 0.0 {
            let mut m = Mesh::new();
            push_box(&mut m, *c, *size);
            add(PartKind::Body, &m);
        }
    }

    for r in &spec.rotors {
        let axis = r.axis.normalized();
        if axis == Vec3::ZERO || !r.center.is_finite() {
            continue;
        }
        if finite_pos(r.motor_radius) && finite_pos(r.motor_height) {
            let mut m = Mesh::new();
            push_cylinder(
                &mut m,
                r.center - axis * r.motor_height,
                r.center,
                r.motor_radius,
                16,
                true,
            );
            add(PartKind::Motor, &m);
        }
        if finite_pos(r.radius) {
            let mut m = Mesh::new();
            push_disc(&mut m, r.center + axis * 0.004, axis, r.radius, 40);
            add(PartKind::Rotor, &m);
        }
    }

    let mut parts: Vec<Part> = by_kind
        .into_iter()
        .filter_map(|(kind, mut mesh)| {
            mesh.drop_non_finite_triangles();
            if mesh.is_empty() {
                None
            } else {
                Some(Part { kind, mesh })
            }
        })
        .collect();
    parts.sort_by_key(|p| p.kind);

    let mut bmin = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut bmax = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in &parts {
        if let Some((a, b)) = p.mesh.bounds() {
            if a.is_finite() && b.is_finite() {
                bmin = bmin.min(a);
                bmax = bmax.max(b);
            }
        }
    }
    if !(bmin.is_finite() && bmax.is_finite()) {
        bmin = Vec3::new(-0.5, -0.5, -0.1);
        bmax = Vec3::new(0.5, 0.5, 0.1);
    }
    Scene {
        parts,
        cg: spec.cg.filter(|v| v.is_finite()),
        np: spec.np.filter(|v| v.is_finite()),
        bounds_min: bmin,
        bounds_max: bmax,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wing(semi: f32, c_r: f32, c_t: f32, sweep: f32, dihedral: f32) -> SurfaceSpec {
        SurfaceSpec {
            root_le: Vec3::ZERO,
            semi_span: semi,
            c_root: c_r,
            c_tip: c_t,
            sweep_le_deg: sweep,
            dihedral_deg: dihedral,
            profile: fallback_profile(),
            mirrored: true,
        }
    }

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn wing_span_and_chord_match_inputs() {
        let spec = AircraftSpec {
            surfaces: vec![(PartKind::Wing, wing(0.8, 0.25, 0.15, 0.0, 0.0))],
            ..Default::default()
        };
        let sc = build(&spec);
        let (a, b) = sc.part(PartKind::Wing).unwrap().mesh.bounds().unwrap();
        assert!(approx(b.y - a.y, 1.6, 1e-4), "açıklık {}", b.y - a.y);
        // Ok açısı 0: kök veteri x boyunca 0..-0.25
        assert!(approx(b.x, 0.0, 1e-4) && approx(a.x, -0.25, 1e-4));
        // Kalınlık ~ %12 kök veter
        assert!(approx(b.z - a.z, 0.12 * 0.25, 0.006));
        assert!(sc.part(PartKind::Wing).unwrap().mesh.is_valid());
    }

    #[test]
    fn sweep_moves_tip_aft_and_dihedral_raises_tip() {
        let s = wing(1.0, 0.3, 0.2, 30.0, 0.0);
        let m = surface_half(&s);
        let (a, _) = m.bounds().unwrap();
        // Uç hücum kenarı: -tan(30°) ≈ -0.577; uç firar: -0.777
        assert!(approx(a.x, -(30f32.to_radians().tan()) - 0.2, 1e-3));
        let s = wing(1.0, 0.3, 0.3, 0.0, 10.0);
        let m = surface_half(&s);
        let (_, b) = m.bounds().unwrap();
        assert!(b.z > 10f32.to_radians().sin() * 1.0 - 1e-3);
        assert!(approx(b.y, 10f32.to_radians().cos() + 0.3 * 0.06, 0.02));
    }

    #[test]
    fn vertical_fin_points_up() {
        let mut s = wing(0.2, 0.12, 0.08, 20.0, 90.0);
        s.mirrored = false;
        let m = surface_half(&s);
        let (a, b) = m.bounds().unwrap();
        assert!(approx(b.z, 0.2, 1e-3) && a.z > -0.02);
        assert!(b.y - a.y < 0.03);
    }

    #[test]
    fn mirrored_surface_is_symmetric_and_outward() {
        let half = surface_half(&wing(0.5, 0.2, 0.1, 5.0, 3.0));
        let other = mirror_y(&half);
        let (a1, b1) = half.bounds().unwrap();
        let (a2, b2) = other.bounds().unwrap();
        assert!(approx(a1.y, -b2.y, 1e-6) && approx(b1.y, -a2.y, 1e-6));
        // Her üçgenin yüz normali, köşe normallerinin ortalamasıyla aynı yönde (dışa bakar).
        for m in [&half, &other] {
            let mut bad = 0;
            for t in m.indices.as_chunks::<3>().0 {
                let v = |i: u32| {
                    let p = m.positions[i as usize];
                    Vec3::new(p[0], p[1], p[2])
                };
                let n = |i: u32| {
                    let p = m.normals[i as usize];
                    Vec3::new(p[0], p[1], p[2])
                };
                let f = (v(t[1]) - v(t[0])).cross(v(t[2]) - v(t[0]));
                if f.length() < 1e-12 {
                    continue;
                }
                if f.dot(n(t[0]) + n(t[1]) + n(t[2])) < 0.0 {
                    bad += 1;
                }
            }
            assert_eq!(bad, 0);
        }
    }

    #[test]
    fn nan_inputs_are_dropped_not_propagated() {
        let mut w = wing(f32::NAN, 0.2, 0.1, 0.0, 0.0);
        let spec = AircraftSpec {
            surfaces: vec![(PartKind::Wing, w.clone())],
            fuselage: Some(FuselageSpec {
                x_nose: f32::NAN,
                length: 1.0,
                diameter: 0.1,
                z_axis: 0.0,
            }),
            rods: vec![(
                PartKind::Arm,
                RodSpec {
                    a: Vec3::ZERO,
                    b: Vec3::new(f32::NAN, 0.0, 0.0),
                    radius: 0.01,
                },
            )],
            rotors: vec![RotorSpec {
                center: Vec3::ZERO,
                axis: Vec3::new(f32::NAN, 0.0, 0.0),
                radius: 0.1,
                motor_radius: 0.01,
                motor_height: 0.01,
            }],
            cg: Some(Vec3::new(f32::NAN, 0.0, 0.0)),
            ..Default::default()
        };
        let sc = build(&spec);
        assert!(sc.is_empty());
        assert!(sc.cg.is_none());
        assert!(sc.bounds_min.is_finite() && sc.bounds_max.is_finite());
        assert!(sc.radius().is_finite());
        // Bozuk profil (NaN noktalar) yedek profile düşer.
        w.semi_span = 0.5;
        w.profile = vec![[f32::NAN, 0.0], [0.5, f32::NAN]];
        let m = surface_half(&w);
        assert!(!m.is_empty() && m.is_valid());
    }

    #[test]
    fn rotors_produce_discs_and_motors() {
        let spec = AircraftSpec {
            rotors: (0..4)
                .map(|i| {
                    let a = (45.0 + 90.0 * i as f32).to_radians();
                    RotorSpec {
                        center: Vec3::new(a.cos() * 0.125, a.sin() * 0.125, 0.03),
                        axis: Vec3::Z,
                        radius: 0.0635,
                        motor_radius: 0.012,
                        motor_height: 0.015,
                    }
                })
                .collect(),
            ..Default::default()
        };
        let sc = build(&spec);
        assert!(sc.part(PartKind::Rotor).is_some());
        assert!(sc.part(PartKind::Motor).is_some());
        // 4 disk × 2 yüz × 40 dilim
        assert_eq!(sc.part(PartKind::Rotor).unwrap().mesh.triangle_count(), 320);
        let span = sc.bounds_max.x - sc.bounds_min.x;
        assert!(approx(
            span,
            2.0 * (0.125 * 45f32.to_radians().cos() + 0.0635),
            2e-3
        ));
    }

    #[test]
    fn fuselage_profile_is_closed_and_monotonic_in_x() {
        let f = FuselageSpec {
            x_nose: 0.2,
            length: 0.9,
            diameter: 0.11,
            z_axis: 0.0,
        };
        let pr = fuselage_profile(&f, 20);
        assert!(pr.windows(2).all(|w| w[1].0 > w[0].0));
        assert!(approx(pr.last().unwrap().0, 0.2, 1e-5));
        assert_eq!(pr.last().unwrap().1, 0.0);
        let rmax = pr.iter().map(|p| p.1).fold(0.0, f32::max);
        assert!(approx(rmax, 0.055, 1e-4));
    }

    #[test]
    fn resample_keeps_leading_edge_and_ends() {
        let p: Vec<[f32; 2]> = fallback_profile();
        let r = resample_profile(&p, 10);
        assert!(r.len() <= 12);
        assert!(r.iter().any(|q| q[0] < 1e-6));
        assert_eq!(r[0], p[0]);
        assert_eq!(*r.last().unwrap(), *p.last().unwrap());
    }
}
