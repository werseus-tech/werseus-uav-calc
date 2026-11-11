// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tasarım dili: renk, boşluk, köşe ve yazı boyutu tokenları; açık ve koyu tema.
//!
//! Tüm yeni arayüz kodu renkleri doğrudan `Color32::from_rgb` ile değil, buradaki
//! [`Tokens`] üzerinden alır: `let tk = theme::tokens(ui);`. `egui::Style` kurulumu tek yerden
//! ([`install`]) yapılır; iki tema da önceden kurulur, geçiş yalnızca tema tercihini değiştirir.
//! Kontrast oranları birim testlerle denetlenir (WCAG AA: metin ≥ 4.5, büyük metin/simge ≥ 3).
use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Theme};
use serde::{Deserialize, Serialize};

/// Kullanıcının tema seçimi (ayarlarda saklanır).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

impl ThemeChoice {
    pub fn theme(self) -> Theme {
        match self {
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Light => Theme::Light,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            ThemeChoice::Dark => ThemeChoice::Light,
            ThemeChoice::Light => ThemeChoice::Dark,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Boşluk, köşe ve yazı boyutu ölçekleri (temadan bağımsız)
// ---------------------------------------------------------------------------------------------

/// 4 px tabanlı boşluk ölçeği.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Köşe yarıçapları (egui 0.37: `CornerRadius` alanları `u8`).
pub mod radius {
    pub const S: u8 = 4;
    pub const M: u8 = 6;
    pub const L: u8 = 10;
}

/// Yazı boyutları (pt).
pub mod font {
    pub const CAPTION: f32 = 11.5;
    pub const BODY: f32 = 13.0;
    pub const SUBHEAD: f32 = 14.5;
    pub const HEADING: f32 = 17.0;
    pub const METRIC: f32 = 21.0;
}

pub fn corner(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}

/// `Margin::same` egui 0.37'de `i8` alır.
pub fn margin(px: f32) -> Margin {
    Margin::same(px.round().clamp(0.0, 127.0) as i8)
}

// ---------------------------------------------------------------------------------------------
// Renk tokenları
// ---------------------------------------------------------------------------------------------

/// Bir temanın anlamsal renkleri. Alan adları kullanım yerini anlatır, renk adını değil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// Pencere/panel zemini.
    pub bg: Color32,
    /// Kart, grup ve açılır pencere yüzeyi.
    pub surface: Color32,
    /// Girdi kutusu, üzerine gelinmiş satır, ikincil yüzey.
    pub surface_alt: Color32,
    /// En koyu/en açık zemin (metin kutusu içi).
    pub extreme: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    /// Tek vurgu rengi (seçili adım, bağlantı, odak).
    pub accent: Color32,
    /// Vurgu rengi zemin olarak kullanıldığında (üzerinde `on_accent` metni).
    pub accent_fill: Color32,
    pub on_accent: Color32,
    /// Vurgunun çok açık/çok koyu tonu (seçili satır zemini).
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
    /// Durum renklerinin zemin tonları (rozet/şerit arka planı).
    pub success_soft: Color32,
    pub warning_soft: Color32,
    pub danger_soft: Color32,
    pub info_soft: Color32,
    /// 3D/2D görünüm zemini ve ızgarası.
    pub viewport_bg: Color32,
    pub viewport_grid: Color32,
    pub viewport_grid_major: Color32,
}

pub const DARK: Tokens = Tokens {
    dark: true,
    bg: Color32::from_rgb(21, 25, 31),
    surface: Color32::from_rgb(28, 33, 41),
    surface_alt: Color32::from_rgb(36, 42, 52),
    extreme: Color32::from_rgb(14, 17, 21),
    border: Color32::from_rgb(46, 54, 66),
    border_strong: Color32::from_rgb(70, 82, 98),
    text: Color32::from_rgb(230, 234, 240),
    text_weak: Color32::from_rgb(154, 164, 178),
    accent: Color32::from_rgb(92, 162, 255),
    accent_fill: Color32::from_rgb(38, 102, 196),
    on_accent: Color32::from_rgb(255, 255, 255),
    accent_soft: Color32::from_rgb(30, 50, 80),
    success: Color32::from_rgb(76, 190, 132),
    warning: Color32::from_rgb(230, 172, 70),
    danger: Color32::from_rgb(244, 112, 108),
    info: Color32::from_rgb(132, 170, 222),
    success_soft: Color32::from_rgb(24, 48, 38),
    warning_soft: Color32::from_rgb(56, 44, 22),
    danger_soft: Color32::from_rgb(60, 28, 30),
    info_soft: Color32::from_rgb(28, 40, 58),
    viewport_bg: Color32::from_rgb(30, 35, 43),
    viewport_grid: Color32::from_rgb(44, 51, 62),
    viewport_grid_major: Color32::from_rgb(60, 70, 85),
};

pub const LIGHT: Tokens = Tokens {
    dark: false,
    bg: Color32::from_rgb(244, 246, 249),
    surface: Color32::from_rgb(255, 255, 255),
    surface_alt: Color32::from_rgb(236, 240, 245),
    extreme: Color32::from_rgb(255, 255, 255),
    border: Color32::from_rgb(212, 218, 227),
    border_strong: Color32::from_rgb(160, 170, 184),
    text: Color32::from_rgb(26, 34, 48),
    text_weak: Color32::from_rgb(88, 99, 116),
    accent: Color32::from_rgb(20, 94, 204),
    accent_fill: Color32::from_rgb(20, 94, 204),
    on_accent: Color32::from_rgb(255, 255, 255),
    accent_soft: Color32::from_rgb(222, 234, 252),
    success: Color32::from_rgb(25, 120, 74),
    warning: Color32::from_rgb(146, 92, 0),
    danger: Color32::from_rgb(190, 44, 40),
    info: Color32::from_rgb(52, 98, 160),
    success_soft: Color32::from_rgb(224, 243, 233),
    warning_soft: Color32::from_rgb(252, 241, 220),
    danger_soft: Color32::from_rgb(252, 228, 227),
    info_soft: Color32::from_rgb(226, 236, 249),
    viewport_bg: Color32::from_rgb(238, 241, 245),
    viewport_grid: Color32::from_rgb(218, 223, 230),
    viewport_grid_major: Color32::from_rgb(196, 203, 213),
};

pub fn tokens_for(theme: Theme) -> &'static Tokens {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// Bu karede etkin temanın tokenları.
pub fn tokens(ui: &egui::Ui) -> &'static Tokens {
    tokens_for(ui.ctx().theme())
}

pub fn tokens_ctx(ctx: &egui::Context) -> &'static Tokens {
    tokens_for(ctx.theme())
}

