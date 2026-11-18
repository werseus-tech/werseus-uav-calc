// SPDX-License-Identifier: AGPL-3.0-or-later
//! GNSS kaybında (sinyal kesilmesi ya da karıştırma) konum sapması için kapalı formlu hata
//! bütçesi: aracın kaç saniyede kaç metre "kayacağını" IMU sınıfı ve yardımcı sensörlere göre
//! karşılaştırır. Bu bir navigasyon filtresi değildir; eğitim amaçlı mertebe tahminidir.
//!
//! Saf ataletsel (INS) hata büyümesi, eksen başına 1σ (D. H. Titterton, J. L. Weston,
//! *Strapdown Inertial Navigation Technology*, 2nd ed., IET 2004, Böl. 12; O. J. Woodman,
//! "An introduction to inertial navigation", Cambridge UCAM-CL-TR-696, 2007):
//! - ivmeölçer sabit kayması b: e = ½·b·t²
//! - jiroskop sabit kayması ω (eğim hatası → yerçekimi sızıntısı): e = ⅙·g·ω·t³
//! - açısal rastgele yürüyüş N (ARW): e = g·N·t^{5/2}/√20 (Wiener sürecinin iki kat integrali)
//!
//! Yardımlı ölü hesap: hız hatası sabit kabul edilir → e = σ_v·t (hava hızı + manyetometrede
//! rüzgâr belirsizliği ve yön hatası; optik akışta irtifa × akış gürültüsü ve ölçek hatası).
//! Yatay hata 2B RMS (DRMS = √(σx² + σy²)) olarak verilir.
//!
//! IMU değerleri **tipik mertebe** örnekleridir (P. D. Groves, *Principles of GNSS, Inertial, and
//! Multisensor Integrated Navigation Systems*, 2nd ed., Artech House 2013, Böl. 4.4); belirli
//! bir ürünün veri sayfası değildir.

use super::G0;
use std::f64::consts::PI;

/// IMU sınıfı ön ayarları.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImuGrade {
    ConsumerMems,
    IndustrialMems,
    Tactical,
}

impl ImuGrade {
    pub const ALL: [ImuGrade; 3] = [
        ImuGrade::ConsumerMems,
        ImuGrade::IndustrialMems,
        ImuGrade::Tactical,
    ];

    /// Tipik (kalibrasyon sonrası, çalışma içi) değerler.
    pub fn spec(self) -> ImuSpec {
        match self {
            ImuGrade::ConsumerMems => ImuSpec {
                gyro_bias_dph: 20.0,
                arw_dpsh: 0.5,
                accel_bias_mg: 10.0,
            },
            ImuGrade::IndustrialMems => ImuSpec {
                gyro_bias_dph: 5.0,
                arw_dpsh: 0.2,
                accel_bias_mg: 2.0,
            },
            ImuGrade::Tactical => ImuSpec {
                gyro_bias_dph: 0.5,
                arw_dpsh: 0.05,
                accel_bias_mg: 0.3,
            },
        }
    }
}

/// IMU hata parametreleri.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImuSpec {
    /// Jiroskop sabit kayması [°/h].
    pub gyro_bias_dph: f64,
    /// Açısal rastgele yürüyüş [°/√h].
    pub arw_dpsh: f64,
    /// İvmeölçer sabit kayması [mg].
    pub accel_bias_mg: f64,
}

/// Yatay konum için yardımcı sensör.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aiding {
    /// Yalnız IMU.
    None,
    /// Hava hızı + manyetometre ile ölü hesap.
    AirspeedMag,
    /// Optik akış (+ mesafe sensörü) ile yer hızı; irtifa sınırı var.
    OpticalFlow,
}

impl Aiding {
    pub const ALL: [Aiding; 3] = [Aiding::None, Aiding::AirspeedMag, Aiding::OpticalFlow];
}

