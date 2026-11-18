// SPDX-License-Identifier: AGPL-3.0-or-later
//! Düz parçalar için 2B topoloji eniyileme (hafifletme). Parça dış hatları ve delikleri
//! `cad::layout::Layout`'tan gelir; yükler projeden (itki, MTOW, yük çarpanı) hesaplanır.
//! Saf kod: arayüz `ui::topology` içindedir. Ham sonuç yalnızca uygulama içi tasarım ön
//! çalışmasıdır; yükler "eşdeğer düzlem içi" basitleştirmelerdir (bkz. `build_problem`).

// Sayısal çekirdekte indeksli döngüler (ızgara/eleman numaraları) bilinçli.
#![allow(clippy::needless_range_loop)]

pub mod contour;
pub mod dxf;
pub mod fem;
pub mod simp;

use crate::cad::freecad::mr_arm_angles;
use crate::cad::layout::Layout;
use crate::calc::Model;
use crate::calc::atmosphere::G;
use crate::project::{DroneType, Project};
use fem::Grid;
use simp::{Cell, MatProps, Problem, SimpParams, SimpResult};

/// Eniyilenecek parça.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Multirotor kolu: gövde ucunda ankastre, uçta motor yükü.
    Arm,
    /// Multirotor merkez plakası: kol cıvata noktalarından yüklü, FC/pil bölgesinden sabit.
    Plate,
    /// Kanat kaburgası: üst kenarda basınç, kiriş deliğinden mesnetli.
    Rib,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Arm, Part::Plate, Part::Rib];

    pub fn label_key(self) -> &'static str {
        match self {
            Part::Arm => "topology.part.arm",
            Part::Plate => "topology.part.plate",
            Part::Rib => "topology.part.rib",
        }
    }

    pub fn default_resolution(self) -> usize {
        match self {
            Part::Arm => 160,
            Part::Plate => 70,
            Part::Rib => 120,
        }
    }
}

/// Bu araç türünde eniyilenebilen parçalar.
pub fn available_parts(dt: DroneType) -> Vec<Part> {
    let mut v = Vec::new();
    if dt == DroneType::Multirotor {
        v.extend([Part::Arm, Part::Plate]);
    }
    if dt.has_wing() {
        v.push(Part::Rib);
    }
    v
}

/// Düz levha malzemesi (kesilerek üretilen parçalar; baskı malzemesi yok).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateMaterial {
    CarbonPlate,
    G10,
    Aluminium,
    Plywood,
}

impl PlateMaterial {
    pub const ALL: [PlateMaterial; 4] = [
        PlateMaterial::CarbonPlate,
        PlateMaterial::G10,
        PlateMaterial::Aluminium,
        PlateMaterial::Plywood,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            PlateMaterial::CarbonPlate => "topology.mat.carbon",
            PlateMaterial::G10 => "topology.mat.g10",
            PlateMaterial::Aluminium => "topology.mat.aluminium",
            PlateMaterial::Plywood => "topology.mat.plywood",
        }
    }

    /// Yaklaşık düzlem içi özellikler. Yoğunluk, malzeme listesinde (`assets/materials.json`)
    /// varsa oradan okunur; E ve izin verilen gerilme tipik katalog değerleridir (yarı-izotrop
    /// karbon levha, G10/FR4, 6061-T6, kavak kontrplak) ve ön tasarım içindir.
    pub fn props(self) -> MatProps {
        let (e, nu, rho, sig, id) = match self {
            PlateMaterial::CarbonPlate => (60.0e9, 0.30, 1550.0, 300.0e6, "carbon_tube_pultruded"),
            PlateMaterial::G10 => (18.0e9, 0.15, 1850.0, 180.0e6, ""),
            PlateMaterial::Aluminium => (69.0e9, 0.33, 2700.0, 240.0e6, "aluminum_6061_tube"),
            PlateMaterial::Plywood => (6.0e9, 0.30, 580.0, 20.0e6, "plywood_poplar_3mm"),
        };
        let rho = crate::calc::weight::load_materials()
            .iter()
            .find(|m| !id.is_empty() && m.id == id)
            .map(|m| m.density_kg_m3)
            .unwrap_or(rho);
        MatProps {
            e,
            nu,
            rho,
            sigma_allow: sig,
        }
    }
}