// ---------------------------------------------------------------------------------------------
// egui Style kurulumu
// ---------------------------------------------------------------------------------------------

/// Verilen tokenlardan tam bir `egui::Style` üretir (saf; testlerde de kullanılır).
pub fn build_style(base: &egui::Style, tk: &Tokens) -> egui::Style {
    let mut s = base.clone();
    s.spacing.item_spacing = egui::vec2(space::S, 5.0);
    s.spacing.button_padding = egui::vec2(8.0, 3.0);
    s.spacing.window_margin = margin(space::M);
    s.spacing.menu_margin = margin(6.0);
    s.spacing.interact_size.y = 22.0;

    s.text_styles
        .insert(TextStyle::Small, FontId::proportional(10.5));
    s.text_styles
        .insert(TextStyle::Body, FontId::proportional(font::BODY));
    s.text_styles
        .insert(TextStyle::Button, FontId::proportional(font::BODY));
    s.text_styles
        .insert(TextStyle::Heading, FontId::proportional(font::HEADING));
    s.text_styles
        .insert(TextStyle::Monospace, FontId::monospace(12.5));

    let mut v = if tk.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.dark_mode = tk.dark;
    v.panel_fill = tk.bg;
    v.window_fill = tk.surface;
    v.window_stroke = Stroke::new(1.0, tk.border);
    v.window_corner_radius = corner(radius::L);
    v.menu_corner_radius = corner(radius::M);
    v.faint_bg_color = tk.surface_alt;
    v.extreme_bg_color = tk.extreme;
    v.code_bg_color = tk.surface_alt;
    v.hyperlink_color = tk.accent;
    v.warn_fg_color = tk.warning;
    v.error_fg_color = tk.danger;
    v.selection.bg_fill = tk.accent_soft;
    v.selection.stroke = Stroke::new(1.0, tk.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = tk.surface;
    w.noninteractive.weak_bg_fill = tk.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, tk.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, tk.text);
    w.noninteractive.corner_radius = corner(radius::M);

    w.inactive.bg_fill = tk.surface_alt;
    w.inactive.weak_bg_fill = tk.surface_alt;
    w.inactive.bg_stroke = Stroke::new(1.0, tk.border);
    w.inactive.fg_stroke = Stroke::new(1.0, tk.text);
    w.inactive.corner_radius = corner(radius::S);

    w.hovered.bg_fill = tk.surface_alt;
    w.hovered.weak_bg_fill = tk.surface_alt;
    w.hovered.bg_stroke = Stroke::new(1.0, tk.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, tk.text);
    w.hovered.corner_radius = corner(radius::S);

    w.active.bg_fill = tk.accent_soft;
    w.active.weak_bg_fill = tk.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0, tk.accent);
    w.active.fg_stroke = Stroke::new(2.0, tk.text);
    w.active.corner_radius = corner(radius::S);

    w.open.bg_fill = tk.surface_alt;
    w.open.weak_bg_fill = tk.surface_alt;
    w.open.bg_stroke = Stroke::new(1.0, tk.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, tk.text);
    w.open.corner_radius = corner(radius::S);

    s.visuals = v;
    s
}

