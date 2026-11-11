// SPDX-License-Identifier: AGPL-3.0-or-later
//! Montaj yerleşimi: parçaların parametreleri ve bağlantı noktaları (anchor) aynı yerden
//! türetilir; her kopyanın yerleşimi bir ebeveyn bağlantısına "mate" edilerek hesaplanır.
//!
//! Bağlantılar (çocuk → ebeveyn):
//! - kanat kökü → gövdedeki kanat yuvası (yuva, kanadın birebir parçası çıkarılarak açılır)
//! - kiriş borusu → kanat kök kaburgasındaki delik
//! - yatay/dikey/V kuyruk kökü → gövde kuyruk ucundaki yuvalar
//! - winglet kökü → kanat ucu kesiti
//! - motor yuvası arka yüzü → gövde burun (ya da kuyruk) yüzü = firewall
//! - iniş takımı tablası → gövde altındaki düz oturma yüzeyi / merkez plaka alt yüzü
//! - VTOL bomu → bom-kanat kelepçesinin deliği; kelepçe semeri → kanat alt yüzeyi
//! - kaldırma motoru yuvası → bom üst yüzeyi (bom uçlarında)
//! - multirotor kolu → merkez plaka üst yüzü; motor yuvası → kolun motor tablası
//!
//! Sağ taraf kopyaları `geom::mirrored_placement` ile sol kopyadan türetilir ve App::Link ile
//! ya parçanın kendisine (simetrikse) ya da parça dosyasındaki parametrik ayna nesnesine bağlanır.

use super::freecad::{
    FuselageCadParams, GearPad, MotorMountCadParams, MultirotorArmCadParams,
    MultirotorPlateCadParams, PlugSpec, TailCadParams, WingCadParams, mr_arm_angles,
    sanitize_ident, wall_mm_from,
};
use super::geom::{Frame, FuseProfile, SurfaceGeom, mate, mirrored_placement};
use super::outputs::{ClampCadParams, LandingLegCadParams, PartId, TubeCadParams, available_parts};
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::project::{DroneType, Project};

/// Montajdaki bir kopya.
#[derive(Clone, Debug)]
pub struct Instance {
    pub label: String,
    pub id: PartId,
    /// Parça dosyasındaki ayna nesnesine mi bağlanır?
    pub mirror: bool,
    /// Parça yerel çerçevesinden montaj (gövde) çerçevesine.
    pub frame: Frame,
    /// Bağlandığı (mate) ebeveyn kopya; kök parçada `None`.
    pub parent: Option<String>,
}

/// Bir projenin tüm parça parametreleri ve montaj kopyaları.
#[derive(Clone, Debug)]
pub struct Layout {
    pub wing: WingCadParams,
    pub tail_h: TailCadParams,
    pub tail_v: TailCadParams,
    pub vtail: TailCadParams,
    pub winglet: TailCadParams,
    pub fuselage: FuselageCadParams,
    pub motor_mount: MotorMountCadParams,
    pub lift_mount: MotorMountCadParams,
    pub mr_mount: MotorMountCadParams,
    pub spar: Option<TubeCadParams>,
    pub boom: TubeCadParams,
    pub clamp: ClampCadParams,
    pub leg: LandingLegCadParams,
    pub mr_arm: MultirotorArmCadParams,
    pub mr_plate: MultirotorPlateCadParams,
    pub instances: Vec<Instance>,
}

fn surface_frame(dihedral_deg: f64, origin: [f64; 3]) -> Frame {
    // Sol yarı: yerel X → +X, yerel Z (açıklık) → (0, -cosγ, sinγ), yerel Y (kalınlık) → X × Z'nin tersi.
    let (s, c) = dihedral_deg.to_radians().sin_cos();
    Frame::from_axes([1.0, 0.0, 0.0], [0.0, s, c], [0.0, -c, s], origin)
}

