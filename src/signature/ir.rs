// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kızılötesi (IR) iz için kaba, eğitim amaçlı tahmin.
//!
//! Zincir: güç akışı (projenin itki/pil hesapları) → kayıp ısı (motor bakır+demir kaybı, ESC
//! kaybı, pil I²R) → kararlı durum yüzey sıcaklığı (zorlanmış taşınım + ışınım dengesi) →
//! 3–5 µm (MWIR) ve 8–12 µm (LWIR) bantlarında Planck ışıması ve ortama göre karşıtlık.
//!
//! Kaynaklar:
//! - F. P. Incropera, D. P. DeWitt et al., *Fundamentals of Heat and Mass Transfer*, 7th ed.,
//!   Wiley 2011: Planck yasası (Denk. 12.30), kara cisim bant kesri F(0→λT) (Tablo 12.2),
//!   Stefan–Boltzmann, silindir çapraz akış Churchill–Bernstein (Denk. 7.54), düz levha laminer/
//!   türbülanslı ortalama Nusselt (Denk. 7.30, 7.38).
//! - W. C. Widger, M. P. Woodall, "Integration of the Planck blackbody radiation function",
//!   *Bull. Am. Meteorol. Soc.* 57 (1976): F(0→λT) için hızlı yakınsayan seri.
//! - R. D. Hudson, *Infrared System Engineering*, Wiley 1969: bant ışıma şiddeti I = ε·M·A/π
//!   (Lambert yüzeyi), karşıtlık ΔI.
//!
//! Basitleştirmeler: atmosfer geçirgenliği, güneş yansıması ve gökyüzü arka planı yok; yüzey gri ve
//! Lambert kabul edilir; iç parçalar (gövde içi ESC/pil) ısısını kabuktaki bir yamaya yayar ve iç
//! ısıl direnç yok sayılır. Sonuçlar mertebe tahminidir.

use crate::calc::Model;
use crate::calc::custom_battery::BatterySourceKind;
use crate::project::{DroneType, Project};
use std::f64::consts::PI;

/// Stefan–Boltzmann sabiti [W/(m²·K⁴)].
pub const SIGMA_SB: f64 = 5.670_374_419e-8;
/// İkinci ışınım sabiti c₂ = hc/k [m·K].
pub const C2: f64 = 1.438_776_877e-2;
/// Birinci ışınım sabiti (yayılım için) c₁ = 2πhc² [W·m²].
pub const C1: f64 = 3.741_771_852e-16;

/// Bant: (alt, üst) dalga boyu [m].
pub const MWIR: (f64, f64) = (3.0e-6, 5.0e-6);
pub const LWIR: (f64, f64) = (8.0e-6, 12.0e-6);

/// Kara cisim spektral yayılımı M_λ [W/(m²·m)] (Planck).
pub fn planck_exitance(lambda_m: f64, t_k: f64) -> f64 {
    if !(lambda_m > 0.0 && t_k > 0.0) {
        return 0.0;
    }
    let x = C2 / (lambda_m * t_k);
    if x > 700.0 {
        return 0.0;
    }
    C1 / (lambda_m.powi(5) * x.exp_m1())
}

/// Kara cisim yayılımının 0…λ arasındaki kesri F(0→λT) (Widger–Woodall serisi).
pub fn blackbody_fraction(lambda_m: f64, t_k: f64) -> f64 {
    if !(lambda_m > 0.0 && t_k > 0.0) {
        return 0.0;
    }
    let x = C2 / (lambda_m * t_k);
    let k = 15.0 / PI.powi(4);
    if x < 0.05 {
        // Rayleigh–Jeans tarafı: 1 − (15/π⁴)·x³·(1/3 − x/8 + x²/60).
        return (1.0 - k * x.powi(3) * (1.0 / 3.0 - x / 8.0 + x * x / 60.0)).clamp(0.0, 1.0);
    }
    let mut sum = 0.0;
    for n in 1..=2000 {
        let nf = n as f64;
        let term = (-nf * x).exp() / nf
            * (x.powi(3) + 3.0 * x * x / nf + 6.0 * x / (nf * nf) + 6.0 / nf.powi(3));
        sum += term;
        if term < 1e-15 * sum.max(1e-300) {
            break;
        }
    }
    (k * sum).clamp(0.0, 1.0)
}