/// Kullanıcı ayarları.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub part: Part,
    pub material: PlateMaterial,
    /// Parça kalınlığı [mm]; 0 ise yerleşimdeki değer.
    pub thickness_mm: f64,
    pub vol_frac: f64,
    pub penal: f64,
    /// Süzgeç yarıçapı [hücre].
    pub rmin: f64,
    /// En uzun kenardaki hücre sayısı.
    pub resolution: usize,
    /// Yük çarpanı (g); 0 ise projedeki değer.
    pub load_factor: f64,
    pub max_iter: usize,
}

impl Settings {
    pub fn new(part: Part) -> Self {
        Settings {
            part,
            material: PlateMaterial::CarbonPlate,
            thickness_mm: 0.0,
            vol_frac: 0.5,
            penal: 3.0,
            rmin: 2.0,
            resolution: part.default_resolution(),
            load_factor: 0.0,
            max_iter: 100,
        }
    }
}

/// Parçanın yerleşimdeki kalınlığı [mm].
pub fn default_thickness_mm(layout: &Layout, part: Part) -> f64 {
    match part {
        Part::Arm => layout.mr_arm.thickness_mm,
        Part::Plate => layout.mr_plate.thickness_mm,
        Part::Rib => layout.wing.structure().rib_t_mm,
    }
}

/// Projeden gelen yük çarpanı.
pub fn project_load_factor(p: &Project, part: Part) -> f64 {
    let n = match part {
        Part::Arm | Part::Plate => p.mr_arm_n_limit,
        Part::Rib => p.n_limit,
    };
    if n.is_finite() && n > 0.0 { n } else { 2.0 }
}

/// Çözüm ızgarası: hücre sayısı en uzun kenarda `res`, toplam en çok `MAX_CELLS`.
const MAX_CELLS: f64 = 7200.0;

fn make_grid(bbox: [f64; 4], res: usize) -> (Grid, f64, [f64; 2]) {
    let (w, h) = (bbox[2] - bbox[0], bbox[3] - bbox[1]);
    let long = w.max(h);
    let cell = (long / res.clamp(20, 400) as f64)
        .max((w * h / MAX_CELLS).sqrt())
        .max(1e-6);
    let nx = (w / cell).ceil().max(4.0) as usize;
    let ny = (h / cell).ceil().max(4.0) as usize;
    (Grid { nx, ny }, cell, [bbox[0], bbox[1]])
}

/// Izgara üzerinde yardımcı: hücre/düğüm koordinatları ve boyama.
struct Canvas {
    grid: Grid,
    /// Hücre kenarı [mm].
    h: f64,
    origin: [f64; 2],
    cells: Vec<Cell>,
    fixed: Vec<bool>,
    loads: Vec<f64>,
}

impl Canvas {
    fn new(bbox: [f64; 4], res: usize) -> Self {
        let (grid, h, origin) = make_grid(bbox, res);
        Canvas {
            grid,
            h,
            origin,
            cells: vec![Cell::Outside; grid.n_elem()],
            fixed: vec![false; grid.n_dof()],
            loads: vec![0.0; grid.n_dof()],
        }
    }

    fn centre(&self, ex: usize, ey: usize) -> [f64; 2] {
        [
            self.origin[0] + (ex as f64 + 0.5) * self.h,
            self.origin[1] + (ey as f64 + 0.5) * self.h,
        ]
    }

    fn node_pos(&self, ix: usize, iy: usize) -> [f64; 2] {
        [
            self.origin[0] + ix as f64 * self.h,
            self.origin[1] + iy as f64 * self.h,
        ]
    }

