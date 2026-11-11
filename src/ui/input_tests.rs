// SPDX-License-Identifier: AGPL-3.0-or-later
//! Başsız klavye/pano testleri: odaktaki metin alanına `Event::Paste`, `Event::Text` ve
//! Ctrl+A + yapıştırma ulaşmalı. Tüm kabuk karesi çizilir; böylece genel kısayollar ya da 3D
//! görünüm girdi işleyicileri olayı yutarsa test düşer. Anahtarlar bellek içi sahte depoya
//! yazılır (gerçek Kimlik Bilgisi Yöneticisi/ayar dosyası kullanılmaz).
use super::App;
use crate::ai::config::AiProviderType;
use crate::project::DroneType;
use egui::accesskit::Role;
use egui::{Event, Id, Key, Modifiers, Rect, pos2, vec2};

/// Başsız uygulama + alan keşfi için egui'nin widget bilgisini açar.
fn new_app(ctx: &egui::Context) -> App {
    let app = App::new_headless(ctx, DroneType::Multirotor);
    ctx.all_styles_mut(|s| s.debug.show_interactive_widgets = true);
    app
}

fn frame(
    app: &mut App,
    ctx: &egui::Context,
    t: f64,
    events: Vec<Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0)));
    input.time = Some(t);
    input.focused = true;
    input.events = events;
    let mut out = ctx.run_ui(input, |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    out.shapes
}

/// Çizilen tüm metinler (maskeleme denetimi için).
fn drawn_text(shapes: &[egui::epaint::ClippedShape]) -> String {
    let mut s = String::new();
    for cs in shapes {
        if let egui::epaint::Shape::Text(t) = &cs.shape {
            s.push_str(t.galley.text());
            s.push('\n');
        }
    }
    s
}

/// Son karedeki metin giriş alanları (rol, id).
fn text_inputs(ctx: &egui::Context) -> Vec<(Role, Id)> {
    ctx.viewport(|v| {
        let mut out = Vec::new();
        for (_, rects) in v.prev_pass.widgets.layers() {
            for w in rects {
                if let Some(info) = v.prev_pass.widgets.info(w.id) {
                    if matches!(
                        info.role,
                        Role::TextInput | Role::MultilineTextInput | Role::PasswordInput
                    ) {
                        out.push((info.role, w.id));
                    }
                }
            }
        }
        out
    })
}

fn focus(app: &mut App, ctx: &egui::Context, id: Id, t: &mut f64) {
    ctx.memory_mut(|m| m.request_focus(id));
    for _ in 0..2 {
        *t += 0.016;
        frame(app, ctx, *t, vec![]);
    }
    assert!(ctx.memory(|m| m.has_focus(id)), "alan odak almalı");
}

fn ctrl_a() -> Event {
    Event::Key {
        key: Key::A,
        physical_key: Some(Key::A),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }
}

fn backspace() -> Event {
    Event::Key {
        key: Key::Backspace,
        physical_key: Some(Key::Backspace),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Üç senaryo: yapıştır, yaz, hepsini seçip yapıştırarak üzerine yaz. Alan hedef alan değilse
/// (yapıştırma `read` değerini değiştirmediyse) `false` döner.
fn exercise(
    app: &mut App,
    ctx: &egui::Context,
    id: Id,
    t: &mut f64,
    read: &dyn Fn(&App) -> String,
) -> bool {
    focus(app, ctx, id, t);
    // Mevcut içeriği temizle (Ctrl+A, Backspace).
    *t += 0.016;
    frame(app, ctx, *t, vec![ctrl_a()]);
    *t += 0.016;
    frame(app, ctx, *t, vec![backspace()]);
    *t += 0.016;
    frame(app, ctx, *t, vec![Event::Paste("sk-test-123".into())]);
    if read(app) != "sk-test-123" {
        return false;
    }
    *t += 0.016;
    frame(app, ctx, *t, vec![Event::Text("xy".into())]);
    assert_eq!(read(app), "sk-test-123xy", "yazma");
    *t += 0.016;
    frame(app, ctx, *t, vec![ctrl_a()]);
    *t += 0.016;
    frame(app, ctx, *t, vec![Event::Paste("sk-new-456".into())]);
    assert_eq!(read(app), "sk-new-456", "Ctrl+A + yapıştırma üzerine yazar");
    // Sonraki karede değer geri alınmamalı (her karede başka kaynaktan geri yazılmamalı).
    *t += 0.016;
    frame(app, ctx, *t, vec![]);
    assert_eq!(read(app), "sk-new-456", "değer kararlı kalmalı");
    true
}

/// İki kare çizip metin alanlarını keşfeder.
fn discover(app: &mut App, ctx: &egui::Context, t: &mut f64) -> Vec<(Role, Id)> {
    for _ in 0..2 {
        frame(app, ctx, *t, vec![]);
        *t += 0.016;
    }
    text_inputs(ctx)
}

#[test]
fn project_name_field_accepts_paste_text_and_overwrite() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut t = 0.0;
    let inputs = discover(&mut app, &ctx, &mut t);
    assert!(
        inputs
            .iter()
            .any(|(_, id)| exercise(&mut app, &ctx, *id, &mut t, &|a| a.project.name.clone())),
        "proje adı alanı yapıştırmayı almadı: {inputs:?}"
    );
}

#[test]
fn chat_box_accepts_paste_text_and_overwrite() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.ai_state.is_panel_open = true;
    app.ai_config.enabled = true;
    let mut t = 0.0;
    let inputs = discover(&mut app, &ctx, &mut t);
    assert!(
        inputs
            .iter()
            .any(|(_, id)| exercise(&mut app, &ctx, *id, &mut t, &|a| a
                .ai_state
                .input_text
                .clone())),
        "sohbet kutusu yapıştırmayı almadı: {inputs:?}"
    );
}

