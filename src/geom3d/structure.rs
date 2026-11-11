// SPDX-License-Identifier: AGPL-3.0-or-later
//! Üretime uygun iç yapı geometrisi: kabuk kalınlıklı kanat/kuyruk dış derisi, kaburgalar, kiriş
//! borusu, ayrı menteşeli kontrol yüzeyleri ve gövde kabuğu/çerçeveleri. Saf geometri; sayılar
//! `spec.rs` içinde projeden (ya da adlandırılmış varsayılanlardan) gelir.
use super::aircraft::{FuselageSpec, PartKind, SurfaceSpec, resample_profile, surface_half};
use super::math::Vec3;
use super::mesh::{Mesh, push_cylinder, push_flat_cap, push_grid};
use std::f32::consts::TAU;

/// Kiriş borusu: kord oranında konum ve dış çap [m].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparSpec {
    pub x_frac: f32,
    pub od: f32,
}

/// Menteşeli kontrol yüzeyi: menteşe çizgisi (kord oranı), açıklık aralığı (0..1) ve menteşe
/// aralığı [m] (ön yüzey ile kontrol yüzeyi arasındaki ve uç yüzlerdeki boşluk).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlSpec {
    pub hinge_frac: f32,
    pub f0: f32,
    pub f1: f32,
    pub gap: f32,
}

/// Taşıyıcı yüzeyin iç yapısı (yarı yüzey başına).
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceStructure {
    /// Deri et kalınlığı [m].
    pub skin_t: f32,
    /// Yarı yüzeydeki kaburga sayısı.
    pub ribs: u32,
    pub rib_t: f32,
    pub spar: Option<SparSpec>,
    pub control: Option<ControlSpec>,
}

/// Gövde kabuğu: et kalınlığı, burundan kord oranıyla çerçeve konumları ve çerçeve kalınlığı.
#[derive(Clone, Debug, PartialEq)]
pub struct FuselageStructure {
    pub wall: f32,
    pub formers: Vec<f32>,
    pub former_t: f32,
}

fn finite_pos(x: f32) -> bool {
    x.is_finite() && x > 0.0
}

// ---------------------------------------------------------------------------------------
// Profil yardımcıları
// ---------------------------------------------------------------------------------------

fn profile_normals(prof: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = prof.len();
    (0..n)
        .map(|i| {
            let a = prof[i.saturating_sub(1)];
            let b = prof[(i + 1).min(n - 1)];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 1e-9 {
                [dy / l, -dx / l]
            } else {
                [0.0, 0.0]
            }
        })
        .collect()
}

fn le_index(prof: &[[f32; 2]]) -> usize {
    prof.iter()
        .enumerate()
        .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// `side` (x tek yönlü) üzerinde x'teki y değeri.
fn y_on(side: &[[f32; 2]], x: f32) -> Option<f32> {
    side.windows(2).find_map(|w| {
        let (a, b) = (w[0], w[1]);
        if (a[0] - x) * (b[0] - x) <= 0.0 && (a[0] - b[0]).abs() > 1e-9 {
            Some(a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]))
        } else {
            None
        }
    })
}

fn sides(prof: &[[f32; 2]]) -> (&[[f32; 2]], &[[f32; 2]]) {
    let le = le_index(prof);
    (&prof[..=le], &prof[le..])
}

/// Normalize profil kalınlığı (üst - alt) x'te; bulunamazsa 0.
pub fn thickness_at(prof: &[[f32; 2]], x: f32) -> f32 {
    let (up, lo) = sides(prof);
    match (y_on(up, x), y_on(lo, x)) {
        (Some(a), Some(b)) => (a - b).max(0.0),
        _ => 0.0,
    }
}

/// Orta (kamber) çizgisi y değeri x'te.
pub fn mid_at(prof: &[[f32; 2]], x: f32) -> f32 {
    let (up, lo) = sides(prof);
    match (y_on(up, x), y_on(lo, x)) {
        (Some(a), Some(b)) => 0.5 * (a + b),
        _ => 0.0,
    }
}

