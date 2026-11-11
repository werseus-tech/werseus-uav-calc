// SPDX-License-Identifier: AGPL-3.0-or-later
//! Hesap kütüphanesi: saf fonksiyonlar + düz yapılar. `evaluate` tüm modülleri zincirler.
#![allow(dead_code)] // kütüphane yüzeyinin bir kısmı yalnızca testlerde kullanılır
pub mod aero;
pub mod aero_presets;
pub mod atmosphere;
pub mod constraint;
pub mod custom_battery;
pub mod geometry;
pub mod mr_structure;
pub mod multirotor;
pub mod performance;
pub mod presets;
pub mod propulsion;
pub mod stability;
pub mod structure;
pub mod weight;

use crate::project::{Link, Project};
use aero::{AeroGeom, AeroIn, AeroOut};
use atmosphere::{Atmo, RHO0, isa};
use constraint::{ConstraintIn, ConstraintOut};
use geometry::{TailIn, TailKind, TailOut, WingIn, WingOut};
use performance::{PerfIn, PerfPoint};
use propulsion::{PropIn, PropOut};
use stability::{Cg, Comp, NpIn, NpOut};
use structure::{StructIn, StructOut};
use weight::{Cell, Material, WeightIn, WeightOut};

/// Gömülü ve topluluk veri tabloları (malzemeler, hücreler, paketler).
pub struct Library {
    pub materials: Vec<Material>,
    pub cells: Vec<Cell>,
    pub packs: Vec<crate::db::DatabasePack>,
    pub db_info: crate::db::DatabaseInfo,
}

impl Library {
    /// Aktif veritabanından yükler (Varsayılan: %APPDATA%\WerSeuS\db\ veya gömülü fallback).
    pub fn load() -> Self {
        let (db_cells, packs, db_info) = crate::db::load_active_database();
        Library {
            materials: weight::load_materials(),
            cells: db_cells.into_iter().map(Cell::from).collect(),
            packs,
            db_info,
        }
    }

    /// Yalnızca derleme anında gömülü olan veritabanından yükler.
    /// Disk erişimi yapmaz, paralel testler ve çevrimdışı çalışma için güvenlidir.
    pub fn embedded() -> Self {
        let cells = crate::db::load_embedded_cells();
        let packs = crate::db::load_embedded_packs();
        let manifest = crate::db::load_embedded_manifest();
        let db_info = crate::db::DatabaseInfo {
            source: crate::db::DbSource::Embedded,
            schema_version: manifest.schema_version,
            updated: manifest.updated,
            cell_count: cells.len(),
            pack_count: packs.len(),
            motor_count: 0,
            prop_count: 0,
            esc_count: 0,
        };
        Library {
            materials: weight::load_materials(),
            cells: cells.into_iter().map(Cell::from).collect(),
            packs,
            db_info,
        }
    }

    /// Belirtilen dizindeki veritabanından yükler (doğrulama başarısızsa gömülü kopyaya fallback yapar).
    pub fn load_from(dir: &std::path::Path) -> Self {
        let (db_cells, packs, db_info) = crate::db::load_active_database_from_dir(dir);
        Library {
            materials: weight::load_materials(),
            cells: db_cells.into_iter().map(Cell::from).collect(),
            packs,
            db_info,
        }
    }

