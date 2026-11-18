// SPDX-License-Identifier: AGPL-3.0-or-later
//! SITL/HIL parametre koprusu (yalniz dosya uretimi): tasarimdan ArduPilot `.parm`, PX4 `param set`
//! listesi ve Gazebo tarzi govde/rotor parcasi metni uretir. Ag erisimi yok, simulator calistirmaz.
//! Her eslesme "baslangic noktasi, kendi yazilim surumunuzde dogrulayin" olarak etiketlenir.

use crate::calc::Model;
use crate::calc::multirotor::MultirotorLayout;
use crate::project::{DroneType, Project};
use std::f64::consts::PI;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleKind {
    Multirotor,
    Vtol,
    FixedWing,
}

/// Bir rotor: konum (x ileri, y sag, z yukari; metre) ve donus yonu.
#[derive(Clone, Copy, Debug)]
pub struct Rotor {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub ccw: bool,
}

/// Tasarimdan cikarilan, simulatore verilecek ozet.
#[derive(Clone, Debug)]
pub struct VehicleSpec {
    pub kind: VehicleKind,
    pub arm_count: u32,
    pub n_motors: u32,
    pub layout: MultirotorLayout,
    pub coaxial: bool,
    pub mass_kg: f64,
    pub hover_throttle: f64,
    pub t_hover_motor_n: f64,
    pub t_max_motor_n: f64,
    pub cells_s: u32,
    pub capacity_mah: f64,
    pub cell_full_v: f64,
    pub cell_low_v: f64,
    pub cell_crit_v: f64,
    pub pack_v: f64,
    pub d_prop_m: f64,
    pub rho: f64,
    pub rotors: Vec<Rotor>,
    pub motor_mass_kg: f64,
    pub r_plate_m: f64,
    /// Motor KV (dev/dk/V); None ise itki ve Ct'den tahmin edilir.
    pub kv: Option<f64>,
    /// Kullanilacak Ct (olcumden ya da varsayilan 0.10).
    pub ct: f64,
}

