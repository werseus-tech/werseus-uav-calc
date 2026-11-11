// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım listesi öğesi: numara/durum simgesi + başlık. Simgeler yazı tipine bağımlı olmasın diye
//! çizilir (onay işareti, kilit, ünlem).
use crate::ui::theme::{self, font, radius};
use egui::{Color32, Pos2, Response, Sense, Stroke, Ui, vec2};

/// Adımın görsel durumu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepVisual {
    /// Tamamlandı (onay işareti).
    Done,
    /// Şu an açık olan adım.
    Current,
    /// Önceki bir değişiklik nedeniyle gözden geçirilmeli.
    NeedsUpdate,
    /// Gidilebilir, henüz ziyaret edilmedi.
    Available,
    /// Önceki adımlar geçersiz: kilitli.
    Locked,
}

impl StepVisual {
    pub fn clickable(self) -> bool {
        !matches!(self, StepVisual::Locked)
    }
}

/// Satır yüksekliği (kompakt: yalnız simge).
pub fn step_item_height(compact: bool) -> f32 {
    if compact { 34.0 } else { 30.0 }
}

pub fn step_item(
    ui: &mut Ui,
    number: usize,
    title: &str,
    visual: StepVisual,
    selected: bool,
    compact: bool,
) -> Response {
    step_item_with_tip(ui, number, title, visual, selected, compact, "")
}

/// `step_item` + ek ipucu. Ad sığmayıp "…" ile kısalırsa ipucunun başına tam ad eklenir.
pub fn step_item_with_tip(
    ui: &mut Ui,
    number: usize,
    title: &str,
    visual: StepVisual,
    selected: bool,
    compact: bool,
    tip: &str,
) -> Response {
    let tk = theme::tokens(ui);
    let width = if compact { 40.0 } else { ui.available_width() };
    let h = step_item_height(compact);
    let sense = if visual.clickable() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), sense);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let mut elided = false;
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, theme::corner(radius::M), tk.accent_soft);
        p.rect_filled(
            egui::Rect::from_min_max(
                rect.left_top() + vec2(0.0, 5.0),
                egui::pos2(rect.left() + 3.0, rect.bottom() - 5.0),
            ),
            theme::corner(2),
            tk.accent,
        );
    } else if resp.hovered() && visual.clickable() {
        p.rect_filled(rect, theme::corner(radius::M), tk.surface_alt);
    }

    let c = egui::pos2(
        if compact {
            rect.center().x
        } else {
            rect.left() + 20.0
        },
        rect.center().y,
    );
    let r = 10.0;
    let (fill, ring, fg) = match visual {
        StepVisual::Done => (tk.success, tk.success, tk.surface),
        StepVisual::Current => (tk.accent_fill, tk.accent, tk.on_accent),
        StepVisual::NeedsUpdate => (tk.warning_soft, tk.warning, tk.warning),
        StepVisual::Available => (tk.surface, tk.border_strong, tk.text_weak),
        StepVisual::Locked => (tk.surface_alt, tk.border, tk.text_weak),
    };
    p.circle_filled(c, r, fill);
    p.circle_stroke(c, r, Stroke::new(1.0, ring));
    match visual {
        StepVisual::Done => draw_check(p, c, fg),
        StepVisual::Locked => draw_lock(p, c, fg),
        StepVisual::NeedsUpdate => draw_bang(p, c, fg),
        _ => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                number.to_string(),
                egui::FontId::proportional(font::CAPTION),
                fg,
            );
        }
    }

    if !compact {
        let color = match visual {
            StepVisual::Locked => tk.text_weak,
            _ if selected => tk.text,
            _ => tk.text,
        };
        let max_w = (rect.right() - (c.x + r + 10.0) - 4.0).max(8.0);
        let mut job = egui::text::LayoutJob::single_section(
            title.to_string(),
            egui::TextFormat::simple(egui::FontId::proportional(font::BODY), color),
        );
        job.wrap.max_width = max_w;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = p.layout_job(job);
        elided = galley.elided;
        let pos = egui::pos2(c.x + r + 10.0, rect.center().y - galley.size().y * 0.5);
        p.galley(pos, galley, color);
    }
    if compact {
        let t = format!("{number}. {title}");
        if tip.is_empty() {
            resp.on_hover_text(t)
        } else {
            resp.on_hover_text(format!(
                "{t}

{tip}"
            ))
        }
    } else if elided {
        if tip.is_empty() {
            resp.on_hover_text(title.to_string())
        } else {
            resp.on_hover_text(format!(
                "{title}

{tip}"
            ))
        }
    } else if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip.to_string())
    }
}

fn draw_check(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(2.0, col);
    p.line_segment([c + vec2(-4.5, 0.0), c + vec2(-1.5, 3.5)], s);
    p.line_segment([c + vec2(-1.5, 3.5), c + vec2(4.5, -3.5)], s);
}

fn draw_lock(p: &egui::Painter, c: Pos2, col: Color32) {
    let body = egui::Rect::from_center_size(c + vec2(0.0, 2.0), vec2(8.0, 6.0));
    p.rect_filled(body, theme::corner(1), col);
    p.circle_stroke(c + vec2(0.0, -1.5), 2.6, Stroke::new(1.4, col));
}

fn draw_bang(p: &egui::Painter, c: Pos2, col: Color32) {
    p.line_segment(
        [c + vec2(0.0, -5.0), c + vec2(0.0, 1.5)],
        Stroke::new(2.0, col),
    );
    p.circle_filled(c + vec2(0.0, 4.5), 1.3, col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_steps_are_not_clickable() {
        assert!(!StepVisual::Locked.clickable());
        assert!(StepVisual::Done.clickable());
        assert!(StepVisual::NeedsUpdate.clickable());
        assert!(step_item_height(true) >= 30.0);
    }
}
