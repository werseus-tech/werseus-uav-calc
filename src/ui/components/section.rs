// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bölüm başlığı: küçük, kalın, soluk başlık + ince ayırıcı çizgi.
use crate::ui::theme::{self, font, space};
use egui::{RichText, Ui};

pub fn section_header(ui: &mut Ui, title: &str) {
    section_header_with(ui, title, |_| {});
}

/// Sağ tarafına küçük denetimler (ör. "Tümü", "Sığdır") eklenebilen bölüm başlığı.
pub fn section_header_with(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    let tk = theme::tokens(ui);
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .size(font::SUBHEAD)
                .strong()
                .color(tk.text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
    let r = ui.available_rect_before_wrap();
    let y = ui.cursor().top() + 1.0;
    ui.painter()
        .hline(r.left()..=r.right(), y, egui::Stroke::new(1.0, tk.border));
    ui.add_space(space::XS + 2.0);
}
