// SPDX-License-Identifier: AGPL-3.0-or-later
//! Klasik 2B SIMP yoğunluk yöntemi (Andreassen ve ark. 2011, "88 satır"): yoğunluk filtresi,
//! optimallik ölçütü (OC) güncellemesi, pasif (katı/boş) bölgeler. Tek iş parçacıklı ve
//! belirlenimcidir (aynı girdi aynı sonucu verir).

use super::fem::{self, Grid, Solver};
use std::sync::atomic::{AtomicBool, Ordering};

/// Izgara hücresinin rolü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Parçanın dışı (malzeme yok, hacme sayılmaz).
    Outside,
    /// Parça içinde sabit boşluk (cıvata deliği, kiriş deliği).
    Hole,
    /// Sabit katı (delik çevresi halkası, kenar bandı, bağlantı alanı).
    Solid,
    /// Tasarım bölgesi.
    Design,
}

/// Çözülecek 2B problem (ızgara üzerinde).
#[derive(Clone, Debug)]
pub struct Problem {
    pub grid: Grid,
    /// Hücre kenarı [m].
    pub h_m: f64,
    /// Izgara sol-alt köşesinin parça koordinatlarındaki yeri [mm].
    pub origin_mm: [f64; 2],
    pub cells: Vec<Cell>,
    /// Sabit serbestlik dereceleri.
    pub fixed: Vec<bool>,
    /// Düğüm yükleri [N] (yük çarpanı dâhil).
    pub loads: Vec<f64>,
}

/// Malzeme özellikleri (SI).
#[derive(Clone, Copy, Debug)]
pub struct MatProps {
    pub e: f64,
    pub nu: f64,
    pub rho: f64,
    /// İzin verilen gerilme [Pa] (tasarım değeri, yaklaşık).
    pub sigma_allow: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct SimpParams {
    pub vol_frac: f64,
    pub penal: f64,
    /// Süzgeç yarıçapı [hücre].
    pub rmin: f64,
    pub thickness_m: f64,
    pub mat: MatProps,
    pub max_iter: usize,
    pub tol: f64,
    pub move_limit: f64,
}

impl SimpParams {
    pub fn new(mat: MatProps, thickness_m: f64) -> Self {
        SimpParams {
            vol_frac: 0.5,
            penal: 3.0,
            rmin: 2.0,
            thickness_m,
            mat,
            max_iter: 100,
            tol: 0.01,
            move_limit: 0.2,
        }
    }
}

/// Her yinelemede (ilerleme/iptal için) çağrılan bilgi.
pub struct IterInfo<'a> {
    pub iter: usize,
    pub compliance: f64,
    pub vol_frac: f64,
    pub change: f64,
    pub density: &'a [f64],
}

#[derive(Clone, Debug)]
pub struct SimpResult {
    /// Fiziksel yoğunluk (Outside/Hole = 0, Solid = 1).
    pub density: Vec<f64>,
    /// Yineleme başına uyum (compliance) [J].
    pub history: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
    pub cancelled: bool,
    /// Parçanın (delikler çıkarılmış, hafifletmeden önce) kütlesi [kg].
    pub mass_before_kg: f64,
    /// Eşiklenmiş son yerleşimin kütlesi [kg].
    pub mass_after_kg: f64,
    /// Son yerleşimde (eşiklenmiş) katı hacim oranı (tasarım + sabit katı / parça).
    pub volume_fraction: f64,
    /// Eşiklenmiş yerleşimin uyumu [J].
    pub compliance_final: f64,
    /// Eleman gerilmesinden von Mises: en büyük ve %99'luk değer [Pa] (yaklaşık).
    pub vm_max: f64,
    pub vm_p99: f64,
    /// İzin verilen gerilme / %99'luk von Mises.
    pub safety_factor: f64,
    /// Eşiklenmiş yerleşimde yük yolu sağlam mı (uyum, eşiklemeden önceki uyumun birkaç katını aşmadı).
    pub load_path_ok: bool,
}

