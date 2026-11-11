// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ortak arayüz bileşenleri (yeni üç panelli düzen için). Renkler yalnız `theme` tokenlarından
//! gelir; düzen hesapları saf fonksiyonlardadır ve birim testlidir.
//!
//! - [`card`]: sonuç kartı ve kart ızgarası
//! - [`section`]: bölüm başlığı
//! - [`alert`]: durum rozeti, uyarı şeridi, uyarı satırı
//! - [`input`]: girdi satırı (etiket + değer + birim + yardım)
//! - [`step`]: adım listesi öğesi
//! - [`empty`]: boş durum
//! - [`format`]: saf biçimlendirme yardımcıları
pub mod alert;
pub mod card;
pub mod empty;
pub mod format;
pub mod input;
pub mod section;
pub mod step;
pub mod toggle;

pub use alert::{badge, banner, warning_row};
pub use card::{MetricCard, card_columns, metric_card};
pub use empty::empty_state;
pub use input::{NumberInput, number_row, u32_row};
pub use section::section_header;
pub use step::{StepVisual, step_item, step_item_with_tip};
pub use toggle::{primary_button, segmented, toggle_chip, tool_button};

use crate::calc::Sev;

/// Anlamsal durum: kart şeridi, rozet ve uyarı rengi bundan türetilir.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Status {
    #[default]
    Neutral,
    Ok,
    Info,
    Warn,
    Err,
}

impl Status {
    pub fn from_sev(s: Sev) -> Status {
        match s {
            Sev::Err => Status::Err,
            Sev::Warn => Status::Warn,
            Sev::Info => Status::Info,
        }
    }

    pub fn from_ok(ok: bool) -> Status {
        if ok { Status::Ok } else { Status::Err }
    }

    /// Önem sırası (büyük = daha ciddi).
    pub fn rank(self) -> u8 {
        match self {
            Status::Neutral => 0,
            Status::Ok => 1,
            Status::Info => 2,
            Status::Warn => 3,
            Status::Err => 4,
        }
    }

    /// İki durumdan daha ciddi olanı.
    pub fn worst(self, other: Status) -> Status {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Kısa durum etiketi (ekran okuyucu ve rozet metni için).
    pub fn label(self) -> &'static str {
        match self {
            Status::Neutral => "",
            Status::Ok => crate::i18n::t("ui.status.ok"),
            Status::Info => crate::i18n::t("ui.status.info"),
            Status::Warn => crate::i18n::t("ui.status.warn"),
            Status::Err => crate::i18n::t("ui.status.err"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_worst_and_rank() {
        assert_eq!(Status::Ok.worst(Status::Warn), Status::Warn);
        assert_eq!(Status::Err.worst(Status::Warn), Status::Err);
        assert_eq!(Status::Neutral.worst(Status::Neutral), Status::Neutral);
        assert_eq!(Status::from_sev(Sev::Err), Status::Err);
        assert_eq!(Status::from_sev(Sev::Info), Status::Info);
        assert_eq!(Status::from_ok(true), Status::Ok);
        assert_eq!(Status::from_ok(false), Status::Err);
        assert!(Status::Err.rank() > Status::Warn.rank());
    }

    /// Bileşenlerin hepsi başsız bir karede iki temada da paniksiz çizilir.
    #[test]
    fn components_render_headless_both_themes() {
        for choice in [
            crate::ui::theme::ThemeChoice::Dark,
            crate::ui::theme::ThemeChoice::Light,
        ] {
            let ctx = egui::Context::default();
            crate::ui::theme::install(&ctx, choice);
            let mut v = 1.25_f64;
            let mut n = 4_u32;
            let mut nan = f64::NAN;
            for _ in 0..2 {
                let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        section_header(ui, "Başlık");
                        let c = MetricCard {
                            title: "Kütle",
                            value: "912".into(),
                            unit: "g",
                            detail: "W = 8.9 N".into(),
                            status: Status::Warn,
                            why: true,
                            tooltip: "",
                            clickable: true,
                        };
                        let _ = metric_card(ui, &c, 160.0);
                        badge(ui, Status::Err, "HATA");
                        banner(ui, Status::Info, "Bilgi şeridi uzun metin ile sarılır.");
                        number_row(
                            ui,
                            NumberInput::new("Açıklık", &mut v)
                                .unit("m")
                                .range(0.1, 10.0)
                                .help("yardım"),
                        );
                        number_row(ui, NumberInput::new("NaN", &mut nan));
                        u32_row(ui, "Hücre", &mut n, 1, 14, "");
                        for (i, vis) in [
                            StepVisual::Done,
                            StepVisual::Current,
                            StepVisual::NeedsUpdate,
                            StepVisual::Available,
                            StepVisual::Locked,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            step_item(ui, i + 1, "Adım", vis, i == 1, false);
                            step_item(ui, i + 1, "Adım", vis, false, true);
                        }
                        empty_state(ui, "Boş", "Henüz bir şey yok.");
                    });
                });
                out.textures_delta.clear();
            }
        }
    }
}
