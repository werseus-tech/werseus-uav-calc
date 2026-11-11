// SPDX-License-Identifier: AGPL-3.0-or-later
//! FreeCAD 1.1 uyumlu parametrik Python betiği üreticileri.
//! Tüm fonksiyonlar saf Rust string'i üretir ve bağımsız olarak birim testlerine tabi tutulabilir.

use super::geom::{Frame, FuseProfile, Profile, SparAxis, SurfaceGeom};
use super::structure::{
    FuselageStructure, INNER_SEGMENTS_PER_SIDE, SECTION_SEGMENTS_PER_SIDE, SurfaceStructure,
    TAIL_RIB_PITCH_MM, WING_RIB_PITCH_MM,
};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::MultirotorLayout;
use crate::project::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Dışa aktarma seçenekleri.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportOptions {
    pub export_fcstd: bool,
    pub export_step: bool,
    pub export_stl: bool,
    pub out_dir: PathBuf,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: PathBuf::from("cad_output"),
        }
    }
}

/// Kanat parametreleri (tüm uzunluklar mm cinsindedir). Yerel çerçeve: X kord, Y kalınlık,
/// Z açıklık (kökten uca); kök hücum kenarı orijin. Betik sol yarıyı ve parametrik ayna
/// kopyasını (sağ yarı) üretir.
#[derive(Clone, Debug)]
pub struct WingCadParams {
    pub name: String,
    pub root_chord_mm: f64,
    pub tip_chord_mm: f64,
    pub half_span_mm: f64,
    pub sweep_le_deg: f64,
    pub dihedral_deg: f64,
    pub twist_deg: f64,
    pub spar_od_mm: f64,
    pub spar_id_mm: f64,
    pub spar_x_frac: f64,
    /// Kabuk et kalınlığı; 0 ise dolu katı.
    pub wall_mm: f64,
    pub root_profile: Vec<[f64; 2]>,
    pub tip_profile: Vec<[f64; 2]>,
}

impl WingCadParams {
    pub fn from_project(p: &Project, m: &Model) -> Self {
        let half_span_mm = (p.b / 2.0) * 1000.0;
        let root_chord_mm = m.wing.c_r * 1000.0;
        let tip_chord_mm = m.wing.c_t * 1000.0;
        let spar_od_mm = p.d_spar_o * 1000.0;

        let root_profile = p.airfoil.points.clone();
        let tip_profile = p.airfoil.points.clone();

        Self {
            name: format!("{}_Kanat", sanitize_ident(&p.name)),
            root_chord_mm,
            tip_chord_mm,
            half_span_mm,
            sweep_le_deg: p.sweep_le_deg,
            dihedral_deg: 0.0,
            twist_deg: 0.0,
            spar_od_mm: spar_od_mm.max(4.0),
            spar_id_mm: (p.d_spar_i * 1000.0).max(0.0),
            spar_x_frac: 0.25,
            wall_mm: wall_mm_from(p),
            root_profile,
            tip_profile,
        }
    }

    /// Geometri (profil geçersizse simetrik NACA 0009). Profil, CAD kesitleriyle aynı az
    /// noktalı örneklemedir (yuvalar ve kelepçe semeri de bu noktaları kullanır).
    pub fn geom(&self) -> SurfaceGeom {
        SurfaceGeom {
            profile: profile_or_default(&self.root_profile).resampled(SECTION_SEGMENTS_PER_SIDE),
            c_root: self.root_chord_mm,
            c_tip: self.tip_chord_mm,
            span: self.half_span_mm,
            sweep_le_deg: self.sweep_le_deg,
        }
    }

    /// Kiriş ekseni (iç boşluğa sığmıyorsa `None`).
    pub fn spar(&self) -> Option<SparAxis> {
        self.geom().spar_axis(
            self.spar_x_frac,
            self.spar_od_mm,
            self.spar_id_mm,
            self.wall_mm,
        )
    }

    /// İç yapı (kabuk, kaburgalar, kiriş kanalı) — tek kaynak: `cad::structure`.
    pub fn structure(&self) -> SurfaceStructure {
        SurfaceStructure::new(
            self.half_span_mm,
            self.wall_mm,
            self.spar(),
            WING_RIB_PITCH_MM,
        )
    }
}

/// Projedeki kabuk et kalınlığı [mm] (0.3 mm altı kabul edilmez).
pub fn wall_mm_from(p: &Project) -> f64 {
    let w = p.t_wall * 1000.0;
    if w.is_finite() && w > 0.0 {
        w.max(0.3)
    } else {
        0.0
    }
}

/// Geçerli profil ya da simetrik NACA 0009 yedeği.
pub fn profile_or_default(points: &[[f64; 2]]) -> Profile {
    Profile::new(points).unwrap_or_else(|| {
        let sym = crate::cad::airfoil::generate_naca4_points("0009", 80, true).unwrap_or_default();
        Profile::new(&sym).expect("NACA 0009 geçerli")
    })
}

/// Kuyruk parametreleri (tüm uzunluklar mm cinsindedir).
#[derive(Clone, Debug)]
pub struct TailCadParams {
    pub name: String,
    pub tail_kind: TailKind,
    pub root_chord_mm: f64,
    pub tip_chord_mm: f64,
    pub span_mm: f64,
    pub sweep_le_deg: f64,
    pub angle_deg: f64,
    pub spar_od_mm: f64,
    pub spar_x_frac: f64,
    /// Kabuk et kalınlığı; 0 ise dolu katı.
    pub wall_mm: f64,
    /// Sol/sağ çift mi (sağ yarı parametrik ayna kopyası olarak üretilir)?
    pub mirrored_pair: bool,
    pub profile: Vec<[f64; 2]>,
}

impl TailCadParams {
    pub fn from_project(p: &Project, _m: &Model, kind: TailKind) -> Self {
        // NACA 0009 simetrik varsayılan
        let sym_pts =
            crate::cad::airfoil::generate_naca4_points("0009", 80, true).unwrap_or_default();
        let wall = wall_mm_from(p);

        match kind {
            TailKind::Conventional => {
                // Yatay kuyruk için
                let root_chord_mm = p.c_tail * 1000.0;
                let span_mm = if p.s_h > 0.0 && p.c_tail > 0.0 {
                    (p.s_h / p.c_tail) * 1000.0
                } else {
                    400.0
                };
                let profile = if !p.tail_airfoil_h.points.is_empty() {
                    p.tail_airfoil_h.points.clone()
                } else {
                    sym_pts
                };
                Self {
                    name: format!("{}_Yatay_Kuyruk", sanitize_ident(&p.name)),
                    tail_kind: kind,
                    root_chord_mm,
                    tip_chord_mm: root_chord_mm * 0.7,
                    span_mm: span_mm / 2.0, // Yarı açıklık
                    sweep_le_deg: 5.0,
                    angle_deg: 0.0,
                    spar_od_mm: 0.0,
                    spar_x_frac: 0.30,
                    wall_mm: wall,
                    mirrored_pair: true,
                    profile,
                }
            }
            TailKind::VTail => {
                let root_chord_mm = p.c_tail * 1000.0;
                let span_mm = if p.s_vtail > 0.0 && p.c_tail > 0.0 {
                    (p.s_vtail / p.c_tail) * 500.0
                } else {
                    300.0
                };
                let profile = if !p.tail_airfoil_v.points.is_empty() {
                    p.tail_airfoil_v.points.clone()
                } else if !p.tail_airfoil_h.points.is_empty() {
                    p.tail_airfoil_h.points.clone()
                } else {
                    sym_pts
                };
                Self {
                    name: format!("{}_V_Kuyruk", sanitize_ident(&p.name)),
                    tail_kind: kind,
                    root_chord_mm,
                    tip_chord_mm: root_chord_mm * 0.7,
                    span_mm,
                    sweep_le_deg: 10.0,
                    angle_deg: p.gamma_v_deg,
                    spar_od_mm: 0.0,
                    spar_x_frac: 0.30,
                    wall_mm: wall,
                    mirrored_pair: true,
                    profile,
                }
            }
            TailKind::FlyingWing => {
                // Dikey kanatçık (winglet): kökü kanat ucu kesitine oturur, kök kordu kanat uç
                // kordu kadardır. Profil: `tail_airfoil_v`, boşsa ana kanat profili, o da
                // yoksa simetrik NACA 0009.
                let wing = WingCadParams::from_project(p, _m);
                let root_chord_mm = wing.tip_chord_mm.max(40.0);
                let profile = if !p.tail_airfoil_v.points.is_empty() {
                    p.tail_airfoil_v.points.clone()
                } else if !p.airfoil.points.is_empty() {
                    p.airfoil.points.clone()
                } else {
                    sym_pts
                };
                Self {
                    name: format!("{}_Winglet", sanitize_ident(&p.name)),
                    tail_kind: kind,
                    root_chord_mm,
                    tip_chord_mm: root_chord_mm * 0.6,
                    span_mm: (p.b * 1000.0 * 0.12).clamp(60.0, 300.0),
                    sweep_le_deg: 25.0,
                    angle_deg: 90.0,
                    spar_od_mm: 0.0,
                    spar_x_frac: 0.30,
                    wall_mm: wall,
                    mirrored_pair: true,
                    profile,
                }
            }
        }
    }

    /// Konvansiyonel dikey kuyruk parametrelerini projeden üretir.
    /// Dikey kuyruk profili olarak `p.tail_airfoil_v` kullanılır (tanımsızsa simetrik NACA 0009).
    pub fn from_project_vertical(p: &Project, _m: &Model) -> Self {
        let sym_pts =
            crate::cad::airfoil::generate_naca4_points("0009", 80, true).unwrap_or_default();
        let root_chord_mm = p.c_tail * 1000.0;
        let span_mm = if p.s_v > 0.0 && p.c_tail > 0.0 {
            (p.s_v / p.c_tail) * 1000.0
        } else {
            250.0
        };
        let profile = if !p.tail_airfoil_v.points.is_empty() {
            p.tail_airfoil_v.points.clone()
        } else {
            sym_pts
        };
        Self {
            name: format!("{}_Dikey_Kuyruk", sanitize_ident(&p.name)),
            tail_kind: TailKind::Conventional,
            root_chord_mm,
            tip_chord_mm: root_chord_mm * 0.7,
            span_mm,
            // Tek dikey yüzey: yükseklik = S_v / veter. Dikey kuyruklarda tipik ön kenar oku 15-30°;
            // orta değer 20° alındı (yatay kuyruk 5°, V-kuyruk 10°).
            sweep_le_deg: 20.0,
            angle_deg: 90.0,
            spar_od_mm: 0.0,
            spar_x_frac: 0.30,
            wall_mm: wall_mm_from(p),
            mirrored_pair: false,
            profile,
        }
    }

    pub fn geom(&self) -> SurfaceGeom {
        SurfaceGeom {
            profile: profile_or_default(&self.profile).resampled(SECTION_SEGMENTS_PER_SIDE),
            c_root: self.root_chord_mm,
            c_tip: self.tip_chord_mm,
            span: self.span_mm,
            sweep_le_deg: self.sweep_le_deg,
        }
    }

