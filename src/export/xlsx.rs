// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel (.xlsx) dışa aktarma modülü.
//! Canlı formüllerle proje girdileri, mühendislik hesaplamaları ve özet sayfaları üretir.

use super::texts::loc;
use crate::calc::{Model, Sev};
use crate::i18n::{Lang, t_lang, tf_lang};
use crate::project::{DroneType, Project};
use rust_xlsxwriter::*;
use std::collections::HashMap;
use std::path::Path;

/// Hücre koordinatını ve başka sayfadan başvuru dizgisini yönetir.
#[derive(Clone, Debug)]
pub struct CellCoord {
    pub sheet: String,
    pub row: u32,          // 1-tabanlı Excel satır no
    pub col: &'static str, // "D"
}

impl CellCoord {
    pub fn abs(&self) -> String {
        format!("${}${}", self.col, self.row)
    }

    pub fn to_ref(&self, cur_sheet: &str) -> String {
        if self.sheet == cur_sheet {
            self.abs()
        } else {
            format!("'{}'!{}", self.sheet, self.abs())
        }
    }
}

/// Formül değişkenlerini ilgili hücre koordinatlarına eşleyen kayıt defteri.
#[derive(Default)]
pub struct CellRegistry {
    map: HashMap<String, CellCoord>,
    /// (sayfa, satır) -> sayısal değer (girdiler, program değerleri ve formül önbellekleri).
    values: HashMap<(String, u32), f64>,
    /// Yazılan formüller: (kimlik, çözümlenmiş formül, sayfa, satır, önbellek değeri).
    formulas: Vec<FormulaRec>,
    /// Dosyaya yazılan metinler (ad/not); dil testleri için.
    texts: Vec<String>,
}

/// Yazılan bir formül hücresinin kaydı.
#[derive(Clone, Debug)]
pub struct FormulaRec {
    pub id: String,
    pub formula: String,
    pub sheet: String,
    pub row: u32,
    pub cached: f64,
}

impl CellRegistry {
    pub fn register(&mut self, id: &str, sheet: &str, row: u32, col: &'static str) {
        self.map.insert(
            id.to_string(),
            CellCoord {
                sheet: sheet.to_string(),
                row,
                col,
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&CellCoord> {
        self.map.get(id)
    }

    /// Formül şablonundaki `{var}` belirteçlerini geçerli hücre başvurularıyla değiştirir.
    pub fn resolve(&self, cur_sheet: &str, template: &str) -> String {
        let mut out = String::with_capacity(template.len() + 32);
        let mut chars = template.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '{' {
                let mut var_name = String::new();
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                    var_name.push(inner);
                }
                if let Some(coord) = self.map.get(&var_name) {
                    out.push_str(&coord.to_ref(cur_sheet));
                } else {
                    panic!("Bilinmeyen formül değişkeni: {{{}}}", var_name);
                }
            } else {
                out.push(ch);
            }
        }
        out
    }
}

fn current_date_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (secs / 86400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1024 + doe / 1461 - doe / 142401) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn format_cache_val(v: f64) -> String {
    if v.is_finite() {
        format!("{}", v)
    } else {
        "0".to_string()
    }
}

/// Yeniden kullanılabilir hücre biçimleri.
struct Styles {
    title: Format,
    subtitle: Format,
    tbl_header: Format,
    sec_header: Format,
    text: Format,
    text_bold: Format,
    unit: Format,
    symbol: Format,
    note: Format,
    badge_err: Format,
    badge_warn: Format,
    badge_info: Format,

    // Girdi biçimleri (Açık sarı arka plan, düzenlenebilir)
    inp_int: Format,
    inp_dec1: Format,
    inp_dec2: Format,
    inp_dec3: Format,
    inp_dec4: Format,
    inp_pct1: Format,

    // Hesaplama biçimleri (Canlı formül hücreleri)
    calc_int: Format,
    calc_dec1: Format,
    calc_dec2: Format,
    calc_dec3: Format,
    calc_dec4: Format,
    calc_pct1: Format,
}

impl Styles {
    fn new() -> Self {
        let font_family = "Segoe UI";
        let c_navy = Color::RGB(0x1F3864);
        let c_lavender = Color::RGB(0xD9E1F2);
        let c_yellow = Color::RGB(0xFFF2CC);
        let c_gray_border = Color::RGB(0xBFBFBF);
        let c_light_border = Color::RGB(0xE0E0E0);

        let title = Format::new()
            .set_font_name(font_family)
            .set_font_size(14)
            .set_bold()
            .set_font_color(c_navy);

        let subtitle = Format::new()
            .set_font_name(font_family)
            .set_font_size(9)
            .set_italic()
            .set_font_color(Color::RGB(0x595959));

        let tbl_header = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_bold()
            .set_font_color(Color::RGB(0xFFFFFF))
            .set_background_color(c_navy)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_navy);

        let sec_header = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_bold()
            .set_font_color(c_navy)
            .set_background_color(c_lavender)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_gray_border);

        let text = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_light_border);

        let text_bold = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_bold()
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_light_border);

        let unit = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_font_color(Color::RGB(0x404040))
            .set_border(FormatBorder::Thin)
            .set_border_color(c_light_border);

        let symbol = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_italic()
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_light_border);

        let note = Format::new()
            .set_font_name(font_family)
            .set_font_size(9)
            .set_font_color(Color::RGB(0x606060))
            .set_align(FormatAlign::Left)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(c_light_border);

        let badge_err = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_bold()
            .set_font_color(Color::RGB(0xC00000))
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin);

        let badge_warn = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_bold()
            .set_font_color(Color::RGB(0xED7D31))
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin);

        let badge_info = Format::new()
            .set_font_name(font_family)
            .set_font_size(10)
            .set_font_color(Color::RGB(0x0070C0))
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin);

        // Input base helper
        let make_inp = |num_fmt: Option<&'static str>| {
            let mut f = Format::new()
                .set_font_name(font_family)
                .set_font_size(10)
                .set_font_color(Color::RGB(0x002060))
                .set_background_color(c_yellow)
                .set_align(FormatAlign::Right)
                .set_align(FormatAlign::VerticalCenter)
                .set_border(FormatBorder::Thin)
                .set_border_color(c_gray_border);
            if let Some(nf) = num_fmt {
                f = f.set_num_format(nf);
            }
            f
        };

        // Calc base helper
        let make_calc = |num_fmt: Option<&'static str>| {
            let mut f = Format::new()
                .set_font_name(font_family)
                .set_font_size(10)
                .set_align(FormatAlign::Right)
                .set_align(FormatAlign::VerticalCenter)
                .set_border(FormatBorder::Thin)
                .set_border_color(c_light_border);
            if let Some(nf) = num_fmt {
                f = f.set_num_format(nf);
            }
            f
        };

        Self {
            title,
            subtitle,
            tbl_header,
            sec_header,
            text,
            text_bold,
            unit,
            symbol,
            note,
            badge_err,
            badge_warn,
            badge_info,
            inp_int: make_inp(Some("#,##0")),
            inp_dec1: make_inp(Some("0.0")),
            inp_dec2: make_inp(Some("0.00")),
            inp_dec3: make_inp(Some("0.000")),
            inp_dec4: make_inp(Some("0.0000")),
            inp_pct1: make_inp(Some("0.0%")),
            calc_int: make_calc(Some("#,##0")),
            calc_dec1: make_calc(Some("0.0")),
            calc_dec2: make_calc(Some("0.00")),
            calc_dec3: make_calc(Some("0.000")),
            calc_dec4: make_calc(Some("0.0000")),
            calc_pct1: make_calc(Some("0.0%")),
        }
    }
}

/// Sayfa satır oluşturma bağlamı.
struct SheetContext<'a> {
    ws: &'a mut Worksheet,
    row: u32,
    sec_no: u32,
    step_no: u32,
    st: &'a Styles,
    reg: &'a mut CellRegistry,
    sheet_name: &'a str,
    lang: Lang,
}

impl<'a> SheetContext<'a> {
    /// Metni arayüz diline çevirir ve dil testleri için kaydeder.
    fn tx(&mut self, s: &str) -> String {
        let out = loc(self.lang, s);
        self.reg.texts.push(out.clone());
        out
    }