/// Profili dış normali boyunca `off` kadar (normalize) içeri iter; ince bölgede kalınlığın
/// yarısından azıyla sınırlı kalır (iç yüzey dış yüzeyi kesmesin).
fn inset(full: &[[f32; 2]], pts: &[[f32; 2]], nrm: &[[f32; 2]], off: f32) -> Vec<[f32; 2]> {
    pts.iter()
        .zip(nrm)
        .map(|(p, n)| {
            let th = thickness_at(full, p[0]);
            let o = off.min(0.45 * th.max(2.0 * off));
            [p[0] - n[0] * o, p[1] - n[1] * o]
        })
        .collect()
}

#[derive(Clone, Debug)]
struct Run {
    pts: Vec<[f32; 2]>,
    nrm: Vec<[f32; 2]>,
}

fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

fn unit2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l > 1e-9 {
        [v[0] / l, v[1] / l]
    } else {
        [0.0, 0.0]
    }
}

/// Profilin x ≤ xc (`front`) ya da x ≥ xc kısmındaki ardışık parçalar; kesişim noktaları
/// enterpole edilerek eklenir. Ön için tek parça (üst kesim → burun → alt kesim), arka için iki
/// parça (üst firar → üst kesim) ve (alt kesim → alt firar).
fn runs(prof: &[[f32; 2]], nrm: &[[f32; 2]], xc: f32, front: bool) -> Vec<Run> {
    let keep = |x: f32| if front { x <= xc } else { x >= xc };
    let mut out: Vec<Run> = Vec::new();
    let mut cur: Option<Run> = None;
    for i in 0..prof.len() {
        if i == 0 {
            if keep(prof[0][0]) {
                cur = Some(Run {
                    pts: vec![prof[0]],
                    nrm: vec![nrm[0]],
                });
            }
            continue;
        }
        let (a, b) = (prof[i - 1], prof[i]);
        let (ka, kb) = (keep(a[0]), keep(b[0]));
        if ka != kb && (b[0] - a[0]).abs() > 1e-9 {
            let t = (xc - a[0]) / (b[0] - a[0]);
            let c = lerp2(a, b, t);
            let cn = unit2(lerp2(nrm[i - 1], nrm[i], t));
            if ka {
                if let Some(mut r) = cur.take() {
                    r.pts.push(c);
                    r.nrm.push(cn);
                    out.push(r);
                }
            } else {
                cur = Some(Run {
                    pts: vec![c],
                    nrm: vec![cn],
                });
            }
        }
        if kb {
            if let Some(r) = cur.as_mut() {
                r.pts.push(b);
                r.nrm.push(nrm[i]);
            }
        }
    }
    if let Some(r) = cur {
        out.push(r);
    }
    out.retain(|r| r.pts.len() >= 2);
    out
}

// ---------------------------------------------------------------------------------------
// Loft eşlemesi (aircraft::surface_half ile aynı)
// ---------------------------------------------------------------------------------------

struct Loft {
    root_le: Vec3,
    chord_dir: Vec3,
    span_dir: Vec3,
    up_dir: Vec3,
    tan_sweep: f32,
    twist: f32,
    semi: f32,
    c_root: f32,
    c_tip: f32,
}

impl Loft {
    fn new(s: &SurfaceSpec) -> Option<Loft> {
        if !(finite_pos(s.semi_span) && finite_pos(s.c_root) && s.root_le.is_finite()) {
            return None;
        }
        let clamp = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { 0.0 };
        let sweep = clamp(s.sweep_le_deg, -60.0, 60.0).to_radians();
        let gamma = clamp(s.dihedral_deg, -90.0, 90.0).to_radians();
        Some(Loft {
            root_le: s.root_le,
            chord_dir: Vec3::new(-1.0, 0.0, 0.0),
            span_dir: Vec3::new(0.0, gamma.cos(), gamma.sin()),
            up_dir: Vec3::new(0.0, -gamma.sin(), gamma.cos()),
            tan_sweep: sweep.tan(),
            twist: clamp(s.twist_tip_deg, -20.0, 20.0).to_radians(),
            semi: s.semi_span,
            c_root: s.c_root,
            c_tip: if finite_pos(s.c_tip) {
                s.c_tip
            } else {
                s.c_root
            },
        })
    }

    fn chord(&self, f: f32) -> f32 {
        self.c_root + (self.c_tip - self.c_root) * f
    }

