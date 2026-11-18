// SPDX-License-Identifier: AGPL-3.0-or-later
//! 2B düzlem gerilme sonlu elemanı: 4 düğümlü çift doğrusal (Q4) kare eleman, düzgün ızgara,
//! bantlı Cholesky çözücü. Eleman matrisi Andreassen ve ark. (2011) "88 satır" kodundakiyle
//! aynıdır (birim E, birim kalınlık; kare eleman için boyuttan bağımsız).
//!
//! Düzen: düğüm (ix, iy), y yukarı; eleman (ex, ey) numarası `ey * nx + ex`; eleman düğümleri
//! saat yönünün tersine sol-alt, sağ-alt, sağ-üst, sol-üst. Düğüm numaralandırması kısa kenar
//! boyunca sütun sütun gider (bant genişliği küçük kalır).

/// 88 satır koduyla aynı 8x8 düzlem gerilme eleman rijitliği (E = 1, kalınlık = 1).
pub fn element_stiffness(nu: f64) -> [[f64; 8]; 8] {
    let k = [
        1.0 / 2.0 - nu / 6.0,
        1.0 / 8.0 + nu / 8.0,
        -1.0 / 4.0 - nu / 12.0,
        -1.0 / 8.0 + 3.0 * nu / 8.0,
        -1.0 / 4.0 + nu / 12.0,
        -1.0 / 8.0 - nu / 8.0,
        nu / 6.0,
        1.0 / 8.0 - 3.0 * nu / 8.0,
    ];
    // Satır başına k indeksleri (0 tabanlı).
    const IDX: [[usize; 8]; 8] = [
        [0, 1, 2, 3, 4, 5, 6, 7],
        [1, 0, 7, 6, 5, 4, 3, 2],
        [2, 7, 0, 5, 6, 3, 4, 1],
        [3, 6, 5, 0, 7, 2, 1, 4],
        [4, 5, 6, 7, 0, 1, 2, 3],
        [5, 4, 3, 2, 1, 0, 7, 6],
        [6, 3, 4, 1, 2, 7, 0, 5],
        [7, 2, 1, 4, 3, 6, 5, 0],
    ];
    let mut ke = [[0.0; 8]; 8];
    let f = 1.0 / (1.0 - nu * nu);
    for (i, row) in IDX.iter().enumerate() {
        for (j, &ix) in row.iter().enumerate() {
            ke[i][j] = f * k[ix];
        }
    }
    ke
}

/// Izgara geometrisi ve numaralandırma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub nx: usize,
    pub ny: usize,
}

impl Grid {
    pub fn n_elem(&self) -> usize {
        self.nx * self.ny
    }
    pub fn n_node(&self) -> usize {
        (self.nx + 1) * (self.ny + 1)
    }
    pub fn n_dof(&self) -> usize {
        2 * self.n_node()
    }
    /// Düğüm numarası (ix: 0..=nx, iy: 0..=ny).
    pub fn node(&self, ix: usize, iy: usize) -> usize {
        if self.ny <= self.nx {
            ix * (self.ny + 1) + iy
        } else {
            iy * (self.nx + 1) + ix
        }
    }
    /// Eleman numarası.
    pub fn elem(&self, ex: usize, ey: usize) -> usize {
        ey * self.nx + ex
    }
    /// Elemanın 8 serbestlik derecesi (sol-alt, sağ-alt, sağ-üst, sol-üst; her biri x, y).
    pub fn edofs(&self, ex: usize, ey: usize) -> [usize; 8] {
        let n = [
            self.node(ex, ey),
            self.node(ex + 1, ey),
            self.node(ex + 1, ey + 1),
            self.node(ex, ey + 1),
        ];
        [
            2 * n[0],
            2 * n[0] + 1,
            2 * n[1],
            2 * n[1] + 1,
            2 * n[2],
            2 * n[2] + 1,
            2 * n[3],
            2 * n[3] + 1,
        ]
    }
}

/// Bantlı Cholesky ile çözen, sabit mesnet kümesi için hazırlanmış çözücü.
pub struct Solver {
    pub grid: Grid,
    /// dof -> serbest dof sırası (`usize::MAX`: sabit).
    free_of: Vec<usize>,
    n_free: usize,
    /// Alt bant genişliği (köşegen hariç).
    bw: usize,
}

