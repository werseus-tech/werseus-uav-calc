// SPDX-License-Identifier: AGPL-3.0-or-later
//! Basit mod ekranları: her sihirbaz adımı için günlük dilde sorular ve sonuçlar.
//! Tüm mantık `calc::presets` içindedir; bu dosya yalnızca soru ve sonuçları çizer. Ham değerler
//! basit modda "Ayrıntılı değerler" bölümünde kapalı durur; orada yapılan düzenlemeler otomatik
//! "özelleştirildi" sayılır ve korunur.
use super::aero_simple::{analogy_name, result_row};
use super::components::{NumberInput, Status, number_row, section::section_header};
use super::step_ui;
use super::widgets::fmt;
use super::wizard::StepId;
use crate::calc::presets::{
    self, Ctx, Rating, SimpleStep, Snapshot, design_point as dpm, endurance as endm, frame as frm,
    mission as mis, propulsion as prp, stability as stb, structure as stc, weight as wgt,
};
use crate::calc::{Library, Model};
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use egui::{RichText, Ui};

/// Uzman sekmesinden sihirbaz adımı karşılığı (karşılığı olmayan sekmeler `None`).
pub fn step_for_tab(tab: super::Tab, dt: DroneType) -> Option<StepId> {
    use super::Tab;
    match tab {
        Tab::Mission => Some(StepId::Mission),
        Tab::Weight => Some(StepId::Weight),
        Tab::Geometry => Some(StepId::Geometry),
        Tab::Aero => Some(StepId::Aero),
        Tab::DesignPoint => Some(StepId::DesignPoint),
        Tab::Endurance => Some(StepId::Endurance),
        Tab::Propulsion => Some(StepId::Propulsion),
        Tab::Stability => Some(StepId::Stability),
        Tab::Structure => Some(if dt == DroneType::Multirotor {
            StepId::MrStructure
        } else {
            StepId::Structure
        }),
        Tab::Multirotor | Tab::Summary | Tab::Schematic | Tab::WindTunnel => None,
    }
}

/// Kare başı (kabuk çağırır): basit mod "bu kare yazdı" listesi sıfırlanır, basit moddaysa
/// türetilmiş değerler eşitlenir ve özelleştirme takibi için anlık görüntü alınır. Sol ve sağ
/// paneller çizildikten sonra [`frame_end`] çağrılır; aradaki elle düzenlemeler (her iki
/// paneldeki girdiler/düğmeler) "özelleştirildi" sayılır. Eski `host` sarmalayıcısının yerini alır.
pub fn frame_begin(ctx: &egui::Context, p: &mut Project, m: &Model, lib: &Library) -> Snapshot {
    p.simple.begin_frame();
    if p.aero_simple.simple_mode {
        let c = Ctx::from_model(p, m, &lib.materials);
        if presets::sync(p, &c) {
            ctx.request_repaint();
        }
    }
    Snapshot::take(p)
}

/// Kare sonu: [`frame_begin`]'den bu yana yapılan elle düzenlemeleri işaretler.
pub fn frame_end(p: &mut Project, snap: &Snapshot) {
    let mut st = std::mem::take(&mut p.simple);
    st.mark_edits(snap, p);
    p.simple = st;
}

/// Basit mod panelinde soru (girdi) var mı? Yoksa (yalnız sonuç gösteren adımlar) sol paneldeki
/// "Ayrıntılı değerler" bölümü açık başlar; böylece her adımda düzenlenebilir bir girdi görünür.
pub fn has_questions(step: StepId) -> bool {
    !matches!(
        step,
        StepId::Endurance
            | StepId::MrBatteryLoiter
            | StepId::Stability
            | StepId::MrCG
            | StepId::Summary
            | StepId::Outputs
    )
}

// ---------------------------------------------------------------------------------------------
// Ortak çizim yardımcıları
// ---------------------------------------------------------------------------------------------

fn intro(ui: &mut Ui, intro_key: &'static str) {
    step_ui::note(ui, t(intro_key));
    ui.add_space(4.0);
}

fn results_header(ui: &mut Ui) {
    section_header(ui, t("simple.results"));
}

