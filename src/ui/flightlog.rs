// SPDX-License-Identifier: AGPL-3.0-or-later
//! Uçuş logu sekmesi: CSV içe aktarma, sütun eşleme, özet/kesim tabloları, tasarım-gerçek
//! karşılaştırması ve zaman serisi grafiği. Proje değerlerini DEĞİŞTİRMEZ; durum egui belleğindedir.
use super::components::{self, Status, section_header};
use super::theme::{self, space};
use super::widgets::{fmt, section};
use crate::calc::{Library, Model};
use crate::flightlog::bin;
use crate::flightlog::compare::{self, DesignPrediction, Metric};
use crate::flightlog::csv::{
    self, AltUnit, CurUnit, Field, Log, Mapping, SpeedUnit, ThrottleUnit, TimeUnit, VoltUnit,
};
use crate::flightlog::segments::{self, Analysis, SegKind};
use crate::i18n::{t, tf};
use crate::project::Project;
use egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotPoints, Polygon};
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Grafikte çizilecek en çok nokta (seriler seyreltilir).
const PLOT_POINTS: usize = 3000;

#[derive(Clone)]
struct PlotData {
    volt: Vec<[f64; 2]>,
    cur: Vec<[f64; 2]>,
    power: Vec<[f64; 2]>,
    alt: Vec<[f64; 2]>,
}

/// Yüklenmiş ve çözümlenmiş log (ham log bellekte tutulmaz).
struct Loaded {
    analysis: Analysis,
    plot: PlotData,
    samples: usize,
    bad_rows: usize,
    total_rows: usize,
    truncated: bool,
    time_assumed: bool,
    has_power: bool,
    has_speed: bool,
}

type Pending = Arc<Mutex<Option<Result<Loaded, String>>>>;

#[derive(Clone, Default)]
struct State {
    path: Option<PathBuf>,
    headers: Vec<String>,
    map: Mapping,
    loaded: Option<Arc<Loaded>>,
    error: Option<String>,
    pending: Option<Pending>,
    /// (ölçülen seyir hızı + tasarım parmak izi) → tahmin.
    pred: Option<(u64, DesignPrediction)>,
}

fn state_id() -> egui::Id {
    egui::Id::NULL.with("flightlog_state")
}

fn decimate(t: &[f64], y: impl Fn(usize) -> f64) -> Vec<[f64; 2]> {
    let n = t.len();
    let stride = n.div_ceil(PLOT_POINTS).max(1);
    (0..n)
        .step_by(stride)
        .filter_map(|k| {
            let v = y(k);
            v.is_finite().then(|| [t[k], v])
        })
        .collect()
}

fn build_loaded(log: &Log) -> Loaded {
    let analysis = segments::analyze(log);
    let plot = PlotData {
        volt: if log.volt.is_empty() {
            vec![]
        } else {
            decimate(&log.t, |k| log.v_at(k))
        },
        cur: if log.cur.is_empty() {
            vec![]
        } else {
            decimate(&log.t, |k| log.i_at(k))
        },
        power: if log.has_power() {
            decimate(&log.t, |k| log.power_at(k))
        } else {
            vec![]
        },
        alt: if log.alt.is_empty() {
            vec![]
        } else {
            decimate(&log.t, |k| log.alt_at(k))
        },
    };
    Loaded {
        analysis,
        plot,
        samples: log.len(),
        bad_rows: log.bad_rows,
        total_rows: log.total_rows,
        truncated: log.truncated,
        time_assumed: log.time_assumed,
        has_power: log.has_power(),
        has_speed: log.has_speed(),
    }
}

