// SPDX-License-Identifier: AGPL-3.0-or-later
//! Acoustics tab: rough propeller-noise estimate (tip speed and Mach, blade-pass frequency,
//! overall SPL and dBA at an observer, spectrum) and a ranked table of quieter propeller
//! alternatives at equal thrust. The tab only reads the project; nothing is changed.
use super::components::{self, MetricCard, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::acoustics::alternatives::{Alternative, alternatives};
use crate::acoustics::method::{self, Estimate, MachBand, Observer, estimate, mach_band};
use crate::acoustics::sources::{
    DEFAULT_PUSHER_BLADES, OpPoint, Source, air_of, points_for, source_for,
};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{RichText, Ui};
use egui_plot::{HoverPosition, Legend, Line, Plot, PlotPoints};

/// User inputs of the tab (not stored in the project file).
#[derive(Clone, Debug)]
pub struct AcState {
    /// Observer distance (m).
    pub distance_m: f64,
    /// Add the +3 dB ground reflection.
    pub ground: bool,
    /// Selected operating point (falls back to the first one the vehicle has).
    pub point: OpPoint,
    /// Blade count of the forward propeller (the project has no such input).
    pub forward_blades: u32,
}

impl Default for AcState {
    fn default() -> Self {
        AcState {
            distance_m: 10.0,
            ground: false,
            point: OpPoint::LiftHover,
            forward_blades: DEFAULT_PUSHER_BLADES,
        }
    }
}

fn has_forward(point: OpPoint) -> bool {
    matches!(point, OpPoint::Cruise | OpPoint::PusherMax)
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut AcState) {
    section_header(ui, t("acoustics.tab"));
    components::banner(ui, Status::Warn, t("acoustics.banner"));
    ui.add_space(space::XS);

    let pts = points_for(p.drone_type);
    if !pts.contains(&st.point) {
        st.point = pts[0];
    }
    st.distance_m = if st.distance_m.is_finite() {
        st.distance_m.clamp(0.5, 5000.0)
    } else {
        10.0
    };
    st.forward_blades = st.forward_blades.clamp(1, 8);

    inputs(ui, p, st, pts);

    let Some(src) = source_for(p, m, st.point, st.forward_blades) else {
        return;
    };
    let air = air_of(m);
    let obs = Observer {
        distance_m: st.distance_m,
        ground_reflection: st.ground,
    };
    let est = estimate(&src.rotors, &air, &obs);

    ui.label(
        RichText::new(tf(
            "acoustics.setup_fmt",
            &[
                src.rotors.n_rotors.to_string(),
                fmt(src.rotors.diameter_m * 1000.0, 0),
                src.rotors.blades.to_string(),
                fmt(src.rotors.rpm, 0),
                fmt(src.rotors.shaft_power_w, 0),
                fmt(src.thrust_n, 1),
            ],
        ))
        .weak(),
    );
    if !est.valid {
        components::banner(ui, Status::Err, t("acoustics.invalid"));
        return;
    }
    warnings(ui, &est);
    cards(ui, &est, &src, &obs);
    harmonics(ui, &est);
    spectrum(ui, &est);
    alternatives_table(ui, &src, &air, &obs);

    section(ui, t("acoustics.method.title"));
    ui.label(RichText::new(t("acoustics.method.body")).weak().small());
}

fn inputs(ui: &mut Ui, p: &Project, st: &mut AcState, pts: &[OpPoint]) {
    section(ui, t("acoustics.inputs"));
    if pts.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.label(t("acoustics.point"));
            let opts: Vec<(OpPoint, &str, &str)> =
                pts.iter().map(|pt| (*pt, t(pt.label_key()), "")).collect();
            components::segmented(ui, &mut st.point, &opts);
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(t("acoustics.distance"));
        ui.add(
            egui::DragValue::new(&mut st.distance_m)
                .range(0.5..=5000.0)
                .speed(0.5)
                .suffix(" m"),
        );
        ui.add_space(space::M);
        ui.checkbox(&mut st.ground, t("acoustics.ground"))
            .on_hover_text(t("acoustics.ground_tip"));
        if has_forward(st.point) && p.drone_type.has_wing() {
            ui.add_space(space::M);
            ui.label(t("acoustics.blades_fwd"))
                .on_hover_text(t("acoustics.blades_fwd_tip"));
            ui.add(egui::DragValue::new(&mut st.forward_blades).range(1..=8));
        }
    });
}

fn warnings(ui: &mut Ui, e: &Estimate) {
    match mach_band(e.helical_mach) {
        MachBand::Ok => {}
        MachBand::Caution => {
            components::banner(ui, Status::Warn, t("acoustics.mach_caution"));
        }
        MachBand::High => {
            components::banner(ui, Status::Err, t("acoustics.mach_high"));
        }
    }
    if e.near_field {
        components::banner(ui, Status::Info, t("acoustics.near_field"));
    }
}