/// Taşıyıcı yüzeyin [z0, z1] açıklık aralığındaki en küçük/en büyük alt ve üst yüzey değerleri
/// (yerel Y), kord konumu `x_local` ± `half_w` aralığında örneklenir.
fn surface_bounds(g: &SurfaceGeom, x_local: f64, half_w: f64, z0: f64, z1: f64) -> [f64; 3] {
    let (mut min_lo, mut max_lo, mut min_up) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY);
    for i in 0..=12 {
        let x = x_local - half_w + 2.0 * half_w * i as f64 / 12.0;
        for j in 0..=6 {
            let z = z0 + (z1 - z0) * j as f64 / 6.0;
            let c = g.chord_at(z);
            let f = ((x - g.le_x_at(z)) / c).clamp(0.0, 1.0);
            let lo = g.profile.yl(f) * c;
            let up = g.profile.yu(f) * c;
            min_lo = min_lo.min(lo);
            max_lo = max_lo.max(lo);
            min_up = min_up.min(up);
        }
    }
    [min_lo, max_lo, min_up]
}

/// Kopya etiketinden tasarım bağlantısının ebeveyni (çocuk → ebeveyn).
fn mate_parent(label: &str, multirotor: bool) -> Option<String> {
    let side = |l: &str| -> &str { if l.contains("_R") { "R" } else { "L" } };
    let p = if label.starts_with("WingSpar_")
        || label.starts_with("Winglet_")
        || label.starts_with("BoomClamp_")
    {
        format!("Wing_{}", side(label))
    } else if label.starts_with("Boom_") {
        format!("BoomClamp_{}", side(label))
    } else if label.starts_with("LiftMount_") {
        format!("Boom_{}", side(label))
    } else if let Some(k) = label.strip_prefix("MrMotorMount_") {
        format!("MrArm_{k}")
    } else if label.starts_with("MrArm_") {
        "MrPlate".into()
    } else if label.starts_with("LandingGear_") {
        if multirotor { "MrPlate" } else { "Fuselage" }.into()
    } else if label == "Fuselage" || label == "MrPlate" {
        return None;
    } else {
        "Fuselage".into()
    };
    Some(p)
}

