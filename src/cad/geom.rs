// SPDX-License-Identifier: AGPL-3.0-or-later
//! CAD geometri yardımcıları (FreeCAD'siz, saf Rust): rijit dönüşüm çerçeveleri, profil
//! (airfoil) üst/alt yüzey ara değerleme, içi boş kabuk için iç profil, taşıyıcı yüzey
//! kesitleri, kiriş ekseni ve gövde dönel profili.
//!
//! Montaj yerleşimi bu modüldeki çerçevelerle hesaplanır: her parçanın yerel çerçevesinde
//! adlandırılmış bağlantı noktaları (anchor) vardır; montaj, çocuk parçanın bağlantı
//! çerçevesini ebeveynin bağlantı çerçevesine çakıştırır.

/// Rijit (ya da yansımalı) dönüşüm: `p_global = r · p_local + t`. `r` sütunları yerel
/// eksenlerin global karşılıklarıdır.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub r: [[f64; 3]; 3],
    pub t: [f64; 3],
}

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
pub fn unit(a: V3) -> V3 {
    let n = norm(a);
    if n > 1e-12 { scale(a, 1.0 / n) } else { a }
}

impl Frame {
    pub const IDENTITY: Frame = Frame {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0, 0.0, 0.0],
    };

    pub fn translation(t: V3) -> Self {
        Frame {
            t,
            ..Frame::IDENTITY
        }
    }

    /// Yerel X, Y, Z eksenlerinin global yönleri ve orijin.
    pub fn from_axes(x: V3, y: V3, z: V3, origin: V3) -> Self {
        Frame {
            r: [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]],
            t: origin,
        }
    }

    /// Z ekseni `z` yönünde olan, X ekseni olabildiğince `x_hint`'e yakın çerçeve.
    pub fn from_z_axis(z: V3, x_hint: V3, origin: V3) -> Self {
        let z = unit(z);
        let mut x = sub(x_hint, scale(z, dot(x_hint, z)));
        if norm(x) < 1e-9 {
            x = sub([0.0, 1.0, 0.0], scale(z, z[1]));
        }
        let x = unit(x);
        let y = cross(z, x);
        Frame::from_axes(x, y, z, origin)
    }

    pub fn rot_z(deg: f64, origin: V3) -> Self {
        let (s, c) = deg.to_radians().sin_cos();
        Frame::from_axes([c, s, 0.0], [-s, c, 0.0], [0.0, 0.0, 1.0], origin)
    }

    /// `axis` (0=X, 1=Y, 2=Z) eksenine dik ve orijinden geçen düzleme göre yansıma.
    pub fn reflect(axis: usize) -> Self {
        let mut f = Frame::IDENTITY;
        f.r[axis][axis] = -1.0;
        f
    }

    /// `self ∘ o` (önce `o`, sonra `self`).
    pub fn mul(&self, o: &Frame) -> Frame {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.r[i][k] * o.r[k][j]).sum();
            }
        }
        Frame {
            r,
            t: self.apply(o.t),
        }
    }

    pub fn apply_dir(&self, v: V3) -> V3 {
        [
            self.r[0][0] * v[0] + self.r[0][1] * v[1] + self.r[0][2] * v[2],
            self.r[1][0] * v[0] + self.r[1][1] * v[1] + self.r[1][2] * v[2],
            self.r[2][0] * v[0] + self.r[2][1] * v[1] + self.r[2][2] * v[2],
        ]
    }

    pub fn apply(&self, p: V3) -> V3 {
        add(self.apply_dir(p), self.t)
    }

    /// Ortogonal `r` için ters dönüşüm (yansımalı çerçevelerde de geçerli).
    pub fn inverse(&self) -> Frame {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = self.r[j][i];
            }
        }
        let t = Frame { r, t: [0.0; 3] }.apply_dir(self.t);
        Frame {
            r,
            t: scale(t, -1.0),
        }
    }

    pub fn det(&self) -> f64 {
        let r = &self.r;
        r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0])
    }

    /// 4x4 satır öncelikli matris (FreeCAD.Matrix 16 değer).
    pub fn matrix16(&self) -> [f64; 16] {
        let r = &self.r;
        let t = &self.t;
        [
            r[0][0], r[0][1], r[0][2], t[0], r[1][0], r[1][1], r[1][2], t[1], r[2][0], r[2][1],
            r[2][2], t[2], 0.0, 0.0, 0.0, 1.0,
        ]
    }

    pub fn is_finite(&self) -> bool {
        self.r.iter().flatten().all(|v| v.is_finite()) && self.t.iter().all(|v| v.is_finite())
    }
}

