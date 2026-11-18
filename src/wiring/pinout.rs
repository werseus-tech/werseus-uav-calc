// SPDX-License-Identifier: AGPL-3.0-or-later
//! Genel (örnek) uçuş kontrolcüsü pin atama planlayıcısı. Kart önayarları gerçek bir kartın
//! pinout'u değildir; kendi kartınızın belgesine bakın.

use super::harness::{Harness, WireKind};
use crate::i18n::t;
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PresetId {
    F4,
    F7,
    H7,
}

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub id: PresetId,
    pub name: &'static str,
    pub uarts: usize,
    /// Toplam PWM/DShot çıkışı ve ilk kaçının motor (DShot) yapabildiği.
    pub outputs: usize,
    pub motor_outputs: usize,
    pub i2c: usize,
    pub adc: usize,
    /// Zamanlayıcı grubu büyüklüğü (aynı gruptaki çıkışlar aynı protokol hızını paylaşır).
    pub group_size: usize,
}

pub const PRESETS: [Preset; 3] = [
    Preset {
        id: PresetId::F4,
        name: "F4 (4 UART)",
        uarts: 4,
        outputs: 6,
        motor_outputs: 4,
        i2c: 1,
        adc: 3,
        group_size: 4,
    },
    Preset {
        id: PresetId::F7,
        name: "F7 (6 UART)",
        uarts: 6,
        outputs: 10,
        motor_outputs: 8,
        i2c: 2,
        adc: 3,
        group_size: 4,
    },
    Preset {
        id: PresetId::H7,
        name: "H7 (8 UART)",
        uarts: 8,
        outputs: 14,
        motor_outputs: 10,
        i2c: 2,
        adc: 3,
        group_size: 4,
    },
];

