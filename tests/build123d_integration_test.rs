// SPDX-License-Identifier: AGPL-3.0-or-later
//! build123d (fast CAD backend) integration tests. They need a Python with build123d: set
//! `WERSEUS_PYTHON` to its interpreter or have one on PATH / the `py` launcher. Run by hand:
//! `cargo test --test build123d_integration_test -- --ignored`.

use std::fs;
use std::path::{Path, PathBuf};
use werseus_uav_calc::cad::build123d::{
    FastFormats, PythonInstallation, detect_python, plan_fast_part, run_fast_outputs_controlled,
};
use werseus_uav_calc::cad::freecad::profile_or_default;
use werseus_uav_calc::cad::layout::Layout;
use werseus_uav_calc::cad::outputs::{OutputsSummary, PartId, RunEvent, available_parts};
use werseus_uav_calc::cad::{CadLimits, CancelFlag};
use werseus_uav_calc::calc::{Library, evaluate};
use werseus_uav_calc::project::{Project, load_example};

fn python() -> Option<PythonInstallation> {
    let configured = std::env::var("WERSEUS_PYTHON").ok();
    let d = detect_python(configured.as_deref());
    if d.found.is_none() {
        println!(
            "build123d not found ({:?}); test skipped.",
            d.without_build123d
        );
    }
    d.found
}

fn run_blocking(
    py: PythonInstallation,
    p: Project,
    ids: Vec<PartId>,
    out: PathBuf,
) -> Result<OutputsSummary, String> {
    let m = evaluate(&p, &Library::embedded());
    let limits = CadLimits {
        part_secs: 3600,
        assembly_secs: 3600,
        total_secs: 4 * 3600,
    };
    let rx = run_fast_outputs_controlled(
        py,
        p,
        m,
        ids,
        out,
        FastFormats::default(),
        limits,
        CancelFlag::new(),
    );
    while let Ok(ev) = rx.recv() {
        match ev {
            RunEvent::Done(s) => return Ok(*s),
            RunEvent::Error(e) => return Err(e),
            RunEvent::Aborted { .. } => return Err("aborted".into()),
            RunEvent::Log(l) => println!("  log: {l}"),
            RunEvent::Progress(_) => {}
        }
    }
    Err("channel closed".into())
}

/// Shoelace area of a closed polygon.
fn poly_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    (a / 2.0).abs()
}

/// Points on the clamped cubic B-spline with control points `poles` and uniform interior
/// knots (the inner skin section; same knots as the generated script).
fn bspline_points(poles: &[[f64; 2]], per_span: usize) -> Vec<[f64; 2]> {
    let n = poles.len();
    let spans = n - 3;
    let mut knots = vec![0.0; 4];
    knots.extend((1..spans).map(|i| i as f64));
    knots.extend(vec![spans as f64; 4]);
    let mut out = Vec::new();
    for s in 0..spans {
        for k in 0..per_span {
            let u = s as f64 + k as f64 / per_span as f64;
            let j = s + 3; // knot span index
            let mut d: Vec<[f64; 2]> = (0..4).map(|i| poles[j - 3 + i]).collect();
            for r in 1..=3 {
                for i in (r..=3).rev() {
                    let ki = j - 3 + i;
                    let den = knots[ki + 4 - r] - knots[ki];
                    let a = if den.abs() < 1e-12 {
                        0.0
                    } else {
                        (u - knots[ki]) / den
                    };
                    d[i] = [
                        (1.0 - a) * d[i - 1][0] + a * d[i][0],
                        (1.0 - a) * d[i - 1][1] + a * d[i][1],
                    ];
                }
            }
            out.push(d[3]);
        }
    }
    out.push(poles[n - 1]);
    out
}

/// Volume of an (x, r) profile revolved about X (sum of frustums, x increasing).
fn revolved(pts: &[[f64; 2]]) -> f64 {
    pts.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            std::f64::consts::PI * (b[0] - a[0]) * (a[1] * a[1] + a[1] * b[1] + b[1] * b[1]) / 3.0
        })
        .sum()
}

