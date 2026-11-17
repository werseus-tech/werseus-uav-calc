// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compact mass-properties panel (total mass, CG, moments of inertia, CG vs thrust centre, top
//! view sketch and per-item table). Reads the live project and model every frame, so it follows
//! any edit at once. All numbers come from [`crate::massprops`]; this file only draws them.
use super::components::{Status, section::section_header};
use super::step_ui::{self, CardExt, TableRow, card, cards};
use super::{theme, widgets::fmt};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::massprops::{self, AxisFrame, MassProperties, OffsetKind};
use crate::project::Project;
use egui::{Align2, FontId, Sense, Stroke, Ui, pos2, vec2};

fn inertia_text(v: f64) -> String {
    if v.abs() >= 0.01 {
        fmt(v, 4)
    } else {
        fmt(v, 5)
    }
}

/// Status of the CG offset (the colour warning): red above the limit, amber above half of it.
pub fn offset_status(mp: &MassProperties) -> Status {
    let th = &mp.thrust;
    if !th.offset_pct.is_finite() {
        return Status::Neutral;
    }
    if th.exceeds {
        return Status::Err;
    }
    match (th.kind, th.limit_pct) {
        (OffsetKind::MotorCentroid, Some(limit)) if th.offset_pct > 0.5 * limit => Status::Warn,
        _ => Status::Ok,
    }
}

/// Panel entry point: heading, estimate note, cards, sketch and item table.
pub fn panel(ui: &mut Ui, p: &Project, m: &Model) {
    let mp = massprops::compute(p, m);
    section_header(ui, t("massprops.title"));
    step_ui::note(ui, t("massprops.estimate_note"));
    if !mp.valid {
        step_ui::status_text(ui, Status::Warn, t("massprops.invalid"));
        return;
    }
    step_ui::note(
        ui,
        match mp.frame {
            AxisFrame::NoseAft => t("massprops.axes.nose_aft"),
            AxisFrame::CentreForward => t("massprops.axes.centre_forward"),
        },
    );
    let [ixx, iyy, izz] = mp.principal_diag();
    let th = &mp.thrust;
    let status = offset_status(&mp);
    let mut list = vec![
        card(
            t("massprops.card.mass"),
            fmt(mp.total_mass_kg * 1000.0, 0),
            "g",
            t("massprops.card.mass.tip"),
        ),
        card(
            t("massprops.card.cg"),
            format!(
                "{} / {} / {}",
                fmt(mp.cg_m[0] * 1000.0, 1),
                fmt(mp.cg_m[1] * 1000.0, 1),
                fmt(mp.cg_m[2] * 1000.0, 1)
            ),
            "mm",
            t("massprops.card.cg.tip"),
        ),
        card(
            t("massprops.card.ixx"),
            inertia_text(ixx),
            "kg·m²",
            t("massprops.card.ixx.tip"),
        ),
        card(
            t("massprops.card.iyy"),
            inertia_text(iyy),
            "kg·m²",
            t("massprops.card.iyy.tip"),
        ),
        card(
            t("massprops.card.izz"),
            inertia_text(izz),
            "kg·m²",
            t("massprops.card.izz.tip"),
        ),
    ];
    match th.kind {
        OffsetKind::MotorCentroid => list.push(
            card(
                t("massprops.card.offset_mr"),
                fmt(th.offset_m * 1000.0, 1),
                "mm",
                t("massprops.card.offset_mr.tip"),
            )
            .d(tf(
                "massprops.card.offset_mr.detail",
                &[
                    fmt(th.offset_pct, 1),
                    fmt(th.limit_pct.unwrap_or(f64::NAN), 1),
                ],
            ))
            .s(status),
        ),
        OffsetKind::ThrustAxis => {
            list.push(
                card(
                    t("massprops.card.cg_mac"),
                    fmt(th.cg_pct_mac.unwrap_or(f64::NAN), 1),
                    "% MAC",
                    t("massprops.card.cg_mac.tip"),
                )
                .d(tf(
                    "massprops.card.cg_mac.detail",
                    &[fmt(th.static_margin.unwrap_or(f64::NAN) * 100.0, 1)],
                ))
                .s(status),
            );
            list.push(
                card(
                    t("massprops.card.offset_fw"),
                    fmt(th.offset_m * 1000.0, 1),
                    "mm",
                    t("massprops.card.offset_fw.tip"),
                )
                .d(tf(
                    "massprops.card.offset_fw.detail",
                    &[fmt(th.offset_pct, 1)],
                )),
            );
        }
    }
    cards(ui, &list);
    if th.exceeds {
        step_ui::status_text(
            ui,
            Status::Err,
            match th.kind {
                OffsetKind::MotorCentroid => t("massprops.warn.mr_offset"),
                OffsetKind::ThrustAxis => t("massprops.warn.fw_margin"),
            },
        );
    }
    let ref_mass = mp.reference_mass_kg;
    if ref_mass.is_finite() && ref_mass > 0.0 {
        let diff = (mp.total_mass_kg - ref_mass) / ref_mass;
        if diff.abs() > 0.01 {
            step_ui::note(
                ui,
                &tf(
                    "massprops.mass_diff",
                    &[fmt(diff * 100.0, 1), fmt(ref_mass * 1000.0, 0)],
                ),
            );
        }
    }
    sketch(ui, &mp, m);
    items_table(ui, &mp);
}

