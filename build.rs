// SPDX-License-Identifier: AGPL-3.0-or-later
// Windows kaynaklari: exe ikonu + surum bilgisi (rc.exe, Visual Studio Build Tools ile gelir).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/app_icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/app_icon.ico");
        res.set("ProductName", "WerSeuS – UAV Calc");
        res.set("FileDescription", "WerSeuS – UAV Calc");
        res.set("CompanyName", "WerSeuS");
        res.set("LegalCopyright", "WerSeuS – AGPL-3.0-or-later");
        res.set("OriginalFilename", "werseus-uav-calc.exe");
        res.set("InternalName", "werseus-uav-calc");
        // Surum alanlari (FileVersion / ProductVersion) Cargo.toml'daki surumden otomatik gelir.
        if let Err(e) = res.compile() {
            panic!("Windows kaynak derlemesi basarisiz (rc.exe bulunamadi mi?): {e}");
        }
    }
}
