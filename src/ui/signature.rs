// SPDX-License-Identifier: AGPL-3.0-or-later
//! İz ve beka sekmesi (spesifikasyon modül 17): radar kesit alanı (RCS), kızılötesi (IR) iz ve
//! GNSS kaybında konum sapması. Hesaplar `crate::signature` içindedir; bu dosya yalnız ayarları ve
//! sonuçları çizer. Ayarlar proje dosyasına yazılmaz, proje değerleri değiştirilmez.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::signature::drift::{self, Aiding, DriftSettings, ImuGrade};
use crate::signature::ir::{self, IrResult, IrSettings, LWIR, MWIR, Phase, SourceKind};
use crate::signature::rcs::{self, Airframe, Band, MaterialClass, Part, RcsSettings, Sweep};
use crate::signature::to_db;
use egui::{DragValue, Ui};
use egui_plot::{Bar, BarChart, HLine, Legend, Line, Plot, PlotPoint, PlotPoints, Text};

/// Azimut örnek sayısı (1° adım).
const N_AZ: usize = 360;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Rcs,
    Ir,
    Drift,
}

struct RcsCache {
    key: String,
    settings: RcsSettings,
    airframe: Airframe,
    sweeps: Vec<Sweep>,
}

pub struct SigState {
    view: View,
    band: Band,
    pub rcs: RcsSettings,
    pub ir: IrSettings,
    pub drift: DriftSettings,
    log_scale: bool,
    cache: Option<RcsCache>,
}

impl Default for SigState {
    fn default() -> Self {
        SigState {
            view: View::Rcs,
            band: Band::X,
            rcs: RcsSettings::default(),
            ir: IrSettings::default(),
            drift: DriftSettings::default(),
            log_scale: true,
            cache: None,
        }
    }
}

fn material_label(c: MaterialClass) -> &'static str {
    match c {
        MaterialClass::Metal => t("signature.mat.metal"),
        MaterialClass::Carbon => t("signature.mat.carbon"),
        MaterialClass::CfFilled => t("signature.mat.cf_filled"),
        MaterialClass::Fibreglass => t("signature.mat.fibreglass"),
        MaterialClass::Plastic => t("signature.mat.plastic"),
        MaterialClass::Wood => t("signature.mat.wood"),
        MaterialClass::Foam => t("signature.mat.foam"),
    }
}

fn part_label(p: Part) -> &'static str {
    match p {
        Part::Fuselage => t("signature.part.fuselage"),
        Part::Wing => t("signature.part.wing"),
        Part::LeadingEdge => t("signature.part.leading_edge"),
        Part::Spar => t("signature.part.spar"),
        Part::Tail => t("signature.part.tail"),
        Part::Boom => t("signature.part.boom"),
        Part::Arm => t("signature.part.arm"),
        Part::CenterPlate => t("signature.part.center_plate"),
        Part::Motor => t("signature.part.motor"),
        Part::Propeller => t("signature.part.propeller"),
        Part::Battery => t("signature.part.battery"),
        Part::Gear => t("signature.part.gear"),
    }
}

fn source_label(k: SourceKind) -> &'static str {
    match k {
        SourceKind::Motor => t("signature.ir.src_motor"),
        SourceKind::Esc => t("signature.ir.src_esc"),
        SourceKind::Battery => t("signature.ir.src_battery"),
    }
}

fn grade_label(g: ImuGrade) -> &'static str {
    match g {
        ImuGrade::ConsumerMems => t("signature.drift.grade_consumer"),
        ImuGrade::IndustrialMems => t("signature.drift.grade_industrial"),
        ImuGrade::Tactical => t("signature.drift.grade_tactical"),
    }
}

fn aiding_label(a: Aiding) -> &'static str {
    match a {
        Aiding::None => t("signature.drift.aid_none"),
        Aiding::AirspeedMag => t("signature.drift.aid_airspeed"),
        Aiding::OpticalFlow => t("signature.drift.aid_flow"),
    }
}

/// Üç anlamlı basamak.
fn sig3(x: f64) -> String {
    if !x.is_finite() {
        return "—".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let mag = x.abs().log10().floor() as i32;
    if !(-3..5).contains(&mag) {
        return format!("{x:.2e}");
    }
    let dec = (2 - mag).max(0) as usize;
    format!("{x:.dec$}")
}

fn num(
    ui: &mut Ui,
    label: &str,
    v: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    suffix: &str,
) {
    ui.label(label);
    ui.add(DragValue::new(v).range(range).speed(speed).suffix(suffix));
    ui.end_row();
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut SigState) {
    section_header(ui, t("tab_signature"));
    components::banner(ui, Status::Warn, t("signature.disclaimer"));
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.view, View::Rcs, t("signature.view_rcs"));
        ui.selectable_value(&mut st.view, View::Ir, t("signature.view_ir"));
        ui.selectable_value(&mut st.view, View::Drift, t("signature.view_drift"));
    });
    ui.separator();
    match st.view {
        View::Rcs => show_rcs(ui, p, m, st),
        View::Ir => show_ir(ui, p, m, st),
        View::Drift => show_drift(ui, p, st),
    }
}

