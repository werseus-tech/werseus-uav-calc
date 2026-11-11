// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parçaların iç yapısı (FreeCAD'siz, saf Rust): taşıyıcı yüzeylerde kabuk, kök kaburgası,
//! ara kaburgalar ve kiriş kanalı; gövdede kabuk ve halka postalar (former).
//!
//! Bu modül yapı ölçülerinin tek kaynağıdır: FreeCAD betikleri (`cad::freecad`) Spreadsheet
//! değerlerini buradan alır; uygulama içi 3B görünüm de aynı değerleri buradan okuyabilir:
//! `Layout::new(p, m).wing.structure()`, `.tail_h.structure()`, `.fuselage.structure()`.
//! Tüm uzunluklar mm; konumlar ilgili parçanın yerel çerçevesindedir.

use super::geom::{FuseProfile, SparAxis};

/// Profil kesiti başına (üst ve alt yüzey ayrı ayrı) örnek nokta aralığı sayısı. Kesit, bu
/// noktalardan geçen tek bir B-spline'dır; az nokta, FreeCAD çizim çözücüsünün tam
/// kısıtlı kutup (pole) kısıtlarını hızlı ve kararlı çözmesini sağlar.
pub const SECTION_SEGMENTS_PER_SIDE: usize = 16;
/// İç boşluk (kabuk içi) kesitinde yüzey başına nokta aralığı sayısı.
pub const INNER_SEGMENTS_PER_SIDE: usize = 12;
/// Kanatta hedef kaburga aralığı [mm].
pub const WING_RIB_PITCH_MM: f64 = 120.0;
/// Kuyruk yüzeyleri ve winglet'te hedef kaburga aralığı [mm].
pub const TAIL_RIB_PITCH_MM: f64 = 100.0;
/// Gövdede hedef posta (former) aralığı [mm].
pub const FORMER_PITCH_MM: f64 = 150.0;
/// En çok kaburga bölmesi (bay) sayısı.
pub const MAX_RIB_BAYS: u32 = 24;
/// En çok posta sayısı.
pub const MAX_FORMERS: u32 = 10;

/// Kaburga/posta kalınlığı: et kalınlığının 3 katı, en az 2 mm.
pub fn web_thickness_mm(wall_mm: f64) -> f64 {
    (3.0 * wall_mm).max(2.0)
}

/// Taşıyıcı yüzeyin (bir yarısının) iç yapısı. Açıklık yerel Z ekseni boyuncadır: kök z = 0,
/// uç z = `span`. Kabuk (`wall_mm` > 0) kök kaburgası [0, root_rib], uç kapağı
/// [span - wall, span] ve aradaki `rib_bays` bölme arasında `rib_bays - 1` ara kaburgadan oluşur.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceStructure {
    pub span_mm: f64,
    /// Kabuk et kalınlığı; 0 ise dolu katı (kaburga yok).
    pub wall_mm: f64,
    /// Kök kaburgası kalınlığı (kiriş deliği buradan geçer).
    pub root_rib_mm: f64,
    /// Ara kaburga kalınlığı.
    pub rib_t_mm: f64,
    /// Kaburgalar arası bölme sayısı (1 ise ara kaburga yok). Sıfır: içi boş değil.
    pub rib_bays: u32,
    /// Kiriş kanalı (kök kaburgasından ve tüm ara kaburgalardan geçer).
    pub spar: Option<SparAxis>,
}

impl SurfaceStructure {
    /// `wall_mm`, kök kaburgası ve hedef kaburga aralığından yapı. Kabuğa yer yoksa dolu.
    pub fn new(span_mm: f64, wall_mm: f64, spar: Option<SparAxis>, pitch_target_mm: f64) -> Self {
        let root_rib_mm = spar.map(|a| a.root_rib).unwrap_or(wall_mm);
        let rib_t_mm = web_thickness_mm(wall_mm);
        let free = span_mm - wall_mm - root_rib_mm;
        let rib_bays = if wall_mm > 0.0 && free.is_finite() && free > 1.0 {
            let n = (free / pitch_target_mm.max(1.0)).ceil().max(1.0) as u32;
            // Her bölme en az iki kaburga kalınlığı + 5 mm uzunlukta kalmalı.
            let n_fit = ((free + rib_t_mm) / (3.0 * rib_t_mm + 5.0))
                .floor()
                .max(1.0) as u32;
            n.min(n_fit).clamp(1, MAX_RIB_BAYS)
        } else {
            0
        };
        Self {
            span_mm,
            wall_mm,
            root_rib_mm,
            rib_t_mm,
            rib_bays,
            spar,
        }
    }

    pub fn is_hollow(&self) -> bool {
        self.rib_bays > 0
    }

    /// Kaburga merkezleri arası uzaklık.
    pub fn rib_pitch_mm(&self) -> f64 {
        if self.rib_bays == 0 {
            return 0.0;
        }
        (self.span_mm - self.wall_mm - self.root_rib_mm) / self.rib_bays as f64
    }

