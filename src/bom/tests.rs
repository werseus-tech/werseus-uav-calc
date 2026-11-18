// SPDX-License-Identifier: AGPL-3.0-or-later
use super::cost::{self, BomLine, CURRENCY, PRICE_LIST, PriceBasis};
use super::dxf::{DxfStyle, entity_counts, write_dxf};
use super::nest::{SheetSpec, nest};
use super::outlines::{FlatPart, Outline, circle_fits_in_polygon, flat_parts};
use super::{SheetMaterial, budget_mass_g, build_lines, model_mtow_g};
use crate::calc::{Library, evaluate};
use crate::i18n::{Lang, with_lang};
use crate::project::{DroneType, Project};
use std::collections::HashMap;

fn fixture(dt: DroneType) -> (Project, crate::calc::Model) {
    let mut p = Project::default();
    p.set_drone_type(dt);
    let m = evaluate(&p, &Library::embedded());
    (p, m)
}

fn line(key: &'static str, qty: u32, g: f64) -> BomLine {
    BomLine {
        key,
        name: key.to_string(),
        qty,
        unit_mass_g: g,
    }
}

#[test]
fn cost_totals_per_piece_and_per_kg() {
    // cells: 4 USD/adet; spar: 90 USD/kg.
    let lines = vec![line("cells", 6, 45.0), line("spar", 1, 200.0)];
    let b = cost::cost(&lines, &HashMap::new());
    assert!((b.lines[0].line_total - 24.0).abs() < 1e-9);
    assert!((b.lines[1].line_total - 18.0).abs() < 1e-9);
    assert!((b.grand_total - 42.0).abs() < 1e-9);
    assert!((b.total_mass_g - 470.0).abs() < 1e-9);
    assert!(b.lines.iter().all(|l| !l.overridden));
}

#[test]
fn override_changes_only_its_line_and_bad_values_are_ignored() {
    let lines = vec![line("cells", 6, 45.0), line("spar", 1, 200.0)];
    let mut ov = HashMap::new();
    ov.insert("cells".to_string(), 10.0);
    let b = cost::cost(&lines, &ov);
    assert!(b.lines[0].overridden && !b.lines[1].overridden);
    assert!((b.lines[0].line_total - 60.0).abs() < 1e-9);
    assert!((b.lines[1].line_total - 18.0).abs() < 1e-9);
    for bad in [f64::NAN, -1.0, f64::INFINITY] {
        ov.insert("cells".to_string(), bad);
        let b = cost::cost(&lines, &ov);
        assert!(!b.lines[0].overridden);
        assert!((b.lines[0].line_total - 24.0).abs() < 1e-9);
    }
    // Sıfır geçerli bir geçersiz kılmadır.
    ov.insert("cells".to_string(), 0.0);
    assert_eq!(cost::cost(&lines, &ov).lines[0].line_total, 0.0);
}

#[test]
fn price_list_keys_are_unique_and_cover_every_line() {
    for (i, (k, price, _)) in PRICE_LIST.iter().enumerate() {
        assert!(*price > 0.0, "{k}");
        assert!(PRICE_LIST.iter().skip(i + 1).all(|(o, _, _)| o != k), "{k}");
    }
    for dt in DroneType::ALL {
        let (p, m) = fixture(dt);
        for l in build_lines(&p, &m) {
            assert!(
                PRICE_LIST.iter().any(|(k, _, _)| *k == l.key),
                "{dt:?}: {} fiyatsız",
                l.key
            );
        }
    }
    assert_eq!(cost::example_price("cells").1, PriceBasis::PerPiece);
}