/// Top view (nose / forward up). Disc area follows the item mass; the filled ring marks the CG,
/// the cross the centre of thrust, small hollow circles the multirotor motors.
fn sketch(ui: &mut Ui, mp: &MassProperties, m: &Model) {
    let tk = theme::tokens(ui);
    let width = ui.available_width().clamp(120.0, 520.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 190.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, tk.surface_alt);
    // Screen coordinates: right = body y, up = forward.
    let fwd = |x: f64| match mp.frame {
        AxisFrame::NoseAft => -x,
        AxisFrame::CentreForward => x,
    };
    let mut pts: Vec<(f64, f64)> = mp
        .items
        .iter()
        .map(|i| (i.pos_m[1], fwd(i.pos_m[0])))
        .collect();
    pts.push((mp.cg_m[1], fwd(mp.cg_m[0])));
    pts.push((mp.thrust.centre_m[1], fwd(mp.thrust.centre_m[0])));
    if mp.frame == AxisFrame::CentreForward {
        pts.extend(m.mr.motor_coords.iter().map(|&(x, y)| (y, x)));
    }
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for &(a, b) in pts.iter().filter(|(a, b)| a.is_finite() && b.is_finite()) {
        lo = (lo.0.min(a), lo.1.min(b));
        hi = (hi.0.max(a), hi.1.max(b));
    }
    if lo.0 > hi.0 {
        return;
    }
    let pad = 18.0_f32;
    let span = ((hi.0 - lo.0).max(1e-3), (hi.1 - lo.1).max(1e-3));
    let scale = ((rect.width() - 2.0 * pad) as f64 / span.0)
        .min((rect.height() - 2.0 * pad) as f64 / span.1) as f32;
    let mid = ((lo.0 + hi.0) * 0.5, (lo.1 + hi.1) * 0.5);
    let to = |a: f64, b: f64| {
        pos2(
            rect.center().x + ((a - mid.0) as f32) * scale,
            rect.center().y - ((b - mid.1) as f32) * scale,
        )
    };
    if mp.frame == AxisFrame::CentreForward {
        for &(x, y) in &m.mr.motor_coords {
            painter.circle_stroke(to(y, x), 5.0, Stroke::new(1.5, tk.text_weak));
        }
    }
    let m_max = mp
        .items
        .iter()
        .map(|i| i.mass_kg)
        .fold(0.0, f64::max)
        .max(1e-9);
    for i in &mp.items {
        let r = 3.0 + 7.0 * (i.mass_kg / m_max).sqrt() as f32;
        painter.circle_filled(
            to(i.pos_m[1], fwd(i.pos_m[0])),
            r,
            tk.accent.gamma_multiply(0.45),
        );
    }
    let c = to(mp.thrust.centre_m[1], fwd(mp.thrust.centre_m[0]));
    let ring = theme::status_color(tk, offset_status(mp));
    painter.line_segment(
        [c - vec2(7.0, 0.0), c + vec2(7.0, 0.0)],
        Stroke::new(1.5, tk.text),
    );
    painter.line_segment(
        [c - vec2(0.0, 7.0), c + vec2(0.0, 7.0)],
        Stroke::new(1.5, tk.text),
    );
    let g = to(mp.cg_m[1], fwd(mp.cg_m[0]));
    painter.circle_stroke(g, 7.0, Stroke::new(2.5, ring));
    painter.circle_filled(g, 2.5, ring);
    painter.text(
        rect.left_top() + vec2(6.0, 4.0),
        Align2::LEFT_TOP,
        t("massprops.sketch.legend"),
        FontId::proportional(10.0),
        tk.text_weak,
    );
    painter.text(
        rect.center_top() + vec2(0.0, 16.0),
        Align2::CENTER_TOP,
        t("massprops.sketch.front"),
        FontId::proportional(10.0),
        tk.text_weak,
    );
}

fn items_table(ui: &mut Ui, mp: &MassProperties) {
    let rows: Vec<TableRow> = mp
        .items
        .iter()
        .map(|i| TableRow {
            name: if i.name.is_empty() {
                t(i.name_key).to_string()
            } else {
                i.name.clone()
            },
            cells: vec![
                fmt(i.mass_kg * 1000.0, 0),
                fmt(i.pos_m[0] * 1000.0, 0),
                fmt(i.pos_m[1] * 1000.0, 0),
                fmt(i.pos_m[2] * 1000.0, 0),
                t(i.shape_key).to_string(),
            ],
            status: Status::Neutral,
        })
        .collect();
    step_ui::row_table(
        ui,
        "massprops_items",
        t("massprops.table.title"),
        &[
            t("massprops.col.mass"),
            t("massprops.col.x"),
            t("massprops.col.y"),
            t("massprops.col.z"),
            t("massprops.col.shape"),
        ],
        &rows,
        Some(t("massprops.table.footer")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::DroneType;

    #[test]
    fn panel_renders_headless_for_every_type_and_theme() {
        for choice in [theme::ThemeChoice::Dark, theme::ThemeChoice::Light] {
            for dt in DroneType::ALL {
                let mut p = Project::default();
                p.set_drone_type(dt);
                let m = evaluate(&p, &Library::embedded());
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let mut input = egui::RawInput::default();
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(420.0, 900.0),
                ));
                let mut out = ctx.run_ui(input, |ui| {
                    panel(ui, &p, &m);
                });
                out.textures_delta.clear();
            }
        }
    }

    #[test]
    fn colour_warning_follows_the_offset() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let ok = massprops::compute(&p, &evaluate(&p, &Library::embedded()));
        assert_eq!(offset_status(&ok), Status::Ok);
        p.mr_components[0].x += 0.2;
        let bad = massprops::compute(&p, &evaluate(&p, &Library::embedded()));
        assert!(bad.thrust.exceeds);
        assert_eq!(offset_status(&bad), Status::Err);
    }
}
