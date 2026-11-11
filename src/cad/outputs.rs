// SPDX-License-Identifier: AGPL-3.0-or-later
//! "Çıktılar" adımının mantığı (arayüzden bağımsız): parça seti, parça seçimi, parametreden
//! betik üretimi, montaj betiği (App::Link), malzeme listesi (BOM), montaj kontrolleri
//! (temas / çakışma sınıflandırması, bağlantısız parça) ve CAD kütlesinin tasarıma aktarımı.
//!
//! FreeCAD'e giden tüm değerler sayısaldır ya da JSON dizgi değişmezi olarak gömülür; kullanıcı
//! metni doğrudan Python koduna eklenmez.

use super::ExportOptions;
use super::freecad::{
    PY_MESH_PROPS, PY_SURF_HELPERS, PlugSpec, common_python_footer, generate_fuselage_script,
    generate_motor_mount_script, generate_multirotor_arm_script, generate_multirotor_plate_script,
    generate_tail_script, generate_wing_script, py_str_literal, sanitize_ident,
};
use super::layout::Layout;
use super::runner::{
    CadLimits, CadRunError, CancelFlag, Interference, PairMetric, StageInfo,
    run_freecad_script_controlled,
};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::project::{DroneType, Project};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

/// FreeCAD kurulum sayfası (bulunamazsa gösterilir).
pub const FREECAD_DOWNLOAD_URL: &str = "https://www.freecad.org/downloads.php";

/// Üretilebilen parça türleri.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PartId {
    Wing,
    WingSpar,
    TailHorizontal,
    TailVertical,
    TailVTail,
    Winglet,
    Fuselage,
    MotorMount,
    BoomWingClamp,
    VtolBoom,
    LiftMotorMount,
    MrArm,
    MrPlate,
    MrMotorMount,
    LandingGear,
}

impl PartId {
    /// Kararlı (dosya/BOM) anahtarı.
    pub fn key(self) -> &'static str {
        match self {
            PartId::Wing => "wing",
            PartId::WingSpar => "wing_spar",
            PartId::TailHorizontal => "tail_h",
            PartId::TailVertical => "tail_v",
            PartId::TailVTail => "tail_vtail",
            PartId::Winglet => "winglet",
            PartId::Fuselage => "fuselage",
            PartId::MotorMount => "motor_mount",
            PartId::BoomWingClamp => "boom_wing_clamp",
            PartId::VtolBoom => "vtol_boom",
            PartId::LiftMotorMount => "lift_motor_mount",
            PartId::MrArm => "mr_arm",
            PartId::MrPlate => "mr_plate",
            PartId::MrMotorMount => "mr_motor_mount",
            PartId::LandingGear => "landing_gear",
        }
    }

    /// Arayüz/BOM görünen adı (i18n anahtarı).
    pub fn title(self) -> &'static str {
        use crate::i18n::t;
        match self {
            PartId::Wing => t("out.part.wing"),
            PartId::WingSpar => t("out.part.wing_spar"),
            PartId::TailHorizontal => t("out.part.tail_h"),
            PartId::TailVertical => t("out.part.tail_v"),
            PartId::TailVTail => t("out.part.tail_vtail"),
            PartId::Winglet => t("out.part.winglet"),
            PartId::Fuselage => t("out.part.fuselage"),
            PartId::MotorMount => t("out.part.motor_mount"),
            PartId::BoomWingClamp => t("out.part.boom_wing_clamp"),
            PartId::VtolBoom => t("out.part.vtol_boom"),
            PartId::LiftMotorMount => t("out.part.lift_motor_mount"),
            PartId::MrArm => t("out.part.mr_arm"),
            PartId::MrPlate => t("out.part.mr_plate"),
            PartId::MrMotorMount => t("out.part.mr_motor_mount"),
            PartId::LandingGear => t("out.part.landing_gear"),
        }
    }

    /// FreeCAD belgesindeki ana katı nesnesinin adı.
    fn main_object(self, part_name: &str) -> String {
        match self {
            PartId::Wing => "WingSolid".into(),
            PartId::TailHorizontal | PartId::TailVertical | PartId::TailVTail | PartId::Winglet => {
                "TailSolid".into()
            }
            PartId::Fuselage => "FuselageSolid".into(),
            _ => sanitize_ident(part_name),
        }
    }

    /// Karbon boru mu (kütle karbon yoğunluğuyla)?
    pub fn is_tube(self) -> bool {
        matches!(self, PartId::WingSpar | PartId::VtolBoom)
    }
}

/// Ayna nesnesinin adı (sağ taraf kopyaları buna bağlanır).
pub fn mirror_object_name(main: &str) -> String {
    format!("{main}_Mirror")
}

/// Drone türüne göre sunulan parçalar (sıralı).
pub fn available_parts(p: &Project) -> Vec<PartId> {
    match p.drone_type {
        DroneType::Multirotor => vec![
            PartId::MrPlate,
            PartId::MrArm,
            PartId::MrMotorMount,
            PartId::LandingGear,
        ],
        DroneType::FlyingWing => vec![
            PartId::Wing,
            PartId::WingSpar,
            PartId::Winglet,
            PartId::Fuselage,
            PartId::MotorMount,
        ],
        dt => {
            let tail = match p.tail_kind {
                TailKind::VTail => vec![PartId::TailVTail],
                _ => vec![PartId::TailHorizontal, PartId::TailVertical],
            };
            let mut v = vec![PartId::Wing, PartId::WingSpar];
            v.extend(tail);
            v.extend([PartId::Fuselage, PartId::MotorMount]);
            if dt == DroneType::VtolQuadplane {
                v.extend([
                    PartId::BoomWingClamp,
                    PartId::VtolBoom,
                    PartId::LiftMotorMount,
                ]);
            }
            v.push(PartId::LandingGear);
            v
        }
    }
}

/// Parça seçimi (onay kutuları): varsayılan olarak tüm uygun parçalar seçili.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    chosen: BTreeSet<PartId>,
}

impl Selection {
    pub fn all_for(p: &Project) -> Self {
        Self {
            chosen: available_parts(p).into_iter().collect(),
        }
    }
    pub fn contains(&self, id: PartId) -> bool {
        self.chosen.contains(&id)
    }
    pub fn set(&mut self, id: PartId, on: bool) {
        if on {
            self.chosen.insert(id);
        } else {
            self.chosen.remove(&id);
        }
    }
    /// Seçili parçalar, yalnızca bu drone türünde uygun olanlar (sıralı).
    pub fn selected_for(&self, p: &Project) -> Vec<PartId> {
        available_parts(p)
            .into_iter()
            .filter(|id| self.chosen.contains(id))
            .collect()
    }
}

/// Bir parçanın montajdaki adedi.
pub fn part_quantity(id: PartId, p: &Project) -> u32 {
    match id {
        PartId::Wing | PartId::TailHorizontal | PartId::TailVTail | PartId::Winglet => 2,
        PartId::WingSpar | PartId::VtolBoom | PartId::BoomWingClamp => 2,
        PartId::LiftMotorMount => 4,
        PartId::MrArm | PartId::MrMotorMount => p.mr_arm_count.max(1),
        PartId::LandingGear => 2,
        _ => 1,
    }
}

/// Bom-kanat kelepçesi (mm). Yerel çerçeve: delik ekseni X boyunca, orijin delik merkezinde;
/// üstteki ayak (pillar) kanat alt yüzeyinin birebir semeriyle biter.
#[derive(Clone, Debug)]
pub struct ClampCadParams {
    pub name: String,
    pub bore_mm: f64,
    pub wall_mm: f64,
    pub width_mm: f64,
    /// Ayak üst yüzünün delik eksenine yüksekliği (semer kesimi öncesi).
    pub pillar_top_mm: f64,
    pub slit_mm: f64,
    /// Kanat alt yüzeyi semeri (kanadın birebir parçası, kelepçe koordinatlarında).
    pub saddle: Option<PlugSpec>,
}

/// Boru (kanat kirişi, VTOL bomu) parametreleri (mm). Yerel çerçeve: eksen +Z, uç yüz z=0.
#[derive(Clone, Debug)]
pub struct TubeCadParams {
    pub name: String,
    pub od_mm: f64,
    pub id_mm: f64,
    pub length_mm: f64,
}

/// İniş takımı bacağı parametreleri (mm). Yerel çerçeve: bağlantı tablasının üst yüzü z=0
/// (orijin tabla merkezi), boru +Y yönünde `splay_deg` açıyla dışa ve aşağı iner.
#[derive(Clone, Debug)]
pub struct LandingLegCadParams {
    pub name: String,
    pub leg_dia_mm: f64,
    pub leg_len_mm: f64,
    pub splay_deg: f64,
    pub tab_l_mm: f64,
    pub tab_w_mm: f64,
    pub tab_t_mm: f64,
    pub foot_len_mm: f64,
    pub foot_w_mm: f64,
    pub foot_t_mm: f64,
}

impl LandingLegCadParams {
    pub fn from_project(p: &Project) -> Self {
        let base = if p.drone_type.is_multirotor() {
            (p.mr_frame_size * 1000.0 * 0.25).clamp(60.0, 200.0)
        } else {
            (p.d_fuse * 1000.0 * 1.2).clamp(60.0, 200.0)
        };
        Self {
            name: format!("{}_Inis_Takimi_Bacagi", sanitize_ident(&p.name)),
            leg_dia_mm: 6.0,
            leg_len_mm: base,
            splay_deg: 15.0,
            tab_l_mm: 30.0,
            tab_w_mm: 12.0,
            tab_t_mm: 3.0,
            foot_len_mm: 40.0,
            foot_w_mm: 16.0,
            foot_t_mm: 3.0,
        }
    }
}

fn sheet_rows(rows: &[(&str, String)]) -> String {
    rows.iter()
        .enumerate()
        .map(|(i, (alias, val))| {
            let a = sanitize_ident(alias);
            format!(
                "sheet.set(\"A{r}\", \"{a}\"); sheet.set(\"B{r}\", {v}); sheet.setAlias(\"B{r}\", \"{a}\")\n",
                r = i + 1,
                v = py_str_literal(val),
            )
        })
        .collect()
}

/// Kelepçe betiği: halka + ayak, delik, alt yarık; ayak üstü kanat alt yüzeyinin semeriyle
/// kesilir. Sağ taraf için parametrik ayna nesnesi (yerel XZ düzlemine göre) üretilir.
pub fn generate_clamp_script(params: &ClampCadParams, options: &ExportOptions) -> String {
    let clean = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean,
        &options.out_dir,
        options,
        "width",
        &format!("{:.1} mm", params.width_mm + 5.0),
        &[],
        &clean,
    );
    let saddle = params.saddle.as_ref().filter(|s| s.is_finite());
    let spec = serde_json::json!({ "saddle": saddle });
    let rows = sheet_rows(&[
        ("bore", format!("{:.2} mm", params.bore_mm)),
        ("wall", format!("{:.2} mm", params.wall_mm)),
        ("width", format!("{:.2} mm", params.width_mm)),
        ("pillar_top", format!("{:.2} mm", params.pillar_top_mm)),
        ("slit", format!("{:.2} mm", params.slit_mm)),
    ]);
    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Bom-Kanat Kelepcesi - WerSeuS UAV Calc
# Yerel cerceve: delik ekseni X, orijin delik merkezi, ayak +Z yonunde kanat alt yuzeyine uzanir.
import FreeCAD, Part, math, json

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{rows}doc.recompute()

spec = json.loads({spec_py})
{helpers}
rot_x = FreeCAD.Rotation(FreeCAD.Vector(0, 1, 0), 90)

ring = doc.addObject("Part::Cylinder", "ClampRing")
ring.setExpression("Radius", "Params.bore / 2 + Params.wall")
ring.setExpression("Height", "Params.width")
ring.Placement.Rotation = rot_x
ring.setExpression("Placement.Base.x", "-Params.width / 2")