    fn point(&self, f: f32, p: [f32; 2]) -> Vec3 {
        let span = self.semi * f;
        let chord = self.chord(f);
        let le = self.root_le + self.span_dir * span + self.chord_dir * (span * self.tan_sweep);
        let (st, ct) = (self.twist * f).sin_cos();
        let a = (p[0] - 0.25) * chord;
        let h = p[1] * chord;
        le + self.chord_dir * (0.25 * chord + a * ct + h * st) + self.up_dir * (h * ct - a * st)
    }

    fn normal(&self, f: f32, n: [f32; 2]) -> Vec3 {
        let (st, ct) = (self.twist * f).sin_cos();
        self.chord_dir * (n[0] * ct + n[1] * st) + self.up_dir * (n[1] * ct - n[0] * st)
    }
}

fn centroid(pts: &[Vec3]) -> Vec3 {
    let mut c = Vec3::ZERO;
    for p in pts {
        c += *p;
    }
    c * (1.0 / pts.len().max(1) as f32)
}

/// Bir parçayı [f0, f1] açıklık aralığında loft eder. `face`: (a, b, normal işareti) ile iki
/// profil noktası arasında düz bir yüz (menteşe yüzü) eklenir.
struct Piece<'a> {
    l: &'a Loft,
    runs: &'a [Run],
    f0: f32,
    f1: f32,
    caps: (bool, bool),
    face: Option<([f32; 2], [f32; 2], f32)>,
}

impl Piece<'_> {
    fn emit(&self, mesh: &mut Mesh) {
        const STATIONS: usize = 3;
        let fs: Vec<f32> = (0..STATIONS)
            .map(|k| self.f0 + (self.f1 - self.f0) * k as f32 / (STATIONS - 1) as f32)
            .collect();
        for r in self.runs {
            let rows: Vec<Vec<Vec3>> = fs
                .iter()
                .map(|&f| r.pts.iter().map(|p| self.l.point(f, *p)).collect())
                .collect();
            let refs: Vec<Vec<Vec3>> = fs
                .iter()
                .map(|&f| r.nrm.iter().map(|n| self.l.normal(f, *n)).collect())
                .collect();
            push_grid(mesh, &rows, &refs, false);
        }
        if let Some((a, b, sign)) = self.face {
            let rows: Vec<Vec<Vec3>> = fs
                .iter()
                .map(|&f| vec![self.l.point(f, a), self.l.point(f, b)])
                .collect();
            let refs: Vec<Vec<Vec3>> = fs
                .iter()
                .map(|_| vec![self.l.chord_dir * sign; 2])
                .collect();
            push_grid(mesh, &rows, &refs, false);
        }
        for (on, f, dir) in [
            (self.caps.0, self.f0, -1.0_f32),
            (self.caps.1, self.f1, 1.0),
        ] {
            if !on {
                continue;
            }
            let ring: Vec<Vec3> = self
                .runs
                .iter()
                .flat_map(|r| r.pts.iter())
                .map(|p| self.l.point(f, *p))
                .collect();
            if ring.len() >= 3 {
                push_flat_cap(mesh, centroid(&ring), &ring, self.l.span_dir * dir);
            }
        }
    }
}