/// Bağlantı (mate): çocuğun `child_anchor` çerçevesi ebeveynin `parent_anchor` çerçevesine
/// çakışacak şekilde çocuğun global çerçevesi.
pub fn mate(parent_global: &Frame, parent_anchor: &Frame, child_anchor: &Frame) -> Frame {
    parent_global
        .mul(parent_anchor)
        .mul(&child_anchor.inverse())
}

/// Sağ taraf kopyası: global XZ düzlemine göre aynalanmış yerleşim. Parça dosyasında yerel
/// `mirror_axis` düzlemine göre aynalanmış nesne (ya da o düzleme göre simetrik parçanın
/// kendisi) bu çerçeveyle yerleştirildiğinde sol kopyanın tam ayna görüntüsü oluşur.
/// Sonuç her zaman uygun (yansımasız) dönüşümdür, yani App::Link yerleşimi olarak verilebilir.
pub fn mirrored_placement(left: &Frame, mirror_axis: usize) -> Frame {
    Frame::reflect(1)
        .mul(left)
        .mul(&Frame::reflect(mirror_axis))
}

/// Normalize profil (kord 0..1) ve üst/alt yüzeyleri.
#[derive(Clone, Debug)]
pub struct Profile {
    /// FreeCAD'e giden nokta listesi: kapalı (ilk = son), ardışık çift nokta yok.
    pub pts: Vec<[f64; 2]>,
    upper: Vec<[f64; 2]>,
    lower: Vec<[f64; 2]>,
}

fn interp(curve: &[[f64; 2]], x: f64) -> f64 {
    if curve.is_empty() {
        return 0.0;
    }
    if x <= curve[0][0] {
        return curve[0][1];
    }
    for w in curve.windows(2) {
        if x <= w[1][0] {
            let dx = w[1][0] - w[0][0];
            return if dx.abs() < 1e-12 {
                w[1][1]
            } else {
                w[0][1] + (w[1][1] - w[0][1]) * (x - w[0][0]) / dx
            };
        }
    }
    curve[curve.len() - 1][1]
}

impl Profile {
    /// Geçersiz (sonlu olmayan, çok az noktalı) profilde `None`.
    pub fn new(points: &[[f64; 2]]) -> Option<Profile> {
        if points.len() < 5
            || points
                .iter()
                .any(|p| !p[0].is_finite() || !p[1].is_finite())
        {
            return None;
        }
        let mut pts: Vec<[f64; 2]> = Vec::with_capacity(points.len() + 1);
        for p in points {
            if pts
                .last()
                .map(|q: &[f64; 2]| (q[0] - p[0]).abs() + (q[1] - p[1]).abs() > 1e-9)
                .unwrap_or(true)
            {
                pts.push(*p);
            }
        }
        let first = pts[0];
        let last = pts[pts.len() - 1];
        if (first[0] - last[0]).abs() + (first[1] - last[1]).abs() > 1e-9 {
            pts.push(first);
        } else {
            let n = pts.len();
            pts[n - 1] = first;
        }
        if pts.len() < 5 {
            return None;
        }
        let le = pts
            .iter()
            .enumerate()
            .min_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .map(|(i, _)| i)?;
        let mut a: Vec<[f64; 2]> = pts[..=le].to_vec();
        let mut b: Vec<[f64; 2]> = pts[le..].to_vec();
        if a.len() < 2 || b.len() < 2 {
            return None;
        }
        let mean = |v: &[[f64; 2]]| v.iter().map(|p| p[1]).sum::<f64>() / v.len() as f64;
        if mean(&a) < mean(&b) {
            std::mem::swap(&mut a, &mut b);
        }
        a.sort_by(|p, q| p[0].total_cmp(&q[0]));
        b.sort_by(|p, q| p[0].total_cmp(&q[0]));
        Some(Profile {
            pts,
            upper: a,
            lower: b,
        })
    }