/// Seçenek satırı: soru + yan yana seçilebilir etiketler (üzerine gelince açıklama).
fn pick<T: Copy + PartialEq>(
    ui: &mut Ui,
    q_key: &'static str,
    cur: &mut T,
    all: &[T],
    key: fn(T) -> &'static str,
    desc: fn(T) -> &'static str,
) -> bool {
    let mut changed = false;
    ui.add(egui::Label::new(RichText::new(t(q_key)).strong()).wrap());
    ui.horizontal_wrapped(|ui| {
        for &v in all {
            if ui
                .selectable_label(*cur == v, t(key(v)))
                .on_hover_text(t(desc(v)))
                .clicked()
                && *cur != v
            {
                *cur = v;
                changed = true;
            }
        }
    });
    ui.add_space(2.0);
    changed
}

fn notes(ui: &mut Ui, keys: &[&'static str]) {
    for k in keys {
        step_ui::status_text(ui, Status::Warn, &format!("! {}", t(k)));
    }
}

fn line(ui: &mut Ui, r: Rating, text: String, tip: Option<&'static str>) {
    result_row(ui, r, text, tip, None);
}

/// "Özelleştirildi" rozeti: elle değiştirilmiş alanlar korunur; tek tıkla varsayılana dönülür.
fn badge(ui: &mut Ui, p: &mut Project, step: SimpleStep) {
    let custom = p.simple.custom_in(step);
    if custom.is_empty() {
        return;
    }
    let names: Vec<&str> = custom.iter().map(|f| t(f.key())).collect();
    step_ui::status_text(
        ui,
        Status::Warn,
        &tf("simple.custom_badge", &[names.join(", ")]),
    );
    if ui
        .button(t("simple.custom_reset"))
        .on_hover_text(t("simple.custom_reset.desc"))
        .clicked()
    {
        p.simple.reset_step(step);
        ui.ctx().request_repaint();
    }
}

fn inactive_hint(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library, step: SimpleStep) {
    if p.simple.is_active(step) {
        return;
    }
    step_ui::note(ui, t("simple.inactive_hint"));
    if ui.button(t("simple.apply_answers")).clicked() {
        let ctx = Ctx::from_model(p, m, &lib.materials);
        presets::answer_changed(p, step, &ctx);
    }
}

fn changed_apply(p: &mut Project, m: &Model, lib: &Library, step: SimpleStep) {
    let ctx = Ctx::from_model(p, m, &lib.materials);
    presets::answer_changed(p, step, &ctx);
}

fn mtow_kg(p: &Project, m: &Model) -> f64 {
    if p.drone_type == DroneType::Multirotor {
        m.mr.m_mtow
    } else {
        m.weight.m_mtow
    }
}

fn pct(x: f64) -> String {
    fmt(x * 100.0, 0)
}

fn text(ui: &mut Ui, s: &str) {
    step_ui::text(ui, s);
}

// ---------------------------------------------------------------------------------------------
// Adım paneli dağıtımı: sorular (sol panel) ve sonuçlar (sağ panel)
// Not: kanat planı / batarya önerisi / boru seçimi düğmeleri TEK SEFERLİK komutlardır; otomatik
// yeniden uygulanmaz (türetme alanı değildir), bu yüzden özelleştirme takibine girmez ve düğmenin
// açıklaması mevcut değerlerin üzerine yazdığını belirtir.
// ---------------------------------------------------------------------------------------------

/// Basit mod soruları (sol panel).
pub fn inputs(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library, step: StepId) {
    match step {
        StepId::Mission => mission_in(ui, p, m, lib),
        StepId::Weight => {
            if p.drone_type == DroneType::Multirotor {
                mr_weight_in(ui, p, m, lib)
            } else {
                weight_in(ui, p, m, lib)
            }
        }
        StepId::Geometry => wing_in(ui, p, m, lib),
        StepId::Aero => {}
        StepId::DesignPoint => dp_in(ui, p, m, lib),
        StepId::Endurance | StepId::MrBatteryLoiter => intro(ui, "simple.end.intro"),
        StepId::Propulsion => propulsion_in(ui, p, m, lib),
        StepId::Stability => intro(ui, "simple.stab.intro"),
        StepId::Structure => tube_in(ui, p, lib, false),
        StepId::Summary | StepId::Outputs => {}
        StepId::MrFrame => mr_frame_in(ui, p, m, lib),
        StepId::MrPropulsion => mr_propulsion_in(ui, p, m, lib),
        StepId::MrCG => intro(ui, "simple.stab.mr_intro"),
        StepId::MrStructure => tube_in(ui, p, lib, true),
    }
}

/// Basit mod sonuçları (sağ panel).
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library, step: StepId) {
    match step {
        StepId::Mission => mission_out(ui, p, m),
        StepId::Weight => {
            if p.drone_type != DroneType::Multirotor {
                weight_out(ui, p, m)
            }
        }
        StepId::Geometry | StepId::Aero | StepId::Summary | StepId::Outputs => {}
        StepId::DesignPoint => dp_out(ui, p, m),
        StepId::Endurance => endurance_out(ui, p, m, false),
        StepId::MrBatteryLoiter => endurance_out(ui, p, m, true),
        StepId::Propulsion => propulsion_out(ui, p, m),
        StepId::Stability => stability_out(ui, p, m),
        StepId::Structure => spar_out(ui, p, m),
        StepId::MrFrame => mr_frame_out(ui, p, m),
        StepId::MrPropulsion => mr_propulsion_out(ui, p, m),
        StepId::MrCG => mr_cg_out(ui, m),
        StepId::MrStructure => arm_out(ui, p, m),
    }
    let _ = lib;
}