impl Layout {
    pub fn new(p: &Project, m: &Model) -> Self {
        let pname = sanitize_ident(&p.name);
        let wall = wall_mm_from(p);
        let parts = available_parts(p);
        let has = |id: PartId| parts.contains(&id);
        let multirotor = p.drone_type == DroneType::Multirotor;
        let mut inst: Vec<Instance> = Vec::new();
        let push = |inst: &mut Vec<Instance>, label: &str, id: PartId, mirror: bool, f: Frame| {
            inst.push(Instance {
                label: label.to_string(),
                id,
                mirror,
                frame: f,
                parent: mate_parent(label, multirotor),
            });
        };

        let wing = WingCadParams::from_project(p, m);
        let wg = wing.geom();
        let spar_axis = wing.spar();
        let tail_h = TailCadParams::from_project(p, m, TailKind::Conventional);
        let tail_v = TailCadParams::from_project_vertical(p, m);
        let vtail = TailCadParams::from_project(p, m, TailKind::VTail);
        let winglet = TailCadParams::from_project(p, m, TailKind::FlyingWing);
        let motor_mount = MotorMountCadParams::from_project(p, m);
        let mut lift_mount = motor_mount.clone();
        lift_mount.name = format!("{pname}_Kaldirma_Motor_Yuvasi");
        lift_mount.mount_type = "tractor".into();
        let mut mr_mount = motor_mount.clone();
        mr_mount.name = format!("{pname}_MR_Motor_Yuvasi");
        mr_mount.mount_type = "tractor".into();

        // --- Gövde profili
        let length = (p.l_fuse * 1000.0).max(100.0);
        let radius = (p.d_fuse * 500.0).max(15.0);
        let pusher = motor_mount.mount_type == "pusher";
        let fw_r = motor_mount.firewall_radius_mm().min(0.85 * radius);
        let (nose_r, tail_r) = if pusher {
            ((0.3 * radius).max(6.0), fw_r)
        } else {
            (fw_r, (0.3 * radius).max(8.0))
        };
        let (nose_ratio, tail_ratio) = (0.25, 0.35);
        let fprof = FuseProfile::new(length, radius, nose_r, tail_r, nose_ratio, tail_ratio);
        let mut plugs: Vec<PlugSpec> = Vec::new();
        let fuse_frame = Frame::IDENTITY;

        // --- Kanat (sol yarı kökü gövde ekseninde, kord çizgisi z = 0)
        let x_wle = p.x_root_le * 1000.0;
        let wing_l = surface_frame(0.0, [x_wle, 0.0, 0.0]);
        let wing_r = mirrored_placement(&wing_l, 2);
        if !multirotor {
            let z1 = (fprof.r_max(x_wle, x_wle + wg.c_root * 1.2) + 5.0).min(wg.span);
            plugs.push(PlugSpec::new(&wg, 0.0, z1, &wing_l));
            plugs.push(PlugSpec::new(&wg, -z1, 0.0, &wing_r));
            if has(PartId::Wing) {
                push(&mut inst, "Wing_L", PartId::Wing, false, wing_l);
                push(&mut inst, "Wing_R", PartId::Wing, true, wing_r);
            }
        }

        // --- Kiriş borusu (her yarıda kök kaburgasından geçer)
        let spar = spar_axis.map(|a| TubeCadParams {
            name: format!("{pname}_Kanat_Kirisi"),
            od_mm: a.od,
            id_mm: a.id,
            length_mm: a.length,
        });
        if let (Some(a), true) = (spar_axis, has(PartId::WingSpar)) {
            let sl = wing_l.mul(&a.frame());
            push(&mut inst, "WingSpar_L", PartId::WingSpar, false, sl);
            push(
                &mut inst,
                "WingSpar_R",
                PartId::WingSpar,
                false,
                mirrored_placement(&sl, 1),
            );
        }

        // --- Kuyruk: yüzeyler gövde (bom) ucuna; firar kenarı gövde kuyruk yüzüyle aynı x'te.
        let conventional_tail =
            !multirotor && p.drone_type != DroneType::FlyingWing && p.tail_kind != TailKind::VTail;
        if conventional_tail {
            let hg = tail_h.geom();
            let x_h = length - hg.c_root;
            let h_l = surface_frame(0.0, [x_h, 0.0, 0.0]);
            let h_r = mirrored_placement(&h_l, 2);
            let z1 = (fprof.r_max(x_h, length) + 5.0).min(hg.span);
            plugs.push(PlugSpec::new(&hg, 0.0, z1, &h_l));
            plugs.push(PlugSpec::new(&hg, -z1, 0.0, &h_r));
            if has(PartId::TailHorizontal) {
                push(&mut inst, "TailH_L", PartId::TailHorizontal, false, h_l);
                push(&mut inst, "TailH_R", PartId::TailHorizontal, true, h_r);
            }
            // Dikey kuyruk kökü yatay kuyruğun üst yüzeyine teğet düzlemde.
            let vg = tail_v.geom();
            let x_v = length - vg.c_root;
            let v_f = Frame::from_axes(
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [x_v, 0.0, hg.root_top()],
            );
            let z1 = (fprof.r_max(x_v, length) + 5.0).min(vg.span);
            plugs.push(PlugSpec::new(&vg, 0.0, z1, &v_f));
            if has(PartId::TailVertical) {
                push(&mut inst, "TailV", PartId::TailVertical, false, v_f);
            }
        } else if !multirotor && p.drone_type != DroneType::FlyingWing {
            // V kuyruk: kökler, iki yarının hacmi kesişmeyecek kadar (d = T·tanγ) dışarıda.
            let vg = vtail.geom();
            let x_v = length - vg.c_root;
            let gam = p.gamma_v_deg.clamp(5.0, 85.0);
            let d = vg.root_half_thickness() * gam.to_radians().tan() + 0.05;
            let (s, c) = gam.to_radians().sin_cos();
            let v_l = surface_frame(gam, [x_v, -d * c, d * s]);
            let v_r = mirrored_placement(&v_l, 2);
            let z1 = (fprof.r_max(x_v, length) + 5.0 + d).min(vg.span);
            plugs.push(PlugSpec::new(&vg, 0.0, z1, &v_l));
            plugs.push(PlugSpec::new(&vg, -z1, 0.0, &v_r));
            if has(PartId::TailVTail) {
                push(&mut inst, "VTail_L", PartId::TailVTail, false, v_l);
                push(&mut inst, "VTail_R", PartId::TailVTail, true, v_r);
            }
        }

        // --- Winglet: kanat ucu kesitine, kalın yüzeyi uç düzlemine teğet, yukarı doğru.
        if has(PartId::Winglet) {
            let wl = winglet.geom();
            let t_half = wl.root_half_thickness();
            let tip = Frame::from_axes(
                [1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0],
                [0.0, 1.0, 0.0],
                [wg.le_x_at(wg.span), 0.0, wg.span + t_half],
            );
            let wl_l = wing_l.mul(&tip);
            push(&mut inst, "Winglet_L", PartId::Winglet, false, wl_l);
            push(
                &mut inst,
                "Winglet_R",
                PartId::Winglet,
                true,
                mirrored_placement(&wl_l, 2),
            );
        }

        // --- Motor yuvası: arka yüzü firewall'a (burun yüzü; itici ise kuyruk yüzü).
        if has(PartId::MotorMount) {
            let fw = if pusher {
                Frame::from_axes(
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 0.0],
                    [length, 0.0, 0.0],
                )
            } else {
                Frame::from_axes(
                    [0.0, 1.0, 0.0],
                    [0.0, 0.0, -1.0],
                    [-1.0, 0.0, 0.0],
                    [0.0, 0.0, 0.0],
                )
            };
            push(
                &mut inst,
                "MotorMount",
                PartId::MotorMount,
                false,
                mate(&fuse_frame, &fw, &Frame::IDENTITY),
            );
        }