/// Simpson integral of f over [a, b].
fn simpson(a: f64, b: f64, f: impl Fn(f64) -> f64) -> f64 {
    let n = 64;
    let h = (b - a) / n as f64;
    let mut s = f(a) + f(b);
    for i in 1..n {
        s += f(a + i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}

/// Independent coarse volume estimate [mm³] and relative tolerance, from the same `Layout`
/// numbers but polygonal sections (the CAD solid uses splines, so lifting surfaces and the
/// fuselage get a wider band). `None`: no estimate.
fn expected_volume(id: PartId, p: &Project, lay: &Layout) -> Option<(f64, f64)> {
    use std::f64::consts::PI;
    // Outer section area from the full-resolution airfoil (the CAD section is a spline
    // through a resampled subset of it).
    let surface = |raw: &[[f64; 2]],
                   g: werseus_uav_calc::cad::geom::SurfaceGeom,
                   st: werseus_uav_calc::cad::structure::SurfaceStructure| {
        let a0 = poly_area(&profile_or_default(raw).pts);
        let mut v = simpson(0.0, g.span, |z| a0 * g.chord_at(z).powi(2));
        let c_min = g.chord_at(g.span - st.wall_mm).min(g.c_root);
        if let Some(terms) = g.profile.inner_terms(st.wall_mm / c_min, 12) {
            for (za, zb) in st.bays_mm() {
                v -= simpson(za, zb, |z| {
                    let mut poles: Vec<[f64; 2]> = terms
                        .iter()
                        .map(|t| t.eval(g.chord_at(z), st.wall_mm))
                        .collect();
                    poles.push(poles[0]);
                    poly_area(&bspline_points(&poles, 8))
                });
            }
        }
        if let Some(a) = st.spar {
            let r = a.od / 2.0;
            let webs = st.root_rib_mm + st.rib_stations_mm().len() as f64 * st.rib_t_mm;
            v -= PI * r * r * webs / a.dir[2];
        }
        (v, 0.02)
    };
    let plate_holes = |t: f64, holes: &[(f64, usize)]| -> f64 {
        holes
            .iter()
            .map(|(d, n)| PI * d * d / 4.0 * t * *n as f64)
            .sum()
    };
    Some(match id {
        PartId::Wing => surface(
            &lay.wing.root_profile,
            lay.wing.geom(),
            lay.wing.structure(),
        ),
        PartId::TailHorizontal => surface(
            &lay.tail_h.profile,
            lay.tail_h.geom(),
            lay.tail_h.structure(),
        ),
        PartId::TailVertical => surface(
            &lay.tail_v.profile,
            lay.tail_v.geom(),
            lay.tail_v.structure(),
        ),
        PartId::TailVTail => surface(&lay.vtail.profile, lay.vtail.geom(), lay.vtail.structure()),
        PartId::Winglet => surface(
            &lay.winglet.profile,
            lay.winglet.geom(),
            lay.winglet.structure(),
        ),
        PartId::Fuselage => {
            let f = &lay.fuselage;
            let prof = f.profile();
            let st = f.structure();
            let mut v = revolved(&prof.pts);
            if st.wall_mm > 0.0 {
                v -= revolved(&prof.inner(st.wall_mm));
                for x in st.former_stations_mm() {
                    let r_in = prof.r_at(x) - st.wall_mm;
                    let r_h = r_in - st.former_depth_mm;
                    v += PI * (r_in * r_in - r_h * r_h) * st.former_t_mm;
                }
            }
            // Each wing/tail slot cuts the skin once: section area x wall. Formers crossed
            // by a slot and the gear pad are not estimated.
            for pl in &f.plugs {
                v -= poly_area(&pl.pts) * pl.c_r * pl.c_r * st.wall_mm;
            }
            (v, 0.06)
        }
        PartId::WingSpar | PartId::VtolBoom => {
            let t = if id == PartId::VtolBoom {
                lay.boom.clone()
            } else {
                lay.spar.clone()?
            };
            let id2 = if t.id_mm > 0.0 && t.id_mm < t.od_mm {
                t.id_mm
            } else {
                0.0
            };
            (
                PI / 4.0 * (t.od_mm.powi(2) - id2.powi(2)) * t.length_mm,
                0.01,
            )
        }
        PartId::MotorMount | PartId::LiftMotorMount | PartId::MrMotorMount => {
            let m = match id {
                PartId::LiftMotorMount => &lay.lift_mount,
                PartId::MrMotorMount => &lay.mr_mount,
                _ => &lay.motor_mount,
            };
            let t = m.thickness_mm;
            let v = m.plate_size_mm.powi(2) * t
                - plate_holes(
                    t,
                    &[
                        (m.shaft_hole_dia_mm, 1),
                        (m.motor_bolt_dia_mm, 4),
                        (m.firewall_bolt_dia_mm, 4),
                    ],
                );
            (v, 0.01)
        }
        PartId::MrPlate => {
            let m = &lay.mr_plate;
            let t = m.thickness_mm;
            let v = PI * m.plate_radius_mm.powi(2) * t
                - plate_holes(
                    t,
                    &[
                        (m.center_hole_dia_mm, 1),
                        (3.2, 4),
                        (m.chassis_bolt_dia_mm, 2 * m.arm_count as usize),
                    ],
                );
            (v, 0.01)
        }
        PartId::MrArm => {
            let a = &lay.mr_arm;
            let (w, t, l) = (a.width_mm, a.thickness_mm, a.arm_length_mm);
            let mut v = (l - w / 2.0) * w * t + PI * w * w / 8.0 * t;
            v -= plate_holes(
                t,
                &[
                    (a.motor_shaft_dia_mm, 1),
                    (a.motor_bolt_dia_mm, 4),
                    (
                        a.chassis_bolt_dia_mm,
                        if a.chassis_bolt_spacing_mm > 0.0 {
                            2
                        } else {
                            1
                        },
                    ),
                ],
            );
            let x_ch = a.chassis_hole_x0_mm + a.chassis_bolt_spacing_mm;
            let span = (l - w / 2.0 - a.motor_mount_pattern_mm / 2.0) - x_ch;
            if span
                > 12.0_f64
                    .max(4.0 * a.chassis_bolt_dia_mm)
                    .max(4.0 * a.motor_bolt_dia_mm)
            {
                v -= 0.65 * span * 0.4 * w * t;
            }
            (v, 0.03)
        }
        PartId::LandingGear => {
            let g = &lay.leg;
            let v = g.tab_l_mm * g.tab_w_mm * g.tab_t_mm
                + PI * g.leg_dia_mm.powi(2) / 4.0 * g.leg_len_mm
                + g.foot_len_mm * g.foot_w_mm * g.foot_t_mm;
            (v, 0.05)
        }
        PartId::BoomWingClamp => {
            let c = &lay.clamp;
            let ro = c.bore_mm / 2.0 + c.wall_mm;
            let rb = c.bore_mm / 2.0;
            // Pillar height up to the wing's lower surface (the saddle cut): mean height of
            // lower-surface samples over the pillar footprint, in clamp coordinates.
            let h = match &c.saddle {
                Some(sd) => {
                    let prof = lay.wing.geom().profile;
                    let (mut sum, mut n) = (0.0, 0);
                    for i in 0..=40 {
                        let z = sd.z0 + (sd.z1 - sd.z0) * i as f64 / 40.0;
                        let s_ = z.abs();
                        let ch = sd.c_r + (sd.c_t - sd.c_r) * s_ / sd.span;
                        let x0 = s_ * sd.sweep.to_radians().tan();
                        for j in 0..=200 {
                            let f = j as f64 / 200.0;
                            let q = [x0 + f * ch, prof.yl(f) * ch, z];
                            let m = &sd.m;
                            let gx = m[0] * q[0] + m[1] * q[1] + m[2] * q[2] + m[3];
                            let gy = m[4] * q[0] + m[5] * q[1] + m[6] * q[2] + m[7];
                            let gz = m[8] * q[0] + m[9] * q[1] + m[10] * q[2] + m[11];
                            if gx.abs() <= c.width_mm / 2.0 && gy.abs() <= ro {
                                sum += gz;
                                n += 1;
                            }
                        }
                    }
                    if n > 0 {
                        sum / n as f64
                    } else {
                        c.pillar_top_mm
                    }
                }
                None => c.pillar_top_mm,
            };
            let w = c.width_mm;
            let v = PI * ro * ro * w / 2.0 + 2.0 * ro * w * h
                - PI * rb * rb * w
                - c.slit_mm * w * (ro - rb);
            let _ = p;
            (v, 0.05)
        }
    })
}

/// Python checker: for each STEP prints `path|solids|valid|volume`.
const STEP_CHECK: &str = r#"
import sys, json
from build123d import import_step
from OCP.GProp import GProp_GProps
from OCP.BRepGProp import BRepGProp
for p in json.loads(sys.argv[1]):
    s = import_step(p)
    pr = GProp_GProps()
    BRepGProp.VolumeProperties_s(s.wrapped, pr, 1e-6)
    print("CHK|%s|%d|%d|%.3f" % (p, len(s.solids()), int(s.is_valid), abs(pr.Mass())))
"#;

fn check_steps(
    py: &PythonInstallation,
    dir: &Path,
    steps: &[String],
) -> Vec<(String, usize, bool, f64)> {
    let script = dir.join("_step_check.py");
    fs::write(&script, STEP_CHECK).unwrap();
    let out = py
        .command()
        .arg(&script)
        .arg(serde_json::to_string(steps).unwrap())
        .output()
        .expect("python");
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.strip_prefix("CHK|")?.split('|').collect();
            Some((
                f[0].to_string(),
                f[1].parse().ok()?,
                f[2] == "1",
                f[3].parse().ok()?,
            ))
        })
        .collect()
}