#[derive(Clone, Debug, PartialEq)]
pub struct DriftSettings {
    pub grade: ImuGrade,
    pub imu: ImuSpec,
    pub aiding: Aiding,
    pub baro: bool,
    /// Yer hızı [m/s] (0 = projeden).
    pub speed: f64,
    /// İrtifa (yerden) [m] (optik akış için).
    pub agl_m: f64,
    /// GNSS kesildiği andaki konum belirsizliği [m].
    pub sigma0_m: f64,
    /// Rüzgâr kestirimi belirsizliği (eksen başına) [m/s].
    pub wind_sigma: f64,
    /// Yön (heading) hatası [°].
    pub heading_sigma_deg: f64,
    /// Hava hızı hatası [m/s].
    pub airspeed_sigma: f64,
    /// Optik akış açısal hız gürültüsü/kayması [rad/s] ve ölçek hatası [-].
    pub flow_sigma_rad_s: f64,
    pub flow_scale: f64,
    /// Optik akışın çalıştığı en yüksek irtifa [m].
    pub flow_max_agl: f64,
    /// Barometre ilk belirsizliği [m] ve hava değişimi kaynaklı kayma [m/dk].
    pub baro_sigma_m: f64,
    pub baro_drift_m_min: f64,
    /// Eşikler [m] ve süre [s].
    pub thresholds: [f64; 2],
    pub horizon_s: f64,
    /// Monte-Carlo denetimi (saf INS, sabit tohum).
    pub monte_carlo: bool,
}

impl Default for DriftSettings {
    fn default() -> Self {
        DriftSettings {
            grade: ImuGrade::IndustrialMems,
            imu: ImuGrade::IndustrialMems.spec(),
            aiding: Aiding::None,
            baro: true,
            speed: 0.0,
            agl_m: 20.0,
            sigma0_m: 2.0,
            wind_sigma: 1.0,
            heading_sigma_deg: 2.0,
            airspeed_sigma: 0.5,
            flow_sigma_rad_s: 0.005,
            flow_scale: 0.03,
            flow_max_agl: 30.0,
            baro_sigma_m: 1.5,
            baro_drift_m_min: 0.15,
            thresholds: [10.0, 50.0],
            horizon_s: 300.0,
            monte_carlo: false,
        }
    }
}

fn nn(x: f64) -> f64 {
    if x.is_finite() { x.max(0.0) } else { 0.0 }
}

/// İvmeölçer sabit kayması kaynaklı konum hatası [m]: ½·b·g·t² (b: mg).
pub fn accel_bias_error(b_mg: f64, t: f64) -> f64 {
    0.5 * nn(b_mg) * 1e-3 * G0 * t * t
}

/// Jiroskop sabit kayması kaynaklı konum hatası [m]: ⅙·g·ω·t³ (ω: °/h).
pub fn gyro_bias_error(dph: f64, t: f64) -> f64 {
    let w = nn(dph) * PI / 180.0 / 3600.0;
    G0 * w * t.max(0.0).powi(3) / 6.0
}

/// ARW kaynaklı konum hatası [m]: g·N·t^{5/2}/√20 (N: °/√h → rad/√s).
pub fn arw_error(arw_dpsh: f64, t: f64) -> f64 {
    let n = nn(arw_dpsh) * PI / 180.0 / 60.0;
    G0 * n * t.max(0.0).powf(2.5) / 20f64.sqrt()
}

/// Bileşenleri ayrı ayrı (eksen başına 1σ) veren hata noktası.
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub t: f64,
    pub accel: f64,
    pub gyro: f64,
    pub arw: f64,
    /// Yardımlı hız hatası kaynaklı (DRMS) [m].
    pub aided: f64,
    /// Toplam yatay DRMS [m].
    pub horizontal: f64,
    /// Düşey 1σ [m].
    pub vertical: f64,
}

/// Optik akışın kullanılabilir olup olmadığı.
pub fn flow_valid(s: &DriftSettings) -> bool {
    s.aiding == Aiding::OpticalFlow && s.agl_m <= s.flow_max_agl
}

/// Yardımlı (velocity-aided) hız hatası DRMS [m/s]; yardım yoksa None.
pub fn aided_velocity_sigma(s: &DriftSettings, speed: f64) -> Option<f64> {
    let psi = nn(s.heading_sigma_deg).to_radians();
    let v = nn(speed);
    match s.aiding {
        Aiding::None => None,
        Aiding::AirspeedMag => {
            let w = nn(s.wind_sigma);
            Some((2.0 * w * w + (v * psi).powi(2) + nn(s.airspeed_sigma).powi(2)).sqrt())
        }
        Aiding::OpticalFlow => {
            if !flow_valid(s) {
                return None;
            }
            let fv = nn(s.agl_m) * nn(s.flow_sigma_rad_s);
            Some((2.0 * fv * fv + (nn(s.flow_scale) * v).powi(2) + (v * psi).powi(2)).sqrt())
        }
    }
}