/// Kara cisim bant yayılımı [W/m²]: σT⁴·(F(λ₂T) − F(λ₁T)).
pub fn band_exitance(band: (f64, f64), t_k: f64) -> f64 {
    if !t_k.is_finite() || t_k <= 0.0 {
        return 0.0;
    }
    SIGMA_SB
        * t_k.powi(4)
        * (blackbody_fraction(band.1, t_k) - blackbody_fraction(band.0, t_k)).max(0.0)
}

/// Planck fonksiyonunun Simpson kuralıyla sayısal integrali [W/m²] (seri için bağımsız denetim).
pub fn band_exitance_numeric(band: (f64, f64), t_k: f64, n: usize) -> f64 {
    let n = (n.max(2) / 2) * 2;
    let (a, b) = band;
    let h = (b - a) / n as f64;
    let mut s = planck_exitance(a, t_k) + planck_exitance(b, t_k);
    for i in 1..n {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        s += w * planck_exitance(a + i as f64 * h, t_k);
    }
    s * h / 3.0
}

// ------------------------------------------------------------------------------------------
// Taşınım
// ------------------------------------------------------------------------------------------

/// Hava özellikleri (film sıcaklığında sabit kabul).
const K_AIR: f64 = 0.026;
const PR_AIR: f64 = 0.71;
/// Doğal taşınım alt sınırı [W/(m²·K)].
pub const H_MIN: f64 = 5.0;

/// Çapraz akışta silindir, Churchill–Bernstein: ortalama h [W/(m²·K)].
pub fn h_cylinder(v: f64, d: f64, nu: f64) -> f64 {
    if !(v > 0.0 && d > 0.0 && nu > 0.0) {
        return H_MIN;
    }
    let re = v * d / nu;
    let nu_d = 0.3
        + 0.62 * re.sqrt() * PR_AIR.cbrt() / (1.0 + (0.4 / PR_AIR).powf(2.0 / 3.0)).powf(0.25)
            * (1.0 + (re / 282_000.0).powf(5.0 / 8.0)).powf(0.8);
    (nu_d * K_AIR / d).max(H_MIN)
}

/// Düz levha boyunca akış: ortalama h (laminer 0.664 Re^½ Pr^⅓, Re > 5·10⁵ türbülanslı 0.037 Re^0.8 Pr^⅓).
pub fn h_plate(v: f64, l: f64, nu: f64) -> f64 {
    if !(v > 0.0 && l > 0.0 && nu > 0.0) {
        return H_MIN;
    }
    let re = v * l / nu;
    let nu_l = if re < 5e5 {
        0.664 * re.sqrt() * PR_AIR.cbrt()
    } else {
        0.037 * re.powf(0.8) * PR_AIR.cbrt()
    };
    (nu_l * K_AIR / l).max(H_MIN)
}

/// Kararlı yüzey sıcaklığı [K]: q = hA(T − Ta) + εσA(T⁴ − Ta⁴), ikiye bölme ile.
pub fn surface_temperature(q_w: f64, area: f64, h: f64, eps: f64, t_amb: f64) -> f64 {
    if !(q_w > 0.0 && area > 0.0 && t_amb > 0.0 && h.is_finite() && h >= 0.0)
        || !(eps.is_finite() && eps >= 0.0)
    {
        return t_amb;
    }
    let f =
        |t: f64| h * area * (t - t_amb) + eps * SIGMA_SB * area * (t.powi(4) - t_amb.powi(4)) - q_w;
    let (mut lo, mut hi) = (t_amb, t_amb + 2000.0);
    if f(hi) < 0.0 {
        return hi;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if f(mid) > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    0.5 * (lo + hi)
}

// ------------------------------------------------------------------------------------------
// Isı kaynakları
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Motor,
    Esc,
    Battery,
}

/// Uçuş evresi (hangi güç değeri kullanılır).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Cruise,
    Hover,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IrSettings {
    /// Ortam sıcaklığı [°C]; None: proje irtifasında ISA.
    pub ambient_c: Option<f64>,
    /// VTOL için evre (multirotor her zaman askı, sabit kanat her zaman seyir).
    pub phase: Phase,
    pub eps_motor: f64,
    pub eps_esc: f64,
    pub eps_battery: f64,
    /// Gövde kabuğunun ışınım yayıcılığı (iç kaynaklar kabuktan görünür).
    pub eps_skin: f64,
    /// Motor kaybının bakır (I²R) payı; kalanı demir (histerezis + girdap) kaybı.
    pub copper_fraction: f64,
    /// ESC ve pil dışarıda mı (açıkta)? None: türe göre (multirotor açık, sabit kanat gövde içi).
    pub esc_exposed: Option<bool>,
    pub battery_exposed: Option<bool>,
    /// İç kaynağın ısısını yaydığı kabuk yamasının, parça yüzeyine oranı.
    pub skin_spread: f64,
    /// Hücre iç direnci [mΩ]; 0: projeden ya da kimyaya göre tipik.
    pub r_cell_mohm: f64,
}

