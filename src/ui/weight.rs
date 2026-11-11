// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım 2 / Sekme 2: Ağırlık Dökümü: girdiler sol panel, sonuçlar sağ panel, kütle payı
//! grafiği merkez görünümde. Alanlar, aralıklar, ölçekler ve ondalıklar eski `fld` satırlarıyla
//! birebir aynıdır; hesap sonucu değişmez.
use super::components::{NumberInput, Status, number_row, section::section_header, u32_row};
use super::step_ui::{self, CardExt, card, cards};
use super::{theme, widgets::fmt};
use crate::calc::custom_battery::{
    BatterySourceKind, CustomCellData, CustomPackData, GenericBatteryTemplate,
};
use crate::calc::{Library, Model};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::{Sense, Shape, Stroke, Ui};

fn g_row(ui: &mut Ui, label: &str, v: &mut f64, hi: f64, help: &str) {
    number_row(
        ui,
        NumberInput::new(label, v)
            .scale(1000.0)
            .unit("g")
            .range(0.0, hi)
            .decimals(0)
            .help(help),
    );
}

/// Sol panel: bileşen kütleleri, batarya, yapı tahmini girdileri.
pub fn inputs(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    section_header(ui, t("weight.section_comp_masses"));
    g_row(
        ui,
        t("weight.input.payload"),
        &mut p.m_payload,
        1500.0,
        t("weight.input.payload.desc"),
    );
    g_row(
        ui,
        t("weight.input.avionics"),
        &mut p.m_avionics,
        800.0,
        t("weight.input.avionics.desc"),
    );
    g_row(
        ui,
        t("weight.input.propulsion"),
        &mut p.m_propulsion,
        1000.0,
        t("weight.input.propulsion.desc"),
    );
    g_row(
        ui,
        t("weight.input.fuse_tail"),
        &mut p.m_fuse_tail,
        1500.0,
        t("weight.input.fuse_tail.desc"),
    );

    render_battery_source_ui(ui, p, m, lib);

    section_header(ui, t("weight.section_structure_est"));
    let mut sel = p.material_name.clone();
    let mats: Vec<(String, String)> = lib
        .materials
        .iter()
        .filter(|x| x.category == "plastic")
        .map(|mat| {
            (
                mat.name.clone(),
                format!("{} ({:.0} kg/m³)", mat.name, mat.density_kg_m3),
            )
        })
        .collect();
    step_ui::field_label(ui, t("weight.material_label"), "");
    egui::ComboBox::from_id_salt("w_material")
        .selected_text(&sel)
        .width((ui.available_width() - 8.0).max(40.0))
        .truncate()
        .show_ui(ui, |ui| {
            for (name, label) in &mats {
                ui.selectable_value(&mut sel, name.clone(), label);
            }
        });
    if sel != p.material_name {
        if let Some(mat) = lib.material(&sel) {
            p.rho_mat = mat.density_kg_m3;
        }
        p.material_name = sel;
    }
    if let Some(mat) = lib.material(&p.material_name) {
        step_ui::note(ui, &mat.notes);
    }
    number_row(
        ui,
        NumberInput::new(t("weight.input.rho_mat"), &mut p.rho_mat)
            .unit("kg/m³")
            .range(100.0, 2000.0)
            .decimals(0)
            .help(t("weight.input.rho_mat.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.t_wall"), &mut p.t_wall)
            .scale(1000.0)
            .unit("mm")
            .range(0.2, 1.5)
            .decimals(2)
            .help(t("weight.input.t_wall.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.k_infill"), &mut p.k_infill)
            .unit("×")
            .range(1.0, 2.0)
            .decimals(2)
            .help(t("weight.input.k_infill.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.rho_cf"), &mut p.rho_cf)
            .unit("kg/m³")
            .range(1000.0, 2800.0)
            .decimals(0)
            .help(t("weight.input.rho_cf.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.d_spar_o"), &mut p.d_spar_o)
            .scale(1000.0)
            .unit("mm")
            .range(4.0, 30.0)
            .decimals(1)
            .help(t("weight.input.d_spar_o.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.d_spar_i"), &mut p.d_spar_i)
            .scale(1000.0)
            .unit("mm")
            .range(0.0, 28.0)
            .decimals(1)
            .help(t("weight.input.d_spar_i.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("weight.input.l_spar"), &mut p.l_spar)
            .scale(1000.0)
            .unit("mm")
            .range(200.0, 2500.0)
            .decimals(0)
            .help(t("weight.input.l_spar.desc")),
    );
    if p.d_spar_i >= p.d_spar_o {
        step_ui::status_text(ui, Status::Err, t("weight.id_lt_od_err"));
    }
}

/// Batarya paketi sonuç kartları (sabit kanat ağırlık ekranı).
fn battery_cards(ui: &mut Ui, p: &Project, m: &Model) {
    let w = &m.weight;
    cards(
        ui,
        &[
            card(
                t("weight.out.n_cells"),
                format!("{}", w.n_cells),
                "",
                t("weight.out.n_cells.desc"),
            )
            .d(format!("({}S{}P)", p.n_series, p.n_parallel)),
            card(
                t("weight.out.pack_voltage"),
                fmt(w.pack_voltage, 1),
                "V",
                t("weight.out.pack_voltage.desc"),
            )
            .d(t("weight.unit_v_nominal")),
            card(
                t("weight.out.pack_capacity"),
                fmt(w.pack_capacity_ah, 1),
                "Ah",
                t("weight.out.pack_capacity.desc"),
            ),
            card(
                t("weight.out.pack_energy"),
                fmt(w.pack_wh, 0),
                "Wh",
                t("weight.out.pack_energy.desc"),
            ),
            card(
                t("weight.out.m_battery"),
                fmt(w.m_battery * 1000.0, 0),
                "g",
                t("weight.out.m_battery.desc"),
            ),
            card(
                t("weight.out.e_pack_wh_kg"),
                fmt(w.e_pack_wh_kg, 0),
                "Wh/kg",
                t("weight.out.e_pack_wh_kg.desc"),
            ),
        ],
    );
}

/// Kütle payları (grafik ve sağ panel listesi aynı veriyi kullanır).
fn shares(p: &Project, m: &Model) -> [(&'static str, f64); 6] {
    let w = &m.weight;
    [
        (t("weight.item_battery"), w.m_battery),
        (t("weight.item_payload"), p.m_payload),
        (t("weight.item_avionics"), p.m_avionics),
        (t("weight.item_propulsion"), p.m_propulsion),
        (t("weight.item_wing_struct"), w.m_wing),
        (t("weight.item_fuse_tail"), p.m_fuse_tail),
    ]
}

fn palette(tk: &theme::Tokens) -> [egui::Color32; 6] {
    [
        tk.accent,
        tk.warning,
        tk.success,
        tk.danger,
        tk.info,
        tk.text_weak,
    ]
}

fn legend(ui: &mut Ui, items: &[(&str, f64); 6], total: f64) {
    let tk = theme::tokens(ui);
    let pal = palette(tk);
    for (i, (nm, v)) in items.iter().enumerate() {
        let pct = if total > 0.0 { *v / total * 100.0 } else { 0.0 };
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, 2.0, pal[i % pal.len()]);
            ui.add(
                egui::Label::new(format!("{nm}: {:.0} g (%{:.0})", *v * 1000.0, pct)).truncate(),
            );
        });
    }
}

