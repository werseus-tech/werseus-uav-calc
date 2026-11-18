// SPDX-License-Identifier: AGPL-3.0-or-later
//! 2D Lattice-Boltzmann akış çözücüsü (D2Q9), canlı rüzgâr tüneli görselleştirmesi için.
//!
//! Yöntem:
//! - Çarpışma: düzenlenmiş (regularized) BGK (Latt & Chopard, 2006) + Smagorinsky alt ızgara
//!   modeli (Hou vd., 1996). Düzenleme, dengesizlik kısmını yalnız ikinci moment (Π^neq)
//!   üzerinden yeniden kurar; yüksek mertebeli "hayalet" modları söndürdüğü için τ → 0.5'e
//!   yakın (düşük viskozite, yüksek Re) değerlerde düz BGK'ye göre çok daha kararlıdır.
//!   Smagorinsky, yerel gerilme büyükken etkin τ'yu yükseltir; ızgaranın çözemediği küçük
//!   girdapları sönümler.
//! - Akış (streaming): "çek" (pull) düzeni, çarpışmayla tek döngüde; iki dizi (f, g) arasında
//!   yer değiştirilir. Her hücre bağımsız hesaplandığından satır blokları iş parçacıklarına
//!   bölünür ve sonuç iş parçacığı sayısından bağımsız olarak bit düzeyinde aynıdır.
//! - Katı cisim: yarı-yol geri sekme (halfway bounce-back) — duvar iki düğüm arasındadır,
//!   kaymaz (no-slip) koşul ikinci mertebe doğrulukta sağlanır, kütle tam korunur.
//! - Gözenekli bölge (pervane diski): denge dağılımının hızı (1 − k) ile küçültülür; bu bir
//!   momentum yutağıdır (Brinkman benzeri), kütleyi korur.
//! - Giriş: Zou–He hız sınırı. Çıkış: basınç sınırı (ρ = 1; hız ve dengesizlik kısmı bir
//!   önceki sütundan, geri akış kırpılır). Giriş ve çıkışta emici katmanlar dağılımı serbest
//!   akış dengesine doğru çeker (basınç dalgaları kanal boyunca gidip gelmez); çıkış katmanında
//!   viskozite de artırılır (çıkan girdaplar yansımaz, kararsızlık doğmaz).
//! - Üst/alt sınır: kaymalı duvar (aynasal yansıma; açık havaya benzer, sınır tabaka oluşmaz),
//!   kaymaz duvar (geri sekme) ya da periyodik.
//! - Hacim kuvveti (yalnız testlerde, Poiseuille akışı): "tam fark" yöntemi (Kupershtokh):
//!   f* += f^eq(ρ, u + F/ρ) − f^eq(ρ, u); fiziksel hız u + F/(2ρ).
//!
//! Birimler: tüm büyüklükler kafes birimindedir (Δx = Δt = 1, c_s² = 1/3). Fiziksel hız/uzunluk
//! eşlemesi `flowsim` içindedir.
//!
//! Kaynaklar: Krüger vd., "The Lattice Boltzmann Method: Principles and Practice" (2017),
//! Böl. 3, 5, 6, 10; Latt & Chopard, Math. Comput. Simul. 72 (2006); Hou vd., J. Comput. Phys.
//! 118 (1995/1996) "A lattice Boltzmann subgrid model for high Reynolds number flows".

/// Yön sayısı.
pub const Q: usize = 9;
/// D2Q9 hız kümesi: 0 durağan, 1 D, 2 K, 3 B, 4 G, 5 KD, 6 KB, 7 GB, 8 GD.
pub const CX: [i32; Q] = [0, 1, 0, -1, 0, 1, -1, -1, 1];
pub const CY: [i32; Q] = [0, 0, 1, 0, -1, 1, 1, -1, -1];
/// Ağırlıklar.
pub const W: [f64; Q] = [
    4.0 / 9.0,
    1.0 / 9.0,
    1.0 / 9.0,
    1.0 / 9.0,
    1.0 / 9.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
    1.0 / 36.0,
];
/// Ters yön (geri sekme).
pub const OPP: [usize; Q] = [0, 3, 4, 1, 2, 7, 8, 5, 6];
/// y bileşeni ters çevrilmiş yön (kaymalı duvar aynası).
pub const MIRROR_Y: [usize; Q] = [0, 1, 4, 3, 2, 8, 7, 6, 5];

/// Hücre türleri.
pub const FLUID: u8 = 0;
pub const SOLID: u8 = 1;
pub const POROUS: u8 = 2;

