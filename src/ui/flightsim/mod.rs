// SPDX-License-Identifier: AGPL-3.0-or-later
//! Flight simulator screen: fly the designed aircraft (approximate model, for learning).
//!
//! The simulation runs on the UI thread with a fixed 400 Hz step and a per-frame step
//! budget, so a slow frame turns into slow motion instead of a frozen window. The physics
//! lives in `crate::sim`; this module only handles input, controls, camera and the HUD.
pub mod hud;
pub mod view;

use crate::calc::Model;
use crate::i18n::{t, tf};
use crate::project::Project;
use crate::sim::{self, ControlMode, Sim, StartPosition, WindConfig};
use crate::ui::components::{segmented, tool_button};
use crate::ui::theme::{self, font, space};
use egui::{Key, RichText, Sense, Ui, vec2};

/// Most physics steps per frame (0.1 s of simulated time).
const MAX_STEPS_PER_FRAME: usize = 40;

#[derive(Default)]
struct Sticks {
    pitch: f64,
    roll: f64,
    yaw: f64,
}

pub struct FlightSimState {
    sim: Option<Sim>,
    project_key: u64,
    running: bool,
    start: StartPosition,
    wind: WindConfig,
    accumulator: f64,
    sticks: Sticks,
    fail_index: usize,
    view: view::ChaseView,
    /// Wall time spent in physics during the last frame [ms].
    pub last_cost_ms: f64,
}

impl Default for FlightSimState {
    fn default() -> Self {
        Self {
            sim: None,
            project_key: 0,
            running: true,
            start: StartPosition::Ground,
            wind: WindConfig::default(),
            accumulator: 0.0,
            sticks: Sticks::default(),
            fail_index: 0,
            view: view::ChaseView::default(),
            last_cost_ms: 0.0,
        }
    }
}

fn project_key(p: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.to_json().unwrap_or_default().hash(&mut h);
    h.finish()
}

fn approach(cur: f64, target: f64, rate: f64, dt: f64) -> f64 {
    let step = (rate * dt).abs();
    cur + (target - cur).clamp(-step, step)
}

impl FlightSimState {
    pub fn sim(&self) -> Option<&Sim> {
        self.sim.as_ref()
    }

    /// Rebuilds the simulator when the design changed.
    fn sync(&mut self, p: &Project, m: &Model) {
        let key = project_key(p);
        if self.sim.is_some() && key == self.project_key {
            return;
        }
        self.project_key = key;
        let mode = self.sim.as_ref().map(|s| s.mode).unwrap_or_default();
        let mut s = sim::sim_for_project(p, m, self.start);
        s.mode = mode;
        s.set_wind(self.wind);
        self.sim = Some(s);
        let spec = crate::geom3d::from_project(p, m, &crate::geom3d::ViewOptions::default());
        self.view.set_scene(crate::geom3d::build(&spec));
        self.fail_index = 0;
    }

    fn reset(&mut self) {
        if let Some(s) = self.sim.as_mut() {
            s.reset(self.start);
            s.set_wind(self.wind);
        }
        self.sticks = Sticks::default();
        self.accumulator = 0.0;
    }

