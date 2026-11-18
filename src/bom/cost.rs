// SPDX-License-Identifier: AGPL-3.0-or-later
//! Maliyetli malzeme listesi (BOM): yerleşik örnek fiyat listesi, satır başına kullanıcı
//! geçersiz kılması, toplamlar ve CSV. Ağ erişimi yoktur; fiyatlar yalnızca ÖRNEK değerlerdir.

use std::collections::HashMap;

use crate::cad::outputs::csv_cell;
use crate::i18n::t;

/// Örnek fiyat listesinin para birimi.
pub const CURRENCY: &str = "USD";

/// Fiyatın neye göre uygulandığı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceBasis {
    /// Adet başına.
    PerPiece,
    /// Kilogram başına (satır toplamı = adet × birim kütle × fiyat).
    PerKg,
}

/// Bir malzeme listesi satırı (fiyatsız).
#[derive(Clone, Debug, PartialEq)]
pub struct BomLine {
    /// Kararlı anahtar (fiyat listesi ve kullanıcı geçersiz kılması bununla eşlenir).
    pub key: &'static str,
    /// Görünen ad (geçerli dilde).
    pub name: String,
    pub qty: u32,
    /// Birim kütle [g].
    pub unit_mass_g: f64,
}

impl BomLine {
    pub fn total_mass_g(&self) -> f64 {
        self.unit_mass_g * self.qty as f64
    }
}

/// Yerleşik örnek fiyat listesi: (anahtar, fiyat [USD], taban). Gerçek fiyat değildir.
pub const PRICE_LIST: &[(&str, f64, PriceBasis)] = &[
    ("cells", 4.0, PriceBasis::PerPiece),
    ("pack_extra", 8.0, PriceBasis::PerPiece),
    ("propulsion", 70.0, PriceBasis::PerPiece),
    ("avionics", 80.0, PriceBasis::PerPiece),
    ("payload", 40.0, PriceBasis::PerPiece),
    ("wing_shell", 35.0, PriceBasis::PerKg),
    ("spar", 90.0, PriceBasis::PerKg),
    ("fuse_tail", 40.0, PriceBasis::PerKg),
    ("mr_frame", 120.0, PriceBasis::PerKg),
    ("mr_motor", 20.0, PriceBasis::PerPiece),
    ("mr_esc", 40.0, PriceBasis::PerPiece),
    ("mr_fc", 35.0, PriceBasis::PerPiece),
    ("mr_misc", 12.0, PriceBasis::PerPiece),
    ("fasteners", 0.15, PriceBasis::PerPiece),
];

/// Anahtarın örnek fiyatı ve tabanı (listede yoksa 0 / adet).
pub fn example_price(key: &str) -> (f64, PriceBasis) {
    PRICE_LIST
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, p, b)| (*p, *b))
        .unwrap_or((0.0, PriceBasis::PerPiece))
}

/// Fiyatlanmış satır.
#[derive(Clone, Debug, PartialEq)]
pub struct CostedLine {
    pub line: BomLine,
    pub basis: PriceBasis,
    /// Uygulanan birim fiyat (taban birimi başına).
    pub unit_price: f64,
    /// Kullanıcı geçersiz kıldı mı?
    pub overridden: bool,
    pub total_mass_g: f64,
    pub line_total: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CostedBom {
    pub lines: Vec<CostedLine>,
    pub total_mass_g: f64,
    pub grand_total: f64,
}

/// Satır toplamı: adet × fiyat ya da adet × birim kütle[kg] × fiyat.
pub fn line_total(line: &BomLine, basis: PriceBasis, unit_price: f64) -> f64 {
    match basis {
        PriceBasis::PerPiece => line.qty as f64 * unit_price,
        PriceBasis::PerKg => line.total_mass_g() / 1000.0 * unit_price,
    }
}

/// Satırları fiyatlandırır. Geçersiz (sonlu olmayan ya da negatif) geçersiz kılmalar yok sayılır.
pub fn cost(lines: &[BomLine], overrides: &HashMap<String, f64>) -> CostedBom {
    let mut out = CostedBom::default();
    for l in lines {
        let (ex, basis) = example_price(l.key);
        let ov = overrides
            .get(l.key)
            .copied()
            .filter(|v| v.is_finite() && *v >= 0.0);
        let unit_price = ov.unwrap_or(ex);
        let total = line_total(l, basis, unit_price);
        out.total_mass_g += l.total_mass_g();
        out.grand_total += total;
        out.lines.push(CostedLine {
            line: l.clone(),
            basis,
            unit_price,
            overridden: ov.is_some(),
            total_mass_g: l.total_mass_g(),
            line_total: total,
        });
    }
    out
}

/// Fiyat tabanının görünen adı.
pub fn basis_label(b: PriceBasis) -> &'static str {
    match b {
        PriceBasis::PerPiece => t("bom.basis.piece"),
        PriceBasis::PerKg => t("bom.basis.kg"),
    }
}

/// CSV: sütunlar parça, adet, birim kütle, toplam kütle, fiyat tabanı, birim fiyat, satır toplamı,
/// para birimi, fiyat kaynağı; son satır toplamlar.
pub fn bom_csv(b: &CostedBom) -> String {
    let mut out = format!(
        "{},{},{},{},{},{},{},{},{}\n",
        t("bom.col.part"),
        t("bom.col.qty"),
        t("bom.col.unit_mass_g"),
        t("bom.col.total_mass_g"),
        t("bom.col.basis"),
        t("bom.col.unit_price"),
        t("bom.col.line_total"),
        t("bom.col.currency"),
        t("bom.col.source"),
    );
    for l in &b.lines {
        out.push_str(&format!(
            "{},{},{:.2},{:.2},{},{:.2},{:.2},{},{}\n",
            csv_cell(&l.line.name),
            l.line.qty,
            l.line.unit_mass_g,
            l.total_mass_g,
            csv_cell(basis_label(l.basis)),
            l.unit_price,
            l.line_total,
            CURRENCY,
            if l.overridden {
                t("bom.source.override")
            } else {
                t("bom.source.example")
            },
        ));
    }
    out.push_str(&format!(
        "{},,,{:.2},,,{:.2},{},\n",
        csv_cell(t("bom.total")),
        b.total_mass_g,
        b.grand_total,
        CURRENCY
    ));
    out
}