/// x yönündeki sınırlar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XBoundary {
    /// Solda sabit hızlı giriş, sağda sıfır gradyanlı çıkış.
    InletOutlet,
    Periodic,
}

/// y yönündeki (alt/üst) sınırlar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YBoundary {
    FreeSlip,
    NoSlip,
    Periodic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LbmConfig {
    pub nx: usize,
    pub ny: usize,
    /// Moleküler gevşeme süresi τ (ν = (τ − 0.5)/3).
    pub tau: f64,
    /// Giriş hızı (kafes birimi, +x yönünde).
    pub u_in: f64,
    /// Smagorinsky sabiti C_s (0 = kapalı).
    pub smagorinsky: f64,
    pub x_bc: XBoundary,
    pub y_bc: YBoundary,
    /// Hacim kuvveti (kafes birimi, hücre başına).
    pub force: [f64; 2],
    /// Çıkıştaki sönümleme bölgesinin sütun sayısı (0 = yok).
    pub sponge: usize,
    /// Gözenekli hücrelerde hız azaltma oranı k ∈ [0, 1].
    pub porous_k: f64,
    /// İş parçacığı sayısı (1 = tek iş parçacığı).
    pub threads: usize,
}

impl Default for LbmConfig {
    fn default() -> Self {
        LbmConfig {
            nx: 128,
            ny: 64,
            tau: 0.6,
            u_in: 0.08,
            smagorinsky: 0.15,
            x_bc: XBoundary::InletOutlet,
            y_bc: YBoundary::FreeSlip,
            force: [0.0, 0.0],
            sponge: 12,
            porous_k: 0.35,
            threads: 1,
        }
    }
}

/// Kararlılık izleme sınırları: bunların dışı "ıraksadı" sayılır.
pub const RHO_MIN: f64 = 0.3;
pub const RHO_MAX: f64 = 3.0;
pub const U_MAX: f64 = 0.45;
/// Çözücünün kabul ettiği en küçük τ (bunun altı her koşulda kararsızdır).
pub const TAU_FLOOR: f64 = 0.5005;

/// Kafes durumu.
#[derive(Clone, Debug)]
pub struct Lattice {
    cfg: LbmConfig,
    /// Çarpışma sonrası dağılımlar, hücre-ana düzende: f[(y·nx + x)·9 + i].
    f: Vec<f64>,
    g: Vec<f64>,
    cell: Vec<u8>,
    /// Makroskobik alanlar (son `step_macro` çağrısından).
    pub rho: Vec<f64>,
    pub ux: Vec<f64>,
    pub uy: Vec<f64>,
    steps: u64,
}

#[inline]
fn feq(i: usize, rho: f64, ux: f64, uy: f64, usq: f64) -> f64 {
    let cu = CX[i] as f64 * ux + CY[i] as f64 * uy;
    W[i] * rho * (1.0 + 3.0 * cu + 4.5 * cu * cu - 1.5 * usq)
}

/// Salt okunur adım girdileri (iş parçacıkları arasında paylaşılır).
struct StepCtx<'a> {
    cfg: &'a LbmConfig,
    f: &'a [f64],
    cell: &'a [u8],
    u_in: f64,
}

impl Lattice {
    pub fn new(cfg: LbmConfig) -> Lattice {
        let cfg = sanitize(cfg);
        let n = cfg.nx * cfg.ny;
        let mut l = Lattice {
            f: vec![0.0; n * Q],
            g: vec![0.0; n * Q],
            cell: vec![FLUID; n],
            rho: vec![1.0; n],
            ux: vec![0.0; n],
            uy: vec![0.0; n],
            steps: 0,
            cfg,
        };
        l.reset();
        l
    }

    pub fn config(&self) -> &LbmConfig {
        &self.cfg
    }

    pub fn nx(&self) -> usize {
        self.cfg.nx
    }