/// İki temanın stilini kurar ve tercihi uygular. Uygulama açılışında ve tema değişince çağrılır.
pub fn install(ctx: &egui::Context, choice: ThemeChoice) {
    let base = ctx.style_of(Theme::Dark);
    ctx.set_style_of(Theme::Dark, build_style(&base, &DARK));
    let base = ctx.style_of(Theme::Light);
    ctx.set_style_of(Theme::Light, build_style(&base, &LIGHT));
    ctx.set_theme(choice.theme());
}

/// Durum (önem) için ön plan rengi.
pub fn status_color(tk: &Tokens, s: crate::ui::components::Status) -> Color32 {
    use crate::ui::components::Status;
    match s {
        Status::Neutral => tk.text,
        Status::Ok => tk.success,
        Status::Warn => tk.warning,
        Status::Err => tk.danger,
        Status::Info => tk.info,
    }
}

/// Durum için zemin rengi (rozet, şerit).
pub fn status_soft(tk: &Tokens, s: crate::ui::components::Status) -> Color32 {
    use crate::ui::components::Status;
    match s {
        Status::Neutral => tk.surface_alt,
        Status::Ok => tk.success_soft,
        Status::Warn => tk.warning_soft,
        Status::Err => tk.danger_soft,
        Status::Info => tk.info_soft,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::format::contrast_ratio;

    fn rgb(c: Color32) -> [u8; 3] {
        [c.r(), c.g(), c.b()]
    }

    fn check(tk: &Tokens) {
        let name = if tk.dark { "koyu" } else { "açık" };
        // Gövde metni tüm zeminlerde AA (4.5).
        for (zemin_adi, zemin) in [
            ("bg", tk.bg),
            ("surface", tk.surface),
            ("surface_alt", tk.surface_alt),
        ] {
            let r = contrast_ratio(rgb(tk.text), rgb(zemin));
            assert!(r >= 7.0, "{name}: text/{zemin_adi} = {r:.2}");
            let r = contrast_ratio(rgb(tk.text_weak), rgb(zemin));
            assert!(r >= 4.5, "{name}: text_weak/{zemin_adi} = {r:.2}");
        }
        // Durum renkleri metin olarak kullanılır: kart/panel zemininde AA.
        for (ad, c) in [
            ("accent", tk.accent),
            ("success", tk.success),
            ("warning", tk.warning),
            ("danger", tk.danger),
            ("info", tk.info),
        ] {
            for zemin in [tk.bg, tk.surface] {
                let r = contrast_ratio(rgb(c), rgb(zemin));
                assert!(r >= 4.5, "{name}: {ad} = {r:.2}");
            }
        }
        // Rozetler: durum rengi kendi yumuşak zemininde AA.
        for (ad, c, z) in [
            ("success", tk.success, tk.success_soft),
            ("warning", tk.warning, tk.warning_soft),
            ("danger", tk.danger, tk.danger_soft),
            ("info", tk.info, tk.info_soft),
            ("accent", tk.accent, tk.accent_soft),
        ] {
            let r = contrast_ratio(rgb(c), rgb(z));
            assert!(r >= 4.5, "{name}: {ad}/soft = {r:.2}");
        }
        // Vurgu düğmesi metni.
        let r = contrast_ratio(rgb(tk.on_accent), rgb(tk.accent_fill));
        assert!(r >= 4.5, "{name}: on_accent = {r:.2}");
        // Kenarlık zeminden ayırt edilir (dekoratif, ≥ 1.3).
        let r = contrast_ratio(rgb(tk.border_strong), rgb(tk.bg));
        assert!(r >= 2.0, "{name}: border_strong = {r:.2}");
    }

    #[test]
    fn theme_tokens_meet_contrast_targets() {
        check(&DARK);
        check(&LIGHT);
    }

    #[test]
    fn build_style_applies_tokens_for_both_themes() {
        let base = egui::Style::default();
        let d = build_style(&base, &DARK);
        assert!(d.visuals.dark_mode);
        assert_eq!(d.visuals.panel_fill, DARK.bg);
        assert_eq!(d.visuals.hyperlink_color, DARK.accent);
        let l = build_style(&base, &LIGHT);
        assert!(!l.visuals.dark_mode);
        assert_eq!(l.visuals.panel_fill, LIGHT.bg);
        assert_eq!(l.visuals.widgets.inactive.corner_radius, corner(radius::S));
    }

    #[test]
    fn install_switches_theme() {
        let ctx = egui::Context::default();
        install(&ctx, ThemeChoice::Light);
        assert_eq!(ctx.theme(), Theme::Light);
        assert_eq!(tokens_ctx(&ctx).bg, LIGHT.bg);
        install(&ctx, ThemeChoice::Dark);
        assert_eq!(ctx.theme(), Theme::Dark);
        assert_eq!(ThemeChoice::Dark.toggled(), ThemeChoice::Light);
    }

    #[test]
    fn margin_clamps_to_i8() {
        assert_eq!(margin(8.0), Margin::same(8));
        assert_eq!(margin(500.0), Margin::same(127));
        assert_eq!(margin(-3.0), Margin::same(0));
    }
}