impl Default for IrSettings {
    fn default() -> Self {
        IrSettings {
            ambient_c: None,
            phase: Phase::Cruise,
            eps_motor: 0.85,
            eps_esc: 0.9,
            eps_battery: 0.9,
            eps_skin: 0.9,
            copper_fraction: 0.6,
            esc_exposed: None,
            battery_exposed: None,
            skin_spread: 4.0,
            r_cell_mohm: 0.0,
        }
    }
}

/// Bir ısı kaynağının sonucu (adet başına değerler).
#[derive(Clone, Debug)]
pub struct SourceResult {
    pub kind: SourceKind,
    pub count: u32,
    /// Adet başına kayıp ısı [W].
    pub heat_w: f64,
    /// Görünen (ışıyan) yüzey alanı [m²] ve izdüşüm alanı [m²].
    pub area_m2: f64,
    pub proj_m2: f64,
    pub eps: f64,
    pub h: f64,
    pub air_v: f64,
    /// Açıkta mı, yoksa kabuk yamasından mı görünüyor?
    pub exposed: bool,
    pub t_k: f64,
    /// Bant ışıma şiddeti [W/sr] ve ortama göre karşıtlık [W/sr], adet başına.
    pub i_mwir: f64,
    pub i_lwir: f64,
    pub di_mwir: f64,
    pub di_lwir: f64,
}

impl SourceResult {
    pub fn delta_k(&self, t_amb: f64) -> f64 {
        self.t_k - t_amb
    }
    /// Bant ışıması (radyans) [W/(m²·sr)]: ε·M/π.
    pub fn radiance(&self, band: (f64, f64)) -> f64 {
        self.eps * band_exitance(band, self.t_k) / PI
    }
}

#[derive(Clone, Debug)]
pub struct IrResult {
    pub t_amb_k: f64,
    pub phase: Phase,
    /// Pil akımı [A] ve paket direnci [Ω].
    pub i_batt: f64,
    pub r_pack: f64,
    pub motor_copper_w: f64,
    pub motor_iron_w: f64,
    pub sources: Vec<SourceResult>,
    pub total_mwir: f64,
    pub total_lwir: f64,
    pub total_di_mwir: f64,
    pub total_di_lwir: f64,
}

fn pos(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 {
        x
    } else {
        fallback
    }
}

fn frac(x: f64, fallback: f64) -> f64 {
    if x.is_finite() && x > 0.0 && x <= 1.0 {
        x
    } else {
        fallback
    }
}

/// Hücre iç direnci [Ω]: kullanıcı > proje (özel/şablon hücre) > kimyaya göre tipik
/// (LiPo ≈ 12 mΩ·Ah / C, Li-ion ≈ 60 mΩ·Ah / C; 1…80 mΩ).
pub fn cell_resistance(p: &Project, m: &Model, user_mohm: f64) -> f64 {
    if user_mohm.is_finite() && user_mohm > 0.0 {
        return user_mohm * 1e-3;
    }
    let from_proj = match p.battery_source {
        BatterySourceKind::CustomCell => p.custom_cell.r_int_mohm,
        BatterySourceKind::GenericTemplate => p.generic_cell.r_int_mohm,
        _ => 0.0,
    };
    if from_proj.is_finite() && from_proj > 0.0 {
        return from_proj * 1e-3;
    }
    let cap = pos(m.cell.capacity_ah, 3.0);
    let chem = m.cell.chemistry.to_ascii_lowercase();
    let k = if chem.contains("ion") { 60.0 } else { 12.0 };
    (k / cap).clamp(1.0, 80.0) * 1e-3
}

/// Pervane arkasındaki indüklenmiş hız (momentum teorisi): v_i = ½(−V + √(V² + 2T/(ρA))).
pub fn induced_velocity(thrust: f64, rho: f64, disk_area: f64, v: f64) -> f64 {
    if !(thrust > 0.0 && rho > 0.0 && disk_area > 0.0) {
        return 0.0;
    }
    let v = v.max(0.0);
    0.5 * (-v + (v * v + 2.0 * thrust / (rho * disk_area)).sqrt())
}