/// CSV dosyasını akış olarak açar; ArduPilot .bin ise önce `flightlog-core` ile CSV'ye çevirir.
fn open_reader(path: &std::path::Path) -> Result<Box<dyn std::io::BufRead + Send>, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut magic = [0u8; 2];
    let is_bin = std::io::Read::read_exact(&mut f, &mut magic).is_ok() && bin::is_dataflash(&magic);
    if is_bin {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let text = bin::bin_to_csv(&bytes)?;
        return Ok(Box::new(std::io::Cursor::new(text.into_bytes())));
    }
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    Ok(Box::new(BufReader::with_capacity(1 << 20, f)))
}

/// Dosyayı arka plan iş parçacığında okur ve çözümler.
fn start_load(st: &mut State, ctx: &egui::Context) {
    let Some(path) = st.path.clone() else { return };
    let map = st.map.clone();
    let slot: Pending = Arc::new(Mutex::new(None));
    st.pending = Some(slot.clone());
    st.error = None;
    st.pred = None;
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("flightlog".into()).spawn({
        let slot = slot.clone();
        move || {
            let res = match open_reader(&path) {
                Ok(r) => csv::read_log(r, &map, csv::MAX_ROWS)
                    .map(|l| build_loaded(&l))
                    .map_err(|e| match e {
                        csv::LogError::Empty => t("flightlog.err_empty").to_string(),
                        csv::LogError::NoValidRows => t("flightlog.err_no_rows").to_string(),
                        csv::LogError::Io(m) => tf("flightlog.err_io", &[m]),
                    }),
                Err(e) => Err(tf("flightlog.err_io", &[e])),
            };
            if let Ok(mut g) = slot.lock() {
                *g = Some(res);
            }
            ctx.request_repaint();
        }
    });
    if spawned.is_err() {
        st.pending = None;
        st.error = Some(t("flightlog.err_thread").to_string());
    }
}

fn poll(st: &mut State) {
    let Some(slot) = st.pending.clone() else {
        return;
    };
    let done = slot.lock().ok().and_then(|mut g| g.take());
    match done {
        Some(Ok(l)) => {
            st.loaded = Some(Arc::new(l));
            st.pending = None;
            st.pred = None;
        }
        Some(Err(e)) => {
            st.loaded = None;
            st.error = Some(e);
            st.pending = None;
        }
        None => {}
    }
}

fn open_dialog(st: &mut State, ctx: &egui::Context) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV / ArduPilot .bin", &["csv", "txt", "tsv", "bin", "BIN"])
        .pick_file()
    else {
        return;
    };
    match open_reader(&path).and_then(|r| csv::read_headers(r).map_err(|e| format!("{e:?}"))) {
        Ok(h) => {
            st.map = csv::guess_mapping(&h);
            st.headers = h;
            st.path = Some(path);
            st.loaded = None;
            st.pred = None;
            start_load(st, ctx);
        }
        Err(e) => {
            st.error = Some(tf("flightlog.err_io", &[e]));
        }
    }
}

/// Test/örnek: hazır bir logu doğrudan yükler (dosya diyaloğu olmadan).
#[cfg(test)]
pub(crate) fn inject_log(ctx: &egui::Context, log: &Log, headers: Vec<String>) {
    let mut st = State {
        map: csv::guess_mapping(&headers),
        headers,
        loaded: Some(Arc::new(build_loaded(log))),
        ..Default::default()
    };
    st.path = Some(PathBuf::from("flight.csv"));
    ctx.data_mut(|d| d.insert_temp(state_id(), st));
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, lib: &Library) {
    let ctx = ui.ctx().clone();
    let mut st: State = ctx
        .data_mut(|d| d.remove_temp(state_id()))
        .unwrap_or_default();
    show_state(ui, p, m, lib, &mut st);
    ctx.data_mut(|d| d.insert_temp(state_id(), st));
}

