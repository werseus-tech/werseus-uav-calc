// SPDX-License-Identifier: AGPL-3.0-or-later
//! Malzeme listesi (BOM) ve DXF yerleşimi sekmesi: maliyetli BOM (örnek fiyatlar, satır başına
//! geçersiz kılma, CSV) ve levha yerleşimi (önizleme, kullanım, DXF). Hesap `crate::bom` içindedir.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::bom::cost::{self, CURRENCY, CostedBom};
use crate::bom::dxf::{DxfStyle, write_dxf};
use crate::bom::nest::{NestResult, SheetSpec, nest};
use crate::bom::outlines::{Entity, FlatPart, flat_parts};
use crate::bom::{self, SheetMaterial};
use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{Pos2, Rect, Sense, Shape, Stroke, Ui, vec2};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Sekmenin oturum durumu (proje dosyasına yazılmaz).
pub struct BomUiState {
    /// Satır anahtarı -> kullanıcı birim fiyatı.
    pub overrides: HashMap<String, f64>,
    pub spec: SheetSpec,
    pub material: SheetMaterial,
    pub style: DxfStyle,
    /// Önizlenen levha (0 tabanlı).
    pub sheet_view: usize,
    pub status: Option<String>,
    cache: Option<(u64, Vec<FlatPart>)>,
}

impl Default for BomUiState {
    fn default() -> Self {
        Self {
            overrides: HashMap::new(),
            spec: SheetSpec::default(),
            material: SheetMaterial::CarbonPlate,
            style: DxfStyle::R12,
            sheet_view: 0,
            status: None,
            cache: None,
        }
    }
}

fn cache_key(p: &Project) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.to_json().unwrap_or_default().hash(&mut h);
    (crate::i18n::current_lang() as u8).hash(&mut h);
    h.finish()
}

fn save_text(st: &mut BomUiState, name: &str, filter: (&str, &[&str]), body: String) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(filter.0, filter.1)
        .set_file_name(name)
        .save_file()
    else {
        return;
    };
    st.status = Some(match std::fs::write(&path, body) {
        Ok(()) => tf("bom.status.saved_fmt", &[path.display().to_string()]),
        Err(e) => tf("bom.status.error_fmt", &[e.to_string()]),
    });
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut BomUiState) {
    let tk = theme::tokens(ui);
    section_header(ui, t("bom.tab"));
    components::banner(ui, Status::Info, t("bom.disclaimer"));
    ui.add_space(space::S);
    cost_section(ui, p, m, st);
    ui.add_space(space::M);
    ui.separator();
    nest_section(ui, p, m, st);
    if let Some(s) = &st.status {
        ui.add_space(space::S);
        ui.colored_label(tk.text_weak, s);
    }
}

fn cost_section(ui: &mut Ui, p: &Project, m: &Model, st: &mut BomUiState) {
    let tk = theme::tokens(ui);
    section(ui, t("bom.cost_heading"));
    let lines = bom::build_lines(p, m);
    let costed = cost::cost(&lines, &st.overrides);
    egui::Grid::new("bom_cost_grid")
        .striped(true)
        .num_columns(7)
        .show(ui, |ui| {
            for h in [
                t("bom.col.part"),
                t("bom.col.qty"),
                t("bom.col.unit_mass_g"),
                t("bom.col.total_mass_g"),
                t("bom.col.basis"),
                t("bom.col.unit_price"),
                t("bom.col.line_total"),
            ] {
                ui.label(egui::RichText::new(h).strong());
            }
            ui.end_row();
            for l in &costed.lines {
                ui.label(&l.line.name);
                ui.label(l.line.qty.to_string());
                ui.label(fmt(l.line.unit_mass_g, 1));
                ui.label(fmt(l.total_mass_g, 1));
                ui.label(cost::basis_label(l.basis));
                ui.horizontal(|ui| {
                    let mut v = l.unit_price;
                    let r = ui.add(
                        egui::DragValue::new(&mut v)
                            .speed(0.1)
                            .range(0.0..=1.0e6)
                            .fixed_decimals(2)
                            .suffix(format!(" {CURRENCY}")),
                    );
                    if r.changed() {
                        st.overrides.insert(l.line.key.to_string(), v);
                    }
                    let tag = if l.overridden {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(t("bom.source.override")).color(tk.accent),
                            )
                            .sense(Sense::click()),
                        )
                    } else {
                        ui.colored_label(tk.text_weak, t("bom.source.example"))
                    };
                    if l.overridden && tag.on_hover_text(t("bom.reset_line")).clicked() {
                        st.overrides.remove(l.line.key);
                    }
                });
                ui.label(format!("{} {CURRENCY}", fmt(l.line_total, 2)));
                ui.end_row();
            }
            ui.label(egui::RichText::new(t("bom.total")).strong());
            ui.label("");
            ui.label("");
            ui.label(egui::RichText::new(fmt(costed.total_mass_g, 1)).strong());
            ui.label("");
            ui.label("");
            ui.label(
                egui::RichText::new(format!("{} {CURRENCY}", fmt(costed.grand_total, 2))).strong(),
            );
            ui.end_row();
        });
    cost_footer(ui, p, m, st, &lines, &costed);
}