pillar = doc.addObject("Part::Box", "ClampPillar")
pillar.setExpression("Length", "Params.width")
pillar.setExpression("Width", "Params.bore + 2 * Params.wall")
pillar.setExpression("Height", "Params.pillar_top")
pillar.setExpression("Placement.Base.x", "-Params.width / 2")
pillar.setExpression("Placement.Base.y", "-(Params.bore / 2 + Params.wall)")

body = doc.addObject("Part::Fuse", "ClampBody")
body.Base = ring; body.Tool = pillar

bore = doc.addObject("Part::Cylinder", "ClampBore")
bore.setExpression("Radius", "Params.bore / 2")
bore.setExpression("Height", "Params.width + 2 mm")
bore.Placement.Rotation = rot_x
bore.setExpression("Placement.Base.x", "-Params.width / 2 - 1 mm")

slit = doc.addObject("Part::Box", "ClampSlit")
slit.setExpression("Length", "Params.width + 2 mm")
slit.setExpression("Width", "Params.slit")
slit.setExpression("Height", "Params.bore / 2 + Params.wall + 1 mm")
slit.setExpression("Placement.Base.x", "-Params.width / 2 - 1 mm")
slit.setExpression("Placement.Base.y", "-Params.slit / 2")
slit.setExpression("Placement.Base.z", "-(Params.bore / 2 + Params.wall + 1 mm)")

tools = [bore, slit]
if spec["saddle"]:
    sad = doc.addObject("Part::Feature", "WingSaddle")
    sad.Shape = surf_plug(spec["saddle"])
    tools.append(sad)
comp = doc.addObject("Part::Compound", "ClampTools")
comp.Links = tools
main_part = doc.addObject("Part::Cut", "{part_name}")
main_part.Base = body; main_part.Tool = comp
doc.recompute()

# Sag kelepce: yerel XZ duzlemine gore parametrik ayna kopyasi (semer sag kanada uyar).
mir = doc.addObject("Part::Mirroring", "{mirror}")
mir.Source = main_part
mir.Normal = FreeCAD.Vector(0, 1, 0)
mir.Base = FreeCAD.Vector(0, 0, 0)
doc.recompute()

{footer}
"#,
        part_name = clean,
        rows = rows,
        spec_py = py_str_literal(&spec.to_string()),
        helpers = PY_SURF_HELPERS,
        mirror = mirror_object_name(&clean),
        footer = footer,
    )
}

/// Boru betiği (kanat kirişi, VTOL bomu): eksen +Z, uç yüz z=0.
pub fn generate_tube_script(params: &TubeCadParams, options: &ExportOptions) -> String {
    let clean = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean,
        &options.out_dir,
        options,
        "length",
        &format!("{:.1} mm", params.length_mm + 20.0),
        &[],
        &clean,
    );
    let rows = sheet_rows(&[
        ("od", format!("{:.3} mm", params.od_mm)),
        ("id", format!("{:.3} mm", params.id_mm.max(0.0))),
        ("length", format!("{:.2} mm", params.length_mm)),
    ]);
    let body = if params.id_mm > 0.0 && params.id_mm < params.od_mm {
        format!(
            r#"inner = doc.addObject("Part::Cylinder", "TubeInner")
inner.setExpression("Radius", "Params.id / 2")
inner.setExpression("Height", "Params.length + 2 mm")
inner.Placement.Base.z = -1.0
main_part = doc.addObject("Part::Cut", "{clean}")
main_part.Base = outer; main_part.Tool = inner
"#
        )
    } else {
        format!(
            r#"main_part = doc.addObject("Part::Refine", "{clean}")
main_part.Source = outer
"#
        )
    };
    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Karbon Boru - WerSeuS UAV Calc
# Yerel cerceve: boru ekseni +Z, uc yuz z = 0.
import FreeCAD, Part

doc = FreeCAD.newDocument("{clean}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{rows}doc.recompute()

outer = doc.addObject("Part::Cylinder", "TubeOuter")
outer.setExpression("Radius", "Params.od / 2")
outer.setExpression("Height", "Params.length")
{body}
doc.recompute()

{footer}
"#
    )
}

/// İniş takımı bacağı betiği: bağlantı tablası + dışa açılı boru + ayak.
pub fn generate_landing_leg_script(
    params: &LandingLegCadParams,
    options: &ExportOptions,
) -> String {
    let clean = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean,
        &options.out_dir,
        options,
        "leg_len",
        &format!("{:.1} mm", params.leg_len_mm + 10.0),
        &[],
        &clean,
    );
    let rows = sheet_rows(&[
        ("leg_dia", format!("{:.2} mm", params.leg_dia_mm)),
        ("leg_len", format!("{:.2} mm", params.leg_len_mm)),
        ("splay_deg", format!("{:.2} deg", params.splay_deg)),
        ("tab_l", format!("{:.2} mm", params.tab_l_mm)),
        ("tab_w", format!("{:.2} mm", params.tab_w_mm)),
        ("tab_t", format!("{:.2} mm", params.tab_t_mm)),
        ("foot_len", format!("{:.2} mm", params.foot_len_mm)),
        ("foot_w", format!("{:.2} mm", params.foot_w_mm)),
        ("foot_t", format!("{:.2} mm", params.foot_t_mm)),
    ]);
    let (s, c) = params.splay_deg.to_radians().sin_cos();
    let end_y = params.leg_len_mm * s;
    let end_z = -params.tab_t_mm - params.leg_len_mm * c;
    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Inis Takimi Bacagi - WerSeuS UAV Calc
# Yerel cerceve: baglanti tablasinin ust yuzu z = 0 (orijin tabla merkezi); boru +Y yonunde disa acilir.
import FreeCAD, Part

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{rows}doc.recompute()

tab = doc.addObject("Part::Box", "LegTab")
tab.setExpression("Length", "Params.tab_l")
tab.setExpression("Width", "Params.tab_w")
tab.setExpression("Height", "Params.tab_t")
tab.setExpression("Placement.Base.x", "-Params.tab_l / 2")
tab.setExpression("Placement.Base.y", "-Params.tab_w / 2")
tab.setExpression("Placement.Base.z", "-Params.tab_t")

leg = doc.addObject("Part::Cylinder", "LegTube")
leg.setExpression("Radius", "Params.leg_dia / 2")
leg.setExpression("Height", "Params.leg_len")
leg.Placement = FreeCAD.Placement(FreeCAD.Vector(0, 0, -{tab_t:.4}), FreeCAD.Rotation(FreeCAD.Vector(0, 0, 1), FreeCAD.Vector(0, {s:.8}, -{c:.8})))

foot = doc.addObject("Part::Box", "LegFoot")
foot.setExpression("Length", "Params.foot_len")
foot.setExpression("Width", "Params.foot_w")
foot.setExpression("Height", "Params.foot_t")
foot.Placement.Base = FreeCAD.Vector(-{fl_half:.4}, {end_y:.4} - {fw_half:.4}, {end_z:.4} - {ft:.4})

main_part = doc.addObject("Part::MultiFuse", "{part_name}")
main_part.Shapes = [tab, leg, foot]

doc.recompute()

{footer}
"#,
        part_name = clean,
        rows = rows,
        tab_t = params.tab_t_mm,
        s = s,
        c = c,
        fl_half = params.foot_len_mm / 2.0,
        fw_half = params.foot_w_mm / 2.0,
        ft = params.foot_t_mm,
        end_y = end_y,
        end_z = end_z,
        footer = footer,
    )
}

/// Bir parçanın üretim planı.
#[derive(Clone, Debug)]
pub struct PartPlan {
    pub id: PartId,
    /// Dosya adı gövdesi (sanitize edilmiş).
    pub stem: String,
    pub main_object: String,
    pub qty: u32,
    pub script: String,
}

/// Seçilen parça için betik ve dosya bilgisi üretir. Parçalar BÜTÜN verilir (bölme yok).
pub fn plan_part(id: PartId, p: &Project, m: &Model, out_dir: &Path) -> PartPlan {
    plan_part_with(id, p, &Layout::new(p, m), out_dir)
}

fn plan_part_with(id: PartId, p: &Project, lay: &Layout, out_dir: &Path) -> PartPlan {
    let opts = ExportOptions {
        export_fcstd: true,
        export_step: true,
        export_stl: false,
        out_dir: out_dir.to_path_buf(),
    };
    let (name, script) = match id {
        PartId::Wing => (
            lay.wing.name.clone(),
            generate_wing_script(&lay.wing, &opts),
        ),
        PartId::WingSpar => {
            let q = lay.spar.clone().unwrap_or_else(|| TubeCadParams {
                name: format!("{}_Kanat_Kirisi", sanitize_ident(&p.name)),
                od_mm: lay.wing.spar_od_mm,
                id_mm: lay.wing.spar_id_mm,
                length_mm: 10.0,
            });
            (q.name.clone(), generate_tube_script(&q, &opts))
        }
        PartId::TailHorizontal => (
            lay.tail_h.name.clone(),
            generate_tail_script(&lay.tail_h, &opts),
        ),
        PartId::TailVertical => (
            lay.tail_v.name.clone(),
            generate_tail_script(&lay.tail_v, &opts),
        ),
        PartId::TailVTail => (
            lay.vtail.name.clone(),
            generate_tail_script(&lay.vtail, &opts),
        ),
        PartId::Winglet => (
            lay.winglet.name.clone(),
            generate_tail_script(&lay.winglet, &opts),
        ),
        PartId::Fuselage => (
            lay.fuselage.name.clone(),
            generate_fuselage_script(&lay.fuselage, &opts),
        ),
        PartId::MotorMount => (
            lay.motor_mount.name.clone(),
            generate_motor_mount_script(&lay.motor_mount, &opts),
        ),
        PartId::LiftMotorMount => (
            lay.lift_mount.name.clone(),
            generate_motor_mount_script(&lay.lift_mount, &opts),
        ),
        PartId::MrMotorMount => (
            lay.mr_mount.name.clone(),
            generate_motor_mount_script(&lay.mr_mount, &opts),
        ),
        PartId::BoomWingClamp => (
            lay.clamp.name.clone(),
            generate_clamp_script(&lay.clamp, &opts),
        ),
        PartId::VtolBoom => (
            lay.boom.name.clone(),
            generate_tube_script(&lay.boom, &opts),
        ),
        PartId::MrArm => (
            lay.mr_arm.name.clone(),
            generate_multirotor_arm_script(&lay.mr_arm, &opts),
        ),
        PartId::MrPlate => (
            lay.mr_plate.name.clone(),
            generate_multirotor_plate_script(&lay.mr_plate, &opts),
        ),
        PartId::LandingGear => (
            lay.leg.name.clone(),
            generate_landing_leg_script(&lay.leg, &opts),
        ),
    };
    let stem = sanitize_ident(&name);
    PartPlan {
        id,
        main_object: id.main_object(&name),
        stem,
        qty: part_quantity(id, p),
        script,
    }
}

/// Başarıyla üretilmiş bir parçanın özeti (FreeCAD sonucundan).
#[derive(Clone, Debug)]
pub struct PartResult {
    pub id: PartId,
    pub stem: String,
    pub main_object: String,
    pub qty: u32,
    pub volume_mm3: f64,
    pub fcstd_file: String,
}

/// Malzeme listesi satırı.
#[derive(Clone, Debug, PartialEq)]
pub struct BomRow {
    pub id: PartId,
    pub part: String,
    pub qty: u32,
    pub material: String,
    /// Tek parça kütlesi [g]; hacim bilinmiyorsa `None`.
    pub unit_mass_g: Option<f64>,
}

