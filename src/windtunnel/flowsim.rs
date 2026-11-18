// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canlı rüzgâr tüneli motoru: kesit maskesi + LBM çözücüsü + fiziksel birim eşlemesi, ve bunu
//! arayüz iş parçacığını hiç bekletmeden arka planda çalıştıran yürütücü.
//!
//! Bu çıktı NİTELDİR (akışın görünüşü). Kuvvet/katsayı üretmez; sayısal sonuçlar panel ve VLM
//! kodundan gelir.
//!
//! # Birim eşlemesi (fiziksel → kafes)
//!
//! - Uzunluk: kesitteki cismin akış yönündeki boyu `L` [m] (kanat kesitinde yerel kord) kafeste
//!   `L_lat = L / Δx` hücreye karşılık gelir; Δx çerçevelemeden gelir (cisim kanal boyunun
//!   ~%32'si).
//! - Hız: hava hızı V [m/s], kafeste `U_lat` ile temsil edilir. `U_lat`, V ile doğrusal olarak
//!   0.03…0.07 arasında artar (Mach_lat = U_lat/c_s ≤ 0.12). Girdap merkezlerinde yerel hız
//!   serbest akışın birkaç katına çıkabildiğinden üst sınır bilerek düşük tutulmuştur (0.10'da
//!   bazı kesitlerde yerel Mach ~0.8'e çıkıp ıraksama görüldü). Böylece hızı artırınca ekrandaki
//!   akış da hızlanır.
//! - Zaman: bir adım `Δt = Δx · U_lat / V` saniyedir.
//! - Viskozite / Reynolds: gerçek Re = V·L/ν_hava (ν_hava = 1.5·10⁻⁵ m²/s) genellikle 10⁵…10⁶'dır;
//!   bu, dizüstü bilgisayardaki birkaç yüz hücrelik kafeste kararlı çözülemez. Benzetim Re'si
//!   `Re_sim = min(Re_gerçek, U_lat·L_lat/ν_min)` alınır; ν_min, kararlı en küçük gevşeme
//!   süresinden (τ_min) gelir: ν = (τ − 0.5)/3. Sonra `ν_lat = U_lat·L_lat/Re_sim`,
//!   `τ = 3ν_lat + 0.5`. Arayüz iki Re'yi de açıkça gösterir. Bir ıraksama olursa Re_sim
//!   kademeli olarak düşürülüp akış yeniden başlatılır.
use super::lbm::{FLUID, Lattice, LbmConfig, SOLID, XBoundary, YBoundary};
use super::section::{CutGeometry, CutKind, CutParams, Mask, SectionOf, cut_geometry, rasterize};
use crate::geom3d::AircraftSpec;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Çözünürlük/kalite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Quality {
    Low,
    #[default]
    Medium,
    High,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Low, Quality::Medium, Quality::High];

    /// Kafes boyutu (nx, ny).
    pub fn grid(self) -> (usize, usize) {
        match self {
            Quality::Low => (240, 120),
            Quality::Medium => (360, 180),
            Quality::High => (480, 240),
        }
    }
}

/// Hava hızı sürgüsünün aralığı [m/s].
pub const AIRSPEED_MIN: f64 = 1.0;
pub const AIRSPEED_MAX: f64 = 60.0;
/// Havanın kinematik viskozitesi [m²/s] (deniz seviyesi, 15 °C).
pub const NU_AIR: f64 = 1.5e-5;
const U_LAT_MIN: f64 = 0.03;
const U_LAT_MAX: f64 = 0.07;
/// Kararlı en küçük τ (düzenlenmiş BGK + Smagorinsky ile tüm kesit/hız birleşimlerinde
/// sınandı; `tests::every_cut_of_every_example_is_stable`).
pub const TAU_MIN: f64 = 0.508;
const SMAGORINSKY: f64 = 0.16;