/// Sağ panel: batarya, yapı ve toplam kütle sonuçları.
pub fn results(ui: &mut Ui, p: &mut Project, m: &Model) {
    section_header(ui, t("weight.section_battery_pack"));
    battery_cards(ui, p, m);
    battery_formulas(ui, p, m);

    section_header(ui, t("weight.section_structure_est"));
    let w = &m.weight;
    cards(
        ui,
        &[
            card(
                t("weight.out.a_wet_w"),
                fmt(w.a_wet_w, 3),
                "m²",
                t("weight.out.a_wet_w.desc"),
            ),
            card(
                t("weight.out.m_shell"),
                fmt(w.m_shell * 1000.0, 0),
                "g",
                t("weight.out.m_shell.desc"),
            ),
            card(
                t("weight.out.m_spar"),
                fmt(w.m_spar * 1000.0, 1),
                "g",
                t("weight.out.m_spar.desc"),
            ),
            card(
                t("weight.out.m_wing"),
                fmt(w.m_wing * 1000.0, 0),
                "g",
                t("weight.out.m_wing.desc"),
            ),
            card(
                t("weight.out.m_struct"),
                fmt(w.m_struct * 1000.0, 0),
                "g",
                t("weight.out.m_struct.desc"),
            ),
        ],
    );

    section_header(ui, t("weight.section_total_shares"));
    cards(
        ui,
        &[
            card(
                "MTOW",
                fmt(w.m_mtow * 1000.0, 0),
                "g",
                t("weight.out.mtow.desc"),
            )
            .d(tf(
                "weight.out.mtow_detail_fmt",
                &[fmt(w.m_mtow, 3), fmt(w.w, 1)],
            )),
            card(
                t("weight.out.batt_ratio"),
                fmt(w.f_batt * 100.0, 1),
                "%",
                t("weight.out.batt_ratio.desc"),
            )
            .d(t("weight.batt_ratio_rec"))
            .s(if (0.25..=0.45).contains(&w.f_batt) {
                Status::Ok
            } else {
                Status::Warn
            }),
            card(
                t("weight.out.struct_ratio"),
                fmt(w.f_struct * 100.0, 1),
                "%",
                t("weight.out.struct_ratio.desc"),
            ),
        ],
    );
    legend(ui, &shares(p, m), w.m_mtow);
}