    /// Keyboard: arrows = pitch / roll, A/D = yaw, W/S = throttle, Space = pause,
    /// R = reset, M = mode, T = VTOL transition, G = gust.
    fn keyboard(&mut self, ui: &Ui, dt: f64) {
        let focused = ui.ctx().memory(|m| m.focused().is_some());
        if focused {
            return;
        }
        let (up, down, left, right, a, d, w, s, x) = ui.input(|i| {
            (
                i.key_down(Key::ArrowUp),
                i.key_down(Key::ArrowDown),
                i.key_down(Key::ArrowLeft),
                i.key_down(Key::ArrowRight),
                i.key_down(Key::A),
                i.key_down(Key::D),
                i.key_down(Key::W),
                i.key_down(Key::S),
                i.key_pressed(Key::X),
            )
        });
        let (space_k, r_k, m_k, t_k, g_k) = ui.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::R),
                i.key_pressed(Key::M),
                i.key_pressed(Key::T),
                i.key_pressed(Key::G),
            )
        });
        let axis = |neg: bool, pos: bool| match (neg, pos) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };
        // Stick back (Down arrow) pulls the nose up, as in most flight simulators.
        let targets = (axis(up, down), axis(left, right), axis(a, d));
        let shape = |cur: f64, tgt: f64| {
            let rate = if tgt == 0.0 { 5.0 } else { 3.0 };
            approach(cur, tgt, rate, dt)
        };
        self.sticks.pitch = shape(self.sticks.pitch, targets.0);
        self.sticks.roll = shape(self.sticks.roll, targets.1);
        self.sticks.yaw = shape(self.sticks.yaw, targets.2);
        if space_k {
            self.running = !self.running;
        }
        if r_k {
            self.reset();
        }
        if let Some(sim) = self.sim.as_mut() {
            let thr = sim.input.throttle + axis(s, w) * 0.5 * dt;
            sim.input.throttle = if x { 0.0 } else { thr.clamp(0.0, 1.0) };
            sim.input.pitch = self.sticks.pitch;
            sim.input.roll = self.sticks.roll;
            sim.input.yaw = self.sticks.yaw;
            if m_k {
                sim.mode = match sim.mode {
                    ControlMode::Stabilized => ControlMode::Manual,
                    ControlMode::Manual => ControlMode::Stabilized,
                };
            }
            if t_k {
                sim.toggle_transition();
            }
            if g_k {
                sim.trigger_gust();
            }
        }
    }

    /// Advances the simulation by the frame time (fixed steps, bounded per frame).
    fn advance(&mut self, dt: f64) {
        let Some(sim) = self.sim.as_mut() else {
            return;
        };
        if !self.running || sim.is_crashed() {
            self.accumulator = 0.0;
            self.last_cost_ms = 0.0;
            return;
        }
        let t0 = std::time::Instant::now();
        self.accumulator += dt.clamp(0.0, 0.1);
        let mut steps = 0;
        while self.accumulator >= sim::DT && steps < MAX_STEPS_PER_FRAME {
            sim.step();
            self.accumulator -= sim::DT;
            steps += 1;
        }
        if steps == MAX_STEPS_PER_FRAME {
            self.accumulator = 0.0;
        }
        self.last_cost_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }
}

/// Draws the whole screen (controls on the left, flight view on the right).
pub fn show(ui: &mut Ui, p: &Project, m: &Model, st: &mut FlightSimState) {
    let tk = *theme::tokens(ui);
    st.sync(p, m);
    let dt = (ui.input(|i| i.unstable_dt) as f64).clamp(0.0, 0.1);
    let dt = if dt.is_finite() { dt } else { 0.0 };
    st.keyboard(ui, dt);
    st.advance(dt);

    let ctl_w = 250.0_f32.min(ui.available_width() * 0.4);
    ui.horizontal_top(|ui| {
        let size = vec2(ctl_w, ui.available_height());
        ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(ctl_w);
            egui::ScrollArea::vertical()
                .id_salt("flightsim_controls")
                .auto_shrink([false, false])
                .show(ui, |ui| controls(ui, st, &tk));
        });
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(
            vec2(avail.x.max(100.0), avail.y.max(160.0)),
            Sense::click_and_drag(),
        );
        if resp.dragged() {
            let d = resp.drag_delta();
            st.view.yaw_offset += d.x * 0.008;
            st.view.pitch = (st.view.pitch + d.y * 0.006).clamp(-0.2, 1.4);
        }
        if resp.double_clicked() {
            st.view.yaw_offset = 0.0;
            st.view.pitch = 0.28;
            st.view.zoom = 1.0;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                st.view.zoom = (st.view.zoom * (-scroll * 0.002).exp()).clamp(0.3, 6.0);
            }
        }
        let Some(sim) = st.sim.as_ref() else {
            return;
        };
        if st.view.gl_failed() {
            // 2D fallback: the HUD alone (attitude indicator, tapes) still flies the aircraft.
            ui.painter_at(rect)
                .rect_filled(rect, theme::corner(theme::radius::M), tk.viewport_bg);
            ui.painter_at(rect).text(
                rect.center_top() + vec2(0.0, 40.0),
                egui::Align2::CENTER_TOP,
                t("flightsim.gl_fallback"),
                egui::FontId::proportional(font::CAPTION),
                tk.text_weak,
            );
        } else {
            let sim_ref: &Sim = sim;
            st.view.paint(ui, rect, sim_ref, &tk, dt as f32);
        }
        if let Some(sim) = st.sim.as_ref() {
            hud::paint(&ui.painter_at(rect), rect, sim, &tk, !st.running);
        }
    });
    if st.running {
        ui.ctx().request_repaint();
    }
}

