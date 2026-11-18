// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yeniden kullanılabilir 2B kanat profili (airfoil) önizleme bileşeni.
//!
//! Mevcut NACA 4-haneli üreteçten (`crate::cad::airfoil::generate_naca4_points`) ya da `.dat`
//! koordinat verisinden gelen noktaları `egui::Painter` ile eşit x/y ölçeğinde çizer:
//! veter çizgisi, hafif dolgulu kapalı profil konturu, profil adı ile maks. kalınlık / kamber
//! etiketleri. Tüm renkler yalnız `theme::Tokens` üzerinden alınır.

use crate::cad::airfoil::{
    AirfoilMetrics, AirfoilProfile, analyze_airfoil_points, generate_naca4_points,
    interp_surface_y, split_airfoil_surfaces,
};
use crate::i18n::{t, tf};
use crate::ui::theme::{self, font, radius, space};
use egui::{
    Align2, FontId, Mesh, Pos2, Rect, Response, RichText, Shape, Stroke, StrokeKind, Ui,
    epaint::Vertex, pos2, vec2,
};

/// Önizleme bileşeni görünüm seçenekleri.
#[derive(Clone, Copy, Debug)]
pub struct AirfoilPreviewOptions<'a> {
    /// Çizim alanının piksel yüksekliği.
    pub canvas_height: f32,
    /// Profil adını göster.
    pub show_name: bool,
    /// Maks. kalınlık (t/c ve x/c) ile maks. kamber etiketlerini göster.
    pub show_metrics: bool,
    /// Dış kart çerçevesi çiz.
    pub framed: bool,
    /// Üstte gösterilecek isteğe bağlı rol etiketi (ör. "Seçili kanat profili").
    pub role: Option<&'a str>,
}

impl Default for AirfoilPreviewOptions<'_> {
    fn default() -> Self {
        Self {
            canvas_height: 72.0,
            show_name: true,
            show_metrics: true,
            framed: true,
            role: None,
        }
    }
}

/// Ekran koordinatlarına dönüştürülmüş profil geometrisi (testler ve çizim paylaşır).
#[derive(Clone, Debug)]
pub struct AirfoilScreenGeom {
    pub outline: Vec<Pos2>,
    pub chord: [Pos2; 2],
    pub scale: f32,
    pub tris: Vec<[Pos2; 3]>,
}

/// Profil noktalarını çözümler; `points` boşsa ve geçerli 4 haneli NACA kodu varsa üretir.
pub fn resolve_profile_points(profile: &AirfoilProfile) -> Vec<[f64; 2]> {
    let finite: Vec<[f64; 2]> = profile
        .points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if finite.len() >= 3 {
        return finite;
    }
    if profile.is_naca && profile.naca_code.len() == 4 {
        if let Ok(pts) = generate_naca4_points(
            &profile.naca_code,
            profile.num_points.clamp(40, 200),
            profile.closed_te,
        ) {
            return pts;
        }
    }
    Vec::new()
}

