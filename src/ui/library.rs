// SPDX-License-Identifier: AGPL-3.0-or-later
//! Component library tab (expert mode): browse, search, add, edit and delete the user's own
//! motors, propellers, ESCs and batteries, import / export shareable files, and show the
//! mechanical standards. Only batteries can be applied to the project today (custom pack);
//! the other kinds are shown but applying them is left for later, and the panel says so.
use super::components::{self, Status, section_header};
use super::theme::space;
use crate::i18n::{t, tf};
use crate::library::records::{Battery, Chemistry, Esc, Motor, Propeller, Record, Source};
use crate::library::standards::{
    ARM_PROFILES, CARBON_PLATE_THICKNESSES_MM, MOTOR_MOUNTS, MountPattern, STACK_PATTERNS,
};
use crate::library::store::{self, Category, ComponentLibrary, Item};
use crate::library::{samples, standards};
use crate::project::Project;
use egui::{RichText, Ui};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum SourceFilter {
    #[default]
    All,
    Examples,
    Mine,
}

/// One editable text field of the add / edit form.
struct Field {
    key: &'static str,
    value: String,
    choices: Vec<&'static str>,
}

fn field(key: &'static str, value: String) -> Field {
    Field {
        key,
        value,
        choices: Vec::new(),
    }
}

fn field_c(key: &'static str, value: String, choices: Vec<&'static str>) -> Field {
    Field {
        key,
        value,
        choices,
    }
}

struct Editor {
    category: Category,
    /// Name of the record being edited; `None` when adding a new one.
    original: Option<String>,
    fields: Vec<Field>,
    errors: Vec<String>,
}

/// Library tab state. Nothing touches the disk until the tab is first drawn.
pub struct LibraryUi {
    path: Option<PathBuf>,
    loaded: bool,
    user: ComponentLibrary,
    examples: ComponentLibrary,
    /// `None` shows the mechanical standards.
    view: Option<Category>,
    search: String,
    filter: SourceFilter,
    checked: HashSet<(Category, String)>,
    editor: Option<Editor>,
    pending_delete: Option<(Category, String)>,
    status: Option<(Status, String)>,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self {
            path: None,
            loaded: false,
            user: ComponentLibrary::default(),
            examples: ComponentLibrary::default(),
            view: Some(Category::Motor),
            search: String::new(),
            filter: SourceFilter::default(),
            checked: HashSet::new(),
            editor: None,
            pending_delete: None,
            status: None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Form <-> record conversion
// ---------------------------------------------------------------------------------------------

fn num(v: f64) -> String {
    format!("{v}")
}

fn opt(v: Option<f64>) -> String {
    v.map(num).unwrap_or_default()
}

struct Reader<'a> {
    fields: &'a [Field],
    errors: Vec<String>,
}

impl Reader<'_> {
    fn text(&self, key: &str) -> String {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.trim().to_string())
            .unwrap_or_default()
    }

    fn parse(&mut self, key: &'static str, raw: &str) -> Option<f64> {
        match raw.replace(',', ".").parse::<f64>() {
            Ok(v) => Some(v),
            Err(_) => {
                self.errors
                    .push(format!("{}: \"{raw}\" is not a number", t(key)));
                None
            }
        }
    }

    fn f64(&mut self, key: &'static str) -> f64 {
        let raw = self.text(key);
        self.parse(key, &raw).unwrap_or(f64::NAN)
    }

    fn opt(&mut self, key: &'static str) -> Option<f64> {
        let raw = self.text(key);
        if raw.is_empty() {
            None
        } else {
            self.parse(key, &raw)
        }
    }

    fn uint(&mut self, key: &'static str) -> u32 {
        let raw = self.text(key);
        raw.parse::<u32>().unwrap_or_else(|_| {
            self.errors
                .push(format!("{}: \"{raw}\" is not a whole number", t(key)));
            0
        })
    }

    fn finish<T: Record>(self, rec: T) -> Result<T, Vec<String>> {
        let mut errors = self.errors;
        errors.extend(rec.validate());
        if errors.is_empty() {
            Ok(rec)
        } else {
            Err(errors)
        }
    }
}

enum Built {
    Motor(Motor),
    Propeller(Propeller),
    Esc(Esc),
    Battery(Battery),
}