// ---------------------------------------------------------------------------------------------
// 1. Görev
// ---------------------------------------------------------------------------------------------

fn mission_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.mission.intro");
    let mut changed = false;
    {
        let a = &mut p.simple.mission;
        changed |= pick(
            ui,
            "simple.mission.q.goal",
            &mut a.goal,
            mis::Goal::ALL,
            mis::Goal::key,
            mis::Goal::desc_key,
        );
        changed |= pick(
            ui,
            "simple.mission.q.endurance",
            &mut a.endurance,
            mis::EnduranceGoal::ALL,
            mis::EnduranceGoal::key,
            mis::EnduranceGoal::desc_key,
        );
        changed |= pick(
            ui,
            "simple.mission.q.payload",
            &mut a.payload,
            mis::PayloadClass::ALL,
            mis::PayloadClass::key,
            mis::PayloadClass::desc_key,
        );
        if a.payload == mis::PayloadClass::Custom {
            changed |= number_row(
                ui,
                NumberInput::new(t("simple.mission.custom_payload"), &mut a.custom_payload_g)
                    .unit("g")
                    .range(0.0, 5000.0)
                    .decimals(0),
            );
        }
        changed |= pick(
            ui,
            "simple.mission.q.launch",
            &mut a.launch,
            mis::Launch::ALL,
            mis::Launch::key,
            mis::Launch::desc_key,
        );
        changed |= pick(
            ui,
            "simple.mission.q.terrain",
            &mut a.terrain,
            mis::Terrain::ALL,
            mis::Terrain::key,
            mis::Terrain::desc_key,
        );
    }
    if changed {
        // Aerodinamik basit modun "amaç" sorusuna görev amacından başlangıç önerisi.
        if !p.aero_simple.derived_active {
            p.aero_simple.purpose = mis::aero_purpose(p.simple.mission.goal);
        }
        changed_apply(p, m, lib, SimpleStep::Mission);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::Mission);
    badge(ui, p, SimpleStep::Mission);
}