    fn for_cells(&mut self, mut f: impl FnMut([f64; 2], Cell) -> Cell) {
        for ey in 0..self.grid.ny {
            for ex in 0..self.grid.nx {
                let e = self.grid.elem(ex, ey);
                let c = self.centre(ex, ey);
                self.cells[e] = f(c, self.cells[e]);
            }
        }
    }

    /// Daire içinde (merkez, yarıçap mm) olan ve `only` koşulunu sağlayan hücreleri boyar.
    fn paint_disc(&mut self, c: [f64; 2], r: f64, what: Cell, only_in_part: bool) {
        self.for_cells(|p, cur| {
            let d = ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt();
            let in_part = matches!(cur, Cell::Design | Cell::Solid);
            if d <= r && (!only_in_part || in_part) {
                what
            } else {
                cur
            }
        });
    }

    /// Delik + çevresinde sabit katı halka.
    fn hole(&mut self, c: [f64; 2], d_mm: f64, ring_mm: f64) {
        let r = d_mm / 2.0;
        // Delik, hücre merkezleriyle örneklendiği için en az bir hücre çapında olmalı.
        let r_eff = r.max(0.5 * self.h);
        self.paint_disc(c, r_eff + ring_mm.max(self.h), Cell::Solid, true);
        self.paint_disc(c, r_eff, Cell::Hole, false);
    }

    /// Düğümleri (merkez, yarıçap) içinde olanlar.
    fn nodes_in(&self, c: [f64; 2], r: f64) -> Vec<usize> {
        let mut v = Vec::new();
        for iy in 0..=self.grid.ny {
            for ix in 0..=self.grid.nx {
                let q = self.node_pos(ix, iy);
                if ((q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2)).sqrt() <= r {
                    v.push(self.grid.node(ix, iy));
                }
            }
        }
        v
    }

    /// Düğümü bölgede ve en az bir komşu elemanı parçanın içinde olan düğümler.
    fn live_nodes(&self, nodes: Vec<usize>) -> Vec<usize> {
        let mut live = vec![false; self.grid.n_node()];
        for ey in 0..self.grid.ny {
            for ex in 0..self.grid.nx {
                if matches!(
                    self.cells[self.grid.elem(ex, ey)],
                    Cell::Design | Cell::Solid
                ) {
                    for (dx, dy) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                        live[self.grid.node(ex + dx, ey + dy)] = true;
                    }
                }
            }
        }
        nodes.into_iter().filter(|n| live[*n]).collect()
    }

    fn fix(&mut self, nodes: &[usize]) {
        for n in nodes {
            self.fixed[2 * n] = true;
            self.fixed[2 * n + 1] = true;
        }
    }

    /// Kuvveti (fx, fy) [N] düğümlere eşit paylaştırır.
    fn force(&mut self, nodes: &[usize], fx: f64, fy: f64) {
        if nodes.is_empty() {
            return;
        }
        let k = 1.0 / nodes.len() as f64;
        for n in nodes {
            self.loads[2 * n] += fx * k;
            self.loads[2 * n + 1] += fy * k;
        }
    }

    /// Merkez etrafında düzlem içi moment (rijit dönme dağılımı), toplam kuvvet sıfır.
    fn moment(&mut self, nodes: &[usize], c: [f64; 2], mz: f64) {
        let mut pos = Vec::new();
        for iy in 0..=self.grid.ny {
            for ix in 0..=self.grid.nx {
                pos.push((self.grid.node(ix, iy), self.node_pos(ix, iy)));
            }
        }
        let mut sel: Vec<(usize, [f64; 2])> =
            pos.into_iter().filter(|(n, _)| nodes.contains(n)).collect();
        if sel.is_empty() {
            return;
        }
        // Net kuvveti sıfırlamak için merkez, düğümlerin ağırlık merkezine kaydırılır.
        let _ = c;
        let m = sel.len() as f64;
        let gc = [
            sel.iter().map(|s| s.1[0]).sum::<f64>() / m,
            sel.iter().map(|s| s.1[1]).sum::<f64>() / m,
        ];
        let r2: f64 = sel
            .iter()
            .map(|s| (s.1[0] - gc[0]).powi(2) + (s.1[1] - gc[1]).powi(2))
            .sum();
        if r2 < 1e-12 {
            return;
        }
        // Moment [N·m] -> N·mm.
        let q = mz * 1000.0 / r2;
        for (n, p) in sel.drain(..) {
            self.loads[2 * n] += -q * (p[1] - gc[1]);
            self.loads[2 * n + 1] += q * (p[0] - gc[0]);
        }
    }

    fn finish(self) -> Problem {
        Problem {
            grid: self.grid,
            h_m: self.h * 1e-3,
            origin_mm: self.origin,
            cells: self.cells,
            fixed: self.fixed,
            loads: self.loads,
        }
    }
}

