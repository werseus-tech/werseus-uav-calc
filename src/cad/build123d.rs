// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fast CAD backend: build123d (Python on OpenCascade) writes STEP / STL / DXF for every part,
//! an assembled STEP and the same assembly checks, BOM and report as the FreeCAD path.
//!
//! Geometry is not forked: every part is described from the same `Layout` (placements,
//! slots, saddle), the same `*CadParams`, `SurfaceGeom` / `FuseProfile` and `structure`
//! values the FreeCAD scripts use. Only the construction differs: plain B-rep booleans instead
//! of a constrained PartDesign tree, all parts built in one Python process.
//!
//! Every value reaches Python through one JSON string literal (`py_str_literal`); no user text
//! is interpolated into code.

use super::build123d_py::PY_LIB;
use super::freecad::{PlugSpec, mr_arm_angles, py_str_literal, sanitize_ident};
use super::geom::{SurfaceGeom, dot, sub};
use super::layout::Layout;
use super::outputs::{
    ASSEMBLY_NAME, AbortReason, CONTACT_TOL_MM, INTERF_MIN_DEPTH_MM, OutputsProgress,
    OutputsSummary, PartId, PartResult, RunEvent, RunPhase, TubeCadParams, assembly_items,
    assembly_report_markdown, bom_csv, bom_markdown, build_bom, classify_pairs, design_fingerprint,
    floating_parts, mass_check, minutes_text, part_material, part_quantity, remove_new_or_changed,
    snapshot_files, unseated_mates,
};
use super::runner::{
    CadExecutionResult, CadLimits, CancelFlag, ProcessEnd, new_command, parse_cad_json_output,
    parse_stage_line, run_process_controlled,
};
use super::structure::INNER_SEGMENTS_PER_SIDE;
use crate::calc::Model;
use crate::project::Project;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// One-line install hint shown when build123d is missing.
pub const BUILD123D_INSTALL_HINT: &str = "pip install build123d";

/// Which CAD backend generates the outputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CadBackend {
    /// build123d: STEP / STL / DXF, fast.
    #[default]
    Fast,
    /// FreeCAD: editable parametric `.FCStd` trees (slow).
    FreeCad,
}

/// Output formats of the fast backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastFormats {
    pub step: bool,
    pub stl: bool,
    pub dxf: bool,
}

impl Default for FastFormats {
    fn default() -> Self {
        Self {
            step: true,
            stl: true,
            dxf: true,
        }
    }
}

impl FastFormats {
    pub fn any(&self) -> bool {
        self.step || self.stl || self.dxf
    }
}

// ---------------------------------------------------------------------------------------------
// Python detection
// ---------------------------------------------------------------------------------------------

/// A Python interpreter with build123d importable. `args` precede the script (e.g. `-3` for
/// the `py` launcher).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PythonInstallation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub python_version: String,
    pub build123d_version: String,
}

impl PythonInstallation {
    pub fn command(&self) -> std::process::Command {
        let mut c = new_command(&self.program);
        c.args(&self.args);
        c
    }

    pub fn display(&self) -> String {
        let mut s = self.program.display().to_string();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// Result of a Python search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PythonDetection {
    pub found: Option<PythonInstallation>,
    /// Interpreters that run but cannot import build123d ("path (version)").
    pub without_build123d: Vec<String>,
}

/// Result of probing one interpreter: Python version and, if importable, build123d version.
pub type ProbeResult = Option<(String, Option<String>)>;

/// Probe command: prints the Python version, then the build123d version (import may fail).
pub const PROBE_CODE: &str = "import sys; print(sys.version.split()[0], flush=True); import build123d; print(build123d.__version__)";

/// Parses the probe output (first line Python version, second line build123d version).
pub fn parse_probe_output(stdout: &str) -> ProbeResult {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let py = lines.next()?;
    if !py.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let b = lines
        .next()
        .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string);
    Some((py.to_string(), b))
}

/// Runs the probe in `program args`, bounded to 90 s (a cold build123d import can take ~10 s).
pub fn probe_python(program: &Path, args: &[String]) -> ProbeResult {
    let mut cmd = new_command(program);
    cmd.args(args).arg("-c").arg(PROBE_CODE);
    match run_process_controlled(cmd, Duration::from_secs(90), &CancelFlag::new(), |_| {}) {
        ProcessEnd::Finished { stdout, .. } => parse_probe_output(&stdout),
        _ => None,
    }
}

/// Interpreter paths reported by `py -0p` (Windows launcher).
pub fn parse_py_launcher_list(stdout: &str) -> Vec<PathBuf> {
    stdout
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let i = l.to_ascii_lowercase().find(":\\").or_else(|| l.find('/'))?;
            let start = if l[..i].ends_with(|c: char| c.is_ascii_alphabetic()) && i > 0 {
                i - 1
            } else {
                i
            };
            let p = l[start..].trim();
            p.to_ascii_lowercase()
                .ends_with(".exe")
                .then(|| PathBuf::from(p))
        })
        .collect()
}