    pub fn cell(&self, name: &str) -> Cell {
        self.cells
            .iter()
            .find(|c| c.name == name || (!c.id.is_empty() && c.id == name))
            .or_else(|| self.cells.first())
            .cloned()
            .unwrap_or(Cell {
                id: "backup_cell".into(),
                name: crate::i18n::t("cell.backup_name").into(),
                chemistry: "Li-ion".into(),
                nominal_v: 3.6,
                capacity_ah: 4.2,
                mass_g: 70.0,
                max_cont_a: 45.0,
                source: "Backup".into(),
                source_url: String::new(),
                source_date: String::new(),
                verified_by: "system".into(),
            })
    }
    pub fn material(&self, name_or_id: &str) -> Option<&Material> {
        self.materials
            .iter()
            .find(|m| m.id == name_or_id || m.name == name_or_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sev {
    Info,
    Warn,
    Err,
}

#[derive(Clone, Debug)]
pub struct Warn {
    pub sev: Sev,
    pub area_key: &'static str,
    pub msg_key: &'static str,
    pub args: Vec<String>,
}

impl Warn {
    pub fn text(&self) -> String {
        crate::i18n::tf(self.msg_key, &self.args)
    }

    pub fn area_name(&self) -> &'static str {
        crate::i18n::t(self.area_key)
    }
}

#[derive(Clone, Debug, Default)]
pub struct StabOut {
    pub comps: Vec<Comp>,
    /// Gövde burnu datumunda (burundan geriye, m).
    pub cg_nose: Cg,
    /// MAC hücum kenarının burundan uzaklığı [m] = x_root_le + x_LE_MAC.
    pub datum: f64,
    pub np: NpOut,
    pub x_np_nose: f64,
    pub x_cg_rel: f64,
    pub sm: f64,
    pub band: (f64, f64),
    pub in_band: bool,
    pub x_cg_target_rel: f64,
    pub x_cg_target_nose: f64,
    /// İlk 'Batarya' bağlantılı satır (varsa).
    pub batt_row: Option<usize>,
    /// Tüm 'Batarya' bağlantılı satırlar; kütleleri birlikte çözülür, birlikte kaydırılır.
    pub batt_rows: Vec<usize>,
    pub batt_x_nose: f64,
    pub batt_x_shift: f64,
    pub mass_diff_frac: f64,
}

#[derive(Clone)]
pub struct Model {
    pub cell: Cell,
    pub atmo: Atmo,
    pub atmo_msl: Atmo,
    pub weight: WeightOut,
    pub wing: WingOut,
    pub tail: TailOut,
    pub aero_in: AeroIn,
    pub aero_geom: AeroGeom,
    pub aero: AeroOut,
    pub ws: f64,
    pub cons_in: ConstraintIn,
    pub cons: ConstraintOut,
    pub perf_in: PerfIn,
    pub perf_cr: PerfPoint,
    pub perf_end: PerfPoint,
    pub perf_range: PerfPoint,
    pub prop_in: PropIn,
    pub prop: PropOut,
    pub stab: StabOut,
    pub struct_in: StructIn,
    pub st: StructOut,
    pub re_tip: f64,
    pub fuse_wet_geo: f64,
    /// Cl_max_2d = 1.00 olsaydı V_stall (hassasiyet uyarısı için).
    pub v_stall_cl2d_1: f64,
    pub mr: multirotor::MultirotorOut,
    pub mr_st: mr_structure::MrStructureOut,
    pub inputs_ok: bool,
    pub warnings: Vec<Warn>,
}

pub fn effective_mass(link: Link, manual: f64, p: &Project, w: &WeightOut) -> f64 {
    match link {
        Link::None => manual,
        Link::Payload => p.m_payload,
        Link::Avionics => p.m_avionics,
        Link::Propulsion => p.m_propulsion,
        Link::Battery => w.m_battery,
        Link::Wing => w.m_wing,
        Link::FuseTail => p.m_fuse_tail,
    }
}

fn push(
    v: &mut Vec<Warn>,
    sev: Sev,
    area_key: &'static str,
    msg_key: &'static str,
    args: Vec<String>,
) {
    v.push(Warn {
        sev,
        area_key,
        msg_key,
        args,
    });
}

pub fn evaluate(p: &Project, lib: &Library) -> Model {
    let (cell, n_series, n_parallel, pack_overhead) = match p.battery_source {
        custom_battery::BatterySourceKind::Library => (
            lib.cell(&p.cell_name),
            p.n_series,
            p.n_parallel,
            p.pack_overhead,
        ),
        custom_battery::BatterySourceKind::GenericTemplate => {
            let c = &p.generic_cell;
            (
                Cell {
                    id: String::new(),
                    name: c.name.clone(),
                    chemistry: c.chemistry.clone(),
                    nominal_v: c.nominal_v,
                    capacity_ah: c.capacity_ah,
                    mass_g: c.mass_g,
                    max_cont_a: c.max_cont_a,
                    source: "Generic Template".into(),
                    source_url: String::new(),
                    source_date: String::new(),
                    verified_by: "system".into(),
                },
                p.n_series,
                p.n_parallel,
                p.pack_overhead,
            )
        }
        custom_battery::BatterySourceKind::CustomCell => {
            let c = &p.custom_cell;
            (
                Cell {
                    id: String::new(),
                    name: c.name.clone(),
                    chemistry: c.chemistry.clone(),
                    nominal_v: c.nominal_v,
                    capacity_ah: c.capacity_ah,
                    mass_g: c.mass_g,
                    max_cont_a: c.max_cont_a,
                    source: "Custom Cell".into(),
                    source_url: String::new(),
                    source_date: String::new(),
                    verified_by: "user".into(),
                },
                p.n_series,
                p.n_parallel,
                p.pack_overhead,
            )
        }
        custom_battery::BatterySourceKind::CustomPack => {
            let pack = &p.custom_pack;
            let s = pack.s_count.max(1);
            let cap_ah = pack.capacity_ah();
            let v_cell = pack.pack_voltage / (s as f64);
            let m_cell_g = pack.pack_mass_g / (s as f64);
            let max_a = if pack.max_cont_a > 0.0 {
                pack.max_cont_a
            } else {
                pack.c_rating * cap_ah
            };
            (
                Cell {
                    id: String::new(),
                    name: pack.name.clone(),
                    chemistry: pack.chemistry.clone(),
                    nominal_v: v_cell,
                    capacity_ah: cap_ah,
                    mass_g: m_cell_g,
                    max_cont_a: max_a,
                    source: "Custom Pack".into(),
                    source_url: String::new(),
                    source_date: String::new(),
                    verified_by: "user".into(),
                },
                s,
                1,
                1.0,
            )
        }
    };
    let atmo = isa(p.h);
    let atmo_msl = isa(0.0);

    let weight = weight::weight(&WeightIn {
        m_payload: p.m_payload,
        m_avionics: p.m_avionics,
        m_propulsion: p.m_propulsion,
        m_fuse_tail: p.m_fuse_tail,
        cell: cell.clone(),
        n_series,
        n_parallel,
        pack_overhead,
        s: p.s,
        tc_w: p.tc_w,
        t_wall: p.t_wall,
        rho_mat: p.rho_mat,
        k_infill: p.k_infill,
        rho_cf: p.rho_cf,
        d_o: p.d_spar_o,
        d_i: p.d_spar_i,
        l_spar: p.l_spar,
    });
    let wing = geometry::wing(&WingIn {
        b: p.b,
        s: p.s,
        taper: p.taper,
        sweep_le_deg: p.sweep_le_deg,
    });
    let tail = geometry::tail(&TailIn {
        kind: p.tail_kind,
        s_h: p.s_h,
        s_v: p.s_v,
        s_vtail: p.s_vtail,
        gamma_v_deg: p.gamma_v_deg,
        l_h: p.l_h,
        l_v: p.l_v,
        vh_target: p.vh_target,
        vv_target: p.vv_target,
        s: p.s,
        b: p.b,
        mac: wing.mac,
    });
    let aero_geom = AeroGeom {
        ar: wing.ar,
        mac: wing.mac,
        s: p.s,
        a_wet_w: weight.a_wet_w,
        tan_sweep_half: wing.tan_sweep_half,
    };
    let aero_in = AeroIn {
        cl_alpha_2d: p.cl_alpha_2d,
        cl_max_2d: p.cl_max_2d,
        tc_w: p.tc_w,
        tc_t: p.tc_t,
        l_fuse: p.l_fuse,
        d_fuse: p.d_fuse,
        s_wet_fuse: p.s_wet_fuse,
        s_t: tail.s_t,
        c_tail: p.c_tail,
        cd_misc: p.cd_misc,
        q_tail: p.q_tail,
    };
    let aero = aero::aero(&aero_geom, &aero_in, &atmo, p.v_cr, weight.w);
    let ws = weight.w / p.s;

    let cons_in = ConstraintIn {
        v_stall_req: p.v_stall_req,
        v_cr: p.v_cr,
        rc: p.rc,
        v_y: p.v_y,
        phi_turn_deg: p.phi_turn_deg,
        v_turn: p.v_turn,
        v_launch: p.v_launch,
        gamma_launch_deg: p.gamma_launch_deg,
        a_launch: p.a_launch,
        eta_p_cr: p.eta_p_cr,
        eta_p_climb: p.eta_p_climb,
        eta_mot: p.eta_mot,
        eta_esc: p.eta_esc,
        margin: p.pw_margin,
        ws_margin: p.ws_margin,
        cl_max: aero.cl_max,
        cd0: aero.cd0,
        k: aero.k,
        rho_cr: atmo.rho,
        rho_msl: RHO0,
        p_installed_elec: p.p_max_elec,
        rec_criterion: p.rec_criterion,
    };
    let cons = constraint::constraint(ws, weight.w, &cons_in);

    let perf_in = PerfIn {
        e_pack_wh: weight.pack_wh,
        dod: p.dod,
        eta_esc: p.eta_esc,
        eta_mot: p.eta_mot,
        eta_prop: p.eta_p_cr,
        p_hotel: p.p_hotel,
        cd0: aero.cd0,
        k: aero.k,
        rho: atmo.rho,
        ws,
        w: weight.w,
    };
    let perf_cr = performance::at_speed(p.v_cr, &perf_in);
    let perf_end = performance::at_speed(aero.v_end, &perf_in);
    let perf_range = performance::at_speed(aero.v_range, &perf_in);

    let prop_in = PropIn {
        w: weight.w,
        ld_cr: perf_cr.ld,
        v_cr: p.v_cr,
        eta_p_cr: p.eta_p_cr,
        tw_static: p.tw_static,
        d_prop: p.d_prop,
        m_tip_max: p.m_tip_max,
        fm: p.fm,
        eta_mot: p.eta_mot,
        eta_esc: p.eta_esc,
        rho: atmo.rho,
        a_sound: atmo.a,
        p_elec_design: cons.p_elec,
        motor_margin: p.motor_margin,
        p_installed_elec: p.p_max_elec,
        pitch: p.prop_pitch,
        slip: p.prop_slip,
        pack_voltage: weight.pack_voltage,
        pack_capacity_ah: weight.pack_capacity_ah,
        pack_max_cont_a: n_parallel as f64 * cell.max_cont_a,
    };
    let prop = propulsion::propulsion(&prop_in);

    // --- Stabilite (burun datumu -> MAC datumu)
    let datum = p.x_root_le + wing.x_le_mac;
    // Aynı bağlantıya sahip birden çok satır varsa, o kütle satırlar arasında bölünür (toplam kütle korunur).
    let link_count = |link: Link| {
        p.components
            .iter()
            .filter(|c| c.link == link)
            .count()
            .max(1) as f64
    };
    let comps_nose: Vec<Comp> = p
        .components
        .iter()
        .map(|c| {
            let share = if c.link == Link::None {
                1.0
            } else {
                link_count(c.link)
            };
            Comp {
                name: c.name.clone(),
                mass: effective_mass(c.link, c.mass, p, &weight) / share,
                x: c.x,
                y: c.y,
                z: c.z,
            }
        })
        .collect();
    let cg_nose = stability::cg(&comps_nose);
    let np = stability::neutral_point(&NpIn {
        kind: p.tail_kind,
        cla_w: aero.cla_w,
        ar: wing.ar,
        mac: wing.mac,
        s: p.s,
        s_h_eff: tail.s_h_eff,
        l_h: p.l_h,
        cla_h: p.cla_h,
        eta_h: p.eta_h,
        x_ac_frac: p.x_ac_frac,
    });
    let x_cg_rel = cg_nose.x - datum;
    let sm = stability::static_margin(np.x_np, x_cg_rel, wing.mac);
    let band = stability::sm_band(p.tail_kind);
    let x_target_rel = stability::x_cg_target(np.x_np, p.sm_target, wing.mac);
    // Tüm 'Batarya' satırları tek kütle olarak çözülür: toplam kütle, ağırlık merkezi birlikte hedefe konur.
    let batt_rows: Vec<usize> = p
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.link == Link::Battery)
        .map(|(i, _)| i)
        .collect();
    let batt_row = batt_rows.first().copied();
    let (mut batt_x_nose, mut batt_shift) = (f64::NAN, f64::NAN);
    if !batt_rows.is_empty() {
        let is_batt = |i: usize| batt_rows.contains(&i);
        let m_b: f64 = batt_rows.iter().map(|&i| comps_nose[i].mass).sum();
        let m_fixed: f64 = comps_nose
            .iter()
            .enumerate()
            .filter(|(i, _)| !is_batt(*i))
            .map(|(_, c)| c.mass)
            .sum();
        let smx: f64 = comps_nose
            .iter()
            .enumerate()
            .filter(|(i, _)| !is_batt(*i))
            .map(|(_, c)| c.mass * (c.x - datum))
            .sum();
        let xb_rel = stability::battery_x(m_fixed, smx, m_b, x_target_rel);
        batt_x_nose = xb_rel + datum;
        // Mevcut batarya ağırlık merkezinden gereken kayma; tüm satırlar bu kadar kaydırılır.
        let x_b_now: f64 = batt_rows
            .iter()
            .map(|&i| comps_nose[i].mass * comps_nose[i].x)
            .sum::<f64>()
            / m_b;
        batt_shift = batt_x_nose - x_b_now;
    }
    let stab = StabOut {
        mass_diff_frac: (cg_nose.mass - weight.m_mtow) / weight.m_mtow,
        comps: comps_nose.clone(),
        cg_nose,
        datum,
        np,
        x_np_nose: np.x_np + datum,
        x_cg_rel,
        sm,
        band,
        in_band: sm >= band.0 && sm <= band.1,
        x_cg_target_rel: x_target_rel,
        x_cg_target_nose: x_target_rel + datum,
        batt_row,
        batt_rows,
        batt_x_nose,
        batt_x_shift: batt_shift,
    };

    let struct_in = StructIn {
        n_limit: p.n_limit,
        fs: p.fs,
        sigma_allow: p.sigma_allow,
        d_o: p.d_spar_o,
        d_i: p.d_spar_i,
        b: p.b,
        w: weight.w,
    };
    let st = structure::structure(&struct_in);

    let re_tip = atmosphere::reynolds(atmo.rho, p.v_cr, wing.c_t, atmo.mu);
    let fuse_wet_geo = aero::fuse_wetted_geometric(p.l_fuse, p.d_fuse);
    let v_stall_cl2d_1 = aero::v_stall(ws, atmo.rho, 0.9 * 1.0);

    let mr_comps = p.mr_components.iter().map(|c| (c.mass, c.x, c.y)).collect();
    let mr_in = multirotor::MultirotorIn {
        d_frame: p.mr_frame_size,
        arm_count: p.mr_arm_count,
        layout: p.mr_layout,
        coaxial: p.mr_coaxial,
        prop_clearance: p.mr_prop_clearance,
        d_prop: p.mr_d_prop,
        r_plate: p.mr_r_plate,
        m_frame: p.mr_m_frame,
        m_motor: p.mr_m_motor,
        m_esc_tot: p.mr_m_esc_tot,
        m_fc: p.mr_m_fc,
        m_battery: weight.m_battery,
        m_payload: p.m_payload,
        m_misc: p.mr_m_misc,
        pack_wh: weight.pack_wh,
        dod: p.dod,
        pack_voltage: weight.pack_voltage,
        pack_max_cont_a: (n_parallel as f64) * cell.max_cont_a,
        t_max_motor: p.mr_t_max_motor,
        fm: p.fm,
        eta_motor: p.eta_mot,
        eta_esc: p.eta_esc,
        profile_power_ratio: p.mr_profile_power_ratio,
        coax_kappa: p.mr_coax_kappa,
        h_lx: p.mr_h_lx,
        h_wy: p.mr_h_wy,
        flat_plate_area: p.mr_flat_plate_area,
        p_avionics: p.p_hotel,
        v_cruise: p.mr_v_cruise,
        rho: atmo.rho,
        comps_mass_x_y: mr_comps,
    };
    let mr = multirotor::evaluate_multirotor(&mr_in);

    // Multirotor askı RPM tahmini / kullanıcı değeri
    let rpm_hover = if p.mr_hover_rpm_override.is_finite() && p.mr_hover_rpm_override > 0.0 {
        p.mr_hover_rpm_override
    } else if p.mr_d_prop > 0.0 && mr.t_hover_motor > 0.0 && atmo.rho > 0.0 {
        let ct = 0.10;
        let d4 = p.mr_d_prop.powi(4);
        let n_sq = mr.t_hover_motor / (ct * atmo.rho * d4);
        if n_sq > 0.0 {
            n_sq.sqrt() * 60.0
        } else {
            5000.0
        }
    } else {
        5000.0
    };

    let mr_struct_in = mr_structure::MrStructureIn {
        d_frame: p.mr_frame_size,
        r_plate: p.mr_r_plate,
        l_arm_override: p.mr_arm_l_override,
        arm_count: p.mr_arm_count,
        coaxial: p.mr_coaxial,
        t_max_motor: p.mr_t_max_motor,
        t_hover_motor: mr.t_hover_motor,
        m_motor: p.mr_m_motor,
        n_limit: p.mr_arm_n_limit,
        fs: p.mr_arm_fs,
        section_type: p.mr_arm_section,
        d_o: p.mr_arm_d_o,
        d_i: p.mr_arm_d_i,
        sigma_allow: p.mr_arm_sigma_allow,
        e_modulus: p.mr_arm_e_modulus,
        rho: p.mr_arm_rho,
        rpm_hover,
        prop_blades: p.mr_prop_blades,
    };
    let mr_st = mr_structure::mr_structure(&mr_struct_in);

    let mut m = Model {
        cell,
        atmo,
        atmo_msl,
        weight,
        wing,
        tail,
        aero_in,
        aero_geom,
        aero,
        ws,
        cons_in,
        cons,
        perf_in,
        perf_cr,
        perf_end,
        perf_range,
        prop_in,
        prop,
        stab,
        struct_in,
        st,
        re_tip,
        fuse_wet_geo,
        v_stall_cl2d_1,
        mr,
        mr_st,
        inputs_ok: true,
        warnings: vec![],
    };
    m.warnings = build_warnings(p, &mut m);
    m
}

fn build_warnings(p: &Project, m: &mut Model) -> Vec<Warn> {
    let mut v = Vec::new();
    let finite_pos = |x: f64| x.is_finite() && x > 0.0;
    let ok = if p.drone_type == crate::project::DroneType::Multirotor {
        finite_pos(p.mr_frame_size)
            && p.mr_arm_count >= 3
            && finite_pos(m.mr.m_mtow)
            && finite_pos(p.mr_d_prop)
    } else {
        finite_pos(p.b)
            && finite_pos(p.s)
            && finite_pos(p.v_cr)
            && finite_pos(m.weight.m_mtow)
            && finite_pos(m.aero.cd0)
            && finite_pos(m.aero.k)
            && (0.0..=1.0).contains(&p.taper)
            && p.taper > 0.0
    };
    m.inputs_ok = ok;
    if !ok {
        push(
            &mut v,
            Sev::Err,
            "area.general",
            "warn.general.invalid_input",
            vec![],
        );
        return v;
    }
    let a = &m.aero;
    let c = &m.cons;
    let w = &m.weight;

    // --- Genel / Görev
    if a.mach > 0.15 {
        push(
            &mut v,
            Sev::Warn,
            "area.mission",
            "warn.mission.mach_high",
            vec![format!("{:.2}", a.mach)],
        );
    }
    if a.re_wing < 5e5 {
        push(
            &mut v,
            Sev::Info,
            "area.mission",
            "warn.mission.re_wing_low",
            vec![format!("{:.2e}", a.re_wing)],
        );
    }
    // --- Ağırlık
    if !(0.25..=0.45).contains(&w.f_batt) {
        push(
            &mut v,
            Sev::Warn,
            "area.weight",
            "warn.weight.batt_ratio",
            vec![format!("{:.1}", w.f_batt * 100.0)],
        );
    }
    // Batarya enerji yoğunluğu gerçekçilik kontrolü
    let wh_kg = w.e_pack_wh_kg;
    let chem = m.cell.chemistry.to_lowercase();
    if chem.contains("lipo") || chem.contains("li-po") {
        if wh_kg > 220.0 {
            push(
                &mut v,
                Sev::Warn,
                "area.weight",
                "warn.weight.lipo_energy_density_high",
                vec![format!("{:.0}", wh_kg)],
            );
        }
    } else if chem.contains("ion") || chem.contains("solid") {
        if wh_kg > 300.0 {
            push(
                &mut v,
                Sev::Warn,
                "area.weight",
                "warn.weight.liion_energy_density_high",
                vec![format!("{:.0}", wh_kg)],
            );
        }
    }
    if wh_kg > 0.0 && wh_kg < 50.0 {
        push(
            &mut v,
            Sev::Warn,
            "area.weight",
            "warn.weight.energy_density_low",
            vec![format!("{:.0}", wh_kg)],
        );
    }
    if p.t_wall < 0.00035 {
        push(
            &mut v,
            Sev::Err,
            "area.weight",
            "warn.weight.wall_thickness_low",
            vec![],
        );
    }
    let mat_id = if !p.material_id.is_empty() {
        weight::resolve_material_id(&p.material_id)
    } else {
        weight::resolve_material_id(&p.material_name)
    };
    if mat_id == "lw_pla_foamed" {
        push(
            &mut v,
            Sev::Info,
            "area.weight",
            "warn.weight.lw_pla_uncertainty",
            vec![],
        );
    }
    if mat_id == "pla" || mat_id.starts_with("lw_pla") {
        push(
            &mut v,
            Sev::Info,
            "area.weight",
            "warn.weight.pla_temp_warning",
            vec![],
        );
    }
    if p.l_spar > p.b {
        push(
            &mut v,
            Sev::Err,
            "area.weight",
            "warn.weight.spar_exceeds_span",
            vec![],
        );
    } else if p.l_spar < 0.8 * p.b {
        push(
            &mut v,
            Sev::Warn,
            "area.weight",
            "warn.weight.spar_too_short",
            vec![],
        );
    }
    // --- Geometri
    if m.wing.ar < 4.0 {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.ar_low",
            vec![format!("{:.1}", m.wing.ar)],
        );
    }
    if m.re_tip < 1e5 {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.re_tip_low",
            vec![
                format!("{:.0}", m.wing.c_t * 1000.0),
                format!("{:.1e}", m.re_tip),
            ],
        );
    }
    let ws = m.ws;
    if !(45.0..=100.0).contains(&ws) {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.ws_hand_launch",
            vec![format!("{:.1}", ws)],
        );
    }
    if p.tc_w < 0.08 {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.tc_w_thin",
            vec![],
        );
    }
    if p.tc_w > 0.14 {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.tc_w_thick",
            vec![],
        );
    }
    if p.tail_kind == TailKind::VTail && !(30.0..=40.0).contains(&p.gamma_v_deg) {
        push(
            &mut v,
            Sev::Warn,
            "area.geometry",
            "warn.geometry.vtail_dihedral",
            vec![],
        );
    }
    if p.tail_kind == TailKind::Conventional {
        if m.tail.v_h < 0.35 || m.tail.v_h > 0.8 {
            push(
                &mut v,
                Sev::Info,
                "area.geometry",
                "warn.geometry.v_h_out_of_range",
                vec![format!("{:.2}", m.tail.v_h)],
            );
        }
        if m.tail.v_v < 0.02 || m.tail.v_v > 0.06 {
            push(
                &mut v,
                Sev::Info,
                "area.geometry",
                "warn.geometry.v_v_out_of_range",
                vec![format!("{:.3}", m.tail.v_v)],
            );
        }
    }
    if p.tail_kind == TailKind::FlyingWing {
        push(
            &mut v,
            Sev::Info,
            "area.geometry",
            "warn.geometry.flying_wing_np_estimate",
            vec![],
        );
    }

    if p.drone_type.has_wing() {
        // --- Aero
        push(
            &mut v,
            Sev::Info,
            "area.aero",
            "warn.aero.c_tail_assumption",
            vec![format!("{:.0}", p.c_tail * 1000.0)],
        );
        if p.cl_max_2d > 1.15 {
            push(
                &mut v,
                Sev::Warn,
                "area.aero",
                "warn.aero.cl_max_2d_optimistic",
                vec![
                    format!("{:.2}", p.cl_max_2d),
                    format!("{:.1}", m.v_stall_cl2d_1),
                    format!("{:.1}", p.v_stall_req),
                ],
            );
        }
        if p.s_wet_fuse < 0.95 * m.fuse_wet_geo {
            push(
                &mut v,
                Sev::Warn,
                "area.aero",
                "warn.aero.s_wet_fuse_low",
                vec![
                    format!("{:.3}", p.s_wet_fuse),
                    format!("{:.3}", m.fuse_wet_geo),
                    format!("{:.0}", (1.0 - p.s_wet_fuse / m.fuse_wet_geo) * 100.0),
                ],
            );
        }
        let cl_cr = aero::cl_for(ws, m.atmo.rho, p.v_cr);
        if cl_cr > 0.85 * a.cl_max {
            push(
                &mut v,
                Sev::Err,
                "area.aero",
                "warn.aero.cl_cruise_near_stall",
                vec![format!("{:.2}", cl_cr)],
            );
        }
        if a.end_clamped {
            push(
                &mut v,
                Sev::Info,
                "area.aero",
                "warn.aero.end_clamped",
                vec![
                    format!("{:.2}", a.cl_end_unclamped),
                    format!("{:.1}", a.v_min_loiter),
                    format!("{:.1}", a.v_end_unclamped),
                    format!("{:.2}", a.cl_end),
                    format!("{:.1}", a.ld_end),
                ],
            );
        }
        // --- Tasarım noktası
        if !c.stall_ok {
            push(
                &mut v,
                Sev::Err,
                "area.design_point",
                "warn.design_point.stall_not_satisfied",
                vec![
                    format!("{:.1}", c.v_stall),
                    format!("{:.1}", p.v_stall_req),
                    format!("{:.1}", c.ws),
                    format!("{:.1}", c.ws_allowed),
                ],
            );
        }
        if !c.power_ok {
            push(
                &mut v,
                Sev::Err,
                "area.design_point",
                "warn.design_point.power_insufficient",
                vec![
                    format!("{:.2}", c.installed_pw),
                    crate::i18n::t(constraint::NAMES_KEYS[c.critical]).to_string(),
                    format!("{:.2}", c.pw_req),
                ],
            );
        }
        if !m.prop.installed_ok {
            push(
                &mut v,
                Sev::Err,
                "area.design_point",
                "warn.design_point.static_takeoff_shortfall",
                vec![
                    format!("{:.0}", m.prop.p_elec_static.max(c.p_elec)),
                    format!("{:.0}", m.prop.p_elec_static),
                    format!("{:.0}", c.p_elec),
                    format!("{:.0}", p.p_max_elec),
                ],
            );
        }
        if c.power_ok && m.prop.installed_ok && c.installed_pw < c.pw_design {
            push(
                &mut v,
                Sev::Warn,
                "area.design_point",
                "warn.design_point.power_margin_shortfall",
                vec![
                    format!("{:.0}", (p.pw_margin - 1.0) * 100.0),
                    format!("{:.2}", c.installed_pw),
                    format!("{:.2}", c.pw_design),
                ],
            );
        }
        if p.ws_margin >= 1.0 {
            push(
                &mut v,
                Sev::Info,
                "area.design_point",
                "warn.design_point.ws_margin_info",
                vec![],
            );
        }
        // --- Havada kalma
        for (nm_key, pt) in [
            ("speed.cruise", &m.perf_cr),
            ("speed.best_range", &m.perf_range),
            ("speed.best_endurance", &m.perf_end),
        ] {
            if pt.cl > 0.85 * a.cl_max {
                push(
                    &mut v,
                    Sev::Warn,
                    "area.range",
                    "warn.range.cl_optimistic",
                    vec![crate::i18n::t(nm_key).to_string(), format!("{:.2}", pt.cl)],
                );
            }
        }
        push(
            &mut v,
            Sev::Info,
            "area.range",
            "warn.range.prop_efficiency_fixed",
            vec![],
        );
        // --- İtki
        if !m.prop.rpm_ok {
            push(
                &mut v,
                Sev::Warn,
                "area.propulsion",
                "warn.propulsion.rpm_limit_exceeded",
                vec![
                    format!("{:.0}", m.prop.rpm_cr),
                    format!("{:.0}", m.prop.rpm_max),
                ],
            );
        }
        if !m.prop.current_ok {
            push(
                &mut v,
                Sev::Err,
                "area.propulsion",
                "warn.propulsion.current_limit_exceeded",
                vec![
                    format!("{:.0}", m.prop.i_peak),
                    format!("{:.0}", m.prop_in.pack_max_cont_a),
                ],
            );
        }
        if p.d_prop < 0.18 || p.d_prop > 0.30 {
            push(
                &mut v,
                Sev::Warn,
                "area.propulsion",
                "warn.propulsion.prop_diameter_unusual",
                vec![],
            );
        }
        // --- Stabilite
        if m.stab.batt_row.is_none() {
            push(
                &mut v,
                Sev::Info,
                "area.stability",
                "warn.stability.no_battery_row",
                vec![],
            );
        }
        if m.stab.batt_rows.len() > 1 {
            push(
                &mut v,
                Sev::Info,
                "area.stability",
                "warn.stability.multi_battery_rows",
                vec![m.stab.batt_rows.len().to_string()],
            );
        }
        if m.stab.mass_diff_frac.abs() > 0.03 {
            push(
                &mut v,
                Sev::Warn,
                "area.stability",
                "warn.stability.mass_difference",
                vec![
                    format!("{:.0}", m.stab.cg_nose.mass * 1000.0),
                    format!("{:.0}", w.m_mtow * 1000.0),
                    format!("{:.1}", m.stab.mass_diff_frac * 100.0),
                ],
            );
        }
        if !m.stab.sm.is_finite() {
            push(
                &mut v,
                Sev::Err,
                "area.stability",
                "warn.stability.sm_invalid",
                vec![],
            );
        } else if !m.stab.in_band {
            push(
                &mut v,
                Sev::Err,
                "area.stability",
                if m.stab.sm < m.stab.band.0 {
                    "warn.stability.sm_low_move_forward"
                } else {
                    "warn.stability.sm_high_move_backward"
                },
                vec![
                    format!("{:.1}", m.stab.sm * 100.0),
                    format!("{:.0}", m.stab.band.0 * 100.0),
                    format!("{:.0}", m.stab.band.1 * 100.0),
                ],
            );
        }
    }
    // --- Yapı (Sabit Kanat & VTOL kanat kirişi)
    if p.drone_type.has_wing() {
        if m.st.ms < 0.0 {
            push(
                &mut v,
                Sev::Err,
                "area.structure",
                "warn.structure.spar_stress_exceeded",
                vec![
                    format!("{:.0}", m.st.sigma_max / 1e6),
                    format!("{:.0}", p.sigma_allow / 1e6),
                    format!("{:.2}", m.st.ms),
                ],
            );
        } else if m.st.ms < 0.2 {
            push(
                &mut v,
                Sev::Warn,
                "area.structure",
                "warn.structure.spar_margin_low",
                vec![format!("{:.2}", m.st.ms)],
            );
        }
        push(
            &mut v,
            Sev::Info,
            "area.structure",
            "warn.structure.spar_bending_assumption",
            vec![],
        );
    }

    // --- Multirotor & VTOL Kol Yapısı
    if p.drone_type.is_multirotor() || p.drone_type == crate::project::DroneType::VtolQuadplane {
        if m.mr_st.ms < 0.0 {
            push(
                &mut v,
                Sev::Err,
                "area.mr_structure",
                "warn.mr_structure.arm_stress_exceeded",
                vec![
                    format!("{:.1}", m.mr_st.sigma_max / 1e6),
                    format!("{:.1}", p.mr_arm_sigma_allow / 1e6),
                    format!("{:.2}", m.mr_st.ms),
                ],
            );
        } else if m.mr_st.ms < 0.2 {
            push(
                &mut v,
                Sev::Warn,
                "area.mr_structure",
                "warn.mr_structure.arm_margin_low",
                vec![format!("{:.2}", m.mr_st.ms)],
            );
        }
        if !m.mr_st.deflection_ok {
            push(
                &mut v,
                Sev::Warn,
                "area.mr_structure",
                "warn.mr_structure.arm_deflection_high",
                vec![
                    format!("{:.2}", m.mr_st.deflection_max * 1000.0),
                    format!("{:.2}", m.mr_st.deflection_limit * 1000.0),
                ],
            );
        }
        if m.mr_st.resonance_rot_warn {
            push(
                &mut v,
                Sev::Warn,
                "area.mr_structure",
                "warn.mr_structure.resonance_rot",
                vec![
                    format!("{:.1}", m.mr_st.f_natural),
                    format!("{:.1}", m.mr_st.f_rot_hover),
                ],
            );
        }
        if m.mr_st.resonance_bpf_warn {
            push(
                &mut v,
                Sev::Warn,
                "area.mr_structure",
                "warn.mr_structure.resonance_bpf",
                vec![
                    format!("{:.1}", m.mr_st.f_natural),
                    format!("{:.1}", m.mr_st.f_bpf_hover),
                ],
            );
        }
    }

    // --- Multirotor & VTOL Uyarıları
    if p.drone_type.is_multirotor() {
        if m.mr.prop_overlap {
            push(
                &mut v,
                Sev::Err,
                "area.multirotor",
                "warn.mr.prop_overlap",
                vec![],
            );
        } else if !m.mr.prop_clearance_ok {
            push(
                &mut v,
                Sev::Warn,
                "area.multirotor",
                "warn.mr.prop_clearance_low",
                vec![],
            );
        }
        if m.mr.tw_ratio < 1.6 && m.mr.tw_ratio > 0.0 {
            push(
                &mut v,
                Sev::Err,
                "area.multirotor",
                "warn.mr.tw_low",
                vec![format!("{:.2}", m.mr.tw_ratio)],
            );
        }
        if m.mr.hover_throttle > 0.60 {
            push(
                &mut v,
                Sev::Warn,
                "area.multirotor",
                "warn.mr.hover_throttle_high",
                vec![format!("{:.0}", m.mr.hover_throttle * 100.0)],
            );
        }
        if !m.mr.i_hover_ok {
            push(
                &mut v,
                Sev::Err,
                "area.multirotor",
                "warn.mr.hover_current_high",
                vec![format!("{:.1}", m.mr.i_hover)],
            );
        }
        if !m.mr.cg_offset_ok {
            push(
                &mut v,
                Sev::Warn,
                "area.multirotor",
                "warn.mr.cg_offset_high",
                vec![format!("{:.1}", m.mr.cg_offset_d * 1000.0)],
            );
        }
    }
    v
}