/// Yoğunluk süzgeci: her tasarım elemanı için (komşu, ağırlık) listesi (CSR).
struct Filter {
    start: Vec<usize>,
    nb: Vec<u32>,
    w: Vec<f64>,
    /// Satır ağırlık toplamı.
    hs: Vec<f64>,
}

fn build_filter(grid: &Grid, cells: &[Cell], rmin: f64) -> Filter {
    let n = grid.n_elem();
    let r = rmin.ceil() as isize;
    let mut start = vec![0usize; n + 1];
    let (mut nb, mut w) = (Vec::new(), Vec::new());
    let mut hs = vec![0.0; n];
    let in_dom = |c: Cell| matches!(c, Cell::Design | Cell::Solid);
    for ey in 0..grid.ny {
        for ex in 0..grid.nx {
            let e = grid.elem(ex, ey);
            if cells[e] == Cell::Design {
                let mut sum = 0.0;
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (qx, qy) = (ex as isize + dx, ey as isize + dy);
                        if qx < 0 || qy < 0 || qx >= grid.nx as isize || qy >= grid.ny as isize {
                            continue;
                        }
                        let q = grid.elem(qx as usize, qy as usize);
                        if !in_dom(cells[q]) {
                            continue;
                        }
                        let d = ((dx * dx + dy * dy) as f64).sqrt();
                        let wt = rmin - d;
                        if wt > 0.0 {
                            nb.push(q as u32);
                            w.push(wt);
                            sum += wt;
                        }
                    }
                }
                hs[e] = sum;
            }
            start[e + 1] = nb.len();
        }
    }
    Filter { start, nb, w, hs }
}

impl Filter {
    fn apply(&self, cells: &[Cell], x: &[f64], out: &mut [f64]) {
        for (e, c) in cells.iter().enumerate() {
            out[e] = match c {
                Cell::Design => {
                    let mut s = 0.0;
                    for k in self.start[e]..self.start[e + 1] {
                        s += self.w[k] * x[self.nb[k] as usize];
                    }
                    s / self.hs[e]
                }
                Cell::Solid => 1.0,
                _ => 0.0,
            };
        }
    }
}

const E_MIN_RATIO: f64 = 1e-9;
const X_MIN: f64 = 1e-3;