#[test]
fn lines_reuse_the_model_mass_budget() {
    for dt in DroneType::ALL {
        let (p, m) = fixture(dt);
        let lines = build_lines(&p, &m);
        assert!(!lines.is_empty());
        let sum = budget_mass_g(&lines);
        let mtow = model_mtow_g(&p, &m);
        assert!(
            (sum - mtow).abs() < 0.5 + mtow * 1e-6,
            "{dt:?}: satırlar {sum} g, MTOW {mtow} g"
        );
        assert!(lines.iter().all(|l| l.unit_mass_g > 0.0 && l.qty > 0));
    }
    // VTOL: seyir tahriki + 4 kaldırma motoru + kaldırma ESC grubu ayrı kalemlerdir.
    let (p_vtol, m_vtol) = fixture(DroneType::VtolQuadplane);
    let vtol_lines = build_lines(&p_vtol, &m_vtol);
    assert!(
        vtol_lines
            .iter()
            .any(|l| l.key == "propulsion" && l.qty == 1)
    );
    assert!(
        vtol_lines
            .iter()
            .any(|l| l.key == "vtol_motor" && l.qty == m_vtol.mr.n_motors.max(4))
    );
    assert!(vtol_lines.iter().any(|l| l.key == "vtol_esc" && l.qty == 1));

    // Paket çarpanı < 1.0 ya da multirotor batarya farkı olsa bile bütçe toplamı MTOW'u aşmaz.
    for dt in [DroneType::FixedWingConventional, DroneType::Multirotor] {
        let mut p = Project::default();
        p.set_drone_type(dt);
        p.pack_overhead = 0.85;
        let m = evaluate(&p, &Library::embedded());
        let lines = build_lines(&p, &m);
        let sum = budget_mass_g(&lines);
        let mtow = model_mtow_g(&p, &m);
        assert!(
            (sum - mtow).abs() < 0.5 + mtow * 1e-6,
            "{dt:?} (pack_overhead=0.85): satırlar {sum} g, MTOW {mtow} g"
        );
    }
}

fn split_csv(line: &str) -> Vec<String> {
    let (mut out, mut cur, mut q) = (Vec::new(), String::new(), false);
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        match (c, q) {
            ('"', true) if it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            ('"', _) => q = !q,
            (',', false) => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[test]
fn csv_has_nine_columns_on_every_row_and_totals_row() {
    for lang in [Lang::Tr, Lang::En] {
        with_lang(lang, || {
            for dt in DroneType::ALL {
                let (p, m) = fixture(dt);
                let mut ov = HashMap::new();
                ov.insert("payload".to_string(), 12.5);
                let b = cost::cost(&build_lines(&p, &m), &ov);
                let csv = cost::bom_csv(&b);
                let rows: Vec<Vec<String>> = csv.lines().map(split_csv).collect();
                assert_eq!(rows.len(), b.lines.len() + 2, "{dt:?}");
                assert!(rows.iter().all(|r| r.len() == 9), "{dt:?}: {csv}");
                let total = rows.last().unwrap();
                assert_eq!(total[6], format!("{:.2}", b.grand_total));
                assert_eq!(total[7], CURRENCY);
                assert!(
                    rows.iter()
                        .any(|r| r[8] == crate::i18n::t("bom.source.override"))
                );
            }
        });
    }
}

// ------------------------------------------------------------------ nesting

fn rect_part(w: f64, h: f64, qty: u32) -> FlatPart {
    FlatPart {
        name: format!("r{w}x{h}"),
        qty,
        material: SheetMaterial::CarbonPlate,
        thickness_mm: 2.0,
        outline: Outline::Polygon(vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]),
        holes: vec![],
    }
}

/// Belirlenimci sözde-rastgele sayı (LCG), [0, 1).
fn rnd(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 33) as f64) / ((1u64 << 31) as f64)
}

fn assert_valid_layout(parts: &[FlatPart], spec: &SheetSpec, res: &super::nest::NestResult) {
    let eps = 1e-6;
    for (i, a) in res.placements.iter().enumerate() {
        assert!(
            a.x >= spec.margin_mm - eps && a.y >= spec.margin_mm - eps,
            "{a:?}"
        );
        assert!(a.x + a.w <= spec.width_mm - spec.margin_mm + eps, "{a:?}");
        assert!(a.y + a.h <= spec.height_mm - spec.margin_mm + eps, "{a:?}");
        let (pw, ph) = parts[a.part].size().unwrap();
        let want = if a.rotated { (ph, pw) } else { (pw, ph) };
        assert!((a.w - want.0).abs() < eps && (a.h - want.1).abs() < eps);
        for b in &res.placements[i + 1..] {
            if a.sheet != b.sheet {
                continue;
            }
            let sep_x = a.x + a.w + spec.spacing_mm <= b.x + eps
                || b.x + b.w + spec.spacing_mm <= a.x + eps;
            let sep_y = a.y + a.h + spec.spacing_mm <= b.y + eps
                || b.y + b.h + spec.spacing_mm <= a.y + eps;
            assert!(sep_x || sep_y, "çakışma: {a:?} / {b:?}");
        }
    }
}