/// Fiziksel ↔ kafes birimleri.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    pub airspeed: f64,
    pub u_lat: f64,
    pub tau: f64,
    pub re_phys: f64,
    pub re_sim: f64,
    /// Hücre boyu [m] ve adım süresi [s].
    pub dx: f64,
    pub dt: f64,
    /// Başvuru uzunluğu [m] ve hücre cinsinden.
    pub l_ref: f64,
    pub l_lat: f64,
}

/// Eşleme (modül belgesine bakın). `re_scale` ≤ 1: ıraksamadan sonra Re_sim düşürme çarpanı.
pub fn units(airspeed: f64, l_ref: f64, dx: f64, re_scale: f64) -> Units {
    let v = if airspeed.is_finite() {
        airspeed.clamp(AIRSPEED_MIN, AIRSPEED_MAX)
    } else {
        AIRSPEED_MIN
    };
    let dx = if dx.is_finite() && dx > 0.0 { dx } else { 1.0 };
    let l_ref = if l_ref.is_finite() && l_ref > 0.0 {
        l_ref
    } else {
        dx * 40.0
    };
    let s = (v - AIRSPEED_MIN) / (AIRSPEED_MAX - AIRSPEED_MIN);
    let u_lat = U_LAT_MIN + (U_LAT_MAX - U_LAT_MIN) * s;
    let l_lat = (l_ref / dx).max(1.0);
    let re_phys = v * l_ref / NU_AIR;
    let nu_min = (TAU_MIN - 0.5) / 3.0;
    let re_cap = u_lat * l_lat / nu_min;
    let scale = if re_scale.is_finite() {
        re_scale.clamp(0.01, 1.0)
    } else {
        1.0
    };
    let re_sim = (re_phys.min(re_cap) * scale).max(1.0);
    let nu = u_lat * l_lat / re_sim;
    Units {
        airspeed: v,
        u_lat,
        tau: 3.0 * nu + 0.5,
        re_phys,
        re_sim,
        dx,
        dt: dx * u_lat / v,
        l_ref,
        l_lat,
    }
}

/// Arayüzün istediği tünel durumu.
#[derive(Clone, Debug, PartialEq)]
pub struct SimSetup {
    pub spec: Arc<AircraftSpec>,
    pub cut: CutParams,
    pub quality: Quality,
    pub airspeed: f64,
}

/// Arayüze yayımlanan anlık görüntü (yalnız okunur).
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub nx: usize,
    pub ny: usize,
    pub ux: Vec<f32>,
    pub uy: Vec<f32>,
    pub rho: Vec<f32>,
    pub cells: Vec<u8>,
    pub step: u64,
    /// Rampa sonrası hedef giriş hızı (kafes birimi).
    pub u_lat: f64,
    pub units: Units,
    pub empty: bool,
    pub section_of: SectionOf,
    pub offset_m: f64,
    pub cut: CutParams,
    pub recoveries: u32,
    pub steps_per_sec: f64,
    pub seq: u64,
}

/// Tek iş parçacıklı (eşzamanlı) motor: arka plan yürütücüsü ve başsız testler kullanır.
pub struct Engine {
    setup: Option<SimSetup>,
    geom: Option<CutGeometry>,
    mask: Option<Mask>,
    lattice: Option<Lattice>,
    units: Units,
    re_scale: f64,
    recoveries: u32,
    threads: usize,
}

fn geom_key_changed(a: &SimSetup, b: &SimSetup) -> bool {
    a.spec != b.spec || a.cut.kind != b.cut.kind || a.cut.station != b.cut.station
}

impl Engine {
    pub fn new(threads: usize) -> Engine {
        Engine {
            setup: None,
            geom: None,
            mask: None,
            lattice: None,
            units: units(AIRSPEED_MIN, 1.0, 0.01, 1.0),
            re_scale: 1.0,
            recoveries: 0,
            threads: threads.max(1),
        }
    }

    pub fn units(&self) -> Units {
        self.units
    }

    pub fn recoveries(&self) -> u32 {
        self.recoveries
    }

    pub fn lattice(&self) -> Option<&Lattice> {
        self.lattice.as_ref()
    }

    pub fn mask(&self) -> Option<&Mask> {
        self.mask.as_ref()
    }

