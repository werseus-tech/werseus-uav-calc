// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kablolama sekmesi: kablo demeti listesi, blok şeması ve FC pinout planlayıcı.
//! Durum egui belleğindedir; proje dosyasına yazılmaz.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use crate::wiring::harness::{Func, Harness, Settings, Wire, WireKind, block_layout};
use crate::wiring::pinout::PresetId;
use crate::wiring::pinout::{
    Conflict, PRESETS, Warning, assign, pool_size, preset, requirements, slot_name, to_csv,
    to_markdown,
};
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::HashMap;

/// Kablonun kararlı kimliği (sıra numarası değil): tür, indeks, faz.
type LenKey = (WireKind, Option<u32>, Option<u8>);

fn len_key(w: &Wire) -> LenKey {
    (w.kind, w.idx, w.phase)
}

#[derive(Clone)]
struct WState {
    drone: DroneType,
    preset: PresetId,
    slack_pct: f64,
    limit_pct: f64,
    lens: HashMap<LenKey, f64>,
    pins: HashMap<String, usize>,
    message: Option<String>,
}

impl WState {
    fn new(drone: DroneType) -> Self {
        WState {
            drone,
            preset: PresetId::F7,
            slack_pct: 10.0,
            limit_pct: 2.0,
            lens: HashMap::new(),
            pins: HashMap::new(),
            message: None,
        }
    }
}