    pub fn yu(&self, x: f64) -> f64 {
        interp(&self.upper, x)
    }
    pub fn yl(&self, x: f64) -> f64 {
        interp(&self.lower, x)
    }
    pub fn thickness(&self, x: f64) -> f64 {
        self.yu(x) - self.yl(x)
    }
    pub fn y_max(&self) -> f64 {
        self.pts
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max)
    }
    pub fn y_min(&self) -> f64 {
        self.pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)
    }

    /// İçi boş kabuk için iç profil (mm, hücum kenarı x=0). Yüzeye dik et kalınlığı `wall`
    /// (eğim düzeltmeli düşey kaydırma); kalınlığın 3·wall altına düştüğü burun ve firar
    /// bölgeleri dolu kalır. Profil çok inceyse `None`.
    pub fn inner_polygon(&self, chord: f64, wall: f64, n: usize) -> Option<Vec<[f64; 2]>> {
        if !(chord > 0.0 && wall > 0.0) {
            return None;
        }
        let tau = wall / chord;
        let need = 3.0 * tau;
        let samples = 600;
        let xs: Vec<f64> = (1..samples).map(|i| i as f64 / samples as f64).collect();
        let xa = xs
            .iter()
            .copied()
            .find(|&x| x >= 1.5 * tau && self.thickness(x) >= need)?;
        let xb = xs
            .iter()
            .rev()
            .copied()
            .find(|&x| self.thickness(x) >= need)?;
        if xb - xa < 4.0 * tau {
            return None;
        }
        let d = |f: &dyn Fn(f64) -> f64, x: f64| {
            let h = 1e-3;
            let (x0, x1) = ((x - h).max(0.0), (x + h).min(1.0));
            (f(x1) - f(x0)) / (x1 - x0)
        };
        let n = n.max(8);
        let st: Vec<f64> = (0..=n)
            .map(|i| {
                let s = 0.5 * (1.0 - (std::f64::consts::PI * i as f64 / n as f64).cos());
                xa + (xb - xa) * s
            })
            .collect();
        let mut up = Vec::with_capacity(st.len());
        let mut lo = Vec::with_capacity(st.len());
        for &x in &st {
            let su = d(&|x| self.yu(x), x);
            let sl = d(&|x| self.yl(x), x);
            let mut yu = self.yu(x) - tau * (1.0 + su * su).sqrt();
            let mut yl = self.yl(x) + tau * (1.0 + sl * sl).sqrt();
            if yu - yl < 0.5 * tau {
                let m = 0.5 * (self.yu(x) + self.yl(x));
                yu = m + 0.25 * tau;
                yl = m - 0.25 * tau;
            }
            up.push([x * chord, yu * chord]);
            lo.push([x * chord, yl * chord]);
        }
        let mut poly: Vec<[f64; 2]> = up.into_iter().rev().collect();
        poly.extend(lo);
        Some(poly)
    }
}

/// Taşıyıcı yüzeyin bir yarısı. Yerel çerçeve: X kord (firara doğru), Y kalınlık, Z açıklık
/// (kökten uca); kök hücum kenarı orijin. Kesitler doğrusal (iki kesitli loft).
#[derive(Clone, Debug)]
pub struct SurfaceGeom {
    pub profile: Profile,
    pub c_root: f64,
    pub c_tip: f64,
    pub span: f64,
    pub sweep_le_deg: f64,
}

/// Kiriş (boru) ekseni: yüzey yerel çerçevesinde başlangıç, yön ve boy (mm).
#[derive(Clone, Copy, Debug)]
pub struct SparAxis {
    pub start: V3,
    pub dir: V3,
    pub length: f64,
    pub od: f64,
    pub id: f64,
    /// Kök kaburgası (kök kapağı) kalınlığı: kiriş deliği buradan geçer.
    pub root_rib: f64,
    /// Kök kaburgasındaki delik silindiri: başlangıç ve boy (aynı eksen).
    pub hole_start: V3,
    pub hole_len: f64,
}