fn show_state(ui: &mut Ui, p: &Project, m: &Model, lib: &Library, st: &mut State) {
    poll(st);
    let tk = theme::tokens(ui);
    section_header(ui, t("tab_flightlog"));
    components::banner(ui, Status::Info, t("flightlog.intro"));
    ui.add_space(space::XS);
    ui.label(egui::RichText::new(t("flightlog.formats")).weak());
    ui.add_space(space::XS);
    let loading = st.pending.is_some();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!loading, egui::Button::new(t("flightlog.open")))
            .clicked()
        {
            let c = ui.ctx().clone();
            open_dialog(st, &c);
        }
        if let Some(path) = &st.path {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            ui.label(tf("flightlog.file_fmt", &[name]));
        }
        if loading {
            ui.spinner();
            ui.label(t("flightlog.loading"));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    });
    if let Some(e) = &st.error {
        ui.colored_label(tk.danger, e);
    }
    if !st.headers.is_empty() {
        mapping_ui(ui, st);
    }
    let Some(l) = st.loaded.clone() else {
        if !loading && st.error.is_none() {
            ui.add_space(space::XS);
            ui.label(t("flightlog.no_log"));
        }
        return;
    };
    ui.separator();
    warnings(ui, &l);
    summary_ui(ui, &l);
    segments_ui(ui, &l);
    comparison_ui(ui, p, m, lib, &l, st);
    plot_ui(ui, &l);
}

// ---------------------------------------------------------------------------------------------

fn field_label(f: Field) -> &'static str {
    match f {
        Field::Time => t("flightlog.field.time"),
        Field::Voltage => t("flightlog.field.voltage"),
        Field::Current => t("flightlog.field.current"),
        Field::Throttle => t("flightlog.field.throttle"),
        Field::Altitude => t("flightlog.field.altitude"),
        Field::GroundSpeed => t("flightlog.field.gspeed"),
        Field::AirSpeed => t("flightlog.field.aspeed"),
        Field::Rpm => t("flightlog.field.rpm"),
    }
}

