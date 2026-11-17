// SPDX-License-Identifier: AGPL-3.0-or-later
//! Clamped-free (cantilever) beam finite-element models.
//!
//! * Bending: Euler-Bernoulli two-node Hermite elements (DOFs `w`, `theta` per node) with the
//!   consistent mass matrix. Lumped point masses (and their rotary inertia) sit on nodes.
//! * Torsion: two-node linear shaft elements (DOF `phi`) with consistent polar mass and an
//!   optional lumped polar inertia at the tip.
//!
//! The root node is clamped; the beam axis runs from `x = 0` (root) to `x = length` (tip).
//! Units are SI throughout.

use super::eigen;
use crate::windtunnel::linalg::Lu;
use std::f64::consts::PI;

/// Lumped mass on the beam axis. Snapped to the nearest mesh node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointMass {
    pub x: f64,
    pub mass: f64,
    /// Rotary inertia about the bending axis (kg m^2).
    pub rot_inertia: f64,
}

/// Transverse point force (N, positive "up") at axial position `x`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointForce {
    pub x: f64,
    pub force: f64,
}

/// Uniform cantilever bending model.
#[derive(Clone, Debug)]
pub struct BeamModel {
    pub length: f64,
    pub n_el: usize,
    /// Bending stiffness E*I (N m^2).
    pub ei: f64,
    /// Mass per unit length (kg/m).
    pub mu: f64,
    pub point_masses: Vec<PointMass>,
}

/// One bending mode.
#[derive(Clone, Debug, PartialEq)]
pub struct BeamMode {
    pub freq_hz: f64,
    /// Transverse displacement at every node (root included), scaled to max |w| = 1.
    pub shape: Vec<f64>,
}

/// Static solution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticResult {
    /// Transverse displacement at every node (root included).
    pub w: Vec<f64>,
    /// Bending moment magnitude at every node.
    pub moment: Vec<f64>,
    pub max_deflection: f64,
    pub max_moment: f64,
}