#[test]
fn nesting_never_overlaps_and_stays_inside_the_sheet() {
    let mut s = 12345_u64;
    for case in 0..60 {
        let spec = SheetSpec {
            width_mm: 150.0 + 300.0 * rnd(&mut s),
            height_mm: 150.0 + 300.0 * rnd(&mut s),
            spacing_mm: 5.0 * rnd(&mut s),
            margin_mm: 8.0 * rnd(&mut s),
            allow_rotate: case % 2 == 0,
        };
        let n = 3 + (rnd(&mut s) * 10.0) as usize;
        let parts: Vec<FlatPart> = (0..n)
            .map(|_| {
                rect_part(
                    5.0 + 120.0 * rnd(&mut s),
                    5.0 + 120.0 * rnd(&mut s),
                    1 + (rnd(&mut s) * 4.0) as u32,
                )
            })
            .collect();
        let total: u32 = parts.iter().map(|p| p.qty).sum();
        let res = nest(&parts, &spec);
        assert_eq!(
            res.placements.len() + res.unplaced.len(),
            total as usize,
            "hiçbir kopya kaybolmamalı"
        );
        assert!(res.unplaced.is_empty(), "bu boyutlar hep sığar: {spec:?}");
        assert_valid_layout(&parts, &spec, &res);
        assert_eq!(res.usage_pct.len(), res.sheets);
        assert!(res.usage_pct.iter().all(|u| (0.0..=100.0).contains(u)));
        // Belirlenimci: aynı girdi, aynı yerleşim.
        assert_eq!(res, nest(&parts, &spec));
    }
}

#[test]
fn oversized_parts_are_reported_not_dropped() {
    let spec = SheetSpec {
        width_mm: 300.0,
        height_mm: 300.0,
        spacing_mm: 3.0,
        margin_mm: 5.0,
        allow_rotate: true,
    };
    let parts = vec![
        rect_part(400.0, 50.0, 2),
        rect_part(100.0, 100.0, 3),
        rect_part(295.0, 295.0, 1), // kenar payı yüzünden sığmaz
    ];
    let res = nest(&parts, &spec);
    assert_eq!(res.unplaced.len(), 3);
    assert_eq!(res.unplaced.iter().filter(|u| u.part == 0).count(), 2);
    assert_eq!(res.unplaced.iter().filter(|u| u.part == 2).count(), 1);
    assert_eq!(res.placements.len(), 3);
    // Döndürme ile sığan parça: dar levhada (100x300) yalnız 90° döndürülerek sığar.
    let rot_fit = nest(
        &[rect_part(285.0, 20.0, 1)],
        &SheetSpec {
            width_mm: 100.0,
            height_mm: 300.0,
            allow_rotate: true,
            ..spec
        },
    );
    assert!(rot_fit.unplaced.is_empty());
    assert_eq!(rot_fit.placements.len(), 1);
    assert!(rot_fit.placements[0].rotated);
    let no_rot = SheetSpec {
        allow_rotate: false,
        ..spec
    };
    let long = nest(&[rect_part(285.0, 20.0, 1)], &no_rot);
    assert!(long.unplaced.is_empty());
    let too_long = nest(
        &[rect_part(285.0, 20.0, 1)],
        &SheetSpec {
            width_mm: 100.0,
            allow_rotate: false,
            ..spec
        },
    );
    assert_eq!(too_long.unplaced.len(), 1);
}