    /// İç yapı (kabuk ve kaburgalar; kuyrukta kiriş yok) — tek kaynak: `cad::structure`.
    pub fn structure(&self) -> SurfaceStructure {
        SurfaceStructure::new(self.span_mm, self.wall_mm, None, TAIL_RIB_PITCH_MM)
    }
}

/// Bir taşıyıcı yüzeyin başka bir parçayı kesen "yuva tıkacı": yüzeyin [z0, z1] açıklık
/// aralığındaki birebir parçası (aynı kesit formülü), `m` ile ev sahibi parçanın yerel
/// koordinatlarına taşınır. z0, z1 negatifse ayna (sağ) tarafın parçasıdır.
#[derive(Clone, Debug, Serialize)]
pub struct PlugSpec {
    pub pts: Vec<[f64; 2]>,
    pub c_r: f64,
    pub c_t: f64,
    pub span: f64,
    pub sweep: f64,
    pub z0: f64,
    pub z1: f64,
    pub m: [f64; 16],
}

impl PlugSpec {
    pub fn new(g: &SurfaceGeom, z0: f64, z1: f64, host_from_surface: &Frame) -> Self {
        Self {
            pts: g.profile.pts.clone(),
            c_r: g.c_root,
            c_t: g.c_tip,
            span: g.span,
            sweep: g.sweep_le_deg,
            z0,
            z1,
            m: host_from_surface.matrix16(),
        }
    }

    pub fn is_finite(&self) -> bool {
        self.pts.iter().flatten().all(|v| v.is_finite())
            && [self.c_r, self.c_t, self.span, self.sweep, self.z0, self.z1]
                .iter()
                .all(|v| v.is_finite())
            && self.m.iter().all(|v| v.is_finite())
    }
}

/// İniş takımı oturma yüzeyi: gövde altında düz bölge (x0..x1 arası, z = `z` düzlemi).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct GearPad {
    pub x0: f64,
    pub x1: f64,
    pub z: f64,
}

/// Gövde parametreleri (tüm uzunluklar mm cinsindedir). Yerel çerçeve montaj çerçevesiyle
/// aynıdır: X kuyruğa doğru, Y sağa, Z yukarı; burun yüzü (firewall) x=0'da.
#[derive(Clone, Debug)]
pub struct FuselageCadParams {
    pub name: String,
    pub length_mm: f64,
    pub max_diameter_mm: f64,
    pub nose_r_mm: f64,
    pub tail_r_mm: f64,
    pub nose_ratio: f64,
    pub tail_ratio: f64,
    /// Kabuk et kalınlığı; 0 ise dolu katı.
    pub wall_mm: f64,
    /// Kanat/kuyruk yuvaları (gövdeden çıkarılan yüzey parçaları).
    pub plugs: Vec<PlugSpec>,
    pub pad: Option<GearPad>,
}

impl FuselageCadParams {
    /// Projenin tam montaj yerleşimine göre (yuvalar, iniş takımı yüzeyi) gövde.
    pub fn from_project(p: &Project, m: &Model) -> Self {
        crate::cad::layout::Layout::new(p, m).fuselage
    }

    pub fn profile(&self) -> FuseProfile {
        FuseProfile::new(
            self.length_mm,
            self.max_diameter_mm / 2.0,
            self.nose_r_mm,
            self.tail_r_mm,
            self.nose_ratio,
            self.tail_ratio,
        )
    }

    /// İç yapı (kabuk ve halka postalar) — tek kaynak: `cad::structure`.
    pub fn structure(&self) -> FuselageStructure {
        FuselageStructure::new(&self.profile(), self.max_diameter_mm / 2.0, self.wall_mm)
    }
}

/// Multirotor kol parametreleri (tüm uzunluklar mm cinsindedir). Yerel çerçeve: kol kökü
/// orijinde, X kol boyunca, kalınlık +Z; motor tablası merkezi x = arm_length - width/2.
#[derive(Clone, Debug)]
pub struct MultirotorArmCadParams {
    pub name: String,
    pub arm_length_mm: f64,
    pub width_mm: f64,
    pub thickness_mm: f64,
    pub motor_mount_pattern_mm: f64, // 16, 19 veya 25 mm
    pub motor_bolt_dia_mm: f64,      // 3.0 (M3) veya 2.0 (M2)
    pub motor_shaft_dia_mm: f64,     // 6.0 mm
    pub chassis_bolt_dia_mm: f64,    // 3.0 mm
    /// İlk şasi deliğinin kol kökünden uzaklığı.
    pub chassis_hole_x0_mm: f64,
    pub chassis_bolt_spacing_mm: f64,
    pub cable_channel_w_mm: f64,
}

impl MultirotorArmCadParams {
    pub fn from_project(p: &Project, m: &Model) -> Self {
        crate::cad::layout::Layout::new(p, m).mr_arm
    }
}

/// Multirotor merkez plaka parametreleri (tüm uzunluklar mm cinsindedir).
#[derive(Clone, Debug)]
pub struct MultirotorPlateCadParams {
    pub name: String,
    pub layout: MultirotorLayout,
    pub arm_count: u32,
    pub plate_radius_mm: f64,
    pub thickness_mm: f64,
    pub arm_width_mm: f64,
    pub chassis_bolt_dia_mm: f64,
    pub fc_mount_pattern_mm: f64, // 30.5 veya 20.0 mm
    pub center_hole_dia_mm: f64,
    /// Kol bağlantı deliklerinin yarıçapları (kolun şasi delikleriyle aynı).
    pub arm_hole_r_mm: [f64; 2],
}

impl MultirotorPlateCadParams {
    pub fn from_project(p: &Project, m: &Model) -> Self {
        crate::cad::layout::Layout::new(p, m).mr_plate
    }
}

/// Multirotor kol açıları [derece] (X, +, H; plaka delikleri ve montaj aynı listeyi kullanır).
pub fn mr_arm_angles(layout: MultirotorLayout, n: u32) -> Vec<f64> {
    let n = n.max(1);
    let even = |offset: f64| -> Vec<f64> {
        (0..n)
            .map(|i| offset + (i as f64) * 360.0 / (n as f64))
            .collect()
    };
    match (layout, n) {
        (MultirotorLayout::Plus, _) => even(0.0),
        (MultirotorLayout::H, 4) => vec![35.0, 145.0, 215.0, 325.0],
        (MultirotorLayout::H, _) | (MultirotorLayout::X, _) => even(180.0 / (n as f64)),
    }
}

/// Sabit kanat motor yuvası parametreleri (tüm uzunluklar mm cinsindedir). Yerel çerçeve:
/// plaka merkezi orijinde, arka yüz z=0, kalınlık +Z.
#[derive(Clone, Debug)]
pub struct MotorMountCadParams {
    pub name: String,
    pub mount_type: String, // "tractor" veya "pusher"
    pub plate_size_mm: f64,
    pub thickness_mm: f64,
    pub motor_bolt_pattern_mm: f64, // 16x19 veya 19x25 vb.
    pub motor_bolt_dia_mm: f64,
    pub shaft_hole_dia_mm: f64,
    pub firewall_bolt_spacing_mm: f64,
    pub firewall_bolt_dia_mm: f64,
}

impl MotorMountCadParams {
    pub fn from_project(p: &Project, _m: &Model) -> Self {
        let mount_type = if p.drone_type == crate::project::DroneType::FlyingWing {
            "pusher".into()
        } else {
            "tractor".into()
        };
        Self {
            name: format!("{}_Motor_Yuvasi", sanitize_ident(&p.name)),
            mount_type,
            plate_size_mm: 50.0,
            thickness_mm: 4.0,
            motor_bolt_pattern_mm: 19.0,
            motor_bolt_dia_mm: 3.0,
            shaft_hole_dia_mm: 8.0,
            firewall_bolt_spacing_mm: 40.0,
            firewall_bolt_dia_mm: 3.5,
        }
    }

    /// Firewall yüzünün cıvatalarını taşıması için gereken en küçük yarıçap.
    pub fn firewall_radius_mm(&self) -> f64 {
        // Delikler köşelerde (±s/2, ±s/2): merkezden uzaklık s/√2, artı delik yarıçapı ve kenar payı.
        self.firewall_bolt_spacing_mm * std::f64::consts::FRAC_1_SQRT_2
            + self.firewall_bolt_dia_mm / 2.0
            + 2.0
    }
}

pub fn sanitize_filename(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            'ı' => s.push('i'),
            'İ' => s.push('I'),
            'ş' => s.push('s'),
            'Ş' => s.push('S'),
            'ğ' => s.push('g'),
            'Ğ' => s.push('G'),
            'ü' => s.push('u'),
            'Ü' => s.push('U'),
            'ö' => s.push('o'),
            'Ö' => s.push('O'),
            'ç' => s.push('c'),
            'Ç' => s.push('C'),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => s.push(c),
            _ => s.push('_'),
        }
    }
    if s.is_empty() {
        s.push_str("unnamed");
    }
    s
}

pub fn sanitize_ident(name: &str) -> String {
    if name.trim().is_empty() {
        return "part".to_string();
    }
    let transliterated = sanitize_filename(name);
    let mut s: String = transliterated
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        s.push_str("part");
    } else if s
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        s.insert(0, '_');
    }
    s
}

pub fn py_str_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Üçgenleştirmeden hacim, alan ve ağırlık merkezi. OCC'nin `Shape.Volume` integrali çok
/// düğümlü B-spline loft yüzeylerinde belirgin hata verebildiği (kanat loftunda ~%20) için
/// kütle ve hacim kontrolleri bu fonksiyonla yapılır.
pub(crate) const PY_MESH_PROPS: &str = r#"
def mesh_props(shape, defl=0.05):
    """(hacim mm3, alan mm2, [cx, cy, cz]) - ucgenlestirilmis kopyadan."""
    import Mesh
    me = Mesh.Mesh(shape.tessellate(defl))
    c = me.CenterOfGravity
    return abs(float(me.Volume)), float(me.Area), [float(c.x), float(c.y), float(c.z)]
"#;

