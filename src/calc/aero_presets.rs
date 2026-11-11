// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yeni başlayanlar için "basit mod" türetme katmanı.
//!
//! Bu modül doğrulanmış aerodinamik formüllere (`calc::aero`) DOKUNMAZ. Yalnızca kullanıcıya
//! anlaşılır sorular sorup, mevcut girdileri (`cl_max_2d`, `cd_misc`, `s_wet_fuse`) üreten
//! saf fonksiyonlar ve günlük dile çeviri yardımcıları içerir.
//!
//! Tüm sayısal değerler YAKLAŞIKTIR (mühendislik tahmini, ±%20-30 belirsizlik); kaynaklar
//! ilgili sabitlerin yanında yorumlanmıştır. Kesin değer için XFOIL/rüzgar tüneli/uçuş testi gerekir.
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------
// Kullanıcı seçimleri
// ---------------------------------------------------------------------------------------------

/// "Uçak ne yapsın?"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Purpose {
    /// Uzun süre havada kalsın.
    Endurance,
    /// Hızlı gitsin.
    Speed,
    /// Kolay ve güvenli uçsun.
    #[default]
    Easy,
}

impl Purpose {
    pub const ALL: [Purpose; 3] = [Purpose::Endurance, Purpose::Speed, Purpose::Easy];
}

/// "Yüzey kalitesi?"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Surface {
    Rough,
    #[default]
    Sanded,
    Painted,
    Foam,
}

impl Surface {
    pub const ALL: [Surface; 4] = [
        Surface::Rough,
        Surface::Sanded,
        Surface::Painted,
        Surface::Foam,
    ];

    /// Deri sürtünmesi (kanat+gövde+kuyruk Cf·FF·Swet/S) için düzeltme çarpanı.
    ///
    /// Kaynak/gerekçe: Raymer (Aircraft Design: A Conceptual Approach) "skin roughness"
    /// tablosu: pürüzlü yüzeyde eşdeğer kum-pürüzlülüğü Cf'yi birkaç ila %25 artırır;
    /// Katmanlı üretim yüzeylerinde ~0.1-0.2 mm çizgiler düşük Re'de kısmen "hidrolik pürüzsüz"
    /// kalsa da priz/dikiş etkisi ile ~%25 daha yüksek alınmıştır. Zımpara ~%10, boya+zımpara
    /// referans (1.00, mevcut doğrulanmış model "pürüzsüz boyalı" kabul eder).
    /// Köpük (EPP/EPS) yüzeyde boncuk dokusu nedeniyle ~%15. Değerler yaklaşıktır.
    pub fn skin_multiplier(self) -> f64 {
        match self {
            Surface::Rough => 1.25,
            Surface::Sanded => 1.10,
            Surface::Painted => 1.00,
            Surface::Foam => 1.15,
        }
    }
}

/// "Gövde şekli?"
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BodyShape {
    Slim,
    #[default]
    Normal,
    Boxy,
}

impl BodyShape {
    pub const ALL: [BodyShape; 3] = [BodyShape::Slim, BodyShape::Normal, BodyShape::Boxy];

    /// Islak alan çarpanı (kapalı gövde geometrik tahmini `fuse_wetted_geometric` × çarpan).
    /// Hoerner (Fluid-Dynamic Drag, böl. 6): kutu gövdede köşe/kapak/kanopi nedeniyle yüzey
    /// artışı; ince gövdede geometrik tahmin zaten iyi. Yaklaşık değerler.
    pub fn wet_multiplier(self) -> f64 {
        match self {
            BodyShape::Slim => 1.00,
            BodyShape::Normal => 1.00,
            BodyShape::Boxy => 1.25,
        }
    }

    /// Gövde sürükleme (CD0_gövde) için ek form/basınç sürüklemesi çarpanı.
    /// Hoerner: ince gövde ≈ doğrulanmış FF modeli (1.00); kamera/kapak çıkıntılı normal
    /// gövde için +%10; kutu gibi (künt burun/kuyruk, ayrılma) gövdede CD ≈ 1.5-2 kat → 1.6.
    pub fn form_multiplier(self) -> f64 {
        match self {
            BodyShape::Slim => 1.00,
            BodyShape::Normal => 1.10,
            BodyShape::Boxy => 1.60,
        }
    }
}