fn fin(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

impl VehicleSpec {
    /// Projeden ve hesap modelinden ozet cikarir. Tum degerler sonlu olacak sekilde temizlenir.
    pub fn from_model(p: &Project, m: &Model, kv: Option<f64>, ct: Option<f64>) -> VehicleSpec {
        let kind = match p.drone_type {
            DroneType::Multirotor => VehicleKind::Multirotor,
            DroneType::VtolQuadplane => VehicleKind::Vtol,
            _ => VehicleKind::FixedWing,
        };
        let lift = kind != VehicleKind::FixedWing;
        let mass = if kind == VehicleKind::Multirotor || kind == VehicleKind::Vtol {
            m.mr.m_mtow
        } else {
            m.weight.m_mtow
        };
        let n_motors = if lift { m.mr.n_motors } else { 1 };
        let (t_hover, t_max, d_prop, hover_thr) = if lift {
            (
                m.mr.t_hover_motor,
                p.mr_t_max_motor,
                p.mr_d_prop,
                m.mr.hover_throttle,
            )
        } else {
            let thr = if m.prop.t_static > 0.0 {
                (m.prop.t_cr / m.prop.t_static).max(0.0).sqrt()
            } else {
                0.0
            };
            (m.prop.t_cr, m.prop.t_static, p.d_prop, thr)
        };
        let nom = if m.cell.nominal_v > 0.0 {
            m.cell.nominal_v
        } else {
            3.7
        };
        let cells_s = ((m.weight.pack_voltage / nom).round() as i64).clamp(1, 36) as u32;
        let ion = m.cell.chemistry.to_lowercase().contains("ion") || nom < 3.65;
        let (full, low, crit) = if ion {
            (4.2, 3.3, 3.0)
        } else {
            (4.2, 3.5, 3.3)
        };
        let mut rotors = Vec::new();
        if lift {
            let coords = &m.mr.motor_coords;
            for (i, &(x, y)) in coords.iter().enumerate() {
                let ccw = i % 2 == 0;
                rotors.push(Rotor { x, y, z: 0.0, ccw });
                if p.mr_coaxial {
                    rotors.push(Rotor {
                        x,
                        y,
                        z: -0.03,
                        ccw: !ccw,
                    });
                }
            }
        }
        VehicleSpec {
            kind,
            arm_count: p.mr_arm_count,
            n_motors,
            layout: p.mr_layout,
            coaxial: p.mr_coaxial,
            mass_kg: fin(mass),
            hover_throttle: fin(hover_thr),
            t_hover_motor_n: fin(t_hover),
            t_max_motor_n: fin(t_max),
            cells_s,
            capacity_mah: fin(m.weight.pack_capacity_ah * 1000.0),
            cell_full_v: full,
            cell_low_v: low,
            cell_crit_v: crit,
            pack_v: fin(m.weight.pack_voltage),
            d_prop_m: fin(d_prop),
            rho: if m.atmo.rho > 0.1 { m.atmo.rho } else { 1.225 },
            rotors,
            motor_mass_kg: fin(p.mr_m_motor),
            r_plate_m: fin(p.mr_r_plate),
            kv: kv.filter(|k| k.is_finite() && *k > 0.0),
            ct: ct.filter(|c| c.is_finite() && *c > 0.0).unwrap_or(0.10),
        }
    }

    /// Azami rotor hizi [rad/s]: KV verildiyse KV*V*2pi/60, yoksa azami itkiden Ct ile.
    pub fn max_rot_velocity(&self) -> f64 {
        let w = if let Some(kv) = self.kv {
            kv * self.pack_v * 2.0 * PI / 60.0
        } else if self.t_max_motor_n > 0.0 && self.d_prop_m > 0.0 {
            // T = Ct rho n^2 D^4
            let n = (self.t_max_motor_n / (self.ct * self.rho * self.d_prop_m.powi(4))).sqrt();
            2.0 * PI * n
        } else {
            0.0
        };
        fin(w)
    }

    /// Gazebo `motorConstant`: T = k * omega^2 -> k = Ct rho D^4 / (4 pi^2).
    pub fn motor_constant(&self) -> f64 {
        fin(self.ct * self.rho * self.d_prop_m.powi(4) / (4.0 * PI * PI))
    }

    /// Eylemsizlik tahmini (kg m^2): motorlar nokta kutle, kalan kutle r_plate yaricapli disk.
    pub fn inertia_estimate(&self) -> (f64, f64, f64) {
        let n = self.rotors.len().max(1) as f64;
        let m_motors = (self.motor_mass_kg * self.n_motors as f64).min(self.mass_kg);
        let m_each = m_motors / n;
        let rest = (self.mass_kg - m_motors).max(0.0);
        let r = self.r_plate_m.max(0.02);
        let (mut ixx, mut iyy) = (0.0, 0.0);
        for q in &self.rotors {
            ixx += m_each * q.y * q.y;
            iyy += m_each * q.x * q.x;
        }
        ixx += rest * r * r / 4.0;
        iyy += rest * r * r / 4.0;
        let izz = ixx + iyy;
        (fin(ixx), fin(iyy), fin(izz))
    }

    /// Tum sayisal alanlari sonlu yapar (yazicilar NaN/inf basmasin).
    fn sane(&self) -> VehicleSpec {
        let mut v = self.clone();
        v.mass_kg = fin(v.mass_kg).max(0.0);
        v.hover_throttle = fin(v.hover_throttle);
        v.t_hover_motor_n = fin(v.t_hover_motor_n);
        v.t_max_motor_n = fin(v.t_max_motor_n);
        v.capacity_mah = fin(v.capacity_mah).max(0.0);
        v.pack_v = fin(v.pack_v);
        v.d_prop_m = fin(v.d_prop_m);
        v.rho = if v.rho.is_finite() && v.rho > 0.1 {
            v.rho
        } else {
            1.225
        };
        v.motor_mass_kg = fin(v.motor_mass_kg);
        v.r_plate_m = fin(v.r_plate_m);
        v.ct = if v.ct.is_finite() && v.ct > 0.0 {
            v.ct
        } else {
            0.10
        };
        for r in &mut v.rotors {
            r.x = fin(r.x);
            r.y = fin(r.y);
            r.z = fin(r.z);
        }
        v
    }

    fn mot_prefix(&self) -> &'static str {
        if self.kind == VehicleKind::Vtol {
            "Q_M_"
        } else {
            "MOT_"
        }
    }
}