fn controls(ui: &mut Ui, st: &mut FlightSimState, tk: &theme::Tokens) {
    ui.label(
        RichText::new(t("flightsim.title"))
            .size(font::HEADING)
            .strong()
            .color(tk.text),
    );
    ui.label(
        RichText::new(t("flightsim.disclaimer"))
            .size(font::CAPTION)
            .color(tk.warning),
    );
    ui.add_space(space::S);

    ui.horizontal_wrapped(|ui| {
        let label = if st.running {
            t("flightsim.btn.pause")
        } else {
            t("flightsim.btn.run")
        };
        if tool_button(ui, label, t("flightsim.btn.pause_tip")).clicked() {
            st.running = !st.running;
        }
        if tool_button(ui, t("flightsim.btn.reset"), t("flightsim.btn.reset_tip")).clicked() {
            st.reset();
        }
    });
    ui.add_space(space::XS);
    ui.label(
        RichText::new(t("flightsim.start"))
            .size(font::CAPTION)
            .color(tk.text_weak),
    );
    let mut start = st.start;
    if segmented(
        ui,
        &mut start,
        &[
            (StartPosition::Ground, t("flightsim.start.ground"), ""),
            (StartPosition::Air, t("flightsim.start.air"), ""),
        ],
    ) {
        st.start = start;
        st.reset();
    }

    let Some(sim) = st.sim.as_mut() else {
        return;
    };
    ui.add_space(space::XS);
    ui.label(
        RichText::new(t("flightsim.mode"))
            .size(font::CAPTION)
            .color(tk.text_weak),
    );
    let rotor_only = sim.params.wing.is_none();
    let (stab, man) = if rotor_only {
        (t("flightsim.mode.angle"), t("flightsim.mode.acro"))
    } else {
        (t("flightsim.mode.stabilized"), t("flightsim.mode.manual"))
    };
    segmented(
        ui,
        &mut sim.mode,
        &[
            (
                ControlMode::Stabilized,
                stab,
                t("flightsim.mode.stabilized_tip"),
            ),
            (ControlMode::Manual, man, t("flightsim.mode.manual_tip")),
        ],
    );
    if sim.params.wing.is_some() && sim.params.lift_rotor_count() > 0 {
        ui.add_space(space::XS);
        ui.horizontal_wrapped(|ui| {
            if tool_button(
                ui,
                t("flightsim.btn.transition"),
                t("flightsim.btn.transition_tip"),
            )
            .clicked()
            {
                sim.toggle_transition();
            }
            ui.label(RichText::new(hud::phase_label(sim.phase)).color(tk.accent));
        });
        if sim.transition_aborted {
            ui.label(
                RichText::new(t("flightsim.transition_aborted"))
                    .size(font::CAPTION)
                    .color(tk.warning),
            );
        }
    }

    // --- Scenarios
    ui.add_space(space::S);
    ui.separator();
    ui.label(
        RichText::new(t("flightsim.scenarios"))
            .strong()
            .color(tk.text),
    );
    let mut wind = st.wind;
    let mut changed = false;
    changed |= ui
        .add(egui::Slider::new(&mut wind.speed, 0.0..=15.0).text(t("flightsim.wind_speed")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut wind.from_deg, 0.0..=359.0).text(t("flightsim.wind_from")))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut wind.turbulence, 0.0..=1.0).text(t("flightsim.turbulence")))
        .changed();
    if changed {
        st.wind = wind;
        sim.set_wind(wind);
    }
    ui.horizontal_wrapped(|ui| {
        if tool_button(ui, t("flightsim.btn.gust"), t("flightsim.btn.gust_tip")).clicked() {
            sim.trigger_gust();
        }
        let drop = ui.add_enabled(
            !sim.payload_dropped && sim.params.payload_mass > 0.0,
            egui::Button::new(t("flightsim.btn.drop_payload")),
        );
        if drop
            .on_hover_text(t("flightsim.btn.drop_payload_tip"))
            .clicked()
        {
            sim.drop_payload();
        }
        if tool_button(
            ui,
            t("flightsim.btn.add_mass"),
            t("flightsim.btn.add_mass_tip"),
        )
        .clicked()
        {
            sim.add_mass(0.25);
        }
    });
    ui.add_space(space::XS);
    let n = sim.params.rotors.len();
    if n > 0 {
        st.fail_index = st.fail_index.min(n - 1);
        let name = |k: usize, sim: &Sim| {
            if sim.params.rotors[k].lift {
                tf("flightsim.motor_lift_fmt", &[(k + 1).to_string()])
            } else {
                tf("flightsim.motor_fwd_fmt", &[(k + 1).to_string()])
            }
        };
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("flightsim_fail_motor")
                .selected_text(name(st.fail_index, sim))
                .show_ui(ui, |ui| {
                    for k in 0..n {
                        ui.selectable_value(&mut st.fail_index, k, name(k, sim));
                    }
                });
            let enabled = !sim.motor_failed(st.fail_index);
            let b = ui.add_enabled(enabled, egui::Button::new(t("flightsim.btn.fail_motor")));
            if b.on_hover_text(t("flightsim.btn.fail_motor_tip")).clicked() {
                sim.fail_motor(st.fail_index);
            }
        });
    }

    // --- Model summary
    ui.add_space(space::S);
    ui.separator();
    ui.label(RichText::new(t("flightsim.model")).strong().color(tk.text));
    let i = sim.inertia;
    let mut rows = vec![
        (t("flightsim.model.mass"), format!("{:.2} kg", sim.mass)),
        (
            t("flightsim.model.inertia"),
            format!("{:.3} / {:.3} / {:.3}", i.ixx, i.iyy, i.izz),
        ),
        (
            t("flightsim.model.battery"),
            format!("{:.0} Wh", sim.params.battery.energy_wh),
        ),
    ];
    if sim.params.wing.is_some() {
        rows.push((
            t("flightsim.model.vstall"),
            format!("{:.1} m/s", sim.params.v_stall),
        ));
    }
    for (k, v) in rows {
        ui.horizontal(|ui| {
            ui.label(RichText::new(k).size(font::CAPTION).color(tk.text_weak));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(v).size(font::CAPTION).color(tk.text));
            });
        });
    }
    ui.label(
        RichText::new(tf(
            "flightsim.cost_fmt",
            &[format!("{:.2}", st.last_cost_ms)],
        ))
        .size(font::CAPTION)
        .color(tk.text_weak),
    );

    // --- Keys
    ui.add_space(space::S);
    egui::CollapsingHeader::new(RichText::new(t("flightsim.keys")).strong())
        .id_salt("flightsim_keys")
        .default_open(true)
        .show(ui, |ui| {
            for k in [
                "flightsim.keys.pitch",
                "flightsim.keys.roll",
                "flightsim.keys.yaw",
                "flightsim.keys.throttle",
                "flightsim.keys.other",
                "flightsim.keys.mouse",
            ] {
                ui.label(
                    RichText::new(t_key(k))
                        .size(font::CAPTION)
                        .color(tk.text_weak),
                );
            }
        });
    ui.label(
        RichText::new(t("flightsim.assumptions"))
            .size(font::CAPTION)
            .color(tk.text_weak),
    );
}