pub(crate) fn common_python_footer(
    part_name: &str,
    out_dir: &Path,
    export_options: &ExportOptions,
    test_param: &str,
    test_val_str: &str,
    segment_obj_names: &[String],
    main_obj_name: &str,
) -> String {
    let clean_part_name = sanitize_ident(part_name);
    let out_dir_py = py_str_literal(&out_dir.to_string_lossy().replace('\\', "/"));
    let fcstd_name_py = py_str_literal(&format!("{clean_part_name}.FCStd"));
    let step_name_py = py_str_literal(&format!("{clean_part_name}.step"));
    let stl_name_py = py_str_literal(&format!("{clean_part_name}.stl"));
    let part_name_py = py_str_literal(&clean_part_name);
    let test_param_py = py_str_literal(test_param);
    let test_val_py = py_str_literal(test_val_str);
    let main_obj_clean = sanitize_ident(main_obj_name);

    let export_step_bool = if export_options.export_step {
        "True"
    } else {
        "False"
    };
    let export_stl_bool = if export_options.export_stl {
        "True"
    } else {
        "False"
    };
    let export_fcstd_bool = if export_options.export_fcstd {
        "True"
    } else {
        "False"
    };

    let seg_names_py = serde_json::to_string(
        &segment_obj_names
            .iter()
            .map(|s| sanitize_ident(s))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".into());

    format!(
        r#"
# ==========================================
# DOGRULAMA, PARAMETRIK GUNCELLEME VE DISA AKTARIM
# ==========================================
import json, os, sys, traceback
{mesh_props}
try:
    out_dir = {out_dir_py}
    os.makedirs(out_dir, exist_ok=True)

    def _broken():
        # Yeniden hesaplamada hata veren ozellikler (bozuk agac): sessizce eski sekli tutarlar.
        return [o.Name for o in doc.Objects if ("Invalid" in o.State or "Error" in o.State)]
    bad = _broken()
    if bad:
        raise RuntimeError("Hatali ozellik: " + ", ".join(bad))

    main_obj = doc.getObject("{main_obj_clean}")
    is_valid = main_obj.Shape.isValid() if main_obj else False
    vol_mm3 = mesh_props(main_obj.Shape)[0] if main_obj and is_valid else 0.0

    # Parametrik guncelleme testi: bir parametreyi degistir, recompute et, hacim degisimini kontrol et
    param_update_ok = False
    orig_val = None
    test_param = {test_param_py}
    try:
        test_val = {test_val_py}
        orig_val = sheet.getContents(test_param)
        sheet.set(test_param, test_val)
        doc.recompute()
        updated_obj = doc.getObject("{main_obj_clean}")
        updated_vol = mesh_props(updated_obj.Shape)[0] if updated_obj else 0.0
        if abs(updated_vol - vol_mm3) > 0.01 and not _broken() and updated_obj.Shape.isValid():
            param_update_ok = True
    except Exception as e:
        print(f"Parametric update test warning: {{e}}")
    # Ozgun degere geri don: dosyaya ve STEP'e test degeri degil ozgun geometri yazilmali.
    if orig_val is not None:
        sheet.set(test_param, orig_val)
        doc.recompute()
        restored = doc.getObject("{main_obj_clean}")
        restored_vol = mesh_props(restored.Shape)[0] if restored else 0.0
        if abs(restored_vol - vol_mm3) > 1e-3 * max(1.0, abs(vol_mm3)) or _broken():
            raise RuntimeError("Parametre ozgun degerine geri yuklenemedi")

    # Cikti nesneleri listesi
    seg_names = {seg_names_py}
    seg_objs = [doc.getObject(name) for name in seg_names if doc.getObject(name)]
    if not seg_objs and main_obj:
        seg_objs = [main_obj]

    # Surumlu parametre semasi (Spreadsheet): sonraki surumlerde okunabilirlik icin
    try:
        sheet.set("A30", "param_schema_version"); sheet.set("B30", "1"); sheet.setAlias("B30", "param_schema_version")
        sheet.set("A31", "generator"); sheet.set("B31", "WerSeuS UAV Calc"); sheet.setAlias("B31", "generator")
        doc.recompute()
    except Exception as e:
        print(f"Schema row warning: {{e}}")

    fcstd_path = os.path.join(out_dir, {fcstd_name_py})
    if {export_fcstd_bool}:
        doc.saveAs(fcstd_path)

    step_files = []
    if {export_step_bool}:
        import Part
        for i, obj in enumerate(seg_objs):
            step_p = os.path.join(out_dir, {step_name_py} if len(seg_objs) == 1 else f"{{obj.Name}}.step")
            Part.export([obj], step_p)
            step_files.append(step_p)

    stl_files = []
    if {export_stl_bool}:
        import MeshPart, Mesh
        for i, obj in enumerate(seg_objs):
            stl_p = os.path.join(out_dir, {stl_name_py} if len(seg_objs) == 1 else f"{{obj.Name}}.stl")
            mesh = MeshPart.meshFromShape(Shape=obj.Shape, MaxLength=5.0)
            mesh.write(stl_p)
            stl_files.append(stl_p)

    result = {{
        "ok": bool(is_valid and vol_mm3 > 0.0),
        "status": "ok" if is_valid and vol_mm3 > 0.0 else "error",
        "part": {part_name_py},
        "valid": bool(is_valid),
        "volume_mm3": vol_mm3,
        "param_update_ok": bool(param_update_ok),
        "num_segments": len(seg_objs),
        "fcstd_path": fcstd_path,
        "step_files": step_files,
        "stl_files": stl_files,
        "error": None if (is_valid and vol_mm3 > 0.0) else "Geometrik katı geçersiz (invalid shape) veya hacim sıfır",
        "traceback": None,
    }}
    print("__WERSEUS_CAD_JSON_START__" + json.dumps(result) + "__WERSEUS_CAD_JSON_END__")
except Exception as e:
    tb = traceback.format_exc()
    err_res = {{
        "ok": False,
        "status": "error",
        "part": {part_name_py},
        "valid": False,
        "volume_mm3": 0.0,
        "param_update_ok": False,
        "num_segments": 0,
        "fcstd_path": "",
        "step_files": [],
        "stl_files": [],
        "error": str(e),
        "traceback": tb,
    }}
    print("__WERSEUS_CAD_JSON_START__" + json.dumps(err_res) + "__WERSEUS_CAD_JSON_END__")
"#,
        mesh_props = PY_MESH_PROPS,
    )
}
/// Yüzey parçası betiklerinde ortak Python yardımcıları: taşıyıcı yüzeyin [z0, z1]
/// aralığındaki birebir parçasını ("yuva tıkacı") ev sahibi parçanın koordinatlarında üretir.
/// Kesit, kanat/kuyruk çizimleriyle aynıdır: profil noktalarından geçen tek B-spline (açık
/// firarda ayrıca firar çizgisi); doğrusal kord ve hücum kenarı, düz (ruled) loft.
pub(crate) const PY_SURF_HELPERS: &str = r#"
def _surf_section(sp, z):
    s = abs(z)
    c = sp["c_r"] + (sp["c_t"] - sp["c_r"]) * s / sp["span"]
    x0 = s * math.tan(math.radians(sp["sweep"]))
    mtx = FreeCAD.Matrix(*sp["m"])
    vs = [mtx.multVec(FreeCAD.Vector(x0 + q[0] * c, q[1] * c, z)) for q in sp["pts"]]
    bs = Part.BSplineCurve()
    bs.interpolate(vs)
    edges = [bs.toShape()]
    if (vs[0] - vs[-1]).Length > 1e-9:
        edges.append(Part.LineSegment(vs[-1], vs[0]).toShape())
    return Part.Wire(edges)

def surf_plug(sp):
    return Part.makeLoft([_surf_section(sp, sp["z0"]), _surf_section(sp, sp["z1"])], True, True)
"#;

/// PartDesign yardımcıları (FreeCAD Python): tam kısıtlı çizimler. Her köşe/kutup (pole)
/// konumu `DistanceX`/`DistanceY` kısıtıyla verilir ve kısıt değeri Spreadsheet ifadesine
/// bağlanır; böylece GUI'de bir hücre değiştirilip yeniden hesaplandığında çizim, loft ve
/// boşaltmalar birlikte güncellenir. İfadeler yalnızca hücre takma adları ve sayılardan oluşur.
pub(crate) const PY_PD_HELPERS: &str = r#"
from Sketcher import Constraint as _C
_V = FreeCAD.Vector

def _origin(body, role):
    for o in body.Origin.OriginFeatures:
        if o.Role == role:
            return o
    raise RuntimeError("Govde orijin duzlemi yok: " + role)

def _attach(sk, body, plane, place):
    sk.AttachmentSupport = [(_origin(body, plane), "")]
    sk.MapMode = "FlatFace"
    for prop, e in place:
        sk.setExpression(".AttachmentOffset." + prop, e)

def _bind(sk, exprs):
    for i, e in exprs:
        sk.setExpression("Constraints[%d]" % i, e)

def pole_sketch(body, name, poles, knots, mults, te_line, place):
    """Tek B-spline kesit; kutuplar [x0, y0, ifade_x, ifade_y]. Kutup daireleri yardimci
    geometri (agirlik 1), firar acik ise kapatan cizgi."""
    sk = body.newObject("Sketcher::SketchObject", name)
    _attach(sk, body, "XY_Plane", place)
    bs = Part.BSplineCurve()
    bs.buildFromPolesMultsKnots([_V(p[0], p[1], 0) for p in poles], mults, knots, False, 3)
    sk.addGeometry([bs], False)
    sk.addGeometry([Part.Circle(_V(p[0], p[1], 0), _V(0, 0, 1), 1.0) for p in poles], True)
    cons = []
    ex = []
    for j, p in enumerate(poles):
        g = 1 + j
        cons.append(_C("InternalAlignment:Sketcher::BSplineControlPoint", g, 3, 0, j))
        cons.append(_C("Weight", g, 1.0))
        cons.append(_C("DistanceX", g, 3, p[0]))
        ex.append((len(cons) - 1, p[2]))
        cons.append(_C("DistanceY", g, 3, p[1]))
        ex.append((len(cons) - 1, p[3]))
    if te_line:
        n = len(poles)
        sk.addGeometry([Part.LineSegment(_V(poles[-1][0], poles[-1][1], 0), _V(poles[0][0], poles[0][1], 0))], False)
        cons.append(_C("Coincident", 0, 2, n + 1, 1))
        cons.append(_C("Coincident", n + 1, 2, 0, 1))
    sk.addConstraint(cons)
    _bind(sk, ex)
    return sk

def airfoil_sketch(body, name, pts, chord0, chord_e, place):
    """Profil kesiti: normalize noktalardan gecen B-spline. Interpolasyon olcekten bagimsiz
    oldugu icin kutuplar kord ifadesiyle carpilarak tam olarak olceklenir."""
    te_open = (pts[0][0] - pts[-1][0]) ** 2 + (pts[0][1] - pts[-1][1]) ** 2 > 1e-18
    b = Part.BSplineCurve()
    b.interpolate([_V(q[0], q[1], 0) for q in pts])
    poles = [[p.x * chord0, p.y * chord0, "%s * %.12f" % (chord_e, p.x), "%s * %.12f" % (chord_e, p.y)] for p in b.getPoles()]
    return pole_sketch(body, name, poles, b.getKnots(), b.getMultiplicities(), te_open, place)

def poly_sketch(body, name, plane, loops):
    """Kapali cokgenler; her kose [x0, y0, ifade_x, ifade_y]."""
    sk = body.newObject("Sketcher::SketchObject", name)
    _attach(sk, body, plane, [])
    cons = []
    ex = []
    geo = []
    for loop in loops:
        g0 = len(geo)
        n = len(loop)
        for i in range(n):
            a = loop[i]
            b = loop[(i + 1) % n]
            geo.append(Part.LineSegment(_V(a[0], a[1], 0), _V(b[0], b[1], 0)))
            cons.append(_C("Coincident", g0 + i, 2, g0 + (i + 1) % n, 1))
            cons.append(_C("DistanceX", g0 + i, 1, a[0]))
            ex.append((len(cons) - 1, a[2]))
            cons.append(_C("DistanceY", g0 + i, 1, a[1]))
            ex.append((len(cons) - 1, a[3]))
    sk.addGeometry(geo, False)
    sk.addConstraint(cons)
    _bind(sk, ex)
    return sk

def check_sketches(doc):
    """Tum cizimler tam kisitli ve cozulmus olmali."""
    bad = []
    for o in doc.Objects:
        if o.TypeId == "Sketcher::SketchObject":
            if o.solve() != 0 or not o.FullyConstrained:
                bad.append(o.Name)
    if bad:
        raise RuntimeError("Tam kisitli olmayan cizim: " + ", ".join(bad))
"#;

/// Spreadsheet hücre içeriği (birimli). Negatif değerler formül olarak yazılır (`=-3 deg`):
/// FreeCAD, başında eksi olan birimli düz metni sayı yerine metin olarak saklıyor.
pub(crate) fn qty(v: f64, unit: &str) -> String {
    if v < 0.0 {
        format!("={v:.4} {unit}")
    } else {
        format!("{v:.2} {unit}")
    }
}

/// Spreadsheet satırları: A sütunu ad, B sütunu değer (takma adlı), C sütunu not.
fn sheet_lines(rows: &[(String, String, &str)]) -> String {
    rows.iter()
        .enumerate()
        .map(|(i, (alias, val, note))| {
            let a = sanitize_ident(alias);
            let mut l = format!(
                "sheet.set(\"A{r}\", \"{a}\"); sheet.set(\"B{r}\", {v}); sheet.setAlias(\"B{r}\", \"{a}\")\n",
                r = i + 1,
                v = py_str_literal(val),
            );
            if !note.is_empty() {
                l.push_str(&format!(
                    "sheet.set(\"C{r}\", {n})\n",
                    r = i + 1,
                    n = py_str_literal(note)
                ));
            }
            l
        })
        .collect()
}

/// `+ 3.2 mm` / `- 3.2 mm` biçiminde ifade eki.
fn mm_offset(v: f64) -> String {
    if v < 0.0 {
        format!(" - {:.6} mm", -v)
    } else {
        format!(" + {:.6} mm", v)
    }
}

/// Taşıyıcı yüzey betiği (kanat, kuyruk, winglet) için ortak tanım.
struct SurfaceScript<'a> {
    title: &'a str,
    part_name: String,
    main: &'a str,
    prefix: &'a str,
    geom: SurfaceGeom,
    structure: SurfaceStructure,
    mirror: bool,
    span_alias: &'a str,
    sweep_alias: &'a str,
    /// Kanatta dihedral ve burulma (uç kesit yerleşimi) Spreadsheet'ten sürülür.
    attitude: bool,
    extra_sheet: Vec<(&'a str, String)>,
    test_val: String,
}

fn surface_script(s: SurfaceScript, options: &ExportOptions) -> String {
    let g = &s.geom;
    let st = &s.structure;
    let span = s.span_alias;
    let sweep = s.sweep_alias;
    let footer = common_python_footer(
        &s.part_name,
        &options.out_dir,
        options,
        span,
        &s.test_val,
        &[],
        s.main,
    );

    // Uç kesiti kanatta dihedral ile kalınlık yönünde yükselir ve dörtte bir kord noktası
    // (kiriş hattı) etrafında burulur (pozitif burulma = hücum kenarı yukarı). Dış yüzey kök
    // ile uç arasında düz (ruled) loft olduğundan ara istasyondaki kesit, kök ve uç kesitinin
    // doğrusal karışımıdır: ölçekli dönme a + b·e^(-i·burulma), a = (1-s)·c_kök, b = s·c_uç.
    // İç boşluk kesitleri tam bu kord (|a + b·e^(-iθ)|) ve açıyla yerleşir; böylece burulma ve
    // daralma birlikteyken de kabuk kalınlığı korunur.
    let chord_e = |z: &str| -> String {
        if s.attitude {
            let a = format!("(1 - ({z}) / Params.{span}) * Params.root_chord");
            let b = format!("({z}) / Params.{span} * Params.tip_chord");
            format!("sqrt(({a}) ^ 2 + ({b}) ^ 2 + 2 * ({a}) * ({b}) * cos(Params.twist_deg))")
        } else {
            format!(
                "(Params.root_chord + (Params.tip_chord - Params.root_chord) * ({z}) / Params.{span})"
            )
        }
    };
    let place = |z: &str| -> Vec<[String; 2]> {
        if !s.attitude {
            return vec![
                ["Base.z".to_string(), z.to_string()],
                ["Base.x".to_string(), format!("({z}) * tan(Params.{sweep})")],
            ];
        }
        let sf = format!("({z}) / Params.{span}");
        let a = format!("(1 - {sf}) * Params.root_chord");
        let b = format!("{sf} * Params.tip_chord");
        vec![
            ["Base.z".to_string(), z.to_string()],
            [
                "Base.x".to_string(),
                format!(
                    "({z}) * tan(Params.{sweep}) + {sf} * 0.25 * Params.tip_chord * (1 - cos(Params.twist_deg))"
                ),
            ],
            [
                "Base.y".to_string(),
                format!(
                    "({z}) * tan(Params.dihedral_deg) + {sf} * 0.25 * Params.tip_chord * sin(Params.twist_deg)"
                ),
            ],
            [
                "Rotation.Yaw".to_string(),
                format!(
                    "-atan2(({b}) * sin(Params.twist_deg); ({a}) + ({b}) * cos(Params.twist_deg))"
                ),
            ],
        ]
    };

    let mut rows: Vec<(String, String, &str)> = vec![
        ("root_chord".into(), qty(g.c_root, "mm"), ""),
        ("tip_chord".into(), qty(g.c_tip, "mm"), ""),
        (span.into(), qty(g.span, "mm"), ""),
        (sweep.into(), qty(g.sweep_le_deg, "deg"), ""),
        ("wall".into(), qty(st.wall_mm, "mm"), ""),
        ("root_rib".into(), qty(st.root_rib_mm, "mm"), ""),
    ];
    // İç boşluk kesitleri: kord ve et kalınlığına doğrusal (bkz. `geom::InnerTerm`); kord
    // konumları en küçük kordla (uç) seçilir, böylece tüm bölmelerde aynı kutup sayısı olur.
    let c_min = g.chord_at(g.span - st.wall_mm).min(g.c_root).max(1e-6);
    let terms = if st.is_hollow() {
        g.profile
            .inner_terms(st.wall_mm / c_min, INNER_SEGMENTS_PER_SIDE)
    } else {
        None
    };
    let mut bays_json = Vec::new();
    if let Some(terms) = &terms {
        let n = st.rib_bays;
        rows.push(("rib_t".into(), qty(st.rib_t_mm, "mm"), ""));
        rows.push((
            "rib_pitch".into(),
            format!("=({span} - wall - root_rib) / {n}"),
            "",
        ));
        rows.push((
            "rib_count".into(),
            format!("{}", n.saturating_sub(1)),
            "fixed: change in WerSeuS and regenerate / sabit: uygulamada degistirip yeniden uretin",
        ));
        let section = |z_e: &str, z: f64| -> serde_json::Value {
            let c_e = chord_e(z_e);
            let c = g.chord_at(z);
            let mut poles: Vec<serde_json::Value> = terms
                .iter()
                .map(|t| {
                    let p = t.eval(c, st.wall_mm);
                    let (lim_sign, f) = if t.upper { ("+", "max") } else { ("-", "min") };
                    serde_json::json!([
                        p[0],
                        p[1],
                        format!("{c_e} * {:.10}", t.x),
                        format!(
                            "{f}({c_e} * {:.10} + Params.wall * {:.10}; {c_e} * {:.10} {lim_sign} Params.wall * 0.25)",
                            t.y, t.w, t.mid
                        ),
                    ])
                })
                .collect();
            poles.push(poles[0].clone());
            serde_json::json!({"poles": poles, "place": place(z_e)})
        };
        for (k, (za, zb)) in st.bays_mm().into_iter().enumerate() {
            let k = k as u32;
            let za_e = if k == 0 {
                "Params.root_rib".to_string()
            } else {
                format!("Params.root_rib + {k} * Params.rib_pitch + Params.rib_t / 2")
            };
            let zb_e = if k + 1 == n {
                format!("Params.{span} - Params.wall")
            } else {
                format!(
                    "Params.root_rib + {} * Params.rib_pitch - Params.rib_t / 2",
                    k + 1
                )
            };
            bays_json.push(serde_json::json!({"a": section(&za_e, za), "b": section(&zb_e, zb)}));
        }
    }
    for (a, v) in &s.extra_sheet {
        rows.push(((*a).into(), v.clone(), ""));
    }
    let n_in = terms.as_ref().map(|t| t.len() + 1).unwrap_or(4);
    let in_knots: Vec<f64> = (0..n_in.saturating_sub(2)).map(|i| i as f64).collect();
    let mut in_mults = vec![1u32; in_knots.len()];
    if let Some(f) = in_mults.first_mut() {
        *f = 4;
    }
    if let Some(l) = in_mults.last_mut() {
        *l = 4;
    }

    let spar = s.spar_json(g);
    let spec = serde_json::json!({
        "pts": g.profile.pts,
        "c_root": g.c_root,
        "c_tip": g.c_tip,
        "tip_place": place(&format!("Params.{span}")),
        "bays": bays_json,
        "in_knots": in_knots,
        "in_mults": in_mults,
        "spar": spar,
    });
    let spec_py = py_str_literal(&spec.to_string());

    let mirror = if s.mirror {
        format!(
            r#"
# Sag yari: sol yarinin kok duzlemine (yerel Z=0) gore parametrik ayna kopyasi.
# Montajda iki yari da App::Link ile ayni kok baglanti cercevesine yerlestirilir.
mir = doc.addObject("Part::Mirroring", "{main}_Mirror")
mir.Source = body
mir.Normal = FreeCAD.Vector(0, 0, 1)
mir.Base = FreeCAD.Vector(0, 0, 0)
doc.recompute()
"#,
            main = s.main
        )
    } else {
        String::new()
    };

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik {title} - WerSeuS UAV Calc
# Yerel cerceve: X kord (hucum kenarindan firara), Y kalinlik, Z aciklik (kokten uca); kok hucum kenari orijin.
# Agac: PartDesign Body -> kok/uc profil cizimleri (tam kisitli B-spline, kutuplar Params'a bagli)
# -> AdditiveLoft -> kabuk bolmeleri (SubtractiveLoft; aralarda kaburgalar kalir) -> kiris kanali.
# Params hucresini degistirip yeniden hesaplayin (Ctrl+R): parca yeniden uretilir.
import FreeCAD, Part, Sketcher, math, json

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{sheet}doc.recompute()
{helpers}
spec = json.loads({spec_py})
body = doc.addObject("PartDesign::Body", "{main}")

sk_root = airfoil_sketch(body, "SkRoot", spec["pts"], spec["c_root"], "Params.root_chord", [])
sk_tip = airfoil_sketch(body, "SkTip", spec["pts"], spec["c_tip"], "Params.tip_chord", spec["tip_place"])
loft = body.newObject("PartDesign::AdditiveLoft", "{prefix}Loft")
loft.Profile = sk_root
loft.Sections = [sk_tip]
loft.Ruled = True

# Kabuk: her bolme ayri bosaltma; bolmeler arasinda Params.rib_t kalinliginda kaburgalar kalir.
for k, bay in enumerate(spec["bays"]):
    sa = pole_sketch(body, "{prefix}Bay%dRootSk" % k, bay["a"]["poles"], spec["in_knots"], spec["in_mults"], False, bay["a"]["place"])
    sb = pole_sketch(body, "{prefix}Bay%dTipSk" % k, bay["b"]["poles"], spec["in_knots"], spec["in_mults"], False, bay["b"]["place"])
    cut = body.newObject("PartDesign::SubtractiveLoft", "{prefix}Bay%d" % k)
    cut.Profile = sa
    cut.Sections = [sb]
    cut.Ruled = True

# Kiris kanali: kok kaburgasindan ve tum kaburgalardan gecer (cap Params.spar_od).
sp = spec["spar"]
if sp:
    hole = body.newObject("PartDesign::SubtractiveCylinder", "SparHole")
    hole.Height = sp["len"]
    hole.Radius = sp["r"]
    hole.Placement = FreeCAD.Placement(FreeCAD.Vector(*sp["start"]), FreeCAD.Rotation(FreeCAD.Vector(0, 0, 1), FreeCAD.Vector(*sp["dir"])))
    hole.setExpression("Radius", "Params.spar_od / 2")
    hole.setExpression(".Placement.Base.x", sp["ex"])
    hole.setExpression(".Placement.Base.y", sp["ey"])

doc.recompute()
check_sketches(doc)
{mirror}
{footer}
"#,
        title = s.title,
        part_name = s.part_name,
        sheet = sheet_lines(&rows),
        helpers = PY_PD_HELPERS,
        spec_py = spec_py,
        main = s.main,
        prefix = s.prefix,
        mirror = mirror,
        footer = footer,
    )
}