/// Problemi çözer. `cancel` true olursa o ana dek yoğunlukla `cancelled` sonuç döner.
pub fn optimize(
    prob: &Problem,
    par: &SimpParams,
    cancel: &AtomicBool,
    on_iter: &mut dyn FnMut(&IterInfo),
) -> Result<SimpResult, String> {
    let grid = prob.grid;
    let n = grid.n_elem();
    if prob.cells.len() != n || prob.fixed.len() != grid.n_dof() || prob.loads.len() != grid.n_dof()
    {
        return Err("size".into());
    }
    let nd = prob.cells.iter().filter(|c| **c == Cell::Design).count();
    let ns = prob.cells.iter().filter(|c| **c == Cell::Solid).count();
    if nd == 0 {
        return Err("no_design".into());
    }
    if !prob.loads.iter().any(|f| *f != 0.0) || !prob.loads.iter().all(|f| f.is_finite()) {
        return Err("no_load".into());
    }
    if !(par.thickness_m > 0.0 && par.mat.e > 0.0 && par.rmin >= 1.0 && par.penal >= 1.0) {
        return Err("bad_params".into());
    }
    let vf = par.vol_frac.clamp(0.05, 0.95);
    let target = vf * (nd + ns) as f64 - ns as f64;
    if target < 0.02 * nd as f64 {
        return Err("vol_low".into());
    }
    let ke = fem::element_stiffness(par.mat.nu).map(|r| r.map(|v| v * par.thickness_m));
    let solver = Solver::new(grid, &prob.fixed);
    let filt = build_filter(&grid, &prob.cells, par.rmin);
    let (e0, emin) = (par.mat.e, par.mat.e * E_MIN_RATIO);
    let p = par.penal;

    // Süzgeç devriği için dv (hacim duyarlılığı) — sabit.
    let mut dv = vec![0.0; n];
    for e in 0..n {
        if prob.cells[e] != Cell::Design {
            continue;
        }
        for k in filt.start[e]..filt.start[e + 1] {
            dv[filt.nb[k] as usize] += filt.w[k] / filt.hs[e];
        }
    }

    let mut x = vec![0.0; n];
    for (e, c) in prob.cells.iter().enumerate() {
        x[e] = match c {
            Cell::Design => target / nd as f64,
            Cell::Solid => 1.0,
            _ => 0.0,
        };
    }
    let mut xp = vec![0.0; n];
    filt.apply(&prob.cells, &x, &mut xp);
    let mut history = Vec::new();
    let mut converged = false;
    let mut cancelled = false;
    let mut e_el = vec![0.0; n];
    let mut dc = vec![0.0; n];
    let mut dcx = vec![0.0; n];
    let mut xnew = x.clone();
    let mut tmp = vec![0.0; n];
    let mut iter = 0;
    while iter < par.max_iter {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        iter += 1;
        for e in 0..n {
            e_el[e] = match prob.cells[e] {
                Cell::Design | Cell::Solid => emin + xp[e].powf(p) * (e0 - emin),
                _ => emin,
            };
        }
        let u = solver.solve(&ke, &e_el, &prob.loads)?;
        let mut c = 0.0;
        for ey in 0..grid.ny {
            for ex in 0..grid.nx {
                let e = grid.elem(ex, ey);
                let ue = fem::element_u(&grid, &u, ex, ey);
                let en = fem::energy(&ke, &ue);
                c += e_el[e] * en;
                dc[e] = -p * xp[e].powf(p - 1.0) * (e0 - emin) * en;
            }
        }
        if !c.is_finite() {
            return Err("non_finite".into());
        }
        history.push(c);
        // Ölçekleme (OC, SI birimlerinden bağımsız olsun).
        let mut dmax = 0.0f64;
        for e in 0..n {
            if prob.cells[e] == Cell::Design {
                dmax = dmax.max(dc[e].abs());
            }
        }
        let dscale = if dmax > 0.0 { 1.0 / dmax } else { 1.0 };
        dcx.iter_mut().for_each(|v| *v = 0.0);
        for e in 0..n {
            if prob.cells[e] != Cell::Design {
                continue;
            }
            let g = dc[e] * dscale / filt.hs[e];
            for k in filt.start[e]..filt.start[e + 1] {
                dcx[filt.nb[k] as usize] += filt.w[k] * g;
            }
        }
        // OC güncellemesi (ikili arama ile hacim kısıtı).
        let (mut l1, mut l2) = (0.0f64, 1.0e9f64);
        let mut guard = 0;
        while (l2 - l1) / (l1 + l2) > 1e-5 && guard < 200 {
            guard += 1;
            let lmid = 0.5 * (l1 + l2);
            for e in 0..n {
                if prob.cells[e] != Cell::Design {
                    xnew[e] = x[e];
                    continue;
                }
                let ratio = (-dcx[e] / dv[e].max(1e-12) / lmid).max(0.0).sqrt();
                let lo = (x[e] - par.move_limit).max(X_MIN);
                let hi = (x[e] + par.move_limit).min(1.0);
                xnew[e] = (x[e] * ratio).clamp(lo, hi);
            }
            filt.apply(&prob.cells, &xnew, &mut tmp);
            let vol: f64 = (0..n)
                .filter(|&e| prob.cells[e] == Cell::Design)
                .map(|e| tmp[e])
                .sum();
            if vol > target {
                l1 = lmid;
            } else {
                l2 = lmid;
            }
        }
        let mut change = 0.0f64;
        for e in 0..n {
            if prob.cells[e] == Cell::Design {
                change = change.max((xnew[e] - x[e]).abs());
            }
        }
        x.copy_from_slice(&xnew);
        filt.apply(&prob.cells, &x, &mut xp);
        let vol: f64 = (0..n)
            .filter(|&e| prob.cells[e] == Cell::Design)
            .map(|e| xp[e])
            .sum();
        on_iter(&IterInfo {
            iter,
            compliance: c,
            vol_frac: (vol + ns as f64) / (nd + ns) as f64,
            change,
            density: &xp,
        });
        if change < par.tol && iter >= 10 {
            converged = true;
            break;
        }
    }
    Ok(finalize(
        prob, par, &solver, &ke, xp, history, iter, converged, cancelled,
    ))
}