#[test]
fn canonical_shelf_height_and_all_orientations_tested_before_new_shelf() {
    let spec = SheetSpec {
        width_mm: 210.0,
        height_mm: 210.0,
        spacing_mm: 0.0,
        margin_mm: 5.0,
        allow_rotate: true,
    };
    // 10x100 parça döndürülünce yüksekliği 10 olur; 50x50 parça (kanonik yükseklik 50) önce
    // yerleştirilip raf açmalı, 10x100 onun yanına aynı rafa sığmalı (y < 55).
    let parts = vec![rect_part(10.0, 100.0, 1), rect_part(50.0, 50.0, 1)];
    let res = nest(&parts, &spec);
    assert_eq!(res.sheets, 1);
    assert!(res.placements.iter().all(|pl| (pl.y - 5.0).abs() < 1e-6));

    // Mevcut rafa (yükseklik 50, kalan genişlik 25) 40x20 parça yatay sığmaz ama 20x40 olarak
    // döndürülüp mevcut rafa yerleşmeli (yeni raf açmamalı).
    let parts2 = vec![rect_part(175.0, 50.0, 1), rect_part(40.0, 20.0, 1)];
    let res2 = nest(&parts2, &spec);
    assert_eq!(res2.sheets, 1);
    assert!(res2.placements.iter().all(|pl| (pl.y - 5.0).abs() < 1e-6));
}

#[test]
fn invalid_outlines_and_specs_are_unplaced() {
    let bad = FlatPart {
        name: "bad".into(),
        qty: 1,
        material: SheetMaterial::CarbonPlate,
        thickness_mm: 1.0,
        outline: Outline::Polygon(vec![[0.0, 0.0], [f64::NAN, 1.0], [1.0, 1.0]]),
        holes: vec![],
    };
    let res = nest(&[bad, rect_part(10.0, 10.0, 1)], &SheetSpec::default());
    assert_eq!(res.unplaced.len(), 1);
    assert_eq!(res.placements.len(), 1);
    let zero = SheetSpec {
        width_mm: 0.0,
        ..SheetSpec::default()
    };
    let res = nest(&[rect_part(10.0, 10.0, 2)], &zero);
    assert_eq!(res.unplaced.len(), 2);
    assert_eq!(res.sheets, 0);
}

#[test]
fn many_parts_use_several_sheets() {
    let spec = SheetSpec::default();
    let res = nest(&[rect_part(140.0, 140.0, 10)], &spec);
    assert!(res.sheets >= 3);
    assert_valid_layout(&[rect_part(140.0, 140.0, 10)], &spec, &res);
}

// ------------------------------------------------------------------ DXF

#[test]
fn dxf_parses_back_with_expected_entity_counts() {
    for dt in DroneType::ALL {
        let (p, m) = fixture(dt);
        let parts = flat_parts(&p, &m);
        assert!(!parts.is_empty(), "{dt:?}");
        let spec = SheetSpec {
            width_mm: 300.0,
            height_mm: 300.0,
            ..SheetSpec::default()
        };
        let res = nest(&parts, &spec);
        let n_poly = res
            .placements
            .iter()
            .filter(|pl| matches!(parts[pl.part].outline, Outline::Polygon(_)))
            .count();
        let n_vertex: usize = res
            .placements
            .iter()
            .map(|pl| match &parts[pl.part].outline {
                Outline::Polygon(v) => v.len(),
                _ => 0,
            })
            .sum();
        let n_circles: usize = res
            .placements
            .iter()
            .map(|pl| {
                let pt = &parts[pl.part];
                pt.holes.len() + usize::from(matches!(pt.outline, Outline::Circle(_)))
            })
            .sum();
        for style in [DxfStyle::R12, DxfStyle::R14] {
            let dxf = write_dxf(&parts, &res, &spec, style);
            assert!(dxf.ends_with("0\nEOF\n"), "{dt:?}");
            assert!(dxf.contains(if style == DxfStyle::R12 {
                "AC1009"
            } else {
                "AC1014"
            }));
            let c = entity_counts(&dxf).unwrap();
            assert_eq!(c.get("LINE").copied().unwrap_or(0), 4 * res.sheets);
            assert_eq!(c.get("CIRCLE").copied().unwrap_or(0), n_circles, "{dt:?}");
            match style {
                DxfStyle::R12 => {
                    assert!(!dxf.contains("$INSUNITS"));
                    assert!(dxf.contains(
                        "0\nPOLYLINE\n8\nCUT\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n1\n"
                    ));
                    assert_eq!(c.get("POLYLINE").copied().unwrap_or(0), n_poly);
                    assert_eq!(c.get("VERTEX").copied().unwrap_or(0), n_vertex);
                    assert_eq!(c.get("SEQEND").copied().unwrap_or(0), n_poly);
                    assert!(!c.contains_key("LWPOLYLINE"));
                }
                DxfStyle::R14 => {
                    assert!(dxf.contains("$INSUNITS"));
                    assert!(dxf.contains("100\nAcDbEntity\n"));
                    assert!(dxf.contains("100\nAcDbLwPolyline\n"));
                    assert_eq!(c.get("LWPOLYLINE").copied().unwrap_or(0), n_poly);
                    assert!(!c.contains_key("POLYLINE"));
                }
            }
        }
        // Her levhada yalnızca aynı malzeme ve kalınlıktaki parçalar bulunur.
        for s in 0..res.sheets {
            let mut on_sheet = res.placements.iter().filter(|pl| pl.sheet == s);
            if let Some(first) = on_sheet.next() {
                let p0 = &parts[first.part];
                for pl in on_sheet {
                    let pt = &parts[pl.part];
                    assert_eq!(pt.material, p0.material, "{dt:?} sheet {s}");
                    assert!(
                        (pt.thickness_mm - p0.thickness_mm).abs() < 1e-6,
                        "{dt:?} sheet {s}: {} vs {}",
                        pt.thickness_mm,
                        p0.thickness_mm
                    );
                }
            }
        }
    }
}

