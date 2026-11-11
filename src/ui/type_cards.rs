// SPDX-License-Identifier: AGPL-3.0-or-later
//! Araç türü seçim kartları: her kartta o türün sade 2D silueti (egui painter, renkler temadan),
//! ad, bir satırlık açıklama ve "kimler için" notu.
//!
//! Siluetler birim kareye (-1..1) çizilmiş saf veridir (`silhouette`); testlenebilir ve her
//! boyuta ölçeklenir. Kart yerleşimi de saf fonksiyondadır (`card_rects`).

use super::components::{Status, badge};
use super::theme::{self, Tokens, font, radius, space};
use crate::i18n::t;
use crate::project::DroneType;
use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

/// Siluet alanı yüksekliği [px].
const SIL_H: f32 = 84.0;
const PAD: f32 = 10.0;
const GAP: f32 = 8.0;
const MIN_CARD_W: f32 = 200.0;
const MAX_COLS: usize = 3;

/// Birim kare (-1..1) içinde bir silüet: dolu çokgenler, daireler (merkez, yarıçap) ve çizgiler.
/// Üstten görünüm; burun yukarıda.
#[derive(Clone, Debug, Default)]
pub struct Silhouette {
    pub polys: Vec<Vec<[f32; 2]>>,
    pub circles: Vec<([f32; 2], f32)>,
    pub lines: Vec<([f32; 2], [f32; 2])>,
}

fn mirror_x(poly: &[[f32; 2]]) -> Vec<[f32; 2]> {
    poly.iter().rev().map(|p| [-p[0], p[1]]).collect()
}

fn fuselage(half_w: f32, top: f32, bottom: f32) -> Vec<[f32; 2]> {
    // Burun sivri, kuyruk dar: sade pod.
    vec![
        [0.0, top],
        [half_w, top - 0.30],
        [half_w, bottom + 0.15],
        [half_w * 0.4, bottom],
        [-half_w * 0.4, bottom],
        [-half_w, bottom + 0.15],
        [-half_w, top - 0.30],
    ]
}

/// Verilen türün silueti.
pub fn silhouette(dt: DroneType) -> Silhouette {
    let mut s = Silhouette::default();
    match dt {
        DroneType::FixedWingConventional | DroneType::FixedWingVTail => {
            s.polys.push(fuselage(0.07, 0.95, -0.95));
            // Ana kanat (hafif oklu yamuk), sağ yarı + ayna.
            let wing = vec![[0.07, 0.30], [0.98, 0.05], [0.98, -0.12], [0.07, -0.25]];
            s.polys.push(mirror_x(&wing));
            s.polys.push(wing);
            if dt == DroneType::FixedWingConventional {
                let tail = vec![[0.07, -0.72], [0.42, -0.84], [0.42, -0.94], [0.07, -0.93]];
                s.polys.push(mirror_x(&tail));
                s.polys.push(tail);
                s.lines.push(([0.0, -0.62], [0.0, -0.95]));
            } else {
                // V kuyruk: üstten bakınca dışa açılan iki dar yüzey.
                let v = vec![[0.07, -0.60], [0.36, -0.88], [0.30, -0.95], [0.07, -0.88]];
                s.polys.push(mirror_x(&v));
                s.polys.push(v);
            }
            s.lines.push(([-0.10, 0.97], [0.10, 0.97]));
        }
        DroneType::FlyingWing => {
            let half = vec![
                [0.0, 0.70],
                [0.98, -0.45],
                [0.98, -0.62],
                [0.55, -0.55],
                [0.0, -0.40],
            ];
            s.polys.push(mirror_x(&half));
            s.polys.push(half);
            // Kanat ucu dikmeleri (winglet).
            s.lines.push(([0.98, -0.45], [0.98, -0.80]));
            s.lines.push(([-0.98, -0.45], [-0.98, -0.80]));
            // İtici pervane (arka).
            s.lines.push(([-0.18, -0.58], [0.18, -0.58]));
        }
        DroneType::Multirotor => {
            s.polys.push(vec![
                [-0.18, 0.18],
                [0.18, 0.18],
                [0.18, -0.18],
                [-0.18, -0.18],
            ]);
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                s.lines
                    .push(([sx * 0.18, sy * 0.18], [sx * 0.60, sy * 0.60]));
                s.circles.push(([sx * 0.68, sy * 0.68], 0.27));
            }
        }
        DroneType::VtolQuadplane => {
            s.polys.push(fuselage(0.07, 0.95, -0.95));
            let wing = vec![[0.07, 0.25], [0.98, 0.08], [0.98, -0.08], [0.07, -0.20]];
            s.polys.push(mirror_x(&wing));
            s.polys.push(wing);
            let tail = vec![[0.07, -0.74], [0.38, -0.84], [0.38, -0.93], [0.07, -0.92]];
            s.polys.push(mirror_x(&tail));
            s.polys.push(tail);
            // İki bom + dört kaldırma pervanesi.
            for sx in [-1.0_f32, 1.0] {
                s.lines.push(([sx * 0.52, 0.62], [sx * 0.52, -0.62]));
                s.circles.push(([sx * 0.52, 0.62], 0.20));
                s.circles.push(([sx * 0.52, -0.62], 0.20));
            }
            s.lines.push(([-0.10, 0.97], [0.10, 0.97]));
        }
    }
    s
}

