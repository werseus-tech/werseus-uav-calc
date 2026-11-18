// SPDX-License-Identifier: AGPL-3.0-or-later
//! Düz ASCII DXF yazıcı (harici kütüphane yok). Varsayılan stil AutoCAD R12'dir (`AC1009`):
//! LINE, CIRCLE ve kapalı POLYLINE/VERTEX/SEQEND. İsteğe bağlı R14 stilinde çokgenler
//! LWPOLYLINE olarak yazılır. Levhalar X ekseni boyunca yan yana dizilir; birim milimetredir.

use super::nest::{NestResult, SheetSpec};
use super::outlines::{Entity, FlatPart};
use std::collections::BTreeMap;
use std::fmt::Write;

/// DXF sürüm stili.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DxfStyle {
    /// R12 (`AC1009`): çokgenler POLYLINE + VERTEX + SEQEND.
    R12,
    /// R14 (`AC1014`): çokgenler LWPOLYLINE.
    R14,
}

const LAYER_SHEET: &str = "SHEET";
const LAYER_CUT: &str = "CUT";
const LAYER_HOLES: &str = "HOLES";
/// Yan yana levhalar arası boşluk [mm].
const SHEET_GAP: f64 = 20.0;

fn pair(out: &mut String, code: i32, value: impl std::fmt::Display) {
    let _ = writeln!(out, "{code}\n{value}");
}

fn next_handle(out: &mut String, handle: &mut u32) {
    pair(out, 5, format!("{handle:X}"));
    *handle += 1;
}

fn line(
    out: &mut String,
    style: DxfStyle,
    handle: &mut u32,
    layer: &str,
    a: [f64; 2],
    b: [f64; 2],
) {
    pair(out, 0, "LINE");
    if style == DxfStyle::R14 {
        next_handle(out, handle);
        pair(out, 100, "AcDbEntity");
    }
    pair(out, 8, layer);
    if style == DxfStyle::R14 {
        pair(out, 100, "AcDbLine");
    }
    pair(out, 10, format!("{:.4}", a[0]));
    pair(out, 20, format!("{:.4}", a[1]));
    pair(out, 30, "0.0");
    pair(out, 11, format!("{:.4}", b[0]));
    pair(out, 21, format!("{:.4}", b[1]));
    pair(out, 31, "0.0");
}

fn circle(out: &mut String, style: DxfStyle, handle: &mut u32, layer: &str, c: [f64; 2], r: f64) {
    pair(out, 0, "CIRCLE");
    if style == DxfStyle::R14 {
        next_handle(out, handle);
        pair(out, 100, "AcDbEntity");
    }
    pair(out, 8, layer);
    if style == DxfStyle::R14 {
        pair(out, 100, "AcDbCircle");
    }
    pair(out, 10, format!("{:.4}", c[0]));
    pair(out, 20, format!("{:.4}", c[1]));
    pair(out, 30, "0.0");
    pair(out, 40, format!("{r:.4}"));
}

fn polygon(out: &mut String, style: DxfStyle, handle: &mut u32, layer: &str, pts: &[[f64; 2]]) {
    match style {
        DxfStyle::R12 => {
            pair(out, 0, "POLYLINE");
            pair(out, 8, layer);
            pair(out, 66, 1);
            pair(out, 10, "0.0");
            pair(out, 20, "0.0");
            pair(out, 30, "0.0");
            pair(out, 70, 1);
            for p in pts {
                pair(out, 0, "VERTEX");
                pair(out, 8, layer);
                pair(out, 10, format!("{:.4}", p[0]));
                pair(out, 20, format!("{:.4}", p[1]));
                pair(out, 30, "0.0");
            }
            pair(out, 0, "SEQEND");
            pair(out, 8, layer);
        }
        DxfStyle::R14 => {
            pair(out, 0, "LWPOLYLINE");
            next_handle(out, handle);
            pair(out, 100, "AcDbEntity");
            pair(out, 8, layer);
            pair(out, 100, "AcDbLwPolyline");
            pair(out, 90, pts.len());
            pair(out, 70, 1);
            for p in pts {
                pair(out, 10, format!("{:.4}", p[0]));
                pair(out, 20, format!("{:.4}", p[1]));
            }
        }
    }
}

