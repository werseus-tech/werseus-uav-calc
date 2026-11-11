// SPDX-License-Identifier: AGPL-3.0-or-later
//! FreeCAD 1.1 uyumlu parametrik Python betiği üreticileri.
//! Tüm fonksiyonlar saf Rust string'i üretir ve bağımsız olarak birim testlerine tabi tutulabilir.

use super::geom::{Frame, FuseProfile, Profile, SparAxis, SurfaceGeom};
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

    /// Geometri (profil geçersizse simetrik NACA 0009).
    pub fn geom(&self) -> SurfaceGeom {
        SurfaceGeom {
            profile: profile_or_default(&self.root_profile),
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
            profile: profile_or_default(&self.profile),
            c_root: self.root_chord_mm,
            c_tip: self.tip_chord_mm,
            span: self.span_mm,
            sweep_le_deg: self.sweep_le_deg,
        }
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

fn format_points_python(pts: &[[f64; 2]], scale: &str) -> String {
    let mut out = String::from("[\n");
    for p in pts {
        out.push_str(&format!(
            "    FreeCAD.Vector({:.6} * ({}), {:.6} * ({}), 0),\n",
            p[0], scale, p[1], scale
        ));
    }
    out.push(']');
    out
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
        if abs(updated_vol - vol_mm3) > 0.01:
            param_update_ok = True
    except Exception as e:
        print(f"Parametric update test warning: {{e}}")
    # Ozgun degere geri don: dosyaya ve STEP'e test degeri degil ozgun geometri yazilmali.
    if orig_val is not None:
        sheet.set(test_param, orig_val)
        doc.recompute()
        restored = doc.getObject("{main_obj_clean}")
        restored_vol = mesh_props(restored.Shape)[0] if restored else 0.0
        if abs(restored_vol - vol_mm3) > 1e-3 * max(1.0, abs(vol_mm3)):
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
/// Kesit formülü kanat/kuyruk betikleriyle aynıdır (doğrusal kord ve hücum kenarı).
pub(crate) const PY_SURF_HELPERS: &str = r#"
def _surf_section(sp, z):
    s = abs(z)
    c = sp["c_r"] + (sp["c_t"] - sp["c_r"]) * s / sp["span"]
    x0 = s * math.tan(math.radians(sp["sweep"]))
    mtx = FreeCAD.Matrix(*sp["m"])
    vs = [mtx.multVec(FreeCAD.Vector(x0 + q[0] * c, q[1] * c, z)) for q in sp["pts"]]
    bs = Part.BSplineCurve()
    bs.interpolate(vs)
    return Part.Wire([bs.toShape()])

def surf_plug(sp):
    return Part.makeLoft([_surf_section(sp, sp["z0"]), _surf_section(sp, sp["z1"])], True, True)
"#;

/// Taşıyıcı yüzey betiği (kanat, kuyruk, winglet) için ortak tanım.
struct SurfaceScript<'a> {
    title: &'a str,
    part_name: String,
    main: &'a str,
    prefix: &'a str,
    geom: SurfaceGeom,
    wall: f64,
    spar: Option<SparAxis>,
    mirror: bool,
    span_alias: &'a str,
    sweep_alias: &'a str,
    extra_sheet: Vec<(&'a str, String)>,
    test_val: String,
}

fn format_polygon_python(pts: &[[f64; 2]]) -> String {
    let mut out = String::from("[\n");
    for p in pts.iter().filter(|p| p[0].is_finite() && p[1].is_finite()) {
        out.push_str(&format!(
            "    FreeCAD.Vector({:.5}, {:.5}, 0),\n",
            p[0], p[1]
        ));
    }
    out.push(']');
    out
}

fn surface_script(s: SurfaceScript, options: &ExportOptions) -> String {
    let g = &s.geom;
    let root_rib = s.spar.map(|a| a.root_rib).unwrap_or(s.wall);
    let footer = common_python_footer(
        &s.part_name,
        &options.out_dir,
        options,
        s.span_alias,
        &s.test_val,
        &[],
        s.main,
    );

    let mut sheet_rows: Vec<(&str, String)> = vec![
        ("root_chord", format!("{:.2} mm", g.c_root)),
        ("tip_chord", format!("{:.2} mm", g.c_tip)),
        (s.span_alias, format!("{:.2} mm", g.span)),
        (s.sweep_alias, format!("{:.2} deg", g.sweep_le_deg)),
        ("wall", format!("{:.2} mm", s.wall)),
        ("root_rib", format!("{:.2} mm", root_rib)),
    ];
    sheet_rows.extend(s.extra_sheet.iter().cloned());
    let sheet: String = sheet_rows
        .iter()
        .enumerate()
        .map(|(i, (alias, val))| {
            let a = sanitize_ident(alias);
            format!(
                "sheet.set(\"A{r}\", \"{a}\"); sheet.set(\"B{r}\", {v}); sheet.setAlias(\"B{r}\", \"{a}\")\n",
                r = i + 1,
                v = py_str_literal(val),
            )
        })
        .collect();

    // İç boşluk (kabuk): kök kaburgası ve uç kapağı arasında, kesit başına iç profil.
    let z_in_root = root_rib;
    let z_in_tip = g.span - s.wall;
    let inner = if s.wall > 0.0 && z_in_tip > z_in_root + 1.0 {
        let a = g.profile.inner_polygon(g.chord_at(z_in_root), s.wall, 40);
        let b = g.profile.inner_polygon(g.chord_at(z_in_tip), s.wall, 40);
        a.zip(b)
    } else {
        None
    };
    let mut tools = String::new();
    if let Some((pa, pb)) = &inner {
        tools.push_str(&format!(
            r#"
# Ic bosluk (kabuk): kok kaburgasi ile uc kapagi arasinda, et kalinligi Params.wall
# (Part::Feature tel: nokta sirasi korunur, boylece loft burulmaz; yerlesim ifadeyle parametrik)
def _poly_section(name, pts):
    sec = doc.addObject("Part::Feature", name)
    sec.Shape = Part.makePolygon(pts + [pts[0]])
    return sec
sk_in_root = _poly_section("SecInnerRoot", {pa})
sk_in_root.setExpression("Placement.Base.z", "Params.root_rib")
sk_in_root.setExpression("Placement.Base.x", "Params.root_rib * tan(Params.{sweep})")
sk_in_tip = _poly_section("SecInnerTip", {pb})
sk_in_tip.setExpression("Placement.Base.z", "Params.{span} - Params.wall")
sk_in_tip.setExpression("Placement.Base.x", "(Params.{span} - Params.wall) * tan(Params.{sweep})")
inner = doc.addObject("Part::Loft", "{prefix}InnerLoft")
inner.Sections = [sk_in_root, sk_in_tip]
inner.Solid = True; inner.Ruled = True
tools.append(inner)
"#,
            pa = format_polygon_python(pa),
            pb = format_polygon_python(pb),
            sweep = s.sweep_alias,
            span = s.span_alias,
            prefix = s.prefix,
        ));
    }
    if let Some(a) = &s.spar {
        tools.push_str(&format!(
            r#"
# Kiris (boru) deligi: kok kaburgasindan gecer; eksen dortte bir kord cizgisi boyunca
spar = doc.addObject("Part::Cylinder", "SparHole")
spar.setExpression("Radius", "Params.spar_od / 2")
spar.Height = {h:.4}
spar.Placement = FreeCAD.Placement(FreeCAD.Vector({x:.5}, {y:.5}, {z:.5}), FreeCAD.Rotation(FreeCAD.Vector(0, 0, 1), FreeCAD.Vector({dx:.8}, {dy:.8}, {dz:.8})))
tools.append(spar)
"#,
            h = a.hole_len,
            x = a.hole_start[0],
            y = a.hole_start[1],
            z = a.hole_start[2],
            dx = a.dir[0],
            dy = a.dir[1],
            dz = a.dir[2],
        ));
    }
    let has_tools = inner.is_some() || s.spar.is_some();
    let finish = if has_tools {
        format!(
            r#"prev = loft
for i, tool in enumerate(tools):
    nm = "{main}" if i == len(tools) - 1 else "{prefix}Cut%d" % i
    c = doc.addObject("Part::Cut", nm)
    c.Base = prev; c.Tool = tool
    prev = c
"#,
            main = s.main,
            prefix = s.prefix
        )
    } else {
        format!(
            r#"main_ref = doc.addObject("Part::Refine", "{main}")
main_ref.Source = loft
"#,
            main = s.main
        )
    };
    let mirror = if s.mirror {
        format!(
            r#"
# Sag yari: sol yarinin kok duzlemine (yerel Z=0) gore parametrik ayna kopyasi.
# Montajda iki yari da App::Link ile ayni kok baglanti cercevesine yerlestirilir.
mir = doc.addObject("Part::Mirroring", "{main}_Mirror")
mir.Source = doc.getObject("{main}")
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
import FreeCAD, Part, Sketcher, math

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
{sheet}doc.recompute()

root_chord = {c_r:.4}
tip_chord = {c_t:.4}

# Kok profili (Z = 0)
sk_root = doc.addObject("Sketcher::SketchObject", "SkRoot")
pts_root = {root_pts}
b_root = Part.BSplineCurve()
b_root.interpolate(pts_root)
sk_root.addGeometry(b_root, False)

# Uc profili (Z = aciklik)
sk_tip = doc.addObject("Sketcher::SketchObject", "SkTip")
sk_tip.setExpression("Placement.Base.z", "Params.{span}")
sk_tip.setExpression("Placement.Base.x", "Params.{span} * tan(Params.{sweep})")
pts_tip = {tip_pts}
b_tip = Part.BSplineCurve()
b_tip.interpolate(pts_tip)
sk_tip.addGeometry(b_tip, False)

loft = doc.addObject("Part::Loft", "{prefix}Loft")
loft.Sections = [sk_root, sk_tip]
loft.Solid = True
loft.Ruled = False

tools = []
{tools}
{finish}
doc.recompute()
{mirror}
{footer}
"#,
        title = s.title,
        part_name = s.part_name,
        sheet = sheet,
        c_r = g.c_root,
        c_t = g.c_tip,
        root_pts = format_points_python(&g.profile.pts, "root_chord"),
        tip_pts = format_points_python(&g.profile.pts, "tip_chord"),
        span = s.span_alias,
        sweep = s.sweep_alias,
        prefix = s.prefix,
        tools = tools,
        finish = finish,
        mirror = mirror,
        footer = footer,
    )
}

/// Kanat için FreeCAD Python betiği üretir: içi boş kabuk (et kalınlığı `wall_mm`), kök
/// kaburgası, dörtte bir kord çizgisi boyunca kiriş deliği ve parametrik ayna (sağ yarı).
pub fn generate_wing_script(params: &WingCadParams, options: &ExportOptions) -> String {
    surface_script(
        SurfaceScript {
            title: "Kanat Uretici",
            part_name: sanitize_ident(&params.name),
            main: "WingSolid",
            prefix: "Wing",
            geom: params.geom(),
            wall: params.wall_mm,
            spar: params.spar(),
            mirror: true,
            span_alias: "half_span",
            sweep_alias: "sweep_le_deg",
            extra_sheet: vec![
                ("dihedral_deg", format!("{:.2} deg", params.dihedral_deg)),
                ("twist_deg", format!("{:.2} deg", params.twist_deg)),
                ("spar_od", format!("{:.2} mm", params.spar_od_mm)),
                ("spar_x_frac", format!("{:.4}", params.spar_x_frac)),
            ],
            test_val: format!("{:.1} mm", params.half_span_mm + 50.0),
        },
        options,
    )
}

/// Kuyruk (Yatay / Dikey / V-kuyruk / winglet) için FreeCAD Python betiği üretir (içi boş).
pub fn generate_tail_script(params: &TailCadParams, options: &ExportOptions) -> String {
    surface_script(
        SurfaceScript {
            title: "Kuyruk Uretici",
            part_name: sanitize_ident(&params.name),
            main: "TailSolid",
            prefix: "Tail",
            geom: params.geom(),
            wall: params.wall_mm,
            spar: None,
            mirror: params.mirrored_pair,
            span_alias: "span",
            sweep_alias: "sweep_deg",
            extra_sheet: vec![("angle_deg", format!("{:.2} deg", params.angle_deg))],
            test_val: format!("{:.1} mm", params.span_mm + 40.0),
        },
        options,
    )
}

/// Gövde betiği: dönel (revolve) dış profil, et kalınlığı `wall_mm` olan iç boşluk, düz burun
/// yüzü (firewall), iniş takımı oturma yüzeyi ve kanat/kuyruk yuvaları.
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
    let inner = if params.wall_mm > 0.0 {
        Some(prof.inner(params.wall_mm))
    } else {
        None
    };
    let plugs: Vec<&PlugSpec> = params.plugs.iter().filter(|p| p.is_finite()).collect();
    let spec = serde_json::json!({
        "outer": prof.pts,
        "inner": inner,
        "wall": params.wall_mm,
        "r_max": params.max_diameter_mm / 2.0,
        "pad": params.pad,
        "plugs": plugs,
    });
    let spec_py = py_str_literal(&spec.to_string());

    format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD 1.1 Govde Uretici - WerSeuS UAV Calc
# Yerel cerceve = montaj cercevesi: X kuyruga dogru, Y saga, Z yukari; burun yuzu (firewall) x = 0.
# Dis profil donel; ic bosluk et kalinligi kadar ice kaydirilmis profil. Kanat ve kuyruk yuvalari,
# yuzeylerin birebir parcalari (ayni kesit formulu) cikarilarak acilir.
import FreeCAD, Part, math, json

doc = FreeCAD.newDocument("{part_name}")
doc.UseHasher = False

sheet = doc.addObject("Spreadsheet::Sheet", "Params")
sheet.set("A1", "length"); sheet.set("B1", "{length:.2} mm"); sheet.setAlias("B1", "length")
sheet.set("A2", "diameter"); sheet.set("B2", "{dia:.2} mm"); sheet.setAlias("B2", "diameter")
sheet.set("A3", "wall"); sheet.set("B3", "{wall:.2} mm"); sheet.setAlias("B3", "wall")
sheet.set("A4", "nose_r"); sheet.set("B4", "{nose_r:.2} mm"); sheet.setAlias("B4", "nose_r")
sheet.set("A5", "tail_r"); sheet.set("B5", "{tail_r:.2} mm"); sheet.setAlias("B5", "tail_r")
doc.recompute()

spec = json.loads({spec_py})
{helpers}
def _revolve(prof):
    pts = [FreeCAD.Vector(prof[0][0], 0, 0)]
    pts += [FreeCAD.Vector(q[0], 0, q[1]) for q in prof]
    pts.append(FreeCAD.Vector(prof[-1][0], 0, 0))
    pts.append(pts[0])
    face = Part.Face(Part.makePolygon(pts))
    return face.revolve(FreeCAD.Vector(0, 0, 0), FreeCAD.Vector(1, 0, 0), 360)

outer = _revolve(spec["outer"])
void = _revolve(spec["inner"]) if spec["inner"] else None
pad = spec["pad"]
if pad:
    big = 4.0 * spec["r_max"]
    box = Part.makeBox(pad["x1"] - pad["x0"], 2 * big, big + pad["z"], FreeCAD.Vector(pad["x0"], -big, -big))
    outer = outer.cut(box)
    if void is not None:
        w = spec["wall"]
        box_i = Part.makeBox(pad["x1"] - pad["x0"] + 2 * w, 2 * big, big + pad["z"] + w, FreeCAD.Vector(pad["x0"] - w, -big, -big))
        void = void.cut(box_i)
shell = outer.cut(void) if void is not None else outer
for sp in spec["plugs"]:
    shell = shell.cut(surf_plug(sp))
if len(shell.Solids) == 1:
    shell = shell.Solids[0]
main_part = doc.addObject("Part::Feature", "FuselageSolid")
main_part.Shape = shell

doc.recompute()

{footer}
"#,
        part_name = clean_name,
        length = params.length_mm,
        dia = params.max_diameter_mm,
        wall = params.wall_mm,
        nose_r = params.nose_r_mm,
        tail_r = params.tail_r_mm,
        spec_py = spec_py,
        helpers = PY_SURF_HELPERS,
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
        assert!(script.contains("WingInnerLoft") && script.contains("SparHole"));
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