fn cards(ui: &mut Ui, e: &Estimate, src: &Source, obs: &Observer) {
    let mach_status = match mach_band(e.helical_mach) {
        MachBand::Ok => Status::Ok,
        MachBand::Caution => Status::Warn,
        MachBand::High => Status::Err,
    };
    let list = [
        MetricCard {
            title: t("acoustics.card.tip_speed"),
            value: fmt(e.tip_speed, 1),
            unit: "m/s",
            detail: String::new(),
            status: Status::Neutral,
            why: false,
            tooltip: "",
            clickable: false,
        },
        MetricCard {
            title: t("acoustics.card.tip_mach"),
            value: fmt(e.tip_mach, 2),
            unit: "",
            detail: tf("acoustics.card.detail_mach_fmt", &[fmt(e.helical_mach, 2)]),
            status: mach_status,
            why: false,
            tooltip: "",
            clickable: false,
        },
        MetricCard {
            title: t("acoustics.card.bpf"),
            value: fmt(e.bpf, 0),
            unit: "Hz",
            detail: tf(
                "acoustics.card.detail_bpf_fmt",
                &[fmt(e.rev_per_s, 1), src.rotors.blades.to_string()],
            ),
            status: Status::Neutral,
            why: false,
            tooltip: "",
            clickable: false,
        },
        MetricCard {
            title: t("acoustics.card.spl"),
            value: fmt(e.oaspl, 1),
            unit: "dB",
            detail: tf(
                "acoustics.card.detail_spl_fmt",
                &[fmt(e.tonal_spl, 1), fmt(e.broadband_spl, 1)],
            ),
            status: Status::Neutral,
            why: false,
            tooltip: "",
            clickable: false,
        },
        MetricCard {
            title: t("acoustics.card.dba"),
            value: fmt(e.oaspl_a, 1),
            unit: "dBA",
            detail: tf(
                "acoustics.card.detail_dba_fmt",
                &[fmt(obs.distance_m, 0), src.rotors.n_rotors.to_string()],
            ),
            status: Status::Neutral,
            why: false,
            tooltip: "",
            clickable: false,
        },
    ];
    ui.add_space(space::XS);
    components::card::card_grid(ui, list.len(), 200.0, |ui, i, w| {
        components::metric_card(ui, &list[i], w);
    });
}

fn harmonics(ui: &mut Ui, e: &Estimate) {
    section(ui, t("acoustics.harmonics"));
    egui::Grid::new("ac_harmonics")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for h in [
                "acoustics.harm.k",
                "acoustics.harm.freq",
                "acoustics.harm.db",
                "acoustics.harm.dba",
            ] {
                ui.label(RichText::new(t(h)).strong());
            }
            ui.end_row();
            for tone in &e.spectrum.tones {
                ui.label(tone.k.to_string());
                ui.label(fmt(tone.f, 0));
                ui.label(fmt(tone.spl, 1));
                ui.label(fmt(tone.spl_a, 1));
                ui.end_row();
            }
        });
}

fn spectrum(ui: &mut Ui, e: &Estimate) {
    let tk = theme::tokens(ui);
    section(ui, t("acoustics.spectrum"));
    let bands = &e.spectrum.bands;
    let top = bands
        .iter()
        .map(|b| b.spl)
        .fold(f64::NEG_INFINITY, f64::max);
    let floor = if top.is_finite() { top - 60.0 } else { 0.0 };
    let line = |f: &dyn Fn(&method::Band) -> f64| -> PlotPoints<'static> {
        PlotPoints::from(
            bands
                .iter()
                .map(|b| [b.fc.log10(), f(b).max(floor)])
                .collect::<Vec<_>>(),
        )
    };
    Plot::new("ac_spectrum")
        .height(280.0)
        .legend(Legend::default())
        .allow_scroll(false)
        .x_axis_label(t("acoustics.axis_freq"))
        .y_axis_label(t("acoustics.axis_level"))
        .include_y(floor)
        .include_y(top + 3.0)
        .x_axis_formatter(|mark, _| format!("{:.0}", 10f64.powf(mark.value)))
        .label_formatter(|pos| {
            let (name, v) = match pos {
                HoverPosition::NearDataPoint {
                    plot_name,
                    position,
                    ..
                } => (*plot_name, *position),
                HoverPosition::Elsewhere { position } => ("", *position),
            };
            Some(format!("{name}\n{:.0} Hz, {:.1} dB", 10f64.powf(v.x), v.y))
        })
        .show(ui, |pu| {
            pu.line(
                Line::new(t("acoustics.spec.bands"), line(&|b| b.spl))
                    .color(tk.accent)
                    .width(2.0),
            );
            pu.line(
                Line::new(t("acoustics.spec.bands_a"), line(&|b| b.spl_a))
                    .color(tk.info)
                    .width(2.0),
            );
            for tone in &e.spectrum.tones {
                if !is_plottable_tone_freq(tone.f) {
                    continue;
                }
                let x = tone.f.log10();
                let stem = PlotPoints::from(vec![[x, floor], [x, tone.spl.max(floor)]]);
                pu.line(
                    Line::new(t("acoustics.spec.tones"), stem)
                        .color(tk.warning)
                        .width(1.5),
                );
            }
        });
    ui.label(RichText::new(t("acoustics.spec_note")).weak());
}

