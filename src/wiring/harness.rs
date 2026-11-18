// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kablo demeti: varsayılan bağlantı listesi, güzergâh uzunluğu, AWG seçimi, gerilim düşümü,
//! kütle ve konnektör önerisi. Değerler "tipik"tir (silikon kablo, kısa demet).

use crate::cad::layout::Layout;
use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::i18n::t;
use crate::project::{DroneType, Project};
use std::fmt::Write;

/// Bakır yoğunluğu (g/cm³).
const CU_DENSITY: f64 = 8.96;
/// 20 °C bakır özdirenci (Ω·m) ve sıcaklık katsayısı; kablo 50 °C varsayılır.
const RHO_20: f64 = 1.724e-8;
const ALPHA_CU: f64 = 0.00393;
const T_WIRE_C: f64 = 50.0;
/// Koaksiyel (RG-178 benzeri) tipik kütle (g/m).
pub const COAX_G_PER_M: f64 = 6.0;
/// En kısa güzergâh uzunluğu (m): konnektör ve lehim payı.
const MIN_ROUTE_M: f64 = 0.05;

/// (AWG, tipik sürekli akım A — silikon kısa demet, yalıtım g/m).
const AWG_TABLE: [(u8, f64, f64); 13] = [
    (28, 3.0, 0.18),
    (26, 4.0, 0.25),
    (24, 6.0, 0.35),
    (22, 9.0, 0.50),
    (20, 12.0, 0.70),
    (18, 20.0, 1.00),
    (16, 30.0, 1.50),
    (14, 45.0, 2.10),
    (12, 65.0, 3.00),
    (10, 95.0, 4.40),
    (8, 130.0, 6.50),
    (6, 180.0, 9.50),
    (4, 250.0, 14.0),
];

/// Çıplak bakır çapı (mm).
pub fn awg_diameter_mm(awg: u8) -> f64 {
    0.127 * 92f64.powf((36.0 - awg as f64) / 39.0)
}

pub fn awg_area_mm2(awg: u8) -> f64 {
    std::f64::consts::FRAC_PI_4 * awg_diameter_mm(awg).powi(2)
}

/// Kablo sıcaklığında direnç (Ω/m).
pub fn resistance_per_m(awg: u8) -> f64 {
    let rho = RHO_20 * (1.0 + ALPHA_CU * (T_WIRE_C - 20.0));
    rho / (awg_area_mm2(awg) * 1e-6)
}

fn table_row(awg: u8) -> (u8, f64, f64) {
    AWG_TABLE
        .iter()
        .copied()
        .find(|r| r.0 == awg)
        .unwrap_or(AWG_TABLE[AWG_TABLE.len() - 1])
}

pub fn ampacity_a(awg: u8) -> f64 {
    table_row(awg).1
}

/// Metre başına bakır + yalıtım kütlesi (g/m), tek iletken.
pub fn mass_per_m(awg: u8) -> f64 {
    awg_area_mm2(awg) * CU_DENSITY + table_row(awg).2
}

/// Gerilim düşümü (V): `loop_factor` 2 ise gidiş + dönüş.
pub fn voltage_drop(i: f64, awg: u8, len_m: f64, loop_factor: f64) -> f64 {
    i * resistance_per_m(awg) * len_m * loop_factor
}

/// Akım ve gerilim düşümü sınırına göre en ince uygun AWG. `false`: en kalın bile yetmedi.
pub fn select_awg(
    i: f64,
    v: f64,
    len_m: f64,
    loop_factor: f64,
    limit_pct: f64,
    min_awg_thin: u8,
) -> (u8, bool) {
    for (awg, amp, _) in AWG_TABLE {
        if awg > min_awg_thin {
            continue;
        }
        let pct = if v > 0.0 {
            voltage_drop(i, awg, len_m, loop_factor) / v * 100.0
        } else {
            0.0
        };
        if amp >= i && pct <= limit_pct {
            return (awg, true);
        }
    }
    (AWG_TABLE[AWG_TABLE.len() - 1].0, false)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    Xt30,
    Xt60,
    Xt90,
    As150,
    Bullet35,
    Bullet4,
    Bullet55,
    JstGh,
    JstSh,
    Jr3,
    Ufl,
}

