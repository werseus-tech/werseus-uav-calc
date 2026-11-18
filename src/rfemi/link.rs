// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bağ bütçesi: serbest uzay yol kaybı, menzil, Fresnel bölgesi, polarizasyon kaybı.

use super::emi::AntennaLoc;

/// Işık hızı (m/s).
const C: f64 = 299_792_458.0;

/// Serbest uzay yol kaybı (dB): 20log10(d_km) + 20log10(f_MHz) + 32.44.
pub fn fspl_db(d_km: f64, f_mhz: f64) -> f64 {
    let d = d_km.max(1e-9);
    let f = f_mhz.max(1e-6);
    20.0 * d.log10() + 20.0 * f.log10() + 32.44
}

/// Verilen izinli yol kaybında (dB) en büyük mesafe (km); `fspl_db` ile ters işlemdir.
pub fn range_km_for_loss(loss_db: f64, f_mhz: f64) -> f64 {
    let f = f_mhz.max(1e-6);
    10f64.powf((loss_db - 32.44 - 20.0 * f.log10()) / 20.0)
}

/// Dalga boyu (m).
pub fn wavelength_m(f_mhz: f64) -> f64 {
    C / (f_mhz.max(1e-6) * 1e6)
}

/// Yol ortasındaki birinci Fresnel bölgesi yarıçapı (m): r1 = sqrt(λ d1 d2 / (d1 + d2)); d1 = d2 = d/2.
pub fn fresnel_mid_m(d_m: f64, f_mhz: f64) -> f64 {
    (wavelength_m(f_mhz) * d_m.max(0.0)).sqrt() / 2.0
}

/// Düz zeminde, iki ucu aynı yükseklikte iken Fresnel bölgesinin %60'ı açık kalsın diye
/// gereken en az anten yüksekliği (m).
pub fn min_height_60_m(d_m: f64, f_mhz: f64) -> f64 {
    0.6 * fresnel_mid_m(d_m, f_mhz)
}

/// İki ışın (zemin yansıması) modelinin kırılma uzaklığı (m): 4π h1 h2 / λ.
pub fn two_ray_breakpoint_m(h1_m: f64, h2_m: f64, f_mhz: f64) -> f64 {
    4.0 * std::f64::consts::PI * h1_m.max(0.0) * h2_m.max(0.0) / wavelength_m(f_mhz)
}