fn map(r: Rect, p: [f32; 2]) -> Pos2 {
    // Birim kareyi (en-boy oranı korunarak) dikdörtgene sığdırır.
    let side = r.width().min(r.height()) * 0.5;
    pos2(r.center().x + p[0] * side, r.center().y - p[1] * side)
}

/// Siluetini `rect` içine çizer (renkler temadan).
pub fn paint_silhouette(painter: &egui::Painter, rect: Rect, dt: DroneType, tk: &Tokens) {
    let s = silhouette(dt);
    let side = rect.width().min(rect.height()) * 0.5;
    let stroke = Stroke::new(1.5, tk.accent);
    let detail = Stroke::new(1.5, tk.text_weak);
    for poly in &s.polys {
        let pts: Vec<Pos2> = poly.iter().map(|p| map(rect, *p)).collect();
        painter.add(Shape::convex_polygon(pts, tk.surface_alt, stroke));
    }
    for (c, r) in &s.circles {
        let ctr = map(rect, *c);
        painter.circle(ctr, r * side, Color32::TRANSPARENT, detail);
        painter.circle_filled(ctr, 2.0, tk.text_weak);
    }
    for (a, b) in &s.lines {
        painter.line_segment([map(rect, *a), map(rect, *b)], detail);
    }
}

/// Kart ızgarası yerleşimi: `n` kartı `avail_w` genişliğe sütunlara böler. Döndürülen dikdörtgenler
/// `(0,0)` köşeli yerel koordinattadır; her satırın yüksekliği `row_h[satır]`.
pub fn card_rects(avail_w: f32, n: usize, row_h: &[f32]) -> Vec<Rect> {
    let avail_w = avail_w.max(1.0);
    let cols = (((avail_w + GAP) / (MIN_CARD_W + GAP)).floor() as usize).clamp(1, MAX_COLS);
    let cols = cols.min(n.max(1));
    let w = ((avail_w - GAP * (cols as f32 - 1.0)) / cols as f32).max(1.0);
    let mut out = Vec::with_capacity(n);
    let mut y = 0.0;
    for (row, start) in (0..n).step_by(cols).enumerate() {
        let h = row_h.get(row).copied().unwrap_or(0.0);
        for k in 0..cols {
            if start + k >= n {
                break;
            }
            out.push(Rect::from_min_size(
                pos2(k as f32 * (w + GAP), y),
                vec2(w, h),
            ));
        }
        y += h + GAP;
    }
    out
}

/// Sütun sayısı (dar pencerede 1).
pub fn columns_for(avail_w: f32, n: usize) -> usize {
    (((avail_w.max(1.0) + GAP) / (MIN_CARD_W + GAP)).floor() as usize)
        .clamp(1, MAX_COLS)
        .min(n.max(1))
}

fn who_text(dt: DroneType) -> &'static str {
    match dt {
        DroneType::FixedWingConventional => t("type_cards.for.fixed_wing_conv"),
        DroneType::FixedWingVTail => t("type_cards.for.fixed_wing_vtail"),
        DroneType::FlyingWing => t("type_cards.for.flying_wing"),
        DroneType::Multirotor => t("type_cards.for.multirotor"),
        DroneType::VtolQuadplane => t("type_cards.for.vtol_quadplane"),
    }
}

struct Galleys {
    name: std::sync::Arc<egui::Galley>,
    desc: std::sync::Arc<egui::Galley>,
    who_title: std::sync::Arc<egui::Galley>,
    who: std::sync::Arc<egui::Galley>,
}

fn layout_galleys(ui: &Ui, dt: DroneType, text_w: f32, tk: &Tokens) -> Galleys {
    let p = ui.painter();
    Galleys {
        name: p.layout(
            dt.label().to_string(),
            FontId::proportional(font::SUBHEAD),
            tk.text,
            text_w,
        ),
        desc: p.layout(
            dt.description().to_string(),
            FontId::proportional(font::BODY - 0.5),
            tk.text,
            text_w,
        ),
        who_title: p.layout(
            t("type_cards.for_title").to_string(),
            FontId::proportional(font::CAPTION),
            tk.accent,
            text_w,
        ),
        who: p.layout(
            who_text(dt).to_string(),
            FontId::proportional(font::CAPTION),
            tk.text_weak,
            text_w,
        ),
    }
}