fn blank_fields(cat: Category) -> Vec<Field> {
    match cat {
        Category::Motor => motor_fields(&Motor {
            name: String::new(),
            kv: 1000.0,
            r_internal_ohm: 0.05,
            i_noload_a: 1.0,
            i_noload_v: 10.0,
            max_current_a: 20.0,
            max_power_w: 300.0,
            mass_g: 30.0,
            mount: None,
            notes: String::new(),
            source: Source::User,
        }),
        Category::Propeller => prop_fields(&Propeller {
            name: String::new(),
            diameter_in: 8.0,
            pitch_in: 4.0,
            blades: 2,
            mass_g: 10.0,
            ct: None,
            cp: None,
            notes: String::new(),
            source: Source::User,
        }),
        Category::Esc => esc_fields(&Esc {
            name: String::new(),
            continuous_a: 30.0,
            burst_a: None,
            cells_min: 2,
            cells_max: 4,
            mass_g: 10.0,
            notes: String::new(),
            source: Source::User,
        }),
        Category::Battery => battery_fields(&Battery {
            name: String::new(),
            chemistry: Chemistry::LiPo,
            cells_s: 4,
            parallel_p: 1,
            capacity_mah: 5000.0,
            c_rating: 30.0,
            r_internal_mohm: None,
            mass_g: 400.0,
            notes: String::new(),
            source: Source::User,
        }),
    }
}

fn motor_fields(m: &Motor) -> Vec<Field> {
    let mounts = MOTOR_MOUNTS.iter().map(|p| p.name).collect();
    vec![
        field("library.f.name", m.name.clone()),
        field("library.f.kv", num(m.kv)),
        field("library.f.r_internal", num(m.r_internal_ohm)),
        field("library.f.i_noload", num(m.i_noload_a)),
        field("library.f.v_noload", num(m.i_noload_v)),
        field("library.f.max_current", num(m.max_current_a)),
        field("library.f.max_power", num(m.max_power_w)),
        field("library.f.mass", num(m.mass_g)),
        field_c(
            "library.f.mount",
            m.mount.map(|p| p.to_string()).unwrap_or_default(),
            mounts,
        ),
        field("library.f.notes", m.notes.clone()),
    ]
}

fn build_motor(f: &[Field], source: Source) -> Result<Motor, Vec<String>> {
    let mut r = Reader {
        fields: f,
        errors: Vec::new(),
    };
    let mount_text = r.text("library.f.mount");
    let mount = if mount_text.is_empty() {
        None
    } else {
        match mount_text.parse::<MountPattern>() {
            Ok(m) => Some(m),
            Err(e) => {
                r.errors.push(e);
                None
            }
        }
    };
    let rec = Motor {
        name: r.text("library.f.name"),
        kv: r.f64("library.f.kv"),
        r_internal_ohm: r.f64("library.f.r_internal"),
        i_noload_a: r.f64("library.f.i_noload"),
        i_noload_v: r.f64("library.f.v_noload"),
        max_current_a: r.f64("library.f.max_current"),
        max_power_w: r.f64("library.f.max_power"),
        mass_g: r.f64("library.f.mass"),
        mount,
        notes: r.text("library.f.notes"),
        source,
    };
    r.finish(rec)
}

fn prop_fields(p: &Propeller) -> Vec<Field> {
    vec![
        field("library.f.name", p.name.clone()),
        field("library.f.diameter", num(p.diameter_in)),
        field("library.f.pitch", num(p.pitch_in)),
        field("library.f.blades", p.blades.to_string()),
        field("library.f.mass", num(p.mass_g)),
        field("library.f.ct", opt(p.ct)),
        field("library.f.cp", opt(p.cp)),
        field("library.f.notes", p.notes.clone()),
    ]
}

fn build_prop(f: &[Field], source: Source) -> Result<Propeller, Vec<String>> {
    let mut r = Reader {
        fields: f,
        errors: Vec::new(),
    };
    let rec = Propeller {
        name: r.text("library.f.name"),
        diameter_in: r.f64("library.f.diameter"),
        pitch_in: r.f64("library.f.pitch"),
        blades: r.uint("library.f.blades"),
        mass_g: r.f64("library.f.mass"),
        ct: r.opt("library.f.ct"),
        cp: r.opt("library.f.cp"),
        notes: r.text("library.f.notes"),
        source,
    };
    r.finish(rec)
}

fn esc_fields(e: &Esc) -> Vec<Field> {
    vec![
        field("library.f.name", e.name.clone()),
        field("library.f.continuous", num(e.continuous_a)),
        field("library.f.burst", opt(e.burst_a)),
        field("library.f.cells_min", e.cells_min.to_string()),
        field("library.f.cells_max", e.cells_max.to_string()),
        field("library.f.mass", num(e.mass_g)),
        field("library.f.notes", e.notes.clone()),
    ]
}

