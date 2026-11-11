// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yan pencerelerin (Öğretmen, Yapay zekâ, Rüzgâr tüneli, Formüller, Yardım) ortak çerçeveleri.
//! Renk, köşe ve boşluk yalnız `theme` tokenlarından gelir; iki temada okunur.

use super::theme::{self, Tokens, radius, space};
use egui::{Color32, Stroke};

/// Renkli kart çerçevesi: zemin verilir, kenarlık ve köşe temadan.
pub fn tinted_frame(tk: &Tokens, fill: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, tk.border))
        .corner_radius(theme::corner(radius::M))
        .inner_margin(theme::margin(space::S))
}

/// Vurgulu (seçili/öneri) kart çerçevesi: vurgu kenarlığı.
pub fn accent_frame(tk: &Tokens) -> egui::Frame {
    egui::Frame::new()
        .fill(tk.accent_soft)
        .stroke(Stroke::new(1.0, tk.accent))
        .corner_radius(theme::corner(radius::M))
        .inner_margin(theme::margin(space::S + 2.0))
}

/// Düz yüzey kartı.
pub fn surface_frame(tk: &Tokens) -> egui::Frame {
    tinted_frame(tk, tk.surface_alt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_use_theme_tokens_in_both_themes() {
        for tk in [&theme::DARK, &theme::LIGHT] {
            let f = tinted_frame(tk, tk.info_soft);
            assert_eq!(f.fill, tk.info_soft);
            assert_eq!(f.stroke.color, tk.border);
            let a = accent_frame(tk);
            assert_eq!(a.fill, tk.accent_soft);
            assert_eq!(a.stroke.color, tk.accent);
            assert_eq!(surface_frame(tk).fill, tk.surface_alt);
        }
    }
}
