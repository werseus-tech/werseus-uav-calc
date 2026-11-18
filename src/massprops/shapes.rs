// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rigid-body building blocks for the mass-properties estimate: point mass, thin rod, thin
//! rectangular plate, solid box, solid cylinder, solid ellipsoid, thin disc and a trapezoidal
//! wing lamina. Every [`Body`] stores its mass, centre of mass and the inertia tensor **about its
//! own centre of mass** in the common body axes; bodies are combined with the parallel-axis
//! theorem (Steiner): `I = I_cm + m (|d|^2 E - d d^T)`.
//!
//! Closed-form moments used (e.g. Beer & Johnston, Vector Mechanics for Engineers, Appendix B;
//! Hibbeler, Engineering Mechanics: Dynamics, inside back cover):
//! - slender rod, length L: `I = m L^2 / 12` about any axis through the centre normal to the rod
//! - thin rectangular plate a x w: `m w^2/12`, `m a^2/12` in-plane, `m (a^2 + w^2)/12` normal
//! - solid box lx x ly x lz: `m (ly^2 + lz^2)/12` about the x axis (cyclic)
//! - solid cylinder, radius r, length L: `m r^2/2` axial, `m (3 r^2 + L^2)/12` transverse
//! - solid ellipsoid, semi-axes a, b, c: `m (b^2 + c^2)/5` about the a axis (cyclic)
//! - thin disc, radius r: `m r^2/2` about its normal, `m r^2/4` in-plane

pub type V3 = [f64; 3];
pub type M3 = [[f64; 3]; 3];

pub const ZERO_M3: M3 = [[0.0; 3]; 3];

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

pub fn add_m(a: &M3, b: &M3) -> M3 {
    let mut r = ZERO_M3;
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = a[i][j] + b[i][j];
        }
    }
    r
}

/// Parallel-axis term `m (|d|^2 E - d d^T)` for a mass `m` offset by `d` from the reference point.
pub fn parallel_axis(m: f64, d: V3) -> M3 {
    let d2 = dot(d, d);
    let mut r = ZERO_M3;
    for (i, row) in r.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            let delta = if i == j { d2 } else { 0.0 };
            *v = m * (delta - d[i] * d[j]);
        }
    }
    r
}

/// Outer-product sum `sum_k lambda_k e_k e_k^T` (principal moments in an orthonormal basis).
fn principal(l: [f64; 3], e: [V3; 3]) -> M3 {
    let mut r = ZERO_M3;
    for k in 0..3 {
        for (i, row) in r.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v += l[k] * e[k][i] * e[k][j];
            }
        }
    }
    r
}

/// A rigid body: mass, centre of mass and inertia tensor about that centre (body axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub mass: f64,
    pub cg: V3,
    pub inertia: M3,
}

impl Body {
    /// The same body moved rigidly by `delta` (inertia about the own centre is unchanged).
    pub fn translated(mut self, delta: V3) -> Body {
        for (c, d) in self.cg.iter_mut().zip(delta) {
            *c += d;
        }
        self
    }

    /// The same body moved so that its centre of mass sits at `pos`.
    pub fn placed_at(self, pos: V3) -> Body {
        let d = [
            pos[0] - self.cg[0],
            pos[1] - self.cg[1],
            pos[2] - self.cg[2],
        ];
        let mut b = self.translated(d);
        // Exactly the requested position, also when an internal centroid was not finite.
        b.cg = pos;
        b
    }
}

/// Point mass (zero own inertia).
pub fn point(mass: f64, pos: V3) -> Body {
    Body {
        mass,
        cg: pos,
        inertia: ZERO_M3,
    }
}

/// Thin rod of length `len` along the unit vector `dir`: `I = m L^2/12 (E - u u^T)`.
pub fn rod(mass: f64, len: f64, dir: V3, pos: V3) -> Body {
    let k = mass * len * len / 12.0;
    let mut inertia = ZERO_M3;
    for (i, row) in inertia.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            let delta = if i == j { 1.0 } else { 0.0 };
            *v = k * (delta - dir[i] * dir[j]);
        }
    }
    Body {
        mass,
        cg: pos,
        inertia,
    }
}

/// Thin rectangular plate: `a` along body x, `w` along the unit vector `u` (perpendicular to x).
pub fn plate_rect(mass: f64, a: f64, w: f64, u: V3, pos: V3) -> Body {
    let ex = [1.0, 0.0, 0.0];
    let n = cross(ex, u);
    Body {
        mass,
        cg: pos,
        inertia: principal(
            [
                mass * w * w / 12.0,
                mass * a * a / 12.0,
                mass * (a * a + w * w) / 12.0,
            ],
            [ex, u, n],
        ),
    }
}