/// Candidate interpreters, in order: configured path, PATH (python, python3), `py` launcher
/// list. Duplicates are removed.
pub fn python_candidates(
    configured: Option<&str>,
    py_list: impl FnOnce() -> Vec<PathBuf>,
) -> Vec<(PathBuf, Vec<String>)> {
    let mut out: Vec<(PathBuf, Vec<String>)> = Vec::new();
    let mut push = |p: PathBuf, a: Vec<String>| {
        let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if !out
            .iter()
            .any(|(q, b)| std::fs::canonicalize(q).unwrap_or_else(|_| q.clone()) == key && *b == a)
        {
            out.push((p, a));
        }
    };
    if let Some(c) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        push(PathBuf::from(c), Vec::new());
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            // The Microsoft Store alias only opens the Store; skip it.
            if dir.to_string_lossy().contains("WindowsApps") {
                continue;
            }
            for name in ["python.exe", "python3.exe", "python", "python3"] {
                let c = dir.join(name);
                if c.is_file() {
                    push(c, Vec::new());
                }
            }
        }
    }
    for p in py_list() {
        if p.is_file() {
            push(p, Vec::new());
        }
    }
    out
}

/// Finds a Python with build123d importable (`probe` is injected for tests).
pub fn detect_python_with(
    configured: Option<&str>,
    py_list: impl FnOnce() -> Vec<PathBuf>,
    mut probe: impl FnMut(&Path, &[String]) -> ProbeResult,
) -> PythonDetection {
    let mut det = PythonDetection::default();
    for (prog, args) in python_candidates(configured, py_list) {
        match probe(&prog, &args) {
            Some((py, Some(b))) => {
                det.found = Some(PythonInstallation {
                    program: prog,
                    args,
                    python_version: py,
                    build123d_version: b,
                });
                return det;
            }
            Some((py, None)) => det
                .without_build123d
                .push(format!("{} ({py})", prog.display())),
            None => {}
        }
    }
    det
}

/// `py -0p` output (empty when the launcher is missing).
fn py_launcher_list() -> Vec<PathBuf> {
    let mut cmd = new_command("py");
    cmd.arg("-0p");
    match run_process_controlled(cmd, Duration::from_secs(20), &CancelFlag::new(), |_| {}) {
        ProcessEnd::Finished { stdout, .. } => parse_py_launcher_list(&stdout),
        _ => Vec::new(),
    }
}

/// Real detection (spawns interpreters; call off the UI thread).
pub fn detect_python(configured: Option<&str>) -> PythonDetection {
    detect_python_with(configured, py_launcher_list, probe_python)
}

/// Runs `detect_python` in a background thread; repaints `ctx` when done.
pub fn detect_python_async(
    configured: Option<String>,
    ctx: Option<egui::Context>,
) -> std::sync::mpsc::Receiver<PythonDetection> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(detect_python(configured.as_deref()));
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
    rx
}

// ---------------------------------------------------------------------------------------------
// Part specifications (data for the Python builders)
// ---------------------------------------------------------------------------------------------

/// Section of a lifting surface at span `z`: `[chord, base_x, base_y, yaw_rad]`. Same
/// placement as the FreeCAD sketches (`surface_script`): with `attitude` the tip rises with
/// dihedral and twists about the quarter chord, intermediate sections are the linear blend
/// of root and tip (chord |a + b e^(-i twist)|).
pub fn section_place(g: &SurfaceGeom, z: f64, attitude: Option<(f64, f64)>) -> [f64; 4] {
    let tan_sw = g.sweep_le_deg.to_radians().tan();
    match attitude {
        None => [
            g.c_root + (g.c_tip - g.c_root) * z / g.span.max(1e-9),
            z * tan_sw,
            0.0,
            0.0,
        ],
        Some((dihedral, twist)) => {
            let sf = z / g.span.max(1e-9);
            let a = (1.0 - sf) * g.c_root;
            let b = sf * g.c_tip;
            let th = twist.to_radians();
            let chord = (a * a + b * b + 2.0 * a * b * th.cos()).max(0.0).sqrt();
            [
                chord,
                z * tan_sw + sf * 0.25 * g.c_tip * (1.0 - th.cos()),
                z * dihedral.to_radians().tan() + sf * 0.25 * g.c_tip * th.sin(),
                -(b * th.sin()).atan2(a + b * th.cos()),
            ]
        }
    }
}

/// Local plane `{o, x, z}` for a DXF section.
fn plane(o: [f64; 3], x: [f64; 3], z: [f64; 3]) -> Value {
    json!({"o": o, "x": x, "z": z})
}