/// Dışarıdaki ekipman (çoklu seçim). Her biri için sürükleme alanı D/q [m²] (≈ CD·A_ön).
/// Raymer/Hoerner'in "ıslak alan yerine D/q" yaklaşımı; küçük İHA için yaklaşık büyüklükler:
/// küre/silindir CD ≈ 0.4-1.0 × ön alan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Externals {
    pub gimbal: bool,
    pub gear: bool,
    pub antenna: bool,
}

impl Externals {
    /// Kamera gimbali: ~60 mm çaplı küre/silindir gövde, CD≈0.5-0.8 → D/q ≈ 0.0015 m².
    pub const DQ_GIMBAL: f64 = 0.0015;
    /// Sabit iniş takımı (tel bacak + 2-3 küçük tekerlek): D/q ≈ 0.004 m².
    pub const DQ_GEAR: f64 = 0.004;
    /// Kırbaç anten / küçük direk: D/q ≈ 0.0002 m².
    pub const DQ_ANTENNA: f64 = 0.0002;

    pub fn none() -> Self {
        Self::default()
    }
    pub fn dq_total(self) -> f64 {
        let mut d = 0.0;
        if self.gimbal {
            d += Self::DQ_GIMBAL;
        }
        if self.gear {
            d += Self::DQ_GEAR;
        }
        if self.antenna {
            d += Self::DQ_ANTENNA;
        }
        d
    }
}

/// Kanat büyüklüğü tercihi (kanat yüklemesi hedefi, N/m²).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SizePref {
    /// Büyük kanat, yavaş uçuş.
    Floaty,
    #[default]
    Normal,
    /// Küçük kanat, hızlı uçuş.
    Fast,
}

impl SizePref {
    pub const ALL: [SizePref; 3] = [SizePref::Floaty, SizePref::Normal, SizePref::Fast];

    /// Hedef kanat yüklemesi W/S [N/m²]. Hobi İHA tipik aralığı ~25-80 N/m²
    /// (spec referans tasarımı 61.3 N/m²); 35/50/65 bu aralığın alt/orta/üst bölümüdür.
    pub fn wing_loading(self) -> f64 {
        match self {
            SizePref::Floaty => 35.0,
            SizePref::Normal => 50.0,
            SizePref::Fast => 65.0,
        }
    }
}

/// Basit mod durumu (projede saklanır).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AeroSimple {
    /// true = Basit (varsayılan), false = Gelişmiş.
    pub simple_mode: bool,
    pub purpose: Purpose,
    pub surface: Surface,
    pub body: BodyShape,
    pub externals: Externals,
    pub size_pref: SizePref,
    /// Kullanıcı en az bir kez cevap verdi / "türet" dedi → türetilen değerler uygulanır.
    /// false iken proje girdilerine dokunulmaz (eski dosyalar ve örnekler aynen korunur).
    pub derived_active: bool,
    /// Gelişmiş modda elle değiştirilmiş (özelleştirilmiş) değerler; basit mod bunları ezmez.
    pub ov_cl_max: bool,
    pub ov_cd_misc: bool,
    pub ov_s_wet: bool,
}

impl Default for AeroSimple {
    fn default() -> Self {
        Self {
            simple_mode: true,
            purpose: Purpose::default(),
            surface: Surface::default(),
            body: BodyShape::default(),
            externals: Externals::default(),
            size_pref: SizePref::default(),
            derived_active: false,
            ov_cl_max: false,
            ov_cd_misc: false,
            ov_s_wet: false,
        }
    }
}

/// Basit modun yazdığı üç ham aero girdisi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AeroValues {
    pub cl_max_2d: f64,
    pub cd_misc: f64,
    pub s_wet_fuse: f64,
}

impl AeroSimple {
    /// Gelişmiş modda yapılan düzenlemeyi tespit eder: değişen alan "özelleştirildi" olur.
    pub fn mark_overrides(&mut self, before: &AeroValues, after: &AeroValues) {
        let ch = |a: f64, b: f64| (a - b).abs() > 1e-12;
        if ch(before.cl_max_2d, after.cl_max_2d) {
            self.ov_cl_max = true;
        }
        if ch(before.cd_misc, after.cd_misc) {
            self.ov_cd_misc = true;
        }
        if ch(before.s_wet_fuse, after.s_wet_fuse) {
            self.ov_s_wet = true;
        }
    }

