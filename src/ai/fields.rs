// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yapay zekaya açık parametre alanlarının tanımları, geçerli aralıkları ve getter/setter eşlemeleri.
//! Bu liste haricindeki alanlar AI tarafından değiştirilemez; aralık dışı değerler reddedilir.

use crate::i18n::tf;
use crate::project::{DroneType, Project};
use serde::{Deserialize, Serialize};

/// AI tarafından döndürülebilecek parametre değer türü.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AiValue {
    Num(f64),
    Str(String),
}

impl AiValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            AiValue::Num(n) => Some(*n),
            AiValue::Str(s) => s.parse::<f64>().ok(),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            AiValue::Str(s) => s.as_str(),
            AiValue::Num(_) => "",
        }
    }

    pub fn format_display(&self, unit: &str) -> String {
        match self {
            AiValue::Num(n) => {
                if unit == "-" || unit.is_empty() {
                    if (*n - n.round()).abs() < 1e-4 {
                        format!("{:.0}", n)
                    } else if *n < 1.0 {
                        format!("{:.3}", n)
                    } else {
                        format!("{:.2}", n)
                    }
                } else if (*n - n.round()).abs() < 1e-4 {
                    format!("{:.0} {}", n, unit)
                } else if *n < 1.0 {
                    format!("{:.3} {}", n, unit)
                } else {
                    format!("{:.2} {}", n, unit)
                }
            }
            AiValue::Str(s) => s.clone(),
        }
    }
}

/// AI'ya açık parametre tanımı.
pub struct AiFieldDef {
    pub id: &'static str,
    pub unit: &'static str,
    pub min: f64,
    pub max: f64,
    pub desc_key: &'static str,
    pub allowed_strings: &'static [&'static str],
    pub get: fn(&Project) -> AiValue,
    pub set: fn(&mut Project, &AiValue) -> Result<(), String>,
}

impl AiFieldDef {
    pub fn validate(&self, val: &AiValue) -> Result<(), String> {
        if !self.allowed_strings.is_empty() {
            let s = val.as_str();
            if self.allowed_strings.contains(&s) {
                Ok(())
            } else {
                Err(tf(
                    "ai.err.bad_value_fmt",
                    &[s.to_string(), format!("{:?}", self.allowed_strings)],
                ))
            }
        } else {
            let num = val
                .as_f64()
                .ok_or_else(|| tf("ai.err.expects_number_fmt", &[self.id]))?;
            if num < self.min || num > self.max {
                Err(tf(
                    "ai.err.out_of_range_fmt",
                    &[
                        num.to_string(),
                        self.min.to_string(),
                        self.max.to_string(),
                        self.unit.to_string(),
                    ],
                ))
            } else {
                Ok(())
            }
        }
    }
}