fn build_esc(f: &[Field], source: Source) -> Result<Esc, Vec<String>> {
    let mut r = Reader {
        fields: f,
        errors: Vec::new(),
    };
    let rec = Esc {
        name: r.text("library.f.name"),
        continuous_a: r.f64("library.f.continuous"),
        burst_a: r.opt("library.f.burst"),
        cells_min: r.uint("library.f.cells_min"),
        cells_max: r.uint("library.f.cells_max"),
        mass_g: r.f64("library.f.mass"),
        notes: r.text("library.f.notes"),
        source,
    };
    r.finish(rec)
}

fn battery_fields(b: &Battery) -> Vec<Field> {
    let chems = Chemistry::ALL.iter().map(|c| c.label()).collect();
    vec![
        field("library.f.name", b.name.clone()),
        field_c(
            "library.f.chemistry",
            b.chemistry.label().to_string(),
            chems,
        ),
        field("library.f.cells_s", b.cells_s.to_string()),
        field("library.f.parallel", b.parallel_p.to_string()),
        field("library.f.capacity", num(b.capacity_mah)),
        field("library.f.c_rating", num(b.c_rating)),
        field("library.f.r_mohm", opt(b.r_internal_mohm)),
        field("library.f.mass", num(b.mass_g)),
        field("library.f.notes", b.notes.clone()),
    ]
}

fn build_battery(f: &[Field], source: Source) -> Result<Battery, Vec<String>> {
    let mut r = Reader {
        fields: f,
        errors: Vec::new(),
    };
    let chem_text = r.text("library.f.chemistry");
    let chemistry = Chemistry::parse(&chem_text).unwrap_or_else(|| {
        r.errors.push(format!(
            "{}: \"{chem_text}\" is not a known chemistry",
            t("library.f.chemistry")
        ));
        Chemistry::LiPo
    });
    let rec = Battery {
        name: r.text("library.f.name"),
        chemistry,
        cells_s: r.uint("library.f.cells_s"),
        parallel_p: r.uint("library.f.parallel"),
        capacity_mah: r.f64("library.f.capacity"),
        c_rating: r.f64("library.f.c_rating"),
        r_internal_mohm: r.opt("library.f.r_mohm"),
        mass_g: r.f64("library.f.mass"),
        notes: r.text("library.f.notes"),
        source,
    };
    r.finish(rec)
}

fn build(cat: Category, f: &[Field], source: Source) -> Result<Built, Vec<String>> {
    Ok(match cat {
        Category::Motor => Built::Motor(build_motor(f, source)?),
        Category::Propeller => Built::Propeller(build_prop(f, source)?),
        Category::Esc => Built::Esc(build_esc(f, source)?),
        Category::Battery => Built::Battery(build_battery(f, source)?),
    })
}

// ---------------------------------------------------------------------------------------------
// Apply to project
// ---------------------------------------------------------------------------------------------

/// Copies a battery into the project's custom pack fields and selects the custom pack as the
/// battery source. This is the only record kind the project has matching fields for today.
pub fn apply_battery(p: &mut Project, b: &Battery) {
    use crate::calc::custom_battery::BatterySourceKind;
    let pack = &mut p.custom_pack;
    pack.name = b.name.clone();
    pack.chemistry = b.chemistry.label().to_string();
    pack.pack_voltage = b.nominal_v();
    pack.s_count = b.cells_s;
    pack.capacity_mah = b.capacity_mah;
    pack.pack_mass_g = b.mass_g;
    pack.max_cont_a = b.max_cont_a();
    pack.c_rating = b.c_rating;
    p.battery_source = BatterySourceKind::CustomPack;
}

// ---------------------------------------------------------------------------------------------
// State operations (no UI, testable)
// ---------------------------------------------------------------------------------------------

fn source_text(s: &Source) -> String {
    match s {
        Source::Example => t("library.source.example").to_string(),
        Source::User => t("library.source.user").to_string(),
        Source::Imported(f) => tf("library.source.imported_fmt", &[f.as_str()]),
    }
}

fn cat_label(c: Category) -> &'static str {
    match c {
        Category::Motor => t("library.cat.motor"),
        Category::Propeller => t("library.cat.propeller"),
        Category::Esc => t("library.cat.esc"),
        Category::Battery => t("library.cat.battery"),
    }
}

struct Row {
    name: String,
    summary: String,
    notes: String,
    source: Source,
}

fn rows_of<T: Item>(lib: &ComponentLibrary, out: &mut Vec<Row>) {
    for it in T::list(lib) {
        out.push(Row {
            name: it.name().to_string(),
            summary: it.summary(),
            notes: it.notes().to_string(),
            source: it.source().clone(),
        });
    }
}

