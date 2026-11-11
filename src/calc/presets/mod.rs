// SPDX-License-Identifier: AGPL-3.0-or-later
//! "Basit mod" türetme katmanı: her sihirbaz adımı için günlük dilde sorular → ham girdiler.
//!
//! Bu katman UI'dan bağımsızdır: yalnızca saf fonksiyonlar,
//! düz yapılar ve i18n ANAHTARLARI içerir. Doğrulanmış formüllerin matematiğine (`calc::*`)
//! DOKUNMAZ; yalnızca onların girdilerini üretir ya da `evaluate` ile aday tasarımları dener.
//!
//! Tüm sayısal varsayılanlar YAKLAŞIKTIR (hobi/öğrenci ölçeği mühendislik tahmini); her sabitin
//! gerekçesi yanında yorumlanmıştır. Marka adı verilmez.
//!
//! Özelleştirme koruması: gelişmiş modda (ya da basit moddaki ham değer bölümünde) elle değiştirilen
//! bir alan `SimpleState::custom` listesine girer; basit mod bu alanı sessizce ezmez. Türetme bir
//! adım için ilk kez etkinleşirken, proje varsayılanından farklı olan mevcut değerler de
//! "özelleştirilmiş" sayılır; böylece eski dosyalar ve örnek projeler aynen korunur.
use crate::calc::weight::Material;
use crate::project::{DroneType, Project};
use serde::{Deserialize, Serialize};

pub mod design_point;
pub mod endurance;
pub mod frame;
pub mod mission;
pub mod propulsion;
pub mod stability;
pub mod structure;
pub mod weight;

#[cfg(test)]
mod tests;

pub use crate::calc::aero_presets::Rating;

/// Seçenekli bir soru için enum üretir: `ALL`, etiket anahtarı (`key`) ve açıklama anahtarı (`desc_key`).
macro_rules! choice {
    ($(#[$m:meta])* $name:ident, $prefix:literal, $def:ident, [$($v:ident => $k:literal),+ $(,)?]) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $name { $($v),+ }
        impl Default for $name {
            fn default() -> Self { $name::$def }
        }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$v),+];
            /// Kısa etiketin i18n anahtarı.
            pub fn key(self) -> &'static str {
                match self { $($name::$v => concat!($prefix, ".", $k)),+ }
            }
            /// Açıklamanın i18n anahtarı.
            pub fn desc_key(self) -> &'static str {
                match self { $($name::$v => concat!($prefix, ".", $k, ".desc")),+ }
            }
        }
    };
}
pub(crate) use choice;

/// Basit modun türetebildiği ham proje alanları (tek merkezden okuma/yazma).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Field {
    // Görev
    Altitude,
    CruiseSpeed,
    Payload,
    MrCruise,
    // Ağırlık
    Avionics,
    Propulsion,
    FuseTail,
    RhoMat,
    // Kanat & çerçeve
    MrFrame,
    MrArms,
    MrDProp,
    MrClearance,
    MrRPlate,
    MrMFrame,
    MrMMotor,
    MrMEsc,
    MrMFc,
    MrMMisc,
    // Tasarım noktası
    VStall,
    ClimbRate,
    VBestClimb,
    VTurn,
    PwMargin,
    VLaunch,
    // İtki
    TwStatic,
    MotorMargin,
    DProp,
    PropPitch,
    PMaxElec,
    MrTMax,
}

impl Field {
    pub const ALL: [Field; 30] = [
        Field::Altitude,
        Field::CruiseSpeed,
        Field::Payload,
        Field::MrCruise,
        Field::Avionics,
        Field::Propulsion,
        Field::FuseTail,
        Field::RhoMat,
        Field::MrFrame,
        Field::MrArms,
        Field::MrDProp,
        Field::MrClearance,
        Field::MrRPlate,
        Field::MrMFrame,
        Field::MrMMotor,
        Field::MrMEsc,
        Field::MrMFc,
        Field::MrMMisc,
        Field::VStall,
        Field::ClimbRate,
        Field::VBestClimb,
        Field::VTurn,
        Field::PwMargin,
        Field::VLaunch,
        Field::TwStatic,
        Field::MotorMargin,
        Field::DProp,
        Field::PropPitch,
        Field::PMaxElec,
        Field::MrTMax,
    ];