/// Lifting-surface spec: outer ruled loft, one subtractive loft per bay (ribs remain between),
/// spar channel; DXF sections at the root rib, every rib and the tip cap.
fn surface_spec(
    g: &SurfaceGeom,
    st: &super::structure::SurfaceStructure,
    attitude: Option<(f64, f64)>,
) -> Value {
    let pl = |z: f64| {
        let s = section_place(g, z, attitude);
        (s[0], [s[1], s[2], s[3], z])
    };
    let (_, root_pl) = pl(0.0);
    let (_, tip_pl) = pl(g.span);
    let c_min = g.chord_at(g.span - st.wall_mm).min(g.c_root).max(1e-6);
    let terms = if st.is_hollow() {
        g.profile
            .inner_terms(st.wall_mm / c_min, INNER_SEGMENTS_PER_SIDE)
    } else {
        None
    };
    let mut bays = Vec::new();
    let mut knots: Vec<f64> = Vec::new();
    let mut mults: Vec<u32> = Vec::new();
    if let Some(terms) = &terms {
        let n_in = terms.len() + 1;
        knots = (0..n_in.saturating_sub(2)).map(|i| i as f64).collect();
        mults = vec![1; knots.len()];
        if let Some(f) = mults.first_mut() {
            *f = 4;
        }
        if let Some(l) = mults.last_mut() {
            *l = 4;
        }
        let section = |z: f64| {
            let (c, place) = pl(z);
            let mut poles: Vec<[f64; 2]> = terms.iter().map(|t| t.eval(c, st.wall_mm)).collect();
            poles.push(poles[0]);
            json!({"poles": poles, "pl": place})
        };
        for (za, zb) in st.bays_mm() {
            bays.push(json!({"a": section(za), "b": section(zb)}));
        }
    }
    let spar = st.spar.map(|a| {
        let s0 = dot(sub(a.start, a.hole_start), a.dir);
        json!({"start": a.hole_start, "dir": a.dir, "len": s0 + a.length + 1.5, "r": a.od / 2.0})
    });
    let mut dxf_z = Vec::new();
    if bays.is_empty() {
        dxf_z.push(g.span * 0.002);
    } else {
        dxf_z.push(st.root_rib_mm / 2.0);
        dxf_z.extend(st.rib_stations_mm());
        dxf_z.push(g.span - st.wall_mm / 2.0);
    }
    let dxf: Vec<Value> = dxf_z
        .into_iter()
        .map(|z| plane([0.0, 0.0, z], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]))
        .collect();
    json!({
        "kind": "surface",
        "pts": g.profile.pts,
        "root": [g.c_root, root_pl[0], root_pl[1], root_pl[2], root_pl[3]],
        "tip": [g.c_tip, tip_pl[0], tip_pl[1], tip_pl[2], tip_pl[3]],
        "bays": bays,
        "knots": knots,
        "mults": mults,
        "spar": spar,
        "dxf": dxf,
    })
}

fn finite_plugs(plugs: &[PlugSpec]) -> Vec<&PlugSpec> {
    plugs.iter().filter(|p| p.is_finite()).collect()
}

fn fuselage_spec(f: &super::freecad::FuselageCadParams) -> Value {
    let prof = f.profile();
    let st = f.structure();
    let l = f.length_mm;
    let r_max = f.max_diameter_mm / 2.0;
    let mut outer = vec![[0.0, 0.0]];
    outer.extend(prof.pts.iter().copied());
    outer.push([l, 0.0]);
    let inner = (st.wall_mm > 0.0).then(|| {
        let mut v = vec![[st.wall_mm, 0.0]];
        v.extend(prof.inner(st.wall_mm));
        v.push([l - st.wall_mm, 0.0]);
        v
    });
    let h = st.former_t_mm / 2.0;
    let formers: Vec<[[f64; 2]; 4]> = st
        .former_stations_mm()
        .into_iter()
        .map(|x| {
            let r_hole = prof.r_at(x) - st.wall_mm - st.former_depth_mm;
            let big = 2.0 * r_max;
            [[x - h, r_hole], [x + h, r_hole], [x + h, big], [x - h, big]]
        })
        .collect();
    let dxf: Vec<Value> = st
        .former_stations_mm()
        .into_iter()
        .map(|x| plane([x, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]))
        .collect();
    json!({
        "kind": "fuselage",
        "outer": outer,
        "inner": inner,
        "wall": st.wall_mm,
        "length": l,
        "r_max": r_max,
        "pad": f.pad,
        "formers": formers,
        "plugs": finite_plugs(&f.plugs),
        "dxf": dxf,
    })
}

fn mid_plane(t: f64) -> Vec<Value> {
    vec![plane([0.0, 0.0, t / 2.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])]
}