/// İki ışın modelinde yol kaybı (dB): 40log10(d) - 20log10(h1 h2) (anten kazançları hariç).
pub fn two_ray_loss_db(d_m: f64, h1_m: f64, h2_m: f64) -> f64 {
    40.0 * d_m.max(1e-3).log10() - 20.0 * (h1_m.max(1e-3) * h2_m.max(1e-3)).log10()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pol {
    Linear,
    Rhcp,
    Lhcp,
}

/// Polarizasyon uyumsuzluğu kaybı (dB). Doğrusal-doğrusal: -20log10|cos θ| (en çok 40 dB);
/// doğrusal-dairesel: 3 dB; dairesel-dairesel: aynı yön 0, zıt yön 20 dB (tipik örnek).
pub fn pol_loss_db(a: Pol, b: Pol, angle_deg: f64) -> f64 {
    match (a, b) {
        (Pol::Linear, Pol::Linear) => {
            let c = angle_deg.to_radians().cos().abs();
            if c < 0.01 { 40.0 } else { -20.0 * c.log10() }
        }
        (Pol::Linear, _) | (_, Pol::Linear) => 3.0,
        (x, y) if x == y => 0.0,
        _ => 20.0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkKind {
    Rc,
    Telemetry,
    Video,
    Gnss,
}

impl LinkKind {
    pub const ALL: [LinkKind; 4] = [
        LinkKind::Rc,
        LinkKind::Telemetry,
        LinkKind::Video,
        LinkKind::Gnss,
    ];
    pub fn key(self) -> &'static str {
        match self {
            LinkKind::Rc => "rfemi.kind.rc",
            LinkKind::Telemetry => "rfemi.kind.telemetry",
            LinkKind::Video => "rfemi.kind.video",
            LinkKind::Gnss => "rfemi.kind.gnss",
        }
    }
    /// Uçaktaki verici (harmonik kaynağı) mı?
    pub fn onboard_tx(self) -> bool {
        matches!(self, LinkKind::Telemetry | LinkKind::Video)
    }
    /// Uçaktaki alıcı (harmoniklerden etkilenir) mı?
    pub fn onboard_rx(self) -> bool {
        !matches!(self, LinkKind::Video)
    }
}

/// Frekans bandı ön ayarı. Sayılar "tipik örnek"tir; bölgenizin mevzuatına bakın.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Band {
    Mhz433,
    Mhz868,
    Mhz915,
    Ghz1_2,
    Ghz2_4,
    Ghz5_8,
    GnssL1,
    Custom,
}

pub struct BandInfo {
    pub label: &'static str,
    pub f_mhz: f64,
    /// Alıcı bandının tipik genişliği (MHz); harmonik çakışma denetiminde kullanılır.
    pub bw_mhz: f64,
    pub tx_dbm: f64,
    pub sens_dbm: f64,
}

impl Band {
    pub const ALL: [Band; 8] = [
        Band::Mhz433,
        Band::Mhz868,
        Band::Mhz915,
        Band::Ghz1_2,
        Band::Ghz2_4,
        Band::Ghz5_8,
        Band::GnssL1,
        Band::Custom,
    ];
    pub fn info(self) -> BandInfo {
        let b = |label, f_mhz, bw_mhz, tx_dbm, sens_dbm| BandInfo {
            label,
            f_mhz,
            bw_mhz,
            tx_dbm,
            sens_dbm,
        };
        match self {
            Band::Mhz433 => b("433 MHz", 433.92, 2.0, 20.0, -117.0),
            Band::Mhz868 => b("868 MHz", 868.0, 7.0, 20.0, -112.0),
            Band::Mhz915 => b("915 MHz", 915.0, 26.0, 20.0, -112.0),
            Band::Ghz1_2 => b("1.2 GHz", 1280.0, 100.0, 23.0, -92.0),
            Band::Ghz2_4 => b("2.4 GHz", 2440.0, 80.0, 20.0, -105.0),
            Band::Ghz5_8 => b("5.8 GHz", 5800.0, 150.0, 23.0, -90.0),
            Band::GnssL1 => b("GNSS L1", 1575.42, 40.0, 0.0, -148.0),
            Band::Custom => b("Custom", 1000.0, 10.0, 20.0, -100.0),
        }
    }
}

/// Bir telsiz bağı: uçaktaki anten ile yer istasyonu arasındaki hat.
#[derive(Clone, Debug)]
pub struct Link {
    pub name: String,
    pub kind: LinkKind,
    pub band: Band,
    pub f_mhz: f64,
    pub bw_mhz: f64,
    pub tx_dbm: f64,
    pub tx_gain_dbi: f64,
    pub rx_gain_dbi: f64,
    /// Her uçtaki kablo + konnektör kaybı (dB).
    pub loss_db: f64,
    pub sens_dbm: f64,
    pub fade_margin_db: f64,
    /// Tasarım mesafesi (km).
    pub dist_km: f64,
    /// Yer anteninin ve uçak anteninin yerden yüksekliği (m).
    pub h_ground_m: f64,
    pub h_air_m: f64,
    /// Yalnız alıcı bağlarda (GNSS) anten girişindeki tipik sinyal gücü (dBm).
    pub fixed_rx_dbm: Option<f64>,
    pub pol_air: Pol,
    pub pol_ground: Pol,
    /// İki doğrusal anten arasındaki eğim açısı (derece).
    pub pol_angle_deg: f64,
    pub loc: AntennaLoc,
}

impl Link {
    /// Bağ türüne göre tipik örnek ön ayar.
    pub fn typical(kind: LinkKind) -> Link {
        let band = match kind {
            LinkKind::Rc => Band::Mhz868,
            LinkKind::Telemetry => Band::Mhz433,
            LinkKind::Video => Band::Ghz5_8,
            LinkKind::Gnss => Band::GnssL1,
        };
        let mut l = Link {
            name: String::new(),
            kind,
            band,
            f_mhz: 0.0,
            bw_mhz: 0.0,
            tx_dbm: 0.0,
            tx_gain_dbi: 2.0,
            rx_gain_dbi: 2.0,
            loss_db: 0.5,
            sens_dbm: 0.0,
            fade_margin_db: 10.0,
            dist_km: match kind {
                LinkKind::Rc => 5.0,
                LinkKind::Telemetry => 3.0,
                LinkKind::Video => 1.0,
                LinkKind::Gnss => 0.0,
            },
            h_ground_m: 1.5,
            h_air_m: 100.0,
            fixed_rx_dbm: None,
            pol_air: Pol::Linear,
            pol_ground: Pol::Linear,
            pol_angle_deg: 0.0,
            loc: match kind {
                LinkKind::Rc => AntennaLoc::Tail,
                LinkKind::Telemetry => AntennaLoc::WingtipL,
                LinkKind::Video => AntennaLoc::Belly,
                LinkKind::Gnss => AntennaLoc::FuselageTop,
            },
        };
        l.apply_band(band);
        if kind == LinkKind::Video {
            l.pol_air = Pol::Rhcp;
            l.pol_ground = Pol::Rhcp;
        }
        if kind == LinkKind::Gnss {
            l.pol_air = Pol::Rhcp;
            l.pol_ground = Pol::Rhcp;
            l.tx_gain_dbi = 0.0;
            l.rx_gain_dbi = 3.0;
            l.fixed_rx_dbm = Some(-128.5);
            l.fade_margin_db = 3.0;
            l.h_air_m = 0.0;
        }
        l
    }

    /// Bant ön ayarının frekans/güç/duyarlılık sayılarını uygular.
    pub fn apply_band(&mut self, band: Band) {
        self.band = band;
        let i = band.info();
        self.f_mhz = i.f_mhz;
        self.bw_mhz = i.bw_mhz;
        self.tx_dbm = i.tx_dbm;
        self.sens_dbm = i.sens_dbm;
    }
}

#[derive(Clone, Debug, Default)]
pub struct LinkResult {
    pub eirp_dbm: f64,
    pub fspl_db: f64,
    pub pol_loss_db: f64,
    pub rx_dbm: f64,
    /// Alınan güç - duyarlılık - gereken sönme payı (dB); negatifse bağ kapanmaz.
    pub margin_db: f64,
    /// Gereken payla en büyük menzil (km); yalnız alıcı bağlarda `None`.
    pub max_range_km: Option<f64>,
    pub fresnel_mid_m: f64,
    pub min_height_m: f64,
    /// Düz zeminde Fresnel açıklığı için etkin yükseklik (m): sqrt(h_ground * h_air).
    pub clearance_m: f64,
    pub two_ray_breakpoint_m: f64,
    /// Kırılma uzaklığından sonra iki ışın kaybı serbest uzaydan fazlaysa fark (dB).
    pub two_ray_extra_db: f64,
}

pub fn compute(l: &Link) -> LinkResult {
    let pol = pol_loss_db(l.pol_air, l.pol_ground, l.pol_angle_deg);
    let d_m = l.dist_km * 1000.0;
    let rx_only = l.fixed_rx_dbm.is_some();
    let fspl = if rx_only {
        0.0
    } else {
        fspl_db(l.dist_km, l.f_mhz)
    };
    let eirp = l.tx_dbm + l.tx_gain_dbi - l.loss_db;
    let rx = match l.fixed_rx_dbm {
        Some(p) => p + l.rx_gain_dbi - l.loss_db - pol,
        None => l.tx_dbm + l.tx_gain_dbi + l.rx_gain_dbi - 2.0 * l.loss_db - fspl - pol,
    };
    let margin = rx - l.sens_dbm - l.fade_margin_db;
    let max_range = if rx_only {
        None
    } else {
        let allowed = l.tx_dbm + l.tx_gain_dbi + l.rx_gain_dbi
            - 2.0 * l.loss_db
            - pol
            - l.sens_dbm
            - l.fade_margin_db;
        Some(range_km_for_loss(allowed, l.f_mhz))
    };
    let (fresnel, min_h, clear, bp, extra) = if rx_only || d_m <= 0.0 {
        (0.0, 0.0, 0.0, 0.0, 0.0)
    } else {
        let bp = two_ray_breakpoint_m(l.h_ground_m, l.h_air_m, l.f_mhz);
        let extra = if d_m > bp && bp > 0.0 {
            (two_ray_loss_db(d_m, l.h_ground_m, l.h_air_m) - fspl).max(0.0)
        } else {
            0.0
        };
        (
            fresnel_mid_m(d_m, l.f_mhz),
            min_height_60_m(d_m, l.f_mhz),
            (l.h_ground_m.max(0.0) * l.h_air_m.max(0.0)).sqrt(),
            bp,
            extra,
        )
    };
    LinkResult {
        eirp_dbm: eirp,
        fspl_db: fspl,
        pol_loss_db: pol,
        rx_dbm: rx,
        margin_db: margin,
        max_range_km: max_range,
        fresnel_mid_m: fresnel,
        min_height_m: min_h,
        clearance_m: clear,
        two_ray_breakpoint_m: bp,
        two_ray_extra_db: extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fspl_hand_values() {
        // 2.4 GHz, 1 km: 20log(2400) + 32.44 = 67.60 + 32.44 = 100.04 dB.
        assert!((fspl_db(1.0, 2400.0) - 100.04).abs() < 0.01);
        // 5.8 GHz, 10 km: 20 + 75.27 + 32.44 = 127.71 dB.
        assert!((fspl_db(10.0, 5800.0) - 127.71).abs() < 0.01);
        // Mesafe ikiye katlanınca +6.02 dB.
        assert!((fspl_db(2.0, 868.0) - fspl_db(1.0, 868.0) - 6.0206).abs() < 1e-3);
    }

    #[test]
    fn range_is_inverse_of_fspl() {
        for (d, f) in [(0.5, 433.0), (3.0, 868.0), (12.0, 2440.0), (1.0, 5800.0)] {
            let loss = fspl_db(d, f);
            assert!((range_km_for_loss(loss, f) - d).abs() < 1e-9 * d.max(1.0));
        }
    }

    #[test]
    fn fresnel_radius_formula() {
        // 2.4 GHz, 1 km: λ = 0.1249 m; r1 = 0.5 sqrt(0.1249 * 1000) = 5.59 m.
        let r = fresnel_mid_m(1000.0, 2400.0);
        assert!((r - 5.588).abs() < 0.01, "{r}");
        assert!((min_height_60_m(1000.0, 2400.0) - 0.6 * r).abs() < 1e-12);
        // Daha düşük frekansta daha büyük.
        assert!(fresnel_mid_m(1000.0, 433.0) > r);
    }

    #[test]
    fn polarization_losses() {
        assert!(pol_loss_db(Pol::Linear, Pol::Linear, 0.0).abs() < 1e-9);
        assert!((pol_loss_db(Pol::Linear, Pol::Linear, 45.0) - 3.0103).abs() < 1e-3);
        assert_eq!(pol_loss_db(Pol::Linear, Pol::Linear, 90.0), 40.0);
        assert_eq!(pol_loss_db(Pol::Linear, Pol::Rhcp, 0.0), 3.0);
        assert_eq!(pol_loss_db(Pol::Lhcp, Pol::Linear, 0.0), 3.0);
        assert_eq!(pol_loss_db(Pol::Rhcp, Pol::Rhcp, 0.0), 0.0);
        assert_eq!(pol_loss_db(Pol::Rhcp, Pol::Lhcp, 0.0), 20.0);
    }

    #[test]
    fn margin_is_zero_at_max_range() {
        let mut l = Link::typical(LinkKind::Rc);
        let r = compute(&l);
        let range = r.max_range_km.unwrap();
        assert!(r.margin_db > 0.0 && range > l.dist_km);
        l.dist_km = range;
        assert!(compute(&l).margin_db.abs() < 1e-6);
    }

    #[test]
    fn typical_links_are_finite() {
        for k in LinkKind::ALL {
            let r = compute(&Link::typical(k));
            assert!(r.rx_dbm.is_finite() && r.margin_db.is_finite(), "{k:?}");
            assert!(r.fresnel_mid_m.is_finite() && r.two_ray_extra_db.is_finite());
        }
    }

    #[test]
    fn two_ray_extra_loss_is_positive_past_breakpoint() {
        let mut l = Link::typical(LinkKind::Rc);
        // 868 MHz, h_ground=1.5 m, h_air=100 m -> kırılma ~5458 m.
        let r_before = compute(&l);
        assert!((r_before.two_ray_breakpoint_m - 5457.6).abs() < 2.0);
        assert_eq!(r_before.two_ray_extra_db, 0.0);

        l.dist_km = 10.0;
        let r_after = compute(&l);
        assert!(
            r_after.two_ray_extra_db > 5.0,
            "{}",
            r_after.two_ray_extra_db
        );
    }

    #[test]
    fn fresnel_clearance_uses_geometric_mean() {
        let mut l = Link::typical(LinkKind::Rc);
        l.h_ground_m = 0.0;
        l.h_air_m = 100.0;
        let r = compute(&l);
        assert_eq!(r.clearance_m, 0.0);
        assert!(r.clearance_m < r.min_height_m);

        l.h_ground_m = 0.5;
        let r = compute(&l);
        assert!((r.clearance_m - 50.0f64.sqrt()).abs() < 1e-9);
        assert!(r.clearance_m < r.min_height_m);
    }
}