impl SurfaceScript<'_> {
    /// Kiriş kanalı: kök kaburgasındaki delikten kirişin ucunu 1.5 mm geçecek kadar; başlangıç
    /// noktası kord ve `spar_x_frac` hücreleriyle sürülür (eksen yönü sabit).
    fn spar_json(&self, g: &SurfaceGeom) -> serde_json::Value {
        let Some(a) = self.structure.spar else {
            return serde_json::Value::Null;
        };
        let frac = self
            .extra_sheet
            .iter()
            .find(|(k, _)| *k == "spar_x_frac")
            .and_then(|(_, v)| v.trim().parse::<f64>().ok())
            .unwrap_or(0.25);
        let mid = 0.5 * (g.profile.yu(frac) + g.profile.yl(frac));
        let s0 = super::geom::dot(super::geom::sub(a.start, a.hole_start), a.dir);
        let len = s0 + a.length + 1.5;
        let kx = a.hole_start[0] - frac * g.c_root;
        let ky = a.hole_start[1] - mid * g.c_root;
        serde_json::json!({
            "start": a.hole_start,
            "dir": a.dir,
            "len": len,
            "r": a.od / 2.0,
            "ex": format!("Params.spar_x_frac * Params.root_chord{}", mm_offset(kx)),
            "ey": format!("{mid:.10} * Params.root_chord{}", mm_offset(ky)),
        })
    }
}