    pub fn any_override(&self) -> bool {
        self.ov_cl_max || self.ov_cd_misc || self.ov_s_wet
    }

    pub fn reset_overrides(&mut self) {
        self.ov_cl_max = false;
        self.ov_cd_misc = false;
        self.ov_s_wet = false;
    }

    /// Türetmeyi ilk kez etkinleştirir. Proje varsayılanlarından farklı (elle girilmiş) mevcut
    /// değerler "özelleştirilmiş" sayılır ve ilk cevapta sessizce ezilmez.
    pub fn activate(&mut self, cur: &AeroValues, defaults: &AeroValues) {
        if self.derived_active {
            return;
        }
        self.mark_overrides(defaults, cur);
        self.derived_active = true;
    }

    /// Türetilmiş değerleri uygular; özelleştirilmiş alanlar sessizce ezilmez.
    pub fn apply(&self, d: &DerivedAero, cur: &mut AeroValues) {
        if !self.derived_active {
            return;
        }
        if !self.ov_cl_max && d.cl_max_2d.is_finite() {
            cur.cl_max_2d = d.cl_max_2d;
        }
        if !self.ov_cd_misc && d.cd_misc.is_finite() {
            cur.cd_misc = d.cd_misc;
        }
        if !self.ov_s_wet && d.s_wet_fuse.is_finite() {
            cur.s_wet_fuse = d.s_wet_fuse;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Öneriler (uygulanmaz, gösterilir)
// ---------------------------------------------------------------------------------------------

/// Amaca göre öneri. Profil kodu NACA 4 haneli; AR ve seyir hızı tipik hobi aralıklarındandır
/// (Raymer tipik AR: planör 10-20, hobi motorlu 7-12, hızlı/oyun 5-7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recommendation {
    pub naca: &'static str,
    pub ar: f64,
    pub v_cruise: f64,
}

pub fn recommend(purpose: Purpose) -> Recommendation {
    match purpose {
        // Yüksek kamburluk: düşük hızda yüksek CL → düşük güç; yüksek AR → düşük indüklenmiş sürükleme.
        Purpose::Endurance => Recommendation {
            naca: "4412",
            ar: 10.0,
            v_cruise: 14.0,
        },
        // İnce, düşük kamburluklu profil ve düşük AR: düşük sürükleme, yüksek kanat yüklemesi.
        Purpose::Speed => Recommendation {
            naca: "2410",
            ar: 6.0,
            v_cruise: 25.0,
        },
        // Orta kalınlık, hafif kamburluk: yumuşak stall, kolay yapım.
        Purpose::Easy => Recommendation {
            naca: "2412",
            ar: 7.0,
            v_cruise: 16.0,
        },
    }
}

// ---------------------------------------------------------------------------------------------
// CL_max (2B) tahmini
// ---------------------------------------------------------------------------------------------

/// Düşük Reynolds sayısı için BASİT, tek yönlü (monoton) CL_max_2d tahmini.
///
/// Dayanak (yaklaşık, sayısal tablo değildir): Selig/UIUC düşük-Re profil verileri ve
/// Abbott&von Doenhoff: ince NACA 4-hane profillerinde CL_max kalınlıkla ~%12-15 t/c'ye kadar
/// artar, kamburlukla (≈ +0.1 / %1 kamber) artar; Re ~1e5 altında laminer ayrılma balonu
/// nedeniyle belirgin düşer, Re ≳ 5e5'te doygunluğa gider (Raymer, bölüm 12).
///
///   base   = 0.75 + 3.5·min(t/c, 0.15)
///   camber = 7.0·min(m/c, 0.06)
///   f(Re)  = clamp(0.80 + 0.20·(log10 Re − 5.0)/0.7, 0.70, 1.00)
///   CL_max_2d = (base + camber)·f(Re)
///
/// Sonuç 0.6..1.8 aralığına sıkıştırılır (arayüz alanının aralığı). Örn. NACA 2412 @ Re=2.4e5
/// ≈ 1.19 (spec varsayılanı 1.25 ile uyumlu). Bu bir TAHMİNDİR; ±%15 belirsizlik taşır.
pub fn cl_max_2d_estimate(tc: f64, camber: f64, re: f64) -> f64 {
    let tc = if tc.is_finite() {
        tc.clamp(0.0, 0.15)
    } else {
        0.12
    };
    let camber = if camber.is_finite() {
        camber.clamp(0.0, 0.06)
    } else {
        0.0
    };
    let f = if re.is_finite() && re > 1.0 {
        (0.80 + 0.20 * (re.log10() - 5.0) / 0.7).clamp(0.70, 1.00)
    } else {
        0.70
    };
    ((0.75 + 3.5 * tc + 7.0 * camber) * f).clamp(0.6, 1.8)
}

// ---------------------------------------------------------------------------------------------
// Türetme
// ---------------------------------------------------------------------------------------------

/// Türetme için model bağlamı (mevcut `Model` çıktılarından doldurulur).
#[derive(Clone, Copy, Debug)]
pub struct DeriveCtx {
    pub s_ref: f64,
    pub tc_w: f64,
    pub camber: f64,
    pub re_wing: f64,
    pub wing_cd0: f64,
    pub tail_cd0: f64,
    /// Gövde CD0'ının ıslak alana oranı [1/m²] (cd0_gövde / s_wet_gövde).
    pub fuse_cd0_per_wet: f64,
    pub fuse_wet_geo: f64,
}

/// Çeşitli sürükleme katkıları ve türetilmiş girdiler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DerivedAero {
    pub cl_max_2d: f64,
    pub cd_misc: f64,
    pub s_wet_fuse: f64,
    /// cd_misc bileşenleri
    pub base_misc: f64,
    pub skin_extra: f64,
    pub form_extra: f64,
    pub ext_extra: f64,
}

/// Arayüz/birleşme/menteşe/kaçak taban payı. Raymer "miscellaneous drag" tipik olarak toplam
/// CD0'ın %2-5'i; 0.02-0.03 CD0 için ≈ 0.002 alınmıştır.
pub const BASE_MISC_CD0: f64 = 0.002;

pub fn derive(c: &AeroSimple, x: &DeriveCtx) -> DerivedAero {
    let s_wet = x.fuse_wet_geo * c.body.wet_multiplier();
    let fuse_cd0 = x.fuse_cd0_per_wet * s_wet;
    let skin_sum = x.wing_cd0 + x.tail_cd0 + fuse_cd0;
    let skin_extra = (c.surface.skin_multiplier() - 1.0) * skin_sum;
    let form_extra = (c.body.form_multiplier() - 1.0) * fuse_cd0;
    let ext_extra = if x.s_ref > 0.0 {
        c.externals.dq_total() / x.s_ref
    } else {
        0.0
    };
    DerivedAero {
        cl_max_2d: cl_max_2d_estimate(x.tc_w, x.camber, x.re_wing),
        cd_misc: BASE_MISC_CD0 + skin_extra + form_extra + ext_extra,
        s_wet_fuse: s_wet,
        base_misc: BASE_MISC_CD0,
        skin_extra,
        form_extra,
        ext_extra,
    }
}

// ---------------------------------------------------------------------------------------------
// Günlük dile çeviri
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedAnalogy {
    Walk,
    Run,
    Bike,
    FastBike,
    CityCar,
}

/// Yürüyüş ~5, koşu ~12, bisiklet ~25, hızlı bisiklet ~40, şehir içi araba ~50 km/h.
/// En yakın referansı (log-yakınlık yerine basit eşikler) seçer.
pub fn speed_analogy(v_ms: f64) -> SpeedAnalogy {
    let kmh = v_ms * 3.6;
    if kmh < 8.5 {
        SpeedAnalogy::Walk
    } else if kmh < 18.5 {
        SpeedAnalogy::Run
    } else if kmh < 32.5 {
        SpeedAnalogy::Bike
    } else if kmh < 45.0 {
        SpeedAnalogy::FastBike
    } else {
        SpeedAnalogy::CityCar
    }
}

/// Motorsuz süzülme menzili [m] = L/D × yükseklik.
pub fn glide_distance_m(ld: f64, height_m: f64) -> f64 {
    if ld.is_finite() && ld > 0.0 && height_m.is_finite() && height_m > 0.0 {
        ld * height_m
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rating {
    Good,
    Ok,
    Bad,
}

/// Büyük değer iyi (L/D, havada kalma): `good` ve üzeri yeşil, `ok` ve üzeri sarı.
pub fn rate_higher(x: f64, good: f64, ok: f64) -> Rating {
    if !x.is_finite() {
        Rating::Bad
    } else if x >= good {
        Rating::Good
    } else if x >= ok {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

/// Küçük değer iyi (tutunma kaybı hızı): hedefin altı yeşil, %15 üstüne kadar sarı.
pub fn rate_lower(x: f64, target: f64) -> Rating {
    if !x.is_finite() {
        Rating::Bad
    } else if x <= target {
        Rating::Good
    } else if x <= 1.15 * target {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

/// Önerilen kanat alanı S = W / (W/S) [m²]; W = m·g.
pub fn suggest_wing_area(mtow_kg: f64, pref: SizePref) -> f64 {
    if mtow_kg.is_finite() && mtow_kg > 0.0 {
        mtow_kg * 9.80665 / pref.wing_loading()
    } else {
        f64::NAN
    }
}

/// Elle fırlatma için önerilen en düşük fırlatma hızı: ≈ 1.1 × V_stall
/// (spec referans tasarımı: V_launch 11 m/s, V_stall_req 10 m/s).
pub fn suggest_launch_speed(v_stall: f64) -> f64 {
    1.1 * v_stall
}

/// "Tipik aralık" karşılaştırma satırı (ticari ürün/marka adı yok; yaklaşık aralıklar).
#[derive(Clone, Copy, Debug)]
pub struct TypicalRow {
    pub key: &'static str,
    /// (alt, üst)
    pub ws: (f64, f64),
    pub ar: (f64, f64),
    pub ld: (f64, f64),
    pub v_stall_kmh: (f64, f64),
    pub endurance_min: (f64, f64),
}

/// Yaklaşık tipik hobi aralıkları (Raymer tipik AR/L/D değerleri ve hobi uçuş deneyimi).
pub const TYPICAL: [TypicalRow; 3] = [
    TypicalRow {
        key: "motor_glider",
        ws: (25.0, 45.0),
        ar: (10.0, 16.0),
        ld: (15.0, 25.0),
        v_stall_kmh: (22.0, 32.0),
        endurance_min: (40.0, 120.0),
    },
    TypicalRow {
        key: "fpv_wing",
        ws: (40.0, 70.0),
        ar: (4.0, 6.0),
        ld: (8.0, 14.0),
        v_stall_kmh: (30.0, 45.0),
        endurance_min: (20.0, 60.0),
    },
    TypicalRow {
        key: "trainer",
        ws: (40.0, 60.0),
        ar: (6.0, 8.0),
        ld: (8.0, 12.0),
        v_stall_kmh: (28.0, 40.0),
        endurance_min: (10.0, 25.0),
    },
];

// ---------------------------------------------------------------------------------------------
// Anlık geri bildirim (öncekine göre fark)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Snapshot {
    pub v_stall_kmh: f64,
    pub ld_max: f64,
    pub endurance_min: f64,
}

/// Bir değer değişince, değişim başlamadan önceki model değerini saklar ve farkı hesaplar.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DeltaTracker {
    last: Option<Snapshot>,
    baseline: Snapshot,
    t_changed: f64,
}

/// Farkın ekranda kalma süresi [s].
pub const DELTA_HOLD_S: f64 = 6.0;

impl DeltaTracker {
    fn differs(a: &Snapshot, b: &Snapshot) -> bool {
        (a.v_stall_kmh - b.v_stall_kmh).abs() > 1e-9
            || (a.ld_max - b.ld_max).abs() > 1e-9
            || (a.endurance_min - b.endurance_min).abs() > 1e-9
    }

    pub fn update(&mut self, cur: Snapshot, now: f64) {
        match self.last {
            None => self.last = Some(cur),
            Some(last) => {
                if Self::differs(&last, &cur) {
                    // Yeni bir değişim bölümü başlıyorsa taban değeri güncelle
                    if now - self.t_changed > DELTA_HOLD_S || self.t_changed == 0.0 {
                        self.baseline = last;
                    }
                    self.t_changed = now;
                    self.last = Some(cur);
                }
            }
        }
    }

    /// Gösterilecek fark (şu anki − değişimden önceki); süre dolduysa `None`.
    pub fn delta(&self, now: f64) -> Option<Snapshot> {
        let last = self.last?;
        if self.t_changed == 0.0 || now - self.t_changed > DELTA_HOLD_S {
            return None;
        }
        Some(Snapshot {
            v_stall_kmh: last.v_stall_kmh - self.baseline.v_stall_kmh,
            ld_max: last.ld_max - self.baseline.ld_max,
            endurance_min: last.endurance_min - self.baseline.endurance_min,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::Project;

    fn ctx_from(p: &Project, m: &crate::calc::Model) -> DeriveCtx {
        let fuse = &m.aero.items[1];
        DeriveCtx {
            s_ref: m.aero_geom.s,
            tc_w: p.tc_w,
            camber: p.airfoil.max_camber(),
            re_wing: m.aero.re_wing,
            wing_cd0: m.aero.items[0].cd0,
            tail_cd0: m.aero.items[2].cd0,
            fuse_cd0_per_wet: fuse.cd0 / p.s_wet_fuse,
            fuse_wet_geo: m.fuse_wet_geo,
        }
    }

    #[test]
    fn all_combinations_valid_and_in_range() {
        let lib = Library::embedded();
        let p = Project::default();
        let m = evaluate(&p, &lib);
        let x = ctx_from(&p, &m);
        for s in Surface::ALL {
            for b in BodyShape::ALL {
                for mask in 0..8u8 {
                    let c = AeroSimple {
                        surface: s,
                        body: b,
                        externals: Externals {
                            gimbal: mask & 1 != 0,
                            gear: mask & 2 != 0,
                            antenna: mask & 4 != 0,
                        },
                        derived_active: true,
                        ..Default::default()
                    };
                    let d = derive(&c, &x);
                    assert!(d.cd_misc.is_finite() && d.cd_misc > 0.0 && d.cd_misc < 0.08);
                    assert!(d.s_wet_fuse > 0.0 && d.s_wet_fuse < 1.0);
                    assert!((0.6..=1.8).contains(&d.cl_max_2d));
                    // Uygulanan değerlerle model hâlâ geçerli
                    let mut q = p.clone();
                    let mut v = AeroValues {
                        cl_max_2d: q.cl_max_2d,
                        cd_misc: q.cd_misc,
                        s_wet_fuse: q.s_wet_fuse,
                    };
                    c.apply(&d, &mut v);
                    q.cl_max_2d = v.cl_max_2d;
                    q.cd_misc = v.cd_misc;
                    q.s_wet_fuse = v.s_wet_fuse;
                    let m2 = evaluate(&q, &lib);
                    assert!(m2.aero.cd0.is_finite() && m2.aero.cd0 > 0.0);
                    assert!(m2.aero.v_stall.is_finite());
                }
            }
        }
        for pu in Purpose::ALL {
            let r = recommend(pu);
            assert!(r.ar > 3.0 && r.ar < 20.0 && r.v_cruise > 8.0 && r.v_cruise < 40.0);
            assert!(crate::cad::airfoil::generate_naca4_points(r.naca, 60, true).is_ok());
        }
    }

    #[test]
    fn normal_sanded_close_to_verified_default() {
        let lib = Library::embedded();
        let p = Project::default();
        let m0 = evaluate(&p, &lib);
        let c = AeroSimple {
            surface: Surface::Sanded,
            body: BodyShape::Normal,
            externals: Externals::none(),
            derived_active: true,
            ..Default::default()
        };
        let d = derive(&c, &ctx_from(&p, &m0));
        let mut q = p.clone();
        q.cd_misc = d.cd_misc;
        q.s_wet_fuse = d.s_wet_fuse;
        let m1 = evaluate(&q, &lib);
        let rel = (m1.aero.cd0 - m0.aero.cd0) / m0.aero.cd0;
        assert!(rel.abs() < 0.10, "CD0 sapması %{:.1}", rel * 100.0);
    }

    #[test]
    fn derive_is_monotonic_in_choices() {
        let lib = Library::embedded();
        let p = Project::default();
        let m = evaluate(&p, &lib);
        let x = ctx_from(&p, &m);
        let mk = |s, b, e| {
            derive(
                &AeroSimple {
                    surface: s,
                    body: b,
                    externals: e,
                    ..Default::default()
                },
                &x,
            )
            .cd_misc
        };
        let n = Externals::none();
        assert!(
            mk(Surface::Rough, BodyShape::Normal, n) > mk(Surface::Sanded, BodyShape::Normal, n)
        );
        assert!(
            mk(Surface::Sanded, BodyShape::Normal, n) > mk(Surface::Painted, BodyShape::Normal, n)
        );
        assert!(
            mk(Surface::Sanded, BodyShape::Boxy, n) > mk(Surface::Sanded, BodyShape::Normal, n)
        );
        assert!(
            mk(Surface::Sanded, BodyShape::Normal, n) > mk(Surface::Sanded, BodyShape::Slim, n)
        );
        let g = Externals { gimbal: true, ..n };
        assert!(
            mk(Surface::Sanded, BodyShape::Normal, g) > mk(Surface::Sanded, BodyShape::Normal, n)
        );
    }

    #[test]
    fn activate_marks_non_default_values_as_overrides() {
        let def = AeroValues {
            cl_max_2d: 1.25,
            cd_misc: 0.004,
            s_wet_fuse: 0.235,
        };
        let cur = AeroValues {
            cl_max_2d: 1.4,
            ..def
        };
        let mut st = AeroSimple::default();
        st.activate(&cur, &def);
        assert!(st.derived_active && st.ov_cl_max && !st.ov_cd_misc && !st.ov_s_wet);
        let d = DerivedAero {
            cl_max_2d: 1.0,
            cd_misc: 0.01,
            s_wet_fuse: 0.3,
            base_misc: 0.0,
            skin_extra: 0.0,
            form_extra: 0.0,
            ext_extra: 0.0,
        };
        let mut v = cur;
        st.apply(&d, &mut v);
        assert_eq!(v.cl_max_2d, 1.4, "elle girilen CL_max ezilmemeli");
        assert_eq!(v.cd_misc, 0.01);
    }

    #[test]
    fn override_survives_simple_advanced_simple() {
        let mut st = AeroSimple {
            derived_active: true,
            ..Default::default()
        };
        let d = DerivedAero {
            cl_max_2d: 1.2,
            cd_misc: 0.005,
            s_wet_fuse: 0.25,
            base_misc: 0.0,
            skin_extra: 0.0,
            form_extra: 0.0,
            ext_extra: 0.0,
        };
        let mut v = AeroValues {
            cl_max_2d: 1.0,
            cd_misc: 0.004,
            s_wet_fuse: 0.2,
        };
        st.apply(&d, &mut v); // basit
        assert_eq!(v.cl_max_2d, 1.2);
        // gelişmiş: kullanıcı CL_max'ı elle değiştirir
        let before = v;
        v.cl_max_2d = 1.4;
        st.mark_overrides(&before, &v);
        assert!(st.ov_cl_max && !st.ov_cd_misc && !st.ov_s_wet);
        // basite dönüş: özelleştirilen korunur, diğerleri türetilir
        v.cd_misc = 0.0;
        st.apply(&d, &mut v);
        assert_eq!(v.cl_max_2d, 1.4);
        assert_eq!(v.cd_misc, 0.005);
        // sıfırlayınca türetilen geri gelir
        st.reset_overrides();
        st.apply(&d, &mut v);
        assert_eq!(v.cl_max_2d, 1.2);
    }

    #[test]
    fn inactive_does_not_touch_values() {
        let st = AeroSimple::default();
        let d = DerivedAero {
            cl_max_2d: 1.2,
            cd_misc: 0.005,
            s_wet_fuse: 0.25,
            base_misc: 0.0,
            skin_extra: 0.0,
            form_extra: 0.0,
            ext_extra: 0.0,
        };
        let mut v = AeroValues {
            cl_max_2d: 1.0,
            cd_misc: 0.004,
            s_wet_fuse: 0.2,
        };
        st.apply(&d, &mut v);
        assert_eq!(v.cl_max_2d, 1.0);
        assert!(st.simple_mode);
    }

    #[test]
    fn cl_max_monotonic_in_re_tc_camber() {
        let mut prev = 0.0;
        for k in 0..40 {
            let re = 3e4 * 10f64.powf(k as f64 * 0.05);
            let v = cl_max_2d_estimate(0.12, 0.02, re);
            assert!(v >= prev - 1e-12, "Re ile azalmamalı");
            prev = v;
        }
        let mut prev = 0.0;
        for k in 0..=15 {
            let v = cl_max_2d_estimate(k as f64 / 100.0, 0.02, 2e5);
            assert!(v >= prev - 1e-12, "t/c ile azalmamalı");
            prev = v;
        }
        let mut prev = 0.0;
        for k in 0..=6 {
            let v = cl_max_2d_estimate(0.12, k as f64 / 100.0, 2e5);
            assert!(v >= prev - 1e-12, "kamburlukla azalmamalı");
            prev = v;
        }
        // Düşük Re'de belirgin azalma ve spec varsayılanı ile uyum
        assert!(cl_max_2d_estimate(0.12, 0.02, 5e4) < cl_max_2d_estimate(0.12, 0.02, 5e5) - 0.2);
        let v = cl_max_2d_estimate(0.12, 0.02, 2.4e5);
        assert!((v - 1.25).abs() < 0.15, "{v}");
        assert!(cl_max_2d_estimate(f64::NAN, f64::NAN, f64::NAN).is_finite());
    }

    #[test]
    fn everyday_language_helpers() {
        assert_eq!(speed_analogy(11.0 / 3.6), SpeedAnalogy::Run);
        assert_eq!(speed_analogy(40.0 / 3.6), SpeedAnalogy::FastBike);
        assert_eq!(speed_analogy(60.0 / 3.6), SpeedAnalogy::CityCar);
        assert_eq!(speed_analogy(1.0), SpeedAnalogy::Walk);
        assert!((glide_distance_m(14.0, 100.0) - 1400.0).abs() < 1e-9);
        assert_eq!(glide_distance_m(f64::NAN, 100.0), 0.0);
        assert_eq!(rate_higher(14.0, 12.0, 8.0), Rating::Good);
        assert_eq!(rate_higher(9.0, 12.0, 8.0), Rating::Ok);
        assert_eq!(rate_higher(5.0, 12.0, 8.0), Rating::Bad);
        assert_eq!(rate_lower(9.0, 10.0), Rating::Good);
        assert_eq!(rate_lower(11.0, 10.0), Rating::Ok);
        assert_eq!(rate_lower(13.0, 10.0), Rating::Bad);
        let s = suggest_wing_area(2.0, SizePref::Normal);
        assert!((s - 0.3923).abs() < 1e-3);
        assert!(suggest_wing_area(0.0, SizePref::Normal).is_nan());
        for r in TYPICAL {
            assert!(r.ws.0 < r.ws.1 && r.ar.0 < r.ar.1 && r.ld.0 < r.ld.1);
        }
    }

    #[test]
    fn delta_tracker_reports_difference_then_expires() {
        let mut t = DeltaTracker::default();
        let a = Snapshot {
            v_stall_kmh: 40.0,
            ld_max: 14.0,
            endurance_min: 30.0,
        };
        let b = Snapshot {
            v_stall_kmh: 38.8,
            ld_max: 14.5,
            endurance_min: 36.0,
        };
        t.update(a, 1.0);
        assert!(t.delta(1.0).is_none());
        t.update(b, 2.0);
        let d = t.delta(2.5).unwrap();
        assert!((d.v_stall_kmh + 1.2).abs() < 1e-9);
        assert!((d.endurance_min - 6.0).abs() < 1e-9);
        assert!(t.delta(2.0 + DELTA_HOLD_S + 1.0).is_none());
    }

    #[test]
    fn examples_complete_wizard_in_simple_mode() {
        use crate::project::load_example;
        use crate::ui::wizard::{StepId, validate_step};
        let lib = Library::embedded();
        for f in ["fixed_wing_2kg_1600mm.json", "flying_wing_1200mm.json"] {
            let mut p = load_example(f).expect("örnek yüklenmeli");
            assert!(p.aero_simple.simple_mode, "varsayılan Basit olmalı");
            let m = evaluate(&p, &lib);
            p.aero_simple.derived_active = true;
            let d = derive(&p.aero_simple, &ctx_from(&p, &m));
            let mut v = AeroValues {
                cl_max_2d: p.cl_max_2d,
                cd_misc: p.cd_misc,
                s_wet_fuse: p.s_wet_fuse,
            };
            p.aero_simple.apply(&d, &mut v);
            p.cl_max_2d = v.cl_max_2d;
            p.cd_misc = v.cd_misc;
            p.s_wet_fuse = v.s_wet_fuse;
            let m = evaluate(&p, &lib);
            assert!(
                validate_step(StepId::Aero, &p, &m).is_ok(),
                "{f}: aero adımı"
            );
            assert!(
                validate_step(StepId::DesignPoint, &p, &m).is_ok(),
                "{f}: tasarım noktası"
            );
        }
    }
}