fn unit_combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    cur: &mut T,
    opts: &[(T, &'static str)],
) -> bool {
    let sel = opts
        .iter()
        .find(|o| o.0 == *cur)
        .map(|o| o.1)
        .unwrap_or("?");
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(sel)
        .width(90.0)
        .show_ui(ui, |ui| {
            for (v, name) in opts {
                changed |= ui.selectable_value(cur, *v, *name).changed();
            }
        });
    changed
}

fn apply_inferred_unit(map: &mut Mapping, f: Field, header: &str) {
    match f {
        Field::Time => map.time_unit = csv::infer_time_unit(header),
        Field::Voltage => map.volt_unit = csv::infer_volt_unit(header),
        Field::Current => map.cur_unit = csv::infer_cur_unit(header),
        Field::Throttle => map.thr_unit = csv::infer_thr_unit(header),
        Field::Altitude => map.alt_unit = csv::infer_alt_unit(header),
        Field::GroundSpeed => map.gs_unit = csv::infer_speed_unit(header),
        Field::AirSpeed => map.asp_unit = csv::infer_speed_unit(header),
        Field::Rpm => {}
    }
}

fn mapping_ui(ui: &mut Ui, st: &mut State) {
    let auto = t("flightlog.auto");
    egui::CollapsingHeader::new(t("flightlog.mapping_title"))
        .default_open(st.loaded.is_none())
        .show(ui, |ui| {
            ui.label(egui::RichText::new(t("flightlog.map_hint")).weak());
            let headers = st.headers.clone();
            egui::Grid::new("flightlog_map")
                .num_columns(3)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for f in Field::ALL {
                        ui.label(field_label(f));
                        let cur = st.map.get(f);
                        let text = match cur {
                            Some(i) => headers.get(i).map(|s| s.as_str()).unwrap_or("?"),
                            None => t("flightlog.none"),
                        };
                        egui::ComboBox::from_id_salt(("flightlog_col", f as u8))
                            .selected_text(text)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(cur.is_none(), t("flightlog.none"))
                                    .clicked()
                                {
                                    st.map.set(f, None);
                                    apply_inferred_unit(&mut st.map, f, "");
                                }
                                for (i, h) in headers.iter().enumerate() {
                                    if ui.selectable_label(cur == Some(i), h).clicked() {
                                        st.map.set(f, Some(i));
                                        apply_inferred_unit(&mut st.map, f, h);
                                    }
                                }
                            });
                        match f {
                            Field::Time => {
                                unit_combo(
                                    ui,
                                    "fl_u_time",
                                    &mut st.map.time_unit,
                                    &[
                                        (TimeUnit::Auto, auto),
                                        (TimeUnit::Micro, "µs"),
                                        (TimeUnit::Milli, "ms"),
                                        (TimeUnit::Sec, "s"),
                                    ],
                                );
                            }
                            Field::Voltage => {
                                unit_combo(
                                    ui,
                                    "fl_u_volt",
                                    &mut st.map.volt_unit,
                                    &[
                                        (VoltUnit::Auto, auto),
                                        (VoltUnit::Volt, "V"),
                                        (VoltUnit::MilliVolt, "mV"),
                                        (VoltUnit::CentiVolt, "cV"),
                                    ],
                                );
                            }
                            Field::Current => {
                                unit_combo(
                                    ui,
                                    "fl_u_cur",
                                    &mut st.map.cur_unit,
                                    &[
                                        (CurUnit::Auto, auto),
                                        (CurUnit::Amp, "A"),
                                        (CurUnit::MilliAmp, "mA"),
                                        (CurUnit::CentiAmp, "cA"),
                                    ],
                                );
                            }
                            Field::Throttle => {
                                unit_combo(
                                    ui,
                                    "fl_u_thr",
                                    &mut st.map.thr_unit,
                                    &[
                                        (ThrottleUnit::Auto, auto),
                                        (ThrottleUnit::Fraction, "0–1"),
                                        (ThrottleUnit::Percent, "%"),
                                        (ThrottleUnit::Pwm, "PWM µs"),
                                        (ThrottleUnit::DShot, "DShot"),
                                    ],
                                );
                            }
                            Field::Altitude => {
                                unit_combo(
                                    ui,
                                    "fl_u_alt",
                                    &mut st.map.alt_unit,
                                    &[
                                        (AltUnit::Auto, auto),
                                        (AltUnit::Meter, "m"),
                                        (AltUnit::Centimeter, "cm"),
                                        (AltUnit::Millimeter, "mm"),
                                    ],
                                );
                            }
                            Field::GroundSpeed => {
                                unit_combo(
                                    ui,
                                    "fl_u_gs",
                                    &mut st.map.gs_unit,
                                    &[
                                        (SpeedUnit::Auto, auto),
                                        (SpeedUnit::MetersPerSec, "m/s"),
                                        (SpeedUnit::KmPerHour, "km/h"),
                                        (SpeedUnit::CmPerSec, "cm/s"),
                                    ],
                                );
                            }
                            Field::AirSpeed => {
                                unit_combo(
                                    ui,
                                    "fl_u_asp",
                                    &mut st.map.asp_unit,
                                    &[
                                        (SpeedUnit::Auto, auto),
                                        (SpeedUnit::MetersPerSec, "m/s"),
                                        (SpeedUnit::KmPerHour, "km/h"),
                                        (SpeedUnit::CmPerSec, "cm/s"),
                                    ],
                                );
                            }
                            Field::Rpm => {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            if ui.button(t("flightlog.reload")).clicked() {
                let c = ui.ctx().clone();
                start_load(st, &c);
            }
        });
}

fn warnings(ui: &mut Ui, l: &Loaded) {
    if l.bad_rows > 0 {
        components::banner(
            ui,
            Status::Warn,
            &tf(
                "flightlog.warn_bad",
                &[l.bad_rows.to_string(), l.total_rows.to_string()],
            ),
        );
    }
    if l.truncated {
        components::banner(
            ui,
            Status::Warn,
            &tf("flightlog.warn_trunc", &[csv::MAX_ROWS.to_string()]),
        );
    }
    if l.time_assumed {
        components::banner(ui, Status::Warn, t("flightlog.warn_time"));
    }
    if !l.has_power {
        components::banner(ui, Status::Warn, t("flightlog.warn_nopower"));
    }
    if !l.has_speed {
        components::banner(ui, Status::Warn, t("flightlog.warn_nospeed"));
    }
}

fn nan_dash(x: f64, dec: usize) -> String {
    if x.is_finite() {
        fmt(x, dec)
    } else {
        "—".to_string()
    }
}

fn summary_ui(ui: &mut Ui, l: &Loaded) {
    section(ui, t("flightlog.summary"));
    let o = &l.analysis.overall;
    egui::Grid::new("flightlog_summary")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            let mut row = |k: &'static str, v: String| {
                ui.label(t(k));
                ui.label(v);
                ui.end_row();
            };
            row(
                "flightlog.sum.duration",
                format!("{} s", fmt(o.duration_s, 1)),
            );
            row(
                "flightlog.sum.samples",
                format!(
                    "{} ({} {})",
                    l.samples,
                    l.bad_rows,
                    t("flightlog.sum.skipped")
                ),
            );
            row(
                "flightlog.sum.mean_p",
                format!("{} W", nan_dash(o.mean_p, 1)),
            );
            row(
                "flightlog.sum.active_p",
                format!("{} W", nan_dash(o.active_mean_p, 1)),
            );
            row("flightlog.sum.max_p", format!("{} W", nan_dash(o.max_p, 1)));
            row(
                "flightlog.sum.mean_i",
                format!("{} A", nan_dash(o.mean_i, 2)),
            );
            row("flightlog.sum.max_i", format!("{} A", nan_dash(o.max_i, 2)));
            row("flightlog.sum.min_v", format!("{} V", nan_dash(o.min_v, 2)));
            row("flightlog.sum.mah", format!("{} mAh", fmt(o.used_mah, 0)));
            row("flightlog.sum.wh", format!("{} Wh", fmt(o.used_wh, 2)));
            let res = match &l.analysis.resistance {
                Some(r) => format!(
                    "{} mΩ  (V₀ = {} V, R² = {})",
                    fmt(r.r_ohm * 1000.0, 1),
                    fmt(r.v0, 2),
                    fmt(r.r2, 2)
                ),
                None => t("flightlog.sum.res_na").to_string(),
            };
            row("flightlog.sum.res", res);
        });
}

