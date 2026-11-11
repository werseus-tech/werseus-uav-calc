// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ortak arayüz yardımcıları: çıktı satırları, kartlar, biçimlendirme.
use crate::calc::Sev;
use egui::{Color32, RichText, Ui};

pub fn sev_color(tk: &crate::ui::theme::Tokens, s: Sev) -> Color32 {
    use crate::ui::components::Status;
    crate::ui::theme::status_color(
        tk,
        match s {
            Sev::Err => Status::Err,
            Sev::Warn => Status::Warn,
            Sev::Info => Status::Info,
        },
    )
}

pub fn sev_tag(s: Sev) -> &'static str {
    match s {
        Sev::Err => "[HATA]",
        Sev::Warn => "[UYARI]",
        Sev::Info => "[NOT]",
    }
}

/// Sayıyı biçimlendirir; sonlu değilse "—".
pub fn fmt(x: f64, dec: usize) -> String {
    if x.is_finite() {
        format!("{:.*}", dec, x)
    } else {
        "—".to_string()
    }
}

pub fn fmt_sci(x: f64, dec: usize) -> String {
    if x.is_finite() {
        format!("{:.*e}", dec, x)
    } else {
        "—".to_string()
    }
}

/// Çıktı satırı (etiket, değer, birim/not, neden/açıklama).
/// Değerin yanına '[i]' simgesi ekler; üzerine gelindiğinde zengin açıklama gösterilir.
pub fn out(ui: &mut Ui, label: &str, value: String, unit: &str, why: &str) {
    // Değer rengi temadan gelir (açık temada beyaz metin görünmez kalırdı).
    let color = super::theme::tokens(ui).text;
    out_c(ui, label, value, unit, why, color);
}

pub fn out_c(ui: &mut Ui, label: &str, value: String, unit: &str, why: &str, color: Color32) {
    ui.label(label).on_hover_text(why);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&value).strong().color(color))
            .on_hover_text(why);
        if !why.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new("[i]")
                        .color(super::theme::tokens(ui).accent)
                        .size(11.5),
                )
                .sense(egui::Sense::click()),
            )
            .on_hover_text(why);
        }
    });
    ui.label(RichText::new(unit).weak()).on_hover_text(why);
    ui.end_row();
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().size(15.0));
}

/// Kart (özet sekmesi).
pub fn card(ui: &mut Ui, title: &str, value: &str, sub: &str, color: Option<Color32>) {
    crate::ui::panel_kit::surface_frame(crate::ui::theme::tokens(ui)).show(ui, |ui| {
        ui.set_min_width(150.0);
        ui.set_max_width(190.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).weak().small());
            let mut t = RichText::new(value).strong().size(20.0);
            if let Some(c) = color {
                t = t.color(c);
            }
            ui.label(t);
            ui.label(RichText::new(sub).weak().small());
        });
    });
}

pub fn ms_to_kmh(v: f64) -> String {
    format!("= {:.1} km/h", v * 3.6)
}