fn motor_mount_spec(m: &super::freecad::MotorMountCadParams) -> Value {
    json!({
        "kind": "motor_mount",
        "size": m.plate_size_mm,
        "t": m.thickness_mm,
        "pattern": m.motor_bolt_pattern_mm,
        "bolt": m.motor_bolt_dia_mm,
        "shaft": m.shaft_hole_dia_mm,
        "fw_spacing": m.firewall_bolt_spacing_mm,
        "fw_bolt": m.firewall_bolt_dia_mm,
        "dxf": mid_plane(m.thickness_mm),
    })
}

fn tube_spec(t: &TubeCadParams) -> Value {
    json!({"kind": "tube", "od": t.od_mm, "id": t.id_mm.max(0.0), "length": t.length_mm})
}

/// Spar tube parameters (same fallback as the FreeCAD path when no spar fits).
fn spar_tube(p: &Project, lay: &Layout) -> TubeCadParams {
    lay.spar.clone().unwrap_or_else(|| TubeCadParams {
        name: format!("{}_Kanat_Kirisi", sanitize_ident(&p.name)),
        od_mm: lay.wing.spar_od_mm,
        id_mm: lay.wing.spar_id_mm,
        length_mm: 10.0,
    })
}

/// Part name (file stem source): identical to the FreeCAD path's part names.
pub fn part_name(id: PartId, p: &Project, lay: &Layout) -> String {
    match id {
        PartId::Wing => lay.wing.name.clone(),
        PartId::WingSpar => spar_tube(p, lay).name,
        PartId::TailHorizontal => lay.tail_h.name.clone(),
        PartId::TailVertical => lay.tail_v.name.clone(),
        PartId::TailVTail => lay.vtail.name.clone(),
        PartId::Winglet => lay.winglet.name.clone(),
        PartId::Fuselage => lay.fuselage.name.clone(),
        PartId::MotorMount => lay.motor_mount.name.clone(),
        PartId::LiftMotorMount => lay.lift_mount.name.clone(),
        PartId::MrMotorMount => lay.mr_mount.name.clone(),
        PartId::BoomWingClamp => lay.clamp.name.clone(),
        PartId::VtolBoom => lay.boom.name.clone(),
        PartId::MrArm => lay.mr_arm.name.clone(),
        PartId::MrPlate => lay.mr_plate.name.clone(),
        PartId::LandingGear => lay.leg.name.clone(),
    }
}

/// Local mirror plane normal of the right-hand copy (same as the FreeCAD mirror objects).
pub fn mirror_axis(id: PartId) -> usize {
    if id == PartId::BoomWingClamp { 1 } else { 2 }
}

/// Builder spec of one part (all lengths mm, part-local frame as in the FreeCAD scripts).
pub fn part_spec(id: PartId, p: &Project, lay: &Layout) -> Value {
    match id {
        PartId::Wing => surface_spec(
            &lay.wing.geom(),
            &lay.wing.structure(),
            Some((lay.wing.dihedral_deg, lay.wing.twist_deg)),
        ),
        PartId::TailHorizontal => surface_spec(&lay.tail_h.geom(), &lay.tail_h.structure(), None),
        PartId::TailVertical => surface_spec(&lay.tail_v.geom(), &lay.tail_v.structure(), None),
        PartId::TailVTail => surface_spec(&lay.vtail.geom(), &lay.vtail.structure(), None),
        PartId::Winglet => surface_spec(&lay.winglet.geom(), &lay.winglet.structure(), None),
        PartId::Fuselage => fuselage_spec(&lay.fuselage),
        PartId::WingSpar => tube_spec(&spar_tube(p, lay)),
        PartId::VtolBoom => tube_spec(&lay.boom),
        PartId::MotorMount => motor_mount_spec(&lay.motor_mount),
        PartId::LiftMotorMount => motor_mount_spec(&lay.lift_mount),
        PartId::MrMotorMount => motor_mount_spec(&lay.mr_mount),
        PartId::BoomWingClamp => {
            let c = &lay.clamp;
            json!({
                "kind": "clamp",
                "bore": c.bore_mm,
                "wall": c.wall_mm,
                "width": c.width_mm,
                "pillar_top": c.pillar_top_mm,
                "slit": c.slit_mm,
                "saddle": c.saddle.as_ref().filter(|s| s.is_finite()),
                // Ring cross-section (bore axis is X).
                "dxf": [plane([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0])],
            })
        }
        PartId::MrArm => {
            let a = &lay.mr_arm;
            json!({
                "kind": "arm",
                "length": a.arm_length_mm,
                "width": a.width_mm,
                "t": a.thickness_mm,
                "pattern": a.motor_mount_pattern_mm,
                "bolt": a.motor_bolt_dia_mm,
                "shaft": a.motor_shaft_dia_mm,
                "ch_bolt": a.chassis_bolt_dia_mm,
                "ch_x0": a.chassis_hole_x0_mm,
                "ch_spacing": a.chassis_bolt_spacing_mm,
                "dxf": mid_plane(a.thickness_mm),
            })
        }
        PartId::MrPlate => {
            let m = &lay.mr_plate;
            json!({
                "kind": "plate",
                "r": m.plate_radius_mm,
                "t": m.thickness_mm,
                "center": m.center_hole_dia_mm,
                "fc": m.fc_mount_pattern_mm,
                "ch_bolt": m.chassis_bolt_dia_mm,
                "angles": mr_arm_angles(m.layout, m.arm_count),
                "arm_r": m.arm_hole_r_mm,
                "dxf": mid_plane(m.thickness_mm),
            })
        }
        PartId::LandingGear => {
            let l = &lay.leg;
            json!({
                "kind": "leg",
                "dia": l.leg_dia_mm,
                "len": l.leg_len_mm,
                "splay": l.splay_deg,
                "tab_l": l.tab_l_mm,
                "tab_w": l.tab_w_mm,
                "tab_t": l.tab_t_mm,
                "foot_l": l.foot_len_mm,
                "foot_w": l.foot_w_mm,
                "foot_t": l.foot_t_mm,
                "dxf": [],
            })
        }
    }
}