impl BomRow {
    pub fn total_mass_g(&self) -> Option<f64> {
        self.unit_mass_g.map(|m| m * self.qty as f64)
    }
}

/// Parçaların malzemesi: projedeki yapı malzemesi (ad, yoğunluk kg/m³).
pub fn project_material(p: &Project) -> (String, f64) {
    let name = if p.material_name.is_empty() {
        p.material_id.clone()
    } else {
        p.material_name.clone()
    };
    (name, p.rho_mat)
}

/// Parçanın malzemesi: borular karbon (`rho_cf`), diğerleri proje yapı malzemesi.
pub fn part_material(id: PartId, p: &Project) -> (String, f64) {
    if id.is_tube() {
        (
            crate::i18n::t("out.material.carbon_tube").to_string(),
            p.rho_cf,
        )
    } else {
        project_material(p)
    }
}

/// Kütle [g] = hacim [mm³] × 1e-9 × yoğunluk [kg/m³] × 1000.
pub fn mass_g_from_volume(volume_mm3: f64, density_kg_m3: f64) -> Option<f64> {
    if volume_mm3.is_finite()
        && volume_mm3 > 0.0
        && density_kg_m3.is_finite()
        && density_kg_m3 > 0.0
    {
        Some(volume_mm3 * 1e-9 * density_kg_m3 * 1000.0)
    } else {
        None
    }
}

/// Üretim sonuçlarından malzeme listesi.
pub fn build_bom(p: &Project, results: &[PartResult]) -> Vec<BomRow> {
    results
        .iter()
        .map(|r| {
            let (mat, rho) = part_material(r.id, p);
            BomRow {
                id: r.id,
                part: r.id.title().to_string(),
                qty: r.qty,
                material: mat,
                unit_mass_g: mass_g_from_volume(r.volume_mm3, rho),
            }
        })
        .collect()
}

fn csv_cell(s: &str) -> String {
    // Formül enjeksiyonuna karşı: baştaki boşluk/sekme/satır sonu atlanınca tehlikeli karakterle
    // başlıyorsa başına ' eklenir (Excel/LibreOffice baştaki boşlukları yutabilir).
    let lead = s.trim_start_matches([' ', '\t', '\r', '\n']);
    let s = if lead.starts_with(['=', '+', '-', '@', '|']) || s.starts_with(['\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if s.contains([',', '"', '\n', '\r', ';', '\t']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// BOM CSV metni.
pub fn bom_csv(rows: &[BomRow]) -> String {
    use crate::i18n::t;
    let mut out = format!(
        "{},{},{},{},{}\n",
        t("out.bom.part"),
        t("out.bom.qty"),
        t("out.bom.material"),
        t("out.bom.unit_mass_g"),
        t("out.bom.total_mass_g")
    );
    for r in rows {
        let um = r.unit_mass_g.map(|v| format!("{v:.1}")).unwrap_or_default();
        let tm = r
            .total_mass_g()
            .map(|v| format!("{v:.1}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_cell(&r.part),
            r.qty,
            csv_cell(&r.material),
            um,
            tm
        ));
    }
    out
}

/// BOM Markdown metni.
pub fn bom_markdown(title: &str, rows: &[BomRow]) -> String {
    use crate::i18n::t;
    let mut out = format!("# {title}\n\n");
    out.push_str(&format!(
        "| {} | {} | {} | {} | {} |\n|---|---:|---|---:|---:|\n",
        t("out.bom.part"),
        t("out.bom.qty"),
        t("out.bom.material"),
        t("out.bom.unit_mass_g"),
        t("out.bom.total_mass_g")
    ));
    let mut total = 0.0;
    for r in rows {
        let um = r
            .unit_mass_g
            .map(|v| format!("{v:.1}"))
            .unwrap_or_else(|| "-".into());
        let tm = r
            .total_mass_g()
            .map(|v| {
                total += v;
                format!("{v:.1}")
            })
            .unwrap_or_else(|| "-".into());
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            r.part.replace(['|', '\n', '\r'], "/"),
            r.qty,
            r.material.replace(['|', '\n', '\r'], "/"),
            um,
            tm
        ));
    }
    out.push_str(&format!(
        "\n**{}: {:.1} g**\n\n{}\n",
        t("out.bom.total"),
        total,
        t("out.bom.note_solid")
    ));
    out
}

/// Montajdaki bir örnek (parça kopyası) ve yerleşimi.
#[derive(Clone, Debug, Serialize)]
pub struct AssemblyItem {
    pub label: String,
    pub file: String,
    pub obj: String,
    /// Parça yerel çerçevesinden montaj çerçevesine 4x4 satır öncelikli matris.
    pub m: [f64; 16],
    pub density: f64,
    /// Bağlandığı (mate) ebeveyn kopyanın etiketi.
    pub parent: Option<String>,
}

/// Montaj kopyaları: yerleşim `Layout` bağlantılarından gelir (bkz. `cad::layout`). Montaj
/// çerçevesi: X kuyruğa doğru, Y sağa, Z yukarı; burun (firewall) x=0. Multirotorda merkez
/// plaka orijinde.
pub fn assembly_items(p: &Project, m: &Model, results: &[PartResult]) -> Vec<AssemblyItem> {
    let lay = Layout::new(p, m);
    let by_id: BTreeMap<PartId, &PartResult> = results.iter().map(|r| (r.id, r)).collect();
    lay.instances
        .iter()
        .filter_map(|i| {
            let r = by_id.get(&i.id)?;
            let obj = if i.mirror {
                mirror_object_name(&r.main_object)
            } else {
                r.main_object.clone()
            };
            Some(AssemblyItem {
                label: sanitize_ident(&i.label),
                file: r.fcstd_file.clone(),
                obj,
                m: i.frame.matrix16(),
                density: part_material(i.id, p).1,
                parent: i.parent.as_deref().map(sanitize_ident),
            })
        })
        .collect()
}

/// Temas sayılan en büyük açıklık [mm] (yüzey yüzeye, delikte boru, yuvada parça).
pub const CONTACT_TOL_MM: f64 = 0.05;
/// Çakışma için en küçük ortak hacim [mm³].
pub const INTERF_MIN_VOLUME_MM3: f64 = 1.0;
/// Çakışma için en küçük girişim derinliği [mm] (ortak hacim / yarım yüzey alanı): bunun
/// altındaki ortak hacim, çakışık yüzeylerdeki sayısal kalıntıdır (temas).
pub const INTERF_MIN_DEPTH_MM: f64 = 0.05;

/// İki parçanın teması.
#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    pub a: String,
    pub b: String,
}

/// Çift ölçümlerinin sınıflandırması.
#[derive(Clone, Debug, Default)]
pub struct PairClasses {
    pub interferences: Vec<Interference>,
    pub contacts: Vec<Contact>,
    /// Ölçümü başarısız olan çiftler (gizlenmez, ayrıca raporlanır).
    pub failed: Vec<String>,
    /// Tasarım bağlantısı (mate) olduğu hâlde ebeveyniyle temas etmeyen kopyalar
    /// ("çocuk: ebeveyn"): yuvasından kaymış ya da oturmamış parça.
    pub unseated: Vec<String>,
}

/// Her tasarım bağlantısı için çocuk ile ebeveyn arasında temas (ya da çakışma) olmalı.
pub fn unseated_mates(items: &[AssemblyItem], classes: &PairClasses) -> Vec<String> {
    let labels: BTreeSet<&str> = items.iter().map(|i| i.label.as_str()).collect();
    let touching = |a: &str, b: &str| {
        classes
            .contacts
            .iter()
            .map(|c| (c.a.as_str(), c.b.as_str()))
            .chain(
                classes
                    .interferences
                    .iter()
                    .map(|i| (i.a.as_str(), i.b.as_str())),
            )
            .any(|(x, y)| (x == a && y == b) || (x == b && y == a))
    };
    items
        .iter()
        .filter_map(|i| {
            let par = i.parent.as_deref()?;
            (labels.contains(par) && !touching(&i.label, par))
                .then(|| format!("{}: {par}", i.label))
        })
        .collect()
}

/// Çift ölçümlerini temas / çakışma olarak sınıflandırır. Çakışma: (a) bulanık ortak hacim
/// `INTERF_MIN_VOLUME_MM3` üstünde VE girişim derinliği (2·hacim/alan) `INTERF_MIN_DEPTH_MM`
/// üstünde, ya da (b) örnekleme ölçümü en az iki noktayı iki parçanın da içinde ve
/// `INTERF_MIN_DEPTH_MM`'den derinde buldu. Temas: mesafe ≤ `CONTACT_TOL_MM` ya da eşik altı
/// ortak hacim.
pub fn classify_pairs(pairs: &[PairMetric]) -> PairClasses {
    let mut out = PairClasses::default();
    for pm in pairs {
        if pm.error.is_some() && pm.volume_mm3 < 0.0 {
            out.failed.push(format!(
                "{} x {}: {}",
                pm.a,
                pm.b,
                pm.error.clone().unwrap_or_default()
            ));
            continue;
        }
        let v = pm.volume_mm3.max(0.0);
        let depth = if pm.area_mm2 > 0.0 {
            2.0 * v / pm.area_mm2
        } else {
            f64::INFINITY
        };
        let fuzzy_hit = v > INTERF_MIN_VOLUME_MM3 && depth > INTERF_MIN_DEPTH_MM;
        // Örnekleme kanıtı: en az iki nokta iki parçanın da malzemesinde ve yüzeylerden derinde.
        let probe_hit = pm.probe_hits >= 2 && pm.probe_depth_mm > INTERF_MIN_DEPTH_MM;
        if fuzzy_hit || probe_hit {
            out.interferences.push(Interference {
                a: pm.a.clone(),
                b: pm.b.clone(),
                volume_mm3: v.max(pm.verified_volume_mm3),
            });
        } else if v > 0.0 || (pm.dist_mm >= 0.0 && pm.dist_mm <= CONTACT_TOL_MM) {
            out.contacts.push(Contact {
                a: pm.a.clone(),
                b: pm.b.clone(),
            });
        }
    }
    out
}