/// Yerleşimi DXF metnine çevirir. Her levha için dış çerçeve (4 LINE, `SHEET` katmanı), parça
/// dış hatları (`CUT`) ve delikleri (`HOLES`, CIRCLE) yazılır.
pub fn write_dxf(
    parts: &[FlatPart],
    nest: &NestResult,
    spec: &SheetSpec,
    style: DxfStyle,
) -> String {
    let mut out = String::new();
    let mut handle: u32 = 1;
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "HEADER");
    pair(&mut out, 9, "$ACADVER");
    pair(
        &mut out,
        1,
        match style {
            DxfStyle::R12 => "AC1009",
            DxfStyle::R14 => "AC1014",
        },
    );
    if style == DxfStyle::R14 {
        pair(&mut out, 9, "$INSUNITS");
        pair(&mut out, 70, 4);
    }
    pair(&mut out, 0, "ENDSEC");
    // Katman tablosu.
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "TABLES");
    pair(&mut out, 0, "TABLE");
    pair(&mut out, 2, "LAYER");
    if style == DxfStyle::R14 {
        next_handle(&mut out, &mut handle);
        pair(&mut out, 100, "AcDbSymbolTable");
    }
    pair(&mut out, 70, 3);
    for (name, color) in [(LAYER_SHEET, 8), (LAYER_CUT, 7), (LAYER_HOLES, 1)] {
        pair(&mut out, 0, "LAYER");
        if style == DxfStyle::R14 {
            next_handle(&mut out, &mut handle);
            pair(&mut out, 100, "AcDbSymbolTableRecord");
            pair(&mut out, 100, "AcDbLayerTableRecord");
        }
        pair(&mut out, 2, name);
        pair(&mut out, 70, 0);
        pair(&mut out, 62, color);
        pair(&mut out, 6, "CONTINUOUS");
    }
    pair(&mut out, 0, "ENDTAB");
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "SECTION");
    pair(&mut out, 2, "ENTITIES");
    for s in 0..nest.sheets {
        let ox = s as f64 * (spec.width_mm + SHEET_GAP);
        let (x0, x1, y0, y1) = (ox, ox + spec.width_mm, 0.0, spec.height_mm);
        let c = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        for k in 0..4 {
            line(
                &mut out,
                style,
                &mut handle,
                LAYER_SHEET,
                c[k],
                c[(k + 1) % 4],
            );
        }
        for pl in nest.placements.iter().filter(|pl| pl.sheet == s) {
            let ents = parts[pl.part].placed([ox + pl.x, pl.y], pl.rotated);
            for (i, e) in ents.iter().enumerate() {
                match e {
                    Entity::Polygon(pts) => polygon(&mut out, style, &mut handle, LAYER_CUT, pts),
                    Entity::Circle { c, r } => {
                        let layer = if i == 0 { LAYER_CUT } else { LAYER_HOLES };
                        circle(&mut out, style, &mut handle, layer, *c, *r);
                    }
                }
            }
        }
    }
    pair(&mut out, 0, "ENDSEC");
    pair(&mut out, 0, "EOF");
    out
}

/// DXF metnini geri okuyup ENTITIES bölümündeki varlık türlerini sayar (R12'de POLYLINE sayılır,
/// VERTEX/SEQEND ayrıca sayılır). Yapı bozuksa (çift sayısı, kapanmamış SECTION, EOF eksik veya
/// sonlu olmayan koordinat/yarıçap) hata döner.
pub fn entity_counts(dxf: &str) -> Result<BTreeMap<String, usize>, String> {
    let lines: Vec<&str> = dxf.lines().collect();
    if !lines.len().is_multiple_of(2) {
        return Err("odd number of lines".into());
    }
    let mut counts = BTreeMap::new();
    let mut in_entities = false;
    let mut after_section = false;
    let mut in_section = false;
    let mut eof = false;
    let mut i = 0;
    while i < lines.len() {
        let code: i32 = lines[i]
            .trim()
            .parse()
            .map_err(|_| format!("bad group code at line {}", i + 1))?;
        let val = lines[i + 1].trim();
        if code == 0 {
            match val {
                "EOF" => {
                    if in_entities || after_section || in_section {
                        return Err("unclosed SECTION at EOF".into());
                    }
                    eof = true;
                    if i + 2 != lines.len() {
                        return Err("data after EOF".into());
                    }
                }
                "SECTION" => {
                    if in_section {
                        return Err("nested SECTION".into());
                    }
                    after_section = true;
                    in_section = true;
                }
                "ENDSEC" => {
                    if !in_section || after_section {
                        return Err("unexpected ENDSEC".into());
                    }
                    in_entities = false;
                    in_section = false;
                }
                other if in_entities => *counts.entry(other.to_string()).or_insert(0) += 1,
                _ => {}
            }
        } else {
            if after_section {
                if code == 2 {
                    in_entities = val == "ENTITIES";
                    after_section = false;
                } else {
                    return Err("SECTION must be followed by group code 2".into());
                }
            }
            if matches!(code, 10 | 11 | 20 | 21 | 30 | 31 | 40) {
                let v: f64 = val
                    .parse()
                    .map_err(|_| format!("bad coordinate at line {}", i + 2))?;
                if !v.is_finite() {
                    return Err(format!("non-finite coordinate at line {}", i + 2));
                }
            }
        }
        i += 2;
    }
    if !eof {
        return Err("missing EOF".into());
    }
    Ok(counts)
}