fn is_plottable_tone_freq(f: f64) -> bool {
    crate::acoustics::weighting::band_index(f).is_some()
}

fn alternatives_table(ui: &mut Ui, src: &Source, air: &method::Air, obs: &Observer) {
    let tk = theme::tokens(ui);
    section(ui, t("acoustics.alt.title"));
    ui.label(t("acoustics.alt.intro"));
    let rows: Vec<Alternative> = alternatives(src, air, obs);
    if rows.is_empty() {
        return;
    }
    egui::ScrollArea::horizontal()
        .id_salt("ac_alt_scroll")
        .show(ui, |ui| {
            egui::Grid::new("ac_alt_grid")
                .striped(true)
                .num_columns(11)
                .show(ui, |ui| {
                    for h in [
                        "acoustics.alt.rank",
                        "acoustics.alt.diameter",
                        "acoustics.alt.blades",
                        "acoustics.alt.rpm",
                        "acoustics.alt.power",
                        "acoustics.alt.mach",
                        "acoustics.alt.bpf",
                        "acoustics.alt.bpf_shift",
                        "acoustics.alt.dba",
                        "acoustics.alt.delta",
                        "acoustics.alt.note",
                    ] {
                        ui.label(RichText::new(t(h)).strong());
                    }
                    ui.end_row();
                    for (i, r) in rows.iter().enumerate() {
                        let cell = |s: String| {
                            if r.is_current {
                                RichText::new(s).strong()
                            } else if !r.fits_frame {
                                RichText::new(s).weak()
                            } else {
                                RichText::new(s)
                            }
                        };
                        ui.label(cell((i + 1).to_string()));
                        ui.label(cell(fmt(r.diameter_m * 1000.0, 0)));
                        ui.label(cell(r.blades.to_string()));
                        ui.label(cell(fmt(r.rpm, 0)));
                        ui.label(cell(fmt(r.shaft_power_w, 0)));
                        ui.label(cell(fmt(r.tip_mach, 2)));
                        ui.label(cell(fmt(r.bpf, 0)));
                        ui.label(cell(format!("{:+.0}", r.bpf_shift_hz)));
                        ui.label(cell(fmt(r.oaspl_a, 1)));
                        let delta = RichText::new(format!("{:+.1}", r.delta_dba));
                        ui.label(if r.delta_dba < -0.05 {
                            delta.color(tk.success)
                        } else if r.delta_dba > 0.05 {
                            delta.color(tk.warning)
                        } else {
                            delta
                        });
                        let note = if r.is_current {
                            t("acoustics.alt.current")
                        } else if !r.fits_frame {
                            t("acoustics.alt.too_big")
                        } else {
                            ""
                        };
                        ui.label(RichText::new(note).weak());
                        ui.end_row();
                    }
                });
        });
    ui.label(RichText::new(t("acoustics.alt.caveat")).weak().small());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;
    use egui::{Pos2, Rect, vec2};

    fn run_frame(ctx: &egui::Context, f: &mut dyn FnMut(&mut egui::Ui)) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 900.0)));
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
    }

    #[test]
    fn panel_renders_for_every_vehicle_theme_and_language() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = evaluate(&p, &Library::embedded());
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    crate::i18n::with_lang(lang, || {
                        for pt in points_for(dt) {
                            let mut st = AcState {
                                point: *pt,
                                ground: true,
                                ..AcState::default()
                            };
                            for _ in 0..2 {
                                run_frame(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
                            }
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn panel_survives_degenerate_project_values() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        p.mr_d_prop = 0.0;
        p.d_prop = f64::NAN;
        p.v_cr = f64::NAN;
        let m = evaluate(&p, &Library::embedded());
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        for pt in points_for(p.drone_type) {
            let mut st = AcState {
                point: *pt,
                distance_m: f64::NAN,
                forward_blades: 0,
                ..AcState::default()
            };
            run_frame(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
        }
    }

    #[test]
    fn state_falls_back_to_a_point_the_vehicle_has() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::FlyingWing);
        let m = evaluate(&p, &Library::embedded());
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Light);
        let mut st = AcState::default();
        assert_eq!(st.point, OpPoint::LiftHover);
        run_frame(&ctx, &mut |ui| show(ui, &p, &m, &mut st));
        assert_eq!(st.point, OpPoint::Cruise);
    }

    #[test]
    fn extreme_tone_frequencies_are_excluded_from_plot() {
        assert!(is_plottable_tone_freq(300.0));
        assert!(!is_plottable_tone_freq(5.0));
        assert!(!is_plottable_tone_freq(1e158));
        assert!(!is_plottable_tone_freq(f64::NAN));
    }
}