/// Montajda hiçbir parçaya (temas ya da çakışma ile) bağlanmayan kopyalar: ilk kopyadan
/// (gövde / merkez plaka) başlayan bağlantı grafında ulaşılamayanlar.
pub fn floating_parts(labels: &[String], classes: &PairClasses) -> Vec<String> {
    let Some(root) = labels.first() else {
        return Vec::new();
    };
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (a, b) in classes
        .contacts
        .iter()
        .map(|c| (c.a.as_str(), c.b.as_str()))
        .chain(
            classes
                .interferences
                .iter()
                .map(|i| (i.a.as_str(), i.b.as_str())),
        )
    {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![root.as_str()];
    while let Some(n) = stack.pop() {
        if seen.insert(n) {
            if let Some(v) = adj.get(n) {
                stack.extend(v.iter().copied());
            }
        }
    }
    labels
        .iter()
        .filter(|l| !seen.contains(l.as_str()))
        .cloned()
        .collect()
}

/// Montaj betiği: App::Link ile tüm kopyalar, tek montaj FCStd + STEP; birbirine yakın her çift
/// için ortak hacim, ortak hacmin yüzey alanı ve en kısa mesafe; kütle ve ağırlık merkezi.
/// Sınıflandırma Rust tarafında (`classify_pairs`).
pub fn generate_assembly_script(asm_name: &str, items: &[AssemblyItem], out_dir: &Path) -> String {
    let clean = sanitize_ident(asm_name);
    let items: Vec<&AssemblyItem> = items
        .iter()
        .filter(|i| i.m.iter().all(|v| v.is_finite()) && i.density.is_finite())
        .collect();
    let spec = serde_json::json!({
        "items": items,
        "tol": CONTACT_TOL_MM,
        "depth_min": INTERF_MIN_DEPTH_MM,
    })
    .to_string();
    let spec_py = py_str_literal(&spec);
    let out_dir_py = py_str_literal(&out_dir.to_string_lossy().replace('\\', "/"));
    let name_py = py_str_literal(&clean);
    let fc_py = py_str_literal(&format!("{clean}.FCStd"));
    let step_py = py_str_literal(&format!("{clean}.step"));
    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Montaj Uretici (App::Link) - WerSeuS UAV Calc
import FreeCAD, Part, json, os, traceback

RESULT_START = "__WERSEUS_CAD_JSON_START__"
RESULT_END = "__WERSEUS_CAD_JSON_END__"

def stage(sid, i=None, n=None):
    # Arayuz ilerlemesi icin sabit kimlikli asama satiri (kullanici verisi icermez).
    line = "__WERSEUS_STAGE__:" + sid
    if i is not None and n is not None:
        line += ":" + str(i) + "/" + str(n)
    print(line, flush=True)

{mesh_props}
{probe}
try:
    spec = json.loads({spec_py})
    tol = float(spec["tol"])
    depth_min = float(spec["depth_min"])
    out_dir = {out_dir_py}
    os.makedirs(out_dir, exist_ok=True)
    doc = FreeCAD.newDocument({name_py})
    fcstd_path = os.path.join(out_dir, {fc_py})
    doc.saveAs(fcstd_path)  # dis belgeye baglanti icin once kaydedilmeli
    docs = {{}}
    export_objs = []
    placed = []
    for k, it in enumerate(spec["items"]):
        stage("load", k + 1, len(spec["items"]))
        d = docs.get(it["file"])
        if d is None:
            d = FreeCAD.openDocument(it["file"])
            docs[it["file"]] = d
        src = d.getObject(it["obj"])
        if src is None:
            raise RuntimeError("Kaynak nesne bulunamadi: " + it["obj"])
        pl = FreeCAD.Placement(FreeCAD.Matrix(*it["m"]))
        obj = doc.addObject("App::Link", it["label"])
        obj.setLink(src)
        obj.Placement = pl
        sh = src.Shape.copy()
        sh.Placement = pl.multiply(sh.Placement)
        export_objs.append(obj)
        placed.append((it["label"], sh, it["density"]))
    doc.recompute()

    # Cift olcumleri: ortak hacim, ortak hacmin alani, en kisa mesafe
    pairs = []
    for a in range(len(placed)):
        stage("pairs", a + 1, len(placed))
        for b in range(a + 1, len(placed)):
            la, sa, _ = placed[a]
            lb, sb, _ = placed[b]
            bb = FreeCAD.BoundBox(sa.BoundBox)
            bb.enlarge(2.0 * tol + 0.01)
            if not bb.intersect(sb.BoundBox):
                continue
            rec = {{"a": la, "b": lb, "volume_mm3": -1.0, "area_mm2": 0.0, "dist_mm": -1.0, "error": None}}
            try:
                rec["dist_mm"] = float(sa.distToShape(sb)[0])
            except Exception as e:
                rec["error"] = "dist: " + str(e)
            if rec["dist_mm"] < 0.0 or rec["dist_mm"] <= tol:
                # Bulanik (fuzzy) ortak hacim: tol'den yakin yuzeyler cakisik sayilir; boylece
                # yuvadaki/delikteki parcanin cakisik yuzeyleri sahte ince hacim uretmez.
                try:
                    c = sa.common([sb], tol)
                except Exception:
                    try:
                        c = sa.common(sb)
                    except Exception as e:
                        c = None
                        rec["error"] = "common: " + str(e)
                if c is not None:
                    if c.Solids:
                        v, ar, _cg = mesh_props(c)
                        rec["volume_mm3"] = v
                        rec["area_mm2"] = ar
                    else:
                        rec["volume_mm3"] = 0.0
                # Bulanik islem gercek girisimi kacirabilir: boolean'dan bagimsiz ornekleme olcumu.
                try:
                    rec.update(probe_pair(sa, sb, depth_min))
                except Exception as e:
                    rec["error"] = "probe: " + str(e)
            else:
                rec["volume_mm3"] = 0.0
            pairs.append(rec)

    # Kutle ve agirlik merkezi
    stage("mass")
    mass_kg = 0.0
    mx = my = mz = 0.0
    total_vol = 0.0
    for label, sh, rho in placed:
        vol, _ar, c = mesh_props(sh)
        m = vol * 1e-9 * float(rho)
        mass_kg += m
        mx += m * c[0]; my += m * c[1]; mz += m * c[2]
        total_vol += vol
    cg = [mx / mass_kg, my / mass_kg, mz / mass_kg] if mass_kg > 0 else [0.0, 0.0, 0.0]

    stage("export")
    doc.save()
    step_path = os.path.join(out_dir, {step_py})
    import Import
    Import.export(export_objs, step_path)

    result = {{
        "ok": True, "status": "ok", "part": {name_py}, "valid": True,
        "volume_mm3": total_vol, "param_update_ok": False, "num_segments": len(export_objs),
        "fcstd_path": fcstd_path, "step_files": [step_path], "stl_files": [],
        "error": None, "traceback": None,
        "mass_kg": mass_kg, "cg_mm": cg, "pairs": pairs,
    }}
    print(RESULT_START + json.dumps(result) + RESULT_END)
except Exception as e:
    err = {{
        "ok": False, "status": "error", "part": {name_py}, "valid": False, "volume_mm3": 0.0,
        "param_update_ok": False, "num_segments": 0, "fcstd_path": "", "step_files": [],
        "stl_files": [], "error": str(e), "traceback": traceback.format_exc(),
    }}
    print(RESULT_START + json.dumps(err) + RESULT_END)
"#,
        mesh_props = PY_MESH_PROPS,
        probe = PY_PROBE,
    )
}

/// Boolean işlemlerinden bağımsız girişim ölçümü (FreeCAD Python). OCC'nin bulanık `common`
/// işlemi çakışık B-spline kabuklarda gerçek girişimi boş döndürebiliyor; tam `common` ise
/// delikteki boru gibi bilinçli geçmelerde sahte hacim üretebiliyor. Bu yüzden üç kaynaktan
/// (A yüzeyi, B yüzeyi, tam ortak hacmin yüzeyi) içeri doğru farklı derinliklerde noktalar
/// örneklenir. Hızlı ön eleme üçgen ağı üzerinde ışın paritesiyle (numpy), kesin onay OCC
/// `isInside` + `distToShape` ile yapılır: bir nokta HEM A'nın HEM B'nin malzemesi içindeyse
/// ve iki yüzeye de `depth_min`'den uzaksa gerçek girişim kanıtıdır. Örnekleme derinlikleri
/// `depth_min`'den büyük başladığı için çakışık yüzeylerde böyle nokta oluşmaz.
const PY_PROBE: &str = r#"
import numpy as np
PROBE_DEPTHS = [0.08, 0.2, 0.5, 1.5, 5.0]
PROBE_N = 120
PROBE_CONFIRM = 10
_TRI = {}
_BND = {}

def _bnd(shape):
    # Kati icindeki noktada distToShape 0 verir; derinlik icin yalniz sinir yuzeyleri.
    k = id(shape)
    if k not in _BND:
        _BND[k] = Part.Compound(shape.Faces)
    return _BND[k]

def _tris(shape, key=None, defl=0.05):
    if key is not None and key in _TRI:
        return _TRI[key]
    pts, tris = shape.tessellate(defl)
    if not tris:
        out = (np.zeros((0, 3)), np.zeros((0, 3)), np.zeros((0, 3)))
    else:
        P = np.array([[p.x, p.y, p.z] for p in pts], dtype=float)
        T = np.array(tris, dtype=int)
        out = (P[T[:, 0]], P[T[:, 1]], P[T[:, 2]])
    if key is not None:
        _TRI[key] = out
    return out

def _inside_mesh(Q, tri):
    # +X yonlu isin ile parite (vektorel); yalniz isinin gecebilecegi ucgenler suzulur.
    a, b, c = tri
    res = np.zeros(len(Q), dtype=bool)
    if len(Q) == 0 or len(a) == 0:
        return res
    ymin = np.minimum(np.minimum(a[:, 1], b[:, 1]), c[:, 1]); ymax = np.maximum(np.maximum(a[:, 1], b[:, 1]), c[:, 1])
    zmin = np.minimum(np.minimum(a[:, 2], b[:, 2]), c[:, 2]); zmax = np.maximum(np.maximum(a[:, 2], b[:, 2]), c[:, 2])
    m = (ymax >= Q[:, 1].min()) & (ymin <= Q[:, 1].max()) & (zmax >= Q[:, 2].min()) & (zmin <= Q[:, 2].max())
    a, b, c = a[m], b[m], c[m]
    if len(a) == 0:
        return res
    ymin, ymax, zmin, zmax = ymin[m], ymax[m], zmin[m], zmax[m]
    e1 = b - a; e2 = c - a
    det = e1[:, 1] * e2[:, 2] - e2[:, 1] * e1[:, 2]
    ok = np.abs(det) > 1e-14
    for i, q in enumerate(Q):
        sel = ok & (ymin <= q[1]) & (ymax >= q[1]) & (zmin <= q[2]) & (zmax >= q[2])
        if not sel.any():
            continue
        dy = q[1] - a[sel, 1]; dz = q[2] - a[sel, 2]; d = det[sel]
        l1 = (dy * e2[sel, 2] - e2[sel, 1] * dz) / d
        l2 = (e1[sel, 1] * dz - dy * e1[sel, 2]) / d
        hit = (l1 >= 0) & (l2 >= 0) & (l1 + l2 <= 1)
        xh = a[sel, 0] + l1 * e1[sel, 0] + l2 * e2[sel, 0]
        res[i] = (np.count_nonzero(hit & (xh > q[0])) % 2) == 1
    return res

def _probe_from(src, src_key, shapes, keys, region, thr):
    """src yuzeyinden ice orneklenen noktalar; hem sa hem sb icinde ve derinde olanlar kanittir."""
    sa, sb = shapes
    a, b, c = _tris(src, src_key)
    if len(a) == 0:
        return 0, 0, 0.0
    cen = (a + b + c) / 3.0
    nrm = np.cross(b - a, c - a)
    ln = np.linalg.norm(nrm, axis=1)
    lo = np.array([region.XMin, region.YMin, region.ZMin]); hi = np.array([region.XMax, region.YMax, region.ZMax])
    sel = (ln > 1e-12) & np.all(cen >= lo, axis=1) & np.all(cen <= hi, axis=1)
    cen = cen[sel]; nrm = nrm[sel] / ln[sel][:, None]
    if len(cen) == 0:
        return 0, 0, 0.0
    if len(cen) > PROBE_N:
        idx = np.linspace(0, len(cen) - 1, PROBE_N).astype(int)
        cen = cen[idx]; nrm = nrm[idx]
    src_tri = (a, b, c)
    e0 = PROBE_DEPTHS[0]
    sgn = np.where(_inside_mesh(cen - nrm * e0, src_tri), -1.0, 1.0)
    tri_ab = [_tris(sh, k) for sh, k in zip(shapes, keys)]
    cands = []
    for e in PROBE_DEPTHS:
        Q = cen + nrm * (sgn[:, None] * e)
        m = _inside_mesh(Q, src_tri)
        for t in tri_ab:
            if not m.any():
                break
            m &= _inside_mesh(Q, t)
        for q in Q[m]:
            cands.append((e, q))
    cands.sort(key=lambda x: -x[0])
    hits = 0; dmax = 0.0
    for e, q in cands[:PROBE_CONFIRM]:
        v = FreeCAD.Vector(float(q[0]), float(q[1]), float(q[2]))
        if sa.isInside(v, 1e-7, False) and sb.isInside(v, 1e-7, False):
            pv = Part.Vertex(v)
            d = min(_bnd(sa).distToShape(pv)[0], _bnd(sb).distToShape(pv)[0])
            dmax = max(dmax, d)
            if d > thr:
                hits += 1
                if hits >= 3:
                    break
    return len(cen), hits, dmax