// ------------------------------------------------------------------------------------------
// RCS
// ------------------------------------------------------------------------------------------

/// Proje ya da ayar değişince yeniden hesaplanan RCS önbelleği.
fn rcs_cache<'a>(p: &Project, m: &Model, st: &'a mut SigState) -> &'a RcsCache {
    let key = serde_json::to_string(p).unwrap_or_default();
    let fresh = matches!(&st.cache, Some(c) if c.key == key && c.settings == st.rcs);
    if !fresh {
        let airframe = rcs::airframe(p, m, &st.rcs);
        let sweeps = Band::ALL
            .iter()
            .map(|b| rcs::sweep(&airframe, *b, &st.rcs, N_AZ))
            .collect();
        st.cache = Some(RcsCache {
            key,
            settings: st.rcs.clone(),
            airframe,
            sweeps,
        });
    }
    st.cache.as_ref().expect("önbellek dolu")
}

fn rcs_settings(ui: &mut Ui, p: &Project, st: &mut SigState) {
    egui::CollapsingHeader::new(t("signature.settings"))
        .id_salt("sig_rcs_settings")
        .default_open(false)
        .show(ui, |ui| {
            let s = &mut st.rcs;
            egui::Grid::new("sig_rcs_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    num(
                        ui,
                        t("signature.rcs.elevation"),
                        &mut s.elevation_deg,
                        -60.0..=60.0,
                        0.5,
                        " °",
                    );
                    ui.label(t("signature.rcs.skin"));
                    let auto = rcs::material_class_for(if p.material_id.is_empty() {
                        &p.material_name
                    } else {
                        &p.material_id
                    });
                    let cur = match s.skin {
                        Some(c) => material_label(c).to_string(),
                        None => tf("signature.rcs.auto_fmt", &[material_label(auto)]),
                    };
                    egui::ComboBox::from_id_salt("sig_skin")
                        .selected_text(cur)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut s.skin,
                                None,
                                tf("signature.rcs.auto_fmt", &[material_label(auto)]),
                            );
                            for c in MaterialClass::ALL {
                                ui.selectable_value(&mut s.skin, Some(c), material_label(c));
                            }
                        });
                    ui.end_row();
                    ui.label(t("signature.rcs.prop_material"));
                    egui::ComboBox::from_id_salt("sig_prop_mat")
                        .selected_text(material_label(s.prop_material))
                        .show_ui(ui, |ui| {
                            for c in MaterialClass::ALL {
                                ui.selectable_value(&mut s.prop_material, c, material_label(c));
                            }
                        });
                    ui.end_row();
                    ui.label(t("signature.rcs.internal"));
                    ui.checkbox(&mut s.include_internal, "");
                    ui.end_row();
                    num(
                        ui,
                        t("signature.rcs.motor_d"),
                        &mut s.motor_d_mm,
                        0.0..=150.0,
                        0.5,
                        " mm",
                    );
                    num(
                        ui,
                        t("signature.rcs.ref_sigma"),
                        &mut s.ref_sigma_m2,
                        1e-4..=100.0,
                        0.01,
                        " m²",
                    );
                    num(
                        ui,
                        t("signature.rcs.ref_range"),
                        &mut s.ref_range_km,
                        0.1..=500.0,
                        0.1,
                        " km",
                    );
                });
            ui.add_space(space::XS);
            ui.label(egui::RichText::new(t("signature.rcs.refl_title")).strong());
            egui::Grid::new("sig_refl_grid")
                .num_columns(3)
                .show(ui, |ui| {
                    for c in MaterialClass::ALL {
                        ui.label(material_label(c));
                        let r = &mut s.reflectivity[c.index()];
                        ui.add(
                            DragValue::new(r)
                                .range(0.0..=1.0)
                                .speed(0.005)
                                .max_decimals(4),
                        );
                        ui.label(format!("{:.1} dB", to_db(r.max(1e-6))));
                        ui.end_row();
                    }
                });
            if ui.button(t("signature.reset")).clicked() {
                *s = RcsSettings::default();
            }
        });
}