fn segments_ui(ui: &mut Ui, l: &Loaded) {
    section(ui, t("flightlog.segments"));
    if l.analysis.segments.is_empty() {
        ui.label(t("flightlog.seg.none"));
        return;
    }
    egui::Grid::new("flightlog_segments")
        .num_columns(10)
        .striped(true)
        .show(ui, |ui| {
            for k in [
                "flightlog.seg.kind",
                "flightlog.seg.time",
                "flightlog.seg.dur",
                "flightlog.seg.v",
                "flightlog.seg.i",
                "flightlog.seg.p",
                "flightlog.seg.thr",
                "flightlog.seg.speed",
                "flightlog.seg.alt",
                "flightlog.seg.rpm",
            ] {
                ui.label(egui::RichText::new(t(k)).strong());
            }
            ui.end_row();
            for s in &l.analysis.segments {
                ui.label(match s.kind {
                    SegKind::Hover => t("flightlog.seg.hover"),
                    SegKind::Cruise => t("flightlog.seg.cruise"),
                });
                ui.label(format!("{} – {}", fmt(s.t0, 0), fmt(s.t1, 0)));
                ui.label(fmt(s.duration(), 0));
                ui.label(nan_dash(s.mean_v, 2));
                ui.label(nan_dash(s.mean_i, 2));
                ui.label(nan_dash(s.mean_p, 1));
                ui.label(nan_dash(s.mean_thr, 0));
                ui.label(nan_dash(s.mean_speed, 1));
                ui.label(nan_dash(s.mean_alt, 1));
                ui.label(nan_dash(s.mean_rpm, 0));
                ui.end_row();
            }
        });
}

