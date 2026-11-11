// SPDX-License-Identifier: AGPL-3.0-or-later
//! Girdi satırı: etiket (solda) + değer kutusu ve birim (sağda) + yardım işareti.
//!
//! Değer SI biriminde saklanır; ekranda `değer × scale` gösterilir (ör. m → mm için 1000).
//! Bu, mevcut `widgets::fld` ile aynı sözleşmedir; böylece taşınan ekranlar aynı alanları
//! aynı aralık ve hassasiyetle düzenler (hesap sonucu değişmez).
use crate::ui::theme::{self, font};
use egui::{RichText, Ui};

pub struct NumberInput<'a> {
    pub label: &'a str,
    pub value: &'a mut f64,
    pub scale: f64,
    pub unit: &'a str,
    pub lo: f64,
    pub hi: f64,
    pub decimals: usize,
    pub help: &'a str,
    /// Değerin altında gösterilecek küçük ek bilgi (ör. "= 64.8 km/h").
    pub note: Option<String>,
    pub enabled: bool,
}

impl<'a> NumberInput<'a> {
    pub fn new(label: &'a str, value: &'a mut f64) -> Self {
        Self {
            label,
            value,
            scale: 1.0,
            unit: "",
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
            decimals: 2,
            help: "",
            note: None,
            enabled: true,
        }
    }
    pub fn scale(mut self, s: f64) -> Self {
        self.scale = s;
        self
    }
    pub fn unit(mut self, u: &'a str) -> Self {
        self.unit = u;
        self
    }
    pub fn range(mut self, lo: f64, hi: f64) -> Self {
        self.lo = lo;
        self.hi = hi;
        self
    }
    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }
    pub fn help(mut self, h: &'a str) -> Self {
        self.help = h;
        self
    }
    pub fn note(mut self, n: Option<String>) -> Self {
        self.note = n;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}

/// Satır genişliğinin etiket / değer bölüşümü. Değer kutusu en az `min_value_w` alır; etiket
/// kalan alanın çoğunu kullanır ama 72 px'in altına inmez (çok dar panelde değer küçülür).
pub fn split_row(avail: f32, min_value_w: f32, help_w: f32) -> (f32, f32) {
    let avail = if avail.is_finite() {
        avail.max(0.0)
    } else {
        0.0
    };
    let usable = (avail - help_w).max(0.0);
    let label_w = (usable * 0.52).clamp(72.0_f32.min(usable), (usable - min_value_w).max(72.0));
    let label_w = label_w.min(usable);
    let value_w = (usable - label_w).max(0.0);
    (label_w, value_w)
}

/// DragValue hızı: aralığın 1/400'ü (mevcut `fld` ile aynı); sonsuz aralıkta değere göre.
pub fn drag_speed(lo: f64, hi: f64, display_value: f64) -> f64 {
    if lo.is_finite() && hi.is_finite() && hi > lo {
        (hi - lo) / 400.0
    } else if display_value.is_finite() && display_value.abs() > 0.0 {
        display_value.abs() / 200.0
    } else {
        0.01
    }
}

