// SPDX-License-Identifier: AGPL-3.0-or-later
//! Üretim çıktıları: maliyetli malzeme listesi (BOM) ve DXF yerleşimi (nesting).
//!
//! Kütleler yeniden hesaplanmaz: BOM satırları `calc::Model` ve proje girdilerinden okunur
//! (kütle bütçesiyle aynı sayılar); düz parça ana hatları `cad::layout::Layout` parametrelerinden gelir.

pub mod cost;
pub mod dxf;
pub mod nest;
pub mod outlines;

#[cfg(test)]
mod tests;

use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::{DroneType, Project};
use cost::BomLine;

/// Tahmini bağlantı elemanı (M3) adedi: motor yuvaları, kol/plaka ve genel montaj.
pub fn fastener_count(p: &Project, m: &Model) -> u32 {
    match p.drone_type {
        DroneType::Multirotor => m.mr.n_motors * 4 + p.mr_arm_count * 2 + 4,
        DroneType::VtolQuadplane => 12 + 4 * 4 + 8,
        _ => 12,
    }
}

/// Bir bileşen satırı; kütle sıfır, negatif ya da sonlu değilse satır eklenmez.
fn push(out: &mut Vec<BomLine>, key: &'static str, name: String, qty: u32, unit_kg: f64) {
    let g = unit_kg * 1000.0;
    if qty > 0 && g.is_finite() && g > 0.0 {
        out.push(BomLine {
            key,
            name,
            qty,
            unit_mass_g: g,
        });
    }
}

/// Tasarımın tüm bileşen ve yapı parçaları. Bağlantı elemanları kütle bütçesinin dışındadır
/// (modelin MTOW'una dahil değildir); diğer satırların toplamı modelin MTOW'una eşittir.
pub fn build_lines(p: &Project, m: &Model) -> Vec<BomLine> {
    let mut v = Vec::new();
    let w = &m.weight;
    let n_cells = w.n_cells;
    let cells_kg = n_cells as f64 * m.cell.mass_g / 1000.0;
    push(
        &mut v,
        "cells",
        tf("bom.line.cells_fmt", &[m.cell.name.as_str()]),
        n_cells,
        m.cell.mass_g / 1000.0,
    );
    push(
        &mut v,
        "pack_extra",
        t("bom.line.pack_extra").to_string(),
        1,
        (w.m_battery - cells_kg).max(0.0),
    );
    let n = |key: &'static str| t(key).to_string();
    if p.drone_type == DroneType::Multirotor {
        push(&mut v, "mr_frame", n("bom.line.mr_frame"), 1, p.mr_m_frame);
        push(
            &mut v,
            "mr_motor",
            n("bom.line.mr_motor"),
            m.mr.n_motors,
            p.mr_m_motor,
        );
        push(&mut v, "mr_esc", n("bom.line.mr_esc"), 1, p.mr_m_esc_tot);
        push(&mut v, "mr_fc", n("bom.line.mr_fc"), 1, p.mr_m_fc);
        push(&mut v, "mr_misc", n("bom.line.mr_misc"), 1, p.mr_m_misc);
        push(&mut v, "payload", n("bom.line.payload"), 1, p.m_payload);
    } else {
        push(
            &mut v,
            "propulsion",
            n("bom.line.propulsion"),
            1,
            p.m_propulsion,
        );
        push(&mut v, "avionics", n("bom.line.avionics"), 1, p.m_avionics);
        push(&mut v, "payload", n("bom.line.payload"), 1, p.m_payload);
        push(&mut v, "wing_shell", n("bom.line.wing_shell"), 1, w.m_shell);
        push(&mut v, "spar", n("bom.line.spar"), 1, w.m_spar);
        push(
            &mut v,
            "fuse_tail",
            n("bom.line.fuse_tail"),
            1,
            p.m_fuse_tail,
        );
    }
    v.push(BomLine {
        key: "fasteners",
        name: n("bom.line.fasteners"),
        qty: fastener_count(p, m),
        unit_mass_g: 0.4,
    });
    v
}

/// Kütle bütçesine giren satırların toplamı [g] (bağlantı elemanları hariç).
pub fn budget_mass_g(lines: &[BomLine]) -> f64 {
    lines
        .iter()
        .filter(|l| l.key != "fasteners")
        .map(|l| l.total_mass_g())
        .sum()
}

/// Modelin MTOW'u [g] (türe göre).
pub fn model_mtow_g(p: &Project, m: &Model) -> f64 {
    let kg = if p.drone_type == DroneType::Multirotor {
        m.mr.m_mtow
    } else {
        m.weight.m_mtow
    };
    kg * 1000.0
}

/// Levha malzemesi (kesim parçası kütlesi için yoğunluk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetMaterial {
    CarbonPlate,
    G10,
    Plywood,
    Aluminium,
}

impl SheetMaterial {
    pub const ALL: [SheetMaterial; 4] = [
        SheetMaterial::CarbonPlate,
        SheetMaterial::G10,
        SheetMaterial::Plywood,
        SheetMaterial::Aluminium,
    ];

    /// Tipik yoğunluk [kg/m3].
    pub fn density_kg_m3(self) -> f64 {
        match self {
            SheetMaterial::CarbonPlate => 1600.0,
            SheetMaterial::G10 => 1850.0,
            SheetMaterial::Plywood => 680.0,
            SheetMaterial::Aluminium => 2700.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SheetMaterial::CarbonPlate => t("bom.material.carbon"),
            SheetMaterial::G10 => t("bom.material.g10"),
            SheetMaterial::Plywood => t("bom.material.plywood"),
            SheetMaterial::Aluminium => t("bom.material.aluminium"),
        }
    }
}