/// Solid box with edge lengths `l` along x, y, z.
pub fn solid_box(mass: f64, l: V3, pos: V3) -> Body {
    let k = mass / 12.0;
    Body {
        mass,
        cg: pos,
        inertia: [
            [k * (l[1] * l[1] + l[2] * l[2]), 0.0, 0.0],
            [0.0, k * (l[0] * l[0] + l[2] * l[2]), 0.0],
            [0.0, 0.0, k * (l[0] * l[0] + l[1] * l[1])],
        ],
    }
}

/// Solid circular cylinder with its axis along body x.
pub fn cylinder_x(mass: f64, len: f64, dia: f64, pos: V3) -> Body {
    let r2 = dia * dia / 4.0;
    let ix = mass * r2 / 2.0;
    let it = mass * (3.0 * r2 + len * len) / 12.0;
    Body {
        mass,
        cg: pos,
        inertia: [[ix, 0.0, 0.0], [0.0, it, 0.0], [0.0, 0.0, it]],
    }
}

/// Solid ellipsoid with semi-axes `s` along x, y, z.
pub fn ellipsoid(mass: f64, s: V3, pos: V3) -> Body {
    let k = mass / 5.0;
    Body {
        mass,
        cg: pos,
        inertia: [
            [k * (s[1] * s[1] + s[2] * s[2]), 0.0, 0.0],
            [0.0, k * (s[0] * s[0] + s[2] * s[2]), 0.0],
            [0.0, 0.0, k * (s[0] * s[0] + s[1] * s[1])],
        ],
    }
}

/// Thin disc in the x-y plane (normal along z).
pub fn disc_z(mass: f64, radius: f64, pos: V3) -> Body {
    let q = mass * radius * radius / 4.0;
    Body {
        mass,
        cg: pos,
        inertia: [[q, 0.0, 0.0], [0.0, q, 0.0], [0.0, 0.0, 2.0 * q]],
    }
}

/// Combines bodies: total mass, mass-weighted centre and the inertia about that centre via the
/// parallel-axis theorem. An empty or massless set returns a zero body.
pub fn combine(parts: &[Body]) -> Body {
    let mass: f64 = parts.iter().map(|b| b.mass).sum();
    if mass <= 0.0 || !mass.is_finite() {
        return Body {
            mass: 0.0,
            cg: [0.0; 3],
            inertia: ZERO_M3,
        };
    }
    let mut cg = [0.0; 3];
    for b in parts {
        for (c, p) in cg.iter_mut().zip(b.cg) {
            *c += b.mass * p / mass;
        }
    }
    let mut inertia = ZERO_M3;
    for b in parts {
        let d = [b.cg[0] - cg[0], b.cg[1] - cg[1], b.cg[2] - cg[2]];
        inertia = add_m(&inertia, &add_m(&b.inertia, &parallel_axis(b.mass, d)));
    }
    Body { mass, cg, inertia }
}

/// Number of spanwise strips per semi-span of the wing lamina.
pub const WING_STRIPS: usize = 120;

