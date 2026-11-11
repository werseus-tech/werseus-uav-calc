// SPDX-License-Identifier: AGPL-3.0-or-later
//! Başsız taşma ölçümü: sol ve sağ panelde çizilen hiçbir şekil/widget panel dikdörtgeninin
//! dışına çıkmaz (±1 px) ve panel kareler boyunca büyüyüp ekrandan kaçmaz.
//!
//! Hata geçmişi: sağ paneldeki kartların içeriği kart genişliğini aşıyordu; yalnız dikey
//! kaydırılan `ScrollArea` yatayda içeriğe göre genişleyip paneli her karede büyüttü ve panel
//! sağ kenardan taştı (içerik soldan kırpıldı, başlık görünmedi).
use crate::project::{DroneType, WizardStepState};
use crate::ui::App;
use crate::ui::wizard::{self, StepId};
use egui::{Rect, pos2, vec2};

const TOL: f32 = 1.0;

struct Measured {
    shapes: Vec<egui::epaint::ClippedShape>,
    widgets: Vec<Rect>,
    /// Etkileşimli (tıklanır/sürüklenir, etkin) widget'lar.
    interactive: Vec<Rect>,
    /// Sürüklenebilen (DragValue, kaydırıcı, metin kutusu) etkin widget'lar.
    draggable: Vec<Rect>,
}

/// `frames` kare çizer; son karenin şekillerini ve widget dikdörtgenlerini döndürür.
fn run(app: &mut App, ctx: &egui::Context, size: egui::Vec2, frames: usize) -> Measured {
    let mut last = Measured {
        shapes: Vec::new(),
        widgets: Vec::new(),
        interactive: Vec::new(),
        draggable: Vec::new(),
    };
    for i in 0..frames {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), size));
        input.time = Some(i as f64 * 0.016);
        let mut out = ctx.run_ui(input, |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        last.shapes = out.shapes;
    }
    let all: Vec<egui::WidgetRect> = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .layers()
            .flat_map(|(_, rs)| rs.iter().cloned())
            .collect()
    });
    last.widgets = all.iter().map(|w| w.rect).collect();
    last.interactive = all
        .iter()
        .filter(|w| w.enabled && (w.sense.senses_click() || w.sense.senses_drag()))
        .map(|w| w.rect)
        .collect();
    last.draggable = all
        .iter()
        .filter(|w| w.enabled && w.sense.senses_drag())
        .map(|w| w.rect)
        .collect();
    last
}

/// Sol paneldeki girdi kutusunda gerçek bir girdi var mı? Sürüklenebilen bir denetim (sayı kutusu,
/// kaydırıcı, metin kutusu) ya da dar bir tıklanabilir denetim (seçenek etiketi, düğme, onay
/// kutusu). Tam genişlikteki tıklanabilir şeyler (katlanabilir başlıklar) sayılmaz.
fn has_real_input(inputs: Rect, m: &Measured) -> bool {
    let inside = |r: &Rect| r.width() >= 4.0 && inputs.expand(1.0).contains_rect(*r);
    m.draggable.iter().any(inside)
        || m.interactive
            .iter()
            .any(|r| inside(r) && r.width() <= inputs.width() * 0.7)
}

fn inside(outer: Rect, r: Rect) -> bool {
    inside_x(outer, r) && r.min.y >= outer.min.y - TOL && r.max.y <= outer.max.y + TOL
}

/// Yalnız yatay: paneller dikey kaydırılır, dikey taşma (kaydırılan içerik) olağandır.
fn inside_x(outer: Rect, r: Rect) -> bool {
    r.min.x >= outer.min.x - TOL && r.max.x <= outer.max.x + TOL
}

/// Panelin içindeki hiçbir şey dışarı taşmamalı. Döndürülen liste taşan öğeleri anlatır.
fn overflows(panel: Rect, m: &Measured) -> Vec<String> {
    let mut bad = Vec::new();
    // Şekiller: kırpma alanı panele ait olanlar (panelin kendi çerçevesi/çizgileri hariç).
    for cs in &m.shapes {
        let clip = cs.clip_rect;
        if clip.width() > panel.width() + 2.0 * TOL || !inside(panel.expand(TOL), clip) {
            continue;
        }
        let b = cs.shape.visual_bounding_rect();
        // Tamamen kırpılmış (görünmeyen) şekiller ölçülmez.
        if !b.is_finite() || !clip.is_positive() || !b.intersects(clip) {
            continue;
        }
        if !inside_x(panel, b) {
            bad.push(format!(
                "şekil {b:?} panel {panel:?} {}",
                format!("{:?}", cs.shape)
                    .chars()
                    .take(300)
                    .collect::<String>()
            ));
        }
    }
    // Widget'lar: panelde başlayanlar yatayda tamamen panelde olmalı.
    for w in &m.widgets {
        // Panelin yeniden boyutlandırma tutamacı (6 px) kenarda durur; ölçüme girmez.
        // Merkez çalışma alanındaki geniş içerik (kapsam dışı) panele girmiş sayılmaz: yalnız
        // panelde başlayan widget'lar ölçülür.
        if w.width() >= 8.0
            && !w.contains_rect(panel)
            && w.min.x >= panel.min.x - TOL
            && w.min.x < panel.max.x
            && w.center().y >= panel.min.y
            && w.center().y <= panel.max.y
            && !inside_x(panel, *w)
        {
            bad.push(format!("widget {w:?} panel {panel:?}"));
        }
    }
    bad
}

fn app_for(dt: DroneType, ctx: &egui::Context) -> App {
    let mut app = App::new_headless(ctx, dt);
    ctx.all_styles_mut(|s| s.debug.show_interactive_widgets = true);
    let n = wizard::steps_for(dt).len();
    app.project.step_states = vec![WizardStepState::Completed; n];
    app
}