    fn write_sec(&mut self, title: &str, col_count: u16) -> Result<(), XlsxError> {
        self.sec_no += 1;
        self.step_no = 0;
        self.ws.write_string_with_format(
            self.row,
            0,
            format!("{}", self.sec_no),
            &self.st.sec_header,
        )?;
        self.ws
            .write_string_with_format(self.row, 1, title, &self.st.sec_header)?;
        for c in 2..col_count {
            self.ws.write_blank(self.row, c, &self.st.sec_header)?;
        }
        self.row += 1;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_inp(
        &mut self,
        id: &str,
        name: &str,
        sym: &str,
        val: f64,
        fmt: &Format,
        unit: &str,
        note: &str,
    ) -> Result<(), XlsxError> {
        self.step_no += 1;
        let step_str = format!("{}.{}", self.sec_no, self.step_no);
        let name = self.tx(name);
        let note = self.tx(note);
        self.ws
            .write_string_with_format(self.row, 0, step_str, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 1, name, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 2, sym, &self.st.symbol)?;
        self.ws.write_number_with_format(self.row, 3, val, fmt)?;
        self.ws
            .write_string_with_format(self.row, 4, unit, &self.st.unit)?;
        self.ws
            .write_string_with_format(self.row, 5, note, &self.st.note)?;

        self.reg
            .values
            .insert((self.sheet_name.to_string(), self.row + 1), val);
        self.reg.register(id, self.sheet_name, self.row + 1, "D");
        self.row += 1;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_formula(
        &mut self,
        id: &str,
        name: &str,
        sym: &str,
        tpl: &str,
        cached: f64,
        fmt: &Format,
        unit: &str,
        note: &str,
    ) -> Result<(), XlsxError> {
        self.step_no += 1;
        let step_str = format!("{}.{}", self.sec_no, self.step_no);
        let resolved = self.reg.resolve(self.sheet_name, tpl);
        let name = self.tx(name);
        let note = self.tx(note);

        self.ws
            .write_string_with_format(self.row, 0, step_str, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 1, name, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 2, sym, &self.st.symbol)?;

        let f_obj = Formula::new(&resolved).set_result(format_cache_val(cached));
        self.ws.write_formula_with_format(self.row, 3, f_obj, fmt)?;

        self.ws
            .write_string_with_format(self.row, 4, unit, &self.st.unit)?;
        self.ws
            .write_string_with_format(self.row, 5, &resolved, &self.st.note)?;
        self.ws
            .write_string_with_format(self.row, 6, note, &self.st.note)?;

        self.reg
            .values
            .insert((self.sheet_name.to_string(), self.row + 1), cached);
        self.reg.formulas.push(FormulaRec {
            id: id.to_string(),
            formula: resolved,
            sheet: self.sheet_name.to_string(),
            row: self.row + 1,
            cached,
        });
        self.reg.register(id, self.sheet_name, self.row + 1, "D");
        self.row += 1;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_value(
        &mut self,
        id: &str,
        name: &str,
        sym: &str,
        val: f64,
        fmt: &Format,
        unit: &str,
        note: &str,
    ) -> Result<(), XlsxError> {
        self.step_no += 1;
        let step_str = format!("{}.{}", self.sec_no, self.step_no);
        let name = self.tx(name);
        let note = self.tx(note);
        self.ws
            .write_string_with_format(self.row, 0, step_str, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 1, name, &self.st.text)?;
        self.ws
            .write_string_with_format(self.row, 2, sym, &self.st.symbol)?;
        self.ws.write_number_with_format(self.row, 3, val, fmt)?;
        self.ws
            .write_string_with_format(self.row, 4, unit, &self.st.unit)?;
        self.ws.write_string_with_format(
            self.row,
            5,
            t_lang("excel.calculated_by_program", self.lang),
            &self.st.note,
        )?;
        self.ws
            .write_string_with_format(self.row, 6, note, &self.st.note)?;

        self.reg
            .values
            .insert((self.sheet_name.to_string(), self.row + 1), val);
        self.reg.register(id, self.sheet_name, self.row + 1, "D");
        self.row += 1;
        Ok(())
    }
}

/// Projeyi ve hesaplanan model sonuçlarını tek bir `.xlsx` çalışma kitabına döker.
pub fn generate_xlsx_workbook(
    project: &Project,
    model: &Model,
    lang: Lang,
) -> Result<Workbook, XlsxError> {
    build_workbook(project, model, lang).map(|(wb, _)| wb)
}

/// Çalışma kitabını ve hücre kayıt defterini (testler için) birlikte üretir.
fn build_workbook(
    project: &Project,
    model: &Model,
    lang: Lang,
) -> Result<(Workbook, CellRegistry), XlsxError> {
    let mut workbook = Workbook::new();
    let styles = Styles::new();
    let mut reg = CellRegistry::default();

    let s_summary = t_lang("excel.sheet_summary", lang);
    let s_inputs = t_lang("excel.sheet_inputs", lang);
    let s_calcs = t_lang("excel.sheet_calcs", lang);

    // 1. Özet Sayfası
    let ws_summary = workbook.add_worksheet();
    ws_summary.set_name(s_summary)?;
    populate_summary_sheet(ws_summary, project, model, lang, &styles, &mut reg)?;

    // 2. Girdiler Sayfası
    let ws_inputs = workbook.add_worksheet();
    ws_inputs.set_name(s_inputs)?;
    populate_inputs_sheet(ws_inputs, project, model, lang, &styles, &mut reg, s_inputs)?;

    // 3. Hesaplar Sayfası
    let ws_calcs = workbook.add_worksheet();
    ws_calcs.set_name(s_calcs)?;
    populate_calcs_sheet(ws_calcs, project, model, lang, &styles, &mut reg, s_calcs)?;

    Ok((workbook, reg))
}

/// Verilen yola `.xlsx` dosyasını kaydeder.
pub fn export_project_to_xlsx(
    project: &Project,
    model: &Model,
    lang: Lang,
    path: &Path,
) -> Result<(), XlsxError> {
    let mut workbook = generate_xlsx_workbook(project, model, lang)?;
    workbook.save(path)?;
    Ok(())
}

/// Bellek içi testler için `.xlsx` bayt dizisi üretir.
pub fn generate_xlsx_bytes(
    project: &Project,
    model: &Model,
    lang: Lang,
) -> Result<Vec<u8>, XlsxError> {
    let mut workbook = generate_xlsx_workbook(project, model, lang)?;
    workbook.save_to_buffer()
}

// =========================================================================
// GİRDİLER SAYFASI
// =========================================================================

fn populate_inputs_sheet(
    ws: &mut Worksheet,
    p: &Project,
    m: &Model,
    lang: Lang,
    st: &Styles,
    reg: &mut CellRegistry,
    sheet_name: &str,
) -> Result<(), XlsxError> {
    ws.set_column_width(0, 8.0)?;
    ws.set_column_width(1, 40.0)?;
    ws.set_column_width(2, 14.0)?;
    ws.set_column_width(3, 16.0)?;
    ws.set_column_width(4, 12.0)?;
    ws.set_column_width(5, 55.0)?;

    ws.write_string_with_format(0, 0, t_lang("excel.inputs_title", lang), &st.title)?;
    ws.write_string_with_format(1, 0, t_lang("excel.inputs_subtitle", lang), &st.subtitle)?;

    let r_hdr = 3;
    ws.write_string_with_format(r_hdr, 0, t_lang("excel.col_step", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 1, t_lang("excel.col_param", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 2, t_lang("excel.col_symbol", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 3, t_lang("excel.col_value", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 4, t_lang("excel.col_unit", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 5, t_lang("excel.col_note", lang), &st.tbl_header)?;

    ws.set_freeze_panes(r_hdr + 1, 0)?;

    let mut ctx = SheetContext {
        ws,
        row: r_hdr + 1,
        sec_no: 0,
        step_no: 0,
        st,
        reg,
        sheet_name,
        lang,
    };

    // 1. Görev & Ortam
    ctx.write_sec(t_lang("excel.sec_mission", lang), 6)?;
    ctx.write_inp(
        "h",
        "Cruise Altitude (MSL)",
        "h",
        p.h,
        &st.inp_int,
        "m MSL",
        "Tasarım seyir irtifası",
    )?;
    ctx.write_inp(
        "v_cr",
        "Cruise Speed (Airspeed)",
        "V_cr",
        p.v_cr,
        &st.inp_dec1,
        "m/s",
        "Tasarım hava seyir hızı",
    )?;
    ctx.write_inp(
        "p_hotel",
        "Hotel & Avionics Power",
        "P_hotel",
        p.p_hotel,
        &st.inp_dec1,
        "W",
        "Aviyonik kartlar, kamera, telemetri sabit tüketim",
    )?;

    // 2. Kütle Bütçesi
    ctx.write_sec(t_lang("excel.sec_mass", lang), 6)?;
    ctx.write_inp(
        "m_payload",
        "Payload Mass",
        "m_payload",
        p.m_payload,
        &st.inp_dec3,
        "kg",
        "Gimbal, kamera veya harici yük",
    )?;

    if p.drone_type.has_wing() {
        ctx.write_inp(
            "m_avionics",
            "Avionics Mass",
            "m_avionics",
            p.m_avionics,
            &st.inp_dec3,
            "kg",
            "FC, GPS, alıcı, telemetri",
        )?;
        ctx.write_inp(
            "m_propulsion",
            "Propulsion System Mass",
            "m_prop",
            p.m_propulsion,
            &st.inp_dec3,
            "kg",
            "İtici motor, ESC ve pervane",
        )?;
        ctx.write_inp(
            "m_fuse_tail",
            "Fuselage & Tail Mass",
            "m_fuse",
            p.m_fuse_tail,
            &st.inp_dec3,
            "kg",
            "Gövde kabuğu, kuyruk ve bağlantı elemanları",
        )?;
    }

    // 3. Batarya Paketi
    ctx.write_sec(t_lang("excel.sec_battery", lang), 6)?;
    let cell = &m.cell;
    // Program, "Özel paket" kaynağında seri sayısını paketten alır, paralel=1 ve ek yük=1.0 kullanır.
    let (n_series, n_parallel, pack_overhead) =
        if p.battery_source == crate::calc::custom_battery::BatterySourceKind::CustomPack {
            (p.custom_pack.s_count.max(1), 1, 1.0)
        } else {
            (p.n_series, p.n_parallel, p.pack_overhead)
        };
    ctx.write_inp(
        "cell_v",
        "Cell Nominal Voltage",
        "V_cell",
        cell.nominal_v,
        &st.inp_dec2,
        "V",
        &if lang == Lang::Tr {
            format!("Hücre: {}", cell.name)
        } else {
            format!("Cell: {}", cell.name)
        },
    )?;
    ctx.write_inp(
        "cell_cap",
        "Cell Capacity",
        "C_cell",
        cell.capacity_ah,
        &st.inp_dec2,
        "Ah",
        "Tek hücre anma kapasitesi",
    )?;
    ctx.write_inp(
        "cell_m",
        "Cell Mass",
        "m_cell",
        cell.mass_g / 1000.0,
        &st.inp_dec4,
        "kg",
        "Tek hücre kütlesi",
    )?;
    ctx.write_inp(
        "n_s",
        "Pack Series Count (S)",
        "n_s",
        n_series as f64,
        &st.inp_int,
        "-",
        "Seri hücre adedi",
    )?;
    ctx.write_inp(
        "n_p",
        "Pack Parallel Count (P)",
        "n_p",
        n_parallel as f64,
        &st.inp_int,
        "-",
        "Paralel hücre adedi",
    )?;
    ctx.write_inp(
        "pack_ovh",
        "Pack Assembly Overhead",
        "k_pack",
        pack_overhead,
        &st.inp_dec2,
        "-",
        "Şerit, kablo, BMS ve paketleme payı çarpanı",
    )?;
    ctx.write_inp(
        "dod",
        "Depth of Discharge (DOD)",
        "DOD",
        p.dod,
        &st.inp_pct1,
        "-",
        "Kullanılabilir batarya kapasite oranı (tipik %80)",
    )?;

    // 4. Kanat Geometrisi (Sabit Kanat / Uçan Kanat / VTOL)
    if p.drone_type.has_wing() {
        ctx.write_sec(t_lang("excel.sec_wing", lang), 6)?;
        ctx.write_inp(
            "b",
            "Wing Span",
            "b",
            p.b,
            &st.inp_dec3,
            "m",
            "Uçtan uca kanat açıklığı",
        )?;
        ctx.write_inp(
            "s",
            "Wing Planform Area",
            "S",
            p.s,
            &st.inp_dec4,
            "m²",
            "Kanat referans alanı",
        )?;
        ctx.write_inp(
            "taper",
            "Wing Taper Ratio",
            "λ",
            p.taper,
            &st.inp_dec2,
            "-",
            "Uç kordu / kök kordu oranı (c_t / c_r)",
        )?;
        ctx.write_inp(
            "sweep_le",
            "LE Sweep Angle",
            "Λ_LE",
            p.sweep_le_deg,
            &st.inp_dec1,
            "deg",
            "Hücum kenarı ok açısı",
        )?;
        ctx.write_inp(
            "tc_w",
            "Wing Thickness Ratio",
            "t/c",
            p.tc_w,
            &st.inp_pct1,
            "-",
            &if lang == Lang::Tr {
                format!("Profil: {}", p.airfoil.name)
            } else {
                format!("Airfoil: {}", p.airfoil.name)
            },
        )?;

        // 5. Aerodinamik & Verimler
        ctx.write_sec(t_lang("excel.sec_aero", lang), 6)?;
        ctx.write_inp(
            "cl_alpha_2d",
            "2D Lift Curve Slope",
            "C_lα",
            p.cl_alpha_2d,
            &st.inp_dec3,
            "1/rad",
            "Profil 2B taşıma eğrisi eğimi (2π rad)",
        )?;
        ctx.write_inp(
            "cl_max_2d",
            "2D Max Lift Coefficient",
            "C_lmax",
            p.cl_max_2d,
            &st.inp_dec2,
            "-",
            "Profil 2B azami taşıma katsayısı",
        )?;
        ctx.write_inp(
            "cd_misc",
            "Misc Parasite Drag Coeff",
            "C_D,misc",
            p.cd_misc,
            &st.inp_dec4,
            "-",
            "İniş takımı, kamera çıkıntısı parazit sürtünme",
        )?;
        ctx.write_inp(
            "eta_prop",
            "Cruise Propeller Efficiency",
            "η_prop",
            p.eta_p_cr,
            &st.inp_pct1,
            "-",
            "İtici pervane seyir aerodinamik verimi",
        )?;
        ctx.write_inp(
            "eta_mot",
            "Cruise Motor Efficiency",
            "η_mot",
            p.eta_mot,
            &st.inp_pct1,
            "-",
            "Fırçasız itici motor elektromekanik verimi",
        )?;
        ctx.write_inp(
            "eta_esc",
            "Cruise ESC Efficiency",
            "η_esc",
            p.eta_esc,
            &st.inp_pct1,
            "-",
            "Elektronik hız kontrolcü elektriksel verimi",
        )?;

        // 6. Kuyruk (Uçan kanat değilse)
        if p.tail_kind != crate::calc::geometry::TailKind::FlyingWing {
            ctx.write_sec(t_lang("excel.sec_tail", lang), 6)?;
            ctx.write_inp(
                "s_h",
                "Horizontal Tail Area",
                "S_h",
                p.s_h,
                &st.inp_dec4,
                "m²",
                "Yatay stabilizör planform alanı",
            )?;
            ctx.write_inp(
                "s_v",
                "Vertical Tail Area",
                "S_v",
                p.s_v,
                &st.inp_dec4,
                "m²",
                "Dikey stabilizör planform alanı",
            )?;
            ctx.write_inp(
                "l_h",
                "Tail Moment Arm",
                "l_h",
                p.l_h,
                &st.inp_dec3,
                "m",
                "Kanat AC ile kuyruk AC arası mesafe",
            )?;
        }
    }

    // 7. Multirotor Donanımı (Multirotor veya VTOL)
    if p.drone_type.is_multirotor() {
        ctx.write_sec(t_lang("excel.sec_multirotor", lang), 6)?;
        // Program, kol sayısını 3/4/6/8 değerlerine normalleştirir; Excel'e kullanılan değer yazılır.
        let arm_cnt = if p.mr_coaxial {
            (m.mr.n_motors / 2) as f64
        } else {
            m.mr.n_motors as f64
        };
        ctx.write_inp(
            "mr_frame",
            "Frame Diagonal Size",
            "D_frame",
            p.mr_frame_size,
            &st.inp_dec3,
            "m",
            "Karşılıklı motor merkezleri mesafesi",
        )?;
        ctx.write_inp(
            "mr_arms",
            "Motor / Arm Count",
            "n_arm",
            arm_cnt,
            &st.inp_int,
            "-",
            &if lang == Lang::Tr {
                format!(
                    "Düzen: {:?}, Koaksiyel: {}",
                    p.mr_layout,
                    if p.mr_coaxial { "Evet" } else { "Hayır" }
                )
            } else {
                format!(
                    "Layout: {:?}, Coaxial: {}",
                    p.mr_layout,
                    if p.mr_coaxial { "Yes" } else { "No" }
                )
            },
        )?;
        ctx.write_inp(
            "mr_coax",
            "Coaxial Layout (1 = yes, 0 = no)",
            "coax",
            if p.mr_coaxial { 1.0 } else { 0.0 },
            &st.inp_int,
            "-",
            "Koaksiyelde kol başına 2 motor",
        )?;
        ctx.write_inp(
            "mr_kappa",
            "Coaxial Interference Factor",
            "κ_int",
            m.mr.coax_kappa,
            &st.inp_dec2,
            "-",
            "Koaksiyel olmayanda 1.0; koaksiyelde alt rotor güç çarpanı",
        )?;
        ctx.write_inp(
            "mr_d_prop",
            "Propeller Diameter",
            "D_prop",
            p.mr_d_prop,
            &st.inp_dec3,
            "m",
            &if lang == Lang::Tr {
                format!("{:.1} inç", p.mr_d_prop / 0.0254)
            } else {
                format!("{:.1} in", p.mr_d_prop / 0.0254)
            },
        )?;
        ctx.write_inp(
            "mr_t_max_motor",
            "Max Thrust per Motor",
            "T_max",
            p.mr_t_max_motor,
            &st.inp_dec1,
            "N",
            "Üretici tam gaz tezgâh itkisi",
        )?;
        ctx.write_inp(
            "mr_m_motor",
            "Motor Mass (each)",
            "m_motor",
            p.mr_m_motor,
            &st.inp_dec3,
            "kg",
            "Tek bir kaldırma motoru kütlesi",
        )?;
        ctx.write_inp(
            "mr_m_frame",
            "Frame Structure Mass",
            "m_frame",
            p.mr_m_frame,
            &st.inp_dec3,
            "kg",
            "Kollar, plakalar ve donanım kütlesi",
        )?;
        ctx.write_inp(
            "mr_m_esc",
            "ESC Group Total Mass",
            "m_esc",
            p.mr_m_esc_tot,
            &st.inp_dec3,
            "kg",
            "Tüm kaldırma ESC'leri kütlesi",
        )?;
        ctx.write_inp(
            "mr_m_fc",
            "Flight Controller Mass",
            "m_fc",
            p.mr_m_fc,
            &st.inp_dec3,
            "kg",
            "FC, güç modülü, GPS",
        )?;
        ctx.write_inp(
            "mr_m_misc",
            "Misc Multirotor Hardware",
            "m_misc",
            p.mr_m_misc,
            &st.inp_dec3,
            "kg",
            "Kablaj, vidalar, iniş ayağı payı",
        )?;
        ctx.write_inp(
            "mr_fm",
            "Propeller Figure of Merit",
            "FM",
            p.fm,
            &st.inp_dec2,
            "-",
            "Pervane askı verim faktörü (tipik 0.60-0.75)",
        )?;
        ctx.write_inp(
            "mr_eta_mot",
            "Hover Motor Efficiency",
            "η_mot",
            p.eta_mot,
            &st.inp_pct1,
            "-",
            "Askı devrinde motor elektromekanik verimi",
        )?;
        ctx.write_inp(
            "mr_eta_esc",
            "Hover ESC Efficiency",
            "η_esc",
            p.eta_esc,
            &st.inp_pct1,
            "-",
            "Askı akımında ESC elektriksel verimi",
        )?;
    }

    Ok(())
}

// =========================================================================
// HESAPLAR SAYFASI (CANLI EXCEL FORMÜLLERİ)
// =========================================================================

fn populate_calcs_sheet(
    ws: &mut Worksheet,
    p: &Project,
    m: &Model,
    lang: Lang,
    st: &Styles,
    reg: &mut CellRegistry,
    sheet_name: &str,
) -> Result<(), XlsxError> {
    ws.set_column_width(0, 8.0)?;
    ws.set_column_width(1, 40.0)?;
    ws.set_column_width(2, 14.0)?;
    ws.set_column_width(3, 16.0)?;
    ws.set_column_width(4, 12.0)?;
    ws.set_column_width(5, 45.0)?;
    ws.set_column_width(6, 50.0)?;

    ws.write_string_with_format(0, 0, t_lang("excel.calcs_title", lang), &st.title)?;
    ws.write_string_with_format(1, 0, t_lang("excel.calcs_subtitle", lang), &st.subtitle)?;

    let r_hdr = 3;
    ws.write_string_with_format(r_hdr, 0, t_lang("excel.col_step", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 1, t_lang("excel.col_param", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 2, t_lang("excel.col_symbol", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 3, t_lang("excel.col_value", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 4, t_lang("excel.col_unit", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 5, t_lang("excel.col_formula", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr, 6, t_lang("excel.col_note", lang), &st.tbl_header)?;

    ws.set_freeze_panes(r_hdr + 1, 0)?;

    let mut ctx = SheetContext {
        ws,
        row: r_hdr + 1,
        sec_no: 0,
        step_no: 0,
        st,
        reg,
        sheet_name,
        lang,
    };

    // 1. Standart Atmosfer (ISA Troposfer)
    ctx.write_sec(t_lang("excel.sec_atmosphere", lang), 7)?;
    ctx.write_formula(
        "isa_T",
        "ISA Temperature",
        "T",
        "=288.15-0.0065*{h}",
        m.atmo.t,
        &st.calc_dec1,
        "K",
        "T = T0 - L·h (ISA standart lapse rate)",
    )?;
    ctx.write_formula(
        "isa_P",
        "ISA Pressure",
        "p",
        "=101325*(1-0.0065*{h}/288.15)^5.255877",
        m.atmo.p,
        &st.calc_dec1,
        "Pa",
        "p = P0·(1 - L·h/T0)^(g/(R·L))",
    )?;
    ctx.write_formula(
        "rho",
        "Air Density (ISA)",
        "ρ",
        "={isa_P}/(287.05287*{isa_T})",
        m.atmo.rho,
        &st.calc_dec4,
        "kg/m³",
        "ρ = p / (R_hava · T)",
    )?;

    // 2. Kütle ve Ağırlık Bütçesi
    ctx.write_sec(t_lang("excel.sec_mass", lang), 7)?;
    ctx.write_formula(
        "m_bat",
        "Battery Pack Mass",
        "m_bat",
        "={n_s}*{n_p}*{cell_m}*{pack_ovh}",
        m.weight.m_battery,
        &st.calc_dec3,
        "kg",
        "m_bat = n_s · n_p · m_hücre · k_paket",
    )?;

    if p.drone_type.is_multirotor() {
        // Çok rotorlu kütle modeli (VTOL'de kanat kütle modelinden ayrıdır; program da ayrı kullanır).
        ctx.write_formula(
            "mr_nmotors",
            "Total Motor Count",
            "n_motors",
            "={mr_arms}*(1+{mr_coax})",
            m.mr.n_motors as f64,
            &st.calc_int,
            "-",
            "n_motor = n_kol · (1 + koaksiyel)",
        )?;
        ctx.write_formula(
            "mr_mtow",
            "Multirotor Take-Off Mass (MTOW)",
            "MTOW_mr",
            "={mr_m_frame}+{mr_nmotors}*{mr_m_motor}+{mr_m_esc}+{mr_m_fc}+{m_bat}+{m_payload}+{mr_m_misc}",
            m.mr.m_mtow,
            &st.calc_dec3,
            "kg",
            "MTOW = m_şasi + n·m_motor + m_esc + m_fc + m_bat + m_yük + m_ek",
        )?;
        ctx.write_formula(
            "mr_W",
            "Multirotor Weight",
            "W_mr",
            "={mr_mtow}*9.80665",
            m.mr.w,
            &st.calc_dec2,
            "N",
            "W = MTOW · g (g = 9.80665 m/s²)",
        )?;
    }
    if p.drone_type.has_wing() {
        // Sabit Kanat / Uçan Kanat / VTOL
        let empty_mass = (m.weight.m_mtow - m.weight.m_battery - p.m_payload).max(0.0);
        ctx.write_value(
            "m_empty",
            "Empty Mass (Structure + Systems)",
            "m_empty",
            empty_mass,
            &st.calc_dec3,
            "kg",
            "Boş kütle = m_aviyonik + m_itki + m_gövde_kuyruk + m_kanat",
        )?;
        ctx.write_formula(
            "mtow",
            "Maximum Take-Off Mass (MTOW)",
            "MTOW",
            "={m_empty}+{m_payload}+{m_bat}",
            m.weight.m_mtow,
            &st.calc_dec3,
            "kg",
            "MTOW = m_boş + m_faydalı_yük + m_batarya",
        )?;
        ctx.write_formula(
            "W",
            "Total Aircraft Weight",
            "W",
            "={mtow}*9.80665",
            m.weight.w,
            &st.calc_dec2,
            "N",
            "W = MTOW · g (g = 9.80665 m/s²)",
        )?;
    }

    // 3. Kanat Geometrisi ve Yüklemesi (Kanatlı platformlar)
    if p.drone_type.has_wing() {
        ctx.write_sec(t_lang("excel.sec_wing_loading", lang), 7)?;
        ctx.write_formula(
            "ar",
            "Aspect Ratio",
            "AR",
            "={b}^2/{s}",
            m.wing.ar,
            &st.calc_dec2,
            "-",
            "AR = b² / S",
        )?;
        ctx.write_formula(
            "c_r",
            "Wing Root Chord",
            "c_r",
            "=2*{s}/({b}*(1+{taper}))",
            m.wing.c_r,
            &st.calc_dec3,
            "m",
            "c_r = 2·S / (b · (1 + λ))",
        )?;
        ctx.write_formula(
            "c_t",
            "Wing Tip Chord",
            "c_t",
            "={taper}*{c_r}",
            m.wing.c_t,
            &st.calc_dec3,
            "m",
            "c_t = λ · c_r",
        )?;
        ctx.write_formula(
            "mac",
            "Mean Aerodynamic Chord (MAC)",
            "c̄",
            "=(2/3)*{c_r}*(1+{taper}+{taper}^2)/(1+{taper})",
            m.wing.mac,
            &st.calc_dec4,
            "m",
            "c̄ = (2/3)·c_r · (1 + λ + λ²) / (1 + λ)",
        )?;
        ctx.write_formula(
            "ws",
            "Wing Loading (Weight-based)",
            "W/S",
            "={W}/{s}",
            m.ws,
            &st.calc_dec1,
            "N/m²",
            "W/S = W / S",
        )?;
        ctx.write_formula(
            "ws_kg",
            "Wing Loading (Mass-based)",
            "M/S",
            "={mtow}/{s}",
            m.weight.m_mtow / p.s,
            &st.calc_dec2,
            "kg/m²",
            "M/S = MTOW / S",
        )?;

        // 4. Aerodinamik, Sürükleme ve Stall
        ctx.write_sec(t_lang("excel.sec_aero", lang), 7)?;
        ctx.write_formula(
            "e",
            "Oswald Efficiency Factor",
            "e",
            "=1/(1.05+0.007*PI()*{ar})",
            m.aero.e,
            &st.calc_dec3,
            "-",
            "e = 1 / (1.05 + 0.007·π·AR) [Raymer]",
        )?;
        ctx.write_formula(
            "k",
            "Induced Drag Factor",
            "k",
            "=1/(PI()*{ar}*{e})",
            m.aero.k,
            &st.calc_dec4,
            "-",
            "k = 1 / (π · AR · e)",
        )?;
        ctx.write_value(
            "cl_max",
            "3D Maximum Lift Coefficient",
            "C_Lmax",
            m.aero.cl_max,
            &st.calc_dec2,
            "-",
            "3B kanat sonlu açıklık ve daralma düzeltmeli C_Lmax",
        )?;
        ctx.write_formula(
            "v_stall",
            "Stall Speed",
            "V_stall",
            "=SQRT(2*{ws}/({rho}*{cl_max}))",
            m.cons.v_stall,
            &st.calc_dec2,
            "m/s",
            "V_stall = √(2·(W/S) / (ρ · C_Lmax))",
        )?;
        ctx.write_formula(
            "v_stall_kmh",
            "Stall Speed (km/h)",
            "V_stall,kmh",
            "={v_stall}*3.6",
            m.cons.v_stall * 3.6,
            &st.calc_dec1,
            "km/h",
            "V[km/h] = V[m/s] · 3.6",
        )?;
        ctx.write_formula(
            "cl_cr",
            "Cruise Lift Coefficient",
            "C_L,cr",
            "={ws}/(0.5*{rho}*{v_cr}^2)",
            m.perf_cr.cl,
            &st.calc_dec3,
            "-",
            "C_L = (W/S) / (0.5 · ρ · V_cr²)",
        )?;
        ctx.write_value(
            "cd0",
            "Zero-Lift Parasite Drag Coeff",
            "C_D0",
            m.aero.cd0,
            &st.calc_dec4,
            "-",
            "Gövde, kanat, kuyruk parazit sürtünme yığılımı (build-up)",
        )?;
        ctx.write_formula(
            "cdi_cr",
            "Induced Drag Coefficient",
            "C_Di,cr",
            "={k}*{cl_cr}^2",
            m.aero.k * m.perf_cr.cl * m.perf_cr.cl,
            &st.calc_dec4,
            "-",
            "C_Di = k · C_L²",
        )?;
        ctx.write_formula(
            "cd_cr",
            "Total Cruise Drag Coefficient",
            "C_D,cr",
            "={cd0}+{cdi_cr}",
            m.aero.cd0 + m.aero.k * m.perf_cr.cl * m.perf_cr.cl,
            &st.calc_dec4,
            "-",
            "C_D = C_D0 + C_Di",
        )?;
        ctx.write_formula(
            "ld_cr",
            "Cruise Lift-to-Drag Ratio",
            "(L/D)_cr",
            "={cl_cr}/{cd_cr}",
            m.perf_cr.ld,
            &st.calc_dec2,
            "-",
            "L/D = C_L / C_D",
        )?;
        ctx.write_formula(
            "ld_max",
            "Maximum Glide Ratio (L/D_max)",
            "(L/D)_max",
            "=1/(2*SQRT({k}*{cd0}))",
            m.aero.ld_max,
            &st.calc_dec2,
            "-",
            "(L/D)_max = 1 / (2 · √(k · C_D0))",
        )?;

        // 5. Seyir Güç, Havada Kalma ve Menzil
        ctx.write_sec(t_lang("excel.sec_performance", lang), 7)?;
        ctx.write_formula(
            "eta_pt",
            "Total Powertrain Efficiency",
            "η_total",
            "={eta_esc}*{eta_mot}*{eta_prop}",
            p.eta_esc * p.eta_mot * p.eta_p_cr,
            &st.calc_pct1,
            "-",
            "η_total = η_esc · η_motor · η_pervane",
        )?;
        ctx.write_formula(
            "e_use_wh",
            "Usable Battery Energy",
            "E_usable",
            "={n_s}*{cell_v}*{n_p}*{cell_cap}*{dod}",
            (p.n_series as f64)
                * m.cell.nominal_v
                * (p.n_parallel as f64)
                * m.cell.capacity_ah
                * p.dod,
            &st.calc_dec1,
            "Wh",
            "E_kul = n_s · V_hücre · n_p · C_hücre · DOD",
        )?;
        ctx.write_formula(
            "p_aero",
            "Aerodynamic Thrust Power",
            "P_aero",
            "={W}*{v_cr}/{ld_cr}",
            m.perf_cr.p_aero,
            &st.calc_dec1,
            "W",
            "P_aero = D · V = (W / (L/D)) · V_cr",
        )?;
        ctx.write_formula(
            "p_prop_el",
            "Propulsion Electrical Power",
            "P_prop,el",
            "={p_aero}/{eta_pt}",
            m.perf_cr.p_prop_elec,
            &st.calc_dec1,
            "W",
            "P_el = P_aero / η_total",
        )?;
        ctx.write_formula(
            "p_total_cr",
            "Total Cruise Electrical Power",
            "P_total,cr",
            "={p_prop_el}+{p_hotel}",
            m.perf_cr.p_total,
            &st.calc_dec1,
            "W",
            "P_toplam = P_itki,el + P_otel (aviyonik)",
        )?;
        ctx.write_formula(
            "t_cr_min",
            "Cruise Flight Endurance",
            "t_cruise",
            "={e_use_wh}/{p_total_cr}*60",
            m.perf_cr.endurance_min(),
            &st.calc_dec1,
            "min",
            "t_uçuş = (E_usable [Wh] / P_total [W]) · 60 dk",
        )?;
        ctx.write_formula(
            "r_cr_km",
            "Cruise Range",
            "R_cruise",
            "={v_cr}*({t_cr_min}*60)/1000",
            m.perf_cr.range_km(),
            &st.calc_dec1,
            "km",
            "R = V_cr · t_uçuş (rüzgârsız düz hat menzili)",
        )?;
    }

    // 6. Multirotor Askı Analizi (Multirotor veya VTOL platformları)
    if p.drone_type.is_multirotor() {
        ctx.write_sec(t_lang("excel.sec_mr_hover", lang), 7)?;
        ctx.write_formula(
            "mr_t_hov",
            "Hover Thrust per Motor",
            "T_hover",
            "={mr_W}/{mr_nmotors}",
            m.mr.t_hover_motor,
            &st.calc_dec2,
            "N",
            "T_motor = W / n_motors",
        )?;
        ctx.write_formula(
            "mr_t_hov_g",
            "Hover Thrust per Motor (gram)",
            "T_hover,g",
            "={mr_t_hov}/9.80665*1000",
            m.mr.t_hover_motor / 9.80665 * 1000.0,
            &st.calc_int,
            "g",
            "T[g] = T[N] / g · 1000",
        )?;
        ctx.write_formula(
            "mr_adisk1",
            "Single Propeller Disk Area",
            "A_disk",
            "=PI()*({mr_d_prop}/2)^2",
            m.mr.a_disk_single,
            &st.calc_dec4,
            "m²",
            "A = π · (D_prop / 2)²",
        )?;
        ctx.write_formula(
            "mr_adisk_tot",
            "Total Rotor Disk Area",
            "A_disk,tot",
            "={mr_arms}*{mr_adisk1}",
            m.mr.a_disk_total,
            &st.calc_dec4,
            "m²",
            "A_toplam = n_kol · A_tek_disk",
        )?;
        ctx.write_formula(
            "mr_dl",
            "Disk Loading",
            "DL",
            "={mr_W}/{mr_adisk_tot}",
            m.mr.disk_loading,
            &st.calc_dec1,
            "N/m²",
            "DL = W / A_toplam",
        )?;
        ctx.write_formula(
            "mr_vi",
            "Induced Inflow Velocity",
            "v_i",
            "=SQRT({mr_t_hov}/(2*{rho}*{mr_adisk1}))",
            m.mr.v_i,
            &st.calc_dec2,
            "m/s",
            "v_i = √(T_motor / (2·ρ·A_disk)) [Momentum]",
        )?;
        ctx.write_formula(
            "mr_p_ideal",
            "Total Ideal Induced Power",
            "P_ideal",
            "={mr_W}^1.5/SQRT(2*{rho}*{mr_adisk_tot})*{mr_kappa}",
            m.mr.p_ideal_total,
            &st.calc_dec1,
            "W",
            "P_ideal = κ · W^1.5 / √(2·ρ·A_toplam)",
        )?;
        ctx.write_formula(
            "mr_p_shaft",
            "Total Shaft Mechanical Power",
            "P_shaft",
            "={mr_p_ideal}/{mr_fm}",
            m.mr.p_shaft_total,
            &st.calc_dec1,
            "W",
            "P_şaft = P_ideal / FM",
        )?;
        ctx.write_formula(
            "mr_p_elec_hov",
            "Hover Motor Electrical Power",
            "P_elec,hov",
            "={mr_p_shaft}/({mr_eta_mot}*{mr_eta_esc})",
            m.mr.p_elec_hover_total,
            &st.calc_dec1,
            "W",
            "P_el,motor = P_şaft / (η_motor · η_esc)",
        )?;
        ctx.write_formula(
            "mr_p_tot_hov",
            "Total Hover Electrical Power",
            "P_total,hov",
            "={mr_p_elec_hov}+{p_hotel}",
            m.mr.p_total_hover,
            &st.calc_dec1,
            "W",
            "P_toplam = P_el,motor + P_otel",
        )?;
        ctx.write_formula(
            "mr_t_max_tot",
            "Total Max Available Thrust",
            "T_max,tot",
            "={mr_nmotors}*{mr_t_max_motor}",
            m.mr.t_max_total,
            &st.calc_dec1,
            "N",
            "T_azami = n_motors · T_motor,max",
        )?;
        ctx.write_formula(
            "mr_tw",
            "Thrust-to-Weight Ratio",
            "T/W",
            "={mr_t_max_tot}/{mr_W}",
            m.mr.tw_ratio,
            &st.calc_dec2,
            "-",
            "T/W = T_azami / W (hedef ≥ 1.8)",
        )?;
        ctx.write_formula(
            "mr_throttle",
            "Hover Throttle Estimate",
            "Throttle",
            "=SQRT(1/{mr_tw})",
            m.mr.hover_throttle,
            &st.calc_pct1,
            "-",
            "Gaz ≈ √(T_hover / T_max) = √(1 / (T/W))",
        )?;

        // Kullanılabilir enerji formülü multirotorda da geçerli
        if !p.drone_type.has_wing() {
            ctx.write_formula(
                "e_use_wh",
                "Usable Battery Energy",
                "E_usable",
                "={n_s}*{cell_v}*{n_p}*{cell_cap}*{dod}",
                (p.n_series as f64)
                    * m.cell.nominal_v
                    * (p.n_parallel as f64)
                    * m.cell.capacity_ah
                    * p.dod,
                &st.calc_dec1,
                "Wh",
                "E_kul = n_s · V_hücre · n_p · C_hücre · DOD",
            )?;
        }

        ctx.write_formula(
            "mr_endurance",
            "Hover Endurance Time",
            "t_hover",
            "={e_use_wh}/{mr_p_tot_hov}*60",
            m.mr.hover_endurance_min,
            &st.calc_dec1,
            "min",
            "t_askı = (E_usable [Wh] / P_hover [W]) · 60 dk",
        )?;
    }

    // 7. Gelişmiş Mühendislik ve Teşhis Çıktıları
    ctx.write_sec(t_lang("excel.sec_diagnostics", lang), 7)?;
    if p.drone_type.has_wing() {
        ctx.write_value(
            "cg_x",
            "Center of Gravity (from Nose)",
            "x_cg",
            m.stab.cg_nose.x,
            &st.calc_dec3,
            "m",
            "Bileşen kütle ve moment kolu dengesi",
        )?;
        ctx.write_value(
            "np_x",
            "Neutral Point (from Nose)",
            "x_np",
            m.stab.np.x_np + m.stab.datum,
            &st.calc_dec3,
            "m",
            "Kanat ve kuyruk kaldırma merkezi dengesi",
        )?;
        ctx.write_value(
            "sm",
            "Static Margin",
            "SM",
            m.stab.sm * 100.0,
            &st.calc_dec1,
            "% MAC",
            "SM = (x_np - x_cg) / c̄ (hedef %8 - %15)",
        )?;
        ctx.write_value(
            "m_b_ult",
            "Spar Ultimate Bending Moment",
            "M_b,ult",
            m.st.m_b_ult,
            &st.calc_dec1,
            "N·m",
            "Limit yük faktörü x 1.5 emniyet katsayısı",
        )?;
        ctx.write_value(
            "sigma_max",
            "Max Spar Bending Stress",
            "σ_max",
            m.st.sigma_max / 1e6,
            &st.calc_dec1,
            "MPa",
            "σ = M · c / I_spar",
        )?;
        ctx.write_value(
            "ms_spar",
            "Structural Margin of Safety",
            "MS",
            m.st.ms,
            &st.calc_dec2,
            "-",
            "MS = σ_izin / σ_max - 1 (> 0 olmalıdır)",
        )?;
    }

    if p.drone_type.is_multirotor() {
        ctx.write_value(
            "mr_spacing",
            "Adjacent Tip-to-Tip Spacing",
            "s_adj",
            m.mr.s_adj * 1000.0,
            &st.calc_dec1,
            "mm",
            "Bitişik motor merkezleri mesafesi",
        )?;
        let clearance_mm = (m.mr.s_adj - p.mr_d_prop).max(0.0) * 1000.0;
        ctx.write_value(
            "mr_clearance",
            "Blade Tip Clearance",
            "Δ_tip",
            clearance_mm,
            &st.calc_dec1,
            "mm",
            "Pala uçları arası net açıklık (çarpışma payı)",
        )?;
        ctx.write_value(
            "mr_cg_offset",
            "CG Planar Offset from Center",
            "Δ_cg",
            m.mr.cg_offset_d * 1000.0,
            &st.calc_dec1,
            "mm",
            "Ağırlık merkezinin geometrik merkezden sapması",
        )?;
    }

    Ok(())
}

// =========================================================================
// ÖZET SAYFASI (YÖNETİCİ ÖZETİ VE UYARILAR)
// =========================================================================

fn populate_summary_sheet(
    ws: &mut Worksheet,
    p: &Project,
    m: &Model,
    lang: Lang,
    st: &Styles,
    reg: &mut CellRegistry,
) -> Result<(), XlsxError> {
    ws.set_column_width(0, 6.0)?;
    ws.set_column_width(1, 38.0)?;
    ws.set_column_width(2, 16.0)?;
    ws.set_column_width(3, 14.0)?;
    ws.set_column_width(4, 55.0)?;

    let title = format!("WerSeuS – UAV Calc — {}", p.name);
    ws.write_string_with_format(0, 0, title, &st.title)?;

    let app_version = env!("CARGO_PKG_VERSION");
    let subtitle = format!(
        "{} | {}: {} | {}: {} | {}: v{}",
        t_lang("excel.summary_platform", lang),
        t_lang("excel.platform_type", lang),
        p.drone_type.label(),
        t_lang("excel.date", lang),
        current_date_str(),
        t_lang("excel.version", lang),
        app_version
    );
    ws.write_string_with_format(1, 0, subtitle, &st.subtitle)?;

    // 1. Anahtar Çıktılar Tablosu
    let r_hdr1 = 3;
    ws.write_string_with_format(r_hdr1, 0, "#", &st.tbl_header)?;
    ws.write_string_with_format(r_hdr1, 1, t_lang("excel.col_param", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr1, 2, t_lang("excel.col_value", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr1, 3, t_lang("excel.col_unit", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr1, 4, t_lang("excel.col_note", lang), &st.tbl_header)?;

    ws.set_freeze_panes(r_hdr1 + 1, 0)?;

    let mut row = r_hdr1 + 1;
    let mut item_no = 0;

    let mtow = if p.drone_type == DroneType::Multirotor {
        m.mr.m_mtow
    } else {
        m.weight.m_mtow
    };
    let empty_weight = (mtow - m.weight.m_battery - p.m_payload).max(0.0);

    let mut metrics: Vec<(&str, f64, &Format, &str, String)> = vec![
        (
            t_lang("report.mtow", lang),
            mtow,
            &st.calc_dec2,
            "kg",
            "Azami Kalkış Kütlesi (MTOW)".into(),
        ),
        (
            t_lang("report.m_empty", lang),
            empty_weight,
            &st.calc_dec2,
            "kg",
            "Gövde, donanım ve sistemler boş kütlesi".into(),
        ),
        (
            t_lang("report.m_payload", lang),
            p.m_payload,
            &st.calc_dec2,
            "kg",
            "Faydalı yük kütlesi".into(),
        ),
        (
            t_lang("report.m_battery", lang),
            m.weight.m_battery,
            &st.calc_dec2,
            "kg",
            "Batarya paketi kütlesi".into(),
        ),
    ];

    if p.drone_type.has_wing() {
        metrics.push((
            "Wing Span (b)",
            p.b,
            &st.calc_dec2,
            "m",
            "Uçtan uca kanat açıklığı".into(),
        ));
        metrics.push((
            "Wing Area (S)",
            p.s,
            &st.calc_dec3,
            "m²",
            "Kanat referans alanı".into(),
        ));
        metrics.push((
            "Aspect Ratio (AR)",
            m.wing.ar,
            &st.calc_dec2,
            "-",
            "Kanat açıklık oranı (b² / S)".into(),
        ));
        metrics.push((
            "Wing Loading (M/S)",
            mtow / p.s,
            &st.calc_dec1,
            "kg/m²",
            "Alan başına düşen uçuş kütlesi".into(),
        ));
        metrics.push((
            t_lang("report.v_stall", lang),
            m.cons.v_stall,
            &st.calc_dec1,
            "m/s",
            if lang == Lang::Tr {
                format!("Tutunma hızı ({:.1} km/h)", m.cons.v_stall * 3.6)
            } else {
                format!("Stall speed ({:.1} km/h)", m.cons.v_stall * 3.6)
            },
        ));
        metrics.push((
            t_lang("report.v_cruise", lang),
            p.v_cr,
            &st.calc_dec1,
            "m/s",
            if lang == Lang::Tr {
                format!("Seyir hızı ({:.1} km/h)", p.v_cr * 3.6)
            } else {
                format!("Cruise speed ({:.1} km/h)", p.v_cr * 3.6)
            },
        ));
        metrics.push((
            "Cruise L/D",
            m.perf_cr.ld,
            &st.calc_dec1,
            "-",
            "Seyir süzülüş taşıma/sürükleme oranı".into(),
        ));
        metrics.push((
            t_lang("report.flight_time_cr", lang),
            m.perf_cr.endurance_min(),
            &st.calc_dec1,
            "min",
            "Düz uçuş havada kalma süresi".into(),
        ));
        metrics.push((
            t_lang("report.range_cr", lang),
            m.perf_cr.range_km(),
            &st.calc_dec1,
            "km",
            "Rüzgârsız seyir menzili".into(),
        ));
        metrics.push((
            t_lang("report.static_margin", lang),
            m.stab.sm * 100.0,
            &st.calc_dec1,
            "% MAC",
            if m.stab.in_band {
                "Hedef stabilite bandında".into()
            } else {
                "Hedef bandın dışında".into()
            },
        ));
    }

    if p.drone_type.is_multirotor() {
        metrics.push((
            t_lang("report.mr_tw", lang),
            m.mr.tw_ratio,
            &st.calc_dec2,
            "-",
            "İtki / Ağırlık oranı (nominal gerilimde)".into(),
        ));
        metrics.push((
            t_lang("report.mr_hover_throttle", lang),
            m.mr.hover_throttle * 100.0,
            &st.calc_dec1,
            "%",
            "Askıda kalma tahmini gaz seviyesi".into(),
        ));
        metrics.push((
            t_lang("report.mr_hover_time", lang),
            m.mr.hover_endurance_min,
            &st.calc_dec1,
            "min",
            "Askıda kalma süresi (hover endurance)".into(),
        ));
        metrics.push((
            t_lang("report.mr_p_hover", lang),
            m.mr.p_total_hover,
            &st.calc_dec1,
            "W",
            "Toplam elektriksel askı güç tüketimi".into(),
        ));
    }

    for (name, val, fmt, unit, desc) in metrics {
        item_no += 1;
        ws.write_number_with_format(row, 0, item_no as f64, &st.text)?;
        let name = loc(lang, name);
        let desc = loc(lang, &desc);
        reg.texts.push(name.clone());
        reg.texts.push(desc.clone());
        ws.write_string_with_format(row, 1, name, &st.text_bold)?;
        ws.write_number_with_format(row, 2, val, fmt)?;
        ws.write_string_with_format(row, 3, unit, &st.unit)?;
        ws.write_string_with_format(row, 4, desc, &st.note)?;
        row += 1;
    }

    // 2. Uyarılar ve Teşhis Tablosu
    row += 2;
    ws.write_string_with_format(row, 0, t_lang("report.sec_warnings", lang), &st.title)?;
    row += 1;

    let r_hdr2 = row;
    ws.write_string_with_format(r_hdr2, 0, "#", &st.tbl_header)?;
    ws.write_string_with_format(
        r_hdr2,
        1,
        t_lang("excel.col_severity", lang),
        &st.tbl_header,
    )?;
    ws.write_string_with_format(r_hdr2, 2, t_lang("excel.col_area", lang), &st.tbl_header)?;
    ws.write_string_with_format(r_hdr2, 3, t_lang("excel.col_message", lang), &st.tbl_header)?;
    ws.write_blank(r_hdr2, 4, &st.tbl_header)?;
    row += 1;

    if m.warnings.is_empty() {
        ws.write_number_with_format(row, 0, 1.0, &st.text)?;
        ws.write_string_with_format(row, 1, "INFO", &st.badge_info)?;
        ws.write_string_with_format(row, 2, "OK", &st.text)?;
        ws.write_string_with_format(row, 3, t_lang("excel.no_warnings", lang), &st.text)?;
        ws.write_blank(row, 4, &st.text)?;
    } else {
        for (i, w) in m.warnings.iter().enumerate() {
            let (badge_txt, badge_fmt) = match w.sev {
                Sev::Err => ("ERROR", &st.badge_err),
                Sev::Warn => ("WARN", &st.badge_warn),
                Sev::Info => ("INFO", &st.badge_info),
            };
            let area_str = t_lang(w.area_key, lang);
            let msg_str = tf_lang(w.msg_key, lang, &w.args);

            ws.write_number_with_format(row, 0, (i + 1) as f64, &st.text)?;
            ws.write_string_with_format(row, 1, badge_txt, badge_fmt)?;
            ws.write_string_with_format(row, 2, area_str, &st.text)?;
            ws.write_string_with_format(row, 3, msg_str, &st.text)?;
            ws.write_blank(row, 4, &st.text)?;
            row += 1;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Library;

    #[test]
    fn test_excel_export_bytes_all_examples() {
        let examples = example_files();

        let lib = Library::embedded();

        for ex in examples {
            let p = crate::project::load_example(&ex)
                .unwrap_or_else(|e| panic!("Örnek proje yüklenemedi: {} ({})", ex, e));
            let m = crate::calc::evaluate(&p, &lib);

            // Türkçe ve İngilizce dışa aktarımı test et
            for lang in [Lang::Tr, Lang::En] {
                let bytes = generate_xlsx_bytes(&p, &m, lang).unwrap_or_else(|e| {
                    panic!("{} ({:?}) xlsx bayt üretimi hatası: {}", ex, lang, e)
                });
                assert!(
                    !bytes.is_empty(),
                    "{} için üretilen xlsx boş olmamalıdır",
                    ex
                );

                // Zip dosya başlığı kontrolü (Excel dosyaları 'PK' ile başlar)
                assert_eq!(
                    &bytes[0..2],
                    b"PK",
                    "{} ({:?}) geçerli bir zip/xlsx arşivi olmalıdır",
                    ex,
                    lang
                );
            }
        }
    }

    #[test]
    fn test_excel_export_to_tempfile_isolation() {
        let examples = example_files();

        let lib = Library::embedded();
        let tmp_dir = tempfile::tempdir().expect("Geçici dizin oluşturulamadı");

        for ex in examples {
            let p = crate::project::load_example(&ex)
                .unwrap_or_else(|e| panic!("Örnek proje yüklenemedi: {} ({})", ex, e));
            let m = crate::calc::evaluate(&p, &lib);

            let file_path = tmp_dir.path().join(format!("{}.xlsx", ex));
            export_project_to_xlsx(&p, &m, Lang::Tr, &file_path)
                .unwrap_or_else(|e| panic!("{} için dosyaya kaydetme başarısız: {}", ex, e));
            assert!(
                file_path.exists(),
                "{} geçici dizine başarıyla yazılmış olmalıdır",
                ex
            );

            let meta = std::fs::metadata(&file_path).expect("Dosya meta verisi okunamadı");
            assert!(
                meta.len() > 1000,
                "{} dosya boyutu anlamlı olmalıdır (>1KB)",
                ex
            );
        }
    }

    #[test]
    fn test_formula_registry_and_resolution() {
        let mut reg = CellRegistry::default();
        reg.register("h", "Girdiler", 5, "D");
        reg.register("isa_P", "Hesaplar", 8, "D");
        reg.register("isa_T", "Hesaplar", 7, "D");

        let res = reg.resolve("Hesaplar", "={isa_P}/(287.05*{isa_T})");
        assert_eq!(res, "=$D$8/(287.05*$D$7)");

        let cross = reg.resolve("Hesaplar", "=288.15-0.0065*{h}");
        assert_eq!(cross, "=288.15-0.0065*'Girdiler'!$D$5");
    }

    #[test]
    fn test_formula_strings_refer_to_correct_cells() {
        let p = crate::project::load_example("fixed_wing_2kg_1600mm.json")
            .expect("Sabit kanat örneği bulunamadı");
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let mut wb = Workbook::new();
        let styles = Styles::new();
        let mut reg = CellRegistry::default();

        let s_inputs = "Girdiler";
        let s_calcs = "Hesaplar";

        let ws_inputs = wb.add_worksheet();
        populate_inputs_sheet(ws_inputs, &p, &m, Lang::Tr, &styles, &mut reg, s_inputs)
            .expect("Girdiler sayfası doldurulamadı");

        // Girdiler koordinatları
        let h_coord = reg.get("h").expect("h kayıtlı olmalı");
        assert_eq!(h_coord.sheet, "Girdiler");
        assert_eq!(h_coord.col, "D");

        let b_coord = reg.get("b").expect("b kayıtlı olmalı");
        assert_eq!(b_coord.sheet, "Girdiler");
        assert_eq!(b_coord.col, "D");

        let s_coord = reg.get("s").expect("s kayıtlı olmalı");
        assert_eq!(s_coord.sheet, "Girdiler");
        assert_eq!(s_coord.col, "D");

        // Formül çözümleme testi
        let ar_formula = reg.resolve(s_calcs, "={b}^2/{s}");
        assert_eq!(
            ar_formula,
            format!(
                "='Girdiler'!$D${}^2/'Girdiler'!$D${}",
                b_coord.row, s_coord.row
            )
        );

        let temp_formula = reg.resolve(s_calcs, "=288.15-0.0065*{h}");
        assert_eq!(
            temp_formula,
            format!("=288.15-0.0065*'Girdiler'!$D${}", h_coord.row)
        );
    }

    #[test]
    fn test_cached_values_match_program_evaluation_fixed_wing() {
        let p = crate::project::load_example("fixed_wing_2kg_1600mm.json")
            .expect("Sabit kanat örneği bulunamadı");
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // ISA
        assert!((m.atmo.t - (288.15 - 0.0065 * p.h)).abs() < 1e-4);
        assert!((m.atmo.rho - (m.atmo.p / (287.05287 * m.atmo.t))).abs() < 1e-4);

        // Kanat Geometrisi
        assert!((m.wing.ar - (p.b * p.b / p.s)).abs() < 1e-9);
        let cr_expected = 2.0 * p.s / (p.b * (1.0 + p.taper));
        assert!((m.wing.c_r - cr_expected).abs() < 1e-9);
        assert!((m.wing.c_t - p.taper * cr_expected).abs() < 1e-9);

        // Stall ve Seyir
        assert!((m.ws - (m.weight.w / p.s)).abs() < 1e-9);
        assert!((m.cons.v_stall - (2.0 * m.ws / (m.atmo.rho * m.aero.cl_max)).sqrt()).abs() < 1e-9);
        assert!((m.perf_cr.cl - (m.ws / (0.5 * m.atmo.rho * p.v_cr * p.v_cr))).abs() < 1e-9);

        // Güç ve Süre
        let eta_pt = p.eta_esc * p.eta_mot * p.eta_p_cr;
        let p_aero_expected = m.weight.w * p.v_cr / m.perf_cr.ld;
        assert!((m.perf_cr.p_aero - p_aero_expected).abs() < 1e-6);
        let p_total_expected = (p_aero_expected / eta_pt) + p.p_hotel;
        assert!((m.perf_cr.p_total - p_total_expected).abs() < 1e-4);
    }

    #[test]
    fn test_cached_values_match_program_evaluation_multirotor() {
        let p =
            crate::project::load_example("quad_250_x.json").expect("Multirotor örneği bulunamadı");
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // Askı itkisi
        let n_motors = m.mr.n_motors as f64;
        assert!((m.mr.t_hover_motor - (m.mr.w / n_motors)).abs() < 1e-9);

        // Disk alanı ve yükü
        let a_single = 0.25 * std::f64::consts::PI * p.mr_d_prop * p.mr_d_prop;
        assert!((m.mr.a_disk_single - a_single).abs() < 1e-9);
        assert!((m.mr.disk_loading - (m.mr.w / m.mr.a_disk_total)).abs() < 1e-9);

        // T/W ve gaz
        assert!((m.mr.tw_ratio - (m.mr.t_max_total / m.mr.w)).abs() < 1e-9);
        assert!((m.mr.hover_throttle - (1.0 / m.mr.tw_ratio).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn test_cached_values_match_program_evaluation_vtol() {
        let p =
            crate::project::load_example("vtol_quadplane.json").expect("VTOL örneği bulunamadı");
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        // Hem kanat hem multirotor çıktıları mevcut olmalı
        assert!(m.wing.ar > 0.0);
        assert!(m.cons.v_stall > 0.0);
        assert!(m.mr.t_hover_motor > 0.0);
        assert!(m.mr.tw_ratio > 0.0);
        assert!(m.perf_cr.endurance_min() > 0.0);
        assert!(m.mr.hover_endurance_min > 0.0);
    }

    // ---- Doğrulama yardımcıları: minimal Excel formül değerlendiricisi ----

    struct Ev<'a> {
        c: Vec<char>,
        i: usize,
        sheet: &'a str,
        reg: &'a CellRegistry,
    }

    impl Ev<'_> {
        fn peek(&self) -> Option<char> {
            self.c.get(self.i).copied()
        }
        fn expr(&mut self) -> f64 {
            let mut v = self.term();
            while let Some(ch) = self.peek() {
                if ch == '+' || ch == '-' {
                    self.i += 1;
                    let r = self.term();
                    v = if ch == '+' { v + r } else { v - r };
                } else {
                    break;
                }
            }
            v
        }
        fn term(&mut self) -> f64 {
            let mut v = self.pow();
            while let Some(ch) = self.peek() {
                if ch == '*' || ch == '/' {
                    self.i += 1;
                    let r = self.pow();
                    v = if ch == '*' { v * r } else { v / r };
                } else {
                    break;
                }
            }
            v
        }
        fn pow(&mut self) -> f64 {
            let b = self.atom();
            if self.peek() == Some('^') {
                self.i += 1;
                let e = self.pow();
                return b.powf(e);
            }
            b
        }
        fn atom(&mut self) -> f64 {
            let ch = self.peek().expect("beklenmeyen formül sonu");
            if ch == '(' {
                self.i += 1;
                let v = self.expr();
                assert_eq!(self.peek(), Some(')'));
                self.i += 1;
                return v;
            }
            if ch == '-' {
                self.i += 1;
                return -self.atom();
            }
            if ch.is_ascii_digit() || ch == '.' {
                let st = self.i;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == 'e')
                {
                    self.i += 1;
                }
                return self.c[st..self.i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap();
            }
            if ch == '\'' || ch == '$' {
                let mut sheet = self.sheet.to_string();
                if ch == '\'' {
                    self.i += 1;
                    let st = self.i;
                    while self.peek() != Some('\'') {
                        self.i += 1;
                    }
                    sheet = self.c[st..self.i].iter().collect();
                    self.i += 2; // ' ve !
                }
                assert_eq!(self.peek(), Some('$'));
                self.i += 2; // $ ve sütun harfi
                assert_eq!(self.peek(), Some('$'));
                self.i += 1;
                let st = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.i += 1;
                }
                let row: u32 = self.c[st..self.i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap();
                return *self
                    .reg
                    .values
                    .get(&(sheet.clone(), row))
                    .unwrap_or_else(|| panic!("değer yok: {sheet}!{row}"));
            }
            let st = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                self.i += 1;
            }
            let name: String = self.c[st..self.i].iter().collect();
            assert_eq!(self.peek(), Some('('), "bilinmeyen simge {name}");
            self.i += 1;
            if name == "PI" {
                assert_eq!(self.peek(), Some(')'));
                self.i += 1;
                return std::f64::consts::PI;
            }
            let a = self.expr();
            assert_eq!(self.peek(), Some(')'));
            self.i += 1;
            match name.as_str() {
                "SQRT" => a.sqrt(),
                _ => panic!("desteklenmeyen işlev {name}"),
            }
        }
    }

    fn eval_formula(reg: &CellRegistry, sheet: &str, f: &str) -> f64 {
        let mut e = Ev {
            c: f.trim_start_matches('=').chars().collect(),
            i: 0,
            sheet,
            reg,
        };
        let v = e.expr();
        assert_eq!(e.i, e.c.len(), "formül tamamen çözümlenmedi: {f}");
        v
    }

    fn example_files() -> Vec<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .expect("examples dizini okunamadı")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".json"))
            .collect();
        v.sort();
        assert!(v.len() >= 6);
        v
    }

    /// Her formülün Excel'de hesaplanacak değeri, Rust modelinden gelen önbellek değeriyle
    /// aynı olmalı (birim/katsayı/başvuru hatalarını yakalar).
    #[test]
    fn test_formulas_evaluate_to_cached_values_all_examples() {
        let lib = Library::embedded();
        for ex in example_files() {
            let p = crate::project::load_example(&ex).unwrap();
            let m = crate::calc::evaluate(&p, &lib);
            for lang in [Lang::Tr, Lang::En] {
                let (_wb, reg) = build_workbook(&p, &m, lang).unwrap();
                assert!(!reg.formulas.is_empty());
                for f in &reg.formulas {
                    let v = eval_formula(&reg, &f.sheet, &f.formula);
                    let tol = 1e-6 * f.cached.abs().max(1.0);
                    assert!(
                        (v - f.cached).abs() <= tol,
                        "{ex}: {} formülü {} -> Excel {v}, önbellek {}",
                        f.id,
                        f.formula,
                        f.cached
                    );
                }
            }
        }
    }

    /// Tür bazlı satırlar: yalnızca ilgili formüller yazılmalı.
    #[test]
    fn test_type_specific_rows() {
        let lib = Library::embedded();
        for ex in example_files() {
            let p = crate::project::load_example(&ex).unwrap();
            let m = crate::calc::evaluate(&p, &lib);
            let (_wb, reg) = build_workbook(&p, &m, Lang::En).unwrap();
            let has = |id: &str| reg.formulas.iter().any(|f| f.id == id);
            assert_eq!(has("v_stall"), p.drone_type.has_wing(), "{ex} v_stall");
            assert_eq!(has("ld_cr"), p.drone_type.has_wing(), "{ex} ld_cr");
            assert_eq!(
                has("mr_t_hov"),
                p.drone_type.is_multirotor(),
                "{ex} mr_t_hov"
            );
            assert_eq!(has("mr_throttle"), p.drone_type.is_multirotor(), "{ex} thr");
            assert!(has("rho") && has("e_use_wh"), "{ex} ortak satırlar");
        }
    }

    /// Önemli formüllerin dizgileri doğru girdi hücrelerine başvurmalı.
    #[test]
    fn test_key_formula_strings() {
        let lib = Library::embedded();
        let p = crate::project::load_example("fixed_wing_2kg_1600mm.json").unwrap();
        let m = crate::calc::evaluate(&p, &lib);
        let (_wb, reg) = build_workbook(&p, &m, Lang::En).unwrap();
        let inp = |id: &str| reg.get(id).unwrap().to_ref("Calculations");
        let calc = |id: &str| reg.get(id).unwrap().abs();
        let f = |id: &str| {
            reg.formulas
                .iter()
                .find(|f| f.id == id)
                .unwrap()
                .formula
                .clone()
        };
        assert_eq!(f("ar"), format!("={}^2/{}", inp("b"), inp("s")));
        assert_eq!(
            f("v_stall"),
            format!(
                "=SQRT(2*{}/({}*{}))",
                calc("ws"),
                calc("rho"),
                calc("cl_max")
            )
        );
        assert_eq!(
            f("cl_cr"),
            format!("={}/(0.5*{}*{}^2)", calc("ws"), calc("rho"), inp("v_cr"))
        );
        assert_eq!(f("ld_cr"), format!("={}/{}", calc("cl_cr"), calc("cd_cr")));
        assert_eq!(f("ws"), format!("={}/{}", calc("W"), inp("s")));
        assert!(inp("b").starts_with("'Inputs'!$D$"));
    }

    /// Dosyaya yazılan her sabit metin, seçilen dilde karşı dilin metni olmamalı.
    #[test]
    fn test_texts_are_localized() {
        use crate::export::texts::PAIRS;
        let lib = Library::embedded();
        for ex in example_files() {
            let p = crate::project::load_example(&ex).unwrap();
            let m = crate::calc::evaluate(&p, &lib);
            let (_wb, reg_en) = build_workbook(&p, &m, Lang::En).unwrap();
            let (_wb, reg_tr) = build_workbook(&p, &m, Lang::Tr).unwrap();
            for t in &reg_en.texts {
                assert!(
                    !PAIRS.iter().any(|(en, tr)| tr == t && en != tr),
                    "{ex}: EN dosyada Türkçe metin: {t}"
                );
            }
            for t in &reg_tr.texts {
                assert!(
                    !PAIRS.iter().any(|(en, tr)| en == t && en != tr),
                    "{ex}: TR dosyada İngilizce metin: {t}"
                );
            }
        }
    }
}
