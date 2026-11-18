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

struct SheetState {
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

/// Parçaları levhalara yerleştirir. Sıra belirlenimcidir: yükseklik (azalan), genişlik (azalan),
/// parça indeksi, kopya.
pub fn nest(parts: &[FlatPart], spec: &SheetSpec) -> NestResult {
    let mut res = NestResult::default();
    let valid = spec_valid(spec);
    let usable_w = spec.width_mm - 2.0 * spec.margin_mm;
    let usable_h = spec.height_mm - 2.0 * spec.margin_mm;
    // Her kopya: (parça, kopya no, w, h)
    let mut items: Vec<(usize, u32, f64, f64)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        for c in 0..p.qty {
            match p.size() {
                Some((w, h)) if valid => {
                    let fits = (w <= usable_w && h <= usable_h)
                        || (spec.allow_rotate && h <= usable_w && w <= usable_h);
                    if fits {
                        items.push((i, c, w, h));
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
        b.3.total_cmp(&a.3)
            .then(b.2.total_cmp(&a.2))
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });
    let mut sheets: Vec<SheetState> = Vec::new();
    for (part, copy, w0, h0) in items {
        // Yönelim adayları: önce yüksekliği küçük olan (rafa daha iyi oturur).
        let mut cands = vec![(w0, h0, false)];
        if spec.allow_rotate {
            cands.push((h0, w0, true));
        }
        cands.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut placed = None;
        'orient: for &(w, h, rot) in &cands {
            if w > usable_w || h > usable_h {
                continue;
            }
            // Mevcut raflarda ilk uygun yer.
            for (si, sh) in sheets.iter_mut().enumerate() {
                for shelf in sh.shelves.iter_mut() {
                    let gap = if shelf.x > 0.0 { spec.spacing_mm } else { 0.0 };
                    if h <= shelf.h && shelf.x + gap + w <= usable_w {
                        let x = shelf.x + gap;
                        shelf.x = x + w;
                        placed = Some((si, x, shelf.y, w, h, rot));
                        break 'orient;
                    }
                }
                // Bu levhada yeni raf.
                let gap = if sh.shelves.is_empty() {
                    0.0
                } else {
                    spec.spacing_mm
                };
                if sh.next_y + gap + h <= usable_h {
                    let y = sh.next_y + gap;
                    sh.shelves.push(Shelf { y, h, x: w });
                    sh.next_y = y + h;
                    placed = Some((si, 0.0, y, w, h, rot));
                    break 'orient;
                }
            }
        }
        let (si, x, y, w, h, rot) = placed.unwrap_or_else(|| {
            // Yeni levha: sığacağı baştan doğrulandı.
            let &(w, h, rot) = cands
                .iter()
                .find(|c| c.0 <= usable_w && c.1 <= usable_h)
                .expect("sığan yönelim var");
            sheets.push(SheetState {
                shelves: vec![Shelf { y: 0.0, h, x: w }],
                next_y: h,
            });
            (sheets.len() - 1, 0.0, 0.0, w, h, rot)
        });
        res.placements.push(Placement {
            part,
            copy,
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
