// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gövde, pervane ve tekerlek gibi dönel/loft parçaların mesh üreticileri. Saf geometri;
//! `aircraft::build` bunları parça türüne göre birleştirir.
use super::aircraft::{FuselageSpec, fuselage_profile};
use super::math::Vec3;
use super::mesh::{Mesh, basis_around, push_flat_cap, push_grid, push_lathe_axis};
use std::f32::consts::TAU;

fn finite_pos(x: f32) -> bool {
    x.is_finite() && x > 0.0
}

/// Süper elips kesitli gövde (genişlik = çap, yükseklik ≈ %92), burun yarım elips, kuyruk
/// koni; kuyruk ucu düz kapakla kapanır. Eksen `z_axis` yüksekliğinde X boyunca uzanır.
pub fn push_fuselage(mesh: &mut Mesh, f: &FuselageSpec) {
    if !(finite_pos(f.length) && finite_pos(f.diameter) && f.x_nose.is_finite()) {
        return;
    }
    const SEG: usize = 32;
    const EXP: f32 = 2.6;
    let zc = if f.z_axis.is_finite() { f.z_axis } else { 0.0 };
    let pr = fuselage_profile(f, 36);
    let sx = |t: f32| t.signum() * t.abs().powf(2.0 / EXP);
    let mut rows = Vec::with_capacity(pr.len());
    let mut refs = Vec::with_capacity(pr.len());
    for (i, &(x, r)) in pr.iter().enumerate() {
        let (xa, ra) = pr[i.saturating_sub(1)];
        let (xb, rb) = pr[(i + 1).min(pr.len() - 1)];
        let slope = if xb > xa { (rb - ra) / (xb - xa) } else { 0.0 };
        let r = r.max(1e-4);
        let (mut ring, mut rf) = (Vec::with_capacity(SEG), Vec::with_capacity(SEG));
        for j in 0..SEG {
            let t = TAU * j as f32 / SEG as f32;
            let (c, s) = (t.cos(), t.sin());
            ring.push(Vec3::new(x, r * sx(c), zc + r * 0.92 * sx(s)));
            rf.push(Vec3::new(-slope, c, s));
        }
        rows.push(ring);
        refs.push(rf);
    }
    push_grid(mesh, &rows, &refs, true);
    if let Some(tail) = rows.first() {
        push_flat_cap(mesh, Vec3::new(tail[0].x, 0.0, zc), tail, -Vec3::X);
    }
}

/// Pervane: göbek (başlık) + `blades` adet burulmuş kanat. `phase` ilk kanadın dönüş
/// düzlemindeki açısı. Göbek ve kanatlar `mesh`'e eklenir.
pub fn push_propeller(
    mesh: &mut Mesh,
    center: Vec3,
    axis: Vec3,
    radius: f32,
    blades: u32,
    phase: f32,
) {
    let d = axis.normalized();
    if d == Vec3::ZERO || !center.is_finite() || !finite_pos(radius) {
        return;
    }
    let n = blades.clamp(2, 6);
    let hub_r = (0.06 * radius).clamp(0.003, 0.02).min(radius * 0.3);
    let hub_l = hub_r * 2.2;
    push_lathe_axis(
        mesh,
        center,
        d,
        &[
            (0.0, hub_r),
            (hub_l * 0.35, hub_r),
            (hub_l * 0.75, hub_r * 0.7),
            (hub_l, 0.0),
        ],
        16,
    );
    let (u0, v0) = basis_around(d);
    const STATIONS: usize = 8;
    const COLS: usize = 10;
    let r0 = hub_r * 0.8;
    for b in 0..n {
        let phi = phase + TAU * b as f32 / n as f32;
        let u = u0 * phi.cos() + v0 * phi.sin();
        let w = d.cross(u);
        let mut rows = Vec::with_capacity(STATIONS + 2);
        let mut refs = Vec::with_capacity(STATIONS + 2);
        for k in 0..=STATIONS + 1 {
            // Son satır: ucu neredeyse kapatan küçültülmüş kesit.
            let s = (k.min(STATIONS)) as f32 / STATIONS as f32;
            let rr = r0 + (radius - r0) * s;
            let frac = rr / radius;
            let mut chord = radius * 0.15 * (1.0 - ((frac - 0.45) / 0.65).powi(2)).max(0.2);
            if k > STATIONS {
                chord *= 0.12;
            }
            let beta = (radius / (TAU * rr)).atan().min(0.9);
            let (sb, cb) = beta.sin_cos();
            let cd = w * cb - d * sb;
            let td = d * cb + w * sb;
            let (a, h) = (0.5 * chord, 0.06 * chord);
            let (mut ring, mut rf) = (Vec::with_capacity(COLS), Vec::with_capacity(COLS));
            for j in 0..COLS {
                let t = TAU * j as f32 / COLS as f32;
                let (c, sn) = (t.cos(), t.sin());
                ring.push(center + d * 0.002 + u * rr + cd * (a * c) + td * (h * sn));
                rf.push(cd * (c / a) + td * (sn / h));
            }
            rows.push(ring);
            refs.push(rf);
        }
        push_grid(mesh, &rows, &refs, true);
    }
}