fn polar_plot(ui: &mut Ui, sw: &Sweep) {
    let tk = theme::tokens(ui);
    let db: Vec<f64> = sw.sigma.iter().map(|v| to_db(*v)).collect();
    let max_db = db.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let top = (max_db / 10.0).ceil() * 10.0;
    let floor = top - 40.0;
    let r_of = |d: f64| (d - floor).max(0.0);
    let pts: Vec<[f64; 2]> = sw
        .az_deg
        .iter()
        .zip(&db)
        .map(|(az, d)| {
            let (s, c) = az.to_radians().sin_cos();
            [r_of(*d) * s, r_of(*d) * c]
        })
        .collect();
    Plot::new(("sig_polar", sw.band.short()))
        .height(340.0)
        .data_aspect(1.0)
        .show_axes(false)
        .show_grid(false)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_zoom(false)
        .show(ui, |pu| {
            for k in 1..=4 {
                let r = 10.0 * k as f64;
                let ring: Vec<[f64; 2]> = (0..=72)
                    .map(|i| {
                        let a = i as f64 * 5f64.to_radians();
                        [r * a.sin(), r * a.cos()]
                    })
                    .collect();
                pu.line(
                    Line::new("", PlotPoints::from(ring))
                        .color(tk.border)
                        .width(1.0),
                );
                pu.text(Text::new(
                    "",
                    PlotPoint::new(0.0, r),
                    egui::RichText::new(format!("{:.0}", floor + r)).color(tk.text_weak),
                ));
            }
            for (az, lab) in [
                (0.0, t("signature.rcs.nose")),
                (90.0, t("signature.rcs.right")),
                (180.0, t("signature.rcs.tail")),
                (270.0, t("signature.rcs.left")),
            ] {
                let a: f64 = f64::to_radians(az);
                pu.line(
                    Line::new(
                        "",
                        PlotPoints::from(vec![[0.0, 0.0], [40.0 * a.sin(), 40.0 * a.cos()]]),
                    )
                    .color(tk.border)
                    .width(1.0),
                );
                pu.text(Text::new(
                    "",
                    PlotPoint::new(45.0 * a.sin(), 45.0 * a.cos()),
                    egui::RichText::new(lab).color(tk.text_weak),
                ));
            }
            pu.line(
                Line::new(sw.band.short(), PlotPoints::from(pts))
                    .color(tk.accent)
                    .width(2.0),
            );
        });
    ui.label(
        egui::RichText::new(tf(
            "signature.rcs.polar_note_fmt",
            &[&format!("{floor:.0}"), &format!("{top:.0}")],
        ))
        .weak(),
    );
}

fn show_rcs(ui: &mut Ui, p: &Project, m: &Model, st: &mut SigState) {
    section(ui, t("signature.rcs.title"));
    ui.label(t("signature.rcs.intro"));
    rcs_settings(ui, p, st);
    let band = st.band;
    let (ref_sigma, ref_range) = (st.rcs.ref_sigma_m2, st.rcs.ref_range_km);
    let elevation = st.rcs.elevation_deg;
    let cache = rcs_cache(p, m, st);
    ui.label(
        egui::RichText::new(tf(
            "signature.rcs.model_fmt",
            &[
                &cache.airframe.prims.len().to_string(),
                material_label(cache.airframe.skin),
                &format!("{:.0}", cache.airframe.motor_d_m * 1000.0),
                &cache.airframe.n_motors.to_string(),
                &format!("{elevation:.0}"),
            ],
        ))
        .weak(),
    );

    // Bant özeti.
    section(ui, t("signature.rcs.summary_title"));
    egui::Grid::new("sig_rcs_sum")
        .num_columns(7)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "signature.rcs.col_band",
                "signature.rcs.col_mean",
                "signature.rcs.col_mean_db",
                "signature.rcs.col_front_db",
                "signature.rcs.col_side_db",
                "signature.rcs.col_max_db",
                "signature.rcs.col_range",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for sw in &cache.sweeps {
                ui.label(sw.band.short());
                ui.label(format!("{} m²", sig3(sw.mean)));
                ui.label(format!("{:.1}", sw.mean_dbsm()));
                ui.label(format!("{:.1}", to_db(sw.front)));
                ui.label(format!("{:.1}", to_db(sw.side)));
                ui.label(format!("{:.1}", to_db(sw.max)));
                let rr = rcs::range_ratio(sw.mean, ref_sigma);
                ui.label(format!("{:.0} % · {} km", 100.0 * rr, sig3(rr * ref_range)));
                ui.end_row();
            }
        });
    ui.label(
        egui::RichText::new(tf(
            "signature.rcs.range_note_fmt",
            &[&sig3(ref_sigma), &sig3(ref_range)],
        ))
        .weak(),
    );

    // Kutup grafiği.
    section(ui, t("signature.rcs.polar_title"));
    ui.horizontal_wrapped(|ui| {
        for b in Band::ALL {
            ui.selectable_value(&mut st.band, b, b.short());
        }
    });
    let cache = st.cache.as_ref().expect("önbellek dolu");
    let Some(sw) = cache.sweeps.iter().find(|s| s.band == band) else {
        return;
    };
    polar_plot(ui, sw);

    // Tüm bantlar, kartezyen.
    section(ui, t("signature.rcs.cart_title"));
    Plot::new("sig_rcs_cart")
        .height(220.0)
        .x_axis_label(t("signature.rcs.az_axis"))
        .y_axis_label("dBsm")
        .legend(Legend::default())
        .allow_scroll(false)
        .show(ui, |pu| {
            for s in &cache.sweeps {
                let pts: Vec<[f64; 2]> = s
                    .az_deg
                    .iter()
                    .zip(&s.sigma)
                    .map(|(a, v)| [*a, to_db(*v)])
                    .collect();
                pu.line(Line::new(s.band.short(), PlotPoints::from(pts)).width(1.5));
            }
        });

    // Parça katkıları.
    section(ui, &tf("signature.rcs.parts_title_fmt", &[sw.band.short()]));
    let mut parts = sw.by_part.clone();
    parts.sort_by(|a, b| b.1.total_cmp(&a.1));
    egui::Grid::new("sig_rcs_parts")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "signature.rcs.col_part",
                "signature.rcs.col_mean",
                "signature.rcs.col_mean_db",
                "signature.rcs.col_share",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for (part, v) in &parts {
                ui.label(part_label(*part));
                ui.label(format!("{} m²", sig3(*v)));
                ui.label(format!("{:.1}", to_db(*v)));
                ui.label(format!("{:.0} %", 100.0 * v / sw.mean.max(1e-30)));
                ui.end_row();
            }
        });
    ui.label(egui::RichText::new(t("signature.rcs.note_dominant")).weak());
    section(ui, t("signature.tips"));
    ui.label(t("signature.rcs.tips"));
    ui.label(egui::RichText::new(t("signature.rcs.sources")).weak());
}