/// Yüzeyin yarı parçaları: dış deri (verilen tür), kontrol yüzeyi, kaburgalar ve kiriş.
/// Yapı tanımı yoksa yalnız dış deri döner. Ayna çağıran tarafta yapılır.
pub fn surface_parts(s: &SurfaceSpec, kind: PartKind) -> Vec<(PartKind, Mesh)> {
    let Some(st) = &s.structure else {
        return vec![(kind, surface_half(s))];
    };
    let Some(l) = Loft::new(s) else {
        return Vec::new();
    };
    let prof = resample_profile(&s.profile, 48);
    let nrm = profile_normals(&prof);
    let full = [Run {
        pts: prof.clone(),
        nrm: nrm.clone(),
    }];
    let root_cap = !s.mirrored;
    let mut skin = Mesh::new();
    let mut ctl = Mesh::new();
    let c_avg = 0.5 * (l.c_root + l.c_tip);
    // Geçerli kontrol yüzeyi (aralıklar ve menteşe makul değilse hiç çizilmez).
    let control = st.control.filter(|c| {
        c.hinge_frac > 0.3
            && c.hinge_frac < 0.92
            && c.f0 >= 0.02
            && c.f1 <= 0.98
            && c.f1 > c.f0 + 0.1
            && finite_pos(c.gap)
    });
    let mut x_fixed_max = None;
    match control {
        None => Piece {
            l: &l,
            runs: &full,
            f0: 0.0,
            f1: 1.0,
            caps: (root_cap, true),
            face: None,
        }
        .emit(&mut skin),
        Some(c) => {
            let gap_n = c.gap / c_avg;
            let xf = c.hinge_frac - gap_n;
            x_fixed_max = Some(xf);
            let dg = c.gap / l.semi;
            for (f0, f1, caps) in [(0.0, c.f0, (root_cap, true)), (c.f1, 1.0, (true, true))] {
                Piece {
                    l: &l,
                    runs: &full,
                    f0,
                    f1,
                    caps,
                    face: None,
                }
                .emit(&mut skin);
            }
            let fixed = runs(&prof, &nrm, xf, true);
            if let Some(r) = fixed.first() {
                let (a, b) = (r.pts[0], *r.pts.last().unwrap_or(&r.pts[0]));
                Piece {
                    l: &l,
                    runs: &fixed,
                    f0: c.f0,
                    f1: c.f1,
                    caps: (false, false),
                    face: Some((a, b, 1.0)),
                }
                .emit(&mut skin);
            }
            let aft = runs(&prof, &nrm, c.hinge_frac, false);
            if aft.len() == 2 {
                let (a, b) = (*aft[0].pts.last().unwrap_or(&aft[0].pts[0]), aft[1].pts[0]);
                Piece {
                    l: &l,
                    runs: &aft,
                    f0: c.f0 + dg,
                    f1: c.f1 - dg,
                    caps: (true, true),
                    face: Some((a, b, -1.0)),
                }
                .emit(&mut ctl);
            }
        }
    }

    // İç deri (kabuk kalınlığı): ters normallerle; kontrol açıklığında yalnız sabit kısım.
    let mut parts: Vec<(PartKind, Mesh)> = Vec::new();
    if finite_pos(st.skin_t) {
        let mut segs: Vec<(f32, f32, Run)> = Vec::new();
        match (control, x_fixed_max) {
            (Some(c), Some(xf)) => {
                segs.push((0.0, c.f0, full[0].clone()));
                if let Some(r) = runs(&prof, &nrm, xf, true).into_iter().next() {
                    segs.push((c.f0, c.f1, r));
                }
                segs.push((c.f1, 1.0, full[0].clone()));
            }
            _ => segs.push((0.0, 1.0, full[0].clone())),
        }
        const ROWS: usize = 4;
        for (fa, fb, run) in segs {
            let (mut rows, mut refs) = (Vec::new(), Vec::new());
            for k in 0..ROWS {
                let f = fa + (fb - fa) * k as f32 / (ROWS - 1) as f32;
                let inner = inset(&prof, &run.pts, &run.nrm, st.skin_t / l.chord(f));
                rows.push(inner.iter().map(|p| l.point(f, *p)).collect::<Vec<_>>());
                refs.push(run.nrm.iter().map(|n| -l.normal(f, *n)).collect::<Vec<_>>());
            }
            push_grid(&mut skin, &rows, &refs, false);
        }
    }
    parts.push((kind, skin));
    parts.push((PartKind::Control, ctl));

    // Kaburgalar: iç çizgide ince plakalar (kontrol yüzeyi aralığında yalnız sabit kısım).
    let n = st.ribs.min(64);
    let mut ribs = Mesh::new();
    let off_t = st.skin_t.max(0.0) + 2e-4;
    let dt = (st.rib_t * 0.5) / l.semi;
    for k in 0..n {
        let f = if n == 1 {
            0.5
        } else {
            0.03 + 0.94 * k as f32 / (n - 1) as f32
        };
        let inner = inset(&prof, &prof, &nrm, off_t / l.chord(f));
        let in_ctl = match (control, x_fixed_max) {
            (Some(c), Some(_)) => f >= c.f0 - dt && f <= c.f1 + dt,
            _ => false,
        };
        let ring2d: Vec<[f32; 2]> = if in_ctl {
            let xmax = x_fixed_max.unwrap_or(1.0) - 0.01;
            let zeros = vec![[0.0, 0.0]; inner.len()];
            match runs(&inner, &zeros, xmax, true).into_iter().next() {
                Some(r) => r.pts,
                None => continue,
            }
        } else {
            inner
        };
        let (fa, fb) = ((f - dt).max(0.0), (f + dt).min(1.0));
        let ra: Vec<Vec3> = ring2d.iter().map(|p| l.point(fa, *p)).collect();
        let rb: Vec<Vec3> = ring2d.iter().map(|p| l.point(fb, *p)).collect();
        let (ca, cb) = (centroid(&ra), centroid(&rb));
        let radial = |ring: &[Vec3], c: Vec3| -> Vec<Vec3> {
            ring.iter().map(|p| (*p - c).normalized()).collect()
        };
        let refs = vec![radial(&ra, ca), radial(&rb, cb)];
        push_grid(&mut ribs, &[ra.clone(), rb.clone()], &refs, true);
        push_flat_cap(&mut ribs, ca, &ra, l.span_dir * -1.0);
        push_flat_cap(&mut ribs, cb, &rb, l.span_dir);
    }
    parts.push((PartKind::Rib, ribs));

    // Kiriş borusu.
    let mut spar = Mesh::new();
    if let Some(sp) = st.spar.filter(|sp| sp.x_frac > 0.05 && sp.x_frac < 0.8) {
        let f_end = 0.96;
        let ys = mid_at(&prof, sp.x_frac);
        let th = thickness_at(&prof, sp.x_frac);
        let c_min = l.chord(0.0).min(l.chord(f_end));
        let r = (sp.od * 0.5).min(0.45 * th * c_min).max(5e-4);
        let a = l.point(0.0, [sp.x_frac, ys]);
        let b = l.point(f_end, [sp.x_frac, ys]);
        push_cylinder(&mut spar, a, b, r, 16, true);
    }
    parts.push((PartKind::Spar, spar));
    parts
}

