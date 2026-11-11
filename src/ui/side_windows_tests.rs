// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yan pencerelerin (Öğretmen, Yapay zekâ, Rüzgâr tüneli, Formüller, Yardım, 2D şema) temaya
//! bağlı olduğunu denetler: kaynakta sabit renk yok, çizilen renkler tokenlardan gelir, iki temada
//! ve iki dilde başsız çizim paniksiz, şemanın renkleri zeminde okunur.

use crate::ai::client::parse_ai_response;
use crate::ai::config::AiConfig;
use crate::ai::credential::{CredentialStorage, SessionCredentialStore};
use crate::ai::state::{AiAssistantState, ChatMessage, MessageRole};
use crate::calc::{Library, Model, evaluate};
use crate::i18n::Lang;
use crate::project::{DroneType, Project};
use crate::teach::{ResultMetric, TeacherSettings};
use crate::ui::components::format::contrast_ratio;
use crate::ui::theme::{self, ThemeChoice, Tokens};
use crate::ui::{formulas, schematic};
use egui::epaint::{ClippedShape, ColorMode, Shape};
use egui::{Color32, Pos2, Rect, vec2};
use std::sync::Arc;

const SIZE: egui::Vec2 = vec2(1100.0, 800.0);

fn run(ctx: &egui::Context, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<ClippedShape> {
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, SIZE));
    let mut out = ctx.run_ui(input, |ui| f(ui));
    out.textures_delta.clear();
    out.shapes
}

fn themes() -> [ThemeChoice; 2] {
    [ThemeChoice::Dark, ThemeChoice::Light]
}

fn fixture(dt: DroneType) -> (Project, Model) {
    let mut p = Project::default();
    p.set_drone_type(dt);
    let m = evaluate(&p, &Library::embedded());
    (p, m)
}

// ------------------------------------------------------------------------------------------
// Kaynak denetimi: yan pencere dosyalarında sabit renk / grup çerçevesi yok
// ------------------------------------------------------------------------------------------

#[test]
fn side_window_sources_have_no_hardcoded_colors() {
    const FILES: &[(&str, &str)] = &[
        ("ai.rs", include_str!("ai.rs")),
        ("teach.rs", include_str!("teach.rs")),
        ("windtunnel.rs", include_str!("windtunnel.rs")),
        ("formulas.rs", include_str!("formulas.rs")),
        ("help.rs", include_str!("help.rs")),
        ("schematic.rs", include_str!("schematic.rs")),
        ("outputs.rs", include_str!("outputs.rs")),
        ("settings_page.rs", include_str!("settings_page.rs")),
        ("type_cards.rs", include_str!("type_cards.rs")),
        ("panel_kit.rs", include_str!("panel_kit.rs")),
        ("wizard.rs", include_str!("wizard.rs")),
        ("widgets.rs", include_str!("widgets.rs")),
        ("mod.rs", include_str!("mod.rs")),
    ];
    const FORBIDDEN: &[&str] = &[
        "Color32::from_rgb(",
        "Color32::from_gray(",
        "Color32::from_black_alpha(",
        "Color32::from_white_alpha(",
        "Color32::BLACK",
        "Color32::WHITE",
        "Color32::RED",
        "Color32::GREEN",
        "Color32::YELLOW",
        "Color32::BLUE",
        "Color32::GRAY",
        "Frame::group(",
    ];
    for (name, src) in FILES {
        // Yalnızca üretim kodu (testler `#[cfg(test)]` ile başlar).
        let prod = src.split("#[cfg(test)]").next().unwrap();
        for bad in FORBIDDEN {
            assert!(
                !prod.contains(bad),
                "{name}: sabit renk/çerçeve kaldı: {bad}"
            );
        }
    }
}

// ------------------------------------------------------------------------------------------
// Çizilen renkler tokenlardan gelir (2D şema)
// ------------------------------------------------------------------------------------------

fn token_colors(tk: &Tokens) -> Vec<Color32> {
    vec![
        tk.bg,
        tk.surface,
        tk.surface_alt,
        tk.extreme,
        tk.border,
        tk.border_strong,
        tk.text,
        tk.text_weak,
        tk.accent,
        tk.accent_fill,
        tk.on_accent,
        tk.accent_soft,
        tk.success,
        tk.warning,
        tk.danger,
        tk.info,
        tk.success_soft,
        tk.warning_soft,
        tk.danger_soft,
        tk.info_soft,
        tk.viewport_bg,
        tk.viewport_grid,
        tk.viewport_grid_major,
    ]
}