/// Merkez görünüm grafiği: kütle payı pastası + açıklama.
pub fn chart(ui: &mut Ui, p: &Project, m: &Model, h: f32) {
    let tk = theme::tokens(ui);
    let items = shares(p, m);
    let total = m.weight.m_mtow;
    let pal = palette(tk);
    ui.horizontal(|ui| {
        let size = (h - 16.0)
            .clamp(80.0, 420.0)
            .min(ui.available_width() * 0.6);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
        let painter = ui.painter_at(rect);
        let c = rect.center();
        let r = size * 0.46;
        if total.is_finite() && total > 0.0 {
            let mut a0 = -std::f32::consts::FRAC_PI_2;
            for (i, (_, v)) in items.iter().enumerate() {
                let frac = (*v / total).clamp(0.0, 1.0) as f32;
                let a1 = a0 + frac * std::f32::consts::TAU;
                let n_seg = (32.0 * frac).ceil().max(2.0) as usize;
                let mut pts = vec![c];
                for s in 0..=n_seg {
                    let a = a0 + (a1 - a0) * (s as f32 / n_seg as f32);
                    pts.push(c + egui::vec2(a.cos(), a.sin()) * r);
                }
                painter.add(Shape::convex_polygon(
                    pts,
                    pal[i % pal.len()],
                    Stroke::new(1.0, tk.surface),
                ));
                a0 = a1;
            }
        }
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(t("weight.section_total_shares")).strong());
            legend(ui, &items, total);
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Batarya kaynağı (sabit kanat, multirotor ve basit mod batarya bölümü ortak kullanır)
// ---------------------------------------------------------------------------------------------

fn generic_cell_rows(ui: &mut Ui, c: &mut CustomCellData) {
    number_row(
        ui,
        NumberInput::new(t("batt.f.cell_v"), &mut c.nominal_v)
            .unit("V")
            .range(1.0, 5.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.capacity_ah"), &mut c.capacity_ah)
            .unit("Ah")
            .range(0.1, 50.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.cell_mass"), &mut c.mass_g)
            .unit("g")
            .range(5.0, 1000.0)
            .decimals(1),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.max_cont_a"), &mut c.max_cont_a)
            .unit("A")
            .range(1.0, 500.0)
            .decimals(0),
    );
}

fn text_row(ui: &mut Ui, label: &str, s: &mut String) {
    step_ui::field_label(ui, label, "");
    ui.add(egui::TextEdit::singleline(s).desired_width((ui.available_width() - 8.0).max(40.0)));
}

fn custom_cell_rows(ui: &mut Ui, c: &mut CustomCellData) {
    text_row(ui, t("batt.f.cell_name"), &mut c.name);
    text_row(ui, t("batt.f.chemistry"), &mut c.chemistry);
    number_row(
        ui,
        NumberInput::new(t("batt.f.nominal_v"), &mut c.nominal_v)
            .unit("V")
            .range(1.0, 5.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.capacity_ah"), &mut c.capacity_ah)
            .unit("Ah")
            .range(0.1, 100.0)
            .decimals(2),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.cell_mass"), &mut c.mass_g)
            .unit("g")
            .range(5.0, 2000.0)
            .decimals(1),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.max_cont_a"), &mut c.max_cont_a)
            .unit("A")
            .range(1.0, 500.0)
            .decimals(0),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.r_int"), &mut c.r_int_mohm)
            .unit("mΩ")
            .range(0.0, 100.0)
            .decimals(1)
            .help(t("batt.f.r_int.desc")),
    );
}

fn custom_pack_rows(ui: &mut Ui, k: &mut CustomPackData) {
    text_row(ui, t("batt.f.pack_name"), &mut k.name);
    text_row(ui, t("batt.f.chemistry"), &mut k.chemistry);
    number_row(
        ui,
        NumberInput::new(t("batt.f.pack_voltage"), &mut k.pack_voltage)
            .unit("V")
            .range(3.0, 100.0)
            .decimals(1),
    );
    if u32_row(
        ui,
        t("batt.f.s_count"),
        &mut k.s_count,
        1,
        24,
        t("batt.f.s_count.desc"),
    ) {
        k.pack_voltage = k.s_count as f64 * 3.7;
    }
    number_row(
        ui,
        NumberInput::new(t("batt.f.capacity_mah"), &mut k.capacity_mah)
            .unit("mAh")
            .range(100.0, 100000.0)
            .decimals(0),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.pack_mass"), &mut k.pack_mass_g)
            .unit("g")
            .range(20.0, 10000.0)
            .decimals(0)
            .help(t("batt.f.pack_mass.desc")),
    );
    number_row(
        ui,
        NumberInput::new(t("batt.f.max_cont_a"), &mut k.max_cont_a)
            .unit("A")
            .range(1.0, 1000.0)
            .decimals(0),
    );
    if number_row(
        ui,
        NumberInput::new(t("batt.f.c_rating"), &mut k.c_rating)
            .unit("C")
            .range(1.0, 200.0)
            .decimals(0)
            .help(t("batt.f.c_rating.desc")),
    ) {
        k.max_cont_a = k.c_rating * (k.capacity_mah / 1000.0);
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn save_row(
    ui: &mut Ui,
    summary: String,
    item: impl FnOnce() -> crate::calc::custom_battery::SavedCustomBattery,
) {
    step_ui::text(ui, &summary);
    if ui.button(t("batt.btn_save_to_my_list")).clicked() {
        let _ = crate::calc::custom_battery::save_custom_battery(item());
    }
}

/// Sol panel: batarya kaynağı, hücre/paket alanları, kayıtlı bataryalar, S × P ve paket ek yükü.
pub fn render_battery_source_ui(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    section_header(ui, t("weight.section_battery_pack"));
    let src_items = [
        (
            BatterySourceKind::Library,
            format!("[{}] {}", t("batt.tag_community"), t("batt.source_library")),
        ),
        (
            BatterySourceKind::GenericTemplate,
            format!("[{}] {}", t("batt.tag_generic"), t("batt.source_generic")),
        ),
        (
            BatterySourceKind::CustomCell,
            format!("[{}] {}", t("batt.tag_my"), t("batt.source_custom_cell")),
        ),
        (
            BatterySourceKind::CustomPack,
            format!("[{}] {}", t("batt.tag_my"), t("batt.source_custom_pack")),
        ),
    ];
    let items: Vec<(BatterySourceKind, &str)> =
        src_items.iter().map(|(k, s)| (*k, s.as_str())).collect();
    step_ui::combo(
        ui,
        "batt_source",
        t("batt.source_label"),
        "",
        &mut p.battery_source,
        &items,
    );

    match p.battery_source {
        BatterySourceKind::Library => {
            let mut sel = p.cell_name.clone();
            step_ui::field_label(ui, t("weight.cell_label"), "");
            egui::ComboBox::from_id_salt("batt_cell")
                .selected_text(format!("[{}] {}", t("batt.tag_community"), sel))
                .width((ui.available_width() - 8.0).max(40.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for c in &lib.cells {
                        let display_text = format!("[{}] {}", t("batt.tag_community"), c.name);
                        ui.selectable_value(&mut sel, c.name.clone(), display_text);
                    }
                });
            p.cell_name = sel;
            let db_status = match lib.db_info.source {
                crate::db::DbSource::Embedded => t("about.db_embedded"),
                crate::db::DbSource::Community => t("about.db_community"),
            };
            step_ui::note(
                ui,
                &tf("batt.db_status_fmt", &[db_status, &lib.db_info.updated]),
            );
            step_ui::note(
                ui,
                &tf(
                    "weight.cell_info_fmt",
                    &[
                        format!("{:.1}", m.cell.capacity_ah),
                        format!("{:.2}", m.cell.nominal_v),
                        format!("{:.1}", m.cell.mass_g),
                        format!("{:.0}", m.cell.max_cont_a),
                        format!("{:.0}", m.cell.wh_per_kg()),
                    ],
                ),
            );
        }
        BatterySourceKind::GenericTemplate => {
            let prev_tpl = p.generic_battery_template;
            step_ui::combo(
                ui,
                "generic_batt_tpl_cb",
                t("batt.template_label"),
                "",
                &mut p.generic_battery_template,
                &[
                    (GenericBatteryTemplate::LiIon21700, t("batt.template_21700")),
                    (GenericBatteryTemplate::LiIon18650, t("batt.template_18650")),
                    (GenericBatteryTemplate::LiPoGeneric, t("batt.template_lipo")),
                    (
                        GenericBatteryTemplate::LiIonSemiSolid,
                        t("batt.template_semisolid"),
                    ),
                ],
            );
            if p.generic_battery_template != prev_tpl {
                p.generic_cell = p.generic_battery_template.default_cell();
            }
            generic_cell_rows(ui, &mut p.generic_cell);
            if p.generic_battery_template == GenericBatteryTemplate::LiPoGeneric
                && ui.button(t("batt.btn_apply_150")).clicked()
            {
                let wh = p.generic_cell.nominal_v * p.generic_cell.capacity_ah;
                p.generic_cell.mass_g = (wh / 0.150).round();
            }
            let cell = p.generic_cell.clone();
            save_row(
                ui,
                tf(
                    "batt.calc_specific_fmt",
                    &[format!("{:.0}", p.generic_cell.wh_per_kg())],
                ),
                || crate::calc::custom_battery::SavedCustomBattery {
                    id: format!("gen_{}", now_ms()),
                    name: cell.name.clone(),
                    is_pack: false,
                    cell: Some(cell),
                    pack: None,
                },
            );
        }
        BatterySourceKind::CustomCell => {
            custom_cell_rows(ui, &mut p.custom_cell);
            let cell = p.custom_cell.clone();
            save_row(
                ui,
                tf(
                    "batt.calc_specific_fmt",
                    &[format!("{:.0}", p.custom_cell.wh_per_kg())],
                ),
                || crate::calc::custom_battery::SavedCustomBattery {
                    id: format!("cell_{}", now_ms()),
                    name: cell.name.clone(),
                    is_pack: false,
                    cell: Some(cell),
                    pack: None,
                },
            );
        }
        BatterySourceKind::CustomPack => {
            custom_pack_rows(ui, &mut p.custom_pack);
            let pack = p.custom_pack.clone();
            save_row(
                ui,
                tf(
                    "batt.calc_pack_fmt",
                    &[
                        format!("{:.1}", p.custom_pack.wh()),
                        format!("{:.0}", p.custom_pack.wh_per_kg()),
                    ],
                ),
                || crate::calc::custom_battery::SavedCustomBattery {
                    id: format!("pack_{}", now_ms()),
                    name: pack.name.clone(),
                    is_pack: true,
                    cell: None,
                    pack: Some(pack),
                },
            );
        }
    }

    // Kayıtlı özel bataryalar ("Benim Bataryalarım")
    let saved = crate::calc::custom_battery::load_saved_batteries();
    if !saved.is_empty() {
        ui.add_space(4.0);
        let head = tf("batt.my_batteries_fmt", &[saved.len().to_string()]);
        let col = theme::tokens(ui).text;
        egui::CollapsingHeader::new(step_ui::short_header(ui, &head, col, false))
            .id_salt("batt_my_list")
            .default_open(false)
            .show(ui, |ui| {
                for item in saved {
                    let tag = t("batt.tag_my");
                    let desc = match (&item.pack, &item.cell) {
                        (Some(pk), _) if item.is_pack => tf(
                            "batt.saved_pack_fmt",
                            &[
                                tag.to_string(),
                                item.name.clone(),
                                format!("{:.1}", pk.pack_voltage),
                                format!("{:.0}", pk.capacity_mah),
                                format!("{:.0}", pk.pack_mass_g),
                            ],
                        ),
                        (_, Some(cl)) => tf(
                            "batt.saved_cell_fmt",
                            &[
                                tag.to_string(),
                                item.name.clone(),
                                format!("{:.1}", cl.nominal_v),
                                format!("{:.1}", cl.capacity_ah),
                                format!("{:.0}", cl.mass_g),
                            ],
                        ),
                        _ => format!("[{tag}] {}", item.name),
                    };
                    step_ui::text(ui, &desc);
                    ui.horizontal(|ui| {
                        if ui.button(t("batt.btn_use")).clicked() {
                            if item.is_pack {
                                if let Some(pk) = item.pack.clone() {
                                    p.custom_pack = pk;
                                    p.battery_source = BatterySourceKind::CustomPack;
                                }
                            } else if let Some(cl) = item.cell.clone() {
                                p.custom_cell = cl;
                                p.battery_source = BatterySourceKind::CustomCell;
                            }
                        }
                        if ui.button(t("btn_delete")).clicked() {
                            let _ = crate::calc::custom_battery::delete_saved_battery(&item.id);
                        }
                    });
                    ui.separator();
                }
            });
    }

    // S × P ve paket ek yükü (hazır paket dışında)
    if p.battery_source != BatterySourceKind::CustomPack {
        u32_row(
            ui,
            t("batt.f.n_series"),
            &mut p.n_series,
            1,
            14,
            t("weight.input.n_series.desc"),
        );
        u32_row(
            ui,
            t("batt.f.n_parallel"),
            &mut p.n_parallel,
            1,
            10,
            t("weight.input.n_parallel.desc"),
        );
        number_row(
            ui,
            NumberInput::new(t("weight.input.pack_overhead"), &mut p.pack_overhead)
                .unit("×")
                .range(1.0, 1.6)
                .decimals(2)
                .help(t("weight.input.pack_overhead.desc")),
        );
    }

    step_ui::info_box(
        ui,
        "batt_why",
        t("batt.why_heading"),
        &[
            t("batt.why_text_1"),
            t("batt.why_text_2"),
            t("batt.why_text_3"),
        ],
    );
}

/// Sağ panel: batarya formülleri (anlık değerlerle).
pub fn battery_formulas(ui: &mut Ui, p: &Project, m: &Model) {
    let cap_ah = m.weight.pack_capacity_ah;
    let v_nom = m.weight.pack_voltage;
    let wh = m.weight.pack_wh;
    let m_kg = m.weight.m_battery;
    let wh_kg = m.weight.e_pack_wh_kg;
    let max_a = (p.n_parallel as f64) * m.cell.max_cont_a;
    let body = tf(
        "batt.formula_lines_fmt",
        &[
            format!("{v_nom:.1}"),
            format!("{cap_ah:.2}"),
            format!("{wh:.1}"),
            format!("{wh:.1}"),
            format!("{m_kg:.3}"),
            format!("{wh_kg:.0}"),
            format!("{max_a:.0}"),
            format!("{:.1}", if cap_ah > 0.0 { max_a / cap_ah } else { 0.0 }),
        ],
    );
    step_ui::formula_box(ui, "batt_formulas", t("batt.formulas_heading"), &body);
}

/// Girdiler + sonuçlar + grafik alt alta (başsız testler için).
pub fn show(ui: &mut Ui, p: &mut Project, m: &Model, lib: &Library) {
    inputs(ui, p, m, lib);
    results(ui, p, m);
    chart(ui, p, m, 200.0);
}
