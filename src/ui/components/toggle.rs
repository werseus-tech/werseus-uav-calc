// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bölmeli seçici (segmented control), aç/kapa çipi ve araç çubuğu düğmesi.
use crate::ui::theme::{self, font, radius};
use egui::{Response, RichText, Ui};

/// Yan yana seçenekler; seçili olan vurgulu. Değişiklik olduysa `true`.
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    options: &[(T, &str, &str)],
) -> bool {
    let tk = theme::tokens(ui);
    let mut changed = false;
    egui::Frame::new()
        .fill(tk.surface_alt)
        .stroke(egui::Stroke::new(1.0, tk.border))
        .corner_radius(theme::corner(radius::M))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (v, label, tip) in options {
                    let sel = *current == *v;
                    let text = RichText::new(*label).size(font::BODY - 0.5).color(if sel {
                        tk.on_accent
                    } else {
                        tk.text
                    });
                    let mut b = egui::Button::new(text)
                        .corner_radius(theme::corner(radius::S))
                        .frame_when_inactive(sel);
                    if sel {
                        b = b.fill(tk.accent_fill);
                    }
                    let mut r = ui.add(b);
                    if !tip.is_empty() {
                        r = r.on_hover_text(*tip);
                    }
                    if r.clicked() && !sel {
                        *current = *v;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// Aç/kapa çipi (ör. "Ağırlık merkezi", "Pervane").
pub fn toggle_chip(ui: &mut Ui, on: &mut bool, label: &str, tip: &str) -> Response {
    let tk = theme::tokens(ui);
    let text = RichText::new(label).size(font::BODY - 0.5).color(if *on {
        tk.accent
    } else {
        tk.text_weak
    });
    let mut b = egui::Button::new(text).corner_radius(theme::corner(radius::L));
    b = if *on {
        b.fill(tk.accent_soft)
            .stroke(egui::Stroke::new(1.0, tk.accent))
    } else {
        b.fill(tk.surface_alt)
            .stroke(egui::Stroke::new(1.0, tk.border))
    };
    let mut r = ui.add(b);
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    if !tip.is_empty() {
        r = r.on_hover_text(tip);
    }
    r
}

/// Sade araç çubuğu düğmesi.
pub fn tool_button(ui: &mut Ui, label: &str, tip: &str) -> Response {
    let tk = theme::tokens(ui);
    let r = ui.add(
        egui::Button::new(RichText::new(label).size(font::BODY - 0.5).color(tk.text))
            .corner_radius(theme::corner(radius::S)),
    );
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// Vurgulu birincil eylem düğmesi (ör. "İleri").
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let tk = theme::tokens(ui);
    let b = egui::Button::new(RichText::new(label).strong().color(if enabled {
        tk.on_accent
    } else {
        tk.text_weak
    }))
    .fill(if enabled {
        tk.accent_fill
    } else {
        tk.surface_alt
    })
    .corner_radius(theme::corner(radius::S))
    .min_size(egui::vec2(84.0, 26.0));
    ui.add_enabled(enabled, b)
}