fn cost_footer(
    ui: &mut Ui,
    p: &Project,
    m: &Model,
    st: &mut BomUiState,
    lines: &[cost::BomLine],
    costed: &CostedBom,
) {
    let tk = theme::tokens(ui);
    ui.add_space(space::XS);
    ui.colored_label(
        tk.text_weak,
        tf(
            "bom.mass_check_fmt",
            &[
                fmt(bom::budget_mass_g(lines), 1),
                fmt(bom::model_mtow_g(p, m), 1),
            ],
        ),
    );
    ui.colored_label(tk.text_weak, t("bom.fastener_note"));
    ui.horizontal(|ui| {
        if ui.button(t("bom.export_csv")).clicked() {
            let name = format!("{}_BOM.csv", p.name.replace(' ', "_"));
            save_text(st, &name, ("CSV (*.csv)", &["csv"]), cost::bom_csv(costed));
        }
        if ui
            .add_enabled(
                !st.overrides.is_empty(),
                egui::Button::new(t("bom.reset_prices")),
            )
            .clicked()
        {
            st.overrides.clear();
        }
    });
}

fn nest_section(ui: &mut Ui, p: &Project, m: &Model, st: &mut BomUiState) {
    let tk = theme::tokens(ui);
    section(ui, t("bom.nest_heading"));
    ui.colored_label(tk.text_weak, t("bom.nest_intro"));
    let key = cache_key(p);
    if st.cache.as_ref().map(|c| c.0) != Some(key) {
        st.cache = Some((key, flat_parts(p, m)));
    }
    let mut parts: Vec<FlatPart> = st.cache.as_ref().map(|c| c.1.clone()).unwrap_or_default();
    for pt in &mut parts {
        if pt.material == SheetMaterial::CarbonPlate {
            pt.material = st.material;
        }
    }

    ui.horizontal_wrapped(|ui| {
        let s = &mut st.spec;
        ui.label(t("bom.sheet_w"));
        ui.add(
            egui::DragValue::new(&mut s.width_mm)
                .range(20.0..=3000.0)
                .suffix(" mm"),
        );
        ui.label(t("bom.sheet_h"));
        ui.add(
            egui::DragValue::new(&mut s.height_mm)
                .range(20.0..=3000.0)
                .suffix(" mm"),
        );
        ui.label(t("bom.spacing"));
        ui.add(
            egui::DragValue::new(&mut s.spacing_mm)
                .range(0.0..=50.0)
                .speed(0.1)
                .suffix(" mm"),
        );
        ui.label(t("bom.margin"));
        ui.add(
            egui::DragValue::new(&mut s.margin_mm)
                .range(0.0..=100.0)
                .speed(0.1)
                .suffix(" mm"),
        );
        ui.checkbox(&mut s.allow_rotate, t("bom.allow_rotate"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(t("bom.material"));
        egui::ComboBox::from_id_salt("bom_sheet_material")
            .selected_text(st.material.label())
            .show_ui(ui, |ui| {
                for mat in SheetMaterial::ALL {
                    ui.selectable_value(&mut st.material, mat, mat.label());
                }
            });
        ui.label(t("bom.dxf_style"));
        egui::ComboBox::from_id_salt("bom_dxf_style")
            .selected_text(style_label(st.style))
            .show_ui(ui, |ui| {
                for s in [DxfStyle::R12, DxfStyle::R14] {
                    ui.selectable_value(&mut st.style, s, style_label(s));
                }
            });
    });

    let res = nest(&parts, &st.spec);
    report(ui, &parts, &res);
    if res.sheets > 0 {
        st.sheet_view = st.sheet_view.min(res.sheets - 1);
        if res.sheets > 1 {
            ui.horizontal_wrapped(|ui| {
                for i in 0..res.sheets {
                    let label = match res.placements.iter().find(|pl| pl.sheet == i) {
                        Some(pl) => {
                            let pt = &parts[pl.part];
                            format!(
                                "{} ({}, {} mm)",
                                tf("bom.sheet_n_fmt", &[(i + 1).to_string()]),
                                pt.material.label(),
                                fmt(pt.thickness_mm, 1),
                            )
                        }
                        None => tf("bom.sheet_n_fmt", &[(i + 1).to_string()]),
                    };
                    ui.selectable_value(&mut st.sheet_view, i, label);
                }
            });
        }
        draw_sheet(ui, &parts, &res, &st.spec, st.sheet_view);
    }
    if ui
        .add_enabled(res.sheets > 0, egui::Button::new(t("bom.export_dxf")))
        .clicked()
    {
        let name = format!("{}_nesting.dxf", p.name.replace(' ', "_"));
        let body = write_dxf(&parts, &res, &st.spec, st.style);
        save_text(st, &name, ("DXF (*.dxf)", &["dxf"]), body);
    }
}

fn style_label(s: DxfStyle) -> &'static str {
    match s {
        DxfStyle::R12 => t("bom.dxf_r12"),
        DxfStyle::R14 => t("bom.dxf_r14"),
    }
}

fn report(ui: &mut Ui, parts: &[FlatPart], res: &NestResult) {
    let tk = theme::tokens(ui);
    let n_placed = res.placements.len();
    let cut_mass_g: f64 = res
        .placements
        .iter()
        .map(|pl| {
            let pt = &parts[pl.part];
            pt.net_area_mm2() * pt.thickness_mm * 1e-9 * pt.material.density_kg_m3() * 1000.0
        })
        .sum();
    ui.colored_label(
        tk.text,
        tf(
            "bom.nest_result_fmt",
            &[
                n_placed.to_string(),
                res.sheets.to_string(),
                fmt(res.total_usage_pct, 1),
                fmt(cut_mass_g, 1),
            ],
        ),
    );
    if !res.unplaced.is_empty() {
        let names: Vec<String> = res
            .unplaced
            .iter()
            .map(|u| match u.size {
                Some((w, h)) => format!("{} ({}x{} mm)", parts[u.part].name, fmt(w, 0), fmt(h, 0)),
                None => format!("{} ({})", parts[u.part].name, t("bom.invalid_outline")),
            })
            .collect();
        components::banner(
            ui,
            Status::Warn,
            &tf(
                "bom.unplaced_fmt",
                &[res.unplaced.len().to_string(), names.join(", ")],
            ),
        );
    }
}

fn draw_sheet(ui: &mut Ui, parts: &[FlatPart], res: &NestResult, spec: &SheetSpec, view: usize) {
    let tk = theme::tokens(ui);
    let avail_w = ui.available_width().clamp(200.0, 720.0);
    let aspect = (spec.height_mm / spec.width_mm).clamp(0.1, 10.0) as f32;
    let h = (avail_w * aspect).min(520.0);
    let (resp, painter) = ui.allocate_painter(vec2(avail_w, h), Sense::hover());
    let rect = resp.rect.shrink(4.0);
    painter.rect_filled(resp.rect, theme::corner(theme::radius::M), tk.viewport_bg);
    let scale =
        (rect.width() as f64 / spec.width_mm).min(rect.height() as f64 / spec.height_mm) as f32;
    let origin = Pos2::new(rect.left(), rect.bottom());
    let to_px = |p: [f64; 2]| {
        Pos2::new(
            origin.x + p[0] as f32 * scale,
            origin.y - p[1] as f32 * scale,
        )
    };
    let sheet = Rect::from_two_pos(to_px([0.0, 0.0]), to_px([spec.width_mm, spec.height_mm]));
    painter.rect_filled(sheet, 0.0, tk.surface);
    painter.rect_stroke(
        sheet,
        0.0,
        Stroke::new(1.5, tk.border_strong),
        egui::StrokeKind::Inside,
    );
    for pl in res.placements.iter().filter(|pl| pl.sheet == view) {
        let part = &parts[pl.part];
        for (i, e) in part
            .placed([pl.x, pl.y], pl.rotated)
            .into_iter()
            .enumerate()
        {
            match e {
                Entity::Polygon(pts) => {
                    let px: Vec<Pos2> = pts.iter().map(|q| to_px(*q)).collect();
                    // Dolgu yalnız dışbükey (dikdörtgen) parçalarda; kaburga hatları çizgiyle.
                    if px.len() == 4 {
                        painter.add(Shape::convex_polygon(
                            px.clone(),
                            tk.accent_soft,
                            Stroke::NONE,
                        ));
                    }
                    painter.add(Shape::closed_line(px, Stroke::new(1.2, tk.accent)));
                }
                Entity::Circle { c, r } if i == 0 => {
                    let (cp, rp) = (to_px(c), r as f32 * scale);
                    painter.circle_filled(cp, rp, tk.accent_soft);
                    painter.circle_stroke(cp, rp, Stroke::new(1.2, tk.accent));
                }
                Entity::Circle { c, r } => {
                    painter.circle_stroke(
                        to_px(c),
                        r as f32 * scale,
                        Stroke::new(1.0, tk.text_weak),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;

    fn render(ctx: &egui::Context, p: &Project, m: &Model, st: &mut BomUiState) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0)));
        let mut out = ctx.run_ui(input, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| show(ui, p, m, st));
        });
        out.textures_delta.clear();
    }

    fn all_projects() -> Vec<Project> {
        let mut v = Vec::new();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            v.push(p);
        }
        for src in [
            include_str!("../../examples/fixed_wing_2kg_1600mm.json"),
            include_str!("../../examples/flying_wing_1200mm.json"),
            include_str!("../../examples/hexa_550_x.json"),
            include_str!("../../examples/quad_250_x.json"),
            include_str!("../../examples/quad_275_x.json"),
            include_str!("../../examples/vtol_quadplane.json"),
        ] {
            v.push(Project::from_json(src).expect("örnek proje"));
        }
        v
    }

    #[test]
    fn bom_screen_renders_for_every_type_and_sample_in_both_themes_and_languages() {
        let lib = Library::embedded();
        for p in all_projects() {
            let m = evaluate(&p, &lib);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    let ctx = egui::Context::default();
                    theme::install(&ctx, choice);
                    let mut st = BomUiState::default();
                    st.overrides.insert("payload".into(), 5.0);
                    crate::i18n::with_lang(lang, || {
                        // Dar levha: birden çok levha ve sığmayan parça uyarısı da çizilir.
                        for (w, h) in [(300.0, 300.0), (60.0, 60.0)] {
                            st.spec.width_mm = w;
                            st.spec.height_mm = h;
                            for _ in 0..2 {
                                render(&ctx, &p, &m, &mut st);
                            }
                        }
                    });
                }
            }
        }
    }

    fn find_text_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
        for cs in shapes {
            match &cs.shape {
                Shape::Text(t) if t.galley.text() == needle => {
                    return Some(t.visual_bounding_rect().center());
                }
                Shape::Vec(v) => {
                    for s in v {
                        if let Shape::Text(t) = s
                            && t.galley.text() == needle
                        {
                            return Some(t.visual_bounding_rect().center());
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn clicking_override_tag_resets_single_line_override() {
        let lib = Library::embedded();
        let p = Project::default();
        let m = evaluate(&p, &lib);
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        let mut st = BomUiState::default();
        st.overrides.insert("payload".into(), 12.0);
        st.overrides.insert("avionics".into(), 25.0);

        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 800.0));
        let mut pos = None;
        for _ in 0..2 {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input.clone(), |ui| show(ui, &p, &m, &mut st));
            pos = find_text_center(&out.shapes, t("bom.source.override"));
            out.textures_delta.clear();
            let _ = &mut input;
        }
        let target = pos.expect("override etiketi çizilmeli");

        for pressed in [true, false] {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![
                    egui::Event::PointerMoved(target),
                    egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| show(ui, &p, &m, &mut st));
            out.textures_delta.clear();
        }
        assert_eq!(st.overrides.len(), 1);
    }
}