fn in_poly(poly: &[[f64; 2]], x: f64, y: f64) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_poly(poly: &[[f64; 2]], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 1e-18 {
            0.0
        } else {
            (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0)
        };
        let d = ((x - a[0] - t * dx).powi(2) + (y - a[1] - t * dy).powi(2)).sqrt();
        best = best.min(d);
    }
    best
}

fn finite_pos(v: f64, what: &str) -> Result<f64, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("bad_geometry:{what}"))
    }
}

/// Parça ve proje değerlerinden problem kurar.
///
/// Yük modelleri (düzlem içi eşdeğer, ön tasarım):
/// - Kol: uç motor merkezinde itki × yük çarpanı yanal kuvvet (en elverişsiz yönelim varsayımı)
///   ve itkiyle orantılı motor reaksiyon torku (Q ≈ 0.06·D_pervane·T) düzlem içi moment olarak.
/// - Plaka: her kol için kol momenti (T·n·L), iki kol cıvatası arasında kuvvet çifti olarak
///   (tek cıvata varsa itki kuvveti), ayrıca dış cıvatada dönüşümlü işaretli tork kuvveti;
///   FC/yığın cıvata halkalarından sabit.
/// - Kaburga: n · W/S basıncı × kaburga aralığı, üst yüzey düğümlerine yukarı doğru; kiriş
///   deliği çevresinden sabit; dış kenarda yapışma bandı katı.
pub fn build_problem(
    p: &Project,
    m: &Model,
    layout: &Layout,
    s: &Settings,
) -> Result<Problem, String> {
    let n = if s.load_factor > 0.0 {
        s.load_factor
    } else {
        project_load_factor(p, s.part)
    };
    match s.part {
        Part::Arm => {
            let a = &layout.mr_arm;
            let (l, w) = (
                finite_pos(a.arm_length_mm, "arm length")?,
                a.width_mm.max(4.0),
            );
            let t_max = finite_pos(p.mr_t_max_motor, "motor thrust")?;
            let mut cv = Canvas::new([0.0, -w / 2.0, l, w / 2.0], s.resolution);
            cv.for_cells(|c, _| {
                if c[0] >= 0.0 && c[0] <= l && c[1].abs() <= w / 2.0 {
                    Cell::Design
                } else {
                    Cell::Outside
                }
            });
            let mc = [l - w / 2.0, 0.0];
            let ring = 2.0 * a.chassis_bolt_dia_mm.max(1.0) / 2.0;
            // Motor tablası: tam katı disk, orta mil deliği ve cıvata delikleri.
            cv.paint_disc(mc, w / 2.0, Cell::Solid, true);
            let rb = (a.motor_mount_pattern_mm / 2.0)
                .min(w / 2.0 - a.motor_bolt_dia_mm)
                .max(a.motor_shaft_dia_mm / 2.0 + a.motor_bolt_dia_mm);
            cv.paint_disc(mc, a.motor_shaft_dia_mm / 2.0, Cell::Hole, false);
            for k in 0..4 {
                let ang = (45.0 + 90.0 * k as f64).to_radians();
                let c = [mc[0] + rb * ang.cos(), mc[1] + rb * ang.sin()];
                cv.paint_disc(c, a.motor_bolt_dia_mm / 2.0, Cell::Hole, false);
            }
            // Gövde cıvata delikleri.
            let n_bolt = if a.chassis_bolt_spacing_mm > 0.0 {
                2
            } else {
                1
            };
            for i in 0..n_bolt {
                let x = a.chassis_hole_x0_mm + i as f64 * a.chassis_bolt_spacing_mm;
                cv.hole([x, 0.0], a.chassis_bolt_dia_mm, ring);
            }
            // Gövde ucunda ankastre.
            let root: Vec<usize> = (0..=cv.grid.ny).map(|iy| cv.grid.node(0, iy)).collect();
            let root = cv.live_nodes(root);
            cv.fix(&root);
            // Uç yükü: motor tablasındaki düğümlere.
            let pad = cv.live_nodes(cv.nodes_in(mc, w / 2.0 * 0.8));
            cv.force(&pad, 0.0, -t_max * n);
            let q = 0.06 * p.mr_d_prop.max(0.05) * t_max * n;
            cv.moment(&pad, mc, q);
            Ok(cv.finish())
        }
        Part::Plate => {
            let pl = &layout.mr_plate;
            let r = finite_pos(pl.plate_radius_mm, "plate radius")?;
            let a = &layout.mr_arm;
            let t_max = finite_pos(p.mr_t_max_motor, "motor thrust")?;
            let mut cv = Canvas::new([-r, -r, r, r], s.resolution);
            cv.for_cells(|c, _| {
                if (c[0] * c[0] + c[1] * c[1]).sqrt() <= r {
                    Cell::Design
                } else {
                    Cell::Outside
                }
            });
            let ring = pl.chassis_bolt_dia_mm.max(1.0);
            cv.hole([0.0, 0.0], pl.center_hole_dia_mm, ring);
            let f = pl.fc_mount_pattern_mm / 2.0;
            let mut fc_nodes = Vec::new();
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let c = [sx * f, sy * f];
                cv.hole(c, 3.2, ring);
                fc_nodes.extend(cv.nodes_in(c, 3.2 / 2.0 + ring.max(cv.h)));
            }
            let angles = mr_arm_angles(pl.layout, pl.arm_count);
            let l_arm = a.arm_length_mm;
            let [r_in, r_out] = pl.arm_hole_r_mm;
            let two = r_out - r_in > 1e-6;
            let kq = 0.06 * p.mr_d_prop.max(0.05);
            let mut arm_loads: Vec<(Vec<usize>, [f64; 2])> = Vec::new();
            for (k, ang) in angles.iter().enumerate() {
                let (sa, ca) = ang.to_radians().sin_cos();
                let at = |rr: f64| [rr * ca, rr * sa];
                cv.hole(at(r_in), pl.chassis_bolt_dia_mm, ring);
                if two {
                    cv.hole(at(r_out), pl.chassis_bolt_dia_mm, ring);
                }
                let rad = ring + pl.chassis_bolt_dia_mm / 2.0;
                let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                let tq = sign * kq * t_max * n / (r_out * 1e-3);
                let ni = cv.nodes_in(at(r_in), rad);
                let no = cv.nodes_in(at(r_out), rad);
                if two {
                    let fm = t_max * n * l_arm / (r_out - r_in);
                    arm_loads.push((ni, [-fm * ca, -fm * sa]));
                    arm_loads.push((no.clone(), [fm * ca, fm * sa]));
                } else {
                    arm_loads.push((no.clone(), [t_max * n * ca, t_max * n * sa]));
                }
                arm_loads.push((no, [-tq * sa, tq * ca]));
            }
            fc_nodes.sort_unstable();
            fc_nodes.dedup();
            let fc_nodes = cv.live_nodes(fc_nodes);
            cv.fix(&fc_nodes);
            for (nodes, [fx, fy]) in arm_loads {
                let nodes = cv.live_nodes(nodes);
                // Sabit düğümler yükü taşımaz; çakışma olursa yükü sabit olmayanlara ver.
                let free: Vec<usize> = nodes.into_iter().filter(|q| !cv.fixed[2 * q]).collect();
                cv.force(&free, fx, fy);
            }
            Ok(cv.finish())
        }
        Part::Rib => {
            let g = layout.wing.geom();
            let c_root = finite_pos(g.c_root, "root chord")?;
            let poly: Vec<[f64; 2]> = g
                .profile
                .pts
                .iter()
                .map(|q| [q[0] * c_root, q[1] * c_root])
                .collect();
            if poly.len() < 4 {
                return Err("bad_geometry:airfoil".into());
            }
            let (mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY);
            for q in &poly {
                y0 = y0.min(q[1]);
                y1 = y1.max(q[1]);
            }
            let x1 = poly.iter().fold(0.0f64, |a, q| a.max(q[0]));
            let mut cv = Canvas::new([0.0, y0, x1, y1], s.resolution);
            let rim = 2.0f64.max(1.2 * cv.h);
            cv.for_cells(|c, _| {
                if !in_poly(&poly, c[0], c[1]) {
                    Cell::Outside
                } else if dist_poly(&poly, c[0], c[1]) <= rim {
                    Cell::Solid
                } else {
                    Cell::Design
                }
            });
            // Kiriş deliği (kalınlık ortasında, çeyrek kordda).
            let frac = layout.wing.spar_x_frac;
            let mid = 0.5 * (g.profile.yu(frac) + g.profile.yl(frac)) * c_root;
            let thick = g.profile.thickness(frac) * c_root;
            let d_hole = layout.wing.spar_od_mm.min(0.6 * thick).max(2.0);
            let hc = [frac * c_root, mid];
            cv.hole(hc, d_hole, 3.0);
            let sup = cv.live_nodes(cv.nodes_in(hc, d_hole / 2.0 + 0.8 * cv.h));
            cv.fix(&sup);
            // Basınç yükü: üst yüzeydeki her sütunun en üst elemanının üst düğümleri.
            let w_n = m.weight.m_mtow * G;
            if !(w_n > 0.0 && p.s > 0.0) {
                return Err("no_weight".into());
            }
            let q_pa = n * w_n / p.s;
            let pitch_m = crate::cad::structure::WING_RIB_PITCH_MM * 1e-3;
            let f_col = q_pa * pitch_m * cv.h * 1e-3;
            for ex in 0..cv.grid.nx {
                let top = (0..cv.grid.ny).rev().find(|&ey| {
                    matches!(cv.cells[cv.grid.elem(ex, ey)], Cell::Design | Cell::Solid)
                });
                if let Some(ey) = top {
                    let nodes = [cv.grid.node(ex, ey + 1), cv.grid.node(ex + 1, ey + 1)];
                    cv.force(&nodes, 0.0, f_col);
                }
            }
            Ok(cv.finish())
        }
    }
}