// ------------------------------------------------------------------------------------------
// IR
// ------------------------------------------------------------------------------------------

fn exposed_combo(ui: &mut Ui, id: &str, v: &mut Option<bool>) {
    let text = |o: Option<bool>| match o {
        None => t("signature.ir.auto"),
        Some(true) => t("signature.ir.exposed"),
        Some(false) => t("signature.ir.internal"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text(*v))
        .show_ui(ui, |ui| {
            for o in [None, Some(true), Some(false)] {
                ui.selectable_value(v, o, text(o));
            }
        });
}

fn show_ir(ui: &mut Ui, p: &Project, m: &Model, st: &mut SigState) {
    let tk = theme::tokens(ui);
    section(ui, t("signature.ir.title"));
    ui.label(t("signature.ir.intro"));
    let motor_d = rcs_cache(p, m, st).airframe.motor_d_m;
    egui::CollapsingHeader::new(t("signature.settings"))
        .id_salt("sig_ir_settings")
        .default_open(false)
        .show(ui, |ui| {
            let s = &mut st.ir;
            egui::Grid::new("sig_ir_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(t("signature.ir.ambient"));
                    ui.horizontal(|ui| {
                        let mut isa = s.ambient_c.is_none();
                        if ui.checkbox(&mut isa, t("signature.ir.isa")).changed() {
                            s.ambient_c = if isa { None } else { Some(m.atmo.t - 273.15) };
                        }
                        if let Some(c) = &mut s.ambient_c {
                            ui.add(
                                DragValue::new(c)
                                    .range(-50.0..=60.0)
                                    .speed(0.5)
                                    .suffix(" °C"),
                            );
                        }
                    });
                    ui.end_row();
                    if p.drone_type == DroneType::VtolQuadplane {
                        ui.label(t("signature.ir.phase"));
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut s.phase,
                                Phase::Cruise,
                                t("signature.ir.phase_cruise"),
                            );
                            ui.selectable_value(
                                &mut s.phase,
                                Phase::Hover,
                                t("signature.ir.phase_hover"),
                            );
                        });
                        ui.end_row();
                    }
                    num(
                        ui,
                        t("signature.ir.eps_motor"),
                        &mut s.eps_motor,
                        0.02..=1.0,
                        0.01,
                        "",
                    );
                    num(
                        ui,
                        t("signature.ir.eps_esc"),
                        &mut s.eps_esc,
                        0.02..=1.0,
                        0.01,
                        "",
                    );
                    num(
                        ui,
                        t("signature.ir.eps_battery"),
                        &mut s.eps_battery,
                        0.02..=1.0,
                        0.01,
                        "",
                    );
                    num(
                        ui,
                        t("signature.ir.eps_skin"),
                        &mut s.eps_skin,
                        0.02..=1.0,
                        0.01,
                        "",
                    );
                    num(
                        ui,
                        t("signature.ir.copper"),
                        &mut s.copper_fraction,
                        0.0..=1.0,
                        0.01,
                        "",
                    );
                    ui.label(t("signature.ir.esc_place"));
                    exposed_combo(ui, "sig_esc_exp", &mut s.esc_exposed);
                    ui.end_row();
                    ui.label(t("signature.ir.batt_place"));
                    exposed_combo(ui, "sig_batt_exp", &mut s.battery_exposed);
                    ui.end_row();
                    num(
                        ui,
                        t("signature.ir.spread"),
                        &mut s.skin_spread,
                        1.0..=50.0,
                        0.1,
                        " ×",
                    );
                    num(
                        ui,
                        t("signature.ir.r_cell"),
                        &mut s.r_cell_mohm,
                        0.0..=200.0,
                        0.1,
                        " mΩ",
                    );
                });
            if ui.button(t("signature.reset")).clicked() {
                *s = IrSettings::default();
            }
        });
    let r: IrResult = ir::evaluate(p, m, &st.ir, motor_d);
    let ta = r.t_amb_k;
    ui.label(tf(
        "signature.ir.line_fmt",
        &[
            &format!("{:.1}", ta - 273.15),
            &match r.phase {
                Phase::Cruise => t("signature.ir.phase_cruise"),
                Phase::Hover => t("signature.ir.phase_hover"),
            }
            .to_string(),
            &fmt(r.i_batt, 1),
            &fmt(r.r_pack * 1000.0, 1),
            &fmt(r.motor_copper_w, 1),
            &fmt(r.motor_iron_w, 1),
        ],
    ));

    section(ui, t("signature.ir.thermal_title"));
    egui::Grid::new("sig_ir_thermal")
        .num_columns(9)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "signature.ir.col_source",
                "signature.ir.col_count",
                "signature.ir.col_heat",
                "signature.ir.col_area",
                "signature.ir.col_air",
                "signature.ir.col_h",
                "signature.ir.col_eps",
                "signature.ir.col_temp",
                "signature.ir.col_dt",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for s in &r.sources {
                let name = if s.exposed {
                    source_label(s.kind).to_string()
                } else {
                    tf("signature.ir.via_skin_fmt", &[source_label(s.kind)])
                };
                ui.label(name);
                ui.label(s.count.to_string());
                ui.label(fmt(s.heat_w, 2));
                ui.label(fmt(s.area_m2 * 1e4, 1));
                ui.label(fmt(s.air_v, 1));
                ui.label(fmt(s.h, 0));
                ui.label(fmt(s.eps, 2));
                ui.label(fmt(s.t_k - 273.15, 1));
                ui.label(fmt(s.delta_k(ta), 1));
                ui.end_row();
            }
        });

    section(ui, t("signature.ir.rad_title"));
    egui::Grid::new("sig_ir_rad")
        .num_columns(5)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "signature.ir.col_source",
                "signature.ir.col_l_mwir",
                "signature.ir.col_l_lwir",
                "signature.ir.col_di_mwir",
                "signature.ir.col_di_lwir",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for s in &r.sources {
                let n = s.count as f64;
                ui.label(format!("{} ×{}", source_label(s.kind), s.count));
                ui.label(sig3(s.radiance(MWIR)));
                ui.label(sig3(s.radiance(LWIR)));
                ui.label(sig3(n * s.di_mwir));
                ui.label(sig3(n * s.di_lwir));
                ui.end_row();
            }
            ui.label(egui::RichText::new(t("signature.ir.total")).strong());
            ui.label(format!("{} W/sr", sig3(r.total_mwir)));
            ui.label(format!("{} W/sr", sig3(r.total_lwir)));
            ui.label(egui::RichText::new(sig3(r.total_di_mwir)).strong());
            ui.label(egui::RichText::new(sig3(r.total_di_lwir)).strong());
            ui.end_row();
        });
    ui.label(egui::RichText::new(t("signature.ir.units_note")).weak());

    // Karşıtlık çubukları (her kaynağın toplam ΔI payı, %).
    let tot_m = r.total_di_mwir.max(1e-30);
    let tot_l = r.total_di_lwir.max(1e-30);
    let bars_m: Vec<Bar> = r
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Bar::new(i as f64 - 0.18, 100.0 * s.count as f64 * s.di_mwir / tot_m).width(0.34)
        })
        .collect();
    let bars_l: Vec<Bar> = r
        .sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Bar::new(i as f64 + 0.18, 100.0 * s.count as f64 * s.di_lwir / tot_l).width(0.34)
        })
        .collect();
    let names: Vec<&'static str> = r.sources.iter().map(|s| source_label(s.kind)).collect();
    Plot::new("sig_ir_bars")
        .height(180.0)
        .y_axis_label(t("signature.ir.share_axis"))
        .legend(Legend::default())
        .allow_scroll(false)
        .x_axis_formatter(move |mark, _| {
            let i = mark.value.round();
            if (mark.value - i).abs() < 1e-6 && i >= 0.0 {
                names
                    .get(i as usize)
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        })
        .show(ui, |pu| {
            pu.bar_chart(BarChart::new("3–5 µm", bars_m).color(tk.warning));
            pu.bar_chart(BarChart::new("8–12 µm", bars_l).color(tk.info));
        });
    section(ui, t("signature.tips"));
    ui.label(t("signature.ir.tips"));
    ui.label(egui::RichText::new(t("signature.ir.sources")).weak());
}