pub const AI_FIELDS: &[AiFieldDef] = &[
    AiFieldDef {
        id: "drone_type",
        unit: "-",
        min: 0.0,
        max: 0.0,
        desc_key: "ai.field.drone_type",
        allowed_strings: &[
            "fixed_wing_conv",
            "fixed_wing_vtail",
            "flying_wing",
            "multirotor",
            "vtol_quadplane",
        ],
        get: |p| {
            AiValue::Str(match p.drone_type {
                DroneType::FixedWingConventional => "fixed_wing_conv".into(),
                DroneType::FixedWingVTail => "fixed_wing_vtail".into(),
                DroneType::FlyingWing => "flying_wing".into(),
                DroneType::Multirotor => "multirotor".into(),
                DroneType::VtolQuadplane => "vtol_quadplane".into(),
            })
        },
        set: |p, val| {
            p.drone_type = match val.as_str() {
                "fixed_wing_conv" => DroneType::FixedWingConventional,
                "fixed_wing_vtail" => DroneType::FixedWingVTail,
                "flying_wing" => DroneType::FlyingWing,
                "multirotor" => DroneType::Multirotor,
                "vtol_quadplane" => DroneType::VtolQuadplane,
                other => return Err(tf("ai.err.unknown_drone_type_fmt", &[other])),
            };
            Ok(())
        },
    },
    AiFieldDef {
        id: "h",
        unit: "m",
        min: 0.0,
        max: 8000.0,
        desc_key: "ai.field.altitude",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.h),
        set: |p, v| {
            p.h = v.as_f64().ok_or("h must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "v_cr",
        unit: "m/s",
        min: 5.0,
        max: 80.0,
        desc_key: "ai.field.v_cr",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.v_cr),
        set: |p, v| {
            p.v_cr = v.as_f64().ok_or("v_cr must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "m_payload",
        unit: "kg",
        min: 0.0,
        max: 25.0,
        desc_key: "ai.field.m_payload",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.m_payload),
        set: |p, v| {
            p.m_payload = v.as_f64().ok_or("m_payload must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "m_avionics",
        unit: "kg",
        min: 0.02,
        max: 5.0,
        desc_key: "ai.field.m_avionics",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.m_avionics),
        set: |p, v| {
            p.m_avionics = v.as_f64().ok_or("m_avionics must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "m_propulsion",
        unit: "kg",
        min: 0.02,
        max: 10.0,
        desc_key: "ai.field.m_propulsion",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.m_propulsion),
        set: |p, v| {
            p.m_propulsion = v.as_f64().ok_or("m_propulsion must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "m_fuse_tail",
        unit: "kg",
        min: 0.05,
        max: 20.0,
        desc_key: "ai.field.m_fuse_tail",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.m_fuse_tail),
        set: |p, v| {
            p.m_fuse_tail = v.as_f64().ok_or("m_fuse_tail must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "b",
        unit: "m",
        min: 0.4,
        max: 6.0,
        desc_key: "ai.field.b",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.b),
        set: |p, v| {
            p.b = v.as_f64().ok_or("b must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "s",
        unit: "m²",
        min: 0.05,
        max: 5.0,
        desc_key: "ai.field.s",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.s),
        set: |p, v| {
            p.s = v.as_f64().ok_or("s must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "taper",
        unit: "-",
        min: 0.1,
        max: 1.0,
        desc_key: "ai.field.taper",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.taper),
        set: |p, v| {
            p.taper = v.as_f64().ok_or("taper must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "sweep_le_deg",
        unit: "deg",
        min: 0.0,
        max: 45.0,
        desc_key: "ai.field.sweep_le_deg",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.sweep_le_deg),
        set: |p, v| {
            p.sweep_le_deg = v.as_f64().ok_or("sweep_le_deg must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "tc_w",
        unit: "-",
        min: 0.06,
        max: 0.22,
        desc_key: "ai.field.tc_w",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.tc_w),
        set: |p, v| {
            p.tc_w = v.as_f64().ok_or("tc_w must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "n_series",
        unit: "S",
        min: 1.0,
        max: 14.0,
        desc_key: "ai.field.n_series",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.n_series as f64),
        set: |p, v| {
            let n = v.as_f64().ok_or("n_series must be number")?;
            p.n_series = n.round() as u32;
            Ok(())
        },
    },
    AiFieldDef {
        id: "n_parallel",
        unit: "P",
        min: 1.0,
        max: 10.0,
        desc_key: "ai.field.n_parallel",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.n_parallel as f64),
        set: |p, v| {
            let n = v.as_f64().ok_or("n_parallel must be number")?;
            p.n_parallel = n.round() as u32;
            Ok(())
        },
    },
    AiFieldDef {
        id: "v_stall_req",
        unit: "m/s",
        min: 6.0,
        max: 35.0,
        desc_key: "ai.field.v_stall_req",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.v_stall_req),
        set: |p, v| {
            p.v_stall_req = v.as_f64().ok_or("v_stall_req must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "rc",
        unit: "m/s",
        min: 0.5,
        max: 25.0,
        desc_key: "ai.field.rc",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.rc),
        set: |p, v| {
            p.rc = v.as_f64().ok_or("rc must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "dod",
        unit: "-",
        min: 0.50,
        max: 0.95,
        desc_key: "ai.field.dod",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.dod),
        set: |p, v| {
            p.dod = v.as_f64().ok_or("dod must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "p_max_elec",
        unit: "W",
        min: 20.0,
        max: 5000.0,
        desc_key: "ai.field.p_max_elec",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.p_max_elec),
        set: |p, v| {
            p.p_max_elec = v.as_f64().ok_or("p_max_elec must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "d_prop",
        unit: "m",
        min: 0.08,
        max: 1.0,
        desc_key: "ai.field.d_prop",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.d_prop),
        set: |p, v| {
            p.d_prop = v.as_f64().ok_or("d_prop must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "prop_pitch",
        unit: "m",
        min: 0.05,
        max: 0.8,
        desc_key: "ai.field.prop_pitch",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.prop_pitch),
        set: |p, v| {
            p.prop_pitch = v.as_f64().ok_or("prop_pitch must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "mr_frame_size",
        unit: "m",
        min: 0.10,
        max: 1.50,
        desc_key: "ai.field.mr_frame_size",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.mr_frame_size),
        set: |p, v| {
            p.mr_frame_size = v.as_f64().ok_or("mr_frame_size must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "mr_arm_count",
        unit: "-",
        min: 3.0,
        max: 8.0,
        desc_key: "ai.field.mr_arm_count",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.mr_arm_count as f64),
        set: |p, v| {
            let n = v.as_f64().ok_or("mr_arm_count must be number")?;
            p.mr_arm_count = n.round() as u32;
            Ok(())
        },
    },
    AiFieldDef {
        id: "mr_d_prop",
        unit: "m",
        min: 0.05,
        max: 0.80,
        desc_key: "ai.field.mr_d_prop",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.mr_d_prop),
        set: |p, v| {
            p.mr_d_prop = v.as_f64().ok_or("mr_d_prop must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "mr_t_max_motor",
        unit: "N",
        min: 1.0,
        max: 150.0,
        desc_key: "ai.field.mr_t_max_motor",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.mr_t_max_motor),
        set: |p, v| {
            p.mr_t_max_motor = v.as_f64().ok_or("mr_t_max_motor must be number")?;
            Ok(())
        },
    },
    AiFieldDef {
        id: "mr_m_motor",
        unit: "kg",
        min: 0.01,
        max: 1.50,
        desc_key: "ai.field.mr_m_motor",
        allowed_strings: &[],
        get: |p| AiValue::Num(p.mr_m_motor),
        set: |p, v| {
            p.mr_m_motor = v.as_f64().ok_or("mr_m_motor must be number")?;
            Ok(())
        },
    },
];

pub fn find_field(id: &str) -> Option<&'static AiFieldDef> {
    AI_FIELDS.iter().find(|f| f.id == id)
}

/// AI sistem istemine gömülecek şema metnini üretir.
pub fn generate_schema_prompt(project: &Project) -> String {
    let mut out = String::from(
        "### İzin Verilen Alanlar ve Mevcut Değerler (Allowed Fields & Current Values):\n",
    );
    for f in AI_FIELDS {
        let cur = (f.get)(project);
        let cur_str = cur.format_display(f.unit);
        let desc = crate::i18n::t(f.desc_key);
        if !f.allowed_strings.is_empty() {
            out.push_str(&format!(
                "- `{}`: {} (Mevcut: '{}', İzin verilen seçenekler: {:?})\n",
                f.id, desc, cur_str, f.allowed_strings
            ));
        } else {
            out.push_str(&format!(
                "- `{}`: {} [Birim: {}, Aralık: {} .. {}] (Mevcut: {})\n",
                f.id, desc, f.unit, f.min, f.max, cur_str
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_lookup_and_validation() {
        let f_b = find_field("b").expect("b field should exist");
        assert_eq!(f_b.unit, "m");

        // Valid
        assert!(f_b.validate(&AiValue::Num(2.0)).is_ok());

        // Below min
        assert!(f_b.validate(&AiValue::Num(0.1)).is_err());

        // Above max
        assert!(f_b.validate(&AiValue::Num(10.0)).is_err());

        // Unknown field
        assert!(find_field("non_existent_field").is_none());
    }

    #[test]
    fn test_drone_type_validation() {
        let f_dt = find_field("drone_type").expect("drone_type should exist");
        assert!(
            f_dt.validate(&AiValue::Str("fixed_wing_conv".into()))
                .is_ok()
        );
        assert!(f_dt.validate(&AiValue::Str("multirotor".into())).is_ok());
        assert!(
            f_dt.validate(&AiValue::Str("space_shuttle".into()))
                .is_err()
        );
    }
}