    fn lbm_config(&self, nx: usize, ny: usize) -> LbmConfig {
        LbmConfig {
            nx,
            ny,
            tau: self.units.tau,
            u_in: self.units.u_lat,
            smagorinsky: SMAGORINSKY,
            x_bc: XBoundary::InletOutlet,
            y_bc: YBoundary::FreeSlip,
            force: [0.0, 0.0],
            sponge: nx / 10,
            porous_k: 0.35,
            threads: self.threads,
        }
    }

    /// Yeni durumu uygular; yalnız değişen kısım yeniden kurulur. Kesit türü, istasyon ya da
    /// tasarım değişirse geometri yeniden türetilir; yalnız α değişirse maske döndürülür ve akış
    /// korunur; yalnız hız değişirse τ/U güncellenir.
    pub fn apply(&mut self, s: SimSetup) {
        let s = SimSetup {
            cut: s.cut.sanitized(),
            ..s
        };
        let old = self.setup.take();
        let (nx, ny) = s.quality.grid();
        let geom_changed = old.as_ref().is_none_or(|o| geom_key_changed(o, &s));
        let grid_changed = old.as_ref().is_none_or(|o| o.quality != s.quality);
        let alpha_changed = old
            .as_ref()
            .is_none_or(|o| o.cut.alpha_deg != s.cut.alpha_deg);
        let restart = old
            .as_ref()
            .is_none_or(|o| o.spec != s.spec || o.cut.kind != s.cut.kind || o.quality != s.quality);
        if geom_changed || self.geom.is_none() {
            self.geom = Some(cut_geometry(&s.spec, &s.cut));
        }
        let mut mask_changed = false;
        if geom_changed || grid_changed || alpha_changed || self.mask.is_none() {
            if let Some(g) = &self.geom {
                self.mask = Some(rasterize(g, s.cut.alpha_deg, nx, ny));
                mask_changed = true;
            }
        }
        if restart {
            self.re_scale = 1.0;
        }
        let (l_ref, dx) = self
            .mask
            .as_ref()
            .map(|m| (m.l_ref, m.dx))
            .unwrap_or((1.0, 0.01));
        self.units = units(s.airspeed, l_ref, dx, self.re_scale);
        let cfg = self.lbm_config(nx, ny);
        match &mut self.lattice {
            Some(l) if !grid_changed && l.nx() == nx && l.ny() == ny => {
                l.set_params(cfg);
                if mask_changed {
                    if let Some(m) = &self.mask {
                        l.set_cells(&m.cells);
                    }
                }
                if restart {
                    l.reset();
                }
            }
            _ => {
                let mut l = Lattice::new(cfg);
                if let Some(m) = &self.mask {
                    l.set_cells(&m.cells);
                }
                l.reset();
                self.lattice = Some(l);
            }
        }
        self.setup = Some(s);
    }

    /// Akışı baştan başlatır (ıraksama sonrası düşürülen Re de geri alınır).
    pub fn reset(&mut self) {
        self.re_scale = 1.0;
        if let Some(s) = self.setup.clone() {
            let (l_ref, dx) = self
                .mask
                .as_ref()
                .map(|m| (m.l_ref, m.dx))
                .unwrap_or((1.0, 0.01));
            self.units = units(s.airspeed, l_ref, dx, self.re_scale);
            let (nx, ny) = s.quality.grid();
            let cfg = self.lbm_config(nx, ny);
            if let Some(l) = &mut self.lattice {
                l.set_params(cfg);
                l.reset();
            }
        }
    }

    /// Iraksama: Re_sim'i düşür ve akışı yeniden başlat.
    fn recover(&mut self) {
        self.recoveries += 1;
        self.re_scale = (self.re_scale * 0.6).max(0.02);
        if let Some(s) = &self.setup {
            let (l_ref, dx) = self
                .mask
                .as_ref()
                .map(|m| (m.l_ref, m.dx))
                .unwrap_or((1.0, 0.01));
            self.units = units(s.airspeed, l_ref, dx, self.re_scale);
            let (nx, ny) = s.quality.grid();
            let cfg = self.lbm_config(nx, ny);
            if let Some(l) = &mut self.lattice {
                l.set_params(cfg);
                l.reset();
            }
        }
    }