impl LibraryUi {
    /// A state that stores its library at `path` instead of the app data directory.
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    fn path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(store::default_path)
    }

    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.examples = samples::examples();
        let path = self.path();
        match ComponentLibrary::load(&path) {
            Ok(lib) => self.user = lib,
            Err(e) => {
                // Keep the unreadable file next to the original so nothing is lost when the
                // library is saved again.
                let aside = path.with_extension("json.unreadable");
                let _ = std::fs::copy(&path, &aside);
                self.status = Some((
                    Status::Err,
                    tf(
                        "library.err.load_fmt",
                        &[e.to_string(), aside.display().to_string()],
                    ),
                ));
            }
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.user.save(&self.path()) {
            self.status = Some((Status::Err, tf("library.err.save_fmt", &[e.to_string()])));
        }
    }

    fn rows(&self, cat: Category) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut add = |lib: &ComponentLibrary| match cat {
            Category::Motor => rows_of::<Motor>(lib, &mut rows),
            Category::Propeller => rows_of::<Propeller>(lib, &mut rows),
            Category::Esc => rows_of::<Esc>(lib, &mut rows),
            Category::Battery => rows_of::<Battery>(lib, &mut rows),
        };
        if self.filter != SourceFilter::Mine {
            add(&self.examples);
        }
        if self.filter != SourceFilter::Examples {
            add(&self.user);
        }
        let q = self.search.trim().to_lowercase();
        rows.retain(|r| {
            q.is_empty()
                || r.name.to_lowercase().contains(&q)
                || r.summary.to_lowercase().contains(&q)
                || r.notes.to_lowercase().contains(&q)
        });
        rows
    }

    fn start_editor(&mut self, cat: Category, original: Option<&str>) {
        let fields = match original {
            None => blank_fields(cat),
            Some(name) => match cat {
                Category::Motor => self.user.find::<Motor>(name).map(motor_fields),
                Category::Propeller => self.user.find::<Propeller>(name).map(prop_fields),
                Category::Esc => self.user.find::<Esc>(name).map(esc_fields),
                Category::Battery => self.user.find::<Battery>(name).map(battery_fields),
            }
            .unwrap_or_else(|| blank_fields(cat)),
        };
        self.editor = Some(Editor {
            category: cat,
            original: original.map(str::to_string),
            fields,
            errors: Vec::new(),
        });
    }

    /// Validates the open form and stores the record. Problems stay in the form.
    fn submit_editor(&mut self) {
        let Some(ed) = self.editor.as_ref() else {
            return;
        };
        let (cat, original) = (ed.category, ed.original.clone());
        let source = original
            .as_deref()
            .and_then(|n| self.source_of(cat, n))
            .unwrap_or(Source::User);
        let built = build(cat, &ed.fields, source);
        let result = built.and_then(|built| match (built, original.as_deref()) {
            (Built::Motor(m), None) => self.user.add(m),
            (Built::Motor(m), Some(o)) => self.user.replace(o, m),
            (Built::Propeller(m), None) => self.user.add(m),
            (Built::Propeller(m), Some(o)) => self.user.replace(o, m),
            (Built::Esc(m), None) => self.user.add(m),
            (Built::Esc(m), Some(o)) => self.user.replace(o, m),
            (Built::Battery(m), None) => self.user.add(m),
            (Built::Battery(m), Some(o)) => self.user.replace(o, m),
        });
        match result {
            Ok(()) => {
                if let Some(o) = &original {
                    self.checked.remove(&(cat, o.clone()));
                }
                self.editor = None;
                self.save();
            }
            Err(errors) => {
                if let Some(ed) = self.editor.as_mut() {
                    ed.errors = errors;
                }
            }
        }
    }

    fn source_of(&self, cat: Category, name: &str) -> Option<Source> {
        match cat {
            Category::Motor => self.user.find::<Motor>(name).map(|x| x.source.clone()),
            Category::Propeller => self.user.find::<Propeller>(name).map(|x| x.source.clone()),
            Category::Esc => self.user.find::<Esc>(name).map(|x| x.source.clone()),
            Category::Battery => self.user.find::<Battery>(name).map(|x| x.source.clone()),
        }
    }

    /// Copies an example into the user's own library.
    fn copy_example(&mut self, cat: Category, name: &str) {
        fn copy<T: Item>(ui: &mut LibraryUi, name: &str) -> Result<(), Vec<String>> {
            let Some(mut it) = ui.examples_find::<T>(name) else {
                return Ok(());
            };
            it.set_source(Source::User);
            ui.user.add(it)
        }
        let r = match cat {
            Category::Motor => copy::<Motor>(self, name),
            Category::Propeller => copy::<Propeller>(self, name),
            Category::Esc => copy::<Esc>(self, name),
            Category::Battery => copy::<Battery>(self, name),
        };
        self.status = Some(match r {
            Ok(()) => {
                self.save();
                (Status::Ok, tf("library.status.copied_fmt", &[name]))
            }
            Err(e) => (Status::Warn, e.join("; ")),
        });
    }

    fn examples_find<T: Item>(&self, name: &str) -> Option<T> {
        self.examples.find::<T>(name).cloned()
    }

    fn delete(&mut self, cat: Category, name: &str) {
        let removed = match cat {
            Category::Motor => self.user.remove::<Motor>(name),
            Category::Propeller => self.user.remove::<Propeller>(name),
            Category::Esc => self.user.remove::<Esc>(name),
            Category::Battery => self.user.remove::<Battery>(name),
        };
        if removed {
            self.checked.remove(&(cat, name.to_string()));
            self.save();
            self.status = Some((Status::Info, tf("library.status.deleted_fmt", &[name])));
        }
    }

    fn import_from(&mut self, path: &Path) {
        self.status = Some(match self.user.import_file(path) {
            Ok(rep) => {
                if !rep.added.is_empty() {
                    self.save();
                }
                let text = tf(
                    "library.status.imported_fmt",
                    &[
                        rep.added.len().to_string(),
                        rep.duplicates.len().to_string(),
                        rep.conflicts.len().to_string(),
                        rep.invalid.len().to_string(),
                    ],
                );
                let status = if rep.invalid.is_empty() && rep.conflicts.is_empty() {
                    Status::Ok
                } else {
                    Status::Warn
                };
                let extra: Vec<String> = rep
                    .conflicts
                    .iter()
                    .map(|(_, n)| tf("library.status.conflict_fmt", &[n.as_str()]))
                    .chain(rep.invalid.iter().cloned())
                    .collect();
                if extra.is_empty() {
                    (status, text)
                } else {
                    (status, format!("{text}\n{}", extra.join("\n")))
                }
            }
            Err(e) => (Status::Err, tf("library.err.import_fmt", &[e.to_string()])),
        });
    }

    fn export_to(&mut self, path: &Path) {
        let keys: Vec<(Category, String)> = self.checked.iter().cloned().collect();
        self.status = Some(match self.user.export_file(&keys, path) {
            Ok(n) => (
                Status::Ok,
                tf(
                    "library.status.exported_fmt",
                    &[n.to_string(), path.display().to_string()],
                ),
            ),
            Err(e) => (Status::Err, tf("library.err.export_fmt", &[e.to_string()])),
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

enum Action {
    Add,
    Edit(String),
    AskDelete(String),
    Copy(String),
    Apply(String),
    Import,
    Export,
}

/// Draws the tab. Applying a battery changes `p`; everything else stays in `st`.
pub fn show(ui: &mut Ui, p: &mut Project, st: &mut LibraryUi) {
    st.ensure_loaded();
    egui::ScrollArea::vertical()
        .id_salt("library_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| body(ui, p, st));
}

fn body(ui: &mut Ui, p: &mut Project, st: &mut LibraryUi) {
    section_header(ui, t("library.title"));
    components::banner(ui, Status::Info, t("library.disclaimer"));
    ui.add_space(space::XS);
    if let Some((status, text)) = st.status.clone() {
        components::banner(ui, status, &text);
        ui.add_space(space::XS);
    }

    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            ui.selectable_value(&mut st.view, Some(c), cat_label(c));
        }
        ui.selectable_value(&mut st.view, None, t("library.cat.standards"));
    });
    ui.add_space(space::XS);

    let Some(cat) = st.view else {
        show_standards(ui);
        return;
    };

    // Toolbar.
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.search)
                .hint_text(t("library.search"))
                .desired_width(180.0),
        );
        egui::ComboBox::from_id_salt("library_filter")
            .selected_text(match st.filter {
                SourceFilter::All => t("library.filter.all"),
                SourceFilter::Examples => t("library.filter.examples"),
                SourceFilter::Mine => t("library.filter.mine"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.filter, SourceFilter::All, t("library.filter.all"));
                ui.selectable_value(
                    &mut st.filter,
                    SourceFilter::Examples,
                    t("library.filter.examples"),
                );
                ui.selectable_value(&mut st.filter, SourceFilter::Mine, t("library.filter.mine"));
            });
        if ui.button(t("library.btn.add")).clicked() {
            action = Some(Action::Add);
        }
        if ui.button(t("library.btn.import")).clicked() {
            action = Some(Action::Import);
        }
        let n = st.checked.len();
        if ui
            .add_enabled(
                n > 0,
                egui::Button::new(tf("library.btn.export_fmt", &[n.to_string()])),
            )
            .on_hover_text(t("library.export_hint"))
            .clicked()
        {
            action = Some(Action::Export);
        }
    });
    if cat != Category::Battery {
        ui.add_space(space::XS);
        components::banner(ui, Status::Info, t("library.apply_later"));
    }

    // Delete confirmation.
    if let Some((c, name)) = st.pending_delete.clone() {
        ui.add_space(space::XS);
        components::banner(
            ui,
            Status::Warn,
            &tf("library.delete_confirm_fmt", &[name.as_str()]),
        );
        ui.horizontal(|ui| {
            if ui.button(t("library.btn.delete")).clicked() {
                st.delete(c, &name);
                st.pending_delete = None;
            }
            if ui.button(t("library.btn.cancel")).clicked() {
                st.pending_delete = None;
            }
        });
    }

    show_editor(ui, st);

    ui.add_space(space::S);
    let rows = st.rows(cat);
    ui.label(
        RichText::new(tf("library.count_fmt", &[rows.len().to_string()]))
            .small()
            .weak(),
    );
    if rows.is_empty() {
        ui.label(t("library.empty"));
    } else {
        egui::Grid::new(("library_rows", cat as u8))
            .striped(true)
            .num_columns(5)
            .spacing([space::M, space::XS])
            .show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new(t("library.col.name")).strong());
                ui.label(RichText::new(t("library.col.details")).strong());
                ui.label(RichText::new(t("library.col.source")).strong());
                ui.label("");
                ui.end_row();
                for r in &rows {
                    let mine = r.source != Source::Example;
                    if mine {
                        let key = (cat, r.name.clone());
                        let mut on = st.checked.contains(&key);
                        if ui
                            .checkbox(&mut on, "")
                            .on_hover_text(t("library.export_hint"))
                            .changed()
                        {
                            if on {
                                st.checked.insert(key);
                            } else {
                                st.checked.remove(&key);
                            }
                        }
                    } else {
                        ui.label("");
                    }
                    let name = ui.label(&r.name);
                    if !r.notes.is_empty() {
                        name.on_hover_text(&r.notes);
                    }
                    ui.label(&r.summary);
                    ui.label(source_text(&r.source));
                    ui.horizontal(|ui| {
                        if mine {
                            if ui.small_button(t("library.btn.edit")).clicked() {
                                action = Some(Action::Edit(r.name.clone()));
                            }
                            if ui.small_button(t("library.btn.delete")).clicked() {
                                action = Some(Action::AskDelete(r.name.clone()));
                            }
                        } else if ui
                            .small_button(t("library.btn.copy"))
                            .on_hover_text(t("library.copy_hint"))
                            .clicked()
                        {
                            action = Some(Action::Copy(r.name.clone()));
                        }
                        if cat == Category::Battery
                            && ui
                                .small_button(t("library.btn.apply"))
                                .on_hover_text(t("library.apply_hint"))
                                .clicked()
                        {
                            action = Some(Action::Apply(r.name.clone()));
                        }
                    });
                    ui.end_row();
                }
            });
    }

    if let Some(a) = action {
        st.status = None;
        match a {
            Action::Add => st.start_editor(cat, None),
            Action::Edit(n) => st.start_editor(cat, Some(&n)),
            Action::AskDelete(n) => st.pending_delete = Some((cat, n)),
            Action::Copy(n) => st.copy_example(cat, &n),
            Action::Apply(n) => {
                let b = st
                    .user
                    .find::<Battery>(&n)
                    .or_else(|| st.examples.find::<Battery>(&n))
                    .cloned();
                if let Some(b) = b {
                    apply_battery(p, &b);
                    st.status = Some((Status::Ok, tf("library.status.applied_fmt", &[n.as_str()])));
                }
            }
            Action::Import => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON (*.json)", &["json"])
                    .pick_file()
                {
                    st.import_from(&path);
                }
            }
            Action::Export => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON (*.json)", &["json"])
                    .set_file_name("werseus_components.json")
                    .save_file()
                {
                    st.export_to(&path);
                }
            }
        }
    }
}