pub fn number_row(ui: &mut Ui, inp: NumberInput) -> bool {
    let tk = theme::tokens(ui);
    let help_w = if inp.help.is_empty() { 0.0 } else { 18.0 };
    // Satırdaki öğeler arası boşluklar (etiket | değer | ?) da genişlikten düşülür; yoksa satır
    // kullanılabilir genişliği aşar ve panel her karede büyür.
    let gaps = if inp.help.is_empty() { 4.0 } else { 8.0 };
    let (label_w, value_w) = split_row(ui.available_width() - gaps, 96.0, help_w);
    let scale = if inp.scale.is_finite() && inp.scale != 0.0 {
        inp.scale
    } else {
        1.0
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(label_w, ui.spacing().interact_size.y),
            egui::Sense::hover(),
        );
        let lbl = ui.put(
            rect,
            egui::Label::new(RichText::new(inp.label).color(tk.text))
                .truncate()
                .halign(egui::Align::LEFT),
        );
        if !inp.help.is_empty() {
            lbl.on_hover_text(inp.help);
        }
        let mut x = *inp.value * scale;
        let speed = drag_speed(inp.lo, inp.hi, x);
        let mut dv = egui::DragValue::new(&mut x)
            .speed(speed)
            .fixed_decimals(inp.decimals);
        if inp.lo.is_finite() || inp.hi.is_finite() {
            dv = dv.range(inp.lo..=inp.hi);
        }
        if !inp.unit.is_empty() {
            dv = dv.suffix(format!(" {}", inp.unit));
        }
        let r = ui.add_enabled_ui(inp.enabled, |ui| {
            ui.add_sized(
                egui::vec2(value_w.max(40.0), ui.spacing().interact_size.y),
                dv,
            )
        });
        let mut resp = r.inner;
        if !inp.help.is_empty() {
            resp = resp.on_hover_text(inp.help);
        }
        if resp.changed() && x.is_finite() {
            *inp.value = x / scale;
            changed = true;
        }
        if !inp.help.is_empty() {
            help_mark(ui, inp.help);
        }
    });
    if let Some(n) = inp.note {
        ui.horizontal(|ui| {
            ui.add_space(label_w + 4.0);
            ui.label(RichText::new(n).size(font::CAPTION).color(tk.text_weak));
        });
    }
    changed
}

pub fn u32_row(ui: &mut Ui, label: &str, v: &mut u32, lo: u32, hi: u32, help: &str) -> bool {
    let tk = theme::tokens(ui);
    let help_w = if help.is_empty() { 0.0 } else { 18.0 };
    let gaps = if help.is_empty() { 4.0 } else { 8.0 };
    let (label_w, value_w) = split_row(ui.available_width() - gaps, 96.0, help_w);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(label_w, ui.spacing().interact_size.y),
            egui::Sense::hover(),
        );
        ui.put(
            rect,
            egui::Label::new(RichText::new(label).color(tk.text))
                .truncate()
                .halign(egui::Align::LEFT),
        );
        let r = ui.add_sized(
            egui::vec2(value_w.max(40.0), ui.spacing().interact_size.y),
            egui::DragValue::new(v).range(lo..=hi).speed(0.1),
        );
        changed = r.changed();
        if !help.is_empty() {
            help_mark(ui, help);
        }
    });
    changed
}

/// Küçük daire içinde "?" işareti; üzerine gelince yardım metni.
pub fn help_mark(ui: &mut Ui, help: &str) {
    let tk = theme::tokens(ui);
    let (r, p) = ui.allocate_painter(egui::vec2(14.0, 14.0), egui::Sense::hover());
    let c = r.rect.center();
    let col = if r.hovered() { tk.accent } else { tk.text_weak };
    p.circle_stroke(c, 6.0, egui::Stroke::new(1.0, col));
    p.text(
        c,
        egui::Align2::CENTER_CENTER,
        "?",
        egui::FontId::proportional(9.5),
        col,
    );
    r.on_hover_text(help);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_row_respects_minimums() {
        let (l, v) = split_row(300.0, 96.0, 18.0);
        assert!((l + v - 282.0).abs() < 1e-3);
        assert!(v >= 96.0);
        assert!(l >= 72.0);
        // Dar panel: etiket 72'de kalır, değer kalan alanı alır.
        let (l, v) = split_row(150.0, 96.0, 0.0);
        assert!((l - 72.0).abs() < 1e-3);
        assert!((v - 78.0).abs() < 1e-3);
        // Aşırı dar ve bozuk girdi: negatif genişlik yok.
        let (l, v) = split_row(30.0, 96.0, 18.0);
        assert!(l >= 0.0 && v >= 0.0 && l + v <= 12.0 + 1e-3);
        let (l, v) = split_row(f32::NAN, 96.0, 0.0);
        assert_eq!((l, v), (0.0, 0.0));
    }

    #[test]
    fn drag_speed_matches_legacy_fld() {
        assert!((drag_speed(0.0, 400.0, 10.0) - 1.0).abs() < 1e-12);
        assert!((drag_speed(f64::NEG_INFINITY, f64::INFINITY, 50.0) - 0.25).abs() < 1e-12);
        assert!((drag_speed(f64::NEG_INFINITY, f64::INFINITY, f64::NAN) - 0.01).abs() < 1e-12);
    }
}