// ---------------------------------------------------------------------------------------
// Gövde kabuğu ve çerçeveler
// ---------------------------------------------------------------------------------------

/// Gövde yarıçapı (x, dış) — `fuselage_profile` üzerinde doğrusal enterpolasyon.
pub fn fuselage_radius_at(f: &FuselageSpec, x: f32) -> f32 {
    let pr = super::aircraft::fuselage_profile(f, 36);
    for w in pr.windows(2) {
        let ((xa, ra), (xb, rb)) = (w[0], w[1]);
        if x >= xa && x <= xb && xb > xa {
            return ra + (rb - ra) * (x - xa) / (xb - xa);
        }
    }
    0.0
}

fn ring_at(f: &FuselageSpec, x: f32, r: f32) -> Vec<Vec3> {
    const SEG: usize = 32;
    const EXP: f32 = 2.6;
    let zc = if f.z_axis.is_finite() { f.z_axis } else { 0.0 };
    let sx = |t: f32| t.signum() * t.abs().powf(2.0 / EXP);
    (0..SEG)
        .map(|j| {
            let t = TAU * j as f32 / SEG as f32;
            Vec3::new(x, r * sx(t.cos()), zc + r * 0.92 * sx(t.sin()))
        })
        .collect()
}

fn radial_refs(ring: &[Vec3], zc: f32, sign: f32) -> Vec<Vec3> {
    ring.iter()
        .map(|p| Vec3::new(0.0, p.y, p.z - zc).normalized() * sign)
        .collect()
}

