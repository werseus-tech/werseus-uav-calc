// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod ai;
pub mod cad;
pub mod calc;
pub mod computebudget;
pub mod db;
pub mod export;
pub mod footprint;
pub mod geom3d;
pub mod i18n;
pub mod library;
pub mod massprops;
pub mod project;
pub mod propcurves;
pub mod regulation;
pub mod report;
pub mod sim;
pub mod structfea;
pub mod support;
pub mod teach;
pub mod thermalpower;
pub mod ui;
pub mod windtunnel;

pub mod acoustics;

/// Gömülü uygulama ikonunu yükler.
pub fn load_icon() -> egui::IconData {
    let icon_bytes = include_bytes!("../assets/icon/app_icon.png");
    let img = image::load_from_memory(icon_bytes).expect("Failed to load icon");
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    }
}
