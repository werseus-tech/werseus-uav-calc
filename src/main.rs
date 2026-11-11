// SPDX-License-Identifier: AGPL-3.0-or-later
// Konsol penceresi açılmasın (release, Windows).
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use werseus_uav_calc::{load_icon, support, ui};

fn main() -> eframe::Result {
    // 1. Çökme günlüğü yakalayıcısını (panic hook) etkinleştir
    support::setup_panic_hook();

    // 2. Komut satırı bağımsız değişkenlerini kontrol et (--uninstall)
    let args: Vec<String> = std::env::args().collect();
    let start_uninstall = args.iter().any(|a| a == "--uninstall");

    // 3. Pencere yapılandırması ve gömülü uygulama ikonu
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WerSeuS – UAV Calc")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "werseus-uav-calc",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new_with_uninstall(cc, start_uninstall)))),
    )
}