fn style_colors(ctx: &egui::Context) -> Vec<Color32> {
    let v = ctx.global_style().visuals.clone();
    let mut c = vec![
        v.text_color(),
        v.weak_text_color(),
        v.strong_text_color(),
        v.hyperlink_color,
        v.window_fill,
        v.panel_fill,
        v.extreme_bg_color,
        v.faint_bg_color,
        v.code_bg_color,
        v.warn_fg_color,
        v.error_fg_color,
        v.selection.bg_fill,
        v.selection.stroke.color,
        v.window_stroke.color,
    ];
    for w in [
        &v.widgets.noninteractive,
        &v.widgets.inactive,
        &v.widgets.hovered,
        &v.widgets.active,
        &v.widgets.open,
    ] {
        c.extend([
            w.bg_fill,
            w.weak_bg_fill,
            w.fg_stroke.color,
            w.bg_stroke.color,
        ]);
    }
    c
}

fn same_rgb(a: Color32, b: Color32) -> bool {
    // Alfa ile çarpılmış (gamma_multiply) renklerin rengi yuvarlama nedeniyle biraz sapar.
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, _] = b.to_srgba_unmultiplied();
    let tol = 3 + (255 / aa.max(1) as i32).min(40);
    (ar as i32 - br as i32).abs() <= tol
        && (ag as i32 - bg as i32).abs() <= tol
        && (ab as i32 - bb as i32).abs() <= tol
}