    /// `n` adım ilerletir (sonuncusu makroskobik alanları da günceller). Iraksama olduysa
    /// kurtarılır ve `false` döner.
    pub fn run(&mut self, n: usize) -> bool {
        let mut clean = true;
        for k in 0..n {
            let Some(l) = &mut self.lattice else {
                return clean;
            };
            let ok = if k + 1 == n { l.step_macro() } else { l.step() };
            if !ok {
                clean = false;
                self.recover();
                if let Some(l) = &mut self.lattice {
                    l.step_macro();
                }
                break;
            }
        }
        clean
    }

    /// Anlık görüntü (makroskobik alanlar son `run`'dan).
    pub fn snapshot(&self, seq: u64, steps_per_sec: f64) -> Option<Snapshot> {
        let l = self.lattice.as_ref()?;
        let s = self.setup.as_ref()?;
        let g = self.geom.as_ref()?;
        let m = self.mask.as_ref()?;
        let to32 = |v: &[f64]| v.iter().map(|x| *x as f32).collect::<Vec<f32>>();
        Some(Snapshot {
            nx: l.nx(),
            ny: l.ny(),
            ux: to32(&l.ux),
            uy: to32(&l.uy),
            rho: to32(&l.rho),
            cells: l.cells().to_vec(),
            step: l.steps(),
            u_lat: self.units.u_lat,
            units: self.units,
            empty: m.empty,
            section_of: g.section_of,
            offset_m: g.offset_m,
            cut: s.cut,
            recoveries: self.recoveries,
            steps_per_sec,
            seq,
        })
    }
}

enum Cmd {
    Setup(Box<SimSetup>),
    Reset,
}

struct Shared {
    latest: Mutex<Option<Arc<Snapshot>>>,
    running: AtomicBool,
    quit: AtomicBool,
    busy: AtomicBool,
    /// Arayüzün son "ekrandayım" sinyali (epoch'tan ms).
    heartbeat_ms: AtomicU64,
    epoch: Instant,
}

impl Shared {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

/// Arayüz bu kadar süre sinyal vermezse (sekme kapalı) çözücü kendiliğinden bekler.
const HEARTBEAT_TIMEOUT_MS: u64 = 500;
/// Bir hesap diliminin en uzun süresi: komutlar en geç bu kadar gecikir.
const SLICE: Duration = Duration::from_millis(24);

/// Arka plan yürütücüsü. Arayüz yalnız kanal ve atomik bayraklar kullanır; kilit yalnız
/// anlık görüntü işaretçisi değiştirilirken çok kısa süre tutulur.
pub struct FlowSim {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
    handle: Option<std::thread::JoinHandle<()>>,
}

/// Çözücü iş parçacığı sayısı: arayüze ve işletim sistemine pay bırakılır.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 6))
        .unwrap_or(1)
}

impl FlowSim {
    pub fn start() -> Result<FlowSim, String> {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            running: AtomicBool::new(true),
            quit: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            heartbeat_ms: AtomicU64::new(0),
            epoch: Instant::now(),
        });
        let sh = shared.clone();
        let handle = std::thread::Builder::new()
            .name("flow-tunnel".into())
            .spawn(move || worker(rx, sh))
            .map_err(|e| e.to_string())?;
        Ok(FlowSim {
            tx,
            shared,
            handle: Some(handle),
        })
    }

    pub fn set_setup(&self, s: SimSetup) {
        let _ = self.tx.send(Cmd::Setup(Box::new(s)));
    }

    pub fn reset(&self) {
        let _ = self.tx.send(Cmd::Reset);
    }

    pub fn set_running(&self, on: bool) {
        self.shared.running.store(on, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Relaxed)
    }

    /// Maske yeniden kuruluyor mu?
    pub fn is_busy(&self) -> bool {
        self.shared.busy.load(Ordering::Relaxed)
    }

    /// Arayüz her karede çağırır: tünel ekranı görünür.
    pub fn touch(&self) {
        self.shared
            .heartbeat_ms
            .store(self.shared.now_ms().max(1), Ordering::Relaxed);
    }

    /// İş parçacığı hâlâ çalışıyor mu (panik yaptıysa `false`).
    pub fn alive(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    pub fn latest(&self) -> Option<Arc<Snapshot>> {
        match self.shared.latest.try_lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
    }
}