fn e2e(ex: &str) {
    let Some(py) = python() else { return };
    let p = load_example(ex).expect("example");
    let m = evaluate(&p, &Library::embedded());
    let lay = Layout::new(&p, &m);
    let out = std::env::temp_dir().join(format!("werseus_fast_e2e_{}", ex.replace('.', "_")));
    let _ = fs::remove_dir_all(&out);
    let ids = available_parts(&p);
    let t0 = std::time::Instant::now();
    let s = run_blocking(py.clone(), p.clone(), ids.clone(), out.clone()).expect("generation");
    println!(
        "FAST|{ex}|secs={:.1}|parts={}|contacts={}|interf={}|floating={}|unseated={}|mass_g={:.0}",
        t0.elapsed().as_secs_f64(),
        s.parts.len(),
        s.contacts.len(),
        s.interferences.len(),
        s.floating.len(),
        s.unseated.len(),
        s.mass_kg * 1000.0
    );
    for i in &s.interferences {
        println!("  INTERFERENCE: {} x {}: {:.2} mm3", i.a, i.b, i.volume_mm3);
    }
    assert!(s.failed.is_empty(), "{ex}: {:?}", s.failed);
    assert_eq!(s.parts.len(), ids.len());
    assert!(out.join("BOM.md").exists() && out.join("BOM.csv").exists());
    assert!(out.join("Montaj_Raporu.md").exists());
    assert!(!s.assembly_step.is_empty() && fs::metadata(&s.assembly_step).is_ok());
    assert!(s.mass_kg > 0.0);
    // Same acceptance as the FreeCAD end-to-end check.
    assert!(s.check_failed.is_empty(), "{ex}: {:?}", s.check_failed);
    assert!(s.interferences.is_empty(), "{ex}: {:?}", s.interferences);
    assert!(!s.contacts.is_empty(), "{ex}: contacts must be listed");
    assert!(s.floating.is_empty(), "{ex}: floating: {:?}", s.floating);

    // Every part: STL written, STEP is one valid solid with the expected volume.
    let steps: Vec<String> = ids
        .iter()
        .map(|&id| {
            let stem = plan_fast_part(id, &p, &lay).stem;
            assert!(out.join(format!("{stem}.stl")).exists(), "{ex}: {stem}.stl");
            out.join(format!("{stem}.step"))
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let checks = check_steps(&py, &out, &steps);
    assert_eq!(checks.len(), steps.len(), "{ex}: STEP check output");
    for (&id, (path, solids, valid, vol)) in ids.iter().zip(&checks) {
        assert_eq!(*solids, 1, "{ex}: {path}");
        assert!(*valid, "{ex}: {path}");
        let reported = s.parts.iter().find(|r| r.id == id).unwrap().volume_mm3;
        assert!(
            (vol - reported).abs() <= 1e-3 * reported.max(1.0),
            "{ex}: {path}: STEP {vol} vs built {reported}"
        );
        if let Some((exp, tol)) = expected_volume(id, &p, &lay) {
            let rel = (vol - exp) / exp;
            println!(
                "  VOL|{:?}|cad={vol:.0}|est={exp:.0}|rel={rel:+.3}|tol={tol}",
                id
            );
            assert!(
                rel.abs() <= tol,
                "{ex}: {id:?}: {vol} vs estimate {exp} ({rel:+.3})"
            );
        }
    }
    // DXF profiles for ribs / formers / plates.
    for &id in &ids {
        let stem = plan_fast_part(id, &p, &lay).stem;
        let has_dxf = out.join(format!("{stem}.dxf")).exists();
        let want = !matches!(
            id,
            PartId::LandingGear | PartId::WingSpar | PartId::VtolBoom
        );
        assert_eq!(has_dxf, want, "{ex}: {stem}.dxf");
    }
    let _ = fs::remove_dir_all(&out);
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_fixed_wing_2kg_1600mm() {
    e2e("fixed_wing_2kg_1600mm.json");
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_flying_wing_1200mm() {
    e2e("flying_wing_1200mm.json");
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_vtol_quadplane() {
    e2e("vtol_quadplane.json");
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_hexa_550_x() {
    e2e("hexa_550_x.json");
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_quad_250_x() {
    e2e("quad_250_x.json");
}

#[test]
#[ignore = "needs Python with build123d; run by hand: cargo test -- --ignored"]
fn fast_e2e_vtail_variant() {
    // V-tail fixed wing (no sample file): derived from the conventional sample.
    let Some(_) = python() else { return };
    let mut p = load_example("fixed_wing_2kg_1600mm.json").expect("example");
    p.drone_type = werseus_uav_calc::project::DroneType::FixedWingVTail;
    p.tail_kind = werseus_uav_calc::calc::geometry::TailKind::VTail;
    let py = python().unwrap();
    let out = std::env::temp_dir().join("werseus_fast_e2e_vtail");
    let _ = fs::remove_dir_all(&out);
    let ids = available_parts(&p);
    let s = run_blocking(py, p, ids.clone(), out.clone()).expect("generation");
    assert!(s.failed.is_empty(), "{:?}", s.failed);
    assert_eq!(s.parts.len(), ids.len());
    assert!(s.interferences.is_empty(), "{:?}", s.interferences);
    assert!(s.floating.is_empty(), "{:?}", s.floating);
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn expected_volume_selects_distinct_motor_mounts() {
    let p = load_example("vtol_quadplane.json").expect("example");
    let m = evaluate(&p, &Library::embedded());
    let mut lay = Layout::new(&p, &m);
    lay.lift_mount.plate_size_mm = lay.motor_mount.plate_size_mm + 10.0;
    lay.mr_mount.plate_size_mm = lay.motor_mount.plate_size_mm + 20.0;
    let v_main = expected_volume(PartId::MotorMount, &p, &lay).unwrap().0;
    let v_lift = expected_volume(PartId::LiftMotorMount, &p, &lay).unwrap().0;
    let v_mr = expected_volume(PartId::MrMotorMount, &p, &lay).unwrap().0;
    assert!(v_lift > v_main && v_mr > v_lift);
}