fn metric_label(m: Metric, v: Option<f64>) -> String {
    match m {
        Metric::HoverPower => t("flightlog.cmp.hover_power").to_string(),
        Metric::HoverCurrent => t("flightlog.cmp.hover_current").to_string(),
        Metric::CruisePower => tf(
            "flightlog.cmp.cruise_power",
            &[v.map(|x| fmt(x, 1)).unwrap_or_else(|| "—".into())],
        ),
        Metric::FlightTime => t("flightlog.cmp.flight_time").to_string(),
    }
}

fn unit_of(m: Metric) -> &'static str {
    match m {
        Metric::HoverPower | Metric::CruisePower => "W",
        Metric::HoverCurrent => "A",
        Metric::FlightTime => "min",
    }
}

/// Yüzdeye göre düz dilde yorum.
fn interpretation(m: Metric, diff: f64) -> String {
    let key = match (m, compare::verdict(diff)) {
        (Metric::HoverPower, "high") => "flightlog.interp.hover_power.high",
        (Metric::HoverPower, "low") => "flightlog.interp.hover_power.low",
        (Metric::HoverPower, _) => "flightlog.interp.hover_power.ok",
        (Metric::HoverCurrent, "high") => "flightlog.interp.hover_current.high",
        (Metric::HoverCurrent, "low") => "flightlog.interp.hover_current.low",
        (Metric::HoverCurrent, _) => "flightlog.interp.hover_current.ok",
        (Metric::CruisePower, "high") => "flightlog.interp.cruise_power.high",
        (Metric::CruisePower, "low") => "flightlog.interp.cruise_power.low",
        (Metric::CruisePower, _) => "flightlog.interp.cruise_power.ok",
        (Metric::FlightTime, "high") => "flightlog.interp.flight_time.high",
        (Metric::FlightTime, "low") => "flightlog.interp.flight_time.low",
        (Metric::FlightTime, _) => "flightlog.interp.flight_time.ok",
    };
    tf(key, &[fmt(diff.abs(), 0)])
}

fn pred_cache_key(p: &Project, m: &Model, v_meas: Option<f64>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    v_meas.map(f64::to_bits).hash(&mut h);
    std::mem::discriminant(&p.drone_type).hash(&mut h);
    for x in [
        p.mr_v_cruise,
        p.mr_flat_plate_area,
        p.mr_coax_kappa,
        p.mr_h_lx,
        p.mr_h_wy,
        p.p_hotel,
        m.mr.p_total_hover,
        m.mr.p_total_cruise,
        m.mr.i_hover,
        m.mr.e_usable_wh,
        m.mr.hover_endurance_min,
        m.mr.cruise_endurance_min,
        m.weight.pack_wh,
        m.weight.pack_voltage,
        m.perf_cr.v,
        m.perf_cr.p_total,
        m.perf_in.e_pack_wh,
        m.perf_in.dod,
        m.perf_in.eta_esc,
        m.perf_in.eta_mot,
        m.perf_in.eta_prop,
        m.perf_in.p_hotel,
        m.perf_in.cd0,
        m.perf_in.k,
        m.perf_in.rho,
        m.perf_in.ws,
        m.perf_in.w,
    ] {
        x.to_bits().hash(&mut h);
    }
    h.finish()
}

