// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ortam koşulları ve görev enerji bütçesi (ön tasarım düzeyi). Saf hesap: arayüz yok.
//! Atmosfer `calc::atmosphere` ISA modelinden, güç `calc::multirotor` / `calc::performance`
//! modellerinden gelir; burada yalnızca ortam düzeltmeleri ve bacak bacak enerji toplamı vardır.
pub mod env;
pub mod legs;
pub mod vehicle;

pub use env::{BatteryDerate, Env, TempMode, battery_derate, density_altitude_m};
pub use legs::{Leg, LegKind, LegResult, MissionResult, default_legs, simulate};
pub use vehicle::{Kind, Vehicle};