/// `t` for the key-help list (keys are literal constants above).
fn t_key(k: &'static str) -> &'static str {
    crate::i18n::t(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::DroneType;

    /// Every `flightsim.*` key used in this module exists in both languages.
    #[test]
    fn all_keys_exist_in_both_languages() {
        use crate::i18n::{Lang, t_lang};
        let sources = [
            include_str!("mod.rs"),
            include_str!("hud.rs"),
            include_str!("view.rs"),
        ];
        let mut n = 0;
        for src in sources {
            for part in src.split('"').skip(1).step_by(2) {
                if part.starts_with("flightsim.") && !part.ends_with('.') {
                    let key: &'static str = Box::leak(part.to_string().into_boxed_str());
                    for lang in [Lang::Tr, Lang::En] {
                        assert_ne!(t_lang(key, lang), key, "missing {key} ({lang:?})");
                    }
                    n += 1;
                }
            }
        }
        assert!(n > 40, "found {n} keys");
    }

    #[test]
    fn screen_renders_headless_for_all_types() {
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
            let mut st = FlightSimState::default();
            let ctx = egui::Context::default();
            crate::ui::theme::install(&ctx, crate::ui::theme::ThemeChoice::Light);
            for frame in 0..3 {
                let mut input = egui::RawInput::default();
                input.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1100.0, 700.0),
                ));
                if frame == 2 {
                    st.view.force_gl_failure();
                }
                let mut out = ctx.run_ui(input, |ui| {
                    egui::CentralPanel::default().show(ui, |ui| show(ui, &p, &m, &mut st));
                });
                out.textures_delta.clear();
            }
            assert!(st.sim().is_some(), "{dt:?}");
            assert!(st.sim().unwrap().state.is_finite());
        }
    }
}
