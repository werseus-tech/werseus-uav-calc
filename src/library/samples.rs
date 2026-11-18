// SPDX-License-Identifier: AGPL-3.0-or-later
//! The small built-in example list. These are generic, made-up parts with typical values for
//! their size class. They are NOT products from any vendor and must be checked against the
//! datasheet of the real part before use.

use super::records::{Battery, Chemistry, Esc, Motor, Propeller, Source};
use super::standards::motor_mount;
use super::store::ComponentLibrary;

const NOTE: &str = "Typical example, check the datasheet.";

#[allow(clippy::too_many_arguments)]
fn motor(
    name: &str,
    kv: f64,
    r: f64,
    i0: f64,
    imax: f64,
    pmax: f64,
    mass: f64,
    mount: &str,
) -> Motor {
    Motor {
        name: name.into(),
        kv,
        r_internal_ohm: r,
        i_noload_a: i0,
        i_noload_v: 10.0,
        max_current_a: imax,
        max_power_w: pmax,
        mass_g: mass,
        mount: Some(motor_mount(mount).expect("standard mount name")),
        notes: NOTE.into(),
        source: Source::Example,
    }
}

fn prop(name: &str, d: f64, p: f64, blades: u32, mass: f64) -> Propeller {
    Propeller {
        name: name.into(),
        diameter_in: d,
        pitch_in: p,
        blades,
        mass_g: mass,
        ct: None,
        cp: None,
        notes: NOTE.into(),
        source: Source::Example,
    }
}

fn esc(name: &str, cont: f64, burst: f64, cmin: u32, cmax: u32, mass: f64) -> Esc {
    Esc {
        name: name.into(),
        continuous_a: cont,
        burst_a: Some(burst),
        cells_min: cmin,
        cells_max: cmax,
        mass_g: mass,
        notes: NOTE.into(),
        source: Source::Example,
    }
}

#[allow(clippy::too_many_arguments)]
fn battery(
    name: &str,
    chem: Chemistry,
    s: u32,
    p: u32,
    mah: f64,
    c: f64,
    r_mohm: f64,
    mass: f64,
) -> Battery {
    Battery {
        name: name.into(),
        chemistry: chem,
        cells_s: s,
        parallel_p: p,
        capacity_mah: mah,
        c_rating: c,
        r_internal_mohm: Some(r_mohm),
        mass_g: mass,
        notes: NOTE.into(),
        source: Source::Example,
    }
}

/// The example list (five records per category).
pub fn examples() -> ComponentLibrary {
    ComponentLibrary {
        motors: vec![
            motor(
                "Example 1404 3000KV",
                3000.0,
                0.090,
                1.2,
                12.0,
                150.0,
                17.0,
                "9x9 M2",
            ),
            motor(
                "Example 2204 2300KV",
                2300.0,
                0.055,
                1.0,
                28.0,
                350.0,
                28.0,
                "16x16 M3",
            ),
            motor(
                "Example 2306 1900KV",
                1900.0,
                0.040,
                1.3,
                35.0,
                700.0,
                33.0,
                "16x16 M3",
            ),
            motor(
                "Example 2812 900KV",
                900.0,
                0.075,
                0.6,
                30.0,
                650.0,
                62.0,
                "19x19 M3",
            ),
            motor(
                "Example 4014 400KV",
                400.0,
                0.080,
                0.8,
                45.0,
                1100.0,
                180.0,
                "25x25 M3",
            ),
        ],
        propellers: vec![
            prop("Example 5x4.3x3", 5.0, 4.3, 3, 4.2),
            prop("Example 7x4x2", 7.0, 4.0, 2, 5.5),
            prop("Example 9x4.7x2", 9.0, 4.7, 2, 12.0),
            prop("Example 10x4.5x2", 10.0, 4.5, 2, 14.0),
            prop("Example 12x6x2", 12.0, 6.0, 2, 24.0),
        ],
        escs: vec![
            esc("Example 12A ESC", 12.0, 15.0, 2, 4, 4.0),
            esc("Example 30A ESC", 30.0, 40.0, 2, 4, 12.0),
            esc("Example 40A ESC", 40.0, 50.0, 3, 6, 28.0),
            esc("Example 60A ESC", 60.0, 80.0, 3, 6, 45.0),
            esc("Example 80A ESC", 80.0, 100.0, 4, 6, 70.0),
        ],
        batteries: vec![
            battery(
                "Example LiPo 3S 1300",
                Chemistry::LiPo,
                3,
                1,
                1300.0,
                75.0,
                18.0,
                105.0,
            ),
            battery(
                "Example LiPo 4S 1500",
                Chemistry::LiPo,
                4,
                1,
                1500.0,
                100.0,
                16.0,
                150.0,
            ),
            battery(
                "Example LiPo 4S 5000",
                Chemistry::LiPo,
                4,
                1,
                5000.0,
                50.0,
                8.0,
                480.0,
            ),
            battery(
                "Example LiPo 6S 10000",
                Chemistry::LiPo,
                6,
                1,
                10000.0,
                25.0,
                9.0,
                1200.0,
            ),
            battery(
                "Example Li-ion 4S2P 10000",
                Chemistry::LiIon,
                4,
                2,
                10000.0,
                5.0,
                40.0,
                620.0,
            ),
        ],
        ..ComponentLibrary::default()
    }
}
