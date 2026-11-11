// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yeni düzene taşınan adım ekranlarının ortak yardımcıları.
//!
//! Her adım ekranı üç parçaya ayrılır:
//! - `inputs(..)`: sol paneldeki girdiler (dar sütun; `number_row`, tam genişlikte seçim kutuları),
//! - `results(..)`: sağ paneldeki sonuç kartları, tablolar (katlanabilir) ve notlar,
//! - `chart(..)` (varsa): merkez görünümün "Grafik" seçeneği.
//!
//! Buradaki yardımcılar yalnız çizim yapar; hesap motoruna dokunmaz. Renkler `theme` tokenlarından
//! gelir, sabit renk yoktur.
use super::components::{self, MetricCard, Status, card::card_grid};
use super::theme::{self, font, radius, space};
use egui::{RichText, Ui};

/// Sonuç kartı kurucusu: başlık, değer, birim ve açıklama (ipucu).
pub fn card<'a>(title: &'a str, value: String, unit: &'a str, tip: &'a str) -> MetricCard<'a> {
    MetricCard {
        title,
        value,
        unit,
        detail: String::new(),
        status: Status::Neutral,
        why: false,
        tooltip: tip,
        clickable: false,
    }
}

/// `MetricCard` için kısa zincirleme ayarlar.
pub trait CardExt {
    fn d(self, detail: impl Into<String>) -> Self;
    fn s(self, status: Status) -> Self;
}

impl CardExt for MetricCard<'_> {
    fn d(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }
    fn s(mut self, status: Status) -> Self {
        self.status = status;
        self
    }
}

/// Kart ızgarası (sağ panelde 1–2 sütun).
pub fn cards(ui: &mut Ui, list: &[MetricCard]) {
    card_grid(ui, list.len(), 140.0, |ui, i, w| {
        components::metric_card(ui, &list[i], w);
    });
}

/// `true` → Tamam, `false` → Hata (eski `ok_color` eşleniği).
pub fn ok(b: bool) -> Status {
    Status::from_ok(b)
}

/// Üç kademeli eşik: `x >= good` Tamam, `x >= warn` Uyarı, aksi Hata (eski yeşil/amber/kırmızı).
/// Sonlu olmayan değer Hata sayılır (eski ekranlarda da kırmızıydı).
pub fn tri(x: f64, good: f64, warn: f64) -> Status {
    if !x.is_finite() {
        Status::Err
    } else if x >= good {
        Status::Ok
    } else if x >= warn {
        Status::Warn
    } else {
        Status::Err
    }
}

/// Üç kademeli eşik (küçük daha iyi): `x <= good` Tamam, `x <= warn` Uyarı, aksi Hata.
pub fn tri_low(x: f64, good: f64, warn: f64) -> Status {
    if !x.is_finite() {
        Status::Err
    } else if x <= good {
        Status::Ok
    } else if x <= warn {
        Status::Warn
    } else {
        Status::Err
    }
}

/// Adım tanıtımı (sol panelin başında, katlanabilir; kullanıcı kapatırsa egui hatırlar).
pub fn intro(ui: &mut Ui, title: &str, what_why: &str, help: Option<&str>) {
    let tk = theme::tokens(ui);
    egui::CollapsingHeader::new(
        RichText::new(crate::i18n::t("ui.step.about"))
            .size(font::CAPTION)
            .strong()
            .color(tk.text_weak),
    )
    .id_salt(("step_about", title))
    .default_open(true)
    .show(ui, |ui| {
        // `widgets::step_intro` ile aynı içerik; başlık da sarılır (dar sol panelde taşmaz).
        egui::Frame::new()
            .fill(tk.info_soft)
            .stroke(egui::Stroke::new(1.0, tk.border))
            .corner_radius(theme::corner(radius::M))
            .inner_margin(theme::margin(space::S))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::i18n::tf("wizard.step_goal_header", &[title]))
                            .strong()
                            .color(tk.accent),
                    )
                    .wrap(),
                );
                ui.add_space(space::XS);
                ui.add(egui::Label::new(RichText::new(what_why).color(tk.text)).wrap());
                if let Some(help_text) = help {
                    egui::CollapsingHeader::new(
                        RichText::new(crate::i18n::t("guide.detailed_rules_header"))
                            .small()
                            .color(tk.accent),
                    )
                    .id_salt(("step_help", title))
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in help_text.lines() {
                            if let Some(h) = line.strip_prefix("# ") {
                                ui.add_space(space::XS);
                                ui.add(
                                    egui::Label::new(RichText::new(h).strong().color(tk.text))
                                        .wrap(),
                                );
                            } else if line.trim().is_empty() {
                                ui.add_space(2.0);
                            } else {
                                note(ui, line);
                            }
                        }
                    });
                }
            });
    });
}