    /// Alanın ait olduğu basit mod adımı.
    pub fn step(self) -> SimpleStep {
        use Field::*;
        match self {
            Altitude | CruiseSpeed | Payload | MrCruise => SimpleStep::Mission,
            Avionics | Propulsion | FuseTail | RhoMat => SimpleStep::Weight,
            MrFrame | MrArms | MrDProp | MrClearance | MrRPlate | MrMFrame | MrMMotor | MrMEsc
            | MrMFc | MrMMisc => SimpleStep::Frame,
            VStall | ClimbRate | VBestClimb | VTurn | PwMargin | VLaunch => SimpleStep::DesignPoint,
            TwStatic | MotorMargin | DProp | PropPitch | PMaxElec | MrTMax => {
                SimpleStep::Propulsion
            }
        }
    }

    /// Kısa, günlük dilde alan adı (i18n anahtarı).
    pub fn key(self) -> &'static str {
        use Field::*;
        match self {
            Altitude => "simple.field.altitude",
            CruiseSpeed => "simple.field.cruise_speed",
            Payload => "simple.field.payload",
            MrCruise => "simple.field.mr_cruise",
            Avionics => "simple.field.avionics",
            Propulsion => "simple.field.propulsion",
            FuseTail => "simple.field.fuse_tail",
            RhoMat => "simple.field.material",
            MrFrame => "simple.field.mr_frame",
            MrArms => "simple.field.mr_arms",
            MrDProp => "simple.field.mr_d_prop",
            MrClearance => "simple.field.mr_clearance",
            MrRPlate => "simple.field.mr_r_plate",
            MrMFrame => "simple.field.mr_m_frame",
            MrMMotor => "simple.field.mr_m_motor",
            MrMEsc => "simple.field.mr_m_esc",
            MrMFc => "simple.field.mr_m_fc",
            MrMMisc => "simple.field.mr_m_misc",
            VStall => "simple.field.v_stall",
            ClimbRate => "simple.field.climb_rate",
            VBestClimb => "simple.field.v_best_climb",
            VTurn => "simple.field.v_turn",
            PwMargin => "simple.field.pw_margin",
            VLaunch => "simple.field.v_launch",
            TwStatic => "simple.field.tw_static",
            MotorMargin => "simple.field.motor_margin",
            DProp => "simple.field.d_prop",
            PropPitch => "simple.field.prop_pitch",
            PMaxElec => "simple.field.p_max",
            MrTMax => "simple.field.mr_t_max",
        }
    }

    pub fn get(self, p: &Project) -> f64 {
        use Field::*;
        match self {
            Altitude => p.h,
            CruiseSpeed => p.v_cr,
            Payload => p.m_payload,
            MrCruise => p.mr_v_cruise,
            Avionics => p.m_avionics,
            Propulsion => p.m_propulsion,
            FuseTail => p.m_fuse_tail,
            RhoMat => p.rho_mat,
            MrFrame => p.mr_frame_size,
            MrArms => p.mr_arm_count as f64,
            MrDProp => p.mr_d_prop,
            MrClearance => p.mr_prop_clearance,
            MrRPlate => p.mr_r_plate,
            MrMFrame => p.mr_m_frame,
            MrMMotor => p.mr_m_motor,
            MrMEsc => p.mr_m_esc_tot,
            MrMFc => p.mr_m_fc,
            MrMMisc => p.mr_m_misc,
            VStall => p.v_stall_req,
            ClimbRate => p.rc,
            VBestClimb => p.v_y,
            VTurn => p.v_turn,
            PwMargin => p.pw_margin,
            VLaunch => p.v_launch,
            TwStatic => p.tw_static,
            MotorMargin => p.motor_margin,
            DProp => p.d_prop,
            PropPitch => p.prop_pitch,
            PMaxElec => p.p_max_elec,
            MrTMax => p.mr_t_max_motor,
        }
    }

    pub fn set(self, p: &mut Project, v: f64) {
        use Field::*;
        match self {
            Altitude => p.h = v,
            CruiseSpeed => p.v_cr = v,
            Payload => p.m_payload = v,
            MrCruise => p.mr_v_cruise = v,
            Avionics => p.m_avionics = v,
            Propulsion => p.m_propulsion = v,
            FuseTail => p.m_fuse_tail = v,
            RhoMat => p.rho_mat = v,
            MrFrame => p.mr_frame_size = v,
            MrArms => p.mr_arm_count = v.round().max(0.0) as u32,
            MrDProp => p.mr_d_prop = v,
            MrClearance => p.mr_prop_clearance = v,
            MrRPlate => p.mr_r_plate = v,
            MrMFrame => p.mr_m_frame = v,
            MrMMotor => p.mr_m_motor = v,
            MrMEsc => p.mr_m_esc_tot = v,
            MrMFc => p.mr_m_fc = v,
            MrMMisc => p.mr_m_misc = v,
            VStall => p.v_stall_req = v,
            ClimbRate => p.rc = v,
            VBestClimb => p.v_y = v,
            VTurn => p.v_turn = v,
            PwMargin => p.pw_margin = v,
            VLaunch => p.v_launch = v,
            TwStatic => p.tw_static = v,
            MotorMargin => p.motor_margin = v,
            DProp => p.d_prop = v,
            PropPitch => p.prop_pitch = v,
            PMaxElec => p.p_max_elec = v,
            MrTMax => p.mr_t_max_motor = v,
        }
    }
}