// ------------------------------------------------------------------------------------------
// GNSS kaybı sapması
// ------------------------------------------------------------------------------------------

fn project_speed(p: &Project) -> f64 {
    let v = if p.drone_type == DroneType::Multirotor {
        p.mr_v_cruise
    } else {
        p.v_cr
    };
    if v.is_finite() && v > 0.0 { v } else { 10.0 }
}

fn fmt_time(t_s: Option<f64>) -> String {
    match t_s {
        Some(s) if s < 120.0 => format!("{s:.0} s"),
        Some(s) => format!("{:.1} min", s / 60.0),
        None => t("signature.drift.never").to_string(),
    }
}

fn show_drift(ui: &mut Ui, p: &Project, st: &mut SigState) {
    let tk = theme::tokens(ui);
    section(ui, t("signature.drift.title"));
    ui.label(t("signature.drift.intro"));
    let s = &mut st.drift;
    ui.horizontal_wrapped(|ui| {
        ui.label(t("signature.drift.grade"));
        for g in ImuGrade::ALL {
            if ui.selectable_label(s.grade == g, grade_label(g)).clicked() {
                s.grade = g;
                s.imu = g.spec();
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("signature.drift.aiding"));
        for a in Aiding::ALL {
            ui.selectable_value(&mut s.aiding, a, aiding_label(a));
        }
        ui.checkbox(&mut s.baro, t("signature.drift.baro"));
    });
    egui::CollapsingHeader::new(t("signature.settings"))
        .id_salt("sig_drift_settings")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("sig_drift_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    num(
                        ui,
                        t("signature.drift.gyro_bias"),
                        &mut s.imu.gyro_bias_dph,
                        0.0..=1000.0,
                        0.1,
                        " °/h",
                    );
                    num(
                        ui,
                        t("signature.drift.arw"),
                        &mut s.imu.arw_dpsh,
                        0.0..=10.0,
                        0.01,
                        " °/√h",
                    );
                    num(
                        ui,
                        t("signature.drift.accel_bias"),
                        &mut s.imu.accel_bias_mg,
                        0.0..=100.0,
                        0.05,
                        " mg",
                    );
                    num(
                        ui,
                        t("signature.drift.speed"),
                        &mut s.speed,
                        0.0..=100.0,
                        0.1,
                        " m/s",
                    );
                    num(
                        ui,
                        t("signature.drift.sigma0"),
                        &mut s.sigma0_m,
                        0.0..=50.0,
                        0.1,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.wind"),
                        &mut s.wind_sigma,
                        0.0..=10.0,
                        0.05,
                        " m/s",
                    );
                    num(
                        ui,
                        t("signature.drift.heading"),
                        &mut s.heading_sigma_deg,
                        0.0..=30.0,
                        0.1,
                        " °",
                    );
                    num(
                        ui,
                        t("signature.drift.airspeed"),
                        &mut s.airspeed_sigma,
                        0.0..=5.0,
                        0.05,
                        " m/s",
                    );
                    num(
                        ui,
                        t("signature.drift.agl"),
                        &mut s.agl_m,
                        0.0..=1000.0,
                        0.5,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.flow_sigma"),
                        &mut s.flow_sigma_rad_s,
                        0.0..=0.1,
                        0.0005,
                        " rad/s",
                    );
                    num(
                        ui,
                        t("signature.drift.flow_scale"),
                        &mut s.flow_scale,
                        0.0..=0.3,
                        0.001,
                        "",
                    );
                    num(
                        ui,
                        t("signature.drift.flow_max"),
                        &mut s.flow_max_agl,
                        1.0..=300.0,
                        0.5,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.baro_sigma"),
                        &mut s.baro_sigma_m,
                        0.0..=20.0,
                        0.1,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.baro_drift"),
                        &mut s.baro_drift_m_min,
                        0.0..=5.0,
                        0.01,
                        " m/min",
                    );
                    num(
                        ui,
                        t("signature.drift.thr1"),
                        &mut s.thresholds[0],
                        0.5..=10000.0,
                        0.5,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.thr2"),
                        &mut s.thresholds[1],
                        0.5..=10000.0,
                        0.5,
                        " m",
                    );
                    num(
                        ui,
                        t("signature.drift.horizon"),
                        &mut s.horizon_s,
                        10.0..=3600.0,
                        1.0,
                        " s",
                    );
                    ui.label(t("signature.drift.mc"));
                    ui.checkbox(&mut s.monte_carlo, "");
                    ui.end_row();
                });
            if ui.button(t("signature.reset")).clicked() {
                *s = DriftSettings::default();
            }
        });
    let speed = if s.speed > 0.0 {
        s.speed
    } else {
        project_speed(p)
    };
    if s.aiding == Aiding::OpticalFlow && !drift::flow_valid(s) {
        components::banner(ui, Status::Info, t("signature.drift.flow_invalid"));
    }
    let thr = s.thresholds;
    let t1 = drift::time_to_exceed(s, speed, thr[0]);
    let t2 = drift::time_to_exceed(s, speed, thr[1]);
    ui.label(
        egui::RichText::new(tf(
            "signature.drift.result_fmt",
            &[
                &fmt(thr[0], 0),
                &fmt_time(t1),
                &fmt(thr[1], 0),
                &fmt_time(t2),
                &fmt(speed, 1),
            ],
        ))
        .strong(),
    );

    let pts = drift::curve(s, speed, 300);
    let log = st.log_scale;
    let y = |v: f64| if log { v.max(1e-2).log10() } else { v };
    let series = |f: &dyn Fn(&drift::Point) -> f64| -> PlotPoints<'static> {
        PlotPoints::from(pts.iter().map(|q| [q.t, y(f(q))]).collect::<Vec<_>>())
    };
    ui.checkbox(&mut st.log_scale, t("signature.drift.log"));
    let s = &st.drift;
    let mc = if s.monte_carlo {
        Some(drift::monte_carlo_ins(
            &s.imu,
            s.horizon_s.clamp(10.0, 3600.0),
            (s.horizon_s / 300.0).max(0.2),
            200,
            17,
        ))
    } else {
        None
    };
    let aided = drift::aided_velocity_sigma(s, speed).is_some();
    Plot::new("sig_drift_plot")
        .height(300.0)
        .x_axis_label(t("signature.drift.t_axis"))
        .y_axis_label(t("signature.drift.e_axis"))
        .legend(Legend::default())
        .allow_scroll(false)
        .y_axis_formatter(move |mark, _| {
            if log {
                let v = 10f64.powf(mark.value);
                if v >= 1.0 {
                    format!("{v:.0} m")
                } else {
                    format!("{v:.2} m")
                }
            } else {
                format!("{:.0} m", mark.value)
            }
        })
        .show(ui, |pu| {
            pu.line(
                Line::new(t("signature.drift.l_total"), series(&|q| q.horizontal))
                    .width(2.5)
                    .color(tk.accent),
            );
            if aided {
                pu.line(Line::new(t("signature.drift.l_aided"), series(&|q| q.aided)).width(1.5));
            } else {
                pu.line(Line::new(t("signature.drift.l_accel"), series(&|q| q.accel)).width(1.2));
                pu.line(Line::new(t("signature.drift.l_gyro"), series(&|q| q.gyro)).width(1.2));
                pu.line(Line::new(t("signature.drift.l_arw"), series(&|q| q.arw)).width(1.2));
            }
            pu.line(
                Line::new(t("signature.drift.l_vertical"), series(&|q| q.vertical))
                    .width(1.2)
                    .color(tk.text_weak),
            );
            if let Some(mc) = &mc {
                let pts: Vec<[f64; 2]> = mc
                    .iter()
                    .map(|q| [q[0], y((q[1] * q[1] + s.sigma0_m * s.sigma0_m).sqrt())])
                    .collect();
                pu.line(
                    Line::new(t("signature.drift.l_mc"), PlotPoints::from(pts))
                        .width(1.5)
                        .color(tk.warning),
                );
            }
            for v in thr {
                pu.hline(
                    HLine::new(format!("{v:.0} m"), y(v))
                        .color(tk.danger)
                        .width(1.0),
                );
            }
        });

    // IMU sınıfları karşılaştırması (seçili yardımla).
    section(ui, t("signature.drift.compare_title"));
    egui::Grid::new("sig_drift_cmp")
        .num_columns(5)
        .striped(true)
        .show(ui, |ui| {
            for h in [
                "signature.drift.col_grade",
                "signature.drift.col_e60",
                "signature.drift.col_e300",
                "signature.drift.col_t1",
                "signature.drift.col_t2",
            ] {
                ui.label(egui::RichText::new(t(h)).strong());
            }
            ui.end_row();
            for g in ImuGrade::ALL {
                let mut c = s.clone();
                c.imu = g.spec();
                let sp = g.spec();
                ui.label(format!(
                    "{} ({} °/h, {} °/√h, {} mg)",
                    grade_label(g),
                    sig3(sp.gyro_bias_dph),
                    sig3(sp.arw_dpsh),
                    sig3(sp.accel_bias_mg)
                ));
                ui.label(format!(
                    "{} m",
                    sig3(drift::point_at(&c, speed, 60.0).horizontal)
                ));
                ui.label(format!(
                    "{} m",
                    sig3(drift::point_at(&c, speed, 300.0).horizontal)
                ));
                ui.label(fmt_time(drift::time_to_exceed(&c, speed, thr[0])));
                ui.label(fmt_time(drift::time_to_exceed(&c, speed, thr[1])));
                ui.end_row();
            }
        });
    ui.label(egui::RichText::new(t("signature.drift.typical_note")).weak());
    section(ui, t("signature.tips"));
    ui.label(t("signature.drift.tips"));
    ui.label(egui::RichText::new(t("signature.drift.sources")).weak());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::ir::SIGMA_SB;

    fn projects() -> Vec<(String, Project)> {
        let mut v = Vec::new();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            v.push((format!("{dt:?}"), p));
        }
        for (name, json) in [
            ("fixed_wing", crate::project::EX_FIXED_WING),
            ("flying_wing", crate::project::EX_FLYING_WING),
            ("quad250", crate::project::EX_QUAD_250),
            ("quad275", crate::project::EX_QUAD_275),
            ("hexa550", crate::project::EX_HEXA_550),
            ("vtol", crate::project::EX_VTOL),
        ] {
            v.push((
                name.to_string(),
                Project::from_json(json).expect("örnek proje"),
            ));
        }
        v
    }

    /// Beş araç türü ve örnek projeler: RCS, IR ve sapma sonuçları sonlu (NaN yok) ve anlamlı.
    #[test]
    fn all_types_and_samples_are_finite() {
        let lib = crate::calc::Library::embedded();
        for (name, p) in projects() {
            let m = crate::calc::evaluate(&p, &lib);
            let rs = RcsSettings::default();
            let af = rcs::airframe(&p, &m, &rs);
            assert!(!af.prims.is_empty(), "{name}");
            assert!(af.n_motors >= 1, "{name}");
            for b in Band::ALL {
                let sw = rcs::sweep(&af, b, &rs, 72);
                assert!(
                    sw.sigma.iter().all(|v| v.is_finite() && *v >= 0.0),
                    "{name} {b:?}"
                );
                assert!(sw.mean > 0.0 && sw.mean.is_finite(), "{name} {b:?}");
                assert!(
                    sw.by_part.iter().any(|(p, _)| *p == Part::Propeller),
                    "{name}"
                );
            }
            for phase in [Phase::Cruise, Phase::Hover] {
                let mut is = IrSettings::default();
                is.phase = phase;
                let r = ir::evaluate(&p, &m, &is, af.motor_d_m);
                assert_eq!(r.sources.len(), 3, "{name}");
                for s in &r.sources {
                    assert!(
                        s.t_k.is_finite() && s.t_k >= r.t_amb_k - 1e-9,
                        "{name} {:?}",
                        s.kind
                    );
                    assert!(
                        s.i_mwir.is_finite() && s.i_lwir.is_finite() && s.di_lwir >= 0.0,
                        "{name}"
                    );
                    // Işınım ≤ kara cisim σT⁴.
                    assert!(s.radiance(LWIR) * std::f64::consts::PI <= SIGMA_SB * s.t_k.powi(4));
                }
                assert!(
                    r.total_di_lwir.is_finite() && r.total_di_mwir.is_finite(),
                    "{name}"
                );
            }
            let ds = DriftSettings::default();
            let pts = drift::curve(&ds, project_speed(&p), 50);
            assert!(
                pts.iter()
                    .all(|q| q.horizontal.is_finite() && q.vertical.is_finite()),
                "{name}"
            );
        }
    }

    /// Kaba değer aralıkları (rapor için de tipik sayılar).
    #[test]
    fn typical_numbers_are_plausible() {
        let lib = crate::calc::Library::embedded();
        for (name, json) in [
            ("fw", crate::project::EX_FIXED_WING),
            ("quad", crate::project::EX_QUAD_250),
        ] {
            let p = Project::from_json(json).unwrap();
            let m = crate::calc::evaluate(&p, &lib);
            let rs = RcsSettings::default();
            let af = rcs::airframe(&p, &m, &rs);
            let x = rcs::sweep(&af, Band::X, &rs, 360);
            // Küçük İHA: ortalama −40…+10 dBsm aralığında.
            assert!(
                (-40.0..10.0).contains(&x.mean_dbsm()),
                "{name}: {}",
                x.mean_dbsm()
            );
            let r = ir::evaluate(&p, &m, &IrSettings::default(), af.motor_d_m);
            let mot = r
                .sources
                .iter()
                .find(|s| s.kind == SourceKind::Motor)
                .unwrap();
            let dt = mot.delta_k(r.t_amb_k);
            assert!(dt > 0.5 && dt < 200.0, "{name}: motor ΔT {dt}");
        }
    }

    /// Bu dosyada kullanılan tüm `signature.*` anahtarları iki dilde de var.
    #[test]
    fn i18n_keys_exist_in_both_languages() {
        let src = include_str!("signature.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let tr = crate::i18n::tr_table();
        let en = crate::i18n::en_table();
        let mut n = 0;
        for part in prod.split('"').skip(1).step_by(2) {
            if (part.starts_with("signature.") || part == "tab_signature")
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
            {
                assert!(tr.contains_key(part), "tr eksik: {part}");
                assert!(en.contains_key(part), "en eksik: {part}");
                n += 1;
            }
        }
        assert!(n > 50, "{n}");
        // Eklenen tüm anahtarlar iki dilde eşit.
        for k in tr.keys().filter(|k| k.starts_with("signature.")) {
            assert!(en.contains_key(k), "en eksik: {k}");
        }
        for k in en.keys().filter(|k| k.starts_with("signature.")) {
            assert!(tr.contains_key(k), "tr eksik: {k}");
        }
    }

    /// Üç görünüm, iki tema ve iki dilde başsız çizim (panik yok), örnek türlerle.
    #[test]
    fn renders_every_view_both_themes_and_languages() {
        use crate::ui::theme::{self, ThemeChoice};
        let lib = crate::calc::Library::embedded();
        let mut cases = Vec::new();
        for dt in [
            DroneType::FixedWingConventional,
            DroneType::Multirotor,
            DroneType::VtolQuadplane,
        ] {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &lib);
            cases.push((p, m));
        }
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                for (p, m) in &cases {
                    let mut st = SigState::default();
                    st.drift.monte_carlo = true;
                    st.drift.aiding = Aiding::OpticalFlow;
                    st.drift.agl_m = 100.0;
                    for view in [View::Rcs, View::Ir, View::Drift] {
                        st.view = view;
                        crate::i18n::with_lang(lang, || {
                            for _ in 0..2 {
                                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        egui::ScrollArea::both()
                                            .show(ui, |ui| show(ui, p, m, &mut st));
                                    });
                                });
                                out.textures_delta.clear();
                            }
                        });
                    }
                }
            }
        }
    }
}
