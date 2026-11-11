// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sonuç kartı: başlık, büyük değer + birim, ayrıntı satırı, sol kenarda durum şeridi ve
//! isteğe bağlı "Neden?" bağlantısı.
use super::Status;
use crate::ui::theme::{self, font, radius, space};
use egui::{Color32, Response, RichText, Sense, Ui};

#[derive(Clone, Debug)]
pub struct MetricCard<'a> {
    pub title: &'a str,
    pub value: String,
    pub unit: &'a str,
    pub detail: String,
    pub status: Status,
    /// "Neden?" bağlantısı gösterilsin mi?
    pub why: bool,
    /// Kartın üzerine gelince gösterilecek açıklama (boşsa yok).
    pub tooltip: &'a str,
    /// Kartın tamamı tıklanabilir mi? (içteki "Neden?" bağlantısı önceliklidir)
    pub clickable: bool,
}

/// Kart çizim sonucu.
pub struct CardOutput {
    pub response: Response,
    pub why_clicked: bool,
}

/// Kullanılabilir genişliğe kaç kart sütunu sığar (en az 1, en çok `max_cols`).
pub fn card_columns(avail_width: f32, min_card_width: f32, gap: f32, max_cols: usize) -> usize {
    if !avail_width.is_finite() || !min_card_width.is_finite() || min_card_width <= 0.0 {
        return 1;
    }
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let n = ((avail_width + gap) / (min_card_width + gap)).floor();
    (n.max(1.0) as usize).clamp(1, max_cols.max(1))
}

/// Sütun sayısı verildiğinde tek kartın genişliği.
pub fn card_width(avail_width: f32, cols: usize, gap: f32) -> f32 {
    let cols = cols.max(1) as f32;
    ((avail_width - gap * (cols - 1.0)) / cols).max(40.0)
}

pub fn metric_card(ui: &mut Ui, card: &MetricCard, width: f32) -> CardOutput {
    let tk = theme::tokens(ui);
    let stripe = match card.status {
        Status::Neutral => tk.border_strong,
        s => theme::status_color(tk, s),
    };
    let mut why_clicked = false;
    let sense = if card.clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let scoped = ui.scope_builder(egui::UiBuilder::new().sense(sense), |ui| {
        egui::Frame::new()
            .fill(tk.surface)
            .stroke(egui::Stroke::new(1.0, tk.border))
            .corner_radius(theme::corner(radius::M))
            .inner_margin(egui::Margin {
                left: 12,
                right: 10,
                top: 8,
                bottom: 8,
            })
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    let w = (width - 24.0).max(40.0);
                    ui.set_width(w);
                    ui.set_max_width(w);
                    // Başlık satırı: durum noktası + (kısaltılan) başlık solda, "Neden?" bağlantısı sağda.
                    // Elle yerleştirilir ki hiçbir içerik kart genişliğini aşıp paneli büyütmesin.
                    let title_font = egui::FontId::proportional(font::CAPTION);
                    let row_h = ui.fonts_mut(|f| f.row_height(&title_font)).max(14.0);
                    let (row, _) = ui.allocate_exact_size(egui::vec2(w, row_h), Sense::hover());
                    let mut left = row.left();
                    let mut right = row.right();
                    if card.why {
                        let galley = ui.painter().layout_no_wrap(
                            crate::i18n::t("ui.why").to_string(),
                            title_font.clone(),
                            tk.accent,
                        );
                        let gw = galley.size().x.min(w * 0.5);
                        let link_rect = egui::Rect::from_min_size(
                            egui::pos2(right - gw, row.top()),
                            egui::vec2(gw, row_h),
                        );
                        let link = ui
                            .interact(link_rect, ui.auto_id_with("why"), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(crate::i18n::t("ui.why_tip"));
                        ui.painter().with_clip_rect(link_rect).galley(
                            egui::pos2(link_rect.left(), row.center().y - galley.size().y * 0.5),
                            galley,
                            tk.accent,
                        );
                        if link.clicked() {
                            why_clicked = true;
                        }
                        right = link_rect.left() - 6.0;
                    }
                    if card.status != Status::Neutral {
                        let c = egui::pos2(left + 4.0, row.center().y);
                        ui.painter().circle_filled(c, 3.5, stripe);
                        let dot = egui::Rect::from_center_size(c, egui::vec2(8.0, 8.0));
                        ui.interact(dot, ui.auto_id_with("dot"), Sense::hover())
                            .on_hover_text(card.status.label());
                        left += 12.0;
                    }
                    let title_w = (right - left).max(8.0);
                    let mut job = egui::text::LayoutJob::single_section(
                        card.title.to_string(),
                        egui::TextFormat::simple(title_font, tk.text_weak),
                    );
                    job.wrap.max_width = title_w;
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    let galley = ui.painter().layout_job(job);
                    let title_rect = egui::Rect::from_min_size(
                        egui::pos2(left, row.center().y - galley.size().y * 0.5),
                        galley.size(),
                    );
                    ui.painter().galley(title_rect.min, galley, tk.text_weak);
                    // Değer + birim tek satırda; sığmazsa "…" ile kısalır.
                    let color = match card.status {
                        Status::Err => tk.danger,
                        Status::Warn => tk.warning,
                        _ => tk.text,
                    };
                    let mut vjob = egui::text::LayoutJob::default();
                    vjob.append(
                        &card.value,
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::proportional(font::METRIC),
                            color,
                            valign: egui::Align::BOTTOM,
                            ..Default::default()
                        },
                    );
                    if !card.unit.is_empty() {
                        vjob.append(
                            card.unit,
                            space::XS,
                            egui::TextFormat {
                                font_id: egui::FontId::proportional(font::BODY),
                                color: tk.text_weak,
                                valign: egui::Align::BOTTOM,
                                ..Default::default()
                            },
                        );
                    }
                    vjob.wrap.max_width = w;
                    vjob.wrap.max_rows = 1;
                    vjob.wrap.break_anywhere = true;
                    ui.add(egui::Label::new(vjob).truncate());
                    if !card.detail.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&card.detail)
                                    .size(font::CAPTION)
                                    .color(tk.text_weak),
                            )
                            .truncate(),
                        );
                    }
                });
            })
    });
    // Sol kenar durum şeridi
    let r = scoped.inner.response.rect;
    let stripe_rect = egui::Rect::from_min_max(
        r.left_top() + egui::vec2(0.0, 6.0),
        egui::pos2(r.left() + 3.0, r.bottom() - 6.0),
    );
    if card.status != Status::Neutral {
        ui.painter()
            .rect_filled(stripe_rect, theme::corner(2), stripe);
    }
    let mut response = scoped.response;
    if card.clickable {
        if response.hovered() {
            ui.painter().rect_stroke(
                r,
                theme::corner(radius::M),
                egui::Stroke::new(1.0, tk.border_strong),
                egui::StrokeKind::Inside,
            );
        }
        response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    if !card.tooltip.is_empty() {
        response = response.on_hover_text(card.tooltip);
    }
    CardOutput {
        response,
        why_clicked,
    }
}