/// Normalize profil noktalarını eşit x/y ölçekle `canvas` dikdörtgenine yerleştirir.
pub fn build_screen_geometry(points: &[[f64; 2]], canvas: Rect) -> Option<AirfoilScreenGeom> {
    let finite: Vec<[f64; 2]> = points
        .iter()
        .copied()
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if finite.len() < 3 {
        return None;
    }
    let xmin = finite.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let xmax = finite
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let chord = xmax - xmin;
    if !chord.is_finite() || chord <= 1e-9 {
        return None;
    }
    let norm: Vec<[f64; 2]> = finite
        .into_iter()
        .map(|p| [(p[0] - xmin) / chord, p[1] / chord])
        .collect();

    let ymin = norm.iter().map(|p| p[1]).fold(0.0_f64, f64::min);
    let ymax = norm.iter().map(|p| p[1]).fold(0.0_f64, f64::max);
    let dx = 1.0_f32;
    let dy = ((ymax - ymin) as f32).max(0.04);

    let pad = vec2(
        8.0_f32.min(canvas.width() * 0.15),
        6.0_f32.min(canvas.height() * 0.15),
    );
    let inner = canvas.shrink2(pad);
    if !inner.is_positive() {
        return None;
    }

    let scale = (inner.width() / dx).min(inner.height() / dy);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    let cx = 0.5_f32;
    let cy = (0.5 * (ymin + ymax)) as f32;
    let center = inner.center();
    let to_screen = |x: f64, y: f64| -> Pos2 {
        pos2(
            center.x + (x as f32 - cx) * scale,
            center.y - (y as f32 - cy) * scale,
        )
    };

    let outline: Vec<Pos2> = norm.iter().map(|p| to_screen(p[0], p[1])).collect();
    let chord_pts = [to_screen(0.0, 0.0), to_screen(1.0, 0.0)];

    let mut tris = Vec::new();
    if let Some(surfaces) = split_airfoil_surfaces(&norm) {
        const SLICES: usize = 48;
        let mut prev: Option<(Pos2, Pos2)> = None;
        for i in 0..=SLICES {
            let x = i as f64 / SLICES as f64;
            if let (Some(yu), Some(yl)) = (
                interp_surface_y(&surfaces.upper, x),
                interp_surface_y(&surfaces.lower, x),
            ) {
                let (top, bot) = if yu >= yl { (yu, yl) } else { (yl, yu) };
                let pu = to_screen(x, top);
                let pl = to_screen(x, bot);
                if let Some((pu0, pl0)) = prev {
                    if pu0 != pl0 || pu != pl {
                        tris.push([pu0, pu, pl]);
                        tris.push([pu0, pl, pl0]);
                    }
                }
                prev = Some((pu, pl));
            }
        }
    }

    Some(AirfoilScreenGeom {
        outline,
        chord: chord_pts,
        scale,
        tris,
    })
}

/// Profil adını üretir; ad boşsa NACA kodundan veya varsayılan metinden tamamlar.
fn display_name(profile: &AirfoilProfile) -> String {
    let trimmed = profile.name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if profile.is_naca && !profile.naca_code.trim().is_empty() {
        return format!("NACA {}", profile.naca_code.trim());
    }
    "-".to_string()
}

/// Seçilen ya da üzerine gelinen profili canlı 2B olarak çizer.
pub fn airfoil_preview(
    ui: &mut Ui,
    profile: &AirfoilProfile,
    opts: AirfoilPreviewOptions<'_>,
) -> Response {
    if opts.framed {
        let tk = theme::tokens(ui);
        egui::Frame::new()
            .fill(tk.surface)
            .stroke(Stroke::new(1.0, tk.border))
            .corner_radius(theme::corner(radius::M))
            .inner_margin(theme::margin(space::S))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                render_inner(ui, profile, opts);
            })
            .response
    } else {
        ui.vertical(|ui| render_inner(ui, profile, opts)).response
    }
}