fn painted_colors(shapes: &[ClippedShape]) -> Vec<Color32> {
    fn walk(s: &Shape, out: &mut Vec<Color32>) {
        match s {
            Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
            Shape::Circle(c) => out.extend([c.fill, c.stroke.color]),
            Shape::Ellipse(e) => out.extend([e.fill, e.stroke.color]),
            Shape::LineSegment { stroke, .. } => out.push(stroke.color),
            Shape::Path(p) => {
                out.push(p.fill);
                if let ColorMode::Solid(c) = p.stroke.color {
                    out.push(c);
                }
            }
            Shape::Rect(r) => out.extend([r.fill, r.stroke.color]),
            Shape::Text(t) => {
                out.push(t.fallback_color);
                if let Some(c) = t.override_text_color {
                    out.push(c);
                }
                for sec in &t.galley.job.sections {
                    if sec.format.color != Color32::PLACEHOLDER {
                        out.push(sec.format.color);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for cs in shapes {
        walk(&cs.shape, &mut out);
    }
    out
}

fn assert_palette_only(shapes: &[ClippedShape], ctx: &egui::Context, tag: &str) {
    let tk = theme::tokens_ctx(ctx);
    let mut allowed = token_colors(tk);
    allowed.extend(style_colors(ctx));
    let mut n = 0;
    for c in painted_colors(shapes) {
        if c == Color32::TRANSPARENT || c.a() == 0 {
            continue;
        }
        n += 1;
        let ok = allowed.iter().any(|a| same_rgb(c, *a))
            // Gölgeler: siyah, saydam.
            || (c.a() < 255 && c.r() < 8 && c.g() < 8 && c.b() < 8)
            // Yazı rengi metin tokenının egui'nin soluklaştırdığı tonu.
            || same_rgb(c, tk.text.gamma_multiply(0.6))
            || allowed.iter().any(|a| same_rgb(c, theme_blend(*a, tk.surface)));
        assert!(ok, "{tag}: tokenlar dışında renk: {c:?}");
    }
    assert!(n > 10, "{tag}: çizim bulunamadı ({n})");
}

/// egui'nin soluk metin rengi: metin rengi zemine doğru karıştırılır.
fn theme_blend(a: Color32, b: Color32) -> Color32 {
    Color32::from_rgb(
        ((a.r() as u16 + b.r() as u16) / 2) as u8,
        ((a.g() as u16 + b.g() as u16) / 2) as u8,
        ((a.b() as u16 + b.b() as u16) / 2) as u8,
    )
}

#[test]
fn schematic_paints_only_theme_colors_for_every_type_and_layout() {
    use crate::calc::multirotor::MultirotorLayout;
    for choice in themes() {
        for dt in DroneType::ALL {
            let mut variants: Vec<(Project, Model)> = vec![fixture(dt)];
            if dt == DroneType::Multirotor {
                for layout in [MultirotorLayout::H, MultirotorLayout::Plus] {
                    let (mut p, _) = fixture(dt);
                    p.mr_layout = layout;
                    let m = evaluate(&p, &Library::embedded());
                    variants.push((p, m));
                }
                let (mut p, _) = fixture(dt);
                p.mr_coaxial = true;
                let m = evaluate(&p, &Library::embedded());
                variants.push((p, m));
                // Pervane çakışması (kırmızı uyarı) yolu.
                let (mut p, _) = fixture(dt);
                p.mr_d_prop = 0.45;
                let m = evaluate(&p, &Library::embedded());
                variants.push((p, m));
            }
            for (i, (p, m)) in variants.iter().enumerate() {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let shapes = run(&ctx, &mut |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        schematic::show(ui, p, m, 420.0);
                    });
                });
                assert_palette_only(&shapes, &ctx, &format!("{dt:?}#{i} {choice:?}"));
            }
        }
    }
}

#[test]
fn schematic_label_and_shape_colors_are_readable_on_the_viewport_background() {
    for tk in [&theme::DARK, &theme::LIGHT] {
        let bg = tk.viewport_bg;
        let rgb = |c: Color32| [c.r(), c.g(), c.b()];
        let name = if tk.dark { "koyu" } else { "açık" };
        // Etiketler (yazı): AA.
        for (n, c) in [
            ("text_weak", tk.text_weak),
            ("success", tk.success),
            ("info", tk.info),
            ("accent", tk.accent),
            ("warning", tk.warning),
            ("danger", tk.danger),
        ] {
            let r = contrast_ratio(rgb(c), rgb(bg));
            assert!(r >= 4.5, "{name}: {n} etiketi zeminde {r:.2}");
        }
        // Şekil çizgileri: grafik nesne (3:1).
        for (n, c) in [
            ("accent", tk.accent),
            ("danger", tk.danger),
            ("warning", tk.warning),
            ("text_weak", tk.text_weak),
            ("success", tk.success),
        ] {
            let r = contrast_ratio(rgb(c), rgb(bg));
            assert!(r >= 3.0, "{name}: {n} çizgisi zeminde {r:.2}");
        }
        // Kollar/bomlar yapı çizgisi (kalın): zeminden ayrışmalı.
        let r = contrast_ratio(rgb(tk.border_strong), rgb(bg));
        assert!(r >= 1.4, "{name}: border_strong zeminde {r:.2}");
        // Işık: ızgara çizgisi zeminden çok az farklı (dekoratif).
        assert_ne!(tk.viewport_grid, bg);
    }
}

// ------------------------------------------------------------------------------------------
// Başsız çizim: iki tema × iki dil
// ------------------------------------------------------------------------------------------

fn store() -> Arc<dyn CredentialStorage> {
    Arc::new(SessionCredentialStore::new())
}

#[test]
fn teacher_windows_render_in_both_themes_and_languages() {
    let (p, m) = fixture(DroneType::FixedWingConventional);
    for choice in themes() {
        for lang in [Lang::Tr, Lang::En] {
            let ctx = egui::Context::default();
            theme::install(&ctx, choice);
            let mut st = crate::ui::teach::TeacherUiState::new(TeacherSettings::default());
            let mut ai = AiAssistantState::new();
            let cfg = AiConfig {
                enabled: true,
                ..AiConfig::default()
            };
            crate::i18n::with_lang(lang, || {
                st.is_open = true;
                st.open_why(ResultMetric::VStall, &p, &m, lang);
                st.open_classroom_report(&p, &m, lang);
                for lesson in [1_u8, 3, 7] {
                    st.selected_lesson_id = lesson;
                    for _ in 0..2 {
                        run(&ctx, &mut |ui| {
                            st.show(ui.ctx(), &p, &m, &mut ai, &cfg, store());
                        });
                    }
                }
            });
        }
    }
}

#[test]
fn ai_panel_and_dialogs_render_in_both_themes_and_languages() {
    let raw = r#"{"explanation":"Açıklama","changes":[
        {"field":"b","value":2.4,"reason":"neden"},
        {"field":"b","value":99.0,"reason":"aralık dışı"},
        {"field":"flux","value":1.0,"reason":"yok"}]}"#;
    for choice in themes() {
        for lang in [Lang::Tr, Lang::En] {
            for enabled in [false, true] {
                let ctx = egui::Context::default();
                theme::install(&ctx, choice);
                let (mut p, m) = fixture(DroneType::FixedWingConventional);
                let mut st = AiAssistantState::new();
                st.is_panel_open = true;
                st.show_cloud_warning_modal = true;
                let mut cfg = AiConfig {
                    enabled,
                    ..AiConfig::default()
                };
                crate::i18n::with_lang(lang, || {
                    if enabled {
                        st.error_message = Some("hata iletisi".into());
                        st.status_message = Some("durum iletisi".into());
                        st.is_working = true;
                        for (role, text) in [
                            (MessageRole::User, "Kanadı büyüt"),
                            (MessageRole::Assistant, "Tamam, öneriler hazır"),
                        ] {
                            st.chat_history.push(ChatMessage {
                                role,
                                text: text.into(),
                                timestamp: "12:00:00".into(),
                            });
                        }
                        st.active_proposal = Some(parse_ai_response(raw, &p).unwrap());
                    }
                    let store = store();
                    for _ in 0..2 {
                        run(&ctx, &mut |ui| {
                            crate::ui::ai::show_ai_panel(
                                ui, &mut st, &mut cfg, &mut p, &m, "Adım", &store,
                            );
                            let pc = p.clone();
                            crate::ui::ai::show_cloud_warning_dialog(
                                ui.ctx(),
                                &mut st,
                                &mut cfg,
                                &pc,
                                lang,
                                &store,
                            );
                        });
                    }
                });
            }
        }
    }
}