/// Türetmesi olan basit mod adımları (aerodinamik kendi durumunu `AeroSimple` içinde tutar).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimpleStep {
    Mission,
    Weight,
    Frame,
    DesignPoint,
    Propulsion,
}

impl SimpleStep {
    pub const ALL: [SimpleStep; 5] = [
        SimpleStep::Mission,
        SimpleStep::Weight,
        SimpleStep::Frame,
        SimpleStep::DesignPoint,
        SimpleStep::Propulsion,
    ];
}

/// Türetilmiş değer listesi.
pub type Derived = Vec<(Field, f64)>;

/// Türetmenin ihtiyaç duyduğu model çıktıları (UI'dan bağımsız küçük bağlam).
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    /// Uçuş kütlesi [kg] (multirotorda `mr.m_mtow`, diğerlerinde `weight.m_mtow`).
    pub mtow_kg: f64,
    /// Aktif motor sayısı (multirotor itki türetmesi için).
    pub n_motors: u32,
    /// Modelin önerdiği motor elektrik gücü [W] (`prop.p_motor_suggest`).
    pub p_motor_suggest_w: f64,
    pub materials: &'a [Material],
}

impl<'a> Ctx<'a> {
    pub fn from_model(p: &Project, m: &crate::calc::Model, materials: &'a [Material]) -> Self {
        let mr = p.drone_type == DroneType::Multirotor;
        Ctx {
            mtow_kg: if mr { m.mr.m_mtow } else { m.weight.m_mtow },
            n_motors: m.mr.n_motors,
            p_motor_suggest_w: m.prop.p_motor_suggest,
            materials,
        }
    }
}

/// Basit mod cevapları ve özelleştirme durumu (projede saklanır; eski dosyalarda varsayılan).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SimpleState {
    pub mission: mission::MissionAnswers,
    pub weight: weight::WeightAnswers,
    pub frame: frame::FrameAnswers,
    pub design_point: design_point::DpAnswers,
    pub propulsion: propulsion::PropAnswers,
    pub structure: structure::StructAnswers,
    /// Türetmesi etkinleştirilmiş adımlar (kullanıcı en az bir kez cevap verdi).
    pub active: Vec<SimpleStep>,
    /// Elle değiştirilmiş (özelleştirilmiş) alanlar; basit mod bunları ezmez.
    pub custom: Vec<Field>,
    /// Bu karede basit modun kendi yazdığı alanlar (özelleştirme sayılmaz).
    #[serde(skip)]
    pub applied_now: Vec<Field>,
}

/// Tüm türetilebilir alanların anlık değerleri (özelleştirme tespiti için).
#[derive(Clone, Debug)]
pub struct Snapshot(Vec<f64>);

impl Snapshot {
    pub fn take(p: &Project) -> Self {
        Snapshot(Field::ALL.iter().map(|f| f.get(p)).collect())
    }
}

const TOL: f64 = 1e-9;

impl SimpleState {
    pub fn is_active(&self, s: SimpleStep) -> bool {
        self.active.contains(&s)
    }

    pub fn is_custom(&self, f: Field) -> bool {
        self.custom.contains(&f)
    }

    /// Adımın özelleştirilmiş alanları (rozet için).
    pub fn custom_in(&self, s: SimpleStep) -> Vec<Field> {
        self.custom
            .iter()
            .copied()
            .filter(|f| f.step() == s)
            .collect()
    }

    /// Kare başı: bu karenin "basit mod yazdı" listesini sıfırlar.
    pub fn begin_frame(&mut self) {
        self.applied_now.clear();
    }

    /// Adımın türetmesini ilk kez açar. Proje varsayılanından farklı mevcut değerler
    /// "özelleştirilmiş" sayılır ve ilk cevapta ezilmez.
    pub fn activate(&mut self, step: SimpleStep, p: &Project) {
        if self.is_active(step) {
            return;
        }
        let d = Project::default();
        for f in Field::ALL.iter().copied().filter(|f| f.step() == step) {
            if (f.get(p) - f.get(&d)).abs() > TOL && !self.custom.contains(&f) {
                self.custom.push(f);
            }
        }
        self.active.push(step);
    }