    pub fn ny(&self) -> usize {
        self.cfg.ny
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    pub fn cells(&self) -> &[u8] {
        &self.cell
    }

    /// Ham dağılımlar (testler ve tanı için).
    pub fn distributions(&self) -> &[f64] {
        &self.f
    }

    /// Parametreler (τ, giriş hızı, Smagorinsky vb.) ızgarayı değiştirmeden güncellenir.
    /// `nx`/`ny` değişmişse yok sayılır (yeni kafes gerekir).
    pub fn set_params(&mut self, mut cfg: LbmConfig) {
        cfg.nx = self.cfg.nx;
        cfg.ny = self.cfg.ny;
        self.cfg = sanitize(cfg);
    }

    /// Akışı başlangıç durumuna getirir: akışkan hücrelerde ρ = 1 ve giriş hızında düzgün akış
    /// (durgun başlangıç, kanal boyunca gidip gelen büyük bir basınç dalgası yaratırdı);
    /// simetriyi kırmak için çok küçük, deterministik bir y-hızı bozuntusu eklenir (yoksa
    /// simetrik cisimde girdap kopması çok geç başlar).
    pub fn reset(&mut self) {
        let (nx, ny) = (self.cfg.nx, self.cfg.ny);
        let u0 = self.cfg.u_in;
        for y in 0..ny {
            for x in 0..nx {
                let k = y * nx + x;
                let (ux, uy) = if self.cell[k] == FLUID || self.cell[k] == POROUS {
                    let phase = std::f64::consts::TAU * x as f64 / nx as f64;
                    (u0, 1e-3 * self.cfg.u_in * phase.sin())
                } else {
                    (0.0, 0.0)
                };
                self.set_equilibrium(k, 1.0, ux, uy);
            }
        }
        self.g.copy_from_slice(&self.f);
        self.steps = 0;
    }

    fn set_equilibrium(&mut self, k: usize, rho: f64, ux: f64, uy: f64) {
        let usq = ux * ux + uy * uy;
        for i in 0..Q {
            self.f[k * Q + i] = feq(i, rho, ux, uy, usq);
        }
        self.rho[k] = rho;
        self.ux[k] = ux;
        self.uy[k] = uy;
    }

    /// Engel maskesini değiştirir (uzunluk nx·ny olmalı). Akış alanı korunur: katılaşan hücreler
    /// durgun dengeye, yeniden akışkan olan hücreler komşularının ortalamasına (yoksa durgun
    /// dengeye) ayarlanır. Böylece açı değişince akış sıfırdan başlamaz ve kararsızlık doğmaz.
    pub fn set_cells(&mut self, cells: &[u8]) {
        if cells.len() != self.cell.len() {
            return;
        }
        let (nx, ny) = (self.cfg.nx, self.cfg.ny);
        let old = std::mem::replace(&mut self.cell, cells.to_vec());
        for k in 0..old.len() {
            let was_solid = old[k] == SOLID;
            let is_solid = self.cell[k] == SOLID;
            if is_solid && !was_solid {
                self.set_equilibrium(k, 1.0, 0.0, 0.0);
            } else if was_solid && !is_solid {
                let (x, y) = (k % nx, k / nx);
                let (mut r, mut u, mut v, mut n) = (0.0, 0.0, 0.0, 0.0);
                for i in 1..Q {
                    let xx = x as i64 + CX[i] as i64;
                    let yy = y as i64 + CY[i] as i64;
                    if xx < 0 || yy < 0 || xx >= nx as i64 || yy >= ny as i64 {
                        continue;
                    }
                    let kk = yy as usize * nx + xx as usize;
                    if old[kk] != SOLID && self.cell[kk] != SOLID {
                        r += self.rho[kk];
                        u += self.ux[kk];
                        v += self.uy[kk];
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    self.set_equilibrium(k, r / n, u / n, v / n);
                } else {
                    self.set_equilibrium(k, 1.0, 0.0, 0.0);
                }
            }
        }
        self.g.copy_from_slice(&self.f);
    }

    /// Toplam kütle (akışkan + gözenekli hücreler).
    pub fn total_mass(&self) -> f64 {
        let mut m = 0.0;
        for (k, c) in self.cell.iter().enumerate() {
            if *c != SOLID {
                m += self.f[k * Q..k * Q + Q].iter().sum::<f64>();
            }
        }
        m
    }

    /// Bir adım; makroskobik alanlar güncellenmez. Iraksama algılanırsa `false`.
    pub fn step(&mut self) -> bool {
        self.step_inner(false)
    }

    /// Bir adım ve makroskobik alanların (ρ, u) güncellenmesi. Iraksama algılanırsa `false`.
    pub fn step_macro(&mut self) -> bool {
        self.step_inner(true)
    }

    fn step_inner(&mut self, want_macro: bool) -> bool {
        let (nx, ny) = (self.cfg.nx, self.cfg.ny);
        let u_in = self.cfg.u_in;
        let threads = self.cfg.threads.clamp(1, 64).min(ny);
        let ok;
        {
            let ctx = StepCtx {
                cfg: &self.cfg,
                f: &self.f,
                cell: &self.cell,
                u_in,
            };
            if threads <= 1 {
                ok = collide_rows(
                    &ctx,
                    0,
                    &mut self.g,
                    &mut self.rho,
                    &mut self.ux,
                    &mut self.uy,
                    want_macro,
                );
            } else {
                let rows_per = ny.div_ceil(threads);
                let ctx = &ctx;
                ok = std::thread::scope(|s| {
                    let handles: Vec<_> = self
                        .g
                        .chunks_mut(rows_per * nx * Q)
                        .zip(self.rho.chunks_mut(rows_per * nx))
                        .zip(self.ux.chunks_mut(rows_per * nx))
                        .zip(self.uy.chunks_mut(rows_per * nx))
                        .enumerate()
                        .map(|(t, (((g, r), u), v))| {
                            s.spawn(move || collide_rows(ctx, t * rows_per, g, r, u, v, want_macro))
                        })
                        .collect();
                    // Bir iş parçacığı panik yaparsa adım başarısız sayılır (çağıran kurtarır).
                    handles
                        .into_iter()
                        .map(|h| h.join().unwrap_or(false))
                        .collect::<Vec<bool>>()
                        .into_iter()
                        .all(|b| b)
                });
            }
        }
        if self.cfg.x_bc == XBoundary::InletOutlet && nx >= 3 {
            // Giriş: Zou–He hız sınırı (`collide_rows` içinde). Çıkış (basınç sınırı): ρ = 1,
            // hız ve dengesizlik kısmı bir önceki sütundan; ortalama yoğunluğun zamanla
            // kaymasını önler.
            for y in 0..ny {
                let k1 = y * nx + nx - 1;
                let k2 = k1 - 1;
                if self.cell[k1] != SOLID && self.cell[k2] != SOLID {
                    let (r2, ux2, uy2) = moments(&self.g[k2 * Q..k2 * Q + Q]);
                    if r2.is_finite() && ux2.is_finite() && uy2.is_finite() {
                        // Geri akışı sınırlı tut (çıkışa giren girdap kararsızlık yaratmasın).
                        let ux_o = ux2.max(0.0);
                        let usq2 = ux2 * ux2 + uy2 * uy2;
                        let usq_o = ux_o * ux_o + uy2 * uy2;
                        for i in 0..Q {
                            let neq = self.g[k2 * Q + i] - feq(i, r2, ux2, uy2, usq2);
                            self.g[k1 * Q + i] = feq(i, 1.0, ux_o, uy2, usq_o) + neq;
                        }
                        if want_macro {
                            self.rho[k1] = 1.0;
                            self.ux[k1] = ux_o;
                            self.uy[k1] = uy2;
                        }
                    }
                }
            }
        }
        std::mem::swap(&mut self.f, &mut self.g);
        self.steps += 1;
        ok
    }
}

/// Tek hücrenin (ρ, u_x, u_y) momentleri.
fn moments(f: &[f64]) -> (f64, f64, f64) {
    let mut r = 0.0;
    let mut jx = 0.0;
    let mut jy = 0.0;
    for i in 0..Q {
        r += f[i];
        jx += CX[i] as f64 * f[i];
        jy += CY[i] as f64 * f[i];
    }
    if r > 1e-12 {
        (r, jx / r, jy / r)
    } else {
        (r, 0.0, 0.0)
    }
}

fn sanitize(mut c: LbmConfig) -> LbmConfig {
    c.nx = c.nx.clamp(4, 4096);
    c.ny = c.ny.clamp(4, 4096);
    if !(c.tau.is_finite()) {
        c.tau = 0.6;
    }
    c.tau = c.tau.clamp(TAU_FLOOR, 5.0);
    if !c.u_in.is_finite() {
        c.u_in = 0.0;
    }
    c.u_in = c.u_in.clamp(0.0, 0.2);
    if !c.smagorinsky.is_finite() {
        c.smagorinsky = 0.0;
    }
    c.smagorinsky = c.smagorinsky.clamp(0.0, 0.5);
    for v in &mut c.force {
        if !v.is_finite() {
            *v = 0.0;
        }
        *v = v.clamp(-1e-3, 1e-3);
    }
    c.sponge = c.sponge.min(c.nx / 3);
    if !c.porous_k.is_finite() {
        c.porous_k = 0.0;
    }
    c.porous_k = c.porous_k.clamp(0.0, 1.0);
    c.threads = c.threads.clamp(1, 64);
    c
}

/// `y0`'dan başlayan satır bloğunu hesaplar (g, rho, ux, uy bu bloğun dilimleridir).
fn collide_rows(
    ctx: &StepCtx,
    y0: usize,
    g: &mut [f64],
    rho_out: &mut [f64],
    ux_out: &mut [f64],
    uy_out: &mut [f64],
    want_macro: bool,
) -> bool {
    let cfg = ctx.cfg;
    let (nx, ny) = (cfg.nx as i64, cfg.ny as i64);
    let rows = g.len() / (cfg.nx * Q);
    let f = ctx.f;
    let cell = ctx.cell;
    let tau0 = cfg.tau;
    // Hou vd.: τ_eff = ½(τ0 + √(τ0² + 18·√2·C²·|Π^neq|/ρ)).
    let smag = 18.0 * std::f64::consts::SQRT_2 * cfg.smagorinsky * cfg.smagorinsky;
    let sponge_start = cfg.nx.saturating_sub(cfg.sponge) as i64;
    let tau_sponge = tau0.max(0.9);
    let inlet_layer = (cfg.sponge / 2) as i64;
    // Sönümleme katmanlarının hedefi: serbest akış dengesi f^eq(1, U).
    let mut feq_inf = [0.0f64; Q];
    for i in 0..Q {
        feq_inf[i] = feq(i, 1.0, ctx.u_in, 0.0, ctx.u_in * ctx.u_in);
    }
    let (fx, fy) = (cfg.force[0], cfg.force[1]);
    let has_force = fx != 0.0 || fy != 0.0;
    let inlet = cfg.x_bc == XBoundary::InletOutlet;
    let mut ok = true;
    let mut fi = [0.0f64; Q];
    for ly in 0..rows {
        let y = (y0 + ly) as i64;
        for x in 0..nx {
            let k = (y * nx + x) as usize;
            let lk = ly * cfg.nx + x as usize;
            let out = &mut g[lk * Q..lk * Q + Q];
            let ct = cell[k];
            if ct == SOLID {
                out.copy_from_slice(&W);
                if want_macro {
                    rho_out[lk] = 1.0;
                    ux_out[lk] = 0.0;
                    uy_out[lk] = 0.0;
                }
                continue;
            }
            // --- akış (çek) + sınırlar ---
            if x > 0 && x < nx - 1 && y > 0 && y < ny - 1 {
                // İç hücre: sınır denetimi yok (hücrelerin büyük çoğunluğu).
                for i in 0..Q {
                    let s = (k as i64 - CY[i] as i64 * nx - CX[i] as i64) as usize;
                    fi[i] = if cell[s] == SOLID {
                        f[k * Q + OPP[i]]
                    } else {
                        f[s * Q + i]
                    };
                }
            } else {
                for i in 0..Q {
                    let mut sx = x - CX[i] as i64;
                    let mut sy = y - CY[i] as i64;
                    let mut dir = i;
                    if sx < 0 || sx >= nx {
                        match cfg.x_bc {
                            XBoundary::Periodic => sx = sx.rem_euclid(nx),
                            XBoundary::InletOutlet => sx = sx.clamp(0, nx - 1),
                        }
                    }
                    if sy < 0 || sy >= ny {
                        match cfg.y_bc {
                            YBoundary::Periodic => sy = sy.rem_euclid(ny),
                            YBoundary::NoSlip => {
                                fi[i] = f[k * Q + OPP[i]];
                                continue;
                            }
                            YBoundary::FreeSlip => {
                                sy = y;
                                dir = MIRROR_Y[i];
                            }
                        }
                    }
                    let s = (sy * nx + sx) as usize;
                    fi[i] = if cell[s] == SOLID {
                        f[k * Q + OPP[i]]
                    } else {
                        f[s * Q + dir]
                    };
                }
            }
            if inlet && x == 0 {
                // Zou & He (1997) hız sınırı, batı duvarı: bilinmeyenler 1, 5, 8.
                let u = ctx.u_in;
                let r = (fi[0] + fi[2] + fi[4] + 2.0 * (fi[3] + fi[6] + fi[7])) / (1.0 - u);
                let r = if r.is_finite() {
                    r.clamp(RHO_MIN, RHO_MAX)
                } else {
                    1.0
                };
                fi[1] = fi[3] + 2.0 / 3.0 * r * u;
                fi[5] = fi[7] - 0.5 * (fi[2] - fi[4]) + r * u / 6.0;
                fi[8] = fi[6] + 0.5 * (fi[2] - fi[4]) + r * u / 6.0;
            }
            // --- momentler ---
            let rho: f64 = fi.iter().sum();
            let mut jx = 0.0;
            let mut jy = 0.0;
            for i in 0..Q {
                jx += CX[i] as f64 * fi[i];
                jy += CY[i] as f64 * fi[i];
            }
            if !(rho > RHO_MIN && rho < RHO_MAX) {
                ok = false;
                out.copy_from_slice(&W);
                if want_macro {
                    rho_out[lk] = 1.0;
                    ux_out[lk] = 0.0;
                    uy_out[lk] = 0.0;
                }
                continue;
            }
            let inv = 1.0 / rho;
            let mut ux = jx * inv;
            let mut uy = jy * inv;
            let usq = ux * ux + uy * uy;
            if !(usq < U_MAX * U_MAX) {
                ok = false;
            }
            // Dengesizlik ikinci momenti Π^neq.
            // Π^neq = Σ c c f − Π^eq, Π^eq_ab = ρ(c_s² δ_ab + u_a u_b) (kapalı biçim; f^eq'yi
            // her yön için ayrıca hesaplamaktan ucuz).
            let sxx = fi[1] + fi[3] + fi[5] + fi[6] + fi[7] + fi[8];
            let syy = fi[2] + fi[4] + fi[5] + fi[6] + fi[7] + fi[8];
            let sxy = fi[5] - fi[6] + fi[7] - fi[8];
            let pxx = sxx - rho * (1.0 / 3.0 + ux * ux);
            let pyy = syy - rho * (1.0 / 3.0 + uy * uy);
            let pxy = sxy - rho * ux * uy;
            let mut tau = if smag > 0.0 {
                let qn = (pxx * pxx + pyy * pyy + 2.0 * pxy * pxy).sqrt();
                0.5 * (tau0 + (tau0 * tau0 + smag * qn * inv).sqrt())
            } else {
                tau0
            };
            if cfg.x_bc == XBoundary::InletOutlet && x >= sponge_start && cfg.sponge > 0 {
                let r = (x - sponge_start) as f64 / cfg.sponge as f64;
                tau = tau.max(tau0 + (tau_sponge - tau0) * r * r);
            }
            let omega = 1.0 - 1.0 / tau;
            // Gözenekli bölge: denge hızını küçült (momentum yutağı).
            let (eqx, eqy) = if ct == POROUS {
                let s = 1.0 - cfg.porous_k;
                (ux * s, uy * s)
            } else {
                (ux, uy)
            };
            let eq_usq = eqx * eqx + eqy * eqy;
            for i in 0..Q {
                let (cx, cy) = (CX[i] as f64, CY[i] as f64);
                // Q_iab = c_ia c_ib − δ_ab/3; f_i^neq,reg = w_i·(9/2)·Q_iab Π_ab.
                let qxx = cx * cx - 1.0 / 3.0;
                let qyy = cy * cy - 1.0 / 3.0;
                let qxy = cx * cy;
                let neq = W[i] * 4.5 * (qxx * pxx + qyy * pyy + 2.0 * qxy * pxy);
                out[i] = feq(i, rho, eqx, eqy, eq_usq) + omega * neq;
            }
            // Emici katmanlar (giriş ve çıkış): dağılım serbest akış dengesine doğru çekilir;
            // basınç dalgaları ve çıkan girdaplar geri yansımaz.
            if inlet && cfg.sponge > 0 {
                let r = if x >= sponge_start {
                    (x - sponge_start + 1) as f64 / cfg.sponge as f64
                } else if x < inlet_layer {
                    (inlet_layer - x) as f64 / inlet_layer.max(1) as f64
                } else {
                    0.0
                };
                if r > 0.0 {
                    let sigma = 0.08 * r * r;
                    for i in 0..Q {
                        out[i] += sigma * (feq_inf[i] - out[i]);
                    }
                }
            }
            if has_force {
                let (ux2, uy2) = (ux + fx * inv, uy + fy * inv);
                let usq2 = ux2 * ux2 + uy2 * uy2;
                for i in 0..Q {
                    out[i] += feq(i, rho, ux2, uy2, usq2) - feq(i, rho, ux, uy, usq);
                }
                ux += 0.5 * fx * inv;
                uy += 0.5 * fy * inv;
            }
            if want_macro {
                rho_out[lk] = rho;
                ux_out[lk] = ux;
                uy_out[lk] = uy;
            }
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed_box(nx: usize, ny: usize) -> LbmConfig {
        LbmConfig {
            nx,
            ny,
            tau: 0.55,
            u_in: 0.0,
            x_bc: XBoundary::Periodic,
            y_bc: YBoundary::NoSlip,
            sponge: 0,
            ..Default::default()
        }
    }

    /// Kapalı/periyodik kutuda (engel, gözenekli bölge ve başlangıç girdabıyla) kütle korunur.
    #[test]
    fn mass_is_conserved_in_closed_and_periodic_boxes() {
        for y_bc in [YBoundary::NoSlip, YBoundary::FreeSlip, YBoundary::Periodic] {
            let mut cfg = closed_box(64, 40);
            cfg.y_bc = y_bc;
            let mut l = Lattice::new(cfg);
            let mut cells = vec![FLUID; 64 * 40];
            for y in 15..25 {
                for x in 20..28 {
                    cells[y * 64 + x] = SOLID;
                }
                for x in 40..44 {
                    cells[y * 64 + x] = POROUS;
                }
            }
            l.set_cells(&cells);
            // Başlangıçta güçlü bir girdap (Taylor–Green benzeri).
            for y in 0..40 {
                for x in 0..64 {
                    let k = y * 64 + x;
                    if cells[k] != SOLID {
                        let a = std::f64::consts::TAU * x as f64 / 64.0;
                        let b = std::f64::consts::TAU * y as f64 / 40.0;
                        l.set_equilibrium(
                            k,
                            1.0,
                            0.05 * a.sin() * b.cos(),
                            -0.05 * a.cos() * b.sin(),
                        );
                    }
                }
            }
            let m0 = l.total_mass();
            for _ in 0..800 {
                assert!(l.step(), "{y_bc:?} ıraksadı");
            }
            let m1 = l.total_mass();
            assert!(
                ((m1 - m0) / m0).abs() < 1e-10,
                "{y_bc:?}: kütle değişti {m0} → {m1}"
            );
        }
    }

    /// Kaymaz duvarlar arasında hacim kuvvetiyle sürülen kanal: parabolik (Poiseuille) profil.
    #[test]
    fn poiseuille_profile_with_no_slip_walls() {
        let ny = 24;
        let tau = 0.8;
        let nu = (tau - 0.5) / 3.0;
        let fx = 1e-6;
        let cfg = LbmConfig {
            nx: 8,
            ny,
            tau,
            u_in: 0.0,
            smagorinsky: 0.0,
            x_bc: XBoundary::Periodic,
            y_bc: YBoundary::NoSlip,
            force: [fx, 0.0],
            sponge: 0,
            ..Default::default()
        };
        let mut l = Lattice::new(cfg);
        for y in 0..ny {
            for x in 0..8 {
                l.set_equilibrium(y * 8 + x, 1.0, 0.0, 0.0);
            }
        }
        l.g.copy_from_slice(&l.f);
        for _ in 0..12000 {
            assert!(l.step());
        }
        l.step_macro();
        // Yarı-yol geri sekme: duvarlar y = −0.5 ve y = ny − 0.5'te; H = ny.
        let h = ny as f64;
        let mut err2 = 0.0;
        let mut ref2 = 0.0;
        let mut umax: f64 = 0.0;
        for y in 0..ny {
            let yy = y as f64 + 0.5;
            let exact = fx / (2.0 * nu) * yy * (h - yy);
            let u = l.ux[y * 8 + 3];
            umax = umax.max(u);
            err2 += (u - exact).powi(2);
            ref2 += exact * exact;
            assert!(l.uy[y * 8 + 3].abs() < 1e-9 * (1.0 + exact.abs() * 1e6));
        }
        let rel = (err2 / ref2).sqrt();
        let exact_max = fx / (2.0 * nu) * (h / 2.0).powi(2);
        assert!(rel < 0.01, "Poiseuille L2 hatası {rel}");
        assert!(
            ((umax - exact_max) / exact_max).abs() < 0.02,
            "{umax} vs {exact_max}"
        );
    }

    /// Aynı girdiler → aynı sonuç; iş parçacığı sayısı sonucu değiştirmez.
    #[test]
    fn deterministic_and_thread_count_independent() {
        let run = |threads: usize| {
            let cfg = LbmConfig {
                nx: 90,
                ny: 40,
                tau: 0.52,
                u_in: 0.1,
                threads,
                ..Default::default()
            };
            let mut l = Lattice::new(cfg);
            let mut cells = vec![FLUID; 90 * 40];
            for y in 0..40 {
                for x in 0..90 {
                    let (dx, dy) = (x as f64 - 25.0, y as f64 - 20.3);
                    if dx * dx + dy * dy < 36.0 {
                        cells[y * 90 + x] = SOLID;
                    }
                }
            }
            l.set_cells(&cells);
            for _ in 0..300 {
                l.step();
            }
            l.step_macro();
            l.distributions().to_vec()
        };
        let a = run(1);
        let b = run(1);
        let c = run(3);
        let d = run(7);
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a, d);
    }

    /// Silindir arkasında girdap kopması (Kármán izi): Strouhal sayısı makul aralıkta.
    #[test]
    fn cylinder_sheds_vortices_with_sane_strouhal() {
        // Re = 100, D = 10 hücre, %16 tıkanma (kaymalı duvarlar). Sınırsız akışta St ≈ 0.165;
        // tıkanma St'yi biraz yükseltir (≈ 0.18–0.2).
        let (nx, ny) = (180usize, 64usize);
        let d = 10.0;
        let u = 0.1;
        let re = 100.0;
        let nu = u * d / re;
        let cfg = LbmConfig {
            nx,
            ny,
            tau: 3.0 * nu + 0.5,
            u_in: u,
            smagorinsky: 0.0,
            threads: 4,
            ..Default::default()
        };
        let mut l = Lattice::new(cfg);
        let (cx, cy) = (50.0, ny as f64 / 2.0);
        let mut cells = vec![FLUID; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                if dx * dx + dy * dy <= (d / 2.0) * (d / 2.0) {
                    cells[y * nx + x] = SOLID;
                }
            }
        }
        l.set_cells(&cells);
        l.reset();
        let probe = (ny / 2) * nx + (cx + 3.0 * d) as usize;
        let transient = 4000;
        let record = 2500;
        let mut series = Vec::with_capacity(record);
        for s in 0..transient + record {
            let okk = if s >= transient {
                l.step_macro()
            } else {
                l.step()
            };
            assert!(okk, "silindir ıraksadı (adım {s})");
            if s >= transient {
                series.push(l.uy[probe]);
            }
        }
        let amp = series.iter().fold(0.0f64, |a, v| a.max(v.abs()));
        assert!(amp > 0.05 * u, "iz salınmıyor: genlik {amp}");
        // Sıfır geçişleri (yukarı yönlü) → frekans.
        let mean = series.iter().sum::<f64>() / series.len() as f64;
        let ups: Vec<usize> = (1..series.len())
            .filter(|&i| series[i - 1] < mean && series[i] >= mean)
            .collect();
        assert!(ups.len() >= 3, "yetersiz periyot: {}", ups.len());
        let period = (ups[ups.len() - 1] - ups[0]) as f64 / (ups.len() - 1) as f64;
        let st = d / (period * u);
        println!("Strouhal = {st:.3} (periyot {period:.0} adım)");
        assert!((0.14..0.24).contains(&st), "Strouhal {st}");
    }

    /// Kararsızlık algılanır: aşırı başlangıç hızında adım `false` döner, panik yoktur.
    #[test]
    fn divergence_is_detected() {
        let cfg = LbmConfig {
            nx: 40,
            ny: 20,
            tau: TAU_FLOOR,
            smagorinsky: 0.0,
            ..Default::default()
        };
        let mut l = Lattice::new(cfg);
        for k in 0..40 * 20 {
            let v = if (k / 3) % 2 == 0 { 0.6 } else { -0.6 };
            l.set_equilibrium(k, 1.0, v, -v);
        }
        l.g.copy_from_slice(&l.f);
        let mut flagged = false;
        for _ in 0..200 {
            if !l.step() {
                flagged = true;
                break;
            }
        }
        assert!(flagged);
    }

    #[test]
    fn config_is_sanitized() {
        let l = Lattice::new(LbmConfig {
            nx: 0,
            ny: 1,
            tau: f64::NAN,
            u_in: 5.0,
            smagorinsky: -1.0,
            threads: 0,
            ..Default::default()
        });
        let c = l.config();
        assert!(c.nx >= 4 && c.ny >= 4);
        assert!(c.tau >= TAU_FLOOR && c.tau.is_finite());
        assert!(c.u_in <= 0.2);
        assert!(c.smagorinsky >= 0.0);
        assert!(c.threads >= 1);
    }
}