/// Eşiklenmiş uyum / eşiklenmemiş uyum bu oranı aşarsa yük yolu kopmuş sayılır.
const LOAD_PATH_RATIO: f64 = 100.0;

#[allow(clippy::too_many_arguments)]
fn finalize(
    prob: &Problem,
    par: &SimpParams,
    solver: &Solver,
    ke: &[[f64; 8]; 8],
    density: Vec<f64>,
    history: Vec<f64>,
    iterations: usize,
    converged: bool,
    cancelled: bool,
) -> SimpResult {
    let grid = prob.grid;
    let n = grid.n_elem();
    let (e0, emin) = (par.mat.e, par.mat.e * E_MIN_RATIO);
    let bin = binary_layout(&prob.cells, &density);
    let n_part = prob
        .cells
        .iter()
        .filter(|c| matches!(c, Cell::Design | Cell::Solid))
        .count();
    let n_solid = bin.iter().filter(|b| **b).count();
    let cell_mass = prob.h_m * prob.h_m * par.thickness_m * par.mat.rho;
    let mut res = SimpResult {
        density,
        history,
        iterations,
        converged,
        cancelled,
        mass_before_kg: n_part as f64 * cell_mass,
        mass_after_kg: n_solid as f64 * cell_mass,
        volume_fraction: n_solid as f64 / n_part.max(1) as f64,
        compliance_final: f64::NAN,
        vm_max: f64::NAN,
        vm_p99: f64::NAN,
        safety_factor: f64::NAN,
        load_path_ok: true,
    };
    let e_el: Vec<f64> = bin.iter().map(|b| if *b { e0 } else { emin }).collect();
    if let Ok(u) = solver.solve(ke, &e_el, &prob.loads) {
        let mut c = 0.0;
        let mut vms = Vec::new();
        for ey in 0..grid.ny {
            for ex in 0..grid.nx {
                let e = grid.elem(ex, ey);
                let ue = fem::element_u(&grid, &u, ex, ey);
                c += e_el[e] * fem::energy(ke, &ue);
                if bin[e] {
                    let s = fem::centre_stress(&ue, prob.h_m, e0, par.mat.nu);
                    vms.push(fem::von_mises(s));
                }
            }
        }
        res.compliance_final = c;
        // Eşikleme yük yolunu kopardıysa katı adalar neredeyse hiç gerilmez; güvenlik katsayısı yanıltır.
        if let Some(c0) = res.history.last() {
            if *c0 > 0.0 && c > LOAD_PATH_RATIO * *c0 {
                res.load_path_ok = false;
            }
        }
        if res.load_path_ok && !vms.is_empty() {
            vms.sort_by(f64::total_cmp);
            res.vm_max = vms[vms.len() - 1];
            let k = ((vms.len() as f64) * 0.99).ceil() as usize;
            res.vm_p99 = vms[k.clamp(1, vms.len()) - 1];
            if res.vm_p99 > 0.0 {
                res.safety_factor = par.mat.sigma_allow / res.vm_p99;
            }
        }
    }
    let _ = n;
    res
}