impl SparAxis {
    /// Kirişin yüzey yerel çerçevesindeki çerçevesi (boru yerel Z ekseni boyunca).
    pub fn frame(&self) -> Frame {
        Frame::from_z_axis(self.dir, [1.0, 0.0, 0.0], self.start)
    }
}

impl SurfaceGeom {
    pub fn chord_at(&self, z: f64) -> f64 {
        let s = (z.abs() / self.span.max(1e-9)).clamp(0.0, 1.0);
        self.c_root + (self.c_tip - self.c_root) * s
    }
    pub fn le_x_at(&self, z: f64) -> f64 {
        z.abs() * self.sweep_le_deg.to_radians().tan()
    }
    /// Kesitteki normalize (x, y) noktasının yerel koordinatı.
    pub fn point(&self, z: f64, x: f64, y: f64) -> V3 {
        let c = self.chord_at(z);
        [self.le_x_at(z) + x * c, y * c, z]
    }
    /// Kökteki en büyük üst yüzey yüksekliği [mm].
    pub fn root_top(&self) -> f64 {
        self.profile.y_max() * self.c_root
    }
    pub fn root_half_thickness(&self) -> f64 {
        self.profile.y_max().max(-self.profile.y_min()) * self.c_root
    }

    /// Kiriş ekseni: `frac` kord konumunda kalınlık ortası boyunca (kök ve uç kesitlerde
    /// aynı noktayı birleştiren doğru, iki kesitli loftta tamamen yüzeyin içinde kalır).
    /// Kiriş, iç boşluğa sığdığı açıklığa kadar uzanır; hiç sığmıyorsa `None`.
    pub fn spar_axis(&self, frac: f64, od: f64, id: f64, wall: f64) -> Option<SparAxis> {
        if !(od > 0.0 && od.is_finite()) {
            return None;
        }
        let th = self.profile.thickness(frac);
        let mid = 0.5 * (self.profile.yu(frac) + self.profile.yl(frac));
        let c_need = (od + 1.0 + 2.0 * wall) / th.max(1e-6);
        if self.c_root < c_need {
            return None;
        }
        let tip_stop = self.span - wall - 2.0;
        let z_fit = if self.c_tip >= c_need {
            tip_stop
        } else {
            self.span * (self.c_root - c_need) / (self.c_root - self.c_tip)
        };
        let root = self.point(0.0, frac, mid);
        let tip = self.point(self.span, frac, mid);
        let dir = unit(sub(tip, root));
        if dir[2] <= 1e-6 {
            return None;
        }
        let r = od / 2.0;
        let sin_a = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let z0 = r * sin_a + 0.2;
        let root_rib = (3.0 * wall).max(2.0 * r * sin_a + 2.0).max(2.0);
        let z1 = z_fit.min(tip_stop) - r * sin_a;
        if z1 <= root_rib + 5.0 {
            return None;
        }
        let s0 = z0 / dir[2];
        let s1 = z1 / dir[2];
        // Delik silindirinin eğik uç yüzleri de kaburganın tamamen dışında kalmalı.
        let hs0 = (-1.0 - r * sin_a) / dir[2];
        let hs1 = (root_rib + 1.0 + r * sin_a) / dir[2];
        Some(SparAxis {
            start: add(root, scale(dir, s0)),
            dir,
            length: s1 - s0,
            od,
            id: if id > 0.0 && id < od { id } else { 0.0 },
            root_rib,
            hole_start: add(root, scale(dir, hs0)),
            hole_len: hs1 - hs0,
        })
    }

    /// Kalınlık ortası çizgisinin `frac` kord konumunda, `z` açıklığındaki üst/alt yüzeyi
    /// (yerel Y) [mm].
    pub fn yu_at(&self, z: f64, frac: f64) -> f64 {
        self.profile.yu(frac) * self.chord_at(z)
    }
    pub fn yl_at(&self, z: f64, frac: f64) -> f64 {
        self.profile.yl(frac) * self.chord_at(z)
    }
}