/// Kanat için FreeCAD Python betiği üretir: PartDesign gövdesi, kabuk (et kalınlığı `wall_mm`),
/// kök kaburgası, ara kaburgalar, kaburgalardan geçen kiriş kanalı ve parametrik ayna (sağ yarı).
pub fn generate_wing_script(params: &WingCadParams, options: &ExportOptions) -> String {
    surface_script(
        SurfaceScript {
            title: "Kanat Uretici",
            part_name: sanitize_ident(&params.name),
            main: "WingSolid",
            prefix: "Wing",
            geom: params.geom(),
            structure: params.structure(),
            mirror: true,
            span_alias: "half_span",
            sweep_alias: "sweep_le_deg",
            attitude: true,
            extra_sheet: vec![
                ("dihedral_deg", qty(params.dihedral_deg, "deg")),
                ("twist_deg", qty(params.twist_deg, "deg")),
                ("spar_od", qty(params.spar_od_mm, "mm")),
                ("spar_x_frac", format!("{:.4}", params.spar_x_frac)),
            ],
            test_val: format!("{:.1} mm", params.half_span_mm + 50.0),
        },
        options,
    )
}

/// Kuyruk (Yatay / Dikey / V-kuyruk / winglet) için FreeCAD Python betiği üretir (kabuk + kaburga).
pub fn generate_tail_script(params: &TailCadParams, options: &ExportOptions) -> String {
    surface_script(
        SurfaceScript {
            title: "Kuyruk Uretici",
            part_name: sanitize_ident(&params.name),
            main: "TailSolid",
            prefix: "Tail",
            geom: params.geom(),
            structure: params.structure(),
            mirror: params.mirrored_pair,
            span_alias: "span",
            sweep_alias: "sweep_deg",
            attitude: false,
            extra_sheet: vec![("angle_deg", qty(params.angle_deg, "deg"))],
            test_val: format!("{:.1} mm", params.span_mm + 40.0),
        },
        options,
    )
}

/// Gövde betiği: PartDesign gövdesi. Dış profil (tam kısıtlı çizim, Params'a bağlı) döndürülür;
/// iç boşluk ayrı bir gövdede (dış profilden `wall` kadar içe kaydırılmış çizim, halka posta
/// yuvaları ve iniş takımı tabanı) üretilip Boolean ile çıkarılır. Düz burun yüzü (firewall),
/// iniş takımı oturma yüzeyi ve kanat/kuyruk yuvaları korunur.
pub fn generate_fuselage_script(params: &FuselageCadParams, options: &ExportOptions) -> String {
    let clean_name = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean_name,
        &options.out_dir,
        options,
        "length",
        &format!("{:.1} mm", params.length_mm + 50.0),
        &[],
        "FuselageSolid",
    );
    let prof = params.profile();
    let st = params.structure();
    let r_max = params.max_diameter_mm / 2.0;
    let l = params.length_mm;

    // Dış profil köşeleri ve ifadeleri (FuseProfile::new ile aynı formül).
    let n_pts = prof.pts.len();
    let k_nose = n_pts.saturating_sub(3); // burun noktası aralığı sayısı
    let mut outer: Vec<serde_json::Value> = Vec::new();
    for (i, p) in prof.pts.iter().enumerate() {
        let (ex, ey) = if i <= k_nose {
            let s = i as f64 / k_nose.max(1) as f64;
            let f = (1.0 - (1.0 - s) * (1.0 - s)).sqrt();
            (
                format!("Params.length * Params.nose_ratio * {s:.10}"),
                format!("Params.nose_r + (Params.diameter / 2 - Params.nose_r) * {f:.10}"),
            )
        } else if i + 1 < n_pts {
            (
                "Params.length * (1 - Params.tail_ratio)".to_string(),
                "Params.diameter / 2".to_string(),
            )
        } else {
            ("Params.length".to_string(), "Params.tail_r".to_string())
        };
        outer.push(serde_json::json!([p[0], p[1], ex, ey]));
    }
    let mut loop_outer = vec![serde_json::json!([0.0, 0.0, "0 mm", "0 mm"])];
    loop_outer.extend(outer.iter().cloned());
    loop_outer.push(serde_json::json!([l, 0.0, "Params.length", "0 mm"]));

    let inner = if st.wall_mm > 0.0 {
        Some(prof.inner(st.wall_mm))
    } else {
        None
    };
    // Halka postalar: iç boşluktan çıkarılan dikdörtgen kesitli halkalar. Delik yarıçapı,
    // postanın bulunduğu x'teki dış yarıçapın (burun/kuyruk/silindir bölgesine göre doğrusal
    // bileşimi) wall + former_depth eksiğidir.
    let formers: Vec<serde_json::Value> = st
        .former_fracs
        .iter()
        .map(|&f| {
            let x = f * l;
            let (a, b, c) = if f <= params.nose_ratio {
                let s = f / params.nose_ratio.max(1e-9);
                let q = (1.0 - (1.0 - s) * (1.0 - s)).sqrt();
                (1.0 - q, q, 0.0)
            } else if f >= 1.0 - params.tail_ratio {
                let t = (f - (1.0 - params.tail_ratio)) / params.tail_ratio.max(1e-9);
                (0.0, 1.0 - t, t)
            } else {
                (0.0, 1.0, 0.0)
            };
            let r_hole = prof.r_at(x) - st.wall_mm - st.former_depth_mm;
            let rh_e = format!(
                "{a:.10} * Params.nose_r + {b:.10} * Params.diameter / 2 + {c:.10} * Params.tail_r - Params.wall - Params.former_depth"
            );
            let x0_e = format!("Params.length * {f:.10} - Params.former_t / 2");
            let x1_e = format!("Params.length * {f:.10} + Params.former_t / 2");
            let h = st.former_t_mm / 2.0;
            let big = 2.0 * r_max;
            serde_json::json!([
                [x - h, r_hole, x0_e, rh_e],
                [x + h, r_hole, x1_e, rh_e],
                [x + h, big, x1_e, "Params.diameter"],
                [x - h, big, x0_e, "Params.diameter"],
            ])
        })
        .collect();
    let plugs: Vec<&PlugSpec> = params.plugs.iter().filter(|p| p.is_finite()).collect();
    let spec = serde_json::json!({
        "outer": loop_outer,
        "outer_chain": outer,
        "inner": inner,
        "wall": st.wall_mm,
        "length": l,
        "r_max": r_max,
        "pad": params.pad,
        "formers": formers,
        "plugs": plugs,
    });
    let spec_py = py_str_literal(&spec.to_string());
    let rows: Vec<(String, String, &str)> = vec![
        ("length".into(), qty(l, "mm"), ""),
        ("diameter".into(), qty(params.max_diameter_mm, "mm"), ""),
        ("wall".into(), qty(st.wall_mm, "mm"), ""),
        ("nose_r".into(), qty(params.nose_r_mm, "mm"), ""),
        ("tail_r".into(), qty(params.tail_r_mm, "mm"), ""),
        ("nose_ratio".into(), format!("{:.4}", params.nose_ratio), ""),
        ("tail_ratio".into(), format!("{:.4}", params.tail_ratio), ""),
        ("former_t".into(), qty(st.former_t_mm, "mm"), ""),
        ("former_depth".into(), qty(st.former_depth_mm, "mm"), ""),
        (
            "former_count".into(),
            format!("{}", st.former_fracs.len()),
            "fixed: change in WerSeuS and regenerate / sabit: uygulamada degistirip yeniden uretin",
        ),
    ];

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Govde Uretici - WerSeuS UAV Calc
# Yerel cerceve = montaj cercevesi: X kuyruga dogru, Y saga, Z yukari; burun yuzu (firewall) x = 0.
# Agac: FuselageSolid (PartDesign Body): dis profil cizimi -> Revolution -> inis takimi tabani
# (SubtractiveBox) -> ic bosluk (FuselageVoid govdesi, Boolean Cut) -> kanat/kuyruk yuvalari.
# FuselageVoid: ice kaydirilmis profil (paralel + mesafe kisitlari, mesafe = Params.wall)
# -> Revolution -> halka posta yuvalari (Groove) -> taban kalinligi (SubtractiveBox).
# Kanat ve kuyruk yuvalari yuzeylerin birebir parcalaridir (montaj yerlesiminden sabit).
import FreeCAD, Part, Sketcher, math, json

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{sheet}doc.recompute()