/// Katlanabilir başlık metni: egui başlık metnini hiç sarmaz; bu yardımcı metni panel
/// genişliğine göre tek satıra kısaltılmış ("…") hazır bir galeriye çevirir (taşma olmaz).
pub fn short_header(ui: &Ui, text: &str, color: egui::Color32, strong: bool) -> egui::WidgetText {
    let w = (ui.available_width() - ui.spacing().indent - 8.0).max(30.0);
    let font = egui::FontId::proportional(if strong { font::BODY } else { font::BODY - 0.5 });
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap.max_width = w;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    egui::WidgetText::Galley(galley)
}

/// Soluk, sarılan küçük not.
pub fn note(ui: &mut Ui, text: &str) {
    let tk = theme::tokens(ui);
    ui.add(egui::Label::new(RichText::new(text).size(font::CAPTION).color(tk.text_weak)).wrap());
}

/// Sarılan normal metin satırı.
pub fn text(ui: &mut Ui, text: &str) {
    let tk = theme::tokens(ui);
    ui.add(egui::Label::new(RichText::new(text).color(tk.text)).wrap());
}

/// Sarılan, durum renginde metin (eski `colored_label(RED/AMBER/GREEN, ..)` eşleniği).
pub fn status_text(ui: &mut Ui, status: Status, text: &str) {
    let tk = theme::tokens(ui);
    let c = match status {
        Status::Neutral => tk.text,
        s => theme::status_color(tk, s),
    };
    ui.add(egui::Label::new(RichText::new(text).color(c)).wrap());
}

/// Etiket üstte, denetim altta tam genişlikte (dar sol panelde yatay taşmayı önler).
pub fn field_label(ui: &mut Ui, label: &str, help: &str) {
    let tk = theme::tokens(ui);
    ui.horizontal(|ui| {
        let w = (ui.available_width() - if help.is_empty() { 0.0 } else { 22.0 }).max(20.0);
        ui.add_sized(
            egui::vec2(w, ui.spacing().interact_size.y * 0.8),
            egui::Label::new(RichText::new(label).color(tk.text_weak))
                .truncate()
                .halign(egui::Align::LEFT),
        );
        if !help.is_empty() {
            components::input::help_mark(ui, help);
        }
    });
}

/// Tam genişlikte açılır seçim kutusu. `items`: (değer, etiket). Değişirse `true`.
pub fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    help: &str,
    cur: &mut T,
    items: &[(T, &str)],
) -> bool {
    field_label(ui, label, help);
    let sel = items
        .iter()
        .find(|(v, _)| v == cur)
        .map(|(_, l)| *l)
        .unwrap_or("");
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(sel)
        .width((ui.available_width() - 8.0).max(40.0))
        .truncate()
        .show_ui(ui, |ui| {
            for (v, l) in items {
                if ui.selectable_value(cur, *v, *l).changed() {
                    changed = true;
                }
            }
        });
    ui.add_space(2.0);
    changed
}

/// Sonuç tablosunun bir satırı (dar panelde "sütun: değer" çiftleri olarak çizilir).
pub struct TableRow {
    pub name: String,
    pub cells: Vec<String>,
    /// Satır vurgusu (ör. kritik kısıt, uyarılı CL).
    pub status: Status,
}

/// Geniş tabloyu dar panele sığdırır: her satır küçük bir kutu, içinde "sütun: değer" ızgarası.
/// Tamamı katlanabilir başlık altındadır (varsayılan kapalı).
pub fn row_table(
    ui: &mut Ui,
    id: &str,
    title: &str,
    headers: &[&str],
    rows: &[TableRow],
    footer: Option<&str>,
) {
    let tk = theme::tokens(ui);
    egui::CollapsingHeader::new(short_header(ui, title, tk.text, true))
        .id_salt(("row_table", id))
        .default_open(false)
        .show(ui, |ui| {
            for (ri, r) in rows.iter().enumerate() {
                let stroke = match r.status {
                    Status::Neutral => tk.border,
                    s => theme::status_color(tk, s),
                };
                egui::Frame::new()
                    .fill(tk.surface_alt)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(theme::corner(radius::S))
                    .inner_margin(theme::margin(space::S - 2.0))
                    .show(ui, |ui| {
                        let w = ui.available_width();
                        ui.set_width(w);
                        ui.add(
                            egui::Label::new(RichText::new(&r.name).strong().color(tk.text))
                                .truncate(),
                        );
                        kv_grid(ui, (id, ri), headers, &r.cells, w);
                    });
                ui.add_space(space::XS);
            }
            if let Some(f) = footer {
                note(ui, f);
            }
        });
}