pub fn preset(id: PresetId) -> Preset {
    PRESETS
        .iter()
        .copied()
        .find(|p| p.id == id)
        .unwrap_or(PRESETS[1])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReqKind {
    Motor,
    Servo,
    Uart,
    I2c,
    Adc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pool {
    Uart,
    Output,
    I2c,
    Adc,
}

impl ReqKind {
    fn pool(self) -> Pool {
        match self {
            ReqKind::Motor | ReqKind::Servo => Pool::Output,
            ReqKind::Uart => Pool::Uart,
            ReqKind::I2c => Pool::I2c,
            ReqKind::Adc => Pool::Adc,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Req {
    pub id: String,
    pub key: &'static str,
    pub idx: Option<u32>,
    pub kind: ReqKind,
    /// Mantık/sinyal seviyesi için i18n anahtarı.
    pub level_key: &'static str,
}

impl Req {
    pub fn label(&self) -> String {
        match self.idx {
            Some(i) => format!("{} {}", t(self.key), i + 1),
            None => t(self.key).to_string(),
        }
    }
}

fn req(
    id: &str,
    key: &'static str,
    idx: Option<u32>,
    kind: ReqKind,
    level_key: &'static str,
) -> Req {
    Req {
        id: match idx {
            Some(i) => format!("{id}{}", i + 1),
            None => id.to_string(),
        },
        key,
        idx,
        kind,
        level_key,
    }
}

/// Demetteki araca göre ihtiyaç listesi (motor, servo, UART, I²C, ADC).
pub fn requirements(h: &Harness) -> Vec<Req> {
    let mut v = Vec::new();
    for w in h.wires.iter().filter(|w| w.kind == WireKind::EscSignal) {
        v.push(req(
            "motor",
            "wiring.req.motor",
            w.idx,
            ReqKind::Motor,
            "wiring.level.logic33",
        ));
    }
    for w in h.wires.iter().filter(|w| w.kind == WireKind::Servo) {
        v.push(req(
            "servo",
            "wiring.req.servo",
            w.idx,
            ReqKind::Servo,
            "wiring.level.logic33",
        ));
    }
    let has = |k: WireKind| h.count(k) > 0;
    let u = |id: &str, key: &'static str| req(id, key, None, ReqKind::Uart, "wiring.level.logic33");
    if has(WireKind::Receiver) {
        v.push(u("uart_rx", "wiring.req.uart_rx"));
    }
    if has(WireKind::Gps) {
        v.push(u("uart_gps", "wiring.req.uart_gps"));
    }
    if has(WireKind::Telemetry) {
        v.push(u("uart_telem", "wiring.req.uart_telem"));
    }
    if has(WireKind::EscTelemetry) {
        v.push(u("uart_esc", "wiring.req.uart_esc"));
    }
    if has(WireKind::VtxControl) {
        v.push(u("uart_vtx", "wiring.req.uart_vtx"));
    }
    if has(WireKind::Compass) {
        v.push(req(
            "i2c_compass",
            "wiring.req.i2c_compass",
            None,
            ReqKind::I2c,
            "wiring.level.logic33",
        ));
    }
    v.push(req(
        "adc_vbat",
        "wiring.req.adc_vbat",
        None,
        ReqKind::Adc,
        "wiring.level.adc",
    ));
    v.push(req(
        "adc_curr",
        "wiring.req.adc_curr",
        None,
        ReqKind::Adc,
        "wiring.level.adc",
    ));
    if h.node_count(super::harness::NodeKind::Servo) > 0 {
        v.push(req(
            "adc_airspeed",
            "wiring.req.adc_airspeed",
            None,
            ReqKind::Adc,
            "wiring.level.adc",
        ));
    }
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conflict {
    /// Havuz tükendi: ihtiyaç > mevcut.
    Exhausted {
        kind: ReqKind,
        need: usize,
        have: usize,
    },
    /// Motor sayısı motor çıkışlarını aşıyor.
    MotorOutputs { need: usize, have: usize },
    /// Aynı yuva birden fazla işleve verilmiş.
    Duplicate { slot: String },
    /// Motor, DShot/motor yapamayan çıkışa atanmış.
    MotorNotCapable { req: String },
    /// İhtiyaca hiç yuva atanamadı.
    Unassigned { req: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    /// Aynı zamanlayıcı grubunda hem motor hem servo var (DShot hızı servo ile paylaşılır).
    DshotGroupShared { group: usize },
}

#[derive(Clone, Debug)]
pub struct PinRow {
    pub slot: Option<usize>,
    pub conflict: bool,
}

#[derive(Clone, Debug)]
pub struct Pinout {
    pub rows: Vec<PinRow>,
    pub conflicts: Vec<Conflict>,
    pub warnings: Vec<Warning>,
}

pub fn pool_size(pre: &Preset, kind: ReqKind) -> usize {
    match kind.pool() {
        Pool::Uart => pre.uarts,
        Pool::Output => pre.outputs,
        Pool::I2c => pre.i2c,
        Pool::Adc => pre.adc,
    }
}

/// Yuva adı (genel örnek): UART2, OUT3, I2C1, ADC1.
pub fn slot_name(kind: ReqKind, slot: usize) -> String {
    let n = slot + 1;
    match kind.pool() {
        Pool::Uart => format!("UART{n} (TX{n}/RX{n})"),
        Pool::Output => format!("OUT{n}"),
        Pool::I2c => format!("I2C{n} (SDA{n}/SCL{n})"),
        Pool::Adc => format!("ADC{n}"),
    }
}

/// Atama: önce kullanıcı seçimleri (`overrides`: ihtiyaç kimliği → yuva), kalanlar otomatik.
pub fn assign(pre: &Preset, reqs: &[Req], overrides: &HashMap<String, usize>) -> Pinout {
    let mut slots: Vec<Option<usize>> = vec![None; reqs.len()];
    let mut used: HashMap<(Pool, usize), usize> = HashMap::new();
    for (k, r) in reqs.iter().enumerate() {
        if let Some(&s) = overrides.get(&r.id) {
            if s < pool_size(pre, r.kind) {
                slots[k] = Some(s);
                *used.entry((r.kind.pool(), s)).or_default() += 1;
            }
        }
    }
    // Otomatik: motorlar önce, sonra diğerleri sırayla.
    let order: Vec<usize> = (0..reqs.len())
        .filter(|&k| reqs[k].kind == ReqKind::Motor)
        .chain((0..reqs.len()).filter(|&k| reqs[k].kind != ReqKind::Motor))
        .collect();
    for k in order {
        if slots[k].is_some() {
            continue;
        }
        let r = &reqs[k];
        let limit = if r.kind == ReqKind::Motor {
            pre.motor_outputs
        } else {
            pool_size(pre, r.kind)
        };
        if let Some(s) = (0..limit).find(|s| !used.contains_key(&(r.kind.pool(), *s))) {
            slots[k] = Some(s);
            *used.entry((r.kind.pool(), s)).or_default() += 1;
        }
    }

    let mut conflicts = Vec::new();
    let mut flagged = vec![false; reqs.len()];
    let count = |kinds: &[ReqKind]| reqs.iter().filter(|r| kinds.contains(&r.kind)).count();
    let n_motor = count(&[ReqKind::Motor]);
    if n_motor > pre.motor_outputs {
        conflicts.push(Conflict::MotorOutputs {
            need: n_motor,
            have: pre.motor_outputs,
        });
    }
    for (kinds, kind) in [
        (&[ReqKind::Uart][..], ReqKind::Uart),
        (&[ReqKind::Motor, ReqKind::Servo][..], ReqKind::Motor),
        (&[ReqKind::I2c][..], ReqKind::I2c),
        (&[ReqKind::Adc][..], ReqKind::Adc),
    ] {
        let need = count(kinds);
        let have = pool_size(pre, kind);
        if need > have {
            conflicts.push(Conflict::Exhausted { kind, need, have });
        }
    }
    for (k, r) in reqs.iter().enumerate() {
        match slots[k] {
            None => {
                flagged[k] = true;
                conflicts.push(Conflict::Unassigned { req: r.label() });
            }
            Some(s) => {
                if used.get(&(r.kind.pool(), s)).copied().unwrap_or(0) > 1 {
                    flagged[k] = true;
                    let c = Conflict::Duplicate {
                        slot: slot_name(r.kind, s),
                    };
                    if !conflicts.contains(&c) {
                        conflicts.push(c);
                    }
                }
                if r.kind == ReqKind::Motor && s >= pre.motor_outputs {
                    flagged[k] = true;
                    conflicts.push(Conflict::MotorNotCapable { req: r.label() });
                }
            }
        }
    }

    let mut warnings = Vec::new();
    let gs = pre.group_size.max(1);
    for g in 0..pre.outputs.div_ceil(gs) {
        let (mut mot, mut srv) = (false, false);
        for (k, r) in reqs.iter().enumerate() {
            if let Some(s) = slots[k] {
                if s / gs == g {
                    mot |= r.kind == ReqKind::Motor;
                    srv |= r.kind == ReqKind::Servo;
                }
            }
        }
        if mot && srv {
            warnings.push(Warning::DshotGroupShared { group: g + 1 });
        }
    }

    Pinout {
        rows: slots
            .into_iter()
            .zip(flagged)
            .map(|(slot, conflict)| PinRow { slot, conflict })
            .collect(),
        conflicts,
        warnings,
    }
}

/// Yazdırılabilir pinout tablosu (Markdown).
pub fn to_markdown(pre: &Preset, reqs: &[Req], out: &Pinout) -> String {
    let mut s = format!(
        "# {} - {}\n\n_{}_\n\n",
        t("wiring.pinout.title"),
        pre.name,
        t("wiring.pinout.generic")
    );
    let _ = writeln!(
        s,
        "| {} | {} | {} | {} |\n|---|---|---|---|",
        t("wiring.col.function"),
        t("wiring.col.pin"),
        t("wiring.col.level"),
        t("wiring.col.status")
    );
    for (r, row) in reqs.iter().zip(&out.rows) {
        let pin = row
            .slot
            .map(|x| slot_name(r.kind, x))
            .unwrap_or_else(|| "-".into());
        let st = if row.conflict {
            t("wiring.status.conflict")
        } else {
            t("wiring.status.ok")
        };
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} |",
            r.label(),
            pin,
            t(r.level_key),
            st
        );
    }
    s
}

pub fn to_csv(pre: &Preset, reqs: &[Req], out: &Pinout) -> String {
    let mut s = String::from("preset,function,pin,level,status\n");
    for (r, row) in reqs.iter().zip(&out.rows) {
        let pin = row.slot.map(|x| slot_name(r.kind, x)).unwrap_or_default();
        let _ = writeln!(
            s,
            "{},{},{},{},{}",
            super::harness::csv_escape(pre.name),
            super::harness::csv_escape(&r.label()),
            super::harness::csv_escape(&pin),
            super::harness::csv_escape(t(r.level_key)),
            if row.conflict { "conflict" } else { "ok" }
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::project::{DroneType, Project};

    fn harness(dt: DroneType) -> Harness {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let m = evaluate(&p, &Library::embedded());
        Harness::default_for(&p, &m)
    }

    #[test]
    fn quad_on_f7_has_no_conflict() {
        let reqs = requirements(&harness(DroneType::Multirotor));
        let out = assign(&preset(PresetId::F7), &reqs, &HashMap::new());
        assert!(out.conflicts.is_empty(), "{:?}", out.conflicts);
        assert!(out.rows.iter().all(|r| r.slot.is_some() && !r.conflict));
    }

    #[test]
    fn runs_out_of_uarts_on_f4() {
        let reqs = requirements(&harness(DroneType::Multirotor));
        let out = assign(&preset(PresetId::F4), &reqs, &HashMap::new());
        assert!(out.conflicts.iter().any(|c| matches!(
            c,
            Conflict::Exhausted {
                kind: ReqKind::Uart,
                ..
            }
        )));
    }

    #[test]
    fn user_reassign_creates_duplicate_and_motor_overflow() {
        let reqs = requirements(&harness(DroneType::Multirotor));
        let mut ov = HashMap::new();
        ov.insert("uart_gps".to_string(), 0usize);
        ov.insert("uart_rx".to_string(), 0usize);
        let out = assign(&preset(PresetId::F7), &reqs, &ov);
        assert!(
            out.conflicts
                .iter()
                .any(|c| matches!(c, Conflict::Duplicate { .. }))
        );
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        p.mr_arm_count = 8;
        let m = evaluate(&p, &Library::embedded());
        let h = Harness::default_for(&p, &m);
        let out = assign(&preset(PresetId::F4), &requirements(&h), &HashMap::new());
        assert!(
            out.conflicts
                .iter()
                .any(|c| matches!(c, Conflict::MotorOutputs { .. }))
        );
    }

    #[test]
    fn servos_filling_motor_outputs_leave_motor_unassigned() {
        let h = harness(DroneType::VtolQuadplane);
        let reqs = requirements(&h);
        let pre = preset(PresetId::F7);
        let mut ov = HashMap::new();
        for (n, r) in reqs.iter().filter(|r| r.kind == ReqKind::Servo).enumerate() {
            ov.insert(r.id.clone(), n);
        }
        let out = assign(&pre, &reqs, &ov);
        let unassigned = out.rows.iter().filter(|r| r.slot.is_none()).count();
        let flagged = out
            .conflicts
            .iter()
            .filter(|c| matches!(c, Conflict::Unassigned { .. }))
            .count();
        assert!(unassigned > 0, "senaryo atanamayan satir uretmedi");
        assert_eq!(unassigned, flagged);
        assert!(
            out.rows
                .iter()
                .filter(|r| r.slot.is_none())
                .all(|r| r.conflict)
        );
    }

    #[test]
    fn motor_on_non_motor_output_is_flagged() {
        let reqs = requirements(&harness(DroneType::Multirotor));
        let pre = preset(PresetId::F7);
        assert!(pre.outputs > pre.motor_outputs);
        let motor = reqs.iter().find(|r| r.kind == ReqKind::Motor).unwrap();
        let mut ov = HashMap::new();
        ov.insert(motor.id.clone(), pre.outputs - 1);
        let out = assign(&pre, &reqs, &ov);
        assert!(
            out.conflicts
                .iter()
                .any(|c| matches!(c, Conflict::MotorNotCapable { .. }))
        );
    }

    #[test]
    fn dshot_group_warning_and_exports() {
        let h = harness(DroneType::VtolQuadplane);
        let reqs = requirements(&h);
        let pre = preset(PresetId::F7);
        let out = assign(&pre, &reqs, &HashMap::new());
        assert!(
            out.warnings
                .iter()
                .any(|w| matches!(w, Warning::DshotGroupShared { .. }))
        );
        assert!(to_markdown(&pre, &reqs, &out).contains('|'));
        assert_eq!(to_csv(&pre, &reqs, &out).lines().count(), reqs.len() + 1);
    }

    #[test]
    fn every_type_assigns_on_h7() {
        for dt in DroneType::ALL {
            let reqs = requirements(&harness(dt));
            let out = assign(&preset(PresetId::H7), &reqs, &HashMap::new());
            assert!(out.conflicts.is_empty(), "{dt:?} {:?}", out.conflicts);
        }
    }
}