fn check_frame(app: &mut App, ctx: &egui::Context, size: egui::Vec2, tag: &str) {
    let _ = run(app, ctx, size, 2);
    let rects_a = app.shell.panel_rects;
    let m = run(app, ctx, size, 1);
    let rects_b = app.shell.panel_rects;

    let right = rects_b.right.expect("sağ panel görünmeli");
    let left = rects_b.left.expect("sol panel görünmeli");
    let screen = Rect::from_min_size(pos2(0.0, 0.0), size);
    assert!(
        inside(screen, right),
        "{tag}: sağ panel ekranda değil {right:?}"
    );
    assert!(
        inside(screen, left),
        "{tag}: sol panel ekranda değil {left:?}"
    );
    assert!(
        (right.width() - super::layout::RIGHT_DEFAULT).abs() <= TOL,
        "{tag}: sağ panel genişliği {} (varsayılan {}) olmalı",
        right.width(),
        super::layout::RIGHT_DEFAULT
    );
    // Kareler arasında büyümez/kaymaz.
    assert_eq!(rects_a.right, rects_b.right, "{tag}: sağ panel kayıyor");
    assert_eq!(rects_a.left, rects_b.left, "{tag}: sol panel kayıyor");

    let bad = overflows(right, &m);
    assert!(
        bad.is_empty(),
        "{tag} SAĞ panel taşıyor: {:#?}",
        &bad[..bad.len().min(6)]
    );
    let bad = overflows(left, &m);
    assert!(
        bad.is_empty(),
        "{tag} SOL panel taşıyor: {:#?}",
        &bad[..bad.len().min(6)]
    );
}

/// Her tür × her sihirbaz adımı × {basit, gelişmiş} × {1280, 1600, 1920} px: panik yok, sol ve
/// sağ panelde hiçbir şekil/widget panelden taşmıyor, panel kaymıyor, Çıktılar dışındaki her
/// adımda sol paneldeki girdi kutusunda en az bir gerçek (etkileşimli) girdi var.
#[test]
fn every_step_fits_panels_and_has_left_inputs_for_every_type_mode_and_width() {
    for dt in DroneType::ALL {
        let steps = wizard::steps_for(dt);
        for simple in [false, true] {
            for width in [1280.0_f32, 1600.0, 1920.0] {
                let ctx = egui::Context::default();
                let mut app = app_for(dt, &ctx);
                // Çok uyarılı durum (adım kilidini bozmadan): uyarı satırları ve rozetler de
                // ölçülür.
                app.project.p_max_elec = 30.0;
                app.project.aero_simple.simple_mode = simple;
                for (idx, step) in steps.iter().enumerate() {
                    app.project.wizard_step = idx;
                    let tag = format!("{dt:?} basit={simple} w={width} adım={step:?}");
                    let size = vec2(width, 900.0);
                    check_frame(&mut app, &ctx, size, &tag);
                    let inputs = app.shell.panel_rects.left_inputs;
                    if *step == StepId::Outputs {
                        assert!(
                            inputs.is_none(),
                            "{tag}: Çıktılar adımında girdi kutusu yok"
                        );
                        continue;
                    }
                    let inputs = inputs.unwrap_or_else(|| panic!("{tag}: girdi kutusu yok"));
                    let m = run(&mut app, &ctx, size, 1);
                    assert!(
                        has_real_input(inputs, &m),
                        "{tag}: sol panelde etkileşimli girdi yok"
                    );
                    assert_eq!(app.project.wizard_step, idx, "{tag}: adım korunmalı");
                }
            }
        }
    }
}

/// Uzman modu: her sekme (rüzgâr tüneli ve Özet'teki çıktılar merkez yuvada) panelden taşmaz.
#[test]
fn expert_tabs_fit_panels() {
    for dt in DroneType::ALL {
        for simple in [false, true] {
            let ctx = egui::Context::default();
            let mut app = app_for(dt, &ctx);
            app.project.expert_mode = true;
            app.project.aero_simple.simple_mode = simple;
            for tab in crate::ui::Tab::tabs_for(dt) {
                app.tab = tab;
                let tag = format!("{dt:?} uzman basit={simple} {tab:?}");
                check_frame(&mut app, &ctx, vec2(1600.0, 900.0), &tag);
            }
        }
    }
}

/// Dar panelde tek, geniş panelde iki sütun; kart genişliği sütuna göre bölünür.
#[test]
fn right_panel_width_limits_are_sane() {
    use super::layout::*;
    assert_eq!((RIGHT_MIN, RIGHT_DEFAULT, RIGHT_MAX), (300.0, 360.0, 520.0));
}

/// Uzun adım adı ("Çıktılar: CAD Parçaları, Montaj ve Malzeme Listesi") dar listede "…" ile
/// kısalır; tam ad ipucundadır (kısaltma `galley.elided` ile ölçülür).
#[test]
fn long_step_titles_are_elided_in_the_list() {
    let ctx = egui::Context::default();
    let mut app = app_for(DroneType::Multirotor, &ctx);
    app.project.wizard_step = wizard::steps_for(DroneType::Multirotor).len() - 1;
    let m = run(&mut app, &ctx, vec2(1280.0, 900.0), 3);
    let mut found = false;
    for cs in &m.shapes {
        if let egui::epaint::Shape::Text(t) = &cs.shape {
            if t.galley.elided && t.galley.text().starts_with("Çıktılar") {
                found = true;
            }
        }
    }
    assert!(found, "uzun adım adı '…' ile kısalmalı");
}