/// One part of a fast job.
#[derive(Clone, Debug)]
pub struct FastPartPlan {
    pub id: PartId,
    pub stem: String,
    pub qty: u32,
    pub spec: Value,
}

pub fn plan_fast_part(id: PartId, p: &Project, lay: &Layout) -> FastPartPlan {
    FastPartPlan {
        id,
        stem: sanitize_ident(&part_name(id, p, lay)),
        qty: part_quantity(id, p),
        spec: part_spec(id, p, lay),
    }
}

/// Assembly instances of the given parts (placements from `Layout`, like the FreeCAD
/// assembly; mirrored copies mirror the left part about its local mirror plane).
pub fn fast_assembly_items(p: &Project, lay: &Layout, plans: &[FastPartPlan]) -> Vec<Value> {
    lay.instances
        .iter()
        .filter_map(|i| {
            let plan = plans.iter().find(|q| q.id == i.id)?;
            let m = i.frame.matrix16();
            let rho = part_material(i.id, p).1;
            (m.iter().all(|v| v.is_finite()) && rho.is_finite()).then(|| {
                json!({
                    "label": sanitize_ident(&i.label),
                    "stem": plan.stem,
                    "mirror_axis": i.mirror.then(|| mirror_axis(i.id)),
                    "m": m,
                    "density": rho,
                })
            })
        })
        .collect()
}

/// Job description (pure data) for one Python run.
pub struct FastJob<'a> {
    pub out_dir: &'a Path,
    pub scratch_dir: &'a Path,
    pub formats: FastFormats,
    pub parts: &'a [FastPartPlan],
    /// `None`: no assembly in this run.
    pub assembly_items: Option<Vec<Value>>,
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Job JSON (the only data channel into the script).
pub fn job_json(job: &FastJob) -> Value {
    let parts: Vec<Value> = job
        .parts
        .iter()
        .map(|q| json!({"key": q.id.key(), "stem": q.stem, "spec": q.spec}))
        .collect();
    let assembly = job.assembly_items.as_ref().map(|items| {
        json!({
            "name": ASSEMBLY_NAME,
            "step": job.formats.step,
            "items": items,
            "tol": CONTACT_TOL_MM,
            "depth_min": INTERF_MIN_DEPTH_MM,
        })
    });
    json!({
        "out_dir": path_str(job.out_dir),
        "scratch_dir": path_str(job.scratch_dir),
        "formats": job.formats,
        "parts": parts,
        "assembly": assembly,
    })
}

/// Full build123d script for a job.
pub fn generate_fast_script(job: &FastJob) -> String {
    let job_py = py_str_literal(&job_json(job).to_string());
    format!(
        "# -*- coding: utf-8 -*-\n# build123d CAD generator (fast backend) - WerSeuS UAV Calc\n\
         # Data only: every value comes from the JSON literal below.\nimport json\n\
         JOB = json.loads({job_py})\n{PY_LIB}\nmain(JOB)\n"
    )
}

// ---------------------------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------------------------

/// Per-part line printed by the script.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct FastPartLine {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub stem: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub solids: usize,
    #[serde(default)]
    pub volume_mm3: f64,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub secs: f64,
}

pub const PART_TAG: &str = "__WERSEUS_FAST_PART__:";

pub fn parse_part_line(line: &str) -> Option<FastPartLine> {
    serde_json::from_str(line.trim().strip_prefix(PART_TAG)?).ok()
}

/// Files a part may write (only these are removed after an interrupted run).
fn expected_fast_files(out_dir: &Path, stem: &str) -> Vec<PathBuf> {
    ["step", "stl", "dxf"]
        .iter()
        .map(|e| out_dir.join(format!("{stem}.{e}")))
        .collect()
}