impl Drop for FlowSim {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn publish(sh: &Shared, eng: &Engine, seq: &mut u64, sps: f64) {
    *seq += 1;
    if let Some(s) = eng.snapshot(*seq, sps) {
        let s = Arc::new(s);
        if let Ok(mut g) = sh.latest.lock() {
            *g = Some(s);
        }
    }
}

fn worker(rx: Receiver<Cmd>, sh: Arc<Shared>) {
    let mut eng = Engine::new(default_threads());
    let mut seq = 0u64;
    let mut sps = 0.0f64;
    let mut have_setup = false;
    loop {
        if sh.quit.load(Ordering::Relaxed) {
            return;
        }
        // Komutları boşalt: yalnız en son durum uygulanır (sürgü sürüklenirken birikmez).
        let mut latest: Option<SimSetup> = None;
        let mut reset = false;
        loop {
            match rx.try_recv() {
                Ok(Cmd::Setup(s)) => latest = Some(*s),
                Ok(Cmd::Reset) => reset = true,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let mut changed = false;
        if let Some(s) = latest {
            sh.busy.store(true, Ordering::Relaxed);
            eng.apply(s);
            sh.busy.store(false, Ordering::Relaxed);
            have_setup = true;
            changed = true;
        }
        if reset {
            eng.reset();
            changed = true;
        }
        let visible = sh
            .now_ms()
            .saturating_sub(sh.heartbeat_ms.load(Ordering::Relaxed))
            < HEARTBEAT_TIMEOUT_MS;
        if !have_setup || !sh.running.load(Ordering::Relaxed) || !visible {
            if changed && have_setup {
                eng.run(1);
                publish(&sh, &eng, &mut seq, 0.0);
            }
            std::thread::sleep(Duration::from_millis(15));
            continue;
        }
        let t0 = Instant::now();
        let mut n = 0usize;
        while t0.elapsed() < SLICE {
            if !eng.run(1) {
                n += 1;
                break;
            }
            n += 1;
        }
        let dt = t0.elapsed().as_secs_f64().max(1e-6);
        let inst = n as f64 / dt;
        sps = if sps > 0.0 {
            0.8 * sps + 0.2 * inst
        } else {
            inst
        };
        publish(&sh, &eng, &mut seq, sps);
        // Arayüze ve diğer işlere nefes payı.
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Hücre görünür bir katı mı (çizimde kullanılır)?
pub fn is_solid(c: u8) -> bool {
    c == SOLID
}

/// Hücre akışkan mı?
pub fn is_fluid(c: u8) -> bool {
    c == FLUID
}

/// Varsayılan kesit istasyonu (kesit türüne göre).
pub fn default_station(kind: CutKind) -> f64 {
    match kind {
        CutKind::Section => 0.35,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom3d::{ViewOptions, from_project};
    use crate::project::Project;

    #[test]
    fn units_mapping_is_sane_and_clamped() {
        let u = units(20.0, 0.25, 0.25 / 80.0, 1.0);
        assert!(u.u_lat >= U_LAT_MIN && u.u_lat <= U_LAT_MAX);
        assert!(u.tau >= TAU_MIN - 1e-12, "{}", u.tau);
        assert!((u.re_phys - 20.0 * 0.25 / NU_AIR).abs() < 1.0);
        assert!(u.re_sim <= u.re_phys);
        assert!((u.dt - u.dx * u.u_lat / 20.0).abs() < 1e-15);
        // Uç değerler ve bozuk girdiler kırpılır.
        for (v, l, dx) in [
            (0.0, 0.0, 0.0),
            (1e9, 1e9, 1e-9),
            (f64::NAN, f64::NAN, f64::NAN),
        ] {
            let u = units(v, l, dx, f64::NAN);
            assert!(u.tau.is_finite() && u.tau >= TAU_MIN - 1e-12);
            assert!(u.u_lat <= U_LAT_MAX && u.u_lat >= U_LAT_MIN);
        }
        // Düşük hızda ve küçük cisimde Re_sim = Re_gerçek.
        let u = units(1.0, 0.005, 0.005 / 30.0, 1.0);
        assert!((u.re_sim - u.re_phys).abs() < 1e-6);
        // Kurtarma çarpanı Re_sim'i düşürür, τ'yu yükseltir.
        let a = units(40.0, 0.3, 0.3 / 100.0, 1.0);
        let b = units(40.0, 0.3, 0.3 / 100.0, 0.5);
        assert!(b.re_sim < a.re_sim && b.tau > a.tau);
    }

    fn examples() -> Vec<(String, Project)> {
        [
            "fixed_wing_2kg_1600mm.json",
            "flying_wing_1200mm.json",
            "quad_250_x.json",
            "quad_275_x.json",
            "hexa_550_x.json",
            "vtol_quadplane.json",
        ]
        .iter()
        .map(|f| {
            (
                f.to_string(),
                crate::project::load_example(f).expect("örnek yüklenmeli"),
            )
        })
        .collect()
    }

    /// Örnek tasarımların her kesiti, UI'ın izin verdiği en zorlu ayarlarda (en yüksek hız,
    /// uç açılar) çok sayıda adım boyunca kararlı kalır: ıraksama/kurtarma olmaz, alanlar
    /// sonludur. Başsız test süresini (hata ayıklama derlemesi) sınırlı tutmak için düşük
    /// kalitede çalışır ve iki uç açı kesitler arasında dönüşümlü sınanır; τ_min ve U_lat
    /// kaliteden bağımsızdır, kaliteyle yalnız L_lat (ve böylece Re_sim) büyür.
    #[test]
    fn every_cut_of_every_example_is_stable() {
        let lib = crate::calc::Library::embedded();
        let threads = default_threads().min(4);
        for (ei, (name, p)) in examples().into_iter().enumerate() {
            let m = crate::calc::evaluate(&p, &lib);
            let spec = Arc::new(from_project(&p, &m, &ViewOptions::default()));
            for (ci, kind) in CutKind::ALL.into_iter().enumerate() {
                let alpha = if (ei + ci) % 2 == 0 {
                    super::super::section::ALPHA_MAX
                } else {
                    super::super::section::ALPHA_MIN
                };
                {
                    let mut eng = Engine::new(threads);
                    eng.apply(SimSetup {
                        spec: spec.clone(),
                        cut: CutParams {
                            kind,
                            station: default_station(kind),
                            alpha_deg: alpha,
                        },
                        quality: Quality::Low,
                        airspeed: AIRSPEED_MAX,
                    });
                    let clean = eng.run(800);
                    let u = eng.units();
                    let l = eng.lattice().unwrap();
                    let finite =
                        l.ux.iter()
                            .chain(&l.uy)
                            .chain(&l.rho)
                            .all(|v| v.is_finite());
                    println!(
                        "{name} {kind:?} α={alpha}: τ={:.4} Re_sim={:.0} kurtarma={}",
                        u.tau,
                        u.re_sim,
                        eng.recoveries()
                    );
                    assert!(
                        clean && eng.recoveries() == 0,
                        "{name} {kind:?} α={alpha} ıraksadı"
                    );
                    assert!(finite, "{name} {kind:?}");
                }
            }
        }
    }

    /// Yüksek kalitede (en büyük L_lat → en yüksek Re_sim) en zorlu iki kesit uzun süre kararlı.
    #[test]
    fn high_quality_worst_case_is_stable() {
        let lib = crate::calc::Library::embedded();
        let p = crate::project::load_example("fixed_wing_2kg_1600mm.json").unwrap();
        let m = crate::calc::evaluate(&p, &lib);
        let spec = Arc::new(from_project(&p, &m, &ViewOptions::default()));
        for kind in [CutKind::Section, CutKind::Side] {
            let mut eng = Engine::new(default_threads().min(6));
            eng.apply(SimSetup {
                spec: spec.clone(),
                cut: CutParams {
                    kind,
                    station: default_station(kind),
                    alpha_deg: 20.0,
                },
                quality: Quality::High,
                airspeed: AIRSPEED_MAX,
            });
            assert!(eng.run(1000), "{kind:?} ıraksadı");
            assert_eq!(eng.recoveries(), 0);
        }
    }

    /// Kurulum değişiklikleri: yalnız α değişince akış sıfırlanmaz; tür değişince sıfırlanır;
    /// kalite değişince kafes yeniden boyutlanır. Bozuk tasarımda panik yok.
    #[test]
    fn engine_apply_transitions() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let spec = Arc::new(from_project(&p, &m, &ViewOptions::default()));
        let mut s = SimSetup {
            spec: spec.clone(),
            cut: CutParams::default(),
            quality: Quality::Low,
            airspeed: 15.0,
        };
        let mut eng = Engine::new(2);
        eng.apply(s.clone());
        eng.run(50);
        assert_eq!(eng.lattice().unwrap().steps(), 50);
        s.cut.alpha_deg = 9.0;
        eng.apply(s.clone());
        assert_eq!(
            eng.lattice().unwrap().steps(),
            50,
            "α değişimi akışı sıfırlamamalı"
        );
        s.airspeed = 30.0;
        eng.apply(s.clone());
        assert_eq!(eng.lattice().unwrap().steps(), 50);
        s.cut.kind = CutKind::Side;
        eng.apply(s.clone());
        assert_eq!(
            eng.lattice().unwrap().steps(),
            0,
            "tür değişimi akışı sıfırlamalı"
        );
        s.quality = Quality::Medium;
        eng.apply(s.clone());
        assert_eq!(eng.lattice().unwrap().nx(), Quality::Medium.grid().0);
        let snap = eng.snapshot(1, 0.0).unwrap();
        assert_eq!(snap.ux.len(), snap.nx * snap.ny);
        // Bozuk tasarım: boş spec.
        s.spec = Arc::new(AircraftSpec::default());
        eng.apply(s.clone());
        assert!(eng.run(20));
        assert!(eng.snapshot(2, 0.0).unwrap().empty);
    }

    /// Arka plan yürütücüsü: başlar, görüntü yayımlar, duraklatılınca adım sayısı ilerlemez,
    /// düzgün kapanır.
    #[test]
    fn background_runner_publishes_and_pauses() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let sim = FlowSim::start().expect("iş parçacığı");
        sim.set_setup(SimSetup {
            spec: Arc::new(from_project(&p, &m, &ViewOptions::default())),
            cut: CutParams::default(),
            quality: Quality::Low,
            airspeed: 15.0,
        });
        let t0 = Instant::now();
        let mut got = None;
        while t0.elapsed() < Duration::from_secs(60) {
            sim.touch();
            if let Some(s) = sim.latest() {
                if s.step > 20 {
                    got = Some(s);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let s = got.expect("görüntü yayımlanmadı");
        assert!(sim.alive());
        assert_eq!(s.cells.len(), s.nx * s.ny);
        sim.set_running(false);
        std::thread::sleep(Duration::from_millis(120));
        let a = sim.latest().map(|s| s.step);
        std::thread::sleep(Duration::from_millis(150));
        let b = sim.latest().map(|s| s.step);
        assert_eq!(a, b, "duraklatınca ilerlememeli");
        // Sinyal kesilince (sekme kapalı) de ilerlemez.
        sim.set_running(true);
        std::thread::sleep(Duration::from_millis(HEARTBEAT_TIMEOUT_MS + 150));
        let a = sim.latest().map(|s| s.step);
        std::thread::sleep(Duration::from_millis(150));
        let b = sim.latest().map(|s| s.step);
        assert_eq!(a, b, "ekran görünmezken ilerlememeli");
        drop(sim);
    }
}