/// t anındaki hata.
pub fn point_at(s: &DriftSettings, speed: f64, t: f64) -> Point {
    let t = nn(t);
    let accel = accel_bias_error(s.imu.accel_bias_mg, t);
    let gyro = gyro_bias_error(s.imu.gyro_bias_dph, t);
    let arw = arw_error(s.imu.arw_dpsh, t);
    let s0 = nn(s.sigma0_m);
    let (aided, horizontal) = match aided_velocity_sigma(s, speed) {
        Some(sv) => {
            let e = sv * t;
            (e, (s0 * s0 + e * e).sqrt())
        }
        None => {
            // Eksen başına 1σ → DRMS (iki eşit eksen): √2.
            let ax = (accel * accel + gyro * gyro + arw * arw).sqrt();
            (0.0, (s0 * s0 + 2.0 * ax * ax).sqrt())
        }
    };
    let vertical = if s.baro {
        nn(s.baro_sigma_m) + nn(s.baro_drift_m_min) * t / 60.0
    } else {
        (s0 * s0 + accel * accel).sqrt()
    };
    Point {
        t,
        accel,
        gyro,
        arw,
        aided,
        horizontal,
        vertical,
    }
}

/// 0…horizon arası eğri (`n` aralık).
pub fn curve(s: &DriftSettings, speed: f64, n: usize) -> Vec<Point> {
    let n = n.max(2);
    let h = if s.horizon_s.is_finite() && s.horizon_s > 0.0 {
        s.horizon_s.min(7200.0)
    } else {
        300.0
    };
    (0..=n)
        .map(|i| point_at(s, speed, h * i as f64 / n as f64))
        .collect()
}

/// Yatay hatanın eşiği ilk geçtiği an [s] (1 saate kadar aranır; aşılmazsa None).
pub fn time_to_exceed(s: &DriftSettings, speed: f64, threshold_m: f64) -> Option<f64> {
    let thr = nn(threshold_m);
    let f = |t: f64| point_at(s, speed, t).horizontal;
    if f(0.0) >= thr {
        return Some(0.0);
    }
    let t_max = 3600.0;
    if f(t_max) < thr {
        return None;
    }
    // Hata zamanla monoton artar → ikiye bölme.
    let (mut lo, mut hi) = (0.0, t_max);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if f(mid) >= thr {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(hi)
}

/// Küçük, deterministik sözde rastgele üreteç (xorshift64*), Box–Muller ile normal dağılım.
struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        let u1 = self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
    }
}