spec = json.loads({spec_py})
{helpers}
{pd_helpers}
def void_sketch(body, chain, inner):
    """Ic profil: dis profilin (yardimci geometri) her parcasina paralel ve Params.wall
    uzakliginda cizgiler; uclarda x = wall ve x = length - wall kapaklari, eksen boyunca kapanis."""
    sk = body.newObject("Sketcher::SketchObject", "SkVoid")
    _attach(sk, body, "XZ_Plane", [])
    n = len(chain) - 1
    geo = [Part.LineSegment(_V(chain[i][0], chain[i][1], 0), _V(chain[i + 1][0], chain[i + 1][1], 0)) for i in range(n)]
    sk.addGeometry(geo, True)
    w = spec["wall"]
    L = spec["length"]
    p0 = _V(w, 0, 0)
    p1 = _V(L - w, 0, 0)
    q = [_V(a[0], a[1], 0) for a in inner]
    lines = [Part.LineSegment(p0, q[0])]
    lines += [Part.LineSegment(q[j], q[j + 1]) for j in range(n)]
    lines += [Part.LineSegment(q[n], p1), Part.LineSegment(p1, p0)]
    base = n
    sk.addGeometry(lines, False)
    m = len(lines)
    cons = []
    ex = []
    for i in range(n - 1):
        cons.append(_C("Coincident", i, 2, i + 1, 1))
    for i in range(n + 1):
        g, pos = (i, 1) if i < n else (n - 1, 2)
        cons.append(_C("DistanceX", g, pos, chain[i][0]))
        ex.append((len(cons) - 1, chain[i][2]))
        cons.append(_C("DistanceY", g, pos, chain[i][1]))
        ex.append((len(cons) - 1, chain[i][3]))
    for k in range(m):
        cons.append(_C("Coincident", base + k, 2, base + (k + 1) % m, 1))
    # Isaretli ice kaydirma: her dis parcanin baslangicindan ice dogru (yon -90 deg, Angle
    # kisiti yonludur) Params.wall boyunda yardimci normal; ic cizgi normalin ucundan gecer ve
    # dis parcaya paraleldir. Isaretsiz nokta-dogru mesafesi parametre degisiminde dis tarafa
    # sicrayabildigi icin kullanilmaz.
    nb = base + m
    normals = []
    for j in range(n):
        a = _V(chain[j][0], chain[j][1], 0)
        d = _V(chain[j + 1][0] - chain[j][0], chain[j + 1][1] - chain[j][1], 0)
        d.normalize()
        normals.append(Part.LineSegment(a, a + _V(d.y, -d.x, 0) * w))
    sk.addGeometry(normals, True)
    for j in range(n):
        cons.append(_C("Parallel", base + 1 + j, j))
        cons.append(_C("Coincident", nb + j, 1, j, 1))
        cons.append(_C("Angle", j, nb + j, -math.pi / 2))
        cons.append(_C("Distance", nb + j, w))
        ex.append((len(cons) - 1, "Params.wall"))
        cons.append(_C("PointOnObject", nb + j, 2, base + 1 + j))
    cons.append(_C("Vertical", base))
    cons.append(_C("Vertical", base + n + 1))
    cons.append(_C("DistanceX", base, 1, w))
    ex.append((len(cons) - 1, "Params.wall"))
    cons.append(_C("DistanceY", base, 1, 0.0))
    cons.append(_C("DistanceX", base + n + 1, 2, L - w))
    ex.append((len(cons) - 1, "Params.length - Params.wall"))
    cons.append(_C("DistanceY", base + n + 1, 2, 0.0))
    sk.addConstraint(cons)
    _bind(sk, ex)
    return sk

def revolve(body, name, sk, groove=False):
    f = body.newObject("PartDesign::Groove" if groove else "PartDesign::Revolution", name)
    f.Profile = sk
    f.ReferenceAxis = (sk, ["H_Axis"])
    f.Angle = 360
    return f

def sub_box(body, name, x0, y0, z0, lx, ly, lz):
    b = body.newObject("PartDesign::SubtractiveBox", name)
    b.Length = lx
    b.Width = ly
    b.Height = lz
    b.Placement = FreeCAD.Placement(FreeCAD.Vector(x0, y0, z0), FreeCAD.Rotation())
    return b

body = doc.addObject("PartDesign::Body", "FuselageSolid")
sk_out = poly_sketch(body, "SkOuter", "XZ_Plane", [spec["outer"]])
revolve(body, "FuseRevolution", sk_out)
pad = spec["pad"]
big = 4.0 * spec["r_max"]
if pad:
    sub_box(body, "GearPadFlat", pad["x0"], -big, -big, pad["x1"] - pad["x0"], 2 * big, big + pad["z"])

if spec["inner"]:
    vb = doc.addObject("PartDesign::Body", "FuselageVoid")
    sk_v = void_sketch(vb, spec["outer_chain"], spec["inner"])
    revolve(vb, "VoidRevolution", sk_v)
    if spec["formers"]:
        sk_f = poly_sketch(vb, "SkFormers", "XZ_Plane", spec["formers"])
        revolve(vb, "FormerSlots", sk_f, groove=True)
    if pad:
        w = spec["wall"]
        sub_box(vb, "GearPadFloor", pad["x0"] - w, -big, -big, pad["x1"] - pad["x0"] + 2 * w, 2 * big, big + pad["z"] + w)
    cv = body.newObject("PartDesign::Boolean", "CutVoid")
    cv.Type = "Cut"
    cv.addObjects([vb])
    vb.Visibility = False

if spec["plugs"]:
    slots = []
    for i, sp in enumerate(spec["plugs"]):
        f = doc.addObject("Part::Feature", "Slot%d" % i)
        f.Shape = surf_plug(sp)
        f.Visibility = False
        slots.append(f)
    cs = body.newObject("PartDesign::Boolean", "CutSlots")
    cs.Type = "Cut"
    cs.addObjects(slots)

doc.recompute()
check_sketches(doc)

{footer}
"#,
        part_name = clean_name,
        sheet = sheet_lines(&rows),
        spec_py = spec_py,
        helpers = PY_SURF_HELPERS,
        pd_helpers = PY_PD_HELPERS,
        footer = footer,
    )
}

/// Multirotor kolu için FreeCAD Python betiği üretir.
pub fn generate_multirotor_arm_script(
    params: &MultirotorArmCadParams,
    options: &ExportOptions,
) -> String {
    let clean_name = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean_name,
        &options.out_dir,
        options,
        "arm_length",
        &format!("{:.1} mm", params.arm_length_mm + 30.0),
        std::slice::from_ref(&clean_name),
        &clean_name,
    );

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Multirotor Kolu - WerSeuS UAV Calc
import FreeCAD, Part

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
sheet.set("A1", "arm_length"); sheet.set("B1", "{arm_len:.2} mm"); sheet.setAlias("B1", "arm_length")
sheet.set("A2", "arm_width"); sheet.set("B2", "{arm_w:.2} mm"); sheet.setAlias("B2", "arm_width")
sheet.set("A3", "arm_thickness"); sheet.set("B3", "{arm_t:.2} mm"); sheet.setAlias("B3", "arm_thickness")
sheet.set("A4", "motor_pattern"); sheet.set("B4", "{pattern:.2} mm"); sheet.setAlias("B4", "motor_pattern")
sheet.set("A5", "motor_bolt_dia"); sheet.set("B5", "{m_bolt:.2} mm"); sheet.setAlias("B5", "motor_bolt_dia")
sheet.set("A6", "shaft_hole_dia"); sheet.set("B6", "{shaft:.2} mm"); sheet.setAlias("B6", "shaft_hole_dia")
sheet.set("A7", "chassis_bolt_dia"); sheet.set("B7", "{ch_bolt:.2} mm"); sheet.setAlias("B7", "chassis_bolt_dia")
sheet.set("A8", "chassis_bolt_spacing"); sheet.set("B8", "{ch_spacing:.2} mm"); sheet.setAlias("B8", "chassis_bolt_spacing")
sheet.set("A9", "chassis_x0"); sheet.set("B9", "{ch_x0:.2} mm"); sheet.setAlias("B9", "chassis_x0")

# Ana kol govdesi
box = doc.addObject("Part::Box", "ArmBody")
box.setExpression("Length", "Params.arm_length - Params.arm_width / 2")
box.setExpression("Width", "Params.arm_width")
box.setExpression("Height", "Params.arm_thickness")
box.setExpression("Placement.Base.y", "-Params.arm_width / 2")

# Motor ucu dairesel tabla
pad = doc.addObject("Part::Cylinder", "MotorPad")
pad.setExpression("Radius", "Params.arm_width / 2")
pad.setExpression("Height", "Params.arm_thickness")
pad.setExpression("Placement.Base.x", "Params.arm_length - Params.arm_width / 2")

arm_solid = doc.addObject("Part::Fuse", "ArmSolid")
arm_solid.Base = box; arm_solid.Tool = pad

# Motor mil deligi
shaft = doc.addObject("Part::Cylinder", "ShaftHole")
shaft.setExpression("Radius", "Params.shaft_hole_dia / 2")
shaft.setExpression("Height", "Params.arm_thickness + 2 mm")
shaft.setExpression("Placement.Base.x", "Params.arm_length - Params.arm_width / 2")
shaft.Placement.Base.z = -1.0

cut1 = doc.addObject("Part::Cut", "CutShaft")
cut1.Base = arm_solid; cut1.Tool = shaft

# 4 motor montaj deligi
holes = []
for i, (dx, dy) in enumerate([(-1,-1), (-1,1), (1,-1), (1,1)]):
    h = doc.addObject("Part::Cylinder", f"MotorBolt_{{i}}")
    h.setExpression("Radius", "Params.motor_bolt_dia / 2")
    h.setExpression("Height", "Params.arm_thickness + 2 mm")
    h.setExpression("Placement.Base.x", f"Params.arm_length - Params.arm_width / 2 + {{dx}} * (Params.motor_pattern / 2)")
    h.setExpression("Placement.Base.y", f"{{dy}} * (Params.motor_pattern / 2)")
    h.Placement.Base.z = -1.0
    holes.append(h)

prev = cut1
for i, h in enumerate(holes):
    c = doc.addObject("Part::Cut", f"CutMotorHole_{{i}}")
    c.Base = prev; c.Tool = h
    prev = c

# Sasi baglanti delikleri (2 adet): merkez plakadaki kol deliklerine denk gelir
ch_holes = []
for i in range(2):
    ch = doc.addObject("Part::Cylinder", f"ChassisBolt_{{i}}")
    ch.setExpression("Radius", "Params.chassis_bolt_dia / 2")
    ch.setExpression("Height", "Params.arm_thickness + 2 mm")
    ch.setExpression("Placement.Base.x", f"Params.chassis_x0 + {{i}} * Params.chassis_bolt_spacing")
    ch.Placement.Base.z = -1.0
    ch_holes.append(ch)

for i, ch in enumerate(ch_holes):
    c = doc.addObject("Part::Cut", f"CutChassisHole_{{i}}")
    c.Base = prev; c.Tool = ch
    prev = c