/// Gövde dönel profili: (x, r) kırık çizgisi, x artan; burun yüzü x=0'da düz (firewall),
/// kuyruk yüzü x=L'de düz. Doğrusal parçalar sayesinde her x'teki yarıçap kesin bilinir.
#[derive(Clone, Debug)]
pub struct FuseProfile {
    pub pts: Vec<[f64; 2]>,
}

impl FuseProfile {
    /// `nose_r`: burun yüzü yarıçapı, `r`: en büyük yarıçap, `tail_r`: kuyruk yüzü yarıçapı.
    pub fn new(
        length: f64,
        r: f64,
        nose_r: f64,
        tail_r: f64,
        nose_ratio: f64,
        tail_ratio: f64,
    ) -> Self {
        let ln = length * nose_ratio;
        let x_tail = length * (1.0 - tail_ratio);
        let mut pts = Vec::new();
        let k = 5;
        for i in 0..=k {
            let s = i as f64 / k as f64;
            // Elips çeyreğine yakın burun
            let x = ln * s;
            let rr = nose_r + (r - nose_r) * (1.0 - (1.0 - s) * (1.0 - s)).sqrt();
            pts.push([x, rr]);
        }
        pts.push([x_tail, r]);
        pts.push([length, tail_r]);
        Self { pts }
    }

    pub fn length(&self) -> f64 {
        self.pts.last().map(|p| p[0]).unwrap_or(0.0)
    }

    pub fn r_at(&self, x: f64) -> f64 {
        interp(&self.pts, x)
    }

    /// [x0, x1] aralığındaki en küçük yarıçap.
    pub fn r_min(&self, x0: f64, x1: f64) -> f64 {
        let mut m = self.r_at(x0).min(self.r_at(x1));
        for p in &self.pts {
            if p[0] > x0 && p[0] < x1 {
                m = m.min(p[1]);
            }
        }
        m
    }
    pub fn r_max(&self, x0: f64, x1: f64) -> f64 {
        let mut m = self.r_at(x0).max(self.r_at(x1));
        for p in &self.pts {
            if p[0] > x0 && p[0] < x1 {
                m = m.max(p[1]);
            }
        }
        m
    }