#[test]
fn every_formula_section_renders_in_both_themes() {
    for choice in themes() {
        for dt in [DroneType::FixedWingConventional, DroneType::Multirotor] {
            let (p, m) = fixture(dt);
            let ctx = egui::Context::default();
            theme::install(&ctx, choice);
            let fns: [fn(&mut egui::Ui, &Project, &Model); 14] = [
                formulas::show_mission_formulas,
                formulas::show_weight_formulas,
                formulas::show_geometry_formulas,
                formulas::show_aero_formulas,
                formulas::show_design_point_formulas,
                formulas::show_endurance_formulas,
                formulas::show_propulsion_formulas,
                formulas::show_stability_formulas,
                formulas::show_structure_formulas,
                formulas::show_mr_frame_formulas,
                formulas::show_mr_propulsion_formulas,
                formulas::show_mr_battery_formulas,
                formulas::show_mr_cg_formulas,
                formulas::show_summary_formulas,
            ];
            for f in fns {
                // Açık başlıkla da çiz: bütün çerçeveler ve renkler devreye girsin.
                ctx.data_mut(|d| d.clear());
                run(&ctx, &mut |ui| {
                    egui::CentralPanel::default().show(ui, |ui| f(ui, &p, &m));
                });
            }
        }
    }
}

#[test]
fn panel_kit_frames_render_with_theme_colors() {
    for choice in themes() {
        let ctx = egui::Context::default();
        theme::install(&ctx, choice);
        let shapes = run(&ctx, &mut |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let tk = theme::tokens(ui);
                crate::ui::panel_kit::tinted_frame(tk, tk.success_soft)
                    .show(ui, |ui| ui.label("a"));
                crate::ui::panel_kit::accent_frame(tk).show(ui, |ui| ui.label("b"));
                crate::ui::panel_kit::surface_frame(tk).show(ui, |ui| ui.label("c"));
            });
        });
        let tk = theme::tokens_ctx(&ctx);
        let cols = painted_colors(&shapes);
        for want in [tk.success_soft, tk.accent_soft, tk.surface_alt] {
            assert!(
                cols.iter().any(|c| same_rgb(*c, want)),
                "{choice:?}: {want:?}"
            );
        }
    }
}