/// Ayarlardan SIMP parametreleri.
pub fn simp_params(layout: &Layout, s: &Settings) -> SimpParams {
    let t_mm = if s.thickness_mm > 0.0 {
        s.thickness_mm
    } else {
        default_thickness_mm(layout, s.part)
    };
    let mut par = SimpParams::new(s.material.props(), t_mm * 1e-3);
    par.vol_frac = s.vol_frac;
    par.penal = s.penal;
    par.rmin = s.rmin;
    par.max_iter = s.max_iter;
    par
}

/// Eşiklenmiş yerleşimin kesim konturları (mm, parça koordinatları), kapalı çokgenler.
pub fn cut_loops(prob: &Problem, res: &SimpResult) -> Vec<Vec<[f64; 2]>> {
    let g = prob.grid;
    let mut field = vec![0.0; g.n_elem()];
    for (e, c) in prob.cells.iter().enumerate() {
        field[e] = match c {
            Cell::Solid => 1.0,
            Cell::Design => res.density[e],
            _ => 0.0,
        };
    }
    let h = prob.h_m * 1e3;
    contour::marching_squares(&field, g.nx, g.ny, 0.5)
        .iter()
        .map(|l| contour::simplify_closed(l, 0.2))
        .map(|l| {
            l.iter()
                .map(|q| [prob.origin_mm[0] + q[0] * h, prob.origin_mm[1] + q[1] * h])
                .collect()
        })
        .collect()
}