/// Eşiklenmiş (0.5) katı katman: katı sabit hücreler + eşiği geçen tasarım hücreleri.
pub fn binary_layout(cells: &[Cell], density: &[f64]) -> Vec<bool> {
    cells
        .iter()
        .zip(density)
        .map(|(c, d)| match c {
            Cell::Solid => true,
            Cell::Design => *d > 0.5,
            _ => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Konsol problemi: sol kenar sabit, sağ kenar ortasına aşağı yük.
    pub(crate) fn cantilever(nx: usize, ny: usize) -> (Problem, SimpParams) {
        let grid = Grid { nx, ny };
        let h = 0.001;
        let mut fixed = vec![false; grid.n_dof()];
        for iy in 0..=ny {
            let n = grid.node(0, iy);
            fixed[2 * n] = true;
            fixed[2 * n + 1] = true;
        }
        let mut loads = vec![0.0; grid.n_dof()];
        loads[2 * grid.node(nx, ny / 2) + 1] = -50.0;
        let prob = Problem {
            grid,
            h_m: h,
            origin_mm: [0.0, 0.0],
            cells: vec![Cell::Design; grid.n_elem()],
            fixed,
            loads,
        };
        let mat = MatProps {
            e: 60.0e9,
            nu: 0.3,
            rho: 1550.0,
            sigma_allow: 300.0e6,
        };
        let mut par = SimpParams::new(mat, 0.002);
        par.vol_frac = 0.4;
        par.rmin = 1.5;
        par.max_iter = 40;
        (prob, par)
    }

    #[test]
    fn reaches_target_volume_and_compliance_drops() {
        let (prob, par) = cantilever(30, 15);
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let r = optimize(&prob, &par, &cancel, &mut |i| {
            seen += 1;
            assert_eq!(i.density.len(), 30 * 15);
        })
        .unwrap();
        assert_eq!(seen, r.iterations);
        // Fiziksel yoğunluk toplamı hedef hacim oranında.
        let vol: f64 = r.density.iter().sum::<f64>() / r.density.len() as f64;
        assert!((vol - 0.4).abs() < 0.01, "vol {vol}");
        let (first, last) = (r.history[0], *r.history.last().unwrap());
        assert!(last < 0.7 * first, "{first} -> {last}");
        // Yaklaşık tekdüze azalma: her adımda en çok %5 artış.
        for w in r.history.windows(2) {
            assert!(w[1] < w[0] * 1.05, "{:?}", r.history);
        }
        assert!(r.mass_after_kg < r.mass_before_kg);
        assert!(r.vm_p99.is_finite() && r.safety_factor > 0.0);
    }

    #[test]
    fn severed_load_path_withholds_safety_factor() {
        // Tek yineleme + düşük hacim oranı: yoğunluklar 0.5 altında kalır, eşikleme her şeyi boşaltır.
        let (prob, mut par) = cantilever(30, 15);
        par.vol_frac = 0.25;
        par.max_iter = 1;
        let r = optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(
            !r.load_path_ok,
            "c={} h={:?}",
            r.compliance_final, r.history
        );
        assert!(r.safety_factor.is_nan());
        // Normal koşuda yük yolu sağlam.
        let (prob, par) = cantilever(30, 15);
        let r = optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert!(r.load_path_ok && r.safety_factor.is_finite());
    }

    #[test]
    fn deterministic() {
        let (prob, mut par) = cantilever(20, 10);
        par.max_iter = 15;
        let c = AtomicBool::new(false);
        let a = optimize(&prob, &par, &c, &mut |_| {}).unwrap();
        let b = optimize(&prob, &par, &c, &mut |_| {}).unwrap();
        assert_eq!(a.density, b.density);
        assert_eq!(a.history, b.history);
    }

    #[test]
    fn cancel_stops_early() {
        let (prob, par) = cantilever(20, 10);
        let cancel = AtomicBool::new(false);
        let r = optimize(&prob, &par, &cancel, &mut |i| {
            if i.iter == 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();
        assert!(r.cancelled);
        assert_eq!(r.iterations, 3);
    }

    #[test]
    fn passive_regions_respected() {
        let (mut prob, mut par) = cantilever(24, 12);
        prob.cells[5 * 24 + 10] = Cell::Hole;
        prob.cells[6 * 24 + 10] = Cell::Solid;
        par.max_iter = 10;
        let r = optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(r.density[5 * 24 + 10], 0.0);
        assert_eq!(r.density[6 * 24 + 10], 1.0);
    }

    #[test]
    fn rejects_bad_input() {
        let (mut prob, par) = cantilever(10, 5);
        prob.loads.iter_mut().for_each(|v| *v = 0.0);
        assert!(optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).is_err());
    }
}
