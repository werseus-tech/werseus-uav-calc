// SPDX-License-Identifier: AGPL-3.0-or-later
//! Basit, belirlenimci raf (shelf) yerleşimi: parçalar sınır kutularıyla (dikdörtgen) levhalara
//! dizilir. Gerçek şekil iç içe geçirme yapılmaz; dairesel ve eğri parçalar kendi sınır kutusunu
//! kaplar. Levhaya sığmayan parçalar sessizce atılmaz, `unplaced` listesinde raporlanır.

use super::outlines::FlatPart;

/// Levha ve kesim ayarları (mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetSpec {
    pub width_mm: f64,
    pub height_mm: f64,
    /// Parçalar arası boşluk (kerf + güvenlik payı).
    pub spacing_mm: f64,
    /// Levha kenarından boşluk.
    pub margin_mm: f64,
    /// Parçaların 90° döndürülmesine izin ver.
    pub allow_rotate: bool,
}

impl Default for SheetSpec {
    fn default() -> Self {
        Self {
            width_mm: 300.0,
            height_mm: 300.0,
            spacing_mm: 3.0,
            margin_mm: 5.0,
            allow_rotate: true,
        }
    }
}

/// Yerleştirilmiş bir parça kopyası.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub part: usize,
    pub copy: u32,
    pub sheet: usize,
    /// Yerleşmiş sınır kutusunun sol-alt köşesi (levha koordinatı).
    pub x: f64,
    pub y: f64,
    /// Yerleşmiş sınır kutusu boyutu (döndürme uygulanmış).
    pub w: f64,
    pub h: f64,
    pub rotated: bool,
}