const GAUSS4: [(f64, f64); 4] = [
    (-0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
    (-0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
];

fn element_k(ei: f64, l: f64) -> [[f64; 4]; 4] {
    let c = ei / (l * l * l);
    let l2 = l * l;
    [
        [12.0 * c, 6.0 * l * c, -12.0 * c, 6.0 * l * c],
        [6.0 * l * c, 4.0 * l2 * c, -6.0 * l * c, 2.0 * l2 * c],
        [-12.0 * c, -6.0 * l * c, 12.0 * c, -6.0 * l * c],
        [6.0 * l * c, 2.0 * l2 * c, -6.0 * l * c, 4.0 * l2 * c],
    ]
}

fn element_m(mu: f64, l: f64) -> [[f64; 4]; 4] {
    let c = mu * l / 420.0;
    let l2 = l * l;
    [
        [156.0 * c, 22.0 * l * c, 54.0 * c, -13.0 * l * c],
        [22.0 * l * c, 4.0 * l2 * c, 13.0 * l * c, -3.0 * l2 * c],
        [54.0 * c, 13.0 * l * c, 156.0 * c, -22.0 * l * c],
        [-13.0 * l * c, -3.0 * l2 * c, -22.0 * l * c, 4.0 * l2 * c],
    ]
}

/// Hermite shape functions at local coordinate `xi` in [0, 1].
fn hermite(xi: f64, l: f64) -> [f64; 4] {
    let x2 = xi * xi;
    let x3 = x2 * xi;
    [
        1.0 - 3.0 * x2 + 2.0 * x3,
        l * (xi - 2.0 * x2 + x3),
        3.0 * x2 - 2.0 * x3,
        l * (-x2 + x3),
    ]
}

impl BeamModel {
    fn valid(&self) -> bool {
        self.n_el >= 1
            && self.length.is_finite()
            && self.length > 0.0
            && self.ei.is_finite()
            && self.ei > 0.0
            && self.mu.is_finite()
            && self.mu >= 0.0
    }

    fn le(&self) -> f64 {
        self.length / self.n_el as f64
    }

    /// Node index nearest to axial position `x` (clamped to the mesh).
    pub fn node_at(&self, x: f64) -> usize {
        if !x.is_finite() {
            return self.n_el;
        }
        ((x / self.le()).round().max(0.0) as usize).min(self.n_el)
    }

    /// Axial coordinates of the nodes.
    pub fn node_x(&self) -> Vec<f64> {
        let le = self.le();
        (0..=self.n_el).map(|i| i as f64 * le).collect()
    }

    /// Free-DOF stiffness and mass matrices (root DOFs removed), size `2 n_el`.
    fn assemble(&self) -> (Vec<f64>, Vec<f64>, usize) {
        let nd = 2 * (self.n_el + 1);
        let mut k = vec![0.0; nd * nd];
        let mut m = vec![0.0; nd * nd];
        let le = self.le();
        let ke = element_k(self.ei, le);
        let me = element_m(self.mu, le);
        for e in 0..self.n_el {
            let dofs = [2 * e, 2 * e + 1, 2 * e + 2, 2 * e + 3];
            for a in 0..4 {
                for b in 0..4 {
                    k[dofs[a] * nd + dofs[b]] += ke[a][b];
                    m[dofs[a] * nd + dofs[b]] += me[a][b];
                }
            }
        }
        for pm in &self.point_masses {
            if !(pm.mass.is_finite() && pm.rot_inertia.is_finite()) {
                continue;
            }
            let n = self.node_at(pm.x);
            m[(2 * n) * nd + 2 * n] += pm.mass.max(0.0);
            m[(2 * n + 1) * nd + 2 * n + 1] += pm.rot_inertia.max(0.0);
        }
        let nf = nd - 2;
        let mut kf = vec![0.0; nf * nf];
        let mut mf = vec![0.0; nf * nf];
        for i in 0..nf {
            for j in 0..nf {
                kf[i * nf + j] = k[(i + 2) * nd + j + 2];
                mf[i * nf + j] = m[(i + 2) * nd + j + 2];
            }
        }
        (kf, mf, nf)
    }

    /// The lowest `count` bending modes. Empty if the model is invalid or massless.
    pub fn modes(&self, count: usize) -> Vec<BeamMode> {
        if !self.valid() {
            return Vec::new();
        }
        let (k, m, nf) = self.assemble();
        let Some((vals, vecs)) = eigen::generalized(&k, &m, nf) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (c, &lam) in vals.iter().enumerate() {
            if out.len() >= count {
                break;
            }
            if !(lam.is_finite() && lam > 0.0) {
                continue;
            }
            let mut shape = vec![0.0];
            for node in 1..=self.n_el {
                shape.push(vecs[(2 * node - 2) * nf + c]);
            }
            let peak = shape
                .iter()
                .fold(0.0f64, |a, v| if v.abs() > a.abs() { *v } else { a });
            if peak.abs() > 0.0 {
                for v in &mut shape {
                    *v /= peak;
                }
            }
            out.push(BeamMode {
                freq_hz: lam.sqrt() / (2.0 * PI),
                shape,
            });
        }
        out
    }

    /// Static response to point forces plus a distributed load `q(x)` (N/m, positive up).
    pub fn solve_static(&self, forces: &[PointForce], q: &dyn Fn(f64) -> f64) -> StaticResult {
        if !self.valid() {
            return StaticResult::default();
        }
        let nd = 2 * (self.n_el + 1);
        let le = self.le();
        // Consistent element load vectors.
        let mut fe_all = vec![[0.0f64; 4]; self.n_el];
        let mut f = vec![0.0; nd];
        for (e, fe) in fe_all.iter_mut().enumerate() {
            let x0 = e as f64 * le;
            for (g, wgt) in GAUSS4 {
                let xi = 0.5 * (g + 1.0);
                let qv = q(x0 + xi * le);
                if !qv.is_finite() {
                    continue;
                }
                let n = hermite(xi, le);
                for a in 0..4 {
                    fe[a] += 0.5 * le * wgt * qv * n[a];
                }
            }
            for a in 0..4 {
                f[2 * e + a] += fe[a];
            }
        }
        for pf in forces {
            if pf.force.is_finite() {
                f[2 * self.node_at(pf.x)] += pf.force;
            }
        }
        let (k, _, nf) = self.assemble();
        let Some(lu) = Lu::factor(k, nf) else {
            return StaticResult::default();
        };
        let Some(uf) = lu.solve(&f[2..]) else {
            return StaticResult::default();
        };
        let mut u = vec![0.0, 0.0];
        u.extend_from_slice(&uf);
        // Element end moments from k_e u_e - f_e (point forces act on nodes only).
        let ke = element_k(self.ei, le);
        let mut moment = vec![0.0f64; self.n_el + 1];
        for (e, fe) in fe_all.iter().enumerate() {
            let ue = [u[2 * e], u[2 * e + 1], u[2 * e + 2], u[2 * e + 3]];
            let mut end = [0.0; 4];
            for a in 0..4 {
                end[a] = (0..4).map(|b| ke[a][b] * ue[b]).sum::<f64>() - fe[a];
            }
            moment[e] = moment[e].max(end[1].abs());
            moment[e + 1] = moment[e + 1].max(end[3].abs());
        }
        let w: Vec<f64> = (0..=self.n_el).map(|i| u[2 * i]).collect();
        let max_deflection = w.iter().fold(0.0f64, |a, v| a.max(v.abs()));
        let max_moment = moment.iter().fold(0.0f64, |a, v| a.max(*v));
        StaticResult {
            w,
            moment,
            max_deflection,
            max_moment,
        }
    }
}

/// Uniform cantilever torsion model (St Venant torsion, no warping restraint).
#[derive(Clone, Debug)]
pub struct ShaftModel {
    pub length: f64,
    pub n_el: usize,
    /// Torsional stiffness G*J (N m^2).
    pub gj: f64,
    /// Polar mass moment of inertia per unit length (kg m).
    pub rho_ip: f64,
    /// Lumped polar inertia at the tip about the beam axis (kg m^2).
    pub tip_inertia: f64,
}

impl ShaftModel {
    /// Lowest `count` torsion frequencies (Hz).
    pub fn frequencies(&self, count: usize) -> Vec<f64> {
        let ok = self.n_el >= 1
            && self.length.is_finite()
            && self.length > 0.0
            && self.gj.is_finite()
            && self.gj > 0.0
            && self.rho_ip.is_finite()
            && self.rho_ip >= 0.0
            && self.tip_inertia.is_finite();
        if !ok {
            return Vec::new();
        }
        let n = self.n_el;
        let le = self.length / n as f64;
        let mut k = vec![0.0; n * n];
        let mut m = vec![0.0; n * n];
        let kc = self.gj / le;
        let mc = self.rho_ip * le / 6.0;
        // Free DOFs are nodes 1..=n (index node-1); node 0 clamped.
        for e in 0..n {
            let a = e as isize - 1;
            let b = e;
            let entries = [
                (a, a, kc, 2.0 * mc),
                (a, b as isize, -kc, mc),
                (b as isize, a, -kc, mc),
                (b as isize, b as isize, kc, 2.0 * mc),
            ];
            for (i, j, kv, mv) in entries {
                if i >= 0 && j >= 0 {
                    let (i, j) = (i as usize, j as usize);
                    k[i * n + j] += kv;
                    m[i * n + j] += mv;
                }
            }
        }
        m[(n - 1) * n + n - 1] += self.tip_inertia.max(0.0);
        let Some((vals, _)) = eigen::generalized(&k, &m, n) else {
            return Vec::new();
        };
        vals.into_iter()
            .filter(|l| l.is_finite() && *l > 0.0)
            .take(count)
            .map(|l| l.sqrt() / (2.0 * PI))
            .collect()
    }
}