/// Saf INS için Monte-Carlo: rastgele sabit kaymalar + ARW yürüyüşüyle iki eksen ayrı ayrı
/// entegre edilir; her `dt` adımında yatay DRMS döner. Kapalı form ile karşılaştırma içindir.
pub fn monte_carlo_ins(
    imu: &ImuSpec,
    horizon_s: f64,
    dt: f64,
    runs: usize,
    seed: u64,
) -> Vec<[f64; 2]> {
    let dt = if dt > 0.0 { dt } else { 1.0 };
    let steps = (nn(horizon_s) / dt).ceil().max(1.0) as usize;
    let sa = nn(imu.accel_bias_mg) * 1e-3 * G0;
    let sg = nn(imu.gyro_bias_dph) * PI / 180.0 / 3600.0;
    let sn = nn(imu.arw_dpsh) * PI / 180.0 / 60.0;
    let mut sum_sq = vec![0.0; steps + 1];
    let mut rng = Rng(seed | 1);
    let runs = runs.max(1);
    for _ in 0..runs {
        for _axis in 0..2 {
            let b = sa * rng.normal();
            let w = sg * rng.normal();
            let (mut theta, mut v, mut x) = (0.0f64, 0.0f64, 0.0f64);
            for acc in sum_sq.iter_mut().skip(1) {
                // Eğim: sabit kayma + ARW (adım başına N·√dt).
                let theta_prev = theta;
                theta += w * dt + sn * dt.sqrt() * rng.normal();
                let a = b + G0 * 0.5 * (theta + theta_prev);
                let v_prev = v;
                v += a * dt;
                x += 0.5 * (v + v_prev) * dt;
                *acc += x * x;
            }
        }
    }
    sum_sq
        .iter()
        .enumerate()
        .map(|(k, s2)| [k as f64 * dt, (s2 / runs as f64).sqrt()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gyro_bias_error_is_zero_for_negative_time() {
        assert_eq!(gyro_bias_error(1.0, -5.0), 0.0);
        assert!(gyro_bias_error(1.0, 5.0) > 0.0);
    }

    fn near(a: f64, b: f64, rel: f64) {
        assert!(((a - b) / b).abs() < rel, "{a} vs {b}");
    }

    #[test]
    fn hand_values() {
        // 1 mg, 10 s: ½·0.00980665·100 = 0.4903 m.
        near(accel_bias_error(1.0, 10.0), 0.490_332_5, 1e-6);
        // 10 °/h, 60 s: ω = 4.8481e-5 rad/s; g·ω·t³/6 = 17.116 m.
        near(gyro_bias_error(10.0, 60.0), 17.116, 1e-3);
        // 0.1 °/√h, 100 s: N = 2.9089e-5 rad/√s; g·N·100^2.5/√20 = 6.379 m.
        near(arw_error(0.1, 100.0), 6.379, 1e-3);
        assert_eq!(accel_bias_error(5.0, 0.0), 0.0);
    }

    #[test]
    fn better_imu_drifts_slower() {
        let mut s = DriftSettings::default();
        let mut times = Vec::new();
        for g in ImuGrade::ALL {
            s.grade = g;
            s.imu = g.spec();
            times.push(time_to_exceed(&s, 18.0, 10.0).unwrap_or(f64::INFINITY));
        }
        assert!(times[0] < times[1] && times[1] < times[2], "{times:?}");
        let mut e = |g: ImuGrade| {
            s.imu = g.spec();
            point_at(&s, 18.0, 60.0).horizontal
        };
        assert!(e(ImuGrade::ConsumerMems) > e(ImuGrade::IndustrialMems));
        assert!(e(ImuGrade::IndustrialMems) > e(ImuGrade::Tactical));
    }

    #[test]
    fn aiding_reduces_drift() {
        let mut s = DriftSettings::default();
        s.imu = ImuGrade::ConsumerMems.spec();
        let ins = point_at(&s, 18.0, 120.0).horizontal;
        s.aiding = Aiding::AirspeedMag;
        let dr = point_at(&s, 18.0, 120.0).horizontal;
        s.aiding = Aiding::OpticalFlow;
        let flow = point_at(&s, 5.0, 120.0).horizontal;
        assert!(dr < ins && flow < ins, "ins {ins} dr {dr} flow {flow}");
        // Optik akış irtifa sınırının üstünde çalışmaz → INS'e döner.
        s.agl_m = 200.0;
        assert!(!flow_valid(&s));
        near(point_at(&s, 5.0, 120.0).horizontal, ins, 1e-12);
        // Hava hızı ölü hesabında hata doğrusal büyür: hand value.
        let mut d = DriftSettings::default();
        d.aiding = Aiding::AirspeedMag;
        d.sigma0_m = 0.0;
        d.wind_sigma = 1.5;
        d.heading_sigma_deg = 3.0;
        let sv = (2.0f64 * 1.5 * 1.5 + (18.0 * 3f64.to_radians()).powi(2) + 0.25).sqrt();
        near(point_at(&d, 18.0, 100.0).horizontal, sv * 100.0, 1e-12);
    }

    #[test]
    fn threshold_times_are_ordered() {
        let s = DriftSettings::default();
        let a = time_to_exceed(&s, 15.0, 10.0).unwrap();
        let b = time_to_exceed(&s, 15.0, 50.0).unwrap();
        assert!(a < b);
        near(point_at(&s, 15.0, a).horizontal, 10.0, 1e-6);
        // Baro ile düşey hata sınırlı kalır.
        let mut nb = s.clone();
        nb.baro = false;
        assert!(point_at(&s, 15.0, 300.0).vertical < point_at(&nb, 15.0, 300.0).vertical);
    }

    #[test]
    fn monte_carlo_agrees_with_closed_form() {
        let imu = ImuGrade::IndustrialMems.spec();
        let mc = monte_carlo_ins(&imu, 120.0, 0.5, 400, 42);
        let last = mc.last().unwrap();
        let mut s = DriftSettings::default();
        s.imu = imu;
        s.sigma0_m = 0.0;
        let cf = point_at(&s, 0.0, last[0]).horizontal;
        near(last[1], cf, 0.15);
        // Sabit tohum: tekrarlanabilir.
        assert_eq!(mc, monte_carlo_ins(&imu, 120.0, 0.5, 400, 42));
    }
}