fn mission_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    let a = p.simple.mission.clone();
    let (sug, reason) = mis::suggest_type(&a);
    text(
        ui,
        &tf(
            "simple.mission.suggest",
            &[sug.label().to_string(), t(reason).to_string()],
        ),
    );
    if sug != p.drone_type
        && ui
            .button(tf(
                "simple.mission.suggest_apply",
                &[sug.label().to_string()],
            ))
            .clicked()
    {
        p.set_drone_type(sug);
    }
    notes(ui, &mis::consistency_notes(&a, p.drone_type));
    text(
        ui,
        &tf(
            "simple.mission.res_speed",
            &[
                fmt(presets::kmh(p.v_cr), 0),
                analogy_name(p.v_cr).to_string(),
            ],
        ),
    );
    text(
        ui,
        &tf(
            "simple.mission.res_payload",
            &[fmt(p.m_payload * 1000.0, 0), fmt(a.target_minutes(), 0)],
        ),
    );
    if a.launch == mis::Launch::Hand && m.inputs_ok {
        let mt = mtow_kg(p, m);
        line(
            ui,
            mis::hand_launch_rating(mt),
            tf("simple.mission.res_hand", &[fmt(mt * 1000.0, 0)]),
            Some(t("simple.mission.tip_hand")),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. Ağırlık / Batarya
// ---------------------------------------------------------------------------------------------

fn battery_section(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    section_header(ui, t("simple.weight.battery"));
    let target = p.simple.mission.target_minutes();
    let id = egui::Id::NULL.with("simple_batt_rec");
    text(ui, &tf("simple.weight.battery_target", &[fmt(target, 0)]));
    if ui
        .button(t("simple.weight.battery_suggest"))
        .on_hover_text(t("simple.weight.battery_suggest.desc"))
        .clicked()
    {
        let txt = match wgt::recommend_battery(p, lib, target) {
            Some(r) => {
                wgt::apply_battery(p, &r);
                tf(
                    if r.meets_target {
                        "simple.weight.battery_rec"
                    } else {
                        "simple.weight.battery_rec_short"
                    },
                    &[
                        r.cell_name.clone(),
                        r.n_series.to_string(),
                        r.n_parallel.to_string(),
                        fmt(r.endurance_min, 0),
                        fmt(r.pack_mass_kg * 1000.0, 0),
                    ],
                )
            }
            None => t("simple.weight.battery_none").to_string(),
        };
        ui.ctx().data_mut(|d| d.insert_temp(id, txt));
    }
    if let Some(txt) = ui.ctx().data(|d| d.get_temp::<String>(id)) {
        ui.add(egui::Label::new(RichText::new(txt).strong()).wrap());
    }
    // Ham değerler bölümündeki aynı batarya alanlarıyla kimlik çakışmasın diye ayrı kapsam.
    ui.push_id("simple_battery", |ui| {
        super::weight::render_battery_source_ui(ui, p, m, lib);
    });
}

fn weight_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.weight.intro");
    let mut changed = false;
    {
        let a = &mut p.simple.weight;
        changed |= pick(
            ui,
            "simple.weight.q.budget",
            &mut a.budget,
            wgt::Budget::ALL,
            wgt::Budget::key,
            wgt::Budget::desc_key,
        );
        changed |= pick(
            ui,
            "simple.weight.q.build",
            &mut a.build,
            wgt::Build::ALL,
            wgt::Build::key,
            wgt::Build::desc_key,
        );
    }
    if changed {
        changed_apply(p, m, lib, SimpleStep::Weight);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::Weight);
    badge(ui, p, SimpleStep::Weight);
    battery_section(ui, p, m, lib);
}

fn weight_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    if m.inputs_ok {
        let w = &m.weight;
        text(
            ui,
            &tf(
                "simple.weight.res_mtow",
                &[
                    fmt(w.m_mtow * 1000.0, 0),
                    fmt(
                        wgt::target_mtow_kg(p.simple.weight.budget, p.drone_type) * 1000.0,
                        0,
                    ),
                ],
            ),
        );
        line(
            ui,
            wgt::battery_fraction_rating(w.f_batt),
            tf("simple.weight.res_batt_frac", &[pct(w.f_batt)]),
            Some(t("simple.weight.tip_batt_frac")),
        );
        if p.simple.mission.launch == mis::Launch::Hand {
            line(
                ui,
                mis::hand_launch_rating(w.m_mtow),
                tf("simple.mission.res_hand", &[fmt(w.m_mtow * 1000.0, 0)]),
                Some(t("simple.mission.tip_hand")),
            );
        }
    }
}

fn mr_weight_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.weight.mr_intro");
    text(
        ui,
        &tf(
            "simple.weight.mr_masses",
            &[fmt(m.mr.m_mtow * 1000.0, 0), fmt(p.mr_m_frame * 1000.0, 0)],
        ),
    );
    battery_section(ui, p, m, lib);
}

// ---------------------------------------------------------------------------------------------
// 3. Kanat & Kuyruk / Çerçeve
// ---------------------------------------------------------------------------------------------