    /// Ara kaburga merkezlerinin açıklık konumları.
    pub fn rib_stations_mm(&self) -> Vec<f64> {
        let p = self.rib_pitch_mm();
        (1..self.rib_bays)
            .map(|k| self.root_rib_mm + k as f64 * p)
            .collect()
    }

    /// İçi boş bölmelerin [z_a, z_b] açıklık aralıkları (kökten uca).
    pub fn bays_mm(&self) -> Vec<(f64, f64)> {
        let n = self.rib_bays;
        let p = self.rib_pitch_mm();
        let h = self.rib_t_mm / 2.0;
        (0..n)
            .map(|k| {
                let za = self.root_rib_mm + k as f64 * p + if k > 0 { h } else { 0.0 };
                let zb = self.root_rib_mm + (k + 1) as f64 * p - if k + 1 < n { h } else { 0.0 };
                (za, zb)
            })
            .collect()
    }
}

/// Gövde iç yapısı: kabuk ve halka postalar. Gövde X ekseni boyunca (burun x = 0).
#[derive(Clone, Debug, PartialEq)]
pub struct FuselageStructure {
    pub length_mm: f64,
    /// Kabuk et kalınlığı; 0 ise dolu katı (posta yok).
    pub wall_mm: f64,
    pub former_t_mm: f64,
    /// Postanın kabuktan içeri doğru halka genişliği (kalan orta açıklık geçiş içindir).
    pub former_depth_mm: f64,
    /// Posta merkezleri, gövde boyuna oranla (0..1).
    pub former_fracs: Vec<f64>,
}

impl FuselageStructure {
    pub fn new(prof: &FuseProfile, r_max_mm: f64, wall_mm: f64) -> Self {
        let length_mm = prof.length();
        let former_t_mm = web_thickness_mm(wall_mm);
        let former_depth_mm = (0.12 * r_max_mm).max(3.0);
        let mut former_fracs = Vec::new();
        if wall_mm > 0.0 && length_mm > 0.0 {
            let n = ((length_mm / FORMER_PITCH_MM).round() as i64 - 1).clamp(1, MAX_FORMERS as i64);
            for k in 1..=n {
                let f = k as f64 / (n + 1) as f64;
                // Posta, iç yarıçapın yarısından fazlasını kapatmamalı (geçiş açıklığı kalsın).
                let r_in = prof.r_at(f * length_mm) - wall_mm;
                if r_in - former_depth_mm >= 0.4 * r_in && r_in > 4.0 {
                    former_fracs.push(f);
                }
            }
        }
        Self {
            length_mm,
            wall_mm,
            former_t_mm,
            former_depth_mm,
            former_fracs,
        }
    }

    /// Posta merkezlerinin x konumları [mm].
    pub fn former_stations_mm(&self) -> Vec<f64> {
        self.former_fracs
            .iter()
            .map(|f| f * self.length_mm)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bays_cover_span_between_root_rib_and_tip_cap() {
        let s = SurfaceStructure::new(800.0, 0.8, None, WING_RIB_PITCH_MM);
        assert!(s.is_hollow());
        assert_eq!(s.rib_bays, 7);
        let bays = s.bays_mm();
        assert_eq!(bays.len(), 7);
        assert!((bays[0].0 - s.root_rib_mm).abs() < 1e-9);
        assert!((bays[6].1 - (800.0 - 0.8)).abs() < 1e-9);
        // Komşu bölmeler arasında tam bir kaburga kalınlığı kalır.
        for w in bays.windows(2) {
            assert!((w[1].0 - w[0].1 - s.rib_t_mm).abs() < 1e-9);
        }
        assert_eq!(s.rib_stations_mm().len(), 6);
    }

    #[test]
    fn solid_surface_has_no_ribs() {
        let s = SurfaceStructure::new(300.0, 0.0, None, TAIL_RIB_PITCH_MM);
        assert!(!s.is_hollow());
        assert!(s.bays_mm().is_empty() && s.rib_stations_mm().is_empty());
        // Çok kısa yüzey: tek bölme.
        let s = SurfaceStructure::new(20.0, 0.5, None, TAIL_RIB_PITCH_MM);
        assert_eq!(s.rib_bays, 1);
    }

    #[test]
    fn formers_leave_a_passage() {
        let prof = FuseProfile::new(900.0, 55.0, 25.0, 16.0, 0.25, 0.35);
        let f = FuselageStructure::new(&prof, 55.0, 0.8);
        assert!(!f.former_fracs.is_empty());
        for x in f.former_stations_mm() {
            assert!(x > 0.0 && x < 900.0);
            let r_in = prof.r_at(x) - 0.8;
            assert!(r_in - f.former_depth_mm > 0.0);
        }
        assert!(
            FuselageStructure::new(&prof, 55.0, 0.0)
                .former_fracs
                .is_empty()
        );
    }
}