const WARN_EN: &str = "Starting point, verify on your firmware version.";

/// ArduPilot FRAME_CLASS ve FRAME_TYPE; ikinci deger belirsizlik notu.
fn ardupilot_frame(s: &VehicleSpec) -> (u32, u32, Option<&'static str>) {
    let class = match (s.arm_count, s.coaxial) {
        (3, false) => 7,
        (3, true) => 5,
        (4, false) => 1,
        (4, true) => 4,
        (6, false) => 2,
        (6, true) => 10,
        (8, false) => 3,
        _ => 1,
    };
    let ftype = match s.layout {
        MultirotorLayout::Plus => 0,
        MultirotorLayout::X => 1,
        MultirotorLayout::H => 3,
    };
    let note = match (s.arm_count, s.coaxial, s.layout) {
        (8, true, _) => Some("no exact class for 8 coaxial arms; pick a frame by hand"),
        (6, true, _) => Some("coaxial hexa class 10 (DodecaHexa): verify"),
        (n, _, MultirotorLayout::H) if n != 4 => Some("H layout is quad-only; check FRAME_TYPE"),
        (3, _, _) => Some("tri/Y6 frame types differ from X/Plus; verify FRAME_TYPE"),
        _ => None,
    };
    (class, ftype, note)
}

/// ArduPilot SITL `.parm` metni.
pub fn ardupilot_parm(s: &VehicleSpec) -> String {
    let s = &s.sane();
    let mut o = String::new();
    let _ = writeln!(o, "# WerSeuS UAV Calc -> ArduPilot SITL parameter file");
    let _ = writeln!(o, "# {WARN_EN}");
    let _ = writeln!(
        o,
        "# Load with: --add-param-file=<this file>  (or MAVProxy: param load <this file>)"
    );
    let _ = writeln!(
        o,
        "# The physical flight model (SITL JSON / FlightAxis) is outside the scope of this file."
    );
    let _ = writeln!(
        o,
        "# Mass {:.3} kg, {} motor(s), {}S {:.0} mAh.",
        s.mass_kg, s.n_motors, s.cells_s, s.capacity_mah
    );
    let _ = writeln!(o);
    match s.kind {
        VehicleKind::FixedWing => {
            let _ = writeln!(
                o,
                "# Fixed wing: only battery values are exported (ArduPlane: sim_vehicle.py -v ArduPlane -f plane)."
            );
        }
        VehicleKind::Multirotor | VehicleKind::Vtol => {
            let (class, ftype, note) = ardupilot_frame(s);
            let pre = if s.kind == VehicleKind::Vtol {
                "Q_"
            } else {
                ""
            };
            if s.kind == VehicleKind::Vtol {
                let _ = writeln!(o, "Q_ENABLE,1");
                let _ = writeln!(
                    o,
                    "# QuadPlane: sim_vehicle.py -v ArduPlane -f quadplane; lift motors use Q_ prefix."
                );
            } else {
                let _ = writeln!(
                    o,
                    "# Frame name for sim_vehicle.py -f: quad / hexa / octa / y6 / tri / octa-quad (verify)."
                );
            }
            if let Some(n) = note {
                let _ = writeln!(o, "# UNCERTAIN: {n}");
            }
            let _ = writeln!(o, "{pre}FRAME_CLASS,{class}");
            let _ = writeln!(o, "{pre}FRAME_TYPE,{ftype}");
            let hover = s.hover_throttle.clamp(0.2, 0.8);
            let mp = s.mot_prefix();
            let _ = writeln!(
                o,
                "# Hover throttle from the model estimate sqrt(T_hover/T_max); refine from a real flight log."
            );
            let _ = writeln!(o, "{mp}THST_HOVER,{hover:.3}");
            let _ = writeln!(o, "{mp}SPIN_MIN,0.15");
            let _ = writeln!(o, "{mp}SPIN_ARM,0.10");
            let _ = writeln!(
                o,
                "# Voltage compensation limits (UNCERTAIN names on some versions)."
            );
            let _ = writeln!(
                o,
                "{mp}BAT_VOLT_MAX,{:.2}",
                s.cell_full_v * s.cells_s as f64
            );
            let _ = writeln!(
                o,
                "{mp}BAT_VOLT_MIN,{:.2}",
                s.cell_crit_v * s.cells_s as f64
            );
        }
    }
    let _ = writeln!(o);
    let _ = writeln!(o, "BATT_MONITOR,4");
    let _ = writeln!(o, "BATT_CAPACITY,{:.0}", s.capacity_mah);
    let _ = writeln!(o, "BATT_LOW_VOLT,{:.2}", s.cell_low_v * s.cells_s as f64);
    let _ = writeln!(o, "BATT_CRT_VOLT,{:.2}", s.cell_crit_v * s.cells_s as f64);
    o
}