fn render_inner(ui: &mut Ui, profile: &AirfoilProfile, opts: AirfoilPreviewOptions<'_>) {
    let tk = theme::tokens(ui);
    let pts = resolve_profile_points(profile);
    let metrics: Option<AirfoilMetrics> = analyze_airfoil_points(&pts);

    if let Some(role) = opts.role {
        ui.add(
            egui::Label::new(RichText::new(role).size(font::CAPTION).color(tk.text_weak)).wrap(),
        );
    }
    if opts.show_name {
        ui.add(
            egui::Label::new(
                RichText::new(display_name(profile))
                    .size(font::BODY)
                    .strong()
                    .color(tk.text),
            )
            .wrap(),
        );
    }

    let w = ui.available_width().max(36.0);
    let h = opts.canvas_height.max(28.0);
    let (resp, painter) = ui.allocate_painter(vec2(w, h), egui::Sense::hover());
    let rect = resp.rect;

    painter.rect_filled(rect, theme::corner(radius::S), tk.viewport_bg);
    painter.rect_stroke(
        rect,
        theme::corner(radius::S),
        Stroke::new(1.0, tk.border),
        StrokeKind::Inside,
    );

    let clipped = painter.with_clip_rect(rect);
    if let Some(geom) = build_screen_geometry(&pts, rect) {
        if !geom.tris.is_empty() {
            let fill_col = tk.accent.gamma_multiply(0.18);
            let mut mesh = Mesh::default();
            for tri in &geom.tris {
                let base = mesh.vertices.len() as u32;
                for &p in tri {
                    mesh.vertices.push(Vertex {
                        pos: p,
                        uv: Pos2::ZERO,
                        color: fill_col,
                    });
                }
                mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            }
            clipped.add(Shape::mesh(mesh));
        }
        clipped.extend(Shape::dashed_line(
            &geom.chord,
            Stroke::new(1.0, tk.text_weak),
            4.0,
            3.0,
        ));
        clipped.add(Shape::closed_line(
            geom.outline,
            Stroke::new(1.75, tk.accent),
        ));
    } else {
        clipped.text(
            rect.center(),
            Align2::CENTER_CENTER,
            t("airfoil.preview.empty"),
            FontId::proportional(font::CAPTION),
            tk.text_weak,
        );
    }

    if opts.show_metrics {
        if let Some(m) = metrics {
            let tc_pct = format!("{:.1}", m.max_thickness * 100.0);
            let tc_x = format!("{:.0}", m.max_thickness_x * 100.0);
            let t_txt = tf("airfoil.preview.thickness", &[tc_pct, tc_x]);
            let c_txt = if m.max_camber < 0.001 {
                t("airfoil.preview.symmetric").to_string()
            } else {
                let cam_pct = format!("{:.1}", m.max_camber * 100.0);
                let cam_x = format!("{:.0}", m.max_camber_x * 100.0);
                tf("airfoil.preview.camber", &[cam_pct, cam_x])
            };
            ui.add(
                egui::Label::new(RichText::new(t_txt).size(font::CAPTION).color(tk.text_weak))
                    .wrap(),
            );
            ui.add(
                egui::Label::new(RichText::new(c_txt).size(font::CAPTION).color(tk.text_weak))
                    .wrap(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cad::airfoil::parse_selig_dat;
    use crate::i18n::{Lang, with_lang};
    use crate::ui::aero_simple::PRESET_NACA_CODES;
    use crate::ui::theme::ThemeChoice;
    use egui::epaint::ClippedShape;

    const SAMPLE_CLARK_Y: &str = r#"CLARK Y
1.0000 0.0010
0.8000 0.0380
0.6000 0.0680
0.4000 0.0880
0.3000 0.0915
0.2000 0.0860
0.1000 0.0660
0.0500 0.0470
0.0000 0.0000
0.0500 -0.0210
0.1000 -0.0250
0.2000 -0.0260
0.3000 -0.0255
0.4000 -0.0220
0.6000 -0.0150
0.8000 -0.0070
1.0000 -0.0010
"#;

    fn selectable_profiles() -> Vec<AirfoilProfile> {
        let mut list: Vec<AirfoilProfile> = PRESET_NACA_CODES
            .iter()
            .map(|code| AirfoilProfile::from_naca4(code, 80, true).unwrap())
            .collect();
        list.push(parse_selig_dat(SAMPLE_CLARK_Y).unwrap());
        list
    }

    fn collect_closed_paths(shapes: &[ClippedShape]) -> usize {
        fn walk(s: &Shape, n: &mut usize) {
            match s {
                Shape::Vec(v) => v.iter().for_each(|x| walk(x, n)),
                Shape::Path(p) if p.closed && p.points.len() >= 10 => *n += 1,
                _ => {}
            }
        }
        let mut count = 0;
        for cs in shapes {
            walk(&cs.shape, &mut count);
        }
        count
    }

    #[test]
    fn outline_is_closed_finite_and_equal_aspect_for_all_profiles() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(260.0, 80.0));
        for prof in selectable_profiles() {
            let pts = resolve_profile_points(&prof);
            let geom = build_screen_geometry(&pts, rect)
                .unwrap_or_else(|| panic!("geometri uretilemedi: {}", prof.name));
            assert!(geom.outline.len() >= 10, "{}", prof.name);
            assert!(geom.scale.is_finite() && geom.scale > 0.0);
            for p in &geom.outline {
                assert!(p.x.is_finite() && p.y.is_finite(), "{}", prof.name);
                assert!(
                    rect.expand(1.0).contains(*p),
                    "{}: nokta {p:?} kutu disinda {rect:?}",
                    prof.name
                );
            }
            // Kord uzunlugu piksel cinsinden `geom.scale * 1.0` olmali (esit x/y olcegi).
            let chord_len = (geom.chord[1].x - geom.chord[0].x).abs();
            assert!((chord_len - geom.scale).abs() < 1e-2, "{}", prof.name);

            // Yukseklik orani kontrolu: veri y farki * scale == ekran y farki.
            let y_min_data = pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let y_max_data = pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
            let x_min_data = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let x_max_data = pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
            let chord_data = x_max_data - x_min_data;
            let expected_h = ((y_max_data - y_min_data) / chord_data) as f32 * geom.scale;
            let y_min_px = geom
                .outline
                .iter()
                .map(|p| p.y)
                .fold(f32::INFINITY, f32::min);
            let y_max_px = geom
                .outline
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                ((y_max_px - y_min_px) - expected_h).abs() < 0.5,
                "{}: esit x/y olcek korunmali",
                prof.name
            );
        }
    }

    #[test]
    fn naca_2412_metrics_are_12pct_at_30pct_and_2pct_camber_at_40pct() {
        let prof = AirfoilProfile::from_naca4("2412", 120, true).unwrap();
        let m = prof.metrics().expect("NACA 2412 metrics");
        assert!((m.max_thickness - 0.12).abs() < 0.004, "{m:?}");
        assert!((m.max_thickness_x - 0.30).abs() < 0.03, "{m:?}");
        assert!((m.max_camber - 0.02).abs() < 0.003, "{m:?}");
        assert!((m.max_camber_x - 0.40).abs() < 0.03, "{m:?}");
    }

    #[test]
    fn renders_headless_for_every_selectable_profile_in_both_themes() {
        let mut profiles = selectable_profiles();
        // Bos profil de panik yapmamali.
        profiles.push(AirfoilProfile {
            name: "Empty".into(),
            is_naca: false,
            naca_code: String::new(),
            closed_te: true,
            num_points: 0,
            points: Vec::new(),
        });

        for theme_choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [Lang::Tr, Lang::En] {
                with_lang(lang, || {
                    for (idx, prof) in profiles.iter().enumerate() {
                        let ctx = egui::Context::default();
                        theme::install(&ctx, theme_choice);
                        let mut shapes = Vec::new();
                        for _ in 0..2 {
                            let mut input = egui::RawInput::default();
                            input.screen_rect =
                                Some(Rect::from_min_size(Pos2::ZERO, vec2(320.0, 400.0)));
                            let mut out = ctx.run_ui(input, |ui| {
                                egui::CentralPanel::default().show(ui, |ui| {
                                    airfoil_preview(ui, prof, AirfoilPreviewOptions::default());
                                });
                            });
                            out.textures_delta.clear();
                            shapes = out.shapes;
                        }
                        if idx + 1 < profiles.len() {
                            assert!(
                                collect_closed_paths(&shapes) >= 1,
                                "{:?} {:?} {}: kapali profil cizilmeli",
                                theme_choice,
                                lang,
                                prof.name
                            );
                        }
                        for cs in &shapes {
                            let b = cs.shape.visual_bounding_rect();
                            if b.is_positive() {
                                assert!(
                                    b.min.x.is_finite()
                                        && b.min.y.is_finite()
                                        && b.max.x.is_finite()
                                        && b.max.y.is_finite(),
                                    "{}: sonlu olmayan sekil siniri {b:?}",
                                    prof.name
                                );
                            }
                        }
                    }
                });
            }
        }
    }
}