const BTN_H: f32 = 26.0;

fn card_height(g: &Galleys) -> f32 {
    PAD + SIL_H
        + space::S
        + g.name.size().y
        + space::XS
        + g.desc.size().y
        + space::S
        + g.who_title.size().y
        + 2.0
        + g.who.size().y
        + space::S
        + BTN_H
        + PAD
}

fn rects_id() -> egui::Id {
    egui::Id::unique("type_cards_rects")
}

/// Son çizilen kartların ekran dikdörtgenleri (testler ve hata ayıklama için).
pub fn last_card_rects(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data(|d| d.get_temp::<Vec<Rect>>(rects_id()))
        .unwrap_or_default()
}

/// Araç türü kartlarını çizer. Bir kart seçildiyse (düğme ya da kart üzerine tıklama) türü döndürür.
pub fn type_card_grid(ui: &mut Ui, current: Option<DroneType>) -> Option<DroneType> {
    let tk = theme::tokens(ui);
    let types = DroneType::ALL;
    let avail_w = ui.available_width();
    let cols = columns_for(avail_w, types.len());
    let card_w = ((avail_w - GAP * (cols as f32 - 1.0)) / cols as f32).max(1.0);
    let text_w = (card_w - 2.0 * PAD).max(20.0);

    // Her kartın metin yerleşimi ve satır yükseklikleri (aynı satırdaki kartlar eşit yükseklikte).
    let galleys: Vec<Galleys> = types
        .iter()
        .map(|dt| layout_galleys(ui, *dt, text_w, tk))
        .collect();
    let heights: Vec<f32> = galleys.iter().map(card_height).collect();
    let row_h: Vec<f32> = heights
        .chunks(cols)
        .map(|c| c.iter().cloned().fold(0.0, f32::max))
        .collect();
    let local = card_rects(avail_w, types.len(), &row_h);
    let total_h = local.last().map(|r| r.max.y).unwrap_or(0.0);
    let (full, _) = ui.allocate_exact_size(vec2(avail_w, total_h), Sense::hover());

    let mut chosen = None;
    let mut shown = Vec::with_capacity(types.len());
    for (i, dt) in types.iter().enumerate() {
        let r = local[i].translate(full.min.to_vec2());
        shown.push(r);
        let is_current = current == Some(*dt);
        let resp = ui.interact(r, ui.scope_id().with(("type_card", i)), Sense::click());
        let hovered = resp.hovered();
        let painter = ui.painter();
        painter.rect(
            r,
            theme::corner(radius::L),
            if is_current {
                tk.accent_soft
            } else {
                tk.surface
            },
            Stroke::new(
                if is_current || hovered { 1.5 } else { 1.0 },
                if is_current {
                    tk.accent
                } else if hovered {
                    tk.border_strong
                } else {
                    tk.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        let sil = Rect::from_min_size(
            pos2(r.min.x + PAD, r.min.y + PAD),
            vec2(r.width() - 2.0 * PAD, SIL_H),
        );
        painter.rect_filled(sil, theme::corner(radius::M), tk.viewport_bg);
        paint_silhouette(painter, sil.shrink(6.0), *dt, tk);

        let g = &galleys[i];
        let mut y = sil.max.y + space::S;
        let x = r.min.x + PAD;
        painter.galley(pos2(x, y), g.name.clone(), tk.text);
        y += g.name.size().y + space::XS;
        painter.galley(pos2(x, y), g.desc.clone(), tk.text);
        y += g.desc.size().y + space::S;
        painter.galley(pos2(x, y), g.who_title.clone(), tk.accent);
        y += g.who_title.size().y + 2.0;
        painter.galley(pos2(x, y), g.who.clone(), tk.text_weak);

        // Alt satır: düğme (ya da "Geçerli" rozeti). Kart yüksekliği satıra eşitlenmiş olabilir.
        let btn_rect = Rect::from_min_size(
            pos2(x, r.max.y - PAD - BTN_H),
            vec2(r.width() - 2.0 * PAD, BTN_H),
        );
        let label = if is_current {
            t("type_cards.selected")
        } else {
            t("btn_select")
        };
        let btn = ui.put(btn_rect, egui::Button::new(label));
        if is_current {
            // Rozet düğmenin üstünde değil, silüet köşesinde.
            let mut badge_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                pos2(sil.min.x + 4.0, sil.min.y + 4.0),
                vec2(sil.width() - 8.0, 22.0),
            )));
            badge(&mut badge_ui, Status::Info, t("current_type"));
        }
        if btn.clicked() || resp.clicked() {
            chosen = Some(*dt);
        }
        let tip = format!("{} - {}", dt.label(), dt.description());
        resp.on_hover_text(tip);
    }
    ui.ctx().data_mut(|d| d.insert_temp(rects_id(), shown));
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::ThemeChoice;

    #[test]
    fn silhouettes_are_inside_unit_square_and_distinct() {
        let mut sigs = Vec::new();
        for dt in DroneType::ALL {
            let s = silhouette(dt);
            assert!(!s.polys.is_empty() || !s.circles.is_empty(), "{dt:?} boş");
            for p in s.polys.iter().flatten() {
                assert!(p[0].abs() <= 1.0 && p[1].abs() <= 1.0, "{dt:?} {p:?}");
            }
            for (c, r) in &s.circles {
                assert!(
                    c[0].abs() + r <= 1.0 + 1e-3 && c[1].abs() + r <= 1.0 + 1e-3,
                    "{dt:?} daire {c:?} r={r}"
                );
            }
            for (a, b) in &s.lines {
                for p in [a, b] {
                    assert!(p[0].abs() <= 1.0 && p[1].abs() <= 1.0, "{dt:?} {p:?}");
                }
            }
            sigs.push(format!("{:?}{:?}{:?}", s.polys, s.circles, s.lines));
        }
        for i in 0..sigs.len() {
            for j in i + 1..sigs.len() {
                assert_ne!(sigs[i], sigs[j], "{i} ve {j} aynı siluet");
            }
        }
        // Multirotor kanatsızdır ve dört pervane diski var; VTOL'da kanat + dört disk.
        assert_eq!(silhouette(DroneType::Multirotor).circles.len(), 4);
        assert_eq!(silhouette(DroneType::VtolQuadplane).circles.len(), 4);
        assert!(silhouette(DroneType::FlyingWing).circles.is_empty());
    }

    #[test]
    fn card_layout_fits_width_without_overlap() {
        for w in [180.0_f32, 300.0, 440.0, 640.0, 740.0, 1200.0] {
            let rows = 5;
            let rects = card_rects(w, 5, &vec![200.0; rows]);
            assert_eq!(rects.len(), 5);
            for (i, a) in rects.iter().enumerate() {
                assert!(a.min.x >= -1e-3 && a.max.x <= w + 1e-3, "w={w} {a:?}");
                for b in &rects[i + 1..] {
                    assert!(!a.shrink(0.5).intersects(*b), "w={w}: {a:?} {b:?}");
                }
            }
        }
        assert_eq!(columns_for(740.0, 5), 3);
        assert_eq!(columns_for(400.0, 5), 1);
        assert_eq!(columns_for(420.0, 5), 2);
        assert_eq!(columns_for(2000.0, 2), 2);
    }

    fn run_frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        current: Option<DroneType>,
        w: f32,
    ) -> Option<DroneType> {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(w + 40.0, 900.0)));
        input.events = events;
        let mut got = None;
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.set_max_width(w);
                if let Some(d) = type_card_grid(ui, current) {
                    got = Some(d);
                }
            });
        });
        out.textures_delta.clear();
        got
    }

    #[test]
    fn cards_render_in_both_themes_and_languages_and_stay_in_width() {
        for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
            for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
                for w in [360.0_f32, 520.0, 740.0] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    crate::i18n::with_lang(lang, || {
                        for cur in [None, Some(DroneType::Multirotor)] {
                            for _ in 0..2 {
                                assert!(run_frame(&ctx, vec![], cur, w).is_none());
                            }
                        }
                        let rects = last_card_rects(&ctx);
                        assert_eq!(rects.len(), DroneType::ALL.len());
                        for r in rects {
                            assert!(r.width() > 50.0 && r.height() > 150.0, "{r:?}");
                            assert!(r.max.x <= w + 40.0, "w={w} {r:?}");
                        }
                    });
                }
            }
        }
    }

    #[test]
    fn clicking_a_card_selects_that_type() {
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Light);
        run_frame(&ctx, vec![], None, 740.0);
        run_frame(&ctx, vec![], None, 740.0);
        let rects = last_card_rects(&ctx);
        // Dördüncü kartın silüet bölgesine (düğme dışı) tıkla.
        let r = rects[3];
        let pos = pos2(r.center().x, r.min.y + PAD + 20.0);
        let ev = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run_frame(&ctx, vec![egui::Event::PointerMoved(pos)], None, 740.0);
        run_frame(&ctx, vec![ev(true)], None, 740.0);
        let got = run_frame(&ctx, vec![ev(false)], None, 740.0);
        assert_eq!(got, Some(DroneType::ALL[3]));
    }
}