    /// Gelişmiş modda / ham değer bölümünde yapılan düzenlemeleri "özelleştirildi" olarak işaretler.
    /// Yalnızca türetmesi etkin adımların alanları izlenir; basit modun kendi yazdıkları sayılmaz.
    pub fn mark_edits(&mut self, before: &Snapshot, p: &Project) {
        for (i, f) in Field::ALL.iter().copied().enumerate() {
            if !self.is_active(f.step()) || self.applied_now.contains(&f) {
                continue;
            }
            if (f.get(p) - before.0[i]).abs() > TOL && !self.custom.contains(&f) {
                self.custom.push(f);
            }
        }
    }

    /// Açık kullanıcı isteğiyle (ör. "Öner" düğmesi) alanları özelleştirmeden çıkarır.
    pub fn clear_custom(&mut self, fields: &[Field]) {
        self.custom.retain(|f| !fields.contains(f));
    }

    /// Adımın tüm özelleştirmelerini kaldırır ("Varsayılana dön").
    pub fn reset_step(&mut self, step: SimpleStep) {
        self.custom.retain(|f| f.step() != step);
    }
}

/// Bir adım için türetilmiş değerler (yalnızca cevaplara, tür ve bağlama bağlı; saf).
pub fn derive_step(
    step: SimpleStep,
    st: &SimpleState,
    dt: DroneType,
    layout: crate::calc::multirotor::MultirotorLayout,
    ctx: &Ctx,
) -> Derived {
    match step {
        SimpleStep::Mission => mission::derive(&st.mission, dt),
        SimpleStep::Weight => weight::derive(&st.weight, dt),
        SimpleStep::Frame => frame::derive(&st.frame, dt, layout),
        SimpleStep::DesignPoint => design_point::derive(&st.design_point, st.mission.launch).0,
        SimpleStep::Propulsion => propulsion::derive(&st.propulsion, st.mission.launch, dt, ctx),
    }
}

/// Basit moddaki etkin adımların türetilmiş değerlerini projeye yazar (özelleştirilenler hariç).
/// Değişim olduysa `true` döner (arayüz yeniden çizim ister). Aynı girdilerle tekrar çağrılması
/// güvenlidir (idempotent); türetme model çıktısına yalnızca kendisinden bağımsız büyüklükler
/// (kütle, önerilen motor gücü) için bakar.
pub fn sync(p: &mut Project, ctx: &Ctx) -> bool {
    if !p.aero_simple.simple_mode {
        return false;
    }
    let mut st = std::mem::take(&mut p.simple);
    let mut changed = false;
    for step in SimpleStep::ALL {
        if !st.is_active(step) {
            continue;
        }
        let d = derive_step(step, &st, p.drone_type, p.mr_layout, ctx);
        for (f, v) in d {
            if st.is_custom(f) || !v.is_finite() {
                continue;
            }
            if (f.get(p) - v).abs() > TOL {
                f.set(p, v);
                changed = true;
            }
            if !st.applied_now.contains(&f) {
                st.applied_now.push(f);
            }
            if f == Field::RhoMat {
                weight::apply_material_identity(p, &st.weight, ctx.materials);
            }
        }
    }
    p.simple = st;
    changed
}

/// Basit moddaki bir cevap değiştiğinde çağrılır: adımı etkinleştirip değerleri hemen uygular.
pub fn answer_changed(p: &mut Project, step: SimpleStep, ctx: &Ctx) {
    let mut st = std::mem::take(&mut p.simple);
    st.activate(step, p);
    p.simple = st;
    sync(p, ctx);
}

// ---------------------------------------------------------------------------------------------
// Ortak günlük dil yardımcıları
// ---------------------------------------------------------------------------------------------

/// m/s → km/h.
pub fn kmh(ms: f64) -> f64 {
    ms * 3.6
}

/// En yakın tam inç [m] (pervane çapı/hatvesi için).
pub fn round_to_inch(d_m: f64) -> f64 {
    (d_m / 0.0254).round().max(1.0) * 0.0254
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::calc::{Library, evaluate};

    /// Tüm adımların türetmesini etkinleştirip kararlı hale gelene kadar uygular.
    pub fn apply_all(p: &mut Project, lib: &Library) {
        p.aero_simple.simple_mode = true;
        for _ in 0..5 {
            let m = evaluate(p, lib);
            let ctx = Ctx::from_model(p, &m, &lib.materials);
            for s in SimpleStep::ALL {
                answer_changed(p, s, &ctx);
            }
        }
    }
}