#[test]
fn api_key_field_accepts_paste_for_every_cloud_provider_and_stays_masked() {
    for (prov, target) in [
        (AiProviderType::OpenAiCompatible, "WerSeuS_AI_ApiKey_openai"),
        (AiProviderType::Anthropic, "WerSeuS_AI_ApiKey_anthropic"),
        (AiProviderType::Gemini, "WerSeuS_AI_ApiKey_gemini"),
    ] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.ai_config.provider = prov;
        crate::ui::settings_page::set_section(&ctx, crate::ui::settings_page::SettingsSection::Ai);
        app.show_settings_modal = true;
        let mut t = 0.0;
        let inputs = discover(&mut app, &ctx, &mut t);
        let store = app.cred_store.clone();
        let key = target.to_string();
        let read = move |_: &App| store.get(&key).unwrap_or_default();
        assert!(
            inputs
                .iter()
                .any(|(_, id)| exercise(&mut app, &ctx, *id, &mut t, &read)),
            "{prov:?}: API anahtarı alanı yapıştırmayı almadı: {inputs:?}"
        );
        // Anahtar ekranda düz metin olarak çizilmemeli (maskeli alan).
        t += 0.016;
        let shapes = frame(&mut app, &ctx, t, vec![]);
        let text = drawn_text(&shapes);
        assert!(
            !text.contains("sk-new-456"),
            "{prov:?}: anahtar maskesiz çizildi"
        );
        // Gerçek depoya hiçbir şey yazılmadı: yalnız bellek içi sahte depo kullanıldı.
        assert!(!app.cred_store.is_store_in_wincred());
    }
}

#[test]
fn global_shortcut_does_not_fire_while_typing_in_a_text_field() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let mut t = 0.0;
    let inputs = discover(&mut app, &ctx, &mut t);
    let (_, id) = inputs[0];
    focus(&mut app, &ctx, id, &mut t);
    let ev = Event::Key {
        key: Key::A,
        physical_key: Some(Key::A),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
    };
    frame(&mut app, &ctx, t + 0.1, vec![ev]);
    assert!(
        !app.ai_state.is_panel_open,
        "odaktaki alanda kısayol çalışmamalı"
    );
}

/// Pano (arboard) derlenmezse egui-winit yalnız uygulama içi yedek panoyu kullanır ve Windows
/// panosundan Ctrl+V çalışmaz. `default-features = false` olduğu için özellik açıkça istenmeli.
/// (Başsız testler gerçek panoyu sınayamaz; bu yüzden derleme özelliği ayrıca korunur.)
#[test]
fn eframe_clipboard_feature_stays_enabled() {
    let toml = include_str!("../../Cargo.toml");
    let line = toml
        .lines()
        .find(|l| l.trim_start().starts_with("eframe"))
        .expect("eframe bağımlılığı");
    assert!(line.contains("\"clipboard\""), "{line}");
    let lock = include_str!("../../Cargo.lock");
    assert!(lock.contains("name = \"arboard\""));
}