fn comparison_ui(ui: &mut Ui, p: &Project, m: &Model, lib: &Library, l: &Loaded, st: &mut State) {
    section(ui, t("flightlog.compare"));
    let v_meas = compare::measured_cruise_speed(&l.analysis);
    let key = pred_cache_key(p, m, v_meas);
    let pred = match st.pred {
        Some((k, d)) if k == key => d,
        _ => {
            let d = compare::design_prediction(p, m, lib, v_meas);
            st.pred = Some((key, d));
            d
        }
    };
    let rows = compare::compare(&l.analysis, &pred);
    if rows.is_empty() {
        ui.label(t("flightlog.cmp.none"));
        return;
    }
    let tk = theme::tokens(ui);
    egui::Grid::new("flightlog_compare")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for k in [
                "flightlog.cmp.metric",
                "flightlog.cmp.pred",
                "flightlog.cmp.meas",
                "flightlog.cmp.diff",
            ] {
                ui.label(egui::RichText::new(t(k)).strong());
            }
            ui.end_row();
            for r in &rows {
                ui.label(metric_label(r.metric, v_meas));
                ui.label(format!("{} {}", fmt(r.predicted, 1), unit_of(r.metric)));
                ui.label(format!("{} {}", fmt(r.measured, 1), unit_of(r.metric)));
                let col = match compare::verdict(r.diff_pct) {
                    "ok" => tk.success,
                    _ => tk.warning,
                };
                ui.colored_label(col, format!("{:+.1} %", r.diff_pct));
                ui.end_row();
            }
        });
    ui.add_space(space::XS);
    for r in &rows {
        let status = match compare::verdict(r.diff_pct) {
            "ok" => Status::Ok,
            _ => Status::Info,
        };
        components::banner(ui, status, &interpretation(r.metric, r.diff_pct));
        ui.add_space(2.0);
    }
    ui.label(egui::RichText::new(t("flightlog.cmp.note")).weak());
}