/// Kartları sütunlara yerleştirir; `f` her kart için çağrılır ve kartın genişliğini alır.
pub fn card_grid(
    ui: &mut Ui,
    count: usize,
    min_card_width: f32,
    mut f: impl FnMut(&mut Ui, usize, f32),
) {
    let gap = space::S;
    let avail = ui.available_width();
    let cols = card_columns(avail, min_card_width, gap, 4);
    let w = card_width(avail, cols, gap);
    let mut i = 0;
    while i < count {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for _ in 0..cols {
                if i >= count {
                    break;
                }
                f(ui, i, w);
                i += 1;
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
}

/// Kart değerinin metin rengi için durum yardımcı fonksiyonu (diğer ekranlar için).
pub fn value_color(tk: &theme::Tokens, s: Status) -> Color32 {
    match s {
        Status::Err | Status::Warn => theme::status_color(tk, s),
        _ => tk.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_columns_fit_width() {
        assert_eq!(card_columns(300.0, 140.0, 8.0, 4), 2);
        assert_eq!(card_columns(287.0, 140.0, 8.0, 4), 1);
        assert_eq!(card_columns(288.0, 140.0, 8.0, 4), 2);
        assert_eq!(card_columns(2000.0, 140.0, 8.0, 4), 4);
        assert_eq!(card_columns(50.0, 140.0, 8.0, 4), 1);
        assert_eq!(card_columns(f32::NAN, 140.0, 8.0, 4), 1);
        assert_eq!(card_columns(300.0, 0.0, 8.0, 4), 1);
    }

    #[test]
    fn card_width_fills_row() {
        let w = card_width(300.0, 2, 8.0);
        assert!((w * 2.0 + 8.0 - 300.0).abs() < 1e-3);
        assert!(card_width(10.0, 3, 8.0) >= 40.0);
    }
}