impl Solver {
    pub fn new(grid: Grid, fixed: &[bool]) -> Self {
        assert_eq!(fixed.len(), grid.n_dof());
        let mut free_of = vec![usize::MAX; grid.n_dof()];
        let mut n_free = 0;
        for (d, &f) in fixed.iter().enumerate() {
            if !f {
                free_of[d] = n_free;
                n_free += 1;
            }
        }
        let mut bw = 0;
        for ey in 0..grid.ny {
            for ex in 0..grid.nx {
                let mut lo = usize::MAX;
                let mut hi = 0;
                for d in grid.edofs(ex, ey) {
                    let f = free_of[d];
                    if f != usize::MAX {
                        lo = lo.min(f);
                        hi = hi.max(f);
                    }
                }
                if lo != usize::MAX {
                    bw = bw.max(hi - lo);
                }
            }
        }
        Solver {
            grid,
            free_of,
            n_free,
            bw,
        }
    }

    pub fn n_free(&self) -> usize {
        self.n_free
    }

    pub fn bandwidth(&self) -> usize {
        self.bw
    }

    /// K u = f çözümü. `ke` = kalınlıkla çarpılmış, E = 1 eleman matrisi; `e_scale[e]` elemanın
    /// elastisite modülü (Pa). Dönüş: tüm serbestlik derecelerinde yer değiştirme (sabitler 0).
    pub fn solve(
        &self,
        ke: &[[f64; 8]; 8],
        e_scale: &[f64],
        f: &[f64],
    ) -> Result<Vec<f64>, String> {
        let g = self.grid;
        assert_eq!(e_scale.len(), g.n_elem());
        let n = self.n_free;
        if n == 0 {
            return Ok(vec![0.0; g.n_dof()]);
        }
        let w = self.bw + 1;
        // Satır i, sütun k (i-bw <= k <= i) -> a[i*w + k + bw - i].
        let mut a = vec![0.0f64; n * w];
        for ey in 0..g.ny {
            for ex in 0..g.nx {
                let s = e_scale[g.elem(ex, ey)];
                let dofs = g.edofs(ex, ey);
                for (r, &dr) in dofs.iter().enumerate() {
                    let fr = self.free_of[dr];
                    if fr == usize::MAX {
                        continue;
                    }
                    for (c, &dc) in dofs.iter().enumerate() {
                        let fc = self.free_of[dc];
                        if fc == usize::MAX || fc > fr {
                            continue;
                        }
                        a[fr * w + fc + self.bw - fr] += s * ke[r][c];
                    }
                }
            }
        }
        let mut b = vec![0.0f64; n];
        for (d, &fr) in self.free_of.iter().enumerate() {
            if fr != usize::MAX {
                b[fr] = f[d];
            }
        }
        cholesky_band(&mut a, n, self.bw)?;
        solve_band(&a, n, self.bw, &mut b);
        let mut u = vec![0.0; g.n_dof()];
        for (d, &fr) in self.free_of.iter().enumerate() {
            if fr != usize::MAX {
                u[d] = b[fr];
            }
        }
        Ok(u)
    }
}

/// Yerinde bantlı Cholesky (A = L Lᵀ), alt üçgen bant depolama.
fn cholesky_band(a: &mut [f64], n: usize, bw: usize) -> Result<(), String> {
    let w = bw + 1;
    for i in 0..n {
        let j0 = i.saturating_sub(bw);
        for j in j0..=i {
            let k0 = j0.max(j.saturating_sub(bw));
            let len = j - k0;
            let mut sum = a[i * w + j + bw - i];
            if len > 0 {
                let si = i * w + k0 + bw - i;
                let sj = j * w + k0 + bw - j;
                let mut d = 0.0;
                for q in 0..len {
                    d += a[si + q] * a[sj + q];
                }
                sum -= d;
            }
            if i == j {
                if sum.is_nan() || sum <= 0.0 || !sum.is_finite() {
                    return Err("not_pd".into());
                }
                a[i * w + bw] = sum.sqrt();
            } else {
                a[i * w + j + bw - i] = sum / a[j * w + bw];
            }
        }
    }
    Ok(())
}

fn solve_band(l: &[f64], n: usize, bw: usize, b: &mut [f64]) {
    let w = bw + 1;
    for i in 0..n {
        let k0 = i.saturating_sub(bw);
        let mut s = b[i];
        for k in k0..i {
            s -= l[i * w + k + bw - i] * b[k];
        }
        b[i] = s / l[i * w + bw];
    }
    for i in (0..n).rev() {
        b[i] /= l[i * w + bw];
        let k0 = i.saturating_sub(bw);
        let bi = b[i];
        for k in k0..i {
            b[k] -= l[i * w + k + bw - i] * bi;
        }
    }
}

/// Elemanın 8 dof yer değiştirmesi.
pub fn element_u(g: &Grid, u: &[f64], ex: usize, ey: usize) -> [f64; 8] {
    let d = g.edofs(ex, ey);
    let mut ue = [0.0; 8];
    for (k, &dk) in d.iter().enumerate() {
        ue[k] = u[dk];
    }
    ue
}