PROBE_CF_N = 12
PROBE_CF_DEPTHS = [0.1, 0.2]

def _probe_common_exact(cf, sa, sb, thr):
    """Tam ortak hacmin icinden (ince duvar kesisimleri dahil) noktalar, yalniz kesin OCC
    testleriyle: sahte ortak hacim (ornegin delikteki borunun tamami) burada elenir."""
    a, b, c = _tris(cf, None, 0.02)
    if len(a) == 0:
        return 0, 0.0
    cen = (a + b + c) / 3.0
    nrm = np.cross(b - a, c - a)
    ln = np.linalg.norm(nrm, axis=1)
    ok = ln > 1e-12
    cen = cen[ok]; nrm = nrm[ok] / ln[ok][:, None]
    if len(cen) > PROBE_CF_N:
        idx = np.linspace(0, len(cen) - 1, PROBE_CF_N).astype(int)
        cen = cen[idx]; nrm = nrm[idx]
    hits = 0; dmax = 0.0
    for q0, n in zip(cen, nrm):
        for e in PROBE_CF_DEPTHS:
            got = False
            for sg in (-1.0, 1.0):
                q = q0 + n * (sg * e)
                v = FreeCAD.Vector(float(q[0]), float(q[1]), float(q[2]))
                if not cf.isInside(v, 1e-7, False):
                    continue
                if sa.isInside(v, 1e-7, False) and sb.isInside(v, 1e-7, False):
                    pv = Part.Vertex(v)
                    d = min(_bnd(sa).distToShape(pv)[0], _bnd(sb).distToShape(pv)[0])
                    dmax = max(dmax, d)
                    if d > thr:
                        hits += 1
                        got = True
                break
            if got:
                break
        if hits >= 3:
            break
    return hits, dmax

def probe_pair(sa, sb, thr):
    region = FreeCAD.BoundBox(sa.BoundBox).intersected(sb.BoundBox)
    region.enlarge(1.0)
    hits = 0; dmax = 0.0; vver = 0.0
    keys = (id(sa), id(sb))
    if region.isValid():
        for src, k in ((sa, keys[0]), (sb, keys[1])):
            _t, h, d = _probe_from(src, k, (sa, sb), keys, region, thr)
            hits += h; dmax = max(dmax, d)
    try:
        cf = sa.common(sb)
        if cf.Solids:
            vfull = mesh_props(cf)[0]
            reg = FreeCAD.BoundBox(cf.BoundBox)
            reg.enlarge(0.5)
            h, d = _probe_common_exact(cf, sa, sb, thr)
            hits += h; dmax = max(dmax, d)
            if h > 0:
                vver = vfull
    except Exception:
        pass
    return {"probe_hits": hits, "probe_depth_mm": dmax, "verified_volume_mm3": vver}
"#;

/// CAD kütlesinin yazılacağı proje alanı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassTarget {
    /// Gövde + kuyruk (ve kanat dışı yapı): `m_fuse_tail`.
    FuseTail,
    /// Multirotor çerçevesi: `mr_m_frame`.
    MrFrame,
}

/// CAD kütlesi aktarımı için izin verilen oran bandı (CAD yapı toplamı / programın yapı
/// tahmini). Kavramsal tasarım yapı kütlesi tahminleri ±%30-50 belirsizdir; CAD ise yalnız
/// birincil yapıyı (kabuk, kaburga, boru, plaka) içerir, bağlantı elemanı/yapıştırıcı/kablo
/// içermez. 2 katlık bant bu doğal farkı kabul eder ama dolu katı modeli (≈3 kat) ya da eksik
/// ana bileşen (yarıdan az) gibi sistematik model hatalarını reddeder.
pub const MASS_RATIO_MIN: f64 = 0.5;
pub const MASS_RATIO_MAX: f64 = 2.0;

/// CAD kütlesi ile programın yapı kütlesi tahmininin karşılaştırması.
#[derive(Clone, Debug, PartialEq)]
pub struct MassCheck {
    pub target: MassTarget,
    /// CAD kanat (iki yarı + kiriş) kütlesi [kg] (multirotorda 0).
    pub cad_wing_kg: f64,
    /// CAD kanat dışı yapı kütlesi [kg] (aktarılacak değer).
    pub cad_other_kg: f64,
    /// Programın kanat kütlesi tahmini (`m_wing`) [kg].
    pub prog_wing_kg: f64,
    /// Projedeki hedef alanın şu anki değeri [kg].
    pub prog_other_kg: f64,
    /// Tüm parçalar üretildi mi?
    pub complete: bool,
}

impl MassCheck {
    pub fn cad_total_kg(&self) -> f64 {
        self.cad_wing_kg + self.cad_other_kg
    }
    pub fn prog_total_kg(&self) -> f64 {
        self.prog_wing_kg + self.prog_other_kg
    }
    /// CAD yapı toplamı / program yapı tahmini.
    pub fn ratio(&self) -> f64 {
        let d = self.prog_total_kg();
        if d > 0.0 {
            self.cad_total_kg() / d
        } else {
            f64::NAN
        }
    }
    pub fn ratio_ok(&self) -> bool {
        let r = self.ratio();
        r.is_finite() && (MASS_RATIO_MIN..=MASS_RATIO_MAX).contains(&r)
    }
    /// Aktarma düğmesi açık mı?
    pub fn transfer_allowed(&self) -> bool {
        self.complete && self.ratio_ok() && self.cad_other_kg.is_finite() && self.cad_other_kg > 0.0
    }
}

/// Parça sonuçlarından bileşen bazında CAD kütlesi ve programın tahminleri.
pub fn mass_check(p: &Project, m: &Model, results: &[PartResult]) -> MassCheck {
    let multirotor = p.drone_type == DroneType::Multirotor;
    let mut wing = 0.0;
    let mut other = 0.0;
    for r in results {
        let rho = part_material(r.id, p).1;
        let kg = mass_g_from_volume(r.volume_mm3, rho).unwrap_or(0.0) / 1000.0 * r.qty as f64;
        match r.id {
            PartId::Wing | PartId::WingSpar => wing += kg,
            _ => other += kg,
        }
    }
    let produced: BTreeSet<PartId> = results.iter().map(|r| r.id).collect();
    let complete = available_parts(p).iter().all(|id| produced.contains(id));
    if multirotor {
        MassCheck {
            target: MassTarget::MrFrame,
            cad_wing_kg: 0.0,
            cad_other_kg: other + wing,
            prog_wing_kg: 0.0,
            prog_other_kg: p.mr_m_frame,
            complete,
        }
    } else {
        MassCheck {
            target: MassTarget::FuseTail,
            cad_wing_kg: wing,
            cad_other_kg: other,
            prog_wing_kg: m.weight.m_wing,
            prog_other_kg: p.m_fuse_tail,
            complete,
        }
    }
}

/// Hedef alanın şu anki değeri.
pub fn mass_target_value(p: &Project, t: MassTarget) -> f64 {
    match t {
        MassTarget::FuseTail => p.m_fuse_tail,
        MassTarget::MrFrame => p.mr_m_frame,
    }
}

fn set_mass_target(p: &mut Project, t: MassTarget, v: f64) {
    match t {
        MassTarget::FuseTail => p.m_fuse_tail = v,
        MassTarget::MrFrame => p.mr_m_frame = v,
    }
}

/// CAD kütlesini bileşen bazında tasarıma aktarır (yalnızca kullanıcı onayıyla çağrılır):
/// kanat dışı yapı → `m_fuse_tail` (multirotorda çerçeve → `mr_m_frame`). Kanat kütlesi
/// program tarafından kabuk+kirişten hesaplandığı için yazılmaz (çift sayım olmaz). Kontrol
/// geçmiyorsa hiçbir şey yazılmaz. Dönüş: geri alma için eski değer.
pub fn apply_mass_to_project(p: &mut Project, mc: &MassCheck) -> Option<f64> {
    if !mc.transfer_allowed() {
        return None;
    }
    let old = mass_target_value(p, mc.target);
    set_mass_target(p, mc.target, mc.cad_other_kg);
    Some(old)
}

/// Aktarımı geri alır; alan aktarımdan sonra kullanıcı tarafından değiştirildiyse
/// (değer artık aktarılan değer değilse) onun değeri ezilmez ve `false` döner.
pub fn undo_mass_transfer(p: &mut Project, t: MassTarget, old: f64, applied: f64) -> bool {
    let cur = mass_target_value(p, t);
    if !(old.is_finite() && old >= 0.0) || (cur - applied).abs() > 1e-9 {
        return false;
    }
    set_mass_target(p, t, old);
    true
}

/// Parça geometrisini etkileyen tasarım verisinin parmak izi. Aktarımın yazdığı kütle
/// alanları hariç tutulur (aktarım sonucu bayat saymaz).
pub fn design_fingerprint(p: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut q = p.clone();
    q.m_fuse_tail = 0.0;
    q.mr_m_frame = 0.0;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(&q).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// Tüm üretimin özeti.
#[derive(Clone, Debug)]
pub struct OutputsSummary {
    pub out_dir: std::path::PathBuf,
    pub parts: Vec<PartResult>,
    pub bom: Vec<BomRow>,
    pub assembly_fcstd: String,
    pub assembly_step: String,
    pub mass_kg: f64,
    pub cg_mm: Vec<f64>,
    pub interferences: Vec<Interference>,
    pub contacts: Vec<Contact>,
    pub check_failed: Vec<String>,
    pub floating: Vec<String>,
    pub unseated: Vec<String>,
    pub mass: MassCheck,
    /// Üretim anındaki tasarım parmak izi (`design_fingerprint`); proje sonradan değişirse
    /// sonuçlar bayattır.
    pub fingerprint: u64,
    pub failed: Vec<String>,
    pub files: Vec<String>,
}

impl OutputsSummary {
    fn empty(out_dir: std::path::PathBuf, mass: MassCheck) -> Self {
        Self {
            out_dir,
            parts: Vec::new(),
            bom: Vec::new(),
            assembly_fcstd: String::new(),
            assembly_step: String::new(),
            mass_kg: 0.0,
            cg_mm: Vec::new(),
            interferences: Vec::new(),
            contacts: Vec::new(),
            check_failed: Vec::new(),
            floating: Vec::new(),
            unseated: Vec::new(),
            mass,
            fingerprint: 0,
            failed: Vec::new(),
            files: Vec::new(),
        }
    }
}

/// Montaj sonucunu sınıflandırır (temas / çakışma / bağlantısız / oturmamış).
fn finish_assembly(
    r: &super::runner::CadExecutionResult,
    items: &[AssemblyItem],
) -> (PairClasses, Vec<String>) {
    let mut classes = classify_pairs(&r.pairs);
    classes.unseated = unseated_mates(items, &classes);
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    let floating = floating_parts(&labels, &classes);
    (classes, floating)
}

/// Montaj kopyalarını FreeCAD'de birleştirir ve kontrolleri sınıflandırır; iptal edilebilir,
/// zaman aşımlıdır ve betiğin aşama satırlarını iletir.
pub fn run_assembly_controlled(
    cmd_path: &Path,
    items: &[AssemblyItem],
    out_dir: &Path,
    script_dir: &Path,
    timeout: Duration,
    cancel: &CancelFlag,
    on_stage: impl FnMut(StageInfo),
) -> Result<(super::runner::CadExecutionResult, PairClasses, Vec<String>), CadRunError> {
    let script = generate_assembly_script(ASSEMBLY_NAME, items, out_dir);
    let script_path = script_dir.join("Montaj.py");
    std::fs::write(&script_path, script).map_err(|e| CadRunError::Failed(e.to_string()))?;
    let r = run_freecad_script_controlled(cmd_path, &script_path, timeout, cancel, on_stage)?;
    let (classes, floating) = finish_assembly(&r, items);
    Ok((r, classes, floating))
}

/// Montaj dosyalarının (FCStd/STEP) adı.
pub const ASSEMBLY_NAME: &str = "Montaj";

/// Üretimin hangi aşamada olduğu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunPhase {
    Parts,
    Assembly,
}

