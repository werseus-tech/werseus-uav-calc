// SPDX-License-Identifier: AGPL-3.0-or-later
//! Durum rozeti, uyarı şeridi ve tıklanabilir uyarı satırı.
use super::Status;
use crate::calc::Warn;
use crate::ui::theme::{self, font, radius, space};
use egui::{Response, RichText, Sense, Ui};

/// Küçük hap biçimli rozet (yumuşak zemin + durum renginde metin).
pub fn badge(ui: &mut Ui, status: Status, text: &str) -> Response {
    let tk = theme::tokens(ui);
    egui::Frame::new()
        .fill(theme::status_soft(tk, status))
        .corner_radius(theme::corner(radius::L))
        .inner_margin(egui::Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(font::CAPTION)
                    .strong()
                    .color(theme::status_color(tk, status)),
            );
        })
        .response
}

/// Tam genişlikte, sarılan metinli bilgi/uyarı şeridi.
pub fn banner(ui: &mut Ui, status: Status, text: &str) -> Response {
    let tk = theme::tokens(ui);
    let color = theme::status_color(tk, status);
    let resp = egui::Frame::new()
        .fill(theme::status_soft(tk, status))
        .corner_radius(theme::corner(radius::M))
        .inner_margin(egui::Margin {
            left: 12,
            right: 8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).color(tk.text)).wrap());
        })
        .response;
    let r = resp.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_max(r.left_top(), egui::pos2(r.left() + 3.0, r.bottom())),
        theme::corner(2),
        color,
    );
    resp
}

/// Uyarı listesindeki tek satır: durum noktası + alan adı + metin. Tıklanabilir
/// (çağıran taraf ilgili adıma gider). Üzerine gelince zemin vurgulanır.
pub fn warning_row(ui: &mut Ui, w: &Warn) -> Response {
    let tk = theme::tokens(ui);
    let status = Status::from_sev(w.sev);
    let color = theme::status_color(tk, status);
    let width = ui.available_width();
    let text = w.text();
    let galley = ui.painter().layout(
        text,
        egui::FontId::proportional(font::BODY - 0.5),
        tk.text,
        (width - 26.0).max(40.0),
    );
    let area = w.area_name();
    let mut area_job = egui::text::LayoutJob::single_section(
        area.to_string(),
        egui::TextFormat::simple(egui::FontId::proportional(font::CAPTION), tk.text_weak),
    );
    area_job.wrap.max_width = (width - 26.0).max(40.0);
    area_job.wrap.max_rows = 1;
    area_job.wrap.break_anywhere = true;
    let area_g = ui.painter().layout_job(area_job);
    let area_h = area_g.size().y;
    let h = galley.size().y + area_h + space::S + 2.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, theme::corner(radius::S), tk.surface_alt);
        }
        let dot = egui::pos2(rect.left() + 9.0, rect.top() + 3.0 + area_h * 0.5);
        p.circle_filled(dot, 4.0, color);
        p.galley(
            egui::pos2(rect.left() + 20.0, rect.top() + 3.0),
            area_g,
            tk.text_weak,
        );
        p.galley(
            egui::pos2(rect.left() + 20.0, rect.top() + 3.0 + area_h + 2.0),
            galley,
            tk.text,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