fn save_text(name: &str, text: &str) -> Option<String> {
    let path = rfd::FileDialog::new().set_file_name(name).save_file()?;
    Some(match std::fs::write(&path, text) {
        Ok(()) => tf("wiring.saved_fmt", &[path.to_string_lossy().as_ref()]),
        Err(e) => tf("wiring.save_failed_fmt", &[e.to_string()]),
    })
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model) {
    let tk = theme::tokens(ui);
    let id = egui::Id::unique("wiring_state");
    let mut st: WState = ui
        .data(|d| d.get_temp::<WState>(id))
        .filter(|s| s.drone == p.drone_type)
        .unwrap_or_else(|| WState::new(p.drone_type));

    section_header(ui, t("tab_wiring"));
    components::banner(ui, Status::Info, t("wiring.disclaimer"));
    ui.add_space(space::XS);

    let mut h = Harness::default_for(p, m);
    for w in h.wires.iter_mut() {
        if let Some(l) = st.lens.get(&len_key(w)) {
            w.length_override_m = Some(*l);
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(t("wiring.drop_limit"));
        ui.add(
            egui::DragValue::new(&mut st.limit_pct)
                .range(0.2..=10.0)
                .speed(0.1)
                .suffix(" %"),
        );
        ui.label(t("wiring.slack"));
        ui.add(
            egui::DragValue::new(&mut st.slack_pct)
                .range(0.0..=50.0)
                .speed(0.5)
                .suffix(" %"),
        );
        if ui.button(t("wiring.reset_lengths")).clicked() {
            st.lens.clear();
        }
    });
    let set = Settings {
        slack: st.slack_pct / 100.0,
        drop_limit_pct: st.limit_pct,
    };
    let rep = h.evaluate(&set);

    ui.horizontal_wrapped(|ui| {
        ui.label(tf("wiring.total_mass_fmt", &[fmt(rep.mass_g, 1)]));
        ui.separator();
        let col = if rep.worst_drop_pct > st.limit_pct {
            tk.danger
        } else {
            tk.success
        };
        ui.colored_label(
            col,
            tf("wiring.worst_drop_fmt", &[fmt(rep.worst_drop_pct, 2)]),
        );
        ui.separator();
        let ccol = if rep.chain_drop_pct > 2.0 * st.limit_pct {
            tk.warning
        } else {
            tk.text_weak
        };
        ui.colored_label(
            ccol,
            tf("wiring.chain_drop_fmt", &[fmt(rep.chain_drop_pct, 2)]),
        );
        if rep.n_over_limit > 0 {
            ui.separator();
            ui.colored_label(
                tk.warning,
                tf("wiring.over_limit_fmt", &[rep.n_over_limit.to_string()]),
            );
        }
    });

    // --- Blok şeması
    section(ui, t("wiring.diagram"));
    draw_diagram(ui, &h);

    // --- Demet tablosu
    section(ui, t("wiring.harness"));
    egui::Grid::new("wiring_harness_grid")
        .striped(true)
        .num_columns(9)
        .show(ui, |ui| {
            for c in [
                "wiring.col.wire",
                "wiring.col.route",
                "wiring.col.length",
                "wiring.col.current",
                "wiring.col.awg",
                "wiring.col.drop",
                "wiring.col.mass",
                "wiring.col.connector",
                "wiring.col.function",
            ] {
                ui.strong(t(c));
            }
            ui.end_row();
            for (w, r) in h.wires.iter().zip(&rep.rows) {
                ui.label(h.wire_label(w));
                ui.label(format!(
                    "{} -> {}",
                    h.nodes[w.from].label(),
                    h.nodes[w.to].label()
                ));
                let mut len = r.length_m;
                if ui
                    .add(
                        egui::DragValue::new(&mut len)
                            .range(0.02..=20.0)
                            .speed(0.01)
                            .fixed_decimals(2)
                            .suffix(" m"),
                    )
                    .changed()
                {
                    st.lens.insert(len_key(w), len);
                }
                ui.label(format!("{} A", fmt(w.current_a, 2)));
                match r.awg {
                    Some(a) => {
                        let txt = format!("{a}");
                        if r.over_limit {
                            ui.colored_label(tk.danger, txt);
                        } else {
                            ui.label(txt);
                        }
                    }
                    None => {
                        ui.label(t("wiring.coax"));
                    }
                }
                let dtxt = format!("{} V ({} %)", fmt(r.drop_v, 3), fmt(r.drop_pct, 2));
                if r.drop_pct > st.limit_pct {
                    ui.colored_label(tk.danger, dtxt);
                } else {
                    ui.label(dtxt);
                }
                ui.label(format!("{} g", fmt(r.mass_g, 2)));
                ui.label(r.connector.label());
                ui.label(func_name(w.func()));
                ui.end_row();
            }
        });
    ui.label(egui::RichText::new(t("wiring.typical_note")).color(tk.text_weak));
    ui.horizontal(|ui| {
        if ui.button(t("wiring.export_csv")).clicked() {
            st.message = save_text("harness.csv", &h.to_csv(&rep));
        }
        if ui.button(t("wiring.copy_csv")).clicked() {
            ui.ctx().copy_text(h.to_csv(&rep));
        }
    });

    // --- Pinout
    section(ui, t("wiring.pinout.title"));
    components::banner(ui, Status::Info, t("wiring.pinout.generic"));
    ui.horizontal(|ui| {
        ui.label(t("wiring.preset"));
        for pr in PRESETS {
            if ui.selectable_label(st.preset == pr.id, pr.name).clicked() {
                st.preset = pr.id;
                st.pins.clear();
            }
        }
    });
    let pre = preset(st.preset);
    let reqs = requirements(&h);
    let out = assign(&pre, &reqs, &st.pins);
    ui.label(tf(
        "wiring.preset_info_fmt",
        &[
            pre.uarts.to_string(),
            pre.outputs.to_string(),
            pre.motor_outputs.to_string(),
            pre.i2c.to_string(),
            pre.adc.to_string(),
        ],
    ));
    if out.conflicts.is_empty() {
        ui.colored_label(tk.success, t("wiring.no_conflict"));
    }
    for c in &out.conflicts {
        ui.colored_label(tk.danger, conflict_text(c));
    }
    for w in &out.warnings {
        let Warning::DshotGroupShared { group } = w;
        ui.colored_label(
            tk.warning,
            tf("wiring.warn.dshot_group_fmt", &[group.to_string()]),
        );
    }
    ui.label(egui::RichText::new(t("wiring.warn.dshot_note")).color(tk.text_weak));
    egui::Grid::new("wiring_pinout_grid")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for c in [
                "wiring.col.function",
                "wiring.col.pin",
                "wiring.col.level",
                "wiring.col.status",
            ] {
                ui.strong(t(c));
            }
            ui.end_row();
            for (r, row) in reqs.iter().zip(&out.rows) {
                ui.label(r.label());
                let cur = row
                    .slot
                    .map(|s| slot_name(r.kind, s))
                    .unwrap_or_else(|| t("wiring.unassigned").to_string());
                egui::ComboBox::from_id_salt(("wiring_pin", r.id.as_str()))
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(!st.pins.contains_key(&r.id), t("wiring.auto"))
                            .clicked()
                        {
                            st.pins.remove(&r.id);
                        }
                        for s in 0..pool_size(&pre, r.kind) {
                            let sel = st.pins.get(&r.id) == Some(&s);
                            if ui.selectable_label(sel, slot_name(r.kind, s)).clicked() {
                                st.pins.insert(r.id.clone(), s);
                            }
                        }
                    });
                ui.label(t(r.level_key));
                if row.conflict {
                    ui.colored_label(tk.danger, t("wiring.status.conflict"));
                } else {
                    ui.colored_label(tk.success, t("wiring.status.ok"));
                }
                ui.end_row();
            }
        });
    ui.horizontal(|ui| {
        if ui.button(t("wiring.export_pinout_md")).clicked() {
            st.message = save_text("pinout.md", &to_markdown(&pre, &reqs, &out));
        }
        if ui.button(t("wiring.export_pinout_csv")).clicked() {
            st.message = save_text("pinout.csv", &to_csv(&pre, &reqs, &out));
        }
        if ui.button(t("wiring.copy_pinout")).clicked() {
            ui.ctx().copy_text(to_markdown(&pre, &reqs, &out));
        }
    });
    if let Some(msg) = &st.message {
        ui.label(msg.as_str());
    }

    ui.data_mut(|d| d.insert_temp(id, st));
}