fn show_editor(ui: &mut Ui, st: &mut LibraryUi) {
    let Some(ed) = st.editor.as_mut() else {
        return;
    };
    ui.add_space(space::S);
    ui.label(
        RichText::new(format!(
            "{} - {}",
            if ed.original.is_some() {
                t("library.editor.edit")
            } else {
                t("library.editor.new")
            },
            cat_label(ed.category)
        ))
        .strong(),
    );
    egui::Grid::new("library_editor")
        .num_columns(2)
        .spacing([space::M, space::XS])
        .show(ui, |ui| {
            for f in &mut ed.fields {
                ui.label(t(f.key));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut f.value).desired_width(220.0));
                    if !f.choices.is_empty() {
                        egui::ComboBox::from_id_salt(("library_choice", f.key))
                            .selected_text("")
                            .width(24.0)
                            .show_ui(ui, |ui| {
                                for c in &f.choices {
                                    if ui.selectable_label(f.value == *c, *c).clicked() {
                                        f.value = (*c).to_string();
                                    }
                                }
                            });
                    }
                });
                ui.end_row();
            }
        });
    if !ed.errors.is_empty() {
        components::banner(ui, Status::Err, &ed.errors.join("\n"));
    }
    let (mut save, mut cancel) = (false, false);
    ui.horizontal(|ui| {
        save = ui.button(t("library.btn.save")).clicked();
        cancel = ui.button(t("library.btn.cancel")).clicked();
    });
    if cancel {
        st.editor = None;
    } else if save {
        st.submit_editor();
    }
}

