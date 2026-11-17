// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small dense symmetric generalised eigenproblem solver, `K x = lambda M x`.
//!
//! `M` is reduced with a Cholesky factorisation (`M = L L^T`), the standard problem
//! `A y = lambda y` with `A = L^-1 K L^-T` is solved with the cyclic Jacobi method and the
//! eigenvectors are mapped back with `x = L^-T y`. Intended for the few-dozen-DOF beam models
//! used by the structural analysis; no external dependency.

/// Cholesky factor (lower triangular, row-major) of a symmetric positive definite matrix.
/// Returns `None` if the matrix is not positive definite or contains non-finite values.
pub fn cholesky(a: &[f64], n: usize) -> Option<Vec<f64>> {
    if a.len() != n * n || a.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if s <= 0.0 {
                    return None;
                }
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    Some(l)
}

/// Eigenvalues (ascending) and eigenvectors of a symmetric matrix by cyclic Jacobi rotations.
/// The returned vectors are columns of a row-major `n x n` matrix, ordered like the values.
pub fn jacobi_symmetric(a: &[f64], n: usize) -> (Vec<f64>, Vec<f64>) {
    let mut a = a.to_vec();
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    let norm: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-300);
    for _sweep in 0..100 {
        let mut off = 0.0;
        for i in 0..n {
            for j in (i + 1)..n {
                off += a[i * n + j] * a[i * n + j];
            }
        }
        if off.sqrt() <= 1e-15 * norm {
            break;
        }
        for p in 0..n {
            for q in (p + 1)..n {
                let apq = a[p * n + q];
                if apq.abs() <= 1e-300 {
                    continue;
                }
                let app = a[p * n + p];
                let aqq = a[q * n + q];
                let theta = (aqq - app) / (2.0 * apq);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let akp = a[k * n + p];
                    let akq = a[k * n + q];
                    a[k * n + p] = c * akp - s * akq;
                    a[k * n + q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let apk = a[p * n + k];
                    let aqk = a[q * n + k];
                    a[p * n + k] = c * apk - s * aqk;
                    a[q * n + k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let vkp = v[k * n + p];
                    let vkq = v[k * n + q];
                    v[k * n + p] = c * vkp - s * vkq;
                    v[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&x, &y| a[x * n + x].total_cmp(&a[y * n + y]));
    let vals: Vec<f64> = idx.iter().map(|&i| a[i * n + i]).collect();
    let mut vecs = vec![0.0; n * n];
    for (col, &i) in idx.iter().enumerate() {
        for r in 0..n {
            vecs[r * n + col] = v[r * n + i];
        }
    }
    (vals, vecs)
}

/// Solves `K x = lambda M x` for symmetric `K` and symmetric positive definite `M`.
/// Returns ascending eigenvalues and mass-normalised eigenvectors (columns, row-major), or
/// `None` if `M` is not positive definite.
pub fn generalized(k: &[f64], m: &[f64], n: usize) -> Option<(Vec<f64>, Vec<f64>)> {
    if n == 0 || k.len() != n * n || k.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let l = cholesky(m, n)?;
    // B = L^-1 K (forward substitution column by column), then A = B L^-T = L^-1 (L^-1 K)^T.
    let lower_solve = |rhs: &mut [f64]| {
        for i in 0..n {
            let mut s = rhs[i];
            for kk in 0..i {
                s -= l[i * n + kk] * rhs[kk];
            }
            rhs[i] = s / l[i * n + i];
        }
    };
    let mut b = vec![0.0; n * n];
    let mut col = vec![0.0; n];
    for j in 0..n {
        for i in 0..n {
            col[i] = k[i * n + j];
        }
        lower_solve(&mut col);
        for i in 0..n {
            b[i * n + j] = col[i];
        }
    }
    // A = L^-1 B^T (B^T = K L^-T since K is symmetric).
    let mut a = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            col[i] = b[j * n + i];
        }
        lower_solve(&mut col);
        for i in 0..n {
            a[i * n + j] = col[i];
        }
    }
    // Symmetrise round-off.
    for i in 0..n {
        for j in (i + 1)..n {
            let s = 0.5 * (a[i * n + j] + a[j * n + i]);
            a[i * n + j] = s;
            a[j * n + i] = s;
        }
    }
    let (vals, y) = jacobi_symmetric(&a, n);
    // x = L^-T y (back substitution per column).
    let mut x = vec![0.0; n * n];
    for c in 0..n {
        for i in (0..n).rev() {
            let mut s = y[i * n + c];
            for kk in (i + 1)..n {
                s -= l[kk * n + i] * x[kk * n + c];
            }
            x[i * n + c] = s / l[i * n + i];
        }
    }
    Some((vals, x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_dof_spring_mass() {
        // Two unit masses, springs k=1 to ground and between: eigenvalues 1 and 3.
        let k = [2.0, -1.0, -1.0, 2.0];
        let m = [1.0, 0.0, 0.0, 1.0];
        let (vals, _) = generalized(&k, &m, 2).unwrap();
        assert!((vals[0] - 1.0).abs() < 1e-12);
        assert!((vals[1] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn mass_scaling_and_singular_mass() {
        let k = [4.0, 0.0, 0.0, 9.0];
        let m = [4.0, 0.0, 0.0, 1.0];
        let (vals, _) = generalized(&k, &m, 2).unwrap();
        assert!((vals[0] - 1.0).abs() < 1e-12 && (vals[1] - 9.0).abs() < 1e-12);
        assert!(generalized(&k, &[1.0, 0.0, 0.0, 0.0], 2).is_none());
    }
}