#[test]
fn dxf_parser_rejects_truncated_files() {
    let spec = SheetSpec::default();
    let parts = vec![rect_part(20.0, 10.0, 1)];
    let res = nest(&parts, &spec);
    let dxf = write_dxf(&parts, &res, &spec, DxfStyle::R12);
    let cut = &dxf[..dxf.len() - "0\nEOF\n".len()];
    assert!(entity_counts(cut).is_err());
    assert!(entity_counts("0\nSECTION\nxx\n").is_err());
    assert!(entity_counts("0\nSECTION\n2\nENTITIES\n0\nEOF\n").is_err());
    assert!(entity_counts("0\nSECTION\n0\nEOF\n").is_err());
    assert!(
        entity_counts(
            "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\nCUT\n10\nNaN\n20\n0.0\n30\n0.0\n40\n5.0\n0\nENDSEC\n0\nEOF\n"
        )
        .is_err()
    );
}

#[test]
fn flat_parts_have_valid_geometry_for_every_type() {
    for dt in DroneType::ALL {
        let (p, m) = fixture(dt);
        for part in flat_parts(&p, &m) {
            assert!(
                part.qty > 0 && part.thickness_mm > 0.0,
                "{dt:?} {}",
                part.name
            );
            let (w, h) = part
                .size()
                .unwrap_or_else(|| panic!("{dt:?} {}", part.name));
            assert!(w > 0.0 && h > 0.0);
            assert!(part.net_area_mm2() > 0.0, "{dt:?} {}", part.name);
            let (lo, hi) = part.bounds().unwrap();
            for (i, (c, d)) in part.holes.iter().enumerate() {
                assert!(*d > 0.0, "{dt:?} {}", part.name);
                assert!(
                    c[0] - d / 2.0 >= lo[0] - 1e-6 && c[0] + d / 2.0 <= hi[0] + 1e-6,
                    "{dt:?} {}",
                    part.name
                );
                assert!(
                    c[1] - d / 2.0 >= lo[1] - 1e-6 && c[1] + d / 2.0 <= hi[1] + 1e-6,
                    "{dt:?} {}",
                    part.name
                );
                match &part.outline {
                    Outline::Circle(r) => {
                        assert!(
                            c[0].hypot(c[1]) + d / 2.0 <= r + 1e-6,
                            "{dt:?} {}: delik {c:?} d={d} daire r={r} dışında",
                            part.name
                        );
                    }
                    Outline::Polygon(pts) => {
                        assert!(
                            circle_fits_in_polygon(pts, *c, d / 2.0 - 1e-6),
                            "{dt:?} {}: delik {c:?} d={d} çokgen sınırının dışında",
                            part.name
                        );
                    }
                }
                for (c2, d2) in &part.holes[i + 1..] {
                    let dist = (c[0] - c2[0]).hypot(c[1] - c2[1]);
                    assert!(
                        dist >= (d + d2) / 2.0 - 1e-6,
                        "{dt:?} {}: delikler çakışıyor ({c:?} ve {c2:?})",
                        part.name
                    );
                }
            }
        }
    }
    // Motor yuvası delik dairesi çapı (BCD) tam olarak `motor_bolt_pattern_mm` olmalı.
    let (p_fw, m_fw) = fixture(DroneType::FixedWingConventional);
    let fw_parts = flat_parts(&p_fw, &m_fw);
    let mount = fw_parts
        .iter()
        .find(|pt| pt.material == SheetMaterial::CarbonPlate)
        .unwrap();
    for (c, _) in mount.holes.iter().skip(1) {
        let bcd = 2.0 * c[0].hypot(c[1]);
        assert!((bcd - 19.0).abs() < 1e-6, "BCD={bcd}");
    }
    // Çok daralan uçlu kanatta kiriş deliği ince uç kaburgasını ikiye bölmemeli.
    let mut p_thin = p_fw;
    p_thin.taper = 0.15;
    p_thin.d_spar_o = 0.016;
    let m_thin = evaluate(&p_thin, &Library::embedded());
    for part in flat_parts(&p_thin, &m_thin) {
        if let Outline::Polygon(pts) = &part.outline {
            for (c, d) in &part.holes {
                assert!(circle_fits_in_polygon(pts, *c, d / 2.0));
            }
        }
    }
}