/// Tekerlek: Y ekseni etrafında, orta kısmı şişkin lastik profili.
pub fn push_wheel(mesh: &mut Mesh, center: Vec3, radius: f32, width: f32) {
    if !center.is_finite() || !finite_pos(radius) || !finite_pos(width) {
        return;
    }
    let (r, w) = (radius, width);
    push_lathe_axis(
        mesh,
        center - Vec3::Y * (w * 0.5),
        Vec3::Y,
        &[
            (0.0, 0.0),
            (w * 0.03, r * 0.6),
            (w * 0.12, r * 0.92),
            (w * 0.35, r),
            (w * 0.65, r),
            (w * 0.88, r * 0.92),
            (w * 0.97, r * 0.6),
            (w, 0.0),
        ],
        24,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn fuselage_is_closed_rounded_and_sized() {
        let f = FuselageSpec {
            x_nose: 0.2,
            length: 0.9,
            diameter: 0.11,
            z_axis: -0.02,
            structure: None,
        };
        let mut m = Mesh::new();
        push_fuselage(&mut m, &f);
        assert!(m.is_valid() && !m.is_empty());
        let (a, b) = m.bounds().unwrap();
        assert!(approx(b.x, 0.2, 1e-4) && approx(a.x, -0.7, 1e-4));
        assert!(approx(b.y - a.y, 0.11, 2e-3), "genişlik {}", b.y - a.y);
        // Yükseklik genişlikten küçük, eksen z_axis civarında ortalı.
        assert!(approx(b.z - a.z, 0.11 * 0.92, 2e-3));
        assert!(approx((a.z + b.z) * 0.5, -0.02, 1e-3));
        // Kutu değil: kesit köşesi (|y|,|z| = tam yarıçap) dışarıda kalmaz.
        let corner = m
            .positions
            .iter()
            .filter(|p| p[0] > -0.3 && p[0] < 0.0)
            .map(|p| (p[1] * p[1] + (p[2] + 0.02).powi(2)).sqrt())
            .fold(0.0, f32::max);
        assert!(corner < 0.055 * 1.05, "{corner}");
    }

    #[test]
    fn propeller_has_blades_within_radius() {
        let mut m2 = Mesh::new();
        push_propeller(&mut m2, Vec3::new(1.0, 0.0, 0.0), Vec3::X, 0.1, 2, 0.0);
        let mut m3 = Mesh::new();
        push_propeller(&mut m3, Vec3::new(1.0, 0.0, 0.0), Vec3::X, 0.1, 3, 0.0);
        assert!(m2.is_valid() && m3.is_valid());
        assert!(m3.triangle_count() > m2.triangle_count());
        let (a, b) = m2.bounds().unwrap();
        // İki kanat Y/Z düzleminde yaklaşık yarıçap kadar uzanır, eksen boyunca ince kalır.
        let reach = (b.y - a.y).max(b.z - a.z);
        assert!(reach > 0.18 && reach < 0.21, "{reach}");
        assert!(b.x - a.x < 0.05);
        // Kanat sayısı sınırlanır.
        let mut m9 = Mesh::new();
        push_propeller(&mut m9, Vec3::ZERO, Vec3::Z, 0.1, 9, 0.0);
        let mut m6 = Mesh::new();
        push_propeller(&mut m6, Vec3::ZERO, Vec3::Z, 0.1, 6, 0.0);
        assert_eq!(m9.triangle_count(), m6.triangle_count());
    }

    #[test]
    fn bad_inputs_add_nothing() {
        let mut m = Mesh::new();
        push_propeller(&mut m, Vec3::ZERO, Vec3::ZERO, 0.1, 2, 0.0);
        push_propeller(&mut m, Vec3::ZERO, Vec3::Z, f32::NAN, 2, 0.0);
        push_wheel(&mut m, Vec3::ZERO, -1.0, 0.1);
        push_wheel(&mut m, Vec3::new(f32::NAN, 0.0, 0.0), 0.1, 0.1);
        assert!(m.is_empty());
    }

    #[test]
    fn wheel_is_a_disc_around_y() {
        let mut m = Mesh::new();
        push_wheel(&mut m, Vec3::new(0.0, 0.3, 0.1), 0.04, 0.02);
        assert!(m.is_valid());
        let (a, b) = m.bounds().unwrap();
        assert!(approx(b.y - a.y, 0.02, 1e-4) && approx((a.y + b.y) * 0.5, 0.3, 1e-4));
        assert!(approx(b.x - a.x, 0.08, 1e-3) && approx(b.z - a.z, 0.08, 1e-3));
    }
}
