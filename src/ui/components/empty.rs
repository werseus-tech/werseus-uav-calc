// SPDX-License-Identifier: AGPL-3.0-or-later
//! Boş durum: gösterilecek veri yokken ortalanmış sakin bir açıklama.
use crate::ui::theme::{self, font, space};
use egui::{RichText, Ui};

pub fn empty_state(ui: &mut Ui, title: &str, body: &str) {
    let tk = theme::tokens(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(space::L);
        // Basit çizilmiş simge: içi boş daire + yatay çizgi (yazı tipi glifine bağımlı değil).
        let (r, p) = ui.allocate_painter(egui::vec2(28.0, 28.0), egui::Sense::hover());
        let c = r.rect.center();
        p.circle_stroke(c, 11.0, egui::Stroke::new(1.5, tk.border_strong));
        p.line_segment(
            [c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)],
            egui::Stroke::new(1.5, tk.border_strong),
        );
        ui.add_space(space::XS);
        ui.label(RichText::new(title).size(font::SUBHEAD).color(tk.text));
        ui.add(egui::Label::new(RichText::new(body).color(tk.text_weak)).wrap());
        ui.add_space(space::L);
    });
}