/// İki sütunlu "etiket: değer" ızgarası (etiketler kısaltılır, satır genişliği aşılmaz).
fn kv_grid(ui: &mut Ui, id: impl std::hash::Hash, labels: &[&str], values: &[String], w: f32) {
    let tk = theme::tokens(ui);
    let pairs = labels.len().min(values.len());
    // İki çift yan yana (geniş panel) ya da tek sütun.
    let per_row = if w >= 300.0 { 2 } else { 1 };
    let col_w = ((w - space::S * (per_row as f32 - 1.0)) / per_row as f32).max(40.0);
    let _ = id;
    let mut i = 0;
    while i < pairs {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = space::S;
            for _ in 0..per_row {
                if i >= pairs {
                    break;
                }
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    labels[i],
                    0.0,
                    egui::TextFormat::simple(
                        egui::FontId::proportional(font::CAPTION),
                        tk.text_weak,
                    ),
                );
                job.append(
                    &values[i],
                    space::XS,
                    egui::TextFormat::simple(egui::FontId::proportional(font::BODY), tk.text),
                );
                job.wrap.max_width = col_w;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                ui.add_sized(
                    egui::vec2(col_w, ui.spacing().interact_size.y * 0.85),
                    egui::Label::new(job).truncate().halign(egui::Align::LEFT),
                );
                i += 1;
            }
        });
    }
}

/// Basit moddaki sonuç satırı (renkli çerçeve + metin + iyileştirme önerisi), panel genişliğinde.
pub fn rating_status(r: crate::calc::presets::Rating) -> Status {
    use crate::calc::presets::Rating;
    match r {
        Rating::Good => Status::Ok,
        Rating::Ok => Status::Warn,
        Rating::Bad => Status::Err,
    }
}

/// Katlanabilir bilgi kutusu (ör. "Neden?" açıklamaları).
pub fn info_box(ui: &mut Ui, id: &str, title: &str, lines: &[&str]) {
    let tk = theme::tokens(ui);
    egui::CollapsingHeader::new(short_header(ui, title, tk.accent, false))
        .id_salt(("info_box", id))
        .default_open(false)
        .show(ui, |ui| {
            for l in lines {
                note(ui, l);
            }
        });
}

/// Çok satırlı formül/hesap metni (sarılır, eş aralıklı yazı).
pub fn formula_box(ui: &mut Ui, id: &str, title: &str, body: &str) {
    let tk = theme::tokens(ui);
    egui::CollapsingHeader::new(short_header(ui, title, tk.accent, false))
        .id_salt(("formula_box", id))
        .default_open(false)
        .show(ui, |ui| {
            for line in body.lines() {
                ui.add(
                    egui::Label::new(
                        RichText::new(line)
                            .monospace()
                            .size(font::CAPTION)
                            .color(tk.text),
                    )
                    .wrap(),
                );
            }
        });
}

/// İçeriği bugünkü genişliğe sabitler: çocuk alanda çizer, ebeveynde yalnız (genişlik × kullanılan
/// yükseklik) kadar yer ayırır. Böylece alt bileşenlerdeki yarım piksellik taşmalar (ör. başka
/// paketin formül başlıkları) paneli kareler boyunca büyütemez; taşan kısım kırpılır.
pub fn fixed_width<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    let w = ui.available_width().max(1.0);
    let top_left = ui.cursor().min;
    // Sonlu bir yükseklik: sonsuz yükseklikli çocuk alanda bazı bileşenler genişliği yanlış ölçüyor.
    let max_rect = egui::Rect::from_min_size(top_left, egui::vec2(w, 1.0e6));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(max_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(egui::Rect::from_min_max(
        egui::pos2(top_left.x, ui.clip_rect().top()),
        egui::pos2(top_left.x + w, ui.clip_rect().bottom()),
    )));
    child.set_max_width(w);
    let r = f(&mut child);
    let h = child.min_rect().height().max(0.0);
    ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    r
}

/// Geçersiz girdide sonuç yerine gösterilen kısa not.
pub fn missing(ui: &mut Ui) {
    note(ui, "—");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_match_legacy_colors() {
        // Eski: ms >= 0.2 yeşil, >= 0 amber, aksi kırmızı.
        assert_eq!(tri(0.25, 0.2, 0.0), Status::Ok);
        assert_eq!(tri(0.1, 0.2, 0.0), Status::Warn);
        assert_eq!(tri(-0.1, 0.2, 0.0), Status::Err);
        assert_eq!(tri(f64::NAN, 0.2, 0.0), Status::Err);
        // Eski: askı gazı <= 0.50 yeşil, <= 0.60 amber, aksi kırmızı.
        assert_eq!(tri_low(0.45, 0.5, 0.6), Status::Ok);
        assert_eq!(tri_low(0.55, 0.5, 0.6), Status::Warn);
        assert_eq!(tri_low(0.7, 0.5, 0.6), Status::Err);
        assert_eq!(ok(true), Status::Ok);
        assert_eq!(ok(false), Status::Err);
    }
}