/// Yerleştirilemeyen parça kopyası.
#[derive(Clone, Debug, PartialEq)]
pub struct Unplaced {
    pub part: usize,
    pub copy: u32,
    /// Sınır kutusu (geçersiz ana hatta `None`).
    pub size: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NestResult {
    pub placements: Vec<Placement>,
    pub unplaced: Vec<Unplaced>,
    pub sheets: usize,
    /// Levha başına net parça alanı yüzdesi (0-100).
    pub usage_pct: Vec<f64>,
    /// Tüm levhalar için toplam kullanım yüzdesi.
    pub total_usage_pct: f64,
}

struct Shelf {
    y: f64,
    h: f64,
    x: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StockKey {
    material: super::SheetMaterial,
    thickness_um: i64,
}

fn stock_key(p: &FlatPart) -> StockKey {
    StockKey {
        material: p.material,
        thickness_um: (p.thickness_mm * 1000.0).round() as i64,
    }
}

struct SheetState {
    group: StockKey,
    shelves: Vec<Shelf>,
    /// Sonraki raf için y.
    next_y: f64,
}

fn spec_valid(s: &SheetSpec) -> bool {
    [s.width_mm, s.height_mm, s.spacing_mm, s.margin_mm]
        .iter()
        .all(|v| v.is_finite())
        && s.width_mm > 0.0
        && s.height_mm > 0.0
        && s.spacing_mm >= 0.0
        && s.margin_mm >= 0.0
}

fn valid_orientations(
    w0: f64,
    h0: f64,
    usable_w: f64,
    usable_h: f64,
    allow_rotate: bool,
) -> Vec<(f64, f64, bool)> {
    let mut cands = Vec::with_capacity(2);
    if w0 <= usable_w && h0 <= usable_h {
        cands.push((w0, h0, false));
    }
    if allow_rotate && h0 <= usable_w && w0 <= usable_h && (w0 - h0).abs() > 1e-9 {
        cands.push((h0, w0, true));
    }
    cands.sort_by(|a, b| a.1.total_cmp(&b.1).then(b.0.total_cmp(&a.0)));
    cands
}

struct NestItem {
    group: StockKey,
    part: usize,
    copy: u32,
    w0: f64,
    h0: f64,
    canon_w: f64,
    canon_h: f64,
}

/// Parçaları levhalara yerleştirir. Parçalar önce `(malzeme, kalınlık)` grubuna göre ayrılır; her
/// grup içinde sıra belirlenimcidir: kanonik raf yüksekliği (azalan), genişlik (azalan), parça
/// indeksi, kopya.
pub fn nest(parts: &[FlatPart], spec: &SheetSpec) -> NestResult {
    let mut res = NestResult::default();
    let valid = spec_valid(spec);
    let usable_w = spec.width_mm - 2.0 * spec.margin_mm;
    let usable_h = spec.height_mm - 2.0 * spec.margin_mm;
    let mut items: Vec<NestItem> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let thick_ok = p.thickness_mm.is_finite() && p.thickness_mm > 0.0;
        for c in 0..p.qty {
            match p.size() {
                Some((w, h)) if valid && thick_ok => {
                    let cands = valid_orientations(w, h, usable_w, usable_h, spec.allow_rotate);
                    if let Some(&(canon_w, canon_h, _)) = cands.first() {
                        items.push(NestItem {
                            group: stock_key(p),
                            part: i,
                            copy: c,
                            w0: w,
                            h0: h,
                            canon_w,
                            canon_h,
                        });
                    } else {
                        res.unplaced.push(Unplaced {
                            part: i,
                            copy: c,
                            size: Some((w, h)),
                        });
                    }
                }
                other => res.unplaced.push(Unplaced {
                    part: i,
                    copy: c,
                    size: other,
                }),
            }
        }
    }
    items.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then(b.canon_h.total_cmp(&a.canon_h))
            .then(b.canon_w.total_cmp(&a.canon_w))
            .then(a.part.cmp(&b.part))
            .then(a.copy.cmp(&b.copy))
    });
    let mut sheets: Vec<SheetState> = Vec::new();
    for item in items {
        let cands = valid_orientations(item.w0, item.h0, usable_w, usable_h, spec.allow_rotate);
        let mut placed = None;
        // 1. Mevcut raflarda tüm geçerli yönelimleri dene.
        'shelf_search: for (si, sh) in sheets.iter_mut().enumerate() {
            if sh.group != item.group {
                continue;
            }
            for shelf in sh.shelves.iter_mut() {
                let gap = if shelf.x > 0.0 { spec.spacing_mm } else { 0.0 };
                for &(w, h, rot) in &cands {
                    if h <= shelf.h && shelf.x + gap + w <= usable_w {
                        let x = shelf.x + gap;
                        shelf.x = x + w;
                        placed = Some((si, x, shelf.y, w, h, rot));
                        break 'shelf_search;
                    }
                }
            }
        }
        // 2. Mevcut raflara sığmadıysa aynı gruptaki levhalarda yeni raf açmayı dene.
        if placed.is_none() {
            'new_shelf: for (si, sh) in sheets.iter_mut().enumerate() {
                if sh.group != item.group {
                    continue;
                }
                let gap = if sh.shelves.is_empty() {
                    0.0
                } else {
                    spec.spacing_mm
                };
                for &(w, h, rot) in &cands {
                    if sh.next_y + gap + h <= usable_h {
                        let y = sh.next_y + gap;
                        sh.shelves.push(Shelf { y, h, x: w });
                        sh.next_y = y + h;
                        placed = Some((si, 0.0, y, w, h, rot));
                        break 'new_shelf;
                    }
                }
            }
        }
        let (si, x, y, w, h, rot) = placed.unwrap_or_else(|| {
            // 3. Yeni levha: sığacağı baştan doğrulandı.
            let (w, h, rot) = cands[0];
            sheets.push(SheetState {
                group: item.group,
                shelves: vec![Shelf { y: 0.0, h, x: w }],
                next_y: h,
            });
            (sheets.len() - 1, 0.0, 0.0, w, h, rot)
        });
        res.placements.push(Placement {
            part: item.part,
            copy: item.copy,
            sheet: si,
            x: spec.margin_mm + x,
            y: spec.margin_mm + y,
            w,
            h,
            rotated: rot,
        });
    }
    res.sheets = sheets.len();
    let sheet_area = spec.width_mm * spec.height_mm;
    let mut used = vec![0.0; res.sheets];
    for pl in &res.placements {
        used[pl.sheet] += parts[pl.part].net_area_mm2();
    }
    res.usage_pct = used.iter().map(|u| 100.0 * u / sheet_area).collect();
    res.total_usage_pct = if res.sheets > 0 {
        100.0 * used.iter().sum::<f64>() / (sheet_area * res.sheets as f64)
    } else {
        0.0
    };
    res
}