fn plot_ui(ui: &mut Ui, l: &Loaded) {
    section(ui, t("flightlog.plot_title"));
    let tk = theme::tokens(ui);
    let link = egui::Id::NULL.with("flightlog_x_link");
    let series: [(&'static str, &Vec<[f64; 2]>, egui::Color32); 4] = [
        (t("flightlog.plot.v"), &l.plot.volt, tk.accent),
        (t("flightlog.plot.i"), &l.plot.cur, tk.warning),
        (t("flightlog.plot.p"), &l.plot.power, tk.danger),
        (t("flightlog.plot.alt"), &l.plot.alt, tk.info),
    ];
    for (idx, (name, pts, color)) in series.iter().enumerate() {
        if pts.is_empty() {
            continue;
        }
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for q in pts.iter() {
            lo = lo.min(q[1]);
            hi = hi.max(q[1]);
        }
        if (hi - lo).abs() < 1e-9 {
            hi = lo + 1.0;
        }
        let pad = 0.05 * (hi - lo);
        let (lo, hi) = (lo - pad, hi + pad);
        Plot::new(("flightlog_plot", idx))
            .height(120.0)
            .allow_scroll(false)
            .link_axis(link, [true, false])
            .link_cursor(link, [true, false])
            .legend(Legend::default())
            .x_axis_label("t [s]")
            .show(ui, |pu| {
                for s in &l.analysis.segments {
                    let c = match s.kind {
                        SegKind::Hover => tk.accent,
                        SegKind::Cruise => tk.success,
                    };
                    let poly = Polygon::new(
                        "",
                        PlotPoints::from(vec![[s.t0, lo], [s.t1, lo], [s.t1, hi], [s.t0, hi]]),
                    )
                    .fill_color(c.gamma_multiply(0.18))
                    .stroke(egui::Stroke::NONE);
                    pu.polygon(poly);
                }
                pu.line(
                    Line::new(*name, PlotPoints::from((*pts).clone()))
                        .color(*color)
                        .width(1.5),
                );
            });
    }
    ui.label(egui::RichText::new(t("flightlog.plot.shade_note")).weak());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::flightlog::segments::tests::synth;
    use crate::i18n::Lang;
    use crate::project::DroneType;
    use crate::ui::theme::ThemeChoice;
    use egui::{Pos2, Rect, vec2};

    fn run(ctx: &egui::Context, f: &mut dyn FnMut(&mut egui::Ui)) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(1100.0, 900.0)));
        let mut out = ctx.run_ui(input, |ui| f(ui));
        out.textures_delta.clear();
    }

    fn sample_log() -> Log {
        let mut l = synth(120.0, |t| {
            if t < 10.0 {
                (12.0, 0.0, 0.0, 0.0)
            } else if t < 50.0 {
                (12.0 - 0.01 * t, 20.0 + (t * 0.5).sin() * 3.0, 0.1, 20.0)
            } else {
                (12.0 - 0.01 * t, 15.0 + (t * 0.3).sin() * 2.0, 14.0, 25.0)
            }
        });
        l.bad_rows = 3;
        l.total_rows = l.len() + 3;
        l
    }

    #[test]
    fn renders_in_both_themes_and_languages_with_and_without_log() {
        for dt in [DroneType::Multirotor, DroneType::FixedWingConventional] {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let lib = Library::embedded();
            let m = evaluate(&p, &lib);
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                for lang in [Lang::Tr, Lang::En] {
                    for with_log in [false, true] {
                        let ctx = egui::Context::default();
                        crate::ui::theme::install(&ctx, choice);
                        if with_log {
                            let hs: Vec<String> =
                                ["TimeUS", "BAT.Volt", "BAT.Curr", "GPS.Spd", "CTUN.Alt"]
                                    .iter()
                                    .map(|s| s.to_string())
                                    .collect();
                            inject_log(&ctx, &sample_log(), hs);
                        }
                        crate::i18n::with_lang(lang, || {
                            for _ in 0..2 {
                                run(&ctx, &mut |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        egui::ScrollArea::vertical().show(ui, |ui| {
                                            show(ui, &p, &m, &lib);
                                        });
                                    });
                                });
                            }
                        });
                    }
                }
            }
        }
    }

    #[test]
    fn interpretation_mentions_percent() {
        crate::i18n::with_lang(Lang::En, || {
            let s = interpretation(Metric::HoverPower, 18.0);
            assert!(s.contains("18"), "{s}");
            assert!(s.contains("higher"), "{s}");
            let s = interpretation(Metric::FlightTime, -25.0);
            assert!(s.contains("25"), "{s}");
        });
    }

    #[test]
    fn decimate_caps_points() {
        let t: Vec<f64> = (0..100_000).map(|k| k as f64).collect();
        let d = decimate(&t, |k| k as f64);
        assert!(d.len() <= PLOT_POINTS, "len={}", d.len());
        let edge: Vec<f64> = (0..(2 * PLOT_POINTS - 1)).map(|k| k as f64).collect();
        let d_edge = decimate(&edge, |k| k as f64);
        assert!(d_edge.len() <= PLOT_POINTS, "edge len={}", d_edge.len());
    }

    #[test]
    fn pred_cache_key_tracks_cell_config_and_drone_type() {
        let lib = Library::embedded();
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let mut m = evaluate(&p, &lib);
        let k0 = pred_cache_key(&p, &m, Some(10.0));
        // Aynı pack_wh ile S/P değişimi (pack_voltage, i_hover) anahtarı değiştirmeli.
        m.weight.pack_voltage *= 2.0;
        m.mr.i_hover *= 0.5;
        assert_ne!(k0, pred_cache_key(&p, &m, Some(10.0)));
        // Seyir gücü veya uçak türü değişimi anahtarı değiştirmeli.
        let m_base = evaluate(&p, &lib);
        let mut m2 = m_base.clone();
        m2.mr.p_total_cruise += 25.0;
        assert_ne!(
            pred_cache_key(&p, &m_base, None),
            pred_cache_key(&p, &m2, None)
        );
        p.set_drone_type(DroneType::FixedWingConventional);
        assert_ne!(pred_cache_key(&p, &m_base, Some(10.0)), k0);
    }
}