/// Gövde kabuğunun iç yüzeyi (et kalınlığı kadar içeride) ve çerçeveler (halka plakalar).
/// Dönen çift: (iç kabuk, çerçeveler).
pub fn fuselage_shell(f: &FuselageSpec, st: &FuselageStructure) -> (Mesh, Mesh) {
    let mut inner = Mesh::new();
    let mut formers = Mesh::new();
    if !(finite_pos(f.length) && finite_pos(f.diameter) && f.x_nose.is_finite()) {
        return (inner, formers);
    }
    let zc = if f.z_axis.is_finite() { f.z_axis } else { 0.0 };
    let wall = st.wall.max(0.0);
    let r_in = |x: f32| (fuselage_radius_at(f, x) - wall).max(0.0);
    // İç kabuk: dış profilin istasyonlarında, kuyruk ucu açık.
    let pr = super::aircraft::fuselage_profile(f, 36);
    let (mut rows, mut refs) = (Vec::new(), Vec::new());
    for &(x, r) in &pr {
        let ri = (r - wall).max(r * 0.35).max(1e-4);
        let ring = ring_at(f, x, ri);
        refs.push(radial_refs(&ring, zc, -1.0));
        rows.push(ring);
    }
    push_grid(&mut inner, &rows, &refs, true);
    // Çerçeveler: iç kabuğa oturan, ortası oyuk halka plakalar.
    let t = st.former_t.max(1e-4);
    for fr in &st.formers {
        let xf = f.x_nose - fr.clamp(0.02, 0.98) * f.length;
        let ro = (r_in(xf) * 0.98).max(0.0);
        if ro < 4e-3 {
            continue;
        }
        let (x0, x1) = (xf - t * 0.5, xf + t * 0.5);
        let ring = |x: f32, r: f32| ring_at(f, x, r);
        let (o0, o1) = (ring(x0, ro), ring(x1, ro));
        let (h0, h1) = (ring(x0, ro * 0.55), ring(x1, ro * 0.55));
        let rad = |r: &[Vec3], s: f32| radial_refs(r, zc, s);
        let ax = |dir: f32| vec![Vec3::X * dir; o0.len()];
        push_grid(
            &mut formers,
            &[o0.clone(), h0.clone()],
            &[ax(-1.0), ax(-1.0)],
            true,
        );
        push_grid(
            &mut formers,
            &[o1.clone(), h1.clone()],
            &[ax(1.0), ax(1.0)],
            true,
        );
        push_grid(
            &mut formers,
            &[o0.clone(), o1.clone()],
            &[rad(&o0, 1.0), rad(&o1, 1.0)],
            true,
        );
        push_grid(
            &mut formers,
            &[h0.clone(), h1.clone()],
            &[rad(&h0, -1.0), rad(&h1, -1.0)],
            true,
        );
    }
    (inner, formers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom3d::aircraft::fallback_profile;

    fn wing_with(ctl: Option<ControlSpec>, ribs: u32) -> SurfaceSpec {
        SurfaceSpec {
            root_le: Vec3::ZERO,
            semi_span: 1.0,
            c_root: 0.3,
            c_tip: 0.3,
            sweep_le_deg: 0.0,
            dihedral_deg: 0.0,
            twist_tip_deg: 0.0,
            structure: Some(SurfaceStructure {
                skin_t: 0.001,
                ribs,
                rib_t: 0.002,
                spar: Some(SparSpec {
                    x_frac: 0.25,
                    od: 0.010,
                }),
                control: ctl,
            }),
            profile: fallback_profile(),
            mirrored: true,
        }
    }

    fn bounds_of(parts: &[(PartKind, Mesh)], kinds: &[PartKind]) -> (Vec3, Vec3) {
        let mut mn = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut mx = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for (k, m) in parts {
            if kinds.contains(k) {
                if let Some((a, b)) = m.bounds() {
                    mn = mn.min(a);
                    mx = mx.max(b);
                }
            }
        }
        (mn, mx)
    }

    fn ctl() -> ControlSpec {
        ControlSpec {
            hinge_frac: 0.7,
            f0: 0.5,
            f1: 0.95,
            gap: 0.004,
        }
    }

    #[test]
    fn structure_parts_exist_and_are_valid() {
        let parts = surface_parts(&wing_with(Some(ctl()), 6), PartKind::Wing);
        for kind in [
            PartKind::Wing,
            PartKind::Control,
            PartKind::Rib,
            PartKind::Spar,
        ] {
            let m = &parts.iter().find(|(k, _)| *k == kind).unwrap().1;
            assert!(!m.is_empty() && m.is_valid(), "{kind:?}");
        }
    }

    #[test]
    fn rib_count_matches_spec() {
        for n in [3u32, 6, 9] {
            let parts = surface_parts(&wing_with(Some(ctl()), n), PartKind::Wing);
            let ribs = &parts.iter().find(|(k, _)| *k == PartKind::Rib).unwrap().1;
            let mut ys: Vec<f32> = ribs.positions.iter().map(|p| p[1]).collect();
            ys.sort_by(f32::total_cmp);
            // Her kaburga iki yüzlü ince plaka: yakın y değerleri tek kümedir.
            let mut clusters = 0;
            let mut last = f32::NEG_INFINITY;
            for y in ys {
                if y - last > 0.004 {
                    clusters += 1;
                }
                last = y;
            }
            assert_eq!(clusters, n as usize);
        }
    }

    #[test]
    fn ribs_and_spar_lie_inside_the_skin_box() {
        for c in [None, Some(ctl())] {
            let parts = surface_parts(&wing_with(c, 7), PartKind::Wing);
            let (smin, smax) = bounds_of(&parts, &[PartKind::Wing, PartKind::Control]);
            let (imin, imax) = bounds_of(&parts, &[PartKind::Rib, PartKind::Spar]);
            let tol = 1e-5;
            assert!(imin.x >= smin.x - tol && imax.x <= smax.x + tol);
            assert!(imin.y >= smin.y - tol && imax.y <= smax.y + tol);
            assert!(imin.z >= smin.z - tol && imax.z <= smax.z + tol);
        }
    }

    #[test]
    fn control_surface_is_separate_with_a_hinge_gap() {
        let parts = surface_parts(&wing_with(Some(ctl()), 5), PartKind::Wing);
        let ctl_mesh = &parts
            .iter()
            .find(|(k, _)| *k == PartKind::Control)
            .unwrap()
            .1;
        let skin = &parts.iter().find(|(k, _)| *k == PartKind::Wing).unwrap().1;
        let (cmin, cmax) = ctl_mesh.bounds().unwrap();
        // Kontrol yüzeyi menteşeden firara kadar (x: -0.21 .. -0.30), açıklıkta 0.5..0.95 içinde.
        assert!((cmax.x - (-0.21)).abs() < 1e-4 && (cmin.x - (-0.30)).abs() < 1e-4);
        assert!(cmin.y > 0.5 && cmax.y < 0.95);
        // Kontrol açıklığındaki sabit deri firar tarafında menteşe aralığı kadar geride durur.
        let aft_fixed = skin
            .positions
            .iter()
            .filter(|p| p[1] > 0.55 && p[1] < 0.9)
            .map(|p| p[0])
            .fold(f32::MAX, f32::min);
        assert!(aft_fixed - cmax.x > 0.003, "{aft_fixed} {}", cmax.x);
    }

    #[test]
    fn invalid_control_is_ignored_and_plain_surface_has_no_structure() {
        let bad = ControlSpec {
            hinge_frac: 0.7,
            f0: 0.6,
            f1: 0.62,
            gap: 0.004,
        };
        let parts = surface_parts(&wing_with(Some(bad), 4), PartKind::Wing);
        assert!(
            parts
                .iter()
                .find(|(k, _)| *k == PartKind::Control)
                .unwrap()
                .1
                .is_empty()
        );
        let mut s = wing_with(None, 4);
        s.structure = None;
        let plain = surface_parts(&s, PartKind::Tail);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].0, PartKind::Tail);
    }

    #[test]
    fn fuselage_shell_is_inside_and_formers_fit() {
        let f = FuselageSpec {
            x_nose: 0.2,
            length: 0.9,
            diameter: 0.11,
            z_axis: -0.02,
            structure: Some(FuselageStructure {
                wall: 0.001,
                formers: vec![0.1, 0.4, 0.7],
                former_t: 0.003,
            }),
        };
        let (inner, formers) = fuselage_shell(&f, f.structure.as_ref().unwrap());
        assert!(inner.is_valid() && formers.is_valid() && !formers.is_empty());
        let mut outer = Mesh::new();
        crate::geom3d::parts::push_fuselage(&mut outer, &f);
        let (omin, omax) = outer.bounds().unwrap();
        for m in [&inner, &formers] {
            let (a, b) = m.bounds().unwrap();
            assert!(a.x >= omin.x - 1e-5 && b.x <= omax.x + 1e-5);
            assert!(a.y >= omin.y && b.y <= omax.y && a.z >= omin.z && b.z <= omax.z);
        }
    }
}