# Kablo kanali / hafifletme yuvasi
slot = doc.addObject("Part::Box", "CableSlot")
slot.setExpression("Length", "Params.arm_length * 0.45")
slot.setExpression("Width", "Params.arm_width * 0.40")
slot.setExpression("Height", "Params.arm_thickness + 2 mm")
slot.setExpression("Placement.Base.x", "40.0 mm")
slot.setExpression("Placement.Base.y", "-Params.arm_width * 0.20")
slot.Placement.Base.z = -1.0

main_part = doc.addObject("Part::Cut", "{part_name}")
main_part.Base = prev; main_part.Tool = slot

doc.recompute()

{footer}
"#,
        part_name = clean_name,
        arm_len = params.arm_length_mm,
        arm_w = params.width_mm,
        arm_t = params.thickness_mm,
        pattern = params.motor_mount_pattern_mm,
        m_bolt = params.motor_bolt_dia_mm,
        shaft = params.motor_shaft_dia_mm,
        ch_bolt = params.chassis_bolt_dia_mm,
        ch_spacing = params.chassis_bolt_spacing_mm,
        ch_x0 = params.chassis_hole_x0_mm,
        footer = footer,
    )
}

/// Multirotor merkez plakası için FreeCAD Python betiği üretir.
pub fn generate_multirotor_plate_script(
    params: &MultirotorPlateCadParams,
    options: &ExportOptions,
) -> String {
    let clean_name = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean_name,
        &options.out_dir,
        options,
        "plate_radius",
        &format!("{:.1} mm", params.plate_radius_mm + 15.0),
        std::slice::from_ref(&clean_name),
        &clean_name,
    );

    // Kolların açısal konumları (montajla aynı liste)
    let arm_angles = mr_arm_angles(params.layout, params.arm_count);

    let arm_r = format!(
        "[{:.3}, {:.3}]",
        params.arm_hole_r_mm[0], params.arm_hole_r_mm[1]
    );
    let angles_py = format!(
        "[{}]",
        arm_angles
            .iter()
            .map(|a| format!("{:.1}", a))
            .collect::<Vec<_>>()
            .join(", ")
    );

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Merkez Plaka - WerSeuS UAV Calc
import FreeCAD, Part, math

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
sheet.set("A1", "plate_radius"); sheet.set("B1", "{radius:.2} mm"); sheet.setAlias("B1", "plate_radius")
sheet.set("A2", "thickness"); sheet.set("B2", "{thickness:.2} mm"); sheet.setAlias("B2", "thickness")
sheet.set("A3", "chassis_bolt_dia"); sheet.set("B3", "{ch_bolt:.2} mm"); sheet.setAlias("B3", "chassis_bolt_dia")
sheet.set("A4", "fc_pattern"); sheet.set("B4", "{fc_pattern:.2} mm"); sheet.setAlias("B4", "fc_pattern")
sheet.set("A5", "center_hole_dia"); sheet.set("B5", "{center_hole:.2} mm"); sheet.setAlias("B5", "center_hole_dia")

# Ana dairesel / poligon plaka
base_plate = doc.addObject("Part::Cylinder", "BasePlate")
base_plate.setExpression("Radius", "Params.plate_radius")
base_plate.setExpression("Height", "Params.thickness")

# Merkez kablo gecis deligi
center_hole = doc.addObject("Part::Cylinder", "CenterHole")
center_hole.setExpression("Radius", "Params.center_hole_dia / 2")
center_hole.setExpression("Height", "Params.thickness + 4.0 mm")
center_hole.Placement.Base.z = -2.0

# FC montaj delikleri (30.5x30.5 mm)
fc_holes = []
for i, (dx, dy) in enumerate([(-1,-1), (-1,1), (1,-1), (1,1)]):
    h = doc.addObject("Part::Cylinder", f"FcHole_{{i}}")
    h.Radius = 1.6 # M3 delik
    h.setExpression("Height", "Params.thickness + 4.0 mm")
    h.setExpression("Placement.Base.x", f"{{dx}} * (Params.fc_pattern / 2)")
    h.setExpression("Placement.Base.y", f"{{dy}} * (Params.fc_pattern / 2)")
    h.Placement.Base.z = -2.0
    fc_holes.append(h)

# Kol montaj delik ciftleri
angles = {angles_py}
arm_mount_holes = []
for arm_idx, ang_deg in enumerate(angles):
    ang_rad = math.radians(ang_deg)
    cos_a = math.cos(ang_rad)
    sin_a = math.sin(ang_rad)
    for pair_idx, r_abs in enumerate({arm_r}):
        h = doc.addObject("Part::Cylinder", f"ArmHole_{{arm_idx}}_{{pair_idx}}")
        h.setExpression("Radius", "Params.chassis_bolt_dia / 2")
        h.setExpression("Height", "Params.thickness + 4.0 mm")
        # Polar yerlesim (kolun sasi delikleriyle ayni yaricaplar)
        h.Placement.Base.x = r_abs * cos_a
        h.Placement.Base.y = r_abs * sin_a
        h.Placement.Base.z = -2.0
        arm_mount_holes.append(h)

# Tum delikleri Compound altinda toplayip tek seferde kes (BRep NULL shape hatasini cozer)
all_holes = [center_hole] + fc_holes + arm_mount_holes
hole_comp = doc.addObject("Part::Compound", "HolesCompound")
hole_comp.Links = all_holes

main_part = doc.addObject("Part::Cut", "{part_name}")
main_part.Base = base_plate
main_part.Tool = hole_comp

doc.recompute()

{footer}
"#,
        part_name = clean_name,
        radius = params.plate_radius_mm,
        thickness = params.thickness_mm,
        ch_bolt = params.chassis_bolt_dia_mm,
        fc_pattern = params.fc_mount_pattern_mm,
        center_hole = params.center_hole_dia_mm,
        angles_py = angles_py,
        arm_r = arm_r,
        footer = footer,
    )
}