#[test]
fn rotated_placement_stays_in_the_rotated_box() {
    let part = rect_part(30.0, 10.0, 1);
    let ents = part.placed([100.0, 50.0], true);
    let super::outlines::Entity::Polygon(pts) = &ents[0] else {
        panic!()
    };
    for p in pts {
        assert!(p[0] >= 100.0 - 1e-9 && p[0] <= 110.0 + 1e-9);
        assert!(p[1] >= 50.0 - 1e-9 && p[1] <= 80.0 + 1e-9);
    }
}

// ------------------------------------------------------------------ i18n

#[test]
fn bom_i18n_keys_have_tr_en_parity_and_are_all_defined() {
    use std::collections::BTreeSet;
    let keys = |t: &std::collections::HashMap<&'static str, &'static str>| -> BTreeSet<String> {
        t.keys()
            .filter(|k| k.starts_with("bom."))
            .map(|k| k.to_string())
            .collect()
    };
    let (tr, en) = (keys(crate::i18n::tr_table()), keys(crate::i18n::en_table()));
    assert!(tr.len() > 40);
    assert_eq!(tr, en);
    for k in &tr {
        let (a, b) = (
            crate::i18n::tr_table()[k.as_str()],
            crate::i18n::en_table()[k.as_str()],
        );
        assert!(!a.trim().is_empty() && !b.trim().is_empty(), "{k}");
        // Yer tutucular iki dilde aynı olmalı.
        for ph in ["{0}", "{1}", "{2}", "{3}"] {
            assert_eq!(a.contains(ph), b.contains(ph), "{k} {ph}");
        }
    }
    // Kaynakta kullanılan her "bom.*" anahtarı tanımlı.
    for src in [
        include_str!("mod.rs"),
        include_str!("cost.rs"),
        include_str!("outlines.rs"),
        include_str!("../ui/bom.rs"),
    ] {
        let prod = src.split("#[cfg(test)]").next().unwrap();
        let mut rest = prod;
        while let Some(i) = rest.find("\"bom.") {
            let tail = &rest[i + 1..];
            let end = tail.find('"').unwrap();
            let key = &tail[..end];
            assert!(tr.contains(key), "tanımsız anahtar: {key}");
            rest = &tail[end..];
        }
    }
}