#[cfg(test)]
mod model_tests {
    use super::*;
    #[test]
    fn default_design_is_consistent() {
        let m = evaluate(&Project::default(), &Library::embedded());
        println!(
            "MTOW {:.4} cd0 {:.5} vstall {:.2} sm {:.3} batt_x {:.1} mm shift {:.1} mm",
            m.weight.m_mtow,
            m.aero.cd0,
            m.cons.v_stall,
            m.stab.sm,
            m.stab.batt_x_nose * 1000.0,
            m.stab.batt_x_shift * 1000.0
        );
        println!(
            "end {:.0} min / range {:.1} km (cruise)  best-end {:.0} min  range-best {:.1} km",
            m.perf_cr.endurance_min(),
            m.perf_cr.range_km(),
            m.perf_end.endurance_min(),
            m.perf_range.range_km()
        );
        println!(
            "pelec design {:.0} suggested {:.0} rec ws {:.1} pw {:.2}",
            m.cons.p_elec, m.prop.p_motor_suggest, m.cons.rec_ws, m.cons.rec_pw
        );
        for w in &m.warnings {
            println!("{:?} [{}] {}", w.sev, w.area_name(), w.text());
        }
        assert!((m.weight.m_mtow - 2.0).abs() / 2.0 < 0.01);
        assert!(m.stab.in_band, "varsayılan SM bantta olmalı: {}", m.stab.sm);
        assert!(
            m.warnings.iter().all(|w| w.sev != Sev::Err),
            "varsayılan tasarımda hata uyarısı olmamalı"
        );
        // Başka bir deneme: bataryayı önerilen konuma koyunca SM = hedef
        let mut p = Project::default();
        let r = m.stab.batt_row.unwrap();
        p.components[r].x = m.stab.batt_x_nose;
        let m2 = evaluate(&p, &Library::embedded());
        assert!((m2.stab.sm - p.sm_target).abs() < 1e-9);
    }
    fn has_err(m: &Model, needle: &str) -> bool {
        m.warnings
            .iter()
            .any(|w| w.sev == Sev::Err && w.text().contains(needle))
    }
    #[test]
    fn static_power_shortfall_is_error_even_if_flight_ok() {
        let mut p = Project::default();
        p.p_max_elec = 200.0; // seyir/tırmanış yeterli, statik kalkış (≈224 W) değil
        let m = evaluate(&p, &Library::embedded());
        assert!(m.cons.power_ok, "uçuş gücü yeterli olmalı");
        assert!(!m.prop.installed_ok);
        assert!(
            has_err(&m, "statik kalkış"),
            "statik güç yetersizliği hata olarak raporlanmalı"
        );
    }
    #[test]
    fn rpm_warning_advises_larger_pitch_or_smaller_diameter() {
        let mut p = Project::default();
        p.prop_pitch = 0.08; // seyir devri uç Mach sınırını aşar
        let m = evaluate(&p, &Library::embedded());
        assert!(!m.prop.rpm_ok);
        let w = m
            .warnings
            .iter()
            .find(|w| w.area_key == "area.propulsion" && w.text().contains("devir"))
            .expect("devir uyarısı");
        assert!(w.text().contains("daha büyük adımlı"), "{}", w.text());
        assert!(w.text().contains("daha küçük çaplı"), "{}", w.text());
        assert!(!w.text().contains("küçük adımlı"), "{}", w.text());
    }
    #[test]
    fn spar_longer_than_span_is_error() {
        let mut p = Project::default();
        p.l_spar = p.b + 0.1;
        let m = evaluate(&p, &Library::embedded());
        assert!(has_err(&m, "kanat açıklığını aşıyor"));
    }
    #[test]
    fn multiple_battery_rows_share_mass_and_move_together() {
        let lib = Library::embedded();
        let m1 = evaluate(&Project::default(), &lib);
        let mut p = Project::default();
        let r = m1.stab.batt_row.unwrap();
        let mut extra = p.components[r].clone();
        extra.name = "Batarya 2".into();
        extra.x = 0.25;
        p.components.push(extra);
        let m2 = evaluate(&p, &lib);
        assert_eq!(m2.stab.batt_rows.len(), 2);
        // Batarya kütlesi iki kez sayılmamalı: toplam AG kütlesi değişmez.
        assert!(
            (m2.stab.cg_nose.mass - m1.stab.cg_nose.mass).abs() < 1e-9,
            "{} vs {}",
            m2.stab.cg_nose.mass,
            m1.stab.cg_nose.mass
        );
        // Tüm batarya satırları birlikte kaydırılınca SM hedefe oturur.
        let shift = m2.stab.batt_x_shift;
        for &row in &m2.stab.batt_rows {
            p.components[row].x += shift;
        }
        let m3 = evaluate(&p, &lib);
        assert!((m3.stab.sm - p.sm_target).abs() < 1e-9, "sm {}", m3.stab.sm);
    }
}