fn wing_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    if !p.drone_type.has_wing() {
        return;
    }
    section_header(ui, t("simple.frame.wing_section"));
    {
        let a = &mut p.simple.frame;
        pick(
            ui,
            "simple.frame.q.transport",
            &mut a.transport,
            frm::Transport::ALL,
            frm::Transport::key,
            frm::Transport::desc_key,
        );
        ui.checkbox(&mut a.removable_wing, t("simple.frame.removable"))
            .on_hover_text(t("simple.frame.removable.desc"));
    }
    let a = p.simple.frame.clone();
    let fit = frm::pack_fit(p.b, p.l_fuse, a.removable_wing);
    if a.transport == frm::Transport::Backpack {
        line(
            ui,
            if fit.fits { Rating::Good } else { Rating::Bad },
            tf(
                if fit.fits {
                    "simple.frame.fit_ok"
                } else {
                    "simple.frame.fit_no"
                },
                &[
                    fmt(fit.longest_m * 100.0, 0),
                    fmt(frm::BACKPACK_MAX_M * 100.0, 0),
                ],
            ),
            Some(t("simple.frame.tip_fit")),
        );
    }
    if m.inputs_ok {
        if let Some(plan) = frm::plan_wing(
            p,
            lib,
            p.simple.mission.goal,
            p.aero_simple.size_pref,
            a.transport,
            a.removable_wing,
        ) {
            text(
                ui,
                &tf(
                    "simple.frame.plan",
                    &[
                        fmt(plan.s * 100.0, 0),
                        fmt(plan.b * 100.0, 0),
                        fmt(plan.ar, 1),
                    ],
                ),
            );
            if plan.capped_by_pack {
                step_ui::status_text(ui, Status::Warn, &format!("! {}", t("simple.frame.capped")));
            }
            if ui
                .button(t("simple.frame.plan_apply"))
                .on_hover_text(t("simple.frame.plan_apply.desc"))
                .clicked()
            {
                frm::apply_wing_plan(p, &plan);
            }
        }
    }
    ui.add_space(6.0);
}

fn mr_frame_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.frame.mr_intro");
    let mut changed = false;
    {
        let a = &mut p.simple.frame;
        changed |= pick(
            ui,
            "simple.frame.q.class",
            &mut a.class,
            frm::FrameClass::ALL,
            frm::FrameClass::key,
            frm::FrameClass::desc_key,
        );
        changed |= pick(
            ui,
            "simple.frame.q.motors",
            &mut a.motors,
            frm::MotorCount::ALL,
            frm::MotorCount::key,
            frm::MotorCount::desc_key,
        );
    }
    if changed {
        changed_apply(p, m, lib, SimpleStep::Frame);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::Frame);
    badge(ui, p, SimpleStep::Frame);
}