        // --- İniş takımı bacağı
        let leg = LandingLegCadParams::from_project(p);
        let tab_y = leg.tab_w_mm / 2.0 + 0.5;
        let mut pad = None;
        if has(PartId::LandingGear) {
            if multirotor {
                // Merkez plakanın alt yüzüne (z = 0)
                let a = Frame::rot_z(180.0, [0.0, -tab_y, 0.0]);
                push(&mut inst, "LandingGear_L", PartId::LandingGear, false, a);
                push(
                    &mut inst,
                    "LandingGear_R",
                    PartId::LandingGear,
                    false,
                    mirrored_placement(&a, 0),
                );
            } else {
                // Gövde altında, kanat hücum kenarı istasyonunda düz oturma yüzeyi.
                let x_g = x_wle;
                let x0 = x_g - leg.tab_l_mm / 2.0 - 3.0;
                let x1 = x_g + leg.tab_l_mm / 2.0 + 3.0;
                let w_need = leg.tab_w_mm + 0.5 + 1.5;
                let r_min = fprof.r_min(x0, x1);
                if r_min > w_need + wall + 2.0 {
                    let z_pad = -(r_min * r_min - w_need * w_need).sqrt();
                    pad = Some(GearPad { x0, x1, z: z_pad });
                    let a = Frame::rot_z(180.0, [x_g, -tab_y, z_pad]);
                    push(&mut inst, "LandingGear_L", PartId::LandingGear, false, a);
                    push(
                        &mut inst,
                        "LandingGear_R",
                        PartId::LandingGear,
                        false,
                        mirrored_placement(&a, 0),
                    );
                }
            }
        }