/// Arayüze giden canlı ilerleme (hangi parça/aşama, kaçta kaç).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputsProgress {
    pub phase: RunPhase,
    /// Parça aşamasında 1 tabanlı sıra; montajda parça sayısı.
    pub current: usize,
    pub total: usize,
    /// Üretilen parçanın görünen adı (montajda boş).
    pub part: String,
    /// Betikten gelen son aşama (yalnız montajda).
    pub stage: Option<StageInfo>,
}

/// Üretim neden yarıda kesildi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortReason {
    Cancelled,
    TotalTimeout,
}

/// Kontrollü üretimden arayüze giden iletiler.
#[derive(Clone, Debug)]
pub enum RunEvent {
    Log(String),
    Progress(OutputsProgress),
    Done(Box<OutputsSummary>),
    /// İptal ya da toplam süre aşımı: `completed` tam üretilen parça sayısı, `removed` silinen
    /// yarım dosya sayısı.
    Aborted {
        reason: AbortReason,
        completed: usize,
        removed: usize,
    },
    Error(String),
}

/// Betik aşama kimliğinin görünen adı (bilinmeyen kimlik için `None`).
pub fn stage_label(id: &str) -> Option<&'static str> {
    use crate::i18n::t;
    Some(match id {
        "load" => t("out.stage.load"),
        "pairs" => t("out.stage.pairs"),
        "mass" => t("out.stage.mass"),
        "export" => t("out.stage.export"),
        _ => return None,
    })
}

/// Parçanın üreteceği dosyalar (yalnız bu adlar; başka parçanın dosyasına dokunmaz).
fn expected_output_files(out_dir: &Path, stem: &str) -> Vec<std::path::PathBuf> {
    ["FCStd", "FCBak", "step", "stl"]
        .iter()
        .map(|ext| out_dir.join(format!("{stem}.{ext}")))
        .collect()
}

type FileSnapshot = Vec<(std::path::PathBuf, Option<std::time::SystemTime>)>;

fn snapshot_files(paths: Vec<std::path::PathBuf>) -> FileSnapshot {
    paths
        .into_iter()
        .map(|p| {
            let m = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
            (p, m)
        })
        .collect()
}

/// Anlık görüntüden sonra oluşan ya da değişen dosyaları siler (yarım çıktı); silinen sayısı.
fn remove_new_or_changed(snap: &FileSnapshot) -> usize {
    let mut n = 0;
    for (p, before) in snap {
        let now = std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if now.is_some() && now != *before && std::fs::remove_file(p).is_ok() {
            n += 1;
        }
    }
    n
}

/// Kalan toplam süreye göre bir adımın sınırı: `(sınır, toplam süre tarafından mı kısıldı)`;
/// toplam süre bittiyse `None`.
fn step_limit(limit: Duration, started: Instant, total: Duration) -> Option<(Duration, bool)> {
    let left = total
        .checked_sub(started.elapsed())
        .filter(|d| !d.is_zero())?;
    Some(if left < limit {
        (left, true)
    } else {
        (limit, false)
    })
}

fn minutes_text(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 && secs.is_multiple_of(60) {
        (secs / 60).to_string()
    } else {
        format!("{:.1}", secs as f64 / 60.0)
    }
}

/// Seçilen parçaları ve montajı arka planda üretir. `out_dir` kullanıcının seçtiği klasördür.
/// İptal bayrağı ve süre sınırları (parça / montaj / toplam) uygulanır; süreç sonlandırılınca
/// yarım dosyalar silinir.
pub fn run_outputs_controlled(
    cmd_path: std::path::PathBuf,
    p: Project,
    m: Model,
    ids: Vec<PartId>,
    out_dir: std::path::PathBuf,
    limits: CadLimits,
    cancel: CancelFlag,
) -> std::sync::mpsc::Receiver<RunEvent> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let log = |s: String| {
            let _ = tx.send(RunEvent::Log(s));
        };
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            let _ = tx.send(RunEvent::Error(format!("{}: {e}", out_dir.display())));
            return;
        }
        let script_dir = match super::runner::get_ascii_temp_cad_dir() {
            Ok(d) => {
                static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let k = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                d.join(format!("outputs_{}_{k}", std::process::id()))
            }
            Err(e) => {
                let _ = tx.send(RunEvent::Error(e));
                return;
            }
        };
        let _ = std::fs::create_dir_all(&script_dir);
        let abort = |reason: AbortReason, completed: usize, removed: usize| {
            let _ = std::fs::remove_dir_all(&script_dir);
            let _ = tx.send(RunEvent::Aborted {
                reason,
                completed,
                removed,
            });
        };
        let started = Instant::now();
        let total_limit = Duration::from_secs(limits.total_secs);
        let part_limit = Duration::from_secs(limits.part_secs);
        let asm_limit = Duration::from_secs(limits.assembly_secs);
        let n_parts = ids.len();
        let lay = Layout::new(&p, &m);
        let mut sum = OutputsSummary::empty(out_dir.clone(), mass_check(&p, &m, &[]));
        sum.fingerprint = design_fingerprint(&p);
        for (i, id) in ids.into_iter().enumerate() {
            if cancel.is_cancelled() {
                abort(AbortReason::Cancelled, sum.parts.len(), 0);
                return;
            }
            let Some((tmo, by_total)) = step_limit(part_limit, started, total_limit) else {
                abort(AbortReason::TotalTimeout, sum.parts.len(), 0);
                return;
            };
            let plan = plan_part_with(id, &p, &lay, &out_dir);
            let script_path = script_dir.join(format!("{}.py", plan.stem));
            if let Err(e) = std::fs::write(&script_path, &plan.script) {
                sum.failed.push(format!("{}: {e}", id.key()));
                continue;
            }
            log(format!("{} ...", id.key()));
            let _ = tx.send(RunEvent::Progress(OutputsProgress {
                phase: RunPhase::Parts,
                current: i + 1,
                total: n_parts,
                part: id.title().to_string(),
                stage: None,
            }));
            let snap = snapshot_files(expected_output_files(&out_dir, &plan.stem));
            match run_freecad_script_controlled(&cmd_path, &script_path, tmo, &cancel, |_| {}) {
                Ok(r) => {
                    sum.files.push(r.fcstd_path.clone());
                    sum.files.extend(r.step_files.iter().cloned());
                    sum.parts.push(PartResult {
                        id,
                        stem: plan.stem,
                        main_object: plan.main_object,
                        qty: plan.qty,
                        volume_mm3: r.volume_mm3,
                        fcstd_file: r.fcstd_path,
                    });
                }
                Err(CadRunError::Cancelled) => {
                    let removed = remove_new_or_changed(&snap);
                    abort(AbortReason::Cancelled, sum.parts.len(), removed);
                    return;
                }
                Err(CadRunError::TimedOut) => {
                    let removed = remove_new_or_changed(&snap);
                    if by_total {
                        abort(AbortReason::TotalTimeout, sum.parts.len(), removed);
                        return;
                    }
                    sum.failed.push(format!(
                        "{}: {}",
                        id.key(),
                        crate::i18n::tf("out.timeout_part_fmt", &[minutes_text(part_limit)])
                    ));
                }
                Err(CadRunError::Failed(e)) => {
                    let first = e.lines().next().unwrap_or("").to_string();
                    sum.failed.push(format!("{}: {first}", id.key()));
                }
            }
        }
        sum.bom = build_bom(&p, &sum.parts);
        sum.mass = mass_check(&p, &m, &sum.parts);
        let bom_title = format!("{} - BOM", p.name);
        let md = out_dir.join("BOM.md");
        let csv = out_dir.join("BOM.csv");
        if std::fs::write(&md, bom_markdown(&bom_title, &sum.bom)).is_ok() {
            sum.files.push(md.to_string_lossy().into_owned());
        }
        if std::fs::write(&csv, bom_csv(&sum.bom)).is_ok() {
            sum.files.push(csv.to_string_lossy().into_owned());
        }
        if !sum.parts.is_empty() {
            let items = assembly_items(&p, &m, &sum.parts);
            log("Montaj ...".to_string());
            let Some((tmo, by_total)) = step_limit(asm_limit, started, total_limit) else {
                abort(AbortReason::TotalTimeout, sum.parts.len(), 0);
                return;
            };
            let progress = |stage: Option<StageInfo>| {
                let _ = tx.send(RunEvent::Progress(OutputsProgress {
                    phase: RunPhase::Assembly,
                    current: n_parts,
                    total: n_parts,
                    part: String::new(),
                    stage,
                }));
            };
            progress(None);
            let snap = snapshot_files(expected_output_files(&out_dir, ASSEMBLY_NAME));
            match run_assembly_controlled(
                &cmd_path,
                &items,
                &out_dir,
                &script_dir,
                tmo,
                &cancel,
                |st| progress(Some(st)),
            ) {
                Ok((r, classes, floating)) => {
                    sum.assembly_fcstd = r.fcstd_path.clone();
                    sum.assembly_step = r.step_files.first().cloned().unwrap_or_default();
                    sum.files.push(r.fcstd_path);
                    sum.files.extend(r.step_files);
                    sum.mass_kg = r.mass_kg;
                    sum.cg_mm = r.cg_mm;
                    sum.interferences = classes.interferences;
                    sum.contacts = classes.contacts;
                    sum.check_failed = classes.failed;
                    sum.unseated = classes.unseated;
                    sum.floating = floating;
                    let rep = assembly_report_markdown(&format!("{} - Montaj", p.name), &sum);
                    let rp = out_dir.join("Montaj_Raporu.md");
                    if std::fs::write(&rp, rep).is_ok() {
                        sum.files.push(rp.to_string_lossy().into_owned());
                    }
                }
                Err(CadRunError::Cancelled) => {
                    let removed = remove_new_or_changed(&snap);
                    abort(AbortReason::Cancelled, sum.parts.len(), removed);
                    return;
                }
                Err(CadRunError::TimedOut) => {
                    let removed = remove_new_or_changed(&snap);
                    if by_total {
                        abort(AbortReason::TotalTimeout, sum.parts.len(), removed);
                        return;
                    }
                    sum.failed.push(format!(
                        "assembly: {}",
                        crate::i18n::tf("out.timeout_asm_fmt", &[minutes_text(asm_limit)])
                    ));
                }
                Err(CadRunError::Failed(e)) => {
                    let first = e.lines().next().unwrap_or("").to_string();
                    sum.failed.push(format!("assembly: {first}"));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&script_dir);
        let _ = tx.send(RunEvent::Done(Box::new(sum)));
    });
    rx
}