/// PX4 `param set` satirlari (px4 shell / QGroundControl parametre dosyasi icin).
pub fn px4_params(s: &VehicleSpec) -> String {
    let s = &s.sane();
    let mut o = String::new();
    let _ = writeln!(o, "# WerSeuS UAV Calc -> PX4 SITL parameter list");
    let _ = writeln!(o, "# {WARN_EN}");
    let _ = writeln!(
        o,
        "# Paste into the px4 shell (pxh>) or load via QGroundControl. Mass {:.3} kg, {} motor(s).",
        s.mass_kg, s.n_motors
    );
    let _ = writeln!(o);
    if s.kind == VehicleKind::FixedWing {
        let _ = writeln!(
            o,
            "# Fixed wing: only battery values are exported (PX4 plane airframes differ widely)."
        );
    } else {
        let _ = writeln!(
            o,
            "# UNCERTAIN: SYS_AUTOSTART depends on the airframe (generic quad X is usually 4001); pick one from the PX4 airframe list."
        );
        let _ = writeln!(
            o,
            "param set MPC_THR_HOVER {:.2}",
            s.hover_throttle.clamp(0.1, 0.8)
        );
        let _ = writeln!(
            o,
            "# Control allocation (PX4 1.13 or newer; verify names on yours):"
        );
        let _ = writeln!(o, "param set CA_AIRFRAME 0");
        let _ = writeln!(o, "param set CA_ROTOR_COUNT {}", s.rotors.len());
        for (i, r) in s.rotors.iter().enumerate() {
            // PX4: x ileri, y sag, z asagi.
            let _ = writeln!(o, "param set CA_ROTOR{i}_PX {:.4}", fin(r.x));
            let _ = writeln!(o, "param set CA_ROTOR{i}_PY {:.4}", fin(r.y));
            let _ = writeln!(o, "param set CA_ROTOR{i}_PZ {:.4}", fin(-r.z));
            let _ = writeln!(
                o,
                "param set CA_ROTOR{i}_KM {}",
                if r.ccw { "0.0500" } else { "-0.0500" }
            );
        }
        let _ = writeln!(
            o,
            "# Spin directions simply alternate in motor order; check against your real motor map."
        );
    }
    let _ = writeln!(o);
    let _ = writeln!(o, "param set BAT1_N_CELLS {}", s.cells_s);
    let _ = writeln!(o, "param set BAT1_CAPACITY {:.0}", s.capacity_mah);
    let _ = writeln!(o, "param set BAT1_V_CHARGED {:.2}", s.cell_full_v);
    let _ = writeln!(o, "param set BAT1_V_EMPTY {:.2}", s.cell_low_v);
    o
}