fn show_standards(ui: &mut Ui) {
    ui.label(t("library.standards.intro"));
    let patterns = |ui: &mut Ui, id: &str, title: &str, list: &[standards::NamedPattern]| {
        ui.add_space(space::S);
        ui.label(RichText::new(title).strong());
        egui::Grid::new(id)
            .striped(true)
            .spacing([space::L, space::XS])
            .show(ui, |ui| {
                ui.label(t("library.f.name"));
                ui.label(t("library.standards.spacing"));
                ui.label(t("library.standards.bolt"));
                ui.end_row();
                for n in list {
                    ui.label(n.name);
                    ui.label(format!("{} x {} mm", n.pattern.x_mm, n.pattern.y_mm));
                    ui.label(format!("M{}", n.pattern.bolt_mm));
                    ui.end_row();
                }
            });
    };
    patterns(ui, "std_motor", t("library.standards.motor"), MOTOR_MOUNTS);
    patterns(
        ui,
        "std_stack",
        t("library.standards.stack"),
        STACK_PATTERNS,
    );
    ui.add_space(space::S);
    ui.label(RichText::new(t("library.standards.plates")).strong());
    let plates: Vec<String> = CARBON_PLATE_THICKNESSES_MM
        .iter()
        .map(|v| format!("{v} mm"))
        .collect();
    ui.label(plates.join(", "));
    ui.add_space(space::S);
    ui.label(RichText::new(t("library.standards.arms")).strong());
    for a in ARM_PROFILES {
        ui.label(format!(
            "{}  |  A = {:.1} mm\u{b2}  |  I = {:.0} mm\u{2074}",
            a.label(),
            a.area_mm2(),
            a.second_moment_mm4()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, with_lang};
    use crate::ui::theme::ThemeChoice;

    fn render(st: &mut LibraryUi, p: &mut Project, theme_choice: ThemeChoice) {
        let ctx = egui::Context::default();
        crate::ui::theme::install(&ctx, theme_choice);
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 800.0),
        ));
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| show(ui, p, st));
        });
        out.textures_delta.clear();
    }

    fn temp_state() -> (tempfile::TempDir, LibraryUi) {
        let dir = tempfile::tempdir().unwrap();
        let st = LibraryUi::with_path(dir.path().join(store::FILE_NAME));
        (dir, st)
    }

    #[test]
    fn panel_renders_in_both_themes_and_languages() {
        for lang in [Lang::Tr, Lang::En] {
            for choice in [ThemeChoice::Dark, ThemeChoice::Light] {
                with_lang(lang, || {
                    let (_d, mut st) = temp_state();
                    let mut p = Project::default();
                    for view in [
                        Some(Category::Motor),
                        Some(Category::Propeller),
                        Some(Category::Esc),
                        Some(Category::Battery),
                        None,
                    ] {
                        st.view = view;
                        render(&mut st, &mut p, choice);
                    }
                    st.view = Some(Category::Motor);
                    st.start_editor(Category::Motor, None);
                    st.pending_delete = Some((Category::Motor, "x".into()));
                    st.status = Some((Status::Warn, "x".into()));
                    render(&mut st, &mut p, choice);
                });
            }
        }
    }

    #[test]
    fn panel_does_not_touch_the_real_app_data_dir() {
        let (dir, mut st) = temp_state();
        let mut p = Project::default();
        render(&mut st, &mut p, ThemeChoice::Dark);
        assert!(
            !dir.path().join(store::FILE_NAME).exists(),
            "nothing to save yet"
        );
    }

    #[test]
    fn library_strings_exist_in_both_languages() {
        let keys: Vec<&str> = crate::i18n::tr_table()
            .keys()
            .copied()
            .filter(|k| k.starts_with("library."))
            .collect();
        assert!(keys.len() > 40);
        for k in keys {
            assert!(crate::i18n::en_table().contains_key(k), "missing en: {k}");
        }
        for k in crate::i18n::en_table()
            .keys()
            .filter(|k| k.starts_with("library."))
        {
            assert!(crate::i18n::tr_table().contains_key(k), "missing tr: {k}");
        }
        for cat in Category::ALL {
            for f in blank_fields(cat) {
                assert!(crate::i18n::tr_table().contains_key(f.key), "{}", f.key);
            }
        }
    }

    #[test]
    fn add_edit_delete_persist_to_the_temp_file() {
        let (dir, mut st) = temp_state();
        st.ensure_loaded();
        st.start_editor(Category::Motor, None);
        // Unnamed record is rejected and stays open with a message.
        st.submit_editor();
        assert!(
            st.editor
                .as_ref()
                .unwrap()
                .errors
                .iter()
                .any(|e| e.contains("name"))
        );
        st.editor.as_mut().unwrap().fields[0].value = "My motor".into();
        st.editor.as_mut().unwrap().fields[1].value = "abc".into();
        st.submit_editor();
        assert!(
            st.editor
                .as_ref()
                .unwrap()
                .errors
                .iter()
                .any(|e| e.contains("not a number"))
        );
        st.editor.as_mut().unwrap().fields[1].value = "2200".into();
        st.editor.as_mut().unwrap().fields[8].value = "16x16 M3".into();
        st.submit_editor();
        assert!(st.editor.is_none());
        let path = dir.path().join(store::FILE_NAME);
        let saved = ComponentLibrary::load(&path).unwrap();
        assert_eq!(saved.motors.len(), 1);
        assert_eq!(saved.motors[0].kv, 2200.0);
        assert_eq!(
            saved.motors[0].mount,
            Some(MountPattern::new(16.0, 16.0, 3.0))
        );

        st.start_editor(Category::Motor, Some("My motor"));
        st.editor.as_mut().unwrap().fields[0].value = "Renamed".into();
        st.submit_editor();
        assert!(st.user.find::<Motor>("Renamed").is_some());
        assert_eq!(st.user.motors.len(), 1);

        st.copy_example(Category::Motor, "Example 2204 2300KV");
        assert_eq!(st.user.motors.len(), 2);
        assert_eq!(st.user.motors[1].source, Source::User);
        st.copy_example(Category::Motor, "Example 2204 2300KV");
        assert_eq!(st.user.motors.len(), 2, "no duplicate copy");

        st.delete(Category::Motor, "Renamed");
        assert_eq!(ComponentLibrary::load(&path).unwrap().motors.len(), 1);
    }

    #[test]
    fn rows_filter_and_search() {
        let (_d, mut st) = temp_state();
        st.ensure_loaded();
        assert_eq!(st.rows(Category::Esc).len(), 5);
        st.search = "40a".into();
        assert_eq!(st.rows(Category::Esc).len(), 1);
        st.search.clear();
        st.filter = SourceFilter::Mine;
        assert!(st.rows(Category::Esc).is_empty());
    }

    #[test]
    fn import_and_export_through_state() {
        let (dir, mut st) = temp_state();
        st.ensure_loaded();
        st.copy_example(Category::Battery, "Example LiPo 4S 5000");
        st.checked
            .insert((Category::Battery, "Example LiPo 4S 5000".into()));
        let shared = dir.path().join("shared.json");
        st.export_to(&shared);
        assert!(matches!(st.status, Some((Status::Ok, _))));

        let (_d2, mut other) = temp_state();
        other.ensure_loaded();
        other.import_from(&shared);
        assert_eq!(other.user.batteries.len(), 1);
        other.import_from(&shared);
        assert_eq!(other.user.batteries.len(), 1);
        other.import_from(&dir.path().join("nope.json"));
        assert!(matches!(other.status, Some((Status::Err, _))));
    }

    #[test]
    fn unreadable_library_file_is_kept_aside() {
        let (dir, mut st) = temp_state();
        let path = dir.path().join(store::FILE_NAME);
        std::fs::write(&path, "garbage").unwrap();
        st.ensure_loaded();
        assert!(matches!(st.status, Some((Status::Err, _))));
        assert!(path.with_extension("json.unreadable").exists());
    }

    #[test]
    fn applying_a_battery_fills_the_custom_pack() {
        let mut p = Project::default();
        let b = samples::examples().batteries[2].clone();
        apply_battery(&mut p, &b);
        assert_eq!(
            p.battery_source,
            crate::calc::custom_battery::BatterySourceKind::CustomPack
        );
        assert_eq!(p.custom_pack.s_count, 4);
        assert_eq!(p.custom_pack.capacity_mah, 5000.0);
        assert!((p.custom_pack.pack_voltage - 14.8).abs() < 1e-9);
        assert!((p.custom_pack.max_cont_a - 250.0).abs() < 1e-9);
        assert_eq!(p.custom_pack.pack_mass_g, 480.0);
    }
}