/// Montaj kontrol raporu (Markdown).
pub fn assembly_report_markdown(title: &str, s: &OutputsSummary) -> String {
    use crate::i18n::{t, tf};
    let mut out = format!("# {title}\n\n");
    out.push_str(&format!(
        "- {}: {:.1} g\n",
        t("out.report.mass"),
        s.mass_kg * 1000.0
    ));
    if s.cg_mm.len() == 3 {
        out.push_str(&format!(
            "- {}: X={:.1}, Y={:.1}, Z={:.1} mm\n",
            t("out.report.cg"),
            s.cg_mm[0],
            s.cg_mm[1],
            s.cg_mm[2]
        ));
    }
    out.push_str(&format!("\n## {}\n\n", t("out.report.interference")));
    if s.interferences.is_empty() {
        out.push_str(&format!("{}\n", t("out.report.no_interference")));
    } else {
        for i in &s.interferences {
            out.push_str(&format!("- {} x {}: {:.1} mm³\n", i.a, i.b, i.volume_mm3));
        }
    }
    out.push_str(&format!(
        "\n## {}\n\n{}\n\n",
        t("out.report.contacts"),
        tf(
            "out.report.contacts_note_fmt",
            &[format!("{CONTACT_TOL_MM}")]
        )
    ));
    for c in &s.contacts {
        out.push_str(&format!("- {} - {}\n", c.a, c.b));
    }
    if !s.floating.is_empty() {
        out.push_str(&format!(
            "\n**{}:** {}\n",
            t("out.report.floating"),
            s.floating.join(", ")
        ));
    }
    if !s.unseated.is_empty() {
        out.push_str(&format!(
            "\n**{}:** {}\n",
            t("out.report.unseated"),
            s.unseated.join(", ")
        ));
    }
    if !s.check_failed.is_empty() {
        out.push_str(&format!("\n**{}:**\n", t("out.report.check_failed")));
        for f in &s.check_failed {
            out.push_str(&format!("- {f}\n"));
        }
    }
    let mc = &s.mass;
    out.push_str(&format!("\n## {}\n\n", t("out.mass.heading")));
    out.push_str(&format!(
        "| | CAD [g] | {} [g] |\n|---|---:|---:|\n",
        t("out.mass.program")
    ));
    if mc.target == MassTarget::FuseTail {
        out.push_str(&format!(
            "| {} | {:.0} | {:.0} |\n",
            t("out.mass.wing"),
            mc.cad_wing_kg * 1000.0,
            mc.prog_wing_kg * 1000.0
        ));
    }
    out.push_str(&format!(
        "| {} | {:.0} | {:.0} |\n| {} | {:.0} | {:.0} |\n\n{}: {:.2}\n\n{}\n",
        mass_target_label(mc.target),
        mc.cad_other_kg * 1000.0,
        mc.prog_other_kg * 1000.0,
        t("out.mass.total"),
        mc.cad_total_kg() * 1000.0,
        mc.prog_total_kg() * 1000.0,
        t("out.mass.ratio"),
        mc.ratio(),
        t("out.bom.note_solid")
    ));
    out
}