/// Varsayılan evre: multirotor askı, sabit kanat seyir, VTOL ayardan.
pub fn effective_phase(p: &Project, s: &IrSettings) -> Phase {
    match p.drone_type {
        DroneType::Multirotor => Phase::Hover,
        DroneType::VtolQuadplane => s.phase,
        _ => Phase::Cruise,
    }
}

/// Projeden ısı kaynakları ve IR iz.
pub fn evaluate(p: &Project, m: &Model, s: &IrSettings, motor_d_m: f64) -> IrResult {
    let t_amb = match s.ambient_c {
        Some(c) if c.is_finite() => c + 273.15,
        _ => pos(m.atmo.t, 288.15),
    };
    let nu = pos(m.atmo.nu, 1.5e-5);
    let rho = pos(m.atmo.rho, 1.2);
    let phase = effective_phase(p, s);
    let eta_mot = frac(p.eta_mot, 0.8);
    let eta_esc = frac(p.eta_esc, 0.95);
    let multirotor = p.drone_type == DroneType::Multirotor;

    // Güç akışı: pil tarafı itki gücü (toplam), motor sayısı, uçuş hızı, motor çevresi hava hızı.
    let (p_prop_batt, n_mot, v_flight, v_wash, p_hotel) = match phase {
        Phase::Hover => {
            let n = m.mr.n_motors.max(1);
            let p_tot = pos(m.mr.p_elec_hover_total, 0.0);
            let hotel = (m.mr.p_total_hover - m.mr.p_elec_hover_total).max(0.0);
            let hotel = if hotel.is_finite() { hotel } else { 0.0 };
            (p_tot, n, 0.0, pos(m.mr.v_i, 5.0), hotel)
        }
        Phase::Cruise => {
            let v = pos(p.v_cr, 15.0);
            let vi = induced_velocity(m.prop.t_cr, rho, m.prop.a_disk, v);
            (
                pos(m.prop.p_elec_cr, 0.0),
                1,
                v,
                v + vi,
                pos(p.p_hotel, 0.0),
            )
        }
    };
    let p_per = p_prop_batt / n_mot as f64;
    let esc_loss = p_per * (1.0 - eta_esc);
    let motor_loss = p_per * eta_esc * (1.0 - eta_mot);
    let cu = s.copper_fraction.clamp(0.0, 1.0);

    // Pil.
    let v_pack = pos(m.weight.pack_voltage, 14.8);
    let i_batt = (p_prop_batt + p_hotel) / v_pack;
    let n_s = m.weight.n_cells.max(1) as f64;
    let n_p = if p.battery_source == BatterySourceKind::CustomPack {
        1.0
    } else {
        p.n_parallel.max(1) as f64
    };
    // n_cells = seri × paralel; seri sayısı:
    let n_series = (n_s / n_p).max(1.0);
    let r_pack = cell_resistance(p, m, s.r_cell_mohm) * n_series / n_p;
    let batt_loss = i_batt * i_batt * r_pack;

    let esc_exposed = s.esc_exposed.unwrap_or(multirotor);
    let batt_exposed = s.battery_exposed.unwrap_or(multirotor);
    let spread = pos(s.skin_spread, 4.0).max(1.0);

    let mut sources = Vec::new();
    let mut add = |kind: SourceKind,
                   count: u32,
                   heat: f64,
                   area: f64,
                   proj: f64,
                   eps: f64,
                   h: f64,
                   air_v: f64,
                   exposed: bool| {
        let t = surface_temperature(heat, area, h, eps, t_amb);
        let i = |band| eps * band_exitance(band, t) * proj / PI;
        let di = |band| eps * (band_exitance(band, t) - band_exitance(band, t_amb)) * proj / PI;
        sources.push(SourceResult {
            kind,
            count,
            heat_w: heat,
            area_m2: area,
            proj_m2: proj,
            eps,
            h,
            air_v,
            exposed,
            t_k: t,
            i_mwir: i(MWIR),
            i_lwir: i(LWIR),
            di_mwir: di(MWIR),
            di_lwir: di(LWIR),
        });
    };

    // Motor: silindir (yan yüz + bir kapak), pervane akışında.
    let d = pos(motor_d_m, 0.03);
    let hm = 0.8 * d;
    let a_motor = PI * d * hm + PI * d * d / 4.0;
    add(
        SourceKind::Motor,
        n_mot,
        motor_loss,
        a_motor,
        d * hm,
        frac(s.eps_motor, 0.85),
        h_cylinder(v_wash, d, nu),
        v_wash,
        true,
    );

    // ESC: kart alanı akımla ölçeklenir (iki yüz); açıkta ise pervane akışında, değilse kabuk yaması.
    let i_esc = (p_per / v_pack).max(1.0);
    let a_esc = (6e-5 * i_esc).max(1.5e-3);
    let eps_skin = frac(s.eps_skin, 0.9);
    let skin_v = v_flight.max(0.0);
    if esc_exposed {
        let l = (a_esc / 2.0).sqrt();
        let v = if multirotor {
            v_wash
        } else {
            v_wash.max(skin_v)
        };
        add(
            SourceKind::Esc,
            n_mot,
            esc_loss,
            a_esc,
            a_esc / 2.0,
            frac(s.eps_esc, 0.9),
            h_plate(v, l, nu),
            v,
            true,
        );
    } else {
        // Kabuk yaması: ısı tek (dış) yüzden yayılır; yama dik görülür, izdüşüm = yama alanı.
        let a = spread * 0.5 * a_esc;
        add(
            SourceKind::Esc,
            n_mot,
            esc_loss,
            a,
            a,
            eps_skin,
            h_plate(skin_v, a.sqrt(), nu),
            skin_v,
            false,
        );
    }

    // Pil: kutu (3:1.5:1, 2200 kg/m³).
    let m_b = pos(m.weight.m_battery, 0.3);
    let hb = (m_b / 2200.0 / 4.5).cbrt();
    let (lb, wb) = (3.0 * hb, 1.5 * hb);
    let a_batt = 2.0 * (lb * wb + lb * hb + wb * hb);
    if batt_exposed {
        let v = if multirotor { 0.5 * v_wash } else { skin_v };
        add(
            SourceKind::Battery,
            1,
            batt_loss,
            a_batt,
            lb * hb,
            frac(s.eps_battery, 0.9),
            h_plate(v, lb, nu),
            v,
            true,
        );
    } else {
        let a = spread * lb * hb;
        add(
            SourceKind::Battery,
            1,
            batt_loss,
            a,
            a,
            eps_skin,
            h_plate(skin_v, a.sqrt(), nu),
            skin_v,
            false,
        );
    }

    let sum = |f: &dyn Fn(&SourceResult) -> f64| -> f64 {
        sources.iter().map(|r| r.count as f64 * f(r)).sum()
    };
    let total_mwir = sum(&|r| r.i_mwir);
    let total_lwir = sum(&|r| r.i_lwir);
    let total_di_mwir = sum(&|r| r.di_mwir);
    let total_di_lwir = sum(&|r| r.di_lwir);
    IrResult {
        t_amb_k: t_amb,
        phase,
        i_batt,
        r_pack,
        motor_copper_w: motor_loss * cu,
        motor_iron_w: motor_loss * (1.0 - cu),
        sources,
        total_mwir,
        total_lwir,
        total_di_mwir,
        total_di_lwir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, rel: f64) {
        assert!(((a - b) / b).abs() < rel, "{a} vs {b}");
    }

    #[test]
    fn planck_integral_gives_stefan_boltzmann() {
        for t in [250.0, 300.0, 400.0, 600.0] {
            // Bantları birleştirerek tüm spektrum (0.5 µm … 1 mm) sayısal integrali.
            let edges = [0.5e-6, 2e-6, 5e-6, 10e-6, 20e-6, 50e-6, 200e-6, 1000e-6];
            let mut total = 0.0;
            for w in edges.windows(2) {
                total += band_exitance_numeric((w[0], w[1]), t, 4000);
            }
            near(total, SIGMA_SB * t.powi(4), 2e-3);
            // Seri kesri de tam spektrumda ~1.
            let f = blackbody_fraction(1e-3, t) - blackbody_fraction(0.5e-6, t);
            near(f, 1.0, 2e-3);
        }
    }

    #[test]
    fn series_matches_numeric_band() {
        for (band, t) in [(MWIR, 300.0), (LWIR, 300.0), (MWIR, 350.0), (LWIR, 330.0)] {
            near(
                band_exitance(band, t),
                band_exitance_numeric(band, t, 2000),
                1e-6,
            );
        }
    }

    #[test]
    fn band_fractions_at_300k() {
        // Incropera Tablo 12.2: F(2400 µm·K) = 0.1403, F(3600) = 0.4036, F(900) ≈ 0.0001, F(1500) ≈ 0.0128.
        near(blackbody_fraction(8e-6, 300.0), 0.1403, 0.01);
        near(blackbody_fraction(12e-6, 300.0), 0.4036, 0.01);
        near(blackbody_fraction(5e-6, 300.0), 0.01285, 0.01);
        let fl = blackbody_fraction(12e-6, 300.0) - blackbody_fraction(8e-6, 300.0);
        let fm = blackbody_fraction(5e-6, 300.0) - blackbody_fraction(3e-6, 300.0);
        assert!((0.25..0.28).contains(&fl), "{fl}");
        assert!((0.010..0.016).contains(&fm), "{fm}");
        // MWIR sıcaklığa LWIR'den çok daha duyarlı.
        let r_m = band_exitance(MWIR, 350.0) / band_exitance(MWIR, 300.0);
        let r_l = band_exitance(LWIR, 350.0) / band_exitance(LWIR, 300.0);
        assert!(r_m > 2.0 * r_l, "{r_m} {r_l}");
    }

    #[test]
    fn surface_balance_closes() {
        let (q, a, h, e, ta) = (15.0, 2.5e-3, 80.0, 0.85, 288.15);
        let t = surface_temperature(q, a, h, e, ta);
        let back = h * a * (t - ta) + e * SIGMA_SB * a * (t.powi(4) - ta.powi(4));
        near(back, q, 1e-6);
        assert_eq!(surface_temperature(0.0, a, h, e, ta), ta);
        // Daha çok hava → daha soğuk.
        assert!(h_cylinder(10.0, 0.03, 1.5e-5) > h_cylinder(2.0, 0.03, 1.5e-5));
    }

    #[test]
    fn induced_velocity_hover_formula() {
        // Askı: v_i = √(T/(2ρA)).
        near(
            induced_velocity(10.0, 1.2, 0.05, 0.0),
            (10.0f64 / (2.0 * 1.2 * 0.05)).sqrt(),
            1e-12,
        );
        assert!(induced_velocity(10.0, 1.2, 0.05, 20.0) < induced_velocity(10.0, 1.2, 0.05, 0.0));
    }

    #[test]
    fn hidden_sources_look_cooler_than_exposed() {
        let p = Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut s = IrSettings::default();
        s.esc_exposed = Some(true);
        let a = evaluate(&p, &m, &s, 0.03);
        s.esc_exposed = Some(false);
        let b = evaluate(&p, &m, &s, 0.03);
        let esc = |r: &IrResult| {
            r.sources
                .iter()
                .find(|x| x.kind == SourceKind::Esc)
                .unwrap()
                .t_k
        };
        assert!(esc(&b) < esc(&a), "{} {}", esc(&b), esc(&a));
    }

    #[test]
    fn hidden_patch_projection_is_not_divided_by_pi() {
        // Fixed wing, hidden battery and ESC: izdüşüm = yama alanı; yama = yayılım x tek yüz.
        let mut p = Project::default();
        p.set_drone_type(DroneType::FixedWingConventional);
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut s = IrSettings::default();
        s.battery_exposed = Some(false);
        s.esc_exposed = Some(false);
        let r = evaluate(&p, &m, &s, 0.03);
        for k in [SourceKind::Esc, SourceKind::Battery] {
            let x = r.sources.iter().find(|x| x.kind == k).unwrap();
            assert!(!x.exposed);
            assert!((x.proj_m2 - x.area_m2).abs() < 1e-12, "{k:?}");
        }
        // Gizli pilin izdüşümü, açıktaki pilin izdüşümünün yayılım katıdır (7.6 kat değil).
        s.battery_exposed = Some(true);
        let e = evaluate(&p, &m, &s, 0.03);
        let proj = |r: &IrResult| {
            r.sources
                .iter()
                .find(|x| x.kind == SourceKind::Battery)
                .unwrap()
                .proj_m2
        };
        assert!((proj(&r) / proj(&e) - s.skin_spread.max(1.0)).abs() < 1e-9);
    }

    #[test]
    fn non_finite_inputs_are_rejected() {
        assert_eq!(band_exitance(LWIR, f64::INFINITY), 0.0);
        assert_eq!(surface_temperature(1.0, 0.01, f64::NAN, 0.9, 290.0), 290.0);
        assert_eq!(surface_temperature(1.0, 0.01, 10.0, -1.0, 290.0), 290.0);
    }
}