/// Gazebo tarzi govde / rotor parcasi (SDF benzeri metin). Gazebo ekseni: x ileri, y SOL, z yukari.
pub fn gazebo_snippet(s: &VehicleSpec) -> String {
    let s = &s.sane();
    let (ixx, iyy, izz) = s.inertia_estimate();
    let mut o = String::new();
    let _ = writeln!(
        o,
        "<!-- WerSeuS UAV Calc -> Gazebo-style body and rotor snippet -->"
    );
    let _ = writeln!(
        o,
        "<!-- {WARN_EN} Inertia is a rough point-mass estimate. -->"
    );
    let _ = writeln!(
        o,
        "<!-- Plugin names follow gazebo-classic (libgazebo_motor_model); newer Gazebo versions use other plugins. -->"
    );
    let _ = writeln!(o, "<link name='base_link'>");
    let _ = writeln!(o, "  <inertial>");
    let _ = writeln!(o, "    <mass>{:.4}</mass>", s.mass_kg);
    let _ = writeln!(
        o,
        "    <inertia><ixx>{ixx:.6}</ixx><ixy>0</ixy><ixz>0</ixz><iyy>{iyy:.6}</iyy><iyz>0</iyz><izz>{izz:.6}</izz></inertia>"
    );
    let _ = writeln!(o, "  </inertial>");
    let _ = writeln!(o, "</link>");
    let w_max = s.max_rot_velocity();
    let kconst = s.motor_constant();
    let src = if s.kv.is_some() {
        "KV x pack voltage (unloaded, optimistic)"
    } else {
        "estimated from max thrust and Ct"
    };
    let _ = writeln!(o, "<!-- maxRotVelocity [rad/s]: {src} -->");
    for (i, r) in s.rotors.iter().enumerate() {
        let _ = writeln!(o, "<link name='rotor_{i}'>");
        let _ = writeln!(
            o,
            "  <pose>{:.4} {:.4} {:.4} 0 0 0</pose>",
            fin(r.x),
            fin(-r.y),
            fin(r.z)
        );
        let _ = writeln!(o, "</link>");
        let _ = writeln!(
            o,
            "<plugin name='rotor_{i}_model' filename='libgazebo_motor_model.so'>"
        );
        let _ = writeln!(o, "  <jointName>rotor_{i}_joint</jointName>");
        let _ = writeln!(o, "  <linkName>rotor_{i}</linkName>");
        let _ = writeln!(
            o,
            "  <turningDirection>{}</turningDirection>",
            if r.ccw { "ccw" } else { "cw" }
        );
        let _ = writeln!(o, "  <maxRotVelocity>{w_max:.1}</maxRotVelocity>");
        let _ = writeln!(o, "  <motorConstant>{kconst:.4e}</motorConstant>");
        let _ = writeln!(o, "  <momentConstant>0.016</momentConstant>");
        let _ = writeln!(o, "  <motorNumber>{i}</motorNumber>");
        let _ = writeln!(o, "</plugin>");
    }
    if s.rotors.is_empty() {
        let _ = writeln!(
            o,
            "<!-- Fixed wing: no lift rotors exported; use a plane model from your simulator. -->"
        );
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    fn spec_for(dt: DroneType, arms: u32) -> VehicleSpec {
        let mut p = Project::default();
        p.set_drone_type(dt);
        p.mr_arm_count = arms;
        let m = evaluate(&p, &Library::embedded());
        VehicleSpec::from_model(&p, &m, None, None)
    }

    fn assert_clean(s: &str) {
        let l = s.to_lowercase();
        assert!(!l.contains("nan"), "{s}");
        assert!(!l.contains("inf"), "{s}");
    }

    fn value_of(text: &str, key: &str) -> f64 {
        let line = text
            .lines()
            .find(|l| l.starts_with(key) && !l.starts_with('#'))
            .unwrap_or_else(|| panic!("{key} yok:\n{text}"));
        line.trim_start_matches(key)
            .trim_matches(|c: char| c == ',' || c == ' ')
            .parse()
            .unwrap()
    }

    #[test]
    fn parm_quad_has_expected_keys_in_range() {
        let s = spec_for(DroneType::Multirotor, 4);
        let t = ardupilot_parm(&s);
        assert!(t.contains("FRAME_CLASS,1"));
        assert!(t.contains("FRAME_TYPE,1"));
        let h = value_of(&t, "MOT_THST_HOVER");
        assert!((0.2..=0.8).contains(&h));
        assert!(value_of(&t, "BATT_CAPACITY") > 100.0);
        let low = value_of(&t, "BATT_LOW_VOLT");
        assert!((low - s.cell_low_v * s.cells_s as f64).abs() < 0.01);
        assert!(t.contains("MOT_SPIN_MIN"));
        assert!(t.contains("verify on your firmware"));
        assert!(t.contains("outside the scope"));
        assert_clean(&t);
    }

    #[test]
    fn parm_hexa_and_px4_hexa() {
        let s = spec_for(DroneType::Multirotor, 6);
        assert_eq!(s.rotors.len(), 6);
        let t = ardupilot_parm(&s);
        assert!(t.contains("FRAME_CLASS,2"));
        let px = px4_params(&s);
        assert!(px.contains("CA_ROTOR_COUNT 6"));
        assert!(px.contains("CA_ROTOR5_PX"));
        assert!(!px.contains("CA_ROTOR6_PX"));
        assert_clean(&px);
    }

    #[test]
    fn px4_quad_keys_and_values() {
        let s = spec_for(DroneType::Multirotor, 4);
        let t = px4_params(&s);
        let thr: f64 = t
            .lines()
            .find(|l| l.starts_with("param set MPC_THR_HOVER"))
            .unwrap()
            .rsplit(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!((0.1..=0.8).contains(&thr));
        assert!(t.contains(&format!("param set BAT1_N_CELLS {}", s.cells_s)));
        assert!(t.contains("param set BAT1_CAPACITY "));
        assert!(t.contains("UNCERTAIN"));
        assert_clean(&t);
    }

    #[test]
    fn gazebo_snippet_has_mass_inertia_rotors() {
        let s = spec_for(DroneType::Multirotor, 4);
        let g = gazebo_snippet(&s);
        assert!(g.contains("<mass>"));
        assert!(g.contains("<ixx>"));
        assert_eq!(g.matches("<plugin ").count(), 4);
        assert!(s.max_rot_velocity() > 100.0, "{}", s.max_rot_velocity());
        assert!(s.motor_constant() > 0.0);
        let (ixx, iyy, izz) = s.inertia_estimate();
        assert!(ixx > 0.0 && iyy > 0.0 && (izz - ixx - iyy).abs() < 1e-12);
        assert_clean(&g);
    }

    #[test]
    fn kv_sets_max_rot_velocity() {
        let mut s = spec_for(DroneType::Multirotor, 4);
        s.kv = Some(1000.0);
        s.pack_v = 16.0;
        let w = s.max_rot_velocity();
        assert!((w - 1000.0 * 16.0 * 2.0 * PI / 60.0).abs() < 1e-6);
    }

    #[test]
    fn all_vehicle_types_produce_clean_outputs() {
        for dt in DroneType::ALL {
            let s = spec_for(dt, 4);
            for t in [ardupilot_parm(&s), px4_params(&s), gazebo_snippet(&s)] {
                assert!(!t.is_empty());
                assert_clean(&t);
            }
            assert!(ardupilot_parm(&s).contains("BATT_CAPACITY"));
        }
        let vtol = spec_for(DroneType::VtolQuadplane, 4);
        assert!(ardupilot_parm(&vtol).contains("Q_M_THST_HOVER"));
        let fw = spec_for(DroneType::FixedWingConventional, 4);
        assert!(!ardupilot_parm(&fw).contains("FRAME_CLASS"));
    }

    #[test]
    fn degenerate_spec_does_not_nan() {
        let mut s = spec_for(DroneType::Multirotor, 4);
        s.mass_kg = f64::NAN;
        s.hover_throttle = f64::NAN;
        s.d_prop_m = 0.0;
        s.t_max_motor_n = 0.0;
        s.capacity_mah = f64::INFINITY;
        let _ = (
            s.max_rot_velocity(),
            s.motor_constant(),
            s.inertia_estimate(),
        );
        s.mass_kg = 0.0;
        s.capacity_mah = 0.0;
        s.hover_throttle = 0.0;
        for t in [ardupilot_parm(&s), px4_params(&s), gazebo_snippet(&s)] {
            assert_clean(&t);
        }
    }
}