/// Hedef alanın görünen adı.
pub fn mass_target_label(t: MassTarget) -> &'static str {
    match t {
        MassTarget::FuseTail => crate::i18n::t("out.mass.fuse_tail"),
        MassTarget::MrFrame => crate::i18n::t("out.mass.mr_frame"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cad::geom::Frame;
    use crate::calc::{Library, evaluate};

    fn fx(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.drone_type = dt;
        let m = evaluate(&p, &Library::embedded());
        (p, m)
    }

    fn fake_results(p: &Project, m: &Model, vol: f64) -> Vec<PartResult> {
        let dir = std::env::temp_dir().join("werseus_out_test");
        available_parts(p)
            .into_iter()
            .map(|id| {
                let plan = plan_part(id, p, m, &dir);
                PartResult {
                    id,
                    stem: plan.stem.clone(),
                    main_object: plan.main_object,
                    qty: plan.qty,
                    volume_mm3: vol,
                    fcstd_file: format!("C:/t/{}.FCStd", plan.stem),
                }
            })
            .collect()
    }

    #[test]
    fn parts_follow_drone_type() {
        let (p, _) = fx(DroneType::FixedWingConventional);
        let v = available_parts(&p);
        assert!(v.contains(&PartId::Wing) && v.contains(&PartId::Fuselage));
        assert!(v.contains(&PartId::WingSpar));
        assert!(!v.contains(&PartId::MrArm));
        let (p, _) = fx(DroneType::Multirotor);
        let v = available_parts(&p);
        assert!(v.contains(&PartId::MrArm) && v.contains(&PartId::MrPlate));
        assert!(!v.contains(&PartId::Wing));
        let (p, _) = fx(DroneType::VtolQuadplane);
        let v = available_parts(&p);
        assert!(v.contains(&PartId::BoomWingClamp) && v.contains(&PartId::LiftMotorMount));
        assert!(v.contains(&PartId::VtolBoom));
    }

    #[test]
    fn selection_filters_and_toggles() {
        let (p, _) = fx(DroneType::FixedWingConventional);
        let mut s = Selection::all_for(&p);
        assert!(s.contains(PartId::Wing));
        s.set(PartId::Wing, false);
        assert!(!s.selected_for(&p).contains(&PartId::Wing));
        // Türe uygun olmayan parça seçili olsa bile listelenmez.
        s.set(PartId::MrArm, true);
        assert!(!s.selected_for(&p).contains(&PartId::MrArm));
    }

    #[test]
    fn every_part_script_is_whole_and_safe() {
        let dir = std::env::temp_dir().join("werseus_out_test");
        for dt in [
            DroneType::FixedWingConventional,
            DroneType::FlyingWing,
            DroneType::Multirotor,
            DroneType::VtolQuadplane,
        ] {
            let (mut p, m) = fx(dt);
            // Enjeksiyon denemesi: tırnaklı ad ve malzeme
            p.name = "x\"); import os; os.system(\"calc\"); (\"".into();
            p.material_name = "\"; import os; #".into();
            for id in available_parts(&p) {
                let plan = plan_part(id, &p, &m, &dir);
                assert!(plan.script.contains("Spreadsheet::Sheet"), "{:?}", id);
                assert!(plan.script.contains("param_schema_version"));
                assert!(
                    !plan.script.contains("Segment_") && !plan.script.contains("\"Segment"),
                    "{:?} bölünmemeli",
                    id
                );
                assert!(!plan.script.contains("os.system"), "{:?}", id);
                assert!(!plan.script.contains("NaN") && !plan.script.contains("inf,"));
                assert!(
                    plan.stem
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                );
            }
        }
    }

    #[test]
    fn surfaces_are_hollow_and_mirrored() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let dir = std::env::temp_dir().join("werseus_out_test");
        let w = plan_part(PartId::Wing, &p, &m, &dir).script;
        assert!(w.contains("WingInnerLoft"), "kanat içi boş olmalı");
        assert!(w.contains("SparHole"));
        assert!(w.contains("WingSolid_Mirror"));
        let f = plan_part(PartId::Fuselage, &p, &m, &dir).script;
        assert!(f.contains("surf_plug") && f.contains("\\\"plugs\\\""));
        let v = plan_part(PartId::TailVertical, &p, &m, &dir).script;
        assert!(v.contains("TailInnerLoft") && !v.contains("_Mirror\""));
    }

    #[test]
    fn bom_mass_from_volume() {
        // 1 000 000 mm³ = 1 L, 1000 kg/m³ -> 1000 g
        assert!((mass_g_from_volume(1.0e6, 1000.0).unwrap() - 1000.0).abs() < 1e-6);
        assert!(mass_g_from_volume(0.0, 1000.0).is_none());
        let (p, _) = fx(DroneType::FixedWingConventional);
        let r = PartResult {
            id: PartId::Wing,
            stem: "w".into(),
            main_object: "WingSolid".into(),
            qty: 2,
            volume_mm3: 2.0e5,
            fcstd_file: "w.FCStd".into(),
        };
        let s = PartResult {
            id: PartId::WingSpar,
            volume_mm3: 1.0e4,
            ..r.clone()
        };
        let rows = build_bom(&p, &[r, s]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].qty, 2);
        let um = rows[0].unit_mass_g.unwrap();
        assert!((rows[0].total_mass_g().unwrap() - 2.0 * um).abs() < 1e-9);
        // Kiriş karbon yoğunluğuyla
        assert!((rows[1].unit_mass_g.unwrap() - 1.0e4 * 1e-6 * p.rho_cf).abs() < 1e-9);
        let csv = bom_csv(&rows);
        assert_eq!(csv.lines().count(), 3);
        let md = bom_markdown("BOM", &rows);
        assert!(md.contains("| 2 |"));
    }

    #[test]
    fn csv_cells_neutralise_formulas() {
        assert!(csv_cell("=1+1").starts_with('\''));
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        // Baştaki boşluk/sekme ile atlatma denemesi
        assert!(csv_cell(" =1+1").starts_with('\''));
        assert!(csv_cell("\t=cmd|' /C calc'!A0").starts_with("\"'"));
        assert!(csv_cell("a\rb").starts_with('"'));
        assert!(csv_cell("|x").starts_with('\''));
    }

    #[test]
    fn assembly_script_uses_links_and_json_literal() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let dir = std::env::temp_dir().join("werseus_out_test");
        let res = fake_results(&p, &m, 1000.0);
        let items = assembly_items(&p, &m, &res);
        let n: u32 = res.iter().map(|r| r.qty).sum();
        assert_eq!(items.len() as u32, n);
        // Sağ yarılar ayna nesnesine bağlanır.
        assert!(
            items
                .iter()
                .any(|i| i.label == "Wing_R" && i.obj == "WingSolid_Mirror")
        );
        let s = generate_assembly_script("Montaj", &items, &dir);
        assert!(s.contains("App::Link"));
        assert!(!s.contains("mirror("), "ayna katı yerine Link kullanılmalı");
        assert!(s.contains("Import.export"));
        assert!(s.contains("distToShape"));
        assert!(s.contains("json.loads(\""));
    }

    fn pm(a: &str, b: &str, v: f64, area: f64, d: f64) -> PairMetric {
        PairMetric {
            a: a.into(),
            b: b.into(),
            volume_mm3: v,
            area_mm2: area,
            dist_mm: d,
            ..Default::default()
        }
    }

    #[test]
    fn probe_evidence_overrides_empty_fuzzy_result() {
        // Bulanık ortak hacim 0, ama örnekleme iki parçanın da içinde 0.2 mm derin noktalar buldu.
        let hit = PairMetric {
            probe_hits: 12,
            probe_depth_mm: 0.2,
            verified_volume_mm3: 4100.0,
            ..pm("Wing_L", "Wing_R", 0.0, 0.0, 0.0)
        };
        // Çakışık yüzey: örnekleme derinliği eşik altında, tek nokta -> temas.
        let film = PairMetric {
            probe_hits: 1,
            probe_depth_mm: 0.01,
            ..pm("Fuselage", "Wing_L", 3.0, 9000.0, 0.0)
        };
        let c = classify_pairs(&[hit, film]);
        assert_eq!(c.interferences.len(), 1);
        assert!((c.interferences[0].volume_mm3 - 4100.0).abs() < 1e-9);
        assert_eq!(c.contacts.len(), 1);
    }

    #[test]
    fn unseated_mate_is_reported() {
        let item = |l: &str, par: Option<&str>| AssemblyItem {
            label: l.into(),
            file: String::new(),
            obj: String::new(),
            m: Frame::IDENTITY.matrix16(),
            density: 1.0,
            parent: par.map(String::from),
        };
        let items = vec![
            item("Fuselage", None),
            item("Wing_L", Some("Fuselage")),
            item("WingSpar_L", Some("Wing_L")),
        ];
        let c = classify_pairs(&[pm("Wing_L", "WingSpar_L", 0.0, 0.0, 0.0)]);
        assert_eq!(
            unseated_mates(&items, &c),
            vec!["Wing_L: Fuselage".to_string()]
        );
    }

    #[test]
    fn fingerprint_ignores_transferred_fields_only() {
        let (mut p, _) = fx(DroneType::FixedWingConventional);
        let f0 = design_fingerprint(&p);
        p.m_fuse_tail = 0.123;
        assert_eq!(design_fingerprint(&p), f0);
        p.b += 0.1;
        assert_ne!(design_fingerprint(&p), f0);
    }

    #[test]
    fn pairs_are_classified_contact_vs_interference() {
        let pairs = vec![
            // Yüzey yüzeye temas
            pm("A", "B", 0.0, 0.0, 0.0),
            // Çakışık yüzeylerde sayısal kalıntı: hacim > 1 mm³ ama derinlik ~0.001 mm
            pm("A", "C", 5.0, 10_000.0, 0.0),
            // Gerçek girişim: 2 mm derinlik
            pm("B", "C", 2000.0, 2000.0, 0.0),
            // Uzak
            pm("C", "D", 0.0, 0.0, 3.0),
            // Ölçüm hatası
            PairMetric {
                error: Some("x".into()),
                ..pm("D", "E", -1.0, 0.0, -1.0)
            },
        ];
        let c = classify_pairs(&pairs);
        assert_eq!(c.interferences.len(), 1);
        assert_eq!(c.interferences[0].a, "B");
        assert_eq!(c.contacts.len(), 2);
        assert_eq!(c.failed.len(), 1);
        let labels: Vec<String> = ["A", "B", "C", "D", "E"].map(String::from).to_vec();
        let fl = floating_parts(&labels, &c);
        assert_eq!(fl, vec!["D".to_string(), "E".to_string()]);
    }

    #[test]
    fn mass_transfer_is_per_component_and_gated() {
        let (mut p, m) = fx(DroneType::FixedWingConventional);
        // Gerçekçi CAD: kanat ≈ program tahmini, gövde+kuyruk ≈ alan değeri.
        let rho = p.rho_mat;
        let mut res = fake_results(&p, &m, 1.0);
        for r in &mut res {
            let target_kg = match r.id {
                PartId::Wing => m.weight.m_wing * 0.8 / 2.0,
                PartId::Fuselage => p.m_fuse_tail * 0.7,
                _ => 0.0,
            };
            if target_kg > 0.0 {
                r.volume_mm3 = target_kg / (1e-9 * rho);
            }
        }
        let mc = mass_check(&p, &m, &res);
        assert!(mc.complete);
        assert!(mc.ratio_ok(), "{}", mc.ratio());
        let before_wing = m.weight.m_wing;
        let old = apply_mass_to_project(&mut p, &mc).expect("aktarım açık olmalı");
        // Kanat kütlesi m_fuse_tail'e yazılmaz (çift sayım yok).
        assert!((p.m_fuse_tail - mc.cad_other_kg).abs() < 1e-12);
        assert!(mc.cad_other_kg < p.m_fuse_tail + mc.cad_wing_kg);
        let m2 = evaluate(&p, &Library::embedded());
        assert!((m2.weight.m_wing - before_wing).abs() < 1e-12);
        assert!(undo_mass_transfer(&mut p, mc.target, old, mc.cad_other_kg));
        assert!((p.m_fuse_tail - old).abs() < 1e-12);
        // Kullanıcı aktarımdan sonra alanı elle değiştirdiyse geri al onu ezmez.
        let old2 = apply_mass_to_project(&mut p, &mc).unwrap();
        p.m_fuse_tail = 0.777;
        assert!(!undo_mass_transfer(
            &mut p,
            mc.target,
            old2,
            mc.cad_other_kg
        ));
        assert_eq!(p.m_fuse_tail, 0.777);
        p.m_fuse_tail = old2;

        // Dolu katı benzeri 3 kat kütle: kapalı.
        let mut heavy = res.clone();
        for r in &mut heavy {
            r.volume_mm3 *= 4.0;
        }
        let mc = mass_check(&p, &m, &heavy);
        assert!(!mc.transfer_allowed());
        let before = p.m_fuse_tail;
        assert!(apply_mass_to_project(&mut p, &mc).is_none());
        assert_eq!(p.m_fuse_tail, before);

        // Eksik parça seti: kapalı.
        let mc = mass_check(&p, &m, &res[..2]);
        assert!(!mc.complete && !mc.transfer_allowed());
    }

    #[test]
    fn multirotor_mass_goes_to_frame() {
        let (mut p, m) = fx(DroneType::Multirotor);
        let rho = p.rho_mat;
        let res: Vec<PartResult> = fake_results(&p, &m, 1.0)
            .into_iter()
            .map(|mut r| {
                r.volume_mm3 = p.mr_m_frame / (r.qty as f64 * 4.0) / (1e-9 * rho);
                r
            })
            .collect();
        let mc = mass_check(&p, &m, &res);
        assert_eq!(mc.target, MassTarget::MrFrame);
        assert!((mc.ratio() - 1.0).abs() < 1e-9);
        let before_ft = p.m_fuse_tail;
        apply_mass_to_project(&mut p, &mc).unwrap();
        assert!((p.mr_m_frame - mc.cad_other_kg).abs() < 1e-12);
        assert_eq!(p.m_fuse_tail, before_ft);
    }

    // ---- İptal / zaman aşımı / ilerleme (FreeCAD yerine zararsız sahte süreç) ----

    fn recv_final(rx: &std::sync::mpsc::Receiver<RunEvent>) -> (RunEvent, usize) {
        let mut progress = 0;
        let t0 = Instant::now();
        loop {
            assert!(t0.elapsed() < Duration::from_secs(60), "olay gelmedi");
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(RunEvent::Progress(_)) => progress += 1,
                Ok(RunEvent::Log(_)) => {}
                Ok(ev) => return (ev, progress),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(e) => panic!("kanal koptu: {e}"),
            }
        }
    }

    fn temp_out(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("werseus_cad_out_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn step_limit_respects_total_budget() {
        let started = Instant::now();
        let (l, by_total) =
            step_limit(Duration::from_secs(300), started, Duration::from_secs(3600)).unwrap();
        assert_eq!(l, Duration::from_secs(300));
        assert!(!by_total);
        let (l, by_total) =
            step_limit(Duration::from_secs(300), started, Duration::from_secs(10)).unwrap();
        assert!(l <= Duration::from_secs(10) && by_total);
        assert!(step_limit(Duration::from_secs(300), started, Duration::ZERO).is_none());
    }

    #[test]
    fn partial_outputs_are_removed_but_other_files_are_kept() {
        let d = temp_out("partial");
        let old_same = d.join("Kanat.step");
        std::fs::write(&old_same, "eski").unwrap();
        let other = d.join("Kanat_Kirisi.step");
        std::fs::write(&other, "baska parca").unwrap();
        let snap = snapshot_files(expected_output_files(&d, "Kanat"));
        // Süreç yeni dosya yazar ve eski dosyayı değiştirir.
        std::thread::sleep(Duration::from_millis(60));
        let newf = d.join("Kanat.FCStd");
        std::fs::write(&newf, "yarim").unwrap();
        std::fs::write(&old_same, "yarim uzerine yazma").unwrap();
        assert_eq!(remove_new_or_changed(&snap), 2);
        assert!(!newf.exists() && !old_same.exists());
        assert!(other.exists(), "başka parçanın dosyası silinmemeli");
        // Hiçbir şey değişmediyse hiçbir şey silinmez.
        std::fs::write(&old_same, "tam").unwrap();
        let snap = snapshot_files(expected_output_files(&d, "Kanat"));
        assert_eq!(remove_new_or_changed(&snap), 0);
        assert!(old_same.exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn assembly_script_reports_fixed_stage_ids() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let d = std::env::temp_dir();
        let res = fake_results(&p, &m, 1.0);
        let items = assembly_items(&p, &m, &res);
        let script = generate_assembly_script("Montaj", &items, &d);
        for id in ["load", "pairs", "mass", "export"] {
            assert!(script.contains(&format!("stage(\"{id}\"")), "{id}");
            assert!(stage_label(id).is_some());
        }
        assert!(script.contains("__WERSEUS_STAGE__:"));
        assert!(stage_label("bilinmeyen").is_none());
    }

    #[test]
    fn pre_cancelled_run_aborts_without_touching_freecad() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let out = temp_out("precancel");
        let cancel = CancelFlag::new();
        cancel.cancel();
        let rx = run_outputs_controlled(
            std::path::PathBuf::from("werseus_no_such_freecad"),
            p,
            m,
            vec![PartId::Wing],
            out.clone(),
            CadLimits::default(),
            cancel,
        );
        let (ev, _) = recv_final(&rx);
        assert!(matches!(
            ev,
            RunEvent::Aborted {
                reason: AbortReason::Cancelled,
                completed: 0,
                removed: 0
            }
        ));
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn exhausted_total_budget_aborts() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let out = temp_out("total0");
        let rx = run_outputs_controlled(
            std::path::PathBuf::from("werseus_no_such_freecad"),
            p,
            m,
            vec![PartId::Wing],
            out.clone(),
            CadLimits {
                part_secs: 300,
                assembly_secs: 900,
                total_secs: 0,
            },
            CancelFlag::new(),
        );
        let (ev, _) = recv_final(&rx);
        assert!(matches!(
            ev,
            RunEvent::Aborted {
                reason: AbortReason::TotalTimeout,
                ..
            }
        ));
        let _ = std::fs::remove_dir_all(&out);
    }

    /// Sahte "FreeCADCmd": 30 sn bekleyen bir toplu iş dosyası (yalnız Windows).
    #[cfg(windows)]
    fn fake_freecad(tag: &str) -> std::path::PathBuf {
        let d = temp_out(tag);
        let f = d.join("fake_freecadcmd.cmd");
        std::fs::write(&f, "@echo off\r\nping -n 30 127.0.0.1 >nul\r\n").unwrap();
        f
    }

    #[cfg(windows)]
    #[test]
    fn fake_process_part_timeout_marks_part_failed_and_finishes() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let fake = fake_freecad("fake_timeout");
        let out = temp_out("fake_timeout_out");
        let t0 = Instant::now();
        let rx = run_outputs_controlled(
            fake.clone(),
            p,
            m,
            vec![PartId::Wing],
            out.clone(),
            CadLimits {
                part_secs: 1,
                assembly_secs: 1,
                total_secs: 600,
            },
            CancelFlag::new(),
        );
        let (ev, progress) = recv_final(&rx);
        match ev {
            RunEvent::Done(sum) => {
                assert!(sum.parts.is_empty());
                assert_eq!(sum.failed.len(), 1, "{:?}", sum.failed);
                assert!(sum.failed[0].starts_with("wing: "));
            }
            other => panic!("{other:?}"),
        }
        assert!(progress >= 1, "ilerleme iletisi gelmeli");
        assert!(
            t0.elapsed() < Duration::from_secs(25),
            "süreç sonlandırılmalı"
        );
        let _ = std::fs::remove_dir_all(fake.parent().unwrap());
        let _ = std::fs::remove_dir_all(&out);
    }

    #[cfg(windows)]
    #[test]
    fn fake_process_cancel_stops_run_and_reports_not_completed() {
        let (p, m) = fx(DroneType::FixedWingConventional);
        let fake = fake_freecad("fake_cancel");
        let out = temp_out("fake_cancel_out");
        let cancel = CancelFlag::new();
        let c2 = cancel.clone();
        let t0 = Instant::now();
        let rx = run_outputs_controlled(
            fake.clone(),
            p,
            m,
            vec![PartId::Wing, PartId::Fuselage],
            out.clone(),
            CadLimits::default(),
            cancel,
        );
        std::thread::sleep(Duration::from_millis(1200));
        c2.cancel();
        let (ev, _) = recv_final(&rx);
        assert!(
            matches!(
                ev,
                RunEvent::Aborted {
                    reason: AbortReason::Cancelled,
                    completed: 0,
                    ..
                }
            ),
            "{ev:?}"
        );
        assert!(t0.elapsed() < Duration::from_secs(25));
        let _ = std::fs::remove_dir_all(fake.parent().unwrap());
        let _ = std::fs::remove_dir_all(&out);
    }
}