        // --- VTOL: bom-kanat kelepçesi, bom ve kaldırma motoru yuvaları
        let boom_od = {
            let d = p.mr_arm_d_o * 1000.0;
            if d.is_finite() && d >= 6.0 { d } else { 16.0 }
        };
        let boom_id = {
            let d = p.mr_arm_d_i * 1000.0;
            if d.is_finite() && d > 0.0 && d < boom_od {
                d
            } else {
                0.8 * boom_od
            }
        };
        let mut clamp = ClampCadParams {
            name: format!("{pname}_Bom_Kanat_Kelepce"),
            bore_mm: boom_od,
            wall_mm: 3.0,
            width_mm: 24.0,
            pillar_top_mm: boom_od,
            slit_mm: 1.0,
            saddle: None,
        };
        let mut boom = TubeCadParams {
            name: format!("{pname}_VTOL_Bomu"),
            od_mm: boom_od,
            id_mm: boom_id,
            length_mm: 500.0,
        };
        if p.drone_type == DroneType::VtolQuadplane {
            let ro = clamp.bore_mm / 2.0 + clamp.wall_mm;
            let d_prop = (p.mr_d_prop * 1000.0).max(50.0);
            let y_b = (radius + d_prop / 2.0 + 15.0).min(0.6 * wg.span);
            let c_b = wg.chord_at(y_b);
            let x_s_local = wg.le_x_at(y_b) + 0.25 * c_b;
            let x_s = x_wle + x_s_local;
            let [min_lo, max_lo, min_up] = surface_bounds(
                &wg,
                x_s_local,
                clamp.width_mm / 2.0,
                y_b - ro - 1.0,
                y_b + ro + 1.0,
            );
            let z_b = min_lo - 2.0 - ro;
            clamp.pillar_top_mm = 0.5 * (max_lo + min_up) - z_b;
            let c_frame = Frame::translation([x_s, -y_b, z_b]);
            clamp.saddle = Some(PlugSpec::new(
                &wg,
                y_b - ro - 1.0,
                y_b + ro + 1.0,
                &c_frame.inverse().mul(&wing_l),
            ));
            // Motorlar: köşegen = çerçeve boyu; ön/arka motor yuvaları kanadın önünde/arkasında.
            let plate = lift_mount.plate_size_mm;
            let diag = p.mr_frame_size * 1000.0;
            let l_diag = if diag > 2.1 * y_b {
                (diag * diag - 4.0 * y_b * y_b).sqrt()
            } else {
                0.0
            };
            let l_m = l_diag.max(d_prop + 30.0).max(1.5 * c_b + plate + 20.0);
            boom.length_mm = l_m + plate;
            if has(PartId::BoomWingClamp) {
                push(
                    &mut inst,
                    "BoomClamp_L",
                    PartId::BoomWingClamp,
                    false,
                    c_frame,
                );
                push(
                    &mut inst,
                    "BoomClamp_R",
                    PartId::BoomWingClamp,
                    true,
                    mirrored_placement(&c_frame, 1),
                );
            }
            let bore = Frame::from_axes(
                [0.0, 0.0, 1.0],
                [0.0, -1.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
            );
            let station = Frame::translation([0.0, 0.0, l_m / 2.0 + plate / 2.0]);
            let boom_l = mate(&c_frame, &bore, &station);
            if has(PartId::VtolBoom) {
                push(&mut inst, "Boom_L", PartId::VtolBoom, false, boom_l);
                push(
                    &mut inst,
                    "Boom_R",
                    PartId::VtolBoom,
                    false,
                    mirrored_placement(&boom_l, 1),
                );
            }
            if has(PartId::LiftMotorMount) {
                for (k, s) in [plate / 2.0, boom.length_mm - plate / 2.0]
                    .into_iter()
                    .enumerate()
                {
                    let top = Frame::from_axes(
                        [0.0, 0.0, 1.0],
                        [0.0, -1.0, 0.0],
                        [1.0, 0.0, 0.0],
                        [boom.od_mm / 2.0, 0.0, s],
                    );
                    let ml = mate(&boom_l, &top, &Frame::IDENTITY);
                    let tag = if k == 0 { "F" } else { "A" };
                    push(
                        &mut inst,
                        &format!("LiftMount_L{tag}"),
                        PartId::LiftMotorMount,
                        false,
                        ml,
                    );
                    push(
                        &mut inst,
                        &format!("LiftMount_R{tag}"),
                        PartId::LiftMotorMount,
                        false,
                        mirrored_placement(&ml, 1),
                    );
                }
            }
        }

        // --- Multirotor: kollar plaka üstüne, motor yuvaları kol tablasına
        let n = p.mr_arm_count.max(1);
        let angles = mr_arm_angles(p.mr_layout, n);
        let arm_w = 24.0;
        let min_gap = if angles.len() > 1 {
            let mut a = angles.clone();
            a.sort_by(f64::total_cmp);
            let mut g = 360.0 - (a[a.len() - 1] - a[0]);
            for w in a.windows(2) {
                g = g.min(w[1] - w[0]);
            }
            g
        } else {
            360.0
        };
        let half_gap = (min_gap / 2.0).clamp(5.0, 90.0);
        let r_root = if half_gap >= 89.9 {
            2.0
        } else {
            (arm_w / 2.0) / half_gap.to_radians().tan() + 2.0
        };
        let plate_t = 2.5;
        let plate_r = (p.mr_r_plate * 1000.0)
            .clamp(25.0, 150.0)
            .max(r_root + 12.0);
        let arm_len = (p.mr_frame_size * 500.0 - r_root + arm_w / 2.0).max(60.0);
        let arm_t = 4.0;
        let ch_x0 = 7.5;
        let ch_bolt = 3.0;
        let mut ch_s = (plate_r - 2.5 - ch_bolt / 2.0 - r_root - ch_x0).min(16.0);
        if ch_s < 4.0 {
            ch_s = 0.0;
        }
        let pattern = if p.mr_d_prop <= 0.15 {
            16.0
        } else if p.mr_d_prop <= 0.25 {
            19.0
        } else {
            25.0
        };
        let mr_arm = MultirotorArmCadParams {
            name: format!("{pname}_Kol"),
            arm_length_mm: arm_len,
            width_mm: arm_w,
            thickness_mm: arm_t,
            motor_mount_pattern_mm: pattern,
            motor_bolt_dia_mm: 3.0,
            motor_shaft_dia_mm: 6.0,
            chassis_bolt_dia_mm: ch_bolt,
            chassis_hole_x0_mm: ch_x0,
            chassis_bolt_spacing_mm: ch_s,
            cable_channel_w_mm: 8.0,
        };
        let mr_plate = MultirotorPlateCadParams {
            name: format!("{pname}_Merkez_Plaka"),
            layout: p.mr_layout,
            arm_count: n,
            plate_radius_mm: plate_r,
            thickness_mm: plate_t,
            arm_width_mm: arm_w,
            chassis_bolt_dia_mm: ch_bolt,
            fc_mount_pattern_mm: 30.5,
            center_hole_dia_mm: 20.0,
            arm_hole_r_mm: [r_root + ch_x0, r_root + ch_x0 + ch_s],
        };
        if multirotor {
            if has(PartId::MrPlate) {
                push(
                    &mut inst,
                    "MrPlate",
                    PartId::MrPlate,
                    false,
                    Frame::IDENTITY,
                );
            }
            for (k, ang) in angles.iter().enumerate() {
                let rad = ang.to_radians();
                let a = Frame::rot_z(*ang, [r_root * rad.cos(), r_root * rad.sin(), plate_t]);
                let arm = mate(&Frame::IDENTITY, &a, &Frame::IDENTITY);
                if has(PartId::MrArm) {
                    push(&mut inst, &format!("MrArm_{k}"), PartId::MrArm, false, arm);
                }
                if has(PartId::MrMotorMount) {
                    let pad = Frame::translation([arm_len - arm_w / 2.0, 0.0, arm_t]);
                    push(
                        &mut inst,
                        &format!("MrMotorMount_{k}"),
                        PartId::MrMotorMount,
                        false,
                        mate(&arm, &pad, &Frame::IDENTITY),
                    );
                }
            }
        }

        let fuselage = FuselageCadParams {
            name: format!("{pname}_Govde"),
            length_mm: length,
            max_diameter_mm: 2.0 * radius,
            nose_r_mm: nose_r,
            tail_r_mm: tail_r,
            nose_ratio,
            tail_ratio,
            wall_mm: wall,
            plugs,
            pad,
        };
        if has(PartId::Fuselage) {
            inst.insert(
                0,
                Instance {
                    label: "Fuselage".into(),
                    id: PartId::Fuselage,
                    mirror: false,
                    frame: fuse_frame,
                    parent: None,
                },
            );
        }

        Layout {
            wing,
            tail_h,
            tail_v,
            vtail,
            winglet,
            fuselage,
            motor_mount,
            lift_mount,
            mr_mount,
            spar,
            boom,
            clamp,
            leg,
            mr_arm,
            mr_plate,
            instances: inst,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    fn lay(file: &str) -> (Project, Layout) {
        let p = crate::project::load_example(file).unwrap();
        let m = evaluate(&p, &Library::embedded());
        let l = Layout::new(&p, &m);
        (p, l)
    }

    #[test]
    fn every_available_part_has_instances_with_proper_frames() {
        for f in [
            "fixed_wing_2kg_1600mm.json",
            "flying_wing_1200mm.json",
            "hexa_550_x.json",
            "quad_250_x.json",
            "quad_275_x.json",
            "vtol_quadplane.json",
        ] {
            let (p, l) = lay(f);
            for id in available_parts(&p) {
                let n = l.instances.iter().filter(|i| i.id == id).count();
                assert_eq!(
                    n as u32,
                    crate::cad::outputs::part_quantity(id, &p),
                    "{f}: {id:?}"
                );
            }
            for i in &l.instances {
                assert!(i.frame.is_finite(), "{f}: {}", i.label);
                assert!((i.frame.det() - 1.0).abs() < 1e-9, "{f}: {}", i.label);
            }
            // Her kopyanın ebeveyni (kök hariç) montajda var.
            for i in &l.instances {
                if let Some(par) = &i.parent {
                    assert!(
                        l.instances.iter().any(|j| &j.label == par),
                        "{f}: {} -> {par}",
                        i.label
                    );
                }
            }
            let mut labels: Vec<_> = l.instances.iter().map(|i| i.label.clone()).collect();
            labels.sort();
            labels.dedup();
            assert_eq!(labels.len(), l.instances.len(), "{f}: etiketler benzersiz");
        }
    }

    #[test]
    fn wing_root_sits_in_fuselage_slot_and_follows_parameters() {
        let (mut p, l) = lay("fixed_wing_2kg_1600mm.json");
        let w = l.instances.iter().find(|i| i.label == "Wing_L").unwrap();
        // Kök hücum kenarı x_root_le'de, gövde ekseninde.
        assert!((w.frame.t[0] - p.x_root_le * 1000.0).abs() < 1e-9);
        assert!(w.frame.t[1].abs() < 1e-9 && w.frame.t[2].abs() < 1e-9);
        // Gövdede bu kanadın yuvası var (aynı çerçeve).
        assert!(
            l.fuselage
                .plugs
                .iter()
                .any(|pl| pl.m == w.frame.matrix16() && pl.z0 == 0.0)
        );
        // Parametre değişince yerleşim de değişir.
        p.x_root_le += 0.05;
        p.l_fuse += 0.1;
        let m = evaluate(&p, &Library::embedded());
        let l2 = Layout::new(&p, &m);
        let w2 = l2.instances.iter().find(|i| i.label == "Wing_L").unwrap();
        assert!((w2.frame.t[0] - p.x_root_le * 1000.0).abs() < 1e-9);
        let th = l2.instances.iter().find(|i| i.label == "TailH_L").unwrap();
        assert!((th.frame.t[0] + l2.tail_h.root_chord_mm - p.l_fuse * 1000.0).abs() < 1e-9);
    }

    #[test]
    fn right_copies_mirror_left() {
        let (_, l) = lay("vtol_quadplane.json");
        let get = |s: &str| l.instances.iter().find(|i| i.label == s).unwrap().frame;
        for (a, b) in [
            ("Boom_L", "Boom_R"),
            ("BoomClamp_L", "BoomClamp_R"),
            ("LiftMount_LF", "LiftMount_RF"),
        ] {
            let (fa, fb) = (get(a), get(b));
            assert!((fa.t[0] - fb.t[0]).abs() < 1e-9);
            assert!((fa.t[1] + fb.t[1]).abs() < 1e-9);
            assert!((fa.t[2] - fb.t[2]).abs() < 1e-9);
        }
        // Kaldırma motoru yuvası bomun üstünde (teğet).
        let boom = get("Boom_L");
        let lm = get("LiftMount_LF");
        assert!((lm.t[2] - (boom.t[2] + l.boom.od_mm / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn mr_plate_holes_match_arm_holes() {
        let (_, l) = lay("hexa_550_x.json");
        let a = &l.mr_arm;
        let first = l.instances.iter().find(|i| i.label == "MrArm_0").unwrap();
        let r_root = (first.frame.t[0].powi(2) + first.frame.t[1].powi(2)).sqrt();
        assert!((l.mr_plate.arm_hole_r_mm[0] - (r_root + a.chassis_hole_x0_mm)).abs() < 1e-9);
        assert!(l.mr_plate.arm_hole_r_mm[1] < l.mr_plate.plate_radius_mm);
        // Motor tablası çerçeve yarıçapında.
        let mm = l
            .instances
            .iter()
            .find(|i| i.label == "MrMotorMount_0")
            .unwrap();
        let r = (mm.frame.t[0].powi(2) + mm.frame.t[1].powi(2)).sqrt();
        assert!((r - 275.0).abs() < 1e-6, "{r}");
    }
}