/// Which watchdog stage is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Watch {
    Import,
    Part(usize),
    Assembly,
}

struct WatchState {
    stage: Watch,
    since: Instant,
}

/// How one Python run ended.
enum RunOutcome {
    Finished(Option<Box<CadExecutionResult>>, String),
    UserCancelled,
    TotalTimeout,
    /// A part exceeded the per-part limit (index into the run's part list).
    PartTimeout(usize),
    AssemblyTimeout,
    Failed(String),
}

/// Runs one Python process with the per-part / assembly watchdog.
#[allow(clippy::too_many_arguments)]
fn run_once(
    py: &PythonInstallation,
    script: &Path,
    total_left: Duration,
    limits: &CadLimits,
    cancel: &CancelFlag,
    mut on_part_stage: impl FnMut(usize),
    mut on_part: impl FnMut(FastPartLine),
    mut on_asm_stage: impl FnMut(super::runner::StageInfo),
) -> RunOutcome {
    let inner = CancelFlag::new();
    let timed_out = Arc::new(std::sync::Mutex::new(None::<Watch>));
    let state = Arc::new(std::sync::Mutex::new(WatchState {
        stage: Watch::Import,
        since: Instant::now(),
    }));
    let done = Arc::new(AtomicBool::new(false));
    let part_limit = Duration::from_secs(limits.part_secs);
    let asm_limit = Duration::from_secs(limits.assembly_secs);
    let watchdog = {
        let (inner, timed_out, state, done, user) = (
            inner.clone(),
            timed_out.clone(),
            state.clone(),
            done.clone(),
            cancel.clone(),
        );
        std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                if user.is_cancelled() {
                    inner.cancel();
                    return;
                }
                if let Ok(s) = state.lock() {
                    // The interpreter start-up counts against the first part's limit.
                    let limit = if s.stage == Watch::Assembly {
                        asm_limit
                    } else {
                        part_limit
                    };
                    if s.since.elapsed() > limit {
                        if let Ok(mut t) = timed_out.lock() {
                            *t = Some(s.stage);
                        }
                        inner.cancel();
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        })
    };
    let mut cmd = py.command();
    cmd.arg("-u").arg(script).env("PYTHONIOENCODING", "utf-8");
    if let Some(dir) = script.parent() {
        cmd.current_dir(dir);
    }
    let end = run_process_controlled(cmd, total_left, &inner, |line| {
        if let Some(pl) = parse_part_line(line) {
            on_part(pl);
        } else if let Some(st) = parse_stage_line(line) {
            let next = match st.id.as_str() {
                "import" => None,
                "part" => st.done.map(|d| Watch::Part(d.saturating_sub(1) as usize)),
                _ => Some(Watch::Assembly),
            };
            if let Some(n) = next {
                if let Ok(mut s) = state.lock() {
                    if s.stage != n {
                        // Interpreter start-up is charged to the first part.
                        let keep = s.stage == Watch::Import && n == Watch::Part(0);
                        s.stage = n;
                        if !keep {
                            s.since = Instant::now();
                        }
                    }
                }
                match n {
                    Watch::Part(i) => on_part_stage(i),
                    _ => on_asm_stage(st),
                }
            }
        }
    });
    done.store(true, Ordering::SeqCst);
    let _ = watchdog.join();
    match end {
        ProcessEnd::Finished { stdout, stderr, .. } => {
            RunOutcome::Finished(parse_cad_json_output(&stdout).map(Box::new), stderr)
        }
        ProcessEnd::TimedOut => RunOutcome::TotalTimeout,
        ProcessEnd::SpawnFailed(e) => RunOutcome::Failed(e),
        ProcessEnd::Cancelled => {
            if cancel.is_cancelled() {
                return RunOutcome::UserCancelled;
            }
            match timed_out.lock().ok().and_then(|t| *t) {
                Some(Watch::Assembly) => RunOutcome::AssemblyTimeout,
                Some(Watch::Part(i)) => RunOutcome::PartTimeout(i),
                Some(Watch::Import) => RunOutcome::PartTimeout(0),
                None => RunOutcome::UserCancelled,
            }
        }
    }
}