    /// İç (boşluk) profili: her doğru parça `wall` kadar içe kaydırılır, uç kapaklar `wall`.
    pub fn inner(&self, wall: f64) -> Vec<[f64; 2]> {
        let n = self.pts.len();
        let l = self.length();
        let lines: Vec<([f64; 2], [f64; 2])> = (0..n - 1)
            .map(|i| {
                let a = self.pts[i];
                let b = self.pts[i + 1];
                let d = [b[0] - a[0], b[1] - a[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-12);
                let d = [d[0] / len, d[1] / len];
                let nrm = [d[1], -d[0]];
                ([a[0] + wall * nrm[0], a[1] + wall * nrm[1]], d)
            })
            .collect();
        let at_x = |line: &([f64; 2], [f64; 2]), x: f64| -> [f64; 2] {
            let (p, d) = line;
            if d[0].abs() < 1e-12 {
                [x, p[1]]
            } else {
                [x, p[1] + d[1] * (x - p[0]) / d[0]]
            }
        };
        let mut out = vec![at_x(&lines[0], wall)];
        for w in lines.windows(2) {
            let (p1, d1) = w[0];
            let (p2, d2) = w[1];
            let cr = d1[0] * d2[1] - d1[1] * d2[0];
            if cr.abs() < 1e-9 {
                out.push(p2);
            } else {
                let t = ((p2[0] - p1[0]) * d2[1] - (p2[1] - p1[1]) * d2[0]) / cr;
                out.push([p1[0] + t * d1[0], p1[1] + t * d1[1]]);
            }
        }
        out.push(at_x(&lines[n - 2], l - wall));
        for p in &mut out {
            p[1] = p[1].max(0.5);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn frame_inverse_and_mate() {
        let f = Frame::from_axes(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [10.0, 2.0, 3.0],
        );
        assert!((f.det() - 1.0).abs() < 1e-12);
        let p = [1.0, 2.0, 3.0];
        let q = f.inverse().apply(f.apply(p));
        for i in 0..3 {
            near(p[i], q[i]);
        }
        // Mate: çocuk bağlantısı ebeveyn bağlantısına çakışır.
        let pa = Frame::translation([5.0, 0.0, 0.0]);
        let ca = Frame::rot_z(90.0, [1.0, 1.0, 0.0]);
        let child = mate(&f, &pa, &ca);
        let a = child.mul(&ca);
        let b = f.mul(&pa);
        for i in 0..3 {
            near(a.t[i], b.t[i]);
            for j in 0..3 {
                near(a.r[i][j], b.r[i][j]);
            }
        }
    }

    #[test]
    fn mirrored_placement_is_proper_and_reflects() {
        let left = Frame::from_axes(
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
            [200.0, 0.0, 0.0],
        );
        let right = mirrored_placement(&left, 2);
        assert!((right.det() - 1.0).abs() < 1e-12);
        // Ayna nesnesindeki nokta (x, y, -z) sağda (x, +z, y) konumuna gelmeli.
        let p_left = left.apply([10.0, 2.0, 300.0]);
        let p_right = right.apply([10.0, 2.0, -300.0]);
        near(p_left[0], p_right[0]);
        near(p_left[1], -p_right[1]);
        near(p_left[2], p_right[2]);
    }

    #[test]
    fn profile_split_and_inner() {
        let pts = crate::cad::airfoil::generate_naca4_points("2412", 100, true).unwrap();
        let pr = Profile::new(&pts).unwrap();
        assert!((pr.thickness(0.3) - 0.12).abs() < 0.01);
        assert!(pr.yu(0.3) > pr.yl(0.3));
        let inner = pr.inner_polygon(240.0, 0.5, 40).unwrap();
        // İç profil dış profilin içinde ve et kalınlığı kadar uzakta.
        for q in &inner {
            let x = q[0] / 240.0;
            assert!(q[1] <= pr.yu(x) * 240.0 - 0.3, "{q:?}");
            assert!(q[1] >= pr.yl(x) * 240.0 + 0.3, "{q:?}");
        }
        // Çok ince profil/kalın duvar: kabuk yok.
        assert!(pr.inner_polygon(10.0, 2.0, 40).is_none());
        assert!(Profile::new(&[[0.0, f64::NAN]; 8]).is_none());
    }

    #[test]
    fn spar_axis_stays_inside_and_starts_past_root() {
        let pts = crate::cad::airfoil::generate_naca4_points("2412", 100, true).unwrap();
        let s = SurfaceGeom {
            profile: Profile::new(&pts).unwrap(),
            c_root: 240.0,
            c_tip: 160.0,
            span: 800.0,
            sweep_le_deg: 20.0,
        };
        let a = s.spar_axis(0.25, 10.0, 8.0, 0.5).unwrap();
        assert!(a.start[2] > 0.0 && a.root_rib > a.start[2]);
        // Delik kaburgayı eğik uçlarıyla birlikte tamamen geçer.
        let sin_a = (a.dir[0].powi(2) + a.dir[1].powi(2)).sqrt();
        let hole_end = add(a.hole_start, scale(a.dir, a.hole_len));
        assert!(a.hole_start[2] + 5.0 * sin_a < 0.0);
        assert!(hole_end[2] - 5.0 * sin_a > a.root_rib);
        let end = add(a.start, scale(a.dir, a.length));
        assert!(end[2] < s.span);
        // Kiriş ucu dörtte bir kord çizgisi üzerinde.
        let x_qc = s.le_x_at(end[2]) + 0.25 * s.chord_at(end[2]);
        assert!((end[0] - x_qc).abs() < 1.0);
    }

    #[test]
    fn fuse_profile_inner_offset() {
        let f = FuseProfile::new(900.0, 55.0, 25.0, 16.0, 0.25, 0.35);
        near(f.r_at(400.0), 55.0);
        assert!(f.r_at(0.0) == 25.0 && f.r_at(900.0) == 16.0);
        let inner = f.inner(0.5);
        assert_eq!(inner.len(), f.pts.len());
        near(inner[0][0], 0.5);
        near(inner[inner.len() - 1][0], 899.5);
        // Silindirik bölümde iç yarıçap tam wall kadar küçük.
        near(interp(&inner, 400.0), 54.5);
    }
}