impl Connector {
    pub fn label(self) -> &'static str {
        match self {
            Connector::Xt30 => "XT30",
            Connector::Xt60 => "XT60",
            Connector::Xt90 => "XT90",
            Connector::As150 => "AS150",
            Connector::Bullet35 => "Bullet 3.5 mm",
            Connector::Bullet4 => "Bullet 4 mm",
            Connector::Bullet55 => "Bullet 5.5 mm",
            Connector::JstGh => "JST-GH",
            Connector::JstSh => "JST-SH",
            Connector::Jr3 => "JR 3-pin",
            Connector::Ufl => "U.FL (IPEX)",
        }
    }
}

/// Güç konnektörü eşikleri (tipik sürekli akım): XT30 ≤ 30 A, XT60 ≤ 60 A, XT90 ≤ 90 A, üstü AS150.
pub fn power_connector(i: f64) -> Connector {
    if i <= 30.0 {
        Connector::Xt30
    } else if i <= 60.0 {
        Connector::Xt60
    } else if i <= 90.0 {
        Connector::Xt90
    } else {
        Connector::As150
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Power,
    Signal,
    Rf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WireKind {
    BatteryMain,
    EscPower,
    MotorPhase,
    EscSignal,
    EscTelemetry,
    PdbToFc,
    Receiver,
    Gps,
    Compass,
    Telemetry,
    VtxControl,
    VtxPower,
    Camera,
    Servo,
    Buzzer,
    Led,
    RxAntenna,
    VtxAntenna,
}

impl WireKind {
    pub fn func(self) -> Func {
        match self {
            WireKind::BatteryMain
            | WireKind::EscPower
            | WireKind::MotorPhase
            | WireKind::PdbToFc
            | WireKind::VtxPower => Func::Power,
            WireKind::RxAntenna | WireKind::VtxAntenna => Func::Rf,
            _ => Func::Signal,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            WireKind::BatteryMain => "wiring.wire.battery_main",
            WireKind::EscPower => "wiring.wire.esc_power",
            WireKind::MotorPhase => "wiring.wire.motor_phase",
            WireKind::EscSignal => "wiring.wire.esc_signal",
            WireKind::EscTelemetry => "wiring.wire.esc_telemetry",
            WireKind::PdbToFc => "wiring.wire.pdb_fc",
            WireKind::Receiver => "wiring.wire.receiver",
            WireKind::Gps => "wiring.wire.gps",
            WireKind::Compass => "wiring.wire.compass",
            WireKind::Telemetry => "wiring.wire.telemetry",
            WireKind::VtxControl => "wiring.wire.vtx_control",
            WireKind::VtxPower => "wiring.wire.vtx_power",
            WireKind::Camera => "wiring.wire.camera",
            WireKind::Servo => "wiring.wire.servo",
            WireKind::Buzzer => "wiring.wire.buzzer",
            WireKind::Led => "wiring.wire.led",
            WireKind::RxAntenna => "wiring.wire.rx_antenna",
            WireKind::VtxAntenna => "wiring.wire.vtx_antenna",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Battery,
    Pdb,
    Fc,
    Esc,
    Motor,
    Servo,
    Rx,
    Gps,
    Telem,
    Vtx,
    Camera,
    Buzzer,
    Led,
    RxAnt,
    VtxAnt,
}

impl NodeKind {
    pub fn key(self) -> &'static str {
        match self {
            NodeKind::Battery => "wiring.node.battery",
            NodeKind::Pdb => "wiring.node.pdb",
            NodeKind::Fc => "wiring.node.fc",
            NodeKind::Esc => "wiring.node.esc",
            NodeKind::Motor => "wiring.node.motor",
            NodeKind::Servo => "wiring.node.servo",
            NodeKind::Rx => "wiring.node.rx",
            NodeKind::Gps => "wiring.node.gps",
            NodeKind::Telem => "wiring.node.telem",
            NodeKind::Vtx => "wiring.node.vtx",
            NodeKind::Camera => "wiring.node.camera",
            NodeKind::Buzzer => "wiring.node.buzzer",
            NodeKind::Led => "wiring.node.led",
            NodeKind::RxAnt => "wiring.node.rx_ant",
            NodeKind::VtxAnt => "wiring.node.vtx_ant",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: NodeKind,
    /// 0 tabanlı sıra (motor, ESC, servo için); yoksa `None`.
    pub idx: Option<u32>,
    /// Montaj çerçevesinde konum (m).
    pub pos: [f64; 3],
}

impl Node {
    pub fn label(&self) -> String {
        match self.idx {
            Some(i) => format!("{} {}", t(self.kind.key()), i + 1),
            None => t(self.kind.key()).to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Wire {
    pub kind: WireKind,
    pub from: usize,
    pub to: usize,
    /// Demetteki iletken sayısı (çift DC = 2, servo = 3 ...).
    pub conductors: u32,
    /// Gerilim düşümü için gidiş-dönüş çarpanı (1 veya 2).
    pub loop_factor: f64,
    pub current_a: f64,
    pub voltage_v: f64,
    pub idx: Option<u32>,
    /// Faz teli ise 0..3.
    pub phase: Option<u8>,
    /// Kullanıcının elle girdiği uzunluk (m).
    pub length_override_m: Option<f64>,
}

impl Wire {
    pub fn func(&self) -> Func {
        self.kind.func()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Güzergâh payı (0.10 = %10).
    pub slack: f64,
    /// İzin verilen gerilim düşümü (% — hat gerilimine göre).
    pub drop_limit_pct: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            slack: 0.10,
            drop_limit_pct: 2.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Row {
    pub awg: Option<u8>,
    pub straight_m: f64,
    pub length_m: f64,
    pub drop_v: f64,
    pub drop_pct: f64,
    pub mass_g: f64,
    pub connector: Connector,
    /// En kalın kablo bile sınırı sağlamadı.
    pub over_limit: bool,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub rows: Vec<Row>,
    pub mass_g: f64,
    pub worst_drop_pct: f64,
    pub worst_idx: Option<usize>,
    pub n_over_limit: usize,
    /// Pil, ESC, motor zincirinin birikmiş gerilim düşümü (%): ana hat + en kötü ESC hattı + en kötü faz kablosu.
    pub chain_drop_pct: f64,
}

#[derive(Clone, Debug)]
pub struct Harness {
    pub drone: DroneType,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
}

fn sane(x: f64) -> f64 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

fn pt(p: [f64; 3]) -> [f64; 3] {
    p.map(|v| if v.is_finite() { v } else { 0.0 })
}

fn inst_pos(lay: &Layout, label: &str) -> Option<[f64; 3]> {
    lay.instances
        .iter()
        .find(|i| i.label == label)
        .map(|i| pt(i.frame.t.map(|v| v / 1000.0)))
}

fn connector_for(kind: WireKind, i: f64) -> Connector {
    match kind {
        WireKind::BatteryMain | WireKind::EscPower | WireKind::PdbToFc | WireKind::VtxPower => {
            if i <= 1.0 {
                Connector::JstGh
            } else {
                power_connector(i)
            }
        }
        WireKind::MotorPhase => {
            if i <= 30.0 {
                Connector::Bullet35
            } else if i <= 60.0 {
                Connector::Bullet4
            } else {
                Connector::Bullet55
            }
        }
        WireKind::Servo => Connector::Jr3,
        WireKind::Compass | WireKind::EscSignal => Connector::JstSh,
        WireKind::RxAntenna | WireKind::VtxAntenna => Connector::Ufl,
        _ => Connector::JstGh,
    }
}

impl Harness {
    /// Mevcut araç için varsayılan bağlantı listesi.
    pub fn default_for(p: &Project, m: &Model) -> Harness {
        let lay = Layout::new(p, m);
        let dt = p.drone_type;
        let v_pack = if sane(m.weight.pack_voltage) > 0.0 {
            m.weight.pack_voltage
        } else {
            14.8
        };
        let tw = m.mr.tw_ratio;
        let peak_f = if tw.is_finite() && tw > 0.0 {
            tw.clamp(1.0, 4.0).powf(1.5)
        } else {
            2f64.powf(1.5)
        };
        let n_mr = m.mr.n_motors.max(1) as f64;
        let i_lift = sane(m.mr.i_hover) / n_mr * peak_f;
        let i_prop = sane(m.prop.i_peak);

        let mut nodes: Vec<Node> = Vec::new();
        let mut add = |kind: NodeKind, idx: Option<u32>, pos: [f64; 3]| -> usize {
            nodes.push(Node {
                kind,
                idx,
                pos: pt(pos),
            });
            nodes.len() - 1
        };

        // Motor konumları ve akımları.
        let mut motors: Vec<([f64; 3], f64)> = Vec::new();
        let fuse_l = (lay.fuselage.length_mm / 1000.0).max(0.1);
        let pusher = lay.motor_mount.mount_type == "pusher";
        let mut n_servo_pos: Vec<[f64; 3]> = Vec::new();
        let (bus_i, base): (f64, [[f64; 3]; 11]);
        let mr = dt == DroneType::Multirotor;
        if mr {
            let coords: Vec<(f64, f64)> = if m.mr.motor_coords.is_empty() {
                let n = m.mr.n_motors.max(1) as usize;
                (0..n)
                    .map(|k| {
                        let a = std::f64::consts::TAU * k as f64 / n as f64;
                        (0.2 * a.cos(), 0.2 * a.sin())
                    })
                    .collect()
            } else {
                m.mr.motor_coords.clone()
            };
            for k in 0..m.mr.n_motors.max(1) as usize {
                let pos = if let Some(q) = inst_pos(&lay, &format!("MrMotorMount_{k}")) {
                    q
                } else {
                    let c = coords[k % coords.len()];
                    let dz = if k >= coords.len() { -0.03 } else { 0.0 }; // koaksiyel alt motor
                    [c.0, c.1, dz]
                };
                motors.push((pos, i_lift));
            }
            bus_i = i_lift * motors.len() as f64;
            // Pil, PDB, FC, Rx, GPS, Telem, VTX, Kamera, Buzzer, LED, anten tabanı
            base = [
                [0.0, 0.0, -0.03],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.02],
                [-0.03, 0.0, 0.02],
                [0.0, 0.0, 0.10],
                [0.03, 0.0, 0.02],
                [-0.04, 0.0, 0.03],
                [0.06, 0.0, 0.02],
                [0.0, 0.05, 0.0],
                [0.0, -0.05, 0.0],
                [-0.05, 0.0, 0.08],
            ];
        } else {
            let motor_x = if pusher { fuse_l } else { 0.0 };
            let mp = inst_pos(&lay, "MotorMount").unwrap_or([motor_x, 0.0, 0.0]);
            if dt == DroneType::VtolQuadplane {
                for tag in ["LF", "LA", "RF", "RA"] {
                    let side = &tag[..1];
                    let end = &tag[1..];
                    let pos = inst_pos(&lay, &format!("LiftMount_{side}{end}")).unwrap_or([
                        0.3 * fuse_l,
                        0.0,
                        if side == "L" { 0.4 } else { -0.4 },
                    ]);
                    motors.push((pos, i_lift));
                }
                motors.push((mp, i_prop));
                // Geçişte kaldırma ve itme motorları birlikte çalışır.
                bus_i = i_lift * 4.0 + i_prop;
            } else {
                motors.push((mp, i_prop));
                bus_i = i_prop;
            }
            let x = |f: f64| f * fuse_l;
            let esc_x = if pusher { 0.88 } else { 0.12 };
            base = [
                [x(0.30), 0.0, 0.0],
                [x(0.26), 0.0, 0.0],
                [x(0.45), 0.0, 0.0],
                [x(0.55), 0.0, 0.0],
                [x(0.60), 0.05, 0.0],
                [x(0.65), 0.0, 0.0],
                [x(0.70), 0.0, 0.0],
                [x(0.10), 0.0, 0.0],
                [x(0.50), 0.0, 0.03],
                [x(0.80), 0.0, 0.0],
                [x(0.92), 0.03, 0.0],
            ];
            // ESC konumu motorun yanında.
            n_servo_pos.push([x(esc_x), 0.0, 0.0]);
            // Servolar.
            let half = (p.b / 2.0).max(0.05);
            let wing_x =
                inst_pos(&lay, "Wing_L").map(|q| q[0]).unwrap_or(x(0.4)) + 0.4 * m.wing.c_r;
            let tail_x = |label: &str| inst_pos(&lay, label).map(|q| q[0]).unwrap_or(x(0.92));
            let mut sp: Vec<[f64; 3]> = vec![[wing_x, 0.0, 0.6 * half], [wing_x, 0.0, -0.6 * half]];
            if dt != DroneType::FlyingWing {
                match p.tail_kind {
                    TailKind::FlyingWing => {}
                    TailKind::VTail => {
                        let tx = tail_x("VTail_L");
                        sp.push([tx, 0.05, 0.08]);
                        sp.push([tx, 0.05, -0.08]);
                    }
                    TailKind::Conventional => {
                        sp.push([tail_x("TailH_L"), 0.0, 0.05]);
                        sp.push([tail_x("TailV"), 0.05, 0.0]);
                    }
                }
            }
            n_servo_pos.truncate(1);
            n_servo_pos.extend(sp);
        }

        let n_bat = add(NodeKind::Battery, None, base[0]);
        let n_pdb = add(NodeKind::Pdb, None, base[1]);
        let n_fc = add(NodeKind::Fc, None, base[2]);
        let n_rx = add(NodeKind::Rx, None, base[3]);
        let n_gps = add(NodeKind::Gps, None, base[4]);
        let n_tel = add(NodeKind::Telem, None, base[5]);
        let n_vtx = add(NodeKind::Vtx, None, base[6]);
        let n_cam = add(NodeKind::Camera, None, base[7]);
        let n_buz = add(NodeKind::Buzzer, None, base[8]);
        let n_led = add(NodeKind::Led, None, base[9]);
        let n_rxa = add(NodeKind::RxAnt, None, base[10]);
        let n_vta = add(
            NodeKind::VtxAnt,
            None,
            [base[10][0] - 0.02, base[10][1], base[10][2]],
        );

        let mut esc_nodes = Vec::new();
        let mut motor_nodes = Vec::new();
        for (k, (mp, _)) in motors.iter().enumerate() {
            let esc_pos = if mr {
                [mp[0] * 0.6, mp[1] * 0.6, mp[2]]
            } else if k == motors.len() - 1 {
                n_servo_pos[0]
            } else {
                [mp[0], mp[1], mp[2] - 0.03] // kaldırma motoru ESC'si motorun altında
            };
            esc_nodes.push(add(NodeKind::Esc, Some(k as u32), esc_pos));
            motor_nodes.push(add(NodeKind::Motor, Some(k as u32), *mp));
        }
        let mut servo_nodes = Vec::new();
        if !mr {
            for (k, sp) in n_servo_pos.iter().skip(1).enumerate() {
                servo_nodes.push(add(NodeKind::Servo, Some(k as u32), *sp));
            }
        }

        let mut wires: Vec<Wire> = Vec::new();
        let mut w = |kind: WireKind,
                     from: usize,
                     to: usize,
                     cond: u32,
                     lf: f64,
                     i: f64,
                     v: f64,
                     idx: Option<u32>,
                     phase: Option<u8>| {
            wires.push(Wire {
                kind,
                from,
                to,
                conductors: cond,
                loop_factor: lf,
                current_a: i,
                voltage_v: v,
                idx,
                phase,
                length_override_m: None,
            });
        };
        w(
            WireKind::BatteryMain,
            n_bat,
            n_pdb,
            2,
            2.0,
            bus_i,
            v_pack,
            None,
            None,
        );
        for (k, (_, ik)) in motors.iter().enumerate() {
            let kk = Some(k as u32);
            w(
                WireKind::EscPower,
                n_pdb,
                esc_nodes[k],
                2,
                2.0,
                *ik,
                v_pack,
                kk,
                None,
            );
            for ph in 0..3u8 {
                w(
                    WireKind::MotorPhase,
                    esc_nodes[k],
                    motor_nodes[k],
                    1,
                    2.0,
                    *ik,
                    v_pack,
                    kk,
                    Some(ph),
                );
            }
            w(
                WireKind::EscSignal,
                n_fc,
                esc_nodes[k],
                2,
                2.0,
                0.01,
                3.3,
                kk,
                None,
            );
        }
        // ESC telemetrisi: zincirin en uzak ESC'sine kadar tek hat.
        let fc_pos = base[2];
        let far = esc_nodes
            .iter()
            .copied()
            .max_by(|a, b| {
                manhattan(nodes[*a].pos, fc_pos).total_cmp(&manhattan(nodes[*b].pos, fc_pos))
            })
            .unwrap_or(n_fc);
        w(
            WireKind::EscTelemetry,
            far,
            n_fc,
            2,
            2.0,
            0.01,
            3.3,
            None,
            None,
        );
        // FC üzerinden beslenen 5 V yüklerin toplamı (FC kendisi 0.3 A).
        let i_fc = 0.3 + 0.2 + 0.15 + 0.5 + 0.03 + 0.3 + servo_nodes.len() as f64;
        w(
            WireKind::PdbToFc,
            n_pdb,
            n_fc,
            2,
            2.0,
            i_fc,
            5.0,
            None,
            None,
        );
        w(WireKind::Receiver, n_fc, n_rx, 4, 2.0, 0.2, 5.0, None, None);
        w(WireKind::Gps, n_fc, n_gps, 4, 2.0, 0.15, 5.0, None, None);
        w(
            WireKind::Compass,
            n_fc,
            n_gps,
            2,
            2.0,
            0.02,
            3.3,
            None,
            None,
        );
        w(
            WireKind::Telemetry,
            n_fc,
            n_tel,
            4,
            2.0,
            0.5,
            5.0,
            None,
            None,
        );
        w(
            WireKind::VtxControl,
            n_fc,
            n_vtx,
            1,
            2.0,
            0.01,
            3.3,
            None,
            None,
        );
        w(
            WireKind::VtxPower,
            n_pdb,
            n_vtx,
            2,
            2.0,
            1.0,
            5.0,
            None,
            None,
        );
        w(
            WireKind::Camera,
            n_cam,
            n_vtx,
            3,
            2.0,
            0.15,
            5.0,
            None,
            None,
        );
        for (k, sn) in servo_nodes.iter().enumerate() {
            w(
                WireKind::Servo,
                n_fc,
                *sn,
                3,
                2.0,
                1.0,
                5.0,
                Some(k as u32),
                None,
            );
        }
        w(WireKind::Buzzer, n_fc, n_buz, 2, 2.0, 0.03, 5.0, None, None);
        w(WireKind::Led, n_fc, n_led, 3, 2.0, 0.3, 5.0, None, None);
        w(
            WireKind::RxAntenna,
            n_rx,
            n_rxa,
            1,
            1.0,
            0.0,
            0.0,
            None,
            None,
        );
        w(
            WireKind::VtxAntenna,
            n_vtx,
            n_vta,
            1,
            1.0,
            0.0,
            0.0,
            None,
            None,
        );

        Harness {
            drone: dt,
            nodes,
            wires,
        }
    }

    pub fn count(&self, kind: WireKind) -> usize {
        self.wires.iter().filter(|w| w.kind == kind).count()
    }

    pub fn node_count(&self, kind: NodeKind) -> usize {
        self.nodes.iter().filter(|n| n.kind == kind).count()
    }

    pub fn wire_label(&self, w: &Wire) -> String {
        let mut s = t(w.kind.key()).to_string();
        if let Some(i) = w.idx {
            let _ = write!(s, " {}", i + 1);
        }
        if let Some(ph) = w.phase {
            let _ = write!(s, " {}", ['A', 'B', 'C'][(ph % 3) as usize]);
        }
        s
    }

    pub fn evaluate(&self, set: &Settings) -> Report {
        let mut rows = Vec::with_capacity(self.wires.len());
        let (mut mass, mut worst, mut worst_idx, mut over) = (0.0, 0.0, None, 0);
        for (k, w) in self.wires.iter().enumerate() {
            let a = self.nodes[w.from].pos;
            let b = self.nodes[w.to].pos;
            let straight = euclid(a, b);
            let route = (manhattan(a, b) * (1.0 + set.slack.max(0.0))).max(MIN_ROUTE_M);
            let len = w
                .length_override_m
                .filter(|l| l.is_finite() && *l > 0.0)
                .unwrap_or(route);
            let connector = connector_for(w.kind, w.current_a);
            let row = if w.func() == Func::Rf {
                Row {
                    awg: None,
                    straight_m: straight,
                    length_m: len,
                    drop_v: 0.0,
                    drop_pct: 0.0,
                    mass_g: len * COAX_G_PER_M,
                    connector,
                    over_limit: false,
                }
            } else {
                let min_thin = if w.func() == Func::Power { 22 } else { 28 };
                let (awg, ok) = select_awg(
                    w.current_a,
                    w.voltage_v,
                    len,
                    w.loop_factor,
                    set.drop_limit_pct,
                    min_thin,
                );
                let drop_v = voltage_drop(w.current_a, awg, len, w.loop_factor);
                let pct = if w.voltage_v > 0.0 {
                    drop_v / w.voltage_v * 100.0
                } else {
                    0.0
                };
                Row {
                    awg: Some(awg),
                    straight_m: straight,
                    length_m: len,
                    drop_v,
                    drop_pct: pct,
                    mass_g: w.conductors as f64 * len * mass_per_m(awg),
                    connector,
                    over_limit: !ok,
                }
            };
            mass += row.mass_g;
            if row.drop_pct > worst {
                worst = row.drop_pct;
                worst_idx = Some(k);
            }
            if row.over_limit {
                over += 1;
            }
            rows.push(row);
        }
        let max_of = |kind: WireKind| {
            self.wires
                .iter()
                .zip(&rows)
                .filter(|(w, _)| w.kind == kind)
                .map(|(_, r)| r.drop_pct)
                .fold(0.0, f64::max)
        };
        let chain = max_of(WireKind::BatteryMain)
            + max_of(WireKind::EscPower)
            + max_of(WireKind::MotorPhase);
        Report {
            rows,
            chain_drop_pct: chain,
            mass_g: mass,
            worst_drop_pct: worst,
            worst_idx,
            n_over_limit: over,
        }
    }

    /// Demet listesi CSV (geçerli dilde adlar).
    pub fn to_csv(&self, rep: &Report) -> String {
        let mut s = String::from(
            "no,name,from,to,function,conductors,current_a,voltage_v,length_m,awg,v_drop_v,v_drop_pct,mass_g,connector,note\n",
        );
        for (k, (w, r)) in self.wires.iter().zip(&rep.rows).enumerate() {
            let func = match w.func() {
                Func::Power => "power",
                Func::Signal => "signal",
                Func::Rf => "rf",
            };
            let note = if r.over_limit {
                "over limit"
            } else {
                "typical"
            };
            let _ = writeln!(
                s,
                "{},{},{},{},{},{},{:.2},{:.1},{:.3},{},{:.4},{:.2},{:.2},{},{}",
                k + 1,
                csv_escape(&self.wire_label(w)),
                csv_escape(&self.nodes[w.from].label()),
                csv_escape(&self.nodes[w.to].label()),
                func,
                w.conductors,
                w.current_a,
                w.voltage_v,
                r.length_m,
                r.awg
                    .map(|a| a.to_string())
                    .unwrap_or_else(|| "coax".into()),
                r.drop_v,
                r.drop_pct,
                r.mass_g,
                r.connector.label(),
                note
            );
        }
        s
    }
}

pub fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn euclid(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn manhattan(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs()
}

/// Blok şeması düğüm yerleşimi (0..1 normalize). Çıktı `nodes` ile aynı sıradadır.
pub fn block_layout(nodes: &[Node]) -> Vec<[f32; 2]> {
    let count = |k: NodeKind| nodes.iter().filter(|n| n.kind == k).count().max(1);
    let mut out = vec![[0.5f32, 0.5]; nodes.len()];
    let stack = |i: u32, n: usize| 0.08 + 0.50 * (i as f32 + 0.5) / n as f32;
    let mut per = 0usize;
    let n_esc = count(NodeKind::Esc);
    let n_mot = count(NodeKind::Motor);
    for (k, n) in nodes.iter().enumerate() {
        let i = n.idx.unwrap_or(0);
        out[k] = match n.kind {
            NodeKind::Battery => [0.08, 0.20],
            NodeKind::Pdb => [0.08, 0.45],
            NodeKind::Fc => [0.33, 0.33],
            NodeKind::Esc => [0.58, stack(i, n_esc)],
            NodeKind::Motor => [0.86, stack(i, n_mot)],
            _ => {
                per += 1;
                [0.0, 0.0]
            }
        };
    }
    // Çevre birimleri: alt iki satır.
    let total = per.max(1);
    let per_row = total.div_ceil(2);
    let mut c = 0usize;
    for (k, n) in nodes.iter().enumerate() {
        if matches!(
            n.kind,
            NodeKind::Battery | NodeKind::Pdb | NodeKind::Fc | NodeKind::Esc | NodeKind::Motor
        ) {
            continue;
        }
        let (row, col) = (c / per_row, c % per_row);
        out[k] = [
            0.07 + 0.86 * (col as f32 + 0.5) / per_row as f32,
            0.74 + 0.18 * row as f32,
        ];
        c += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    fn build(dt: DroneType) -> (Project, Model, Harness) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        let h = Harness::default_for(&p, &m);
        (p, m, h)
    }

    #[test]
    fn phase_wire_counts() {
        let (mut p, _, h) = build(DroneType::FixedWingConventional);
        assert_eq!(h.count(WireKind::MotorPhase), 3);
        assert_eq!(h.count(WireKind::Servo), 4);
        let (_, _, h) = build(DroneType::FlyingWing);
        assert_eq!(h.count(WireKind::Servo), 2);
        let (_, _, h) = build(DroneType::Multirotor);
        assert_eq!(
            h.count(WireKind::MotorPhase),
            3 * h.node_count(NodeKind::Motor)
        );
        assert_eq!(h.node_count(NodeKind::Motor), 4);
        assert_eq!(h.count(WireKind::MotorPhase), 12);
        let (_, _, h) = build(DroneType::VtolQuadplane);
        assert_eq!(h.count(WireKind::MotorPhase), 15);
        p.set_drone_type(DroneType::Multirotor);
        p.mr_arm_count = 6;
        let m = evaluate(&p, &Library::embedded());
        let h = Harness::default_for(&p, &m);
        assert_eq!(h.count(WireKind::MotorPhase), 18);
    }

    #[test]
    fn awg_gets_thicker_with_current() {
        let mut last = 99u8;
        for i in [0.1, 1.0, 5.0, 10.0, 20.0, 40.0, 80.0, 120.0] {
            let (awg, _) = select_awg(i, 14.8, 0.3, 2.0, 2.0, 22);
            assert!(awg <= last, "i={i} awg={awg} last={last}");
            last = awg;
        }
        assert!(!select_awg(500.0, 14.8, 0.3, 2.0, 2.0, 22).1);
    }

    #[test]
    fn voltage_drop_matches_hand_value() {
        // 12 AWG, 30 A, 0.5 m, gidiş-dönüş: ρ50 = 1.9273e-8, A = 3.3088 mm².
        let d = voltage_drop(30.0, 12, 0.5, 2.0);
        assert!((d - 0.1748).abs() < 0.002, "{d}");
    }

    #[test]
    fn connector_thresholds() {
        assert_eq!(power_connector(30.0), Connector::Xt30);
        assert_eq!(power_connector(30.1), Connector::Xt60);
        assert_eq!(power_connector(60.0), Connector::Xt60);
        assert_eq!(power_connector(90.0), Connector::Xt90);
        assert_eq!(power_connector(90.1), Connector::As150);
        // JST-GH yalnızca yaklaşık 1 A'e kadar.
        assert_eq!(connector_for(WireKind::VtxPower, 1.0), Connector::JstGh);
        assert_ne!(connector_for(WireKind::PdbToFc, 2.5), Connector::JstGh);
        assert_eq!(connector_for(WireKind::EscPower, 12.0), Connector::Xt30);
    }

    #[test]
    fn pdb_to_fc_carries_downstream_loads() {
        let (_, _, h) = build(DroneType::FixedWingConventional);
        let w = h
            .wires
            .iter()
            .find(|w| w.kind == WireKind::PdbToFc)
            .unwrap();
        let servos = h.count(WireKind::Servo) as f64;
        assert!(w.current_a >= servos + 1.0, "{}", w.current_a);
    }

    #[test]
    fn quadplane_bus_includes_forward_motor() {
        let (_, _, h) = build(DroneType::VtolQuadplane);
        let w = h
            .wires
            .iter()
            .find(|w| w.kind == WireKind::BatteryMain)
            .unwrap();
        let sum: f64 = h
            .wires
            .iter()
            .filter(|w| w.kind == WireKind::EscPower)
            .map(|w| w.current_a)
            .sum();
        assert!((w.current_a - sum).abs() < 1e-9);
    }

    #[test]
    fn chain_drop_accumulates_stages() {
        let (_, _, h) = build(DroneType::Multirotor);
        let rep = h.evaluate(&Settings::default());
        let stage = |k: WireKind| {
            h.wires
                .iter()
                .zip(&rep.rows)
                .filter(|(w, _)| w.kind == k)
                .map(|(_, r)| r.drop_pct)
                .fold(0.0, f64::max)
        };
        let sum =
            stage(WireKind::BatteryMain) + stage(WireKind::EscPower) + stage(WireKind::MotorPhase);
        assert!((rep.chain_drop_pct - sum).abs() < 1e-9);
        assert!(rep.chain_drop_pct >= rep.worst_drop_pct - 1e-9);
    }

    #[test]
    fn all_types_finite_and_routes_not_shorter_than_straight() {
        for dt in DroneType::ALL {
            let (_, _, h) = build(dt);
            let rep = h.evaluate(&Settings::default());
            assert_eq!(rep.rows.len(), h.wires.len());
            for r in &rep.rows {
                assert!(r.length_m.is_finite() && r.mass_g.is_finite());
                assert!(r.drop_v.is_finite() && r.drop_pct.is_finite());
                assert!(r.length_m + 1e-9 >= r.straight_m, "{dt:?}");
            }
            assert!(rep.mass_g.is_finite() && rep.mass_g > 0.0);
            assert!(rep.worst_drop_pct.is_finite());
            for p in block_layout(&h.nodes) {
                assert!((0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]));
            }
        }
    }

    #[test]
    fn csv_has_expected_columns() {
        let (_, _, h) = build(DroneType::Multirotor);
        let rep = h.evaluate(&Settings::default());
        let csv = h.to_csv(&rep);
        let mut lines = csv.lines();
        let head = lines.next().unwrap();
        assert_eq!(head.split(',').count(), 15);
        assert_eq!(csv.lines().count(), h.wires.len() + 1);
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }
}