/// Last meaningful stderr line (Python traceback tail) for error messages.
fn stderr_tail(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Generates the selected parts and the assembly with build123d in the background. Same
/// events, limits (per part / assembly / total), cancel and partial-file cleanup as
/// `outputs::run_outputs_controlled`. All parts run in one Python process; when a part
/// exceeds its limit the process is killed, that part is reported as failed and a new process
/// continues with the remaining parts.
#[allow(clippy::too_many_arguments)]
pub fn run_fast_outputs_controlled(
    py: PythonInstallation,
    p: Project,
    m: Model,
    ids: Vec<PartId>,
    out_dir: PathBuf,
    formats: FastFormats,
    limits: CadLimits,
    cancel: CancelFlag,
) -> std::sync::mpsc::Receiver<RunEvent> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let send = |e: RunEvent| {
            let _ = tx.send(e);
        };
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            send(RunEvent::Error(format!("{}: {e}", out_dir.display())));
            return;
        }
        let script_dir = match super::runner::get_ascii_temp_cad_dir() {
            Ok(d) => {
                static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let k = RUN.fetch_add(1, Ordering::Relaxed);
                d.join(format!("fast_{}_{k}", std::process::id()))
            }
            Err(e) => {
                send(RunEvent::Error(e));
                return;
            }
        };
        let _ = std::fs::create_dir_all(&script_dir);
        let abort = |reason: AbortReason, completed: usize, removed: usize| {
            let _ = std::fs::remove_dir_all(&script_dir);
            let _ = tx.send(RunEvent::Aborted {
                reason,
                completed,
                removed,
            });
        };
        let started = Instant::now();
        let total_limit = Duration::from_secs(limits.total_secs);
        let lay = Layout::new(&p, &m);
        let plans: Vec<FastPartPlan> = ids.iter().map(|&id| plan_fast_part(id, &p, &lay)).collect();
        let n_parts = plans.len();
        let mut sum = OutputsSummary {
            out_dir: out_dir.clone(),
            parts: Vec::new(),
            bom: Vec::new(),
            assembly_fcstd: String::new(),
            assembly_step: String::new(),
            mass_kg: 0.0,
            cg_mm: Vec::new(),
            interferences: Vec::new(),
            contacts: Vec::new(),
            check_failed: Vec::new(),
            floating: Vec::new(),
            unseated: Vec::new(),
            mass: mass_check(&p, &m, &[]),
            fingerprint: design_fingerprint(&p),
            failed: Vec::new(),
            files: Vec::new(),
        };
        let all_items = fast_assembly_items(&p, &lay, &plans);
        let mut next = 0usize;
        let mut asm_result: Option<Box<CadExecutionResult>> = None;
        let mut asm_done = n_parts == 0;
        while next < n_parts || !asm_done {
            if cancel.is_cancelled() {
                abort(AbortReason::Cancelled, sum.parts.len(), 0);
                return;
            }
            let Some(left) = total_limit
                .checked_sub(started.elapsed())
                .filter(|d| !d.is_zero())
            else {
                abort(AbortReason::TotalTimeout, sum.parts.len(), 0);
                return;
            };
            let batch = &plans[next..];
            let job = FastJob {
                out_dir: &out_dir,
                scratch_dir: &script_dir,
                formats,
                parts: batch,
                assembly_items: Some(all_items.clone()),
            };
            let script_path = script_dir.join(format!("fast_job_{next}.py"));
            if let Err(e) = std::fs::write(&script_path, generate_fast_script(&job)) {
                send(RunEvent::Error(format!("{}: {e}", script_path.display())));
                let _ = std::fs::remove_dir_all(&script_dir);
                return;
            }
            let mut snap_paths: Vec<PathBuf> = batch
                .iter()
                .flat_map(|q| expected_fast_files(&out_dir, &q.stem))
                .collect();
            snap_paths.extend(expected_fast_files(&out_dir, ASSEMBLY_NAME));
            let snap = snapshot_files(snap_paths);
            send(RunEvent::Log(format!("build123d: {}", py.display())));
            let mut finished_in_run = 0usize;
            let outcome = run_once(
                &py,
                &script_path,
                left,
                &limits,
                &cancel,
                |i| {
                    if let Some(q) = batch.get(i) {
                        send(RunEvent::Log(format!("{} ...", q.id.key())));
                        send(RunEvent::Progress(OutputsProgress {
                            phase: RunPhase::Parts,
                            current: next + i + 1,
                            total: n_parts,
                            part: q.id.title().to_string(),
                            stage: None,
                        }));
                    }
                },
                |pl| {
                    finished_in_run += 1;
                    let Some(q) = batch.iter().find(|q| q.stem == pl.stem) else {
                        return;
                    };
                    if pl.ok {
                        sum.files.extend(pl.files.iter().cloned());
                        send(RunEvent::Log(format!(
                            "{}: {:.1} s, {:.0} mm3",
                            q.id.key(),
                            pl.secs,
                            pl.volume_mm3
                        )));
                        sum.parts.push(PartResult {
                            id: q.id,
                            stem: q.stem.clone(),
                            main_object: q.stem.clone(),
                            qty: q.qty,
                            volume_mm3: pl.volume_mm3,
                            fcstd_file: String::new(),
                        });
                    } else {
                        let e = pl.error.clone().unwrap_or_default();
                        let first = e.lines().next().unwrap_or("").to_string();
                        sum.failed.push(format!("{}: {first}", q.id.key()));
                    }
                },
                |st| {
                    send(RunEvent::Progress(OutputsProgress {
                        phase: RunPhase::Assembly,
                        current: n_parts,
                        total: n_parts,
                        part: String::new(),
                        stage: Some(st),
                    }));
                },
            );
            match outcome {
                RunOutcome::Finished(res, stderr) => {
                    // Parts that printed no result (crash) are failures.
                    for q in &batch[finished_in_run.min(batch.len())..] {
                        sum.failed
                            .push(format!("{}: {}", q.id.key(), stderr_tail(&stderr)));
                    }
                    next = n_parts;
                    asm_done = true;
                    asm_result = res;
                    if asm_result.is_none() {
                        sum.failed
                            .push(format!("assembly: {}", stderr_tail(&stderr)));
                    }
                }
                RunOutcome::UserCancelled => {
                    let removed = remove_new_or_changed(&snap);
                    abort(AbortReason::Cancelled, sum.parts.len(), removed);
                    return;
                }
                RunOutcome::TotalTimeout => {
                    let removed = remove_new_or_changed(&snap);
                    abort(AbortReason::TotalTimeout, sum.parts.len(), removed);
                    return;
                }
                RunOutcome::PartTimeout(i) => {
                    let i = i.min(batch.len().saturating_sub(1));
                    if let Some(q) = batch.get(i) {
                        // Only the timed-out part's files can be partial.
                        let part_snap: Vec<_> = snap
                            .iter()
                            .filter(|(path, _)| {
                                expected_fast_files(&out_dir, &q.stem).contains(path)
                            })
                            .cloned()
                            .collect();
                        remove_new_or_changed(&part_snap);
                        sum.failed.push(format!(
                            "{}: {}",
                            q.id.key(),
                            crate::i18n::tf(
                                "out.timeout_part_fmt",
                                &[minutes_text(Duration::from_secs(limits.part_secs))]
                            )
                        ));
                    }
                    next += i + 1;
                }
                RunOutcome::AssemblyTimeout => {
                    let asm_snap: Vec<_> = snap
                        .iter()
                        .filter(|(path, _)| {
                            expected_fast_files(&out_dir, ASSEMBLY_NAME).contains(path)
                        })
                        .cloned()
                        .collect();
                    remove_new_or_changed(&asm_snap);
                    sum.failed.push(format!(
                        "assembly: {}",
                        crate::i18n::tf(
                            "out.timeout_asm_fmt",
                            &[minutes_text(Duration::from_secs(limits.assembly_secs))]
                        )
                    ));
                    next = n_parts;
                    asm_done = true;
                }
                RunOutcome::Failed(e) => {
                    send(RunEvent::Error(format!("{}: {e}", py.display())));
                    let _ = std::fs::remove_dir_all(&script_dir);
                    return;
                }
            }
        }
        // Keep the requested order in the summary/BOM.
        sum.parts
            .sort_by_key(|r| ids.iter().position(|&i| i == r.id).unwrap_or(usize::MAX));
        sum.bom = build_bom(&p, &sum.parts);
        sum.mass = mass_check(&p, &m, &sum.parts);
        let bom_title = format!("{} - BOM", p.name);
        let md = out_dir.join("BOM.md");
        let csv = out_dir.join("BOM.csv");
        if std::fs::write(&md, bom_markdown(&bom_title, &sum.bom)).is_ok() {
            sum.files.push(md.to_string_lossy().into_owned());
        }
        if std::fs::write(&csv, bom_csv(&sum.bom)).is_ok() {
            sum.files.push(csv.to_string_lossy().into_owned());
        }
        if let Some(r) = asm_result.filter(|_| !sum.parts.is_empty()) {
            if r.ok == Some(false) || r.status == "error" {
                let e = r.error.clone().unwrap_or_default();
                sum.failed.push(format!(
                    "assembly: {}",
                    e.lines().next().unwrap_or("")
                ));
            } else {
                let items = assembly_items(&p, &m, &sum.parts);
                let mut classes = classify_pairs(&r.pairs);
                classes.unseated = unseated_mates(&items, &classes);
                let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
                sum.floating = floating_parts(&labels, &classes);
                sum.assembly_step = r.step_files.first().cloned().unwrap_or_default();
                sum.files.extend(r.step_files.iter().cloned());
                sum.mass_kg = r.mass_kg;
                sum.cg_mm = r.cg_mm.clone();
                sum.interferences = classes.interferences;
                sum.contacts = classes.contacts;
                sum.check_failed = classes.failed;
                sum.unseated = classes.unseated;
                let rep = assembly_report_markdown(&format!("{} - Montaj", p.name), &sum);
                let rp = out_dir.join("Montaj_Raporu.md");
                if std::fs::write(&rp, rep).is_ok() {
                    sum.files.push(rp.to_string_lossy().into_owned());
                }
            }
        }
        let _ = std::fs::remove_dir_all(&script_dir);
        send(RunEvent::Done(Box::new(sum)));
    });
    rx
}