/// Kesim konturlarının DXF R12 metni.
pub fn export_dxf(prob: &Problem, res: &SimpResult) -> String {
    dxf::write_dxf(&cut_loops(prob, res), "CUT")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use std::sync::atomic::AtomicBool;

    fn fixture(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        (p, m)
    }

    fn quick(part: Part) -> Settings {
        let mut s = Settings::new(part);
        s.resolution = 40;
        s.max_iter = 12;
        s
    }

    #[test]
    fn every_vehicle_part_sets_up_finite() {
        for dt in DroneType::ALL {
            let (p, m) = fixture(dt);
            let layout = Layout::new(&p, &m);
            let parts = available_parts(dt);
            assert!(!parts.is_empty(), "{dt:?}");
            for part in parts {
                let mut s = quick(part);
                if part == Part::Rib {
                    s.resolution = 110;
                    s.max_iter = 6;
                }
                let prob = build_problem(&p, &m, &layout, &s).unwrap_or_else(|e| {
                    panic!("{dt:?} {part:?}: {e}");
                });
                assert!(prob.loads.iter().all(|v| v.is_finite()));
                assert!(prob.loads.iter().any(|v| *v != 0.0), "{dt:?} {part:?}");
                assert!(prob.fixed.iter().any(|v| *v));
                assert!(prob.cells.contains(&Cell::Design));
                let par = simp_params(&layout, &s);
                let r = simp::optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {})
                    .unwrap_or_else(|e| panic!("{dt:?} {part:?}: {e}"));
                assert!(r.density.iter().all(|v| v.is_finite()));
                assert!(r.history.iter().all(|v| v.is_finite() && *v > 0.0));
                assert!(r.mass_before_kg.is_finite() && r.mass_before_kg > 0.0);
                assert!(r.mass_after_kg <= r.mass_before_kg);
                assert!(r.vm_p99.is_finite() && r.safety_factor.is_finite());
                // Pasif bölgeler.
                for (e, c) in prob.cells.iter().enumerate() {
                    match c {
                        Cell::Solid => assert_eq!(r.density[e], 1.0),
                        Cell::Hole | Cell::Outside => assert_eq!(r.density[e], 0.0),
                        Cell::Design => {}
                    }
                }
            }
        }
    }

    #[test]
    fn all_materials_have_sane_props() {
        for mt in PlateMaterial::ALL {
            let p = mt.props();
            assert!(p.e > 1e9 && p.rho > 100.0 && p.sigma_allow > 1e6 && p.nu > 0.0 && p.nu < 0.5);
        }
    }

    #[test]
    fn dxf_export_is_closed_polylines() {
        let (p, m) = fixture(DroneType::Multirotor);
        let layout = Layout::new(&p, &m);
        let s = quick(Part::Plate);
        let prob = build_problem(&p, &m, &layout, &s).unwrap();
        let par = simp_params(&layout, &s);
        let r = simp::optimize(&prob, &par, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let txt = export_dxf(&prob, &r);
        let back = dxf::read_polylines(&txt).unwrap();
        // En az dış hat + merkez deliği.
        assert!(back.len() >= 2, "{}", back.len());
        let r_mm = layout.mr_plate.plate_radius_mm;
        for (pts, closed) in &back {
            assert!(*closed && pts.len() >= 3);
            for q in pts {
                assert!(q[0].abs() <= r_mm + 1.0 && q[1].abs() <= r_mm + 1.0);
            }
        }
        // Dış hat plaka yarıçapına kadar uzanır.
        let ext = back
            .iter()
            .flat_map(|(pts, _)| pts.iter())
            .fold(0.0f64, |a, q| a.max((q[0] * q[0] + q[1] * q[1]).sqrt()));
        assert!(ext > r_mm - 2.0, "{ext} vs {r_mm}");
    }
}