/// Uniform trapezoidal wing lamina (planform in the x-y plane, x aft, y right, thickness
/// neglected), built from `2 * WING_STRIPS` rectangular strips whose own plate moments are
/// added through the parallel-axis theorem. The chord comes from the existing planform
/// ([`crate::calc::geometry::chord_at`]); the leading edge follows `|y| tan(sweep)`. The returned
/// body has its centre of mass at the area centroid; callers move it with [`Body::placed_at`].
pub fn wing_lamina(
    mass: f64,
    wing: &crate::calc::geometry::WingOut,
    span: f64,
    sweep_le_deg: f64,
) -> Body {
    let half = span * 0.5;
    let dy = half / WING_STRIPS as f64;
    let tan_le = match sweep_le_deg.to_radians().tan() {
        t if t.is_finite() => t,
        _ => 0.0,
    };
    let mut strips: Vec<(f64, V3, f64)> = Vec::with_capacity(2 * WING_STRIPS);
    let mut area = 0.0;
    for side in [-1.0, 1.0] {
        for k in 0..WING_STRIPS {
            let y = (k as f64 + 0.5) * dy;
            let c = crate::calc::geometry::chord_at(wing, span, y).max(0.0);
            let x_mid = y * tan_le + 0.5 * c;
            strips.push((c * dy, [x_mid, side * y, 0.0], c));
            area += c * dy;
        }
    }
    if area <= 0.0 || !area.is_finite() {
        return point(mass, [0.0; 3]);
    }
    let parts: Vec<Body> = strips
        .iter()
        .map(|(a, pos, c)| plate_rect(mass * a / area, *c, dy, [0.0, 1.0, 0.0], *pos))
        .collect();
    combine(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    #[test]
    fn rod_matches_closed_form_and_steiner() {
        let (m, l) = (0.3, 0.8);
        let r = rod(m, l, [0.0, 1.0, 0.0], [0.0; 3]);
        assert!(rel(r.inertia[0][0], m * l * l / 12.0) < 1e-9);
        assert!(rel(r.inertia[2][2], m * l * l / 12.0) < 1e-9);
        assert!(r.inertia[1][1].abs() < 1e-15);
        // About an end: m L^2 / 3.
        let about_end = add_m(&r.inertia, &parallel_axis(m, [0.0, l / 2.0, 0.0]));
        assert!(rel(about_end[0][0], m * l * l / 3.0) < 1e-9);
    }

    #[test]
    fn plate_matches_closed_form() {
        let (m, a, w) = (0.5, 0.2, 0.9);
        let p = plate_rect(m, a, w, [0.0, 1.0, 0.0], [0.0; 3]);
        assert!(rel(p.inertia[0][0], m * w * w / 12.0) < 1e-9);
        assert!(rel(p.inertia[1][1], m * a * a / 12.0) < 1e-9);
        assert!(rel(p.inertia[2][2], m * (a * a + w * w) / 12.0) < 1e-9);
        // Perpendicular-axis theorem for a lamina.
        assert!(rel(p.inertia[2][2], p.inertia[0][0] + p.inertia[1][1]) < 1e-9);
    }

    #[test]
    fn box_cylinder_ellipsoid_disc_closed_forms() {
        let b = solid_box(2.0, [0.3, 0.2, 0.1], [0.0; 3]);
        assert!(rel(b.inertia[0][0], 2.0 / 12.0 * (0.04 + 0.01)) < 1e-9);
        let c = cylinder_x(1.5, 0.6, 0.1, [0.0; 3]);
        assert!(rel(c.inertia[0][0], 1.5 * 0.05 * 0.05 / 2.0) < 1e-9);
        assert!(rel(c.inertia[1][1], 1.5 * (3.0 * 0.0025 + 0.36) / 12.0) < 1e-9);
        let e = ellipsoid(1.0, [0.5, 0.1, 0.1], [0.0; 3]);
        assert!(rel(e.inertia[0][0], 0.02 / 5.0) < 1e-9);
        assert!(rel(e.inertia[1][1], (0.25 + 0.01) / 5.0) < 1e-9);
        let d = disc_z(2.0, 0.1, [0.0; 3]);
        assert!(rel(d.inertia[2][2], 2.0 * 0.01 / 2.0) < 1e-9);
        assert!(rel(d.inertia[0][0], 2.0 * 0.01 / 4.0) < 1e-9);
    }

    #[test]
    fn two_point_masses_dumbbell() {
        let b = combine(&[point(1.0, [0.0, -0.5, 0.0]), point(1.0, [0.0, 0.5, 0.0])]);
        assert!(rel(b.mass, 2.0) < 1e-12);
        assert!(b.cg.iter().all(|c| c.abs() < 1e-15));
        assert!(rel(b.inertia[0][0], 0.5) < 1e-9);
        assert!(rel(b.inertia[2][2], 0.5) < 1e-9);
        assert!(b.inertia[1][1].abs() < 1e-15);
    }

    #[test]
    fn rectangular_wing_lamina_equals_plate() {
        let wing = crate::calc::geometry::wing(&crate::calc::geometry::WingIn {
            b: 1.6,
            s: 0.32,
            taper: 1.0,
            sweep_le_deg: 0.0,
        });
        let l = wing_lamina(0.8, &wing, 1.6, 0.0);
        let c = 0.2;
        assert!(rel(l.inertia[0][0], 0.8 * 1.6 * 1.6 / 12.0) < 1e-9);
        assert!(rel(l.inertia[1][1], 0.8 * c * c / 12.0) < 1e-9);
        assert!(rel(l.cg[0], c / 2.0) < 1e-9);
        assert!(l.cg[1].abs() < 1e-12);
    }
}
