// SPDX-License-Identifier: AGPL-3.0-or-later
//! Küçük yoğun doğrusal cebir yardımcıları (kısmi pivotlu LU). Dış bağımlılık yok.

/// Kısmi pivotlu LU ayrışımı (satır-öncelikli n×n matris).
#[derive(Clone, Debug)]
pub struct Lu {
    n: usize,
    lu: Vec<f64>,
    piv: Vec<usize>,
}

impl Lu {
    /// `a`: satır-öncelikli n×n matris. Tekil ya da sonlu olmayan matriste `None` döner.
    pub fn factor(mut a: Vec<f64>, n: usize) -> Option<Lu> {
        if n == 0 || a.len() != n * n || a.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let scale = a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if scale <= 0.0 {
            return None;
        }
        let mut piv: Vec<usize> = (0..n).collect();
        for k in 0..n {
            let mut p = k;
            let mut best = a[k * n + k].abs();
            for r in (k + 1)..n {
                let v = a[r * n + k].abs();
                if v > best {
                    best = v;
                    p = r;
                }
            }
            if best <= scale * 1e-14 {
                return None;
            }
            if p != k {
                for c in 0..n {
                    a.swap(k * n + c, p * n + c);
                }
                piv.swap(k, p);
            }
            let d = a[k * n + k];
            for r in (k + 1)..n {
                let m = a[r * n + k] / d;
                if m != 0.0 {
                    a[r * n + k] = m;
                    let (top, bot) = a.split_at_mut(r * n);
                    let krow = &top[k * n + k + 1..k * n + n];
                    let rrow = &mut bot[k + 1..n];
                    for (x, y) in rrow.iter_mut().zip(krow) {
                        *x -= m * *y;
                    }
                } else {
                    a[r * n + k] = 0.0;
                }
            }
        }
        Some(Lu { n, lu: a, piv })
    }

    /// A·x = b çözümü. `b.len() != n` ise `None`.
    pub fn solve(&self, b: &[f64]) -> Option<Vec<f64>> {
        let n = self.n;
        if b.len() != n {
            return None;
        }
        let mut x: Vec<f64> = self.piv.iter().map(|&p| b[p]).collect();
        for r in 1..n {
            let mut s = x[r];
            for c in 0..r {
                s -= self.lu[r * n + c] * x[c];
            }
            x[r] = s;
        }
        for r in (0..n).rev() {
            let mut s = x[r];
            for c in (r + 1)..n {
                s -= self.lu[r * n + c] * x[c];
            }
            x[r] = s / self.lu[r * n + r];
        }
        if x.iter().all(|v| v.is_finite()) {
            Some(x)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lu_solves_small_system() {
        // 3x3, pivot gerektiren sistem; çözüm (1, -2, 3).
        let a = vec![0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, -1.0, 3.0];
        let b = [-1.0, 2.0, 13.0];
        let x = Lu::factor(a, 3).unwrap().solve(&b).unwrap();
        for (xi, ei) in x.iter().zip([1.0, -2.0, 3.0]) {
            assert!((xi - ei).abs() < 1e-12, "{x:?}");
        }
    }

    #[test]
    fn lu_rejects_singular_and_nan() {
        assert!(Lu::factor(vec![1.0, 2.0, 2.0, 4.0], 2).is_none());
        assert!(Lu::factor(vec![1.0, f64::NAN, 2.0, 4.0], 2).is_none());
        assert!(Lu::factor(vec![], 0).is_none());
    }
}