/// ueᵀ KE ue
pub fn energy(ke: &[[f64; 8]; 8], ue: &[f64; 8]) -> f64 {
    let mut s = 0.0;
    for i in 0..8 {
        let mut r = 0.0;
        for j in 0..8 {
            r += ke[i][j] * ue[j];
        }
        s += ue[i] * r;
    }
    s
}

/// Eleman merkezinde düzlem gerilme: (σx, σy, τxy) [Pa]. `h` eleman kenarı (m), `e` modül (Pa).
pub fn centre_stress(ue: &[f64; 8], h: f64, e: f64, nu: f64) -> [f64; 3] {
    // Düğüm yerel koordinatları (ξ, η) = (-1,-1), (1,-1), (1,1), (-1,1).
    const XI: [f64; 4] = [-1.0, 1.0, 1.0, -1.0];
    const ETA: [f64; 4] = [-1.0, -1.0, 1.0, 1.0];
    let (mut ex, mut ey, mut gxy) = (0.0, 0.0, 0.0);
    for i in 0..4 {
        let dnx = XI[i] / (2.0 * h);
        let dny = ETA[i] / (2.0 * h);
        ex += dnx * ue[2 * i];
        ey += dny * ue[2 * i + 1];
        gxy += dny * ue[2 * i] + dnx * ue[2 * i + 1];
    }
    let c = e / (1.0 - nu * nu);
    [
        c * (ex + nu * ey),
        c * (nu * ex + ey),
        c * (1.0 - nu) / 2.0 * gxy,
    ]
}

/// Düzlem gerilme von Mises eşdeğeri.
pub fn von_mises(s: [f64; 3]) -> f64 {
    (s[0] * s[0] - s[0] * s[1] + s[1] * s[1] + 3.0 * s[2] * s[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ke_symmetric_and_matches_reference() {
        let ke = element_stiffness(0.3);
        for i in 0..8 {
            for j in 0..8 {
                assert!((ke[i][j] - ke[j][i]).abs() < 1e-14);
            }
        }
        // 88 satır koduyla (nu = 0.3) bilinen değerler.
        assert!((ke[0][0] - 0.494505).abs() < 1e-5);
        assert!((ke[0][1] - 0.178571).abs() < 1e-5);
        assert!((ke[0][2] + 0.302198).abs() < 1e-5);
        assert!((ke[0][3] + 0.013736).abs() < 1e-5);
        // Rijit cisim: öteleme ve küçük dönme sıfır enerji verir.
        let tx = [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        assert!(energy(&ke, &tx).abs() < 1e-12);
        // Düğümler (0,0), (1,0), (1,1), (0,1); u = (-y, x).
        let rot = [0.0, 0.0, 0.0, 1.0, -1.0, 1.0, -1.0, 0.0];
        assert!(energy(&ke, &rot).abs() < 1e-12);
    }

    /// Dolu konsol kiriş uç yer değiştirmesi: Timoshenko (Euler-Bernoulli + kayma) ile uyum.
    #[test]
    fn cantilever_tip_matches_beam_theory() {
        let (nx, ny) = (80usize, 8usize);
        let g = Grid { nx, ny };
        let (e, nu, t, h) = (70.0e9, 0.3, 0.002, 0.001);
        let mut fixed = vec![false; g.n_dof()];
        for iy in 0..=ny {
            let n = g.node(0, iy);
            fixed[2 * n] = true;
            fixed[2 * n + 1] = true;
        }
        let mut f = vec![0.0; g.n_dof()];
        let p = 10.0;
        // Uç kenarına düzgün dağılmış yük, aşağı doğru.
        for iy in 0..=ny {
            let w = if iy == 0 || iy == ny { 0.5 } else { 1.0 };
            f[2 * g.node(nx, iy) + 1] = -p * w / ny as f64;
        }
        let ke = element_stiffness(nu).map(|r| r.map(|v| v * t));
        let s = Solver::new(g, &fixed);
        let u = s.solve(&ke, &vec![e; g.n_elem()], &f).unwrap();
        let mut tip = 0.0;
        for iy in 0..=ny {
            tip += u[2 * g.node(nx, iy) + 1] / (ny + 1) as f64;
        }
        let (l, hh) = (nx as f64 * h, ny as f64 * h);
        let i_ = t * hh.powi(3) / 12.0;
        let eb = p * l.powi(3) / (3.0 * e * i_);
        let gmod = e / (2.0 * (1.0 + nu));
        let shear = p * l / (5.0 / 6.0 * gmod * t * hh);
        let tim = eb + shear;
        let ratio = -tip / tim;
        assert!(
            ratio > 0.9 && ratio < 1.05,
            "ratio {ratio}, eb {eb}, tim {tim}"
        );
        assert!(-tip / eb > 0.9 && -tip / eb < 1.25);
    }
}