fn mr_frame_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    text(
        ui,
        &tf(
            "simple.frame.res_prop",
            &[
                fmt(p.mr_d_prop / 0.0254, 0),
                fmt(p.mr_frame_size * 1000.0, 0),
                p.mr_arm_count.to_string(),
            ],
        ),
    );
    if m.inputs_ok {
        line(
            ui,
            if m.mr.prop_overlap {
                Rating::Bad
            } else {
                Rating::Good
            },
            t(if m.mr.prop_overlap {
                "simple.frame.res_overlap_bad"
            } else {
                "simple.frame.res_overlap_ok"
            })
            .to_string(),
            Some(t("simple.frame.tip_overlap")),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 5. Tasarım noktası
// ---------------------------------------------------------------------------------------------

fn dp_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    section_header(ui, t("simple.dp.section"));
    let mut changed = false;
    {
        let a = &mut p.simple.design_point;
        changed |= pick(
            ui,
            "simple.dp.q.speed",
            &mut a.speed,
            dpm::SpeedClass::ALL,
            dpm::SpeedClass::key,
            dpm::SpeedClass::desc_key,
        );
        changed |= pick(
            ui,
            "simple.dp.q.wind",
            &mut a.wind,
            dpm::Wind::ALL,
            dpm::Wind::key,
            dpm::Wind::desc_key,
        );
        changed |= pick(
            ui,
            "simple.dp.q.climb",
            &mut a.climb,
            dpm::Climb::ALL,
            dpm::Climb::key,
            dpm::Climb::desc_key,
        );
    }
    if changed {
        changed_apply(p, m, lib, SimpleStep::DesignPoint);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::DesignPoint);
    badge(ui, p, SimpleStep::DesignPoint);
    let launch = p.simple.mission.launch;
    let (_, nts) = dpm::derive(&p.simple.design_point, launch);
    notes(ui, &nts);
    ui.add_space(6.0);
}

fn dp_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    let launch = p.simple.mission.launch;
    let wind = p.simple.design_point.wind;
    section_header(ui, t("simple.dp.section"));
    line(
        ui,
        dpm::wind_headway(p.v_cr, wind),
        tf(
            "simple.dp.res_wind",
            &[
                fmt(presets::kmh(p.v_cr), 0),
                fmt(presets::kmh(wind.speed_ms()), 0),
            ],
        ),
        Some(t("simple.dp.tip_wind")),
    );
    if launch == mis::Launch::Hand && m.inputs_ok {
        line(
            ui,
            dpm::hand_launch_speed_rating(p.v_launch, m.cons.v_stall),
            tf(
                "simple.dp.res_hand",
                &[
                    fmt(presets::kmh(p.v_launch), 0),
                    fmt(presets::kmh(dpm::HAND_THROW_MAX), 0),
                ],
            ),
            Some(t("simple.dp.tip_hand")),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 6. Menzil / Havada kalma (yalnızca sonuç)
// ---------------------------------------------------------------------------------------------

fn endurance_out(ui: &mut Ui, p: &mut Project, m: &Model, mr: bool) {
    results_header(ui);
    if !m.inputs_ok {
        ui.label("—");
        return;
    }
    let target = p.simple.mission.target_minutes();
    let facts = if mr {
        endm::Facts {
            endurance_min: m.mr.hover_endurance_min,
            range_km: m.mr.range_km,
            target_min: target,
            v_cruise: p.mr_v_cruise,
            v_endurance: f64::NAN,
            ld: f64::NAN,
            battery_fraction: if m.mr.m_mtow > 0.0 {
                m.weight.m_battery / m.mr.m_mtow
            } else {
                f64::NAN
            },
            multirotor: true,
        }
    } else {
        endm::Facts {
            endurance_min: m.perf_end.endurance_min(),
            range_km: m.perf_range.range_km(),
            target_min: target,
            v_cruise: p.v_cr,
            v_endurance: m.perf_end.v,
            ld: m.perf_cr.ld,
            battery_fraction: m.weight.f_batt,
            multirotor: false,
        }
    };
    let rating = endm::rating(facts.endurance_min, facts.target_min);
    line(
        ui,
        rating,
        tf(
            if mr {
                "simple.end.res_time_mr"
            } else {
                "simple.end.res_time"
            },
            &[
                fmt(facts.endurance_min, 0),
                t(endm::time_analogy(facts.endurance_min)).to_string(),
                fmt(facts.target_min, 0),
            ],
        ),
        None,
    );
    let km = facts.range_km;
    text(
        ui,
        &tf(
            "simple.end.res_range",
            &[fmt(km, 1), fmt(endm::football_laps(km), 0)],
        ),
    );
    for tip in endm::tips(&facts) {
        step_ui::note(ui, &format!("- {}", tf(tip.key, &tip.args)));
    }
}

// ---------------------------------------------------------------------------------------------
// 7. İtki
// ---------------------------------------------------------------------------------------------

fn propulsion_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.prop.intro");
    let mut changed = false;
    {
        let a = &mut p.simple.propulsion;
        changed |= pick(
            ui,
            "simple.prop.q.priority",
            &mut a.priority,
            prp::Priority::ALL,
            prp::Priority::key,
            prp::Priority::desc_key,
        );
    }
    // "Elle fırlatma var mı?" görev adımındaki cevapla ortaktır.
    let mut hand = p.simple.mission.launch == mis::Launch::Hand;
    if ui
        .checkbox(&mut hand, t("simple.prop.hand"))
        .on_hover_text(t("simple.prop.hand.desc"))
        .changed()
    {
        p.simple.mission.launch = if hand {
            mis::Launch::Hand
        } else {
            mis::Launch::Runway
        };
        changed = true;
        changed_apply(p, m, lib, SimpleStep::DesignPoint);
    }
    if changed {
        changed_apply(p, m, lib, SimpleStep::Propulsion);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::Propulsion);
    badge(ui, p, SimpleStep::Propulsion);
}

fn propulsion_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    if m.inputs_ok {
        let o = &m.prop;
        text(
            ui,
            &tf(
                "simple.prop.res_motor",
                &[
                    t(prp::motor_class_key(o.p_motor_suggest)).to_string(),
                    fmt(o.p_motor_suggest, 0),
                ],
            ),
        );
        let (lo, hi) = prp::prop_diameter_range(o.t_static);
        text(
            ui,
            &tf(
                "simple.prop.res_prop",
                &[
                    fmt(lo / 0.0254, 0),
                    fmt(hi / 0.0254, 0),
                    fmt(p.d_prop / 0.0254, 0),
                ],
            ),
        );
        line(
            ui,
            if o.installed_ok {
                Rating::Good
            } else {
                Rating::Bad
            },
            tf(
                "simple.prop.res_installed",
                &[fmt(p.p_max_elec, 0), fmt(o.p_motor_suggest, 0)],
            ),
            Some(t("simple.prop.tip_installed")),
        );
        line(
            ui,
            if o.current_ok {
                Rating::Good
            } else {
                Rating::Bad
            },
            tf("simple.prop.res_current", &[fmt(o.i_peak, 0)]),
            Some(t("simple.prop.tip_current")),
        );
    }
}

fn mr_propulsion_in(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    intro(ui, "simple.prop.mr_intro");
    let mut changed = false;
    {
        let a = &mut p.simple.propulsion;
        changed |= pick(
            ui,
            "simple.prop.q.mr_style",
            &mut a.mr_style,
            prp::MrStyle::ALL,
            prp::MrStyle::key,
            prp::MrStyle::desc_key,
        );
    }
    if changed {
        changed_apply(p, m, lib, SimpleStep::Propulsion);
    }
    inactive_hint(ui, p, m, lib, SimpleStep::Propulsion);
    badge(ui, p, SimpleStep::Propulsion);
}

fn mr_propulsion_out(ui: &mut Ui, p: &Project, m: &Model) {
    results_header(ui);
    if m.inputs_ok {
        line(
            ui,
            prp::hover_throttle_rating(m.mr.hover_throttle),
            tf(
                "simple.prop.res_hover",
                &[pct(m.mr.hover_throttle), fmt(m.mr.tw_ratio, 1)],
            ),
            Some(t("simple.prop.tip_hover")),
        );
        text(
            ui,
            &tf(
                "simple.prop.res_mr_thrust",
                &[
                    fmt(p.mr_t_max_motor / 9.80665 * 1000.0, 0),
                    m.mr.n_motors.to_string(),
                ],
            ),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 8. Ağırlık merkezi & stabilite (yalnızca sonuç + tek seferlik batarya düğmesi)
// ---------------------------------------------------------------------------------------------

fn stability_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    if !m.inputs_ok {
        ui.label("—");
        return;
    }
    let s = &m.stab;
    let (r, key) = stb::rate_static_margin(s.sm, s.band);
    line(
        ui,
        r,
        tf(
            "simple.stab.res_sm",
            &[fmt(s.sm * 100.0, 1), t(key).to_string()],
        ),
        Some(t("simple.stab.tip_sm")),
    );
    match stb::battery_move(s.batt_x_shift) {
        None => text(ui, t("simple.stab.batt_ok")),
        Some(mv) => text(
            ui,
            &tf(
                if mv.backward {
                    "simple.stab.batt_back"
                } else {
                    "simple.stab.batt_fwd"
                },
                &[fmt(mv.mm, 0)],
            ),
        ),
    }
    if s.batt_row.is_some() {
        if ui
            .button(t("simple.stab.batt_btn"))
            .on_hover_text(t("simple.stab.batt_btn.desc"))
            .clicked()
        {
            stb::apply_battery_shift(&mut p.components, &m.stab);
        }
    } else {
        step_ui::note(ui, t("stab.no_batt_row_err"));
    }
    text(
        ui,
        &tf(
            "simple.stab.balance",
            &[fmt(
                stb::balance_point_behind_wing_le_mm(s.x_cg_target_nose, p.x_root_le),
                0,
            )],
        ),
    );
    step_ui::note(ui, t("simple.stab.balance_how"));
}

fn mr_cg_out(ui: &mut Ui, m: &Model) {
    results_header(ui);
    if !m.inputs_ok {
        ui.label("—");
        return;
    }
    line(
        ui,
        stb::mr_cg_rating(m.mr.cg_offset_frac),
        tf(
            "simple.stab.res_mr_cg",
            &[fmt(m.mr.cg_offset_d * 1000.0, 0)],
        ),
        Some(t("simple.stab.tip_mr_cg")),
    );
}

// ---------------------------------------------------------------------------------------------
// 9. Yapı (kanat kirişi / kol borusu)
// ---------------------------------------------------------------------------------------------

fn ms_rating(ms: f64, target: f64) -> Rating {
    if !ms.is_finite() {
        Rating::Bad
    } else if ms >= target {
        Rating::Good
    } else if ms >= 0.0 {
        Rating::Ok
    } else {
        Rating::Bad
    }
}

/// Dayanım sorusu ve "uygun boruyu seç" düğmesi (kanat kirişi ya da multirotor kolu).
fn tube_in(ui: &mut Ui, p: &mut Project, lib: &Library, arm: bool) {
    intro(
        ui,
        if arm {
            "simple.struct.mr_intro"
        } else {
            "simple.struct.intro"
        },
    );
    pick(
        ui,
        "simple.struct.q.strength",
        &mut p.simple.structure.strength,
        stc::Strength::ALL,
        stc::Strength::key,
        stc::Strength::desc_key,
    );
    let target = p.simple.structure.strength.target_ms();
    let id = egui::Id::NULL.with(if arm {
        "simple_arm_pick"
    } else {
        "simple_spar_pick"
    });
    if ui
        .button(t("simple.struct.auto"))
        .on_hover_text(t("simple.struct.auto.desc"))
        .clicked()
    {
        let txt = if arm {
            match stc::auto_arm(p, lib, target) {
                Some(tp) => {
                    stc::apply_arm(p, &tp);
                    tf(
                        if tp.meets_target {
                            "simple.struct.picked_arm"
                        } else {
                            "simple.struct.picked_arm_weak"
                        },
                        &[fmt(tp.d_o_mm, 0), fmt(tp.d_i_mm, 0), fmt(tp.mass_g, 0)],
                    )
                }
                None => t("simple.struct.none_arm").to_string(),
            }
        } else {
            match stc::auto_spar(p, lib, target) {
                Some(tp) => {
                    stc::apply_spar(p, &tp);
                    tf(
                        if tp.meets_target {
                            "simple.struct.picked"
                        } else {
                            "simple.struct.picked_weak"
                        },
                        &[fmt(tp.d_o_mm, 0), fmt(tp.d_i_mm, 0), fmt(tp.mass_g, 0)],
                    )
                }
                None => t("simple.struct.none").to_string(),
            }
        };
        ui.ctx().data_mut(|d| d.insert_temp(id, txt));
    }
    if let Some(txt) = ui.ctx().data(|d| d.get_temp::<String>(id)) {
        ui.add(egui::Label::new(RichText::new(txt).strong()).wrap());
    }
}

fn spar_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    let target = p.simple.structure.strength.target_ms();
    if m.inputs_ok {
        let ms = m.st.ms;
        line(
            ui,
            ms_rating(ms, target),
            tf(
                "simple.struct.res_spar",
                &[
                    fmt(p.d_spar_o * 1000.0, 0),
                    fmt(p.d_spar_i * 1000.0, 0),
                    fmt(p.fs * (1.0 + ms), 1),
                ],
            ),
            Some(t("simple.struct.tip_spar")),
        );
    }
}

fn arm_out(ui: &mut Ui, p: &mut Project, m: &Model) {
    results_header(ui);
    let target = p.simple.structure.strength.target_ms();
    if m.inputs_ok {
        let r = &m.mr_st;
        let rating = if r.deflection_ok {
            ms_rating(r.ms, target)
        } else {
            Rating::Bad
        };
        line(
            ui,
            rating,
            tf(
                "simple.struct.res_arm",
                &[
                    fmt(p.mr_arm_d_o * 1000.0, 0),
                    fmt(p.mr_arm_d_i * 1000.0, 0),
                    fmt(p.mr_arm_fs * (1.0 + r.ms), 1),
                ],
            ),
            Some(t("simple.struct.tip_arm")),
        );
        if !r.deflection_ok {
            step_ui::status_text(
                ui,
                Status::Warn,
                &format!("! {}", t("simple.struct.arm_flex")),
            );
        }
    }
}