/// Sabit kanat motor yuvası için FreeCAD Python betiği üretir.
pub fn generate_motor_mount_script(
    params: &MotorMountCadParams,
    options: &ExportOptions,
) -> String {
    let clean_name = sanitize_ident(&params.name);
    let footer = common_python_footer(
        &clean_name,
        &options.out_dir,
        options,
        "plate_size",
        &format!("{:.1} mm", params.plate_size_mm + 10.0),
        std::slice::from_ref(&clean_name),
        &clean_name,
    );

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Parametrik Motor Yuvasi - WerSeuS UAV Calc
import FreeCAD, Part

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
sheet.set("A1", "plate_size"); sheet.set("B1", "{plate_size:.2} mm"); sheet.setAlias("B1", "plate_size")
sheet.set("A2", "thickness"); sheet.set("B2", "{thickness:.2} mm"); sheet.setAlias("B2", "thickness")
sheet.set("A3", "motor_pattern"); sheet.set("B3", "{pattern:.2} mm"); sheet.setAlias("B3", "motor_pattern")
sheet.set("A4", "motor_bolt_dia"); sheet.set("B4", "{m_bolt:.2} mm"); sheet.setAlias("B4", "motor_bolt_dia")
sheet.set("A5", "shaft_hole_dia"); sheet.set("B5", "{shaft:.2} mm"); sheet.setAlias("B5", "shaft_hole_dia")
sheet.set("A6", "firewall_spacing"); sheet.set("B6", "{fw_spacing:.2} mm"); sheet.setAlias("B6", "firewall_spacing")
sheet.set("A7", "firewall_bolt_dia"); sheet.set("B7", "{fw_bolt:.2} mm"); sheet.setAlias("B7", "firewall_bolt_dia")

# Ana montaj plakasi
plate = doc.addObject("Part::Box", "BasePlate")
plate.setExpression("Length", "Params.plate_size")
plate.setExpression("Width", "Params.plate_size")
plate.setExpression("Height", "Params.thickness")
plate.setExpression("Placement.Base.x", "-Params.plate_size / 2")
plate.setExpression("Placement.Base.y", "-Params.plate_size / 2")

# Motor saft deligi
shaft = doc.addObject("Part::Cylinder", "ShaftHole")
shaft.setExpression("Radius", "Params.shaft_hole_dia / 2")
shaft.setExpression("Height", "Params.thickness + 2 mm")
shaft.Placement.Base.z = -1.0

# Motor montaj delikleri (istavroz deseni: 4 delik)
m_holes = []
for i, (dx, dy) in enumerate([(-1,0), (1,0), (0,-1), (0,1)]):
    h = doc.addObject("Part::Cylinder", f"MotorBolt_{{i}}")
    h.setExpression("Radius", "Params.motor_bolt_dia / 2")
    h.setExpression("Height", "Params.thickness + 4.0 mm")
    h.setExpression("Placement.Base.x", f"{{dx}} * (Params.motor_pattern / 2)")
    h.setExpression("Placement.Base.y", f"{{dy}} * (Params.motor_pattern / 2)")
    h.Placement.Base.z = -2.0
    m_holes.append(h)

# Govde / Firewall montaj delikleri (4 kosede)
fw_holes = []
for i, (dx, dy) in enumerate([(-1,-1), (-1,1), (1,-1), (1,1)]):
    h = doc.addObject("Part::Cylinder", f"FwBolt_{{i}}")
    h.setExpression("Radius", "Params.firewall_bolt_dia / 2")
    h.setExpression("Height", "Params.thickness + 4.0 mm")
    h.setExpression("Placement.Base.x", f"{{dx}} * (Params.firewall_spacing / 2)")
    h.setExpression("Placement.Base.y", f"{{dy}} * (Params.firewall_spacing / 2)")
    h.Placement.Base.z = -2.0
    fw_holes.append(h)

# Tum delikleri Compound altinda toplayip tek seferde kes (BRep NULL shape hatasini cozer)
all_holes = [shaft] + m_holes + fw_holes
hole_comp = doc.addObject("Part::Compound", "MountHolesCompound")
hole_comp.Links = all_holes

main_part = doc.addObject("Part::Cut", "{part_name}")
main_part.Base = plate
main_part.Tool = hole_comp

doc.recompute()

{footer}
"#,
        part_name = clean_name,
        plate_size = params.plate_size_mm,
        thickness = params.thickness_mm,
        pattern = params.motor_bolt_pattern_mm,
        m_bolt = params.motor_bolt_dia_mm,
        shaft = params.shaft_hole_dia_mm,
        fw_spacing = params.firewall_bolt_spacing_mm,
        fw_bolt = params.firewall_bolt_dia_mm,
        footer = footer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wing_script_syntax_and_params() {
        let sym = crate::cad::airfoil::generate_naca4_points("2412", 40, true).unwrap();
        let params = WingCadParams {
            name: "Test_Wing".into(),
            root_chord_mm: 200.0,
            tip_chord_mm: 120.0,
            half_span_mm: 800.0,
            sweep_le_deg: 5.0,
            dihedral_deg: 2.0,
            twist_deg: -1.0,
            spar_od_mm: 10.0,
            spar_id_mm: 8.0,
            spar_x_frac: 0.25,
            wall_mm: 0.5,
            root_profile: sym.clone(),
            tip_profile: sym,
        };
        let opts = ExportOptions::default();
        let script = generate_wing_script(&params, &opts);

        assert!(script.contains("import FreeCAD"));
        assert!(script.contains("Params.half_span"));
        assert!(script.contains("WingBay%d") && script.contains("SparHole"));
        assert!(script.contains("PartDesign::AdditiveLoft") && script.contains("rib_pitch"));
        // Kabuk bölmeleri ve kiriş kanalı spec içinde dolu olmalı (spec JSON, Python dizgesi).
        assert!(script.contains(r#"\"bays\":[{"#), "kaburga bölmeleri yok");
        assert!(!script.contains(r#"\"spar\":null"#), "kiriş kanalı yok");
        // Her şey Spreadsheet takma adlarına bağlı: kutup ifadeleri hücre adlarını kullanır.
        assert!(script.contains("Params.root_chord") && script.contains("Params.rib_t / 2"));
        assert!(script.contains("200.00 mm"));
        assert!(script.contains("120.00 mm"));
        assert!(script.contains("800.00 mm"));
        // Parça bütün verilir: bölme yok.
        assert!(!script.contains("Segment_") && !script.contains("\"Segment"));
        assert!(script.contains("__WERSEUS_CAD_JSON_START__"));
    }

    #[test]
    fn test_tail_script_params() {
        let sym = crate::cad::airfoil::generate_naca4_points("0009", 40, true).unwrap();
        let params = TailCadParams {
            name: "Test_Tail".into(),
            tail_kind: TailKind::Conventional,
            root_chord_mm: 120.0,
            tip_chord_mm: 80.0,
            span_mm: 250.0,
            sweep_le_deg: 8.0,
            angle_deg: 0.0,
            spar_od_mm: 0.0,
            spar_x_frac: 0.30,
            wall_mm: 0.5,
            mirrored_pair: true,
            profile: sym,
        };
        let opts = ExportOptions::default();
        let script = generate_tail_script(&params, &opts);

        assert!(script.contains("root_chord"));
        assert!(script.contains("120.00 mm"));
        assert!(script.contains("span"));
        assert!(script.contains("TailSolid"));
    }

    #[test]
    fn test_tail_cad_params_profile_selection() {
        let mut p = crate::project::Project::default();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let naca12 = crate::cad::airfoil::generate_naca4_points("0012", 40, true).unwrap();
        let naca08 = crate::cad::airfoil::generate_naca4_points("0008", 40, true).unwrap();
        let naca10 = crate::cad::airfoil::generate_naca4_points("0010", 40, true).unwrap();

        p.tail_airfoil_h.is_naca = true;
        p.tail_airfoil_h.naca_code = "0012".into();
        p.tail_airfoil_h.name = "NACA 0012".into();
        p.tail_airfoil_h.points = naca12.clone();

        p.tail_airfoil_v.is_naca = true;
        p.tail_airfoil_v.naca_code = "0008".into();
        p.tail_airfoil_v.name = "NACA 0008".into();
        p.tail_airfoil_v.points = naca08.clone();

        // 1. Yatay kuyruk (Conventional) tail_airfoil_h (0012) okumalı
        let params_h = TailCadParams::from_project(&p, &m, TailKind::Conventional);
        assert_eq!(params_h.profile, naca12);
        assert!(params_h.name.contains("Yatay_Kuyruk"));
        assert_eq!(params_h.angle_deg, 0.0);

        // 2. Dikey kuyruk (from_project_vertical) tail_airfoil_v (0008) okumalı
        let params_v = TailCadParams::from_project_vertical(&p, &m);
        assert_eq!(params_v.profile, naca08);
        assert!(params_v.name.contains("Dikey_Kuyruk"));
        assert_eq!(params_v.angle_deg, 90.0);

        // 3. V-Kuyruk tail_airfoil_v (0010) okumalı
        p.tail_kind = TailKind::VTail;
        p.tail_airfoil_v.points = naca10.clone();
        p.tail_airfoil_v.name = "NACA 0010".into();
        let params_vtail = TailCadParams::from_project(&p, &m, TailKind::VTail);
        assert_eq!(params_vtail.profile, naca10);
        assert!(params_vtail.name.contains("V_Kuyruk"));

        // 4. Uçan kanat winglet tail_airfoil_v (0010) okumalı
        p.tail_kind = TailKind::FlyingWing;
        let params_winglet = TailCadParams::from_project(&p, &m, TailKind::FlyingWing);
        assert_eq!(params_winglet.profile, naca10);
        assert!(params_winglet.name.contains("Winglet"));
    }

    #[test]
    fn test_tail_cad_params_fallback_to_symmetric() {
        let mut p = crate::project::Project::default();
        p.tail_airfoil_h.points.clear();
        p.tail_airfoil_v.points.clear();
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let params_h = TailCadParams::from_project(&p, &m, TailKind::Conventional);
        assert!(
            !params_h.profile.is_empty(),
            "Boş profil simetrik 0009'a düşmeli"
        );

        let params_v = TailCadParams::from_project_vertical(&p, &m);
        assert!(!params_v.profile.is_empty());

        let params_vtail = TailCadParams::from_project(&p, &m, TailKind::VTail);
        assert!(!params_vtail.profile.is_empty());

        // Uçan kanat: tail_airfoil_v boşken kanat profili tanımlıysa kanat profilini alır
        let wing_pts = crate::cad::airfoil::generate_naca4_points("2412", 40, true).unwrap();
        p.airfoil.points = wing_pts.clone();
        let params_winglet = TailCadParams::from_project(&p, &m, TailKind::FlyingWing);
        assert_eq!(params_winglet.profile, wing_pts);
    }

    #[test]
    fn test_fuselage_script_params() {
        let params = FuselageCadParams {
            name: "Test_Fuse".into(),
            length_mm: 900.0,
            max_diameter_mm: 110.0,
            nose_r_mm: 30.0,
            tail_r_mm: 16.0,
            nose_ratio: 0.25,
            tail_ratio: 0.35,
            wall_mm: 0.5,
            plugs: Vec::new(),
            pad: Some(GearPad {
                x0: 180.0,
                x1: 220.0,
                z: -50.0,
            }),
        };
        let opts = ExportOptions::default();
        let script = generate_fuselage_script(&params, &opts);

        assert!(script.contains("FuselageSolid"));
        assert!(script.contains("revolve"));
        assert!(!script.contains("Segment_") && !script.contains("\"Segment"));
    }

    #[test]
    fn test_multirotor_arm_and_plate_scripts() {
        let arm_params = MultirotorArmCadParams {
            name: "Test_Arm".into(),
            arm_length_mm: 140.0,
            width_mm: 24.0,
            thickness_mm: 4.0,
            motor_mount_pattern_mm: 16.0,
            motor_bolt_dia_mm: 3.0,
            motor_shaft_dia_mm: 6.0,
            chassis_bolt_dia_mm: 3.0,
            chassis_hole_x0_mm: 7.5,
            chassis_bolt_spacing_mm: 15.0,
            cable_channel_w_mm: 8.0,
        };
        let opts = ExportOptions::default();
        let arm_script = generate_multirotor_arm_script(&arm_params, &opts);
        assert!(arm_script.contains("ArmBody"));
        assert!(arm_script.contains("MotorPad"));
        assert!(arm_script.contains("16.00 mm"));

        let plate_params = MultirotorPlateCadParams {
            name: "Test_Plate".into(),
            layout: MultirotorLayout::X,
            arm_count: 4,
            plate_radius_mm: 35.0,
            thickness_mm: 2.5,
            arm_width_mm: 24.0,
            chassis_bolt_dia_mm: 3.0,
            fc_mount_pattern_mm: 30.5,
            center_hole_dia_mm: 20.0,
            arm_hole_r_mm: [21.0, 30.0],
        };
        let plate_script = generate_multirotor_plate_script(&plate_params, &opts);
        assert!(plate_script.contains("BasePlate"));
        assert!(plate_script.contains("CenterHole"));
        assert!(plate_script.contains("30.50 mm"));
    }

    #[test]
    fn test_motor_mount_script() {
        let mount_params = MotorMountCadParams {
            name: "Test_Mount".into(),
            mount_type: "tractor".into(),
            plate_size_mm: 50.0,
            thickness_mm: 4.0,
            motor_bolt_pattern_mm: 19.0,
            motor_bolt_dia_mm: 3.0,
            shaft_hole_dia_mm: 8.0,
            firewall_bolt_spacing_mm: 40.0,
            firewall_bolt_dia_mm: 3.5,
        };
        let opts = ExportOptions::default();
        let script = generate_motor_mount_script(&mount_params, &opts);
        assert!(script.contains("BasePlate"));
        assert!(script.contains("ShaftHole"));
        assert!(script.contains("19.00 mm"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("WerSeuS_Tasarım_1"), "WerSeuS_Tasarim_1");
        assert_eq!(
            sanitize_filename("Test_Tasarım_Çğışöü_İĞŞÖÜ"),
            "Test_Tasarim_Cgisou_IGSOU"
        );
        assert_eq!(sanitize_filename("Dosya Adı (Özel)"), "Dosya_Adi__Ozel_");
        assert_eq!(sanitize_filename(""), "unnamed");
    }

    #[test]
    fn test_sanitize_ident() {
        assert_eq!(sanitize_ident("Normal_Name_123"), "Normal_Name_123");
        assert_eq!(
            sanitize_ident("123_starts_with_digit"),
            "_123_starts_with_digit"
        );
        assert_eq!(sanitize_ident(""), "part");
        assert_eq!(
            sanitize_ident("x\"); import os; os.system(\"calc\"); #"),
            "x____import_os__os_system__calc_____"
        );
    }

    #[test]
    fn test_malicious_script_injection_prevented() {
        let sym = crate::cad::airfoil::generate_naca4_points("2412", 20, true).unwrap();
        let malicious_name = "test\"); import os; os.system(\"echo PWNED\"); #";
        let params = WingCadParams {
            name: malicious_name.into(),
            root_chord_mm: 200.0,
            tip_chord_mm: 120.0,
            half_span_mm: 800.0,
            sweep_le_deg: 5.0,
            dihedral_deg: 0.0,
            twist_deg: 0.0,
            spar_od_mm: 8.0,
            spar_id_mm: 6.0,
            spar_x_frac: 0.25,
            wall_mm: 0.5,
            root_profile: sym.clone(),
            tip_profile: sym,
        };
        let opts = ExportOptions {
            export_fcstd: true,
            export_step: false,
            export_stl: false,
            out_dir: PathBuf::from("malicious\\path\\with\"quote; import sys;"),
        };
        let script = generate_wing_script(&params, &opts);

        // Doküman adı temizlenmiş olmalı, enjeksiyon içermemeli
        assert!(!script.contains("newDocument(\"test\"); import os;"));
        let clean_id = sanitize_ident(malicious_name);
        assert!(script.contains(&format!("newDocument(\"{}\")", clean_id)));

        // out_dir ve metinler JSON/Python literal olarak güvenli geçmeli
        assert!(!script.contains("out_dir = r\"malicious\\path\\with\"quote"));
        let norm_path = opts.out_dir.to_string_lossy().replace('\\', "/");
        assert!(script.contains(&py_str_literal(&norm_path)));
    }
}