fn func_name(f: Func) -> &'static str {
    match f {
        Func::Power => t("wiring.func.power"),
        Func::Signal => t("wiring.func.signal"),
        Func::Rf => t("wiring.func.rf"),
    }
}

fn conflict_text(c: &Conflict) -> String {
    match c {
        Conflict::Exhausted { kind, need, have } => tf(
            "wiring.conflict.exhausted_fmt",
            &[
                kind_name(*kind).to_string(),
                need.to_string(),
                have.to_string(),
            ],
        ),
        Conflict::MotorOutputs { need, have } => tf(
            "wiring.conflict.motors_fmt",
            &[need.to_string(), have.to_string()],
        ),
        Conflict::Duplicate { slot } => tf("wiring.conflict.duplicate_fmt", &[slot.as_str()]),
        Conflict::MotorNotCapable { req } => tf("wiring.conflict.motor_cap_fmt", &[req.as_str()]),
        Conflict::Unassigned { req } => tf("wiring.conflict.unassigned_fmt", &[req.as_str()]),
    }
}

fn kind_name(k: crate::wiring::pinout::ReqKind) -> &'static str {
    use crate::wiring::pinout::ReqKind::*;
    match k {
        Motor | Servo => t("wiring.kind.output"),
        Uart => t("wiring.kind.uart"),
        I2c => t("wiring.kind.i2c"),
        Adc => t("wiring.kind.adc"),
    }
}

fn draw_diagram(ui: &mut Ui, h: &Harness) {
    let tk = theme::tokens(ui);
    let width = ui.available_width().max(320.0);
    let (resp, painter) = ui.allocate_painter(Vec2::new(width, 340.0), Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, theme::corner(theme::radius::M), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(theme::radius::M),
        Stroke::new(1.0, tk.border),
        egui::StrokeKind::Inside,
    );
    let lay = block_layout(&h.nodes);
    let at = |k: usize| -> Pos2 {
        let q = lay[k];
        Pos2::new(
            rect.left() + 8.0 + (rect.width() - 16.0) * q[0],
            rect.top() + 8.0 + (rect.height() - 16.0) * q[1],
        )
    };
    let color = |f: Func| match f {
        Func::Power => tk.danger,
        Func::Signal => tk.info,
        Func::Rf => tk.success,
    };
    for w in &h.wires {
        let off = w.phase.map(|p| (p as f32 - 1.0) * 2.0).unwrap_or(0.0);
        let (a, b) = (at(w.from), at(w.to));
        let (a, b) = (Pos2::new(a.x, a.y + off), Pos2::new(b.x, b.y + off));
        let width = if w.func() == Func::Power { 1.6 } else { 1.0 };
        painter.line_segment([a, b], Stroke::new(width, color(w.func())));
    }
    let font = FontId::proportional(10.5);
    for (k, n) in h.nodes.iter().enumerate() {
        let c = at(k);
        let r = Rect::from_center_size(c, Vec2::new(70.0, 17.0));
        painter.rect_filled(r, theme::corner(theme::radius::S), tk.surface_alt);
        painter.rect_stroke(
            r,
            theme::corner(theme::radius::S),
            Stroke::new(1.0, tk.border_strong),
            egui::StrokeKind::Inside,
        );
        painter.text(c, Align2::CENTER_CENTER, n.label(), font.clone(), tk.text);
    }
    ui.horizontal(|ui| {
        for f in [Func::Power, Func::Signal, Func::Rf] {
            ui.colored_label(color(f), format!("- {}", func_name(f)));
        }
    });
}
