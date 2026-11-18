// SPDX-License-Identifier: AGPL-3.0-or-later
//! Head-up display painted over the flight view: attitude indicator, airspeed, altitude,
//! vertical speed, throttle, battery, mode and status.
use crate::i18n::{t, tf};
use crate::sim::{ControlMode, CrashReason, FlightStatus, Sim, VtolPhase};
use crate::ui::theme::{Tokens, font};
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

fn panel(p: &Painter, r: Rect, tk: &Tokens) {
    p.rect_filled(r, 6.0, tk.surface.gamma_multiply(0.82));
    p.rect_stroke(r, 6.0, Stroke::new(1.0, tk.border), StrokeKind::Inside);
}

fn text(p: &Painter, pos: Pos2, align: Align2, s: impl ToString, size: f32, c: Color32) {
    p.text(pos, align, s.to_string(), FontId::proportional(size), c);
}

/// Rows for the control panel: (label, signed fraction in [-1, 1], value text).
/// Winged aircraft show the actual servo positions (after rate limit) as percent of travel
/// and degrees; multirotors have no surfaces, so the pilot stick inputs are shown instead.
pub fn control_rows(sim: &Sim) -> [(&'static str, f64, String); 3] {
    let fin = |v: f64| {
        if v.is_finite() {
            v.clamp(-1.0, 1.0)
        } else {
            0.0
        }
    };
    if sim.params.wing.is_some() {
        let s = &sim.surfaces;
        let row = |k: &'static str, v: f64| {
            let v = fin(v);
            (
                t(k),
                v,
                format!(
                    "{:+.0} % · {:+.1}°",
                    v * 100.0,
                    (v * crate::sim::vehicle::DELTA_MAX).to_degrees()
                ),
            )
        };
        [
            row("flightsim.hud.elevator", s.elevator),
            row("flightsim.hud.aileron", s.aileron),
            row("flightsim.hud.rudder", s.rudder),
        ]
    } else {
        let i = &sim.input;
        let row = |k: &'static str, v: f64| {
            let v = fin(v);
            (t(k), v, format!("{:+.0} %", v * 100.0))
        };
        [
            row("flightsim.hud.stick_pitch", i.pitch),
            row("flightsim.hud.stick_roll", i.roll),
            row("flightsim.hud.stick_yaw", i.yaw),
        ]
    }
}

pub fn mode_label(sim: &Sim) -> &'static str {
    match sim.mode {
        ControlMode::Stabilized => {
            if sim.params.wing.is_none() {
                t("flightsim.mode.angle")
            } else {
                t("flightsim.mode.stabilized")
            }
        }
        ControlMode::Manual => {
            if sim.params.wing.is_none() {
                t("flightsim.mode.acro")
            } else {
                t("flightsim.mode.manual")
            }
        }
    }
}

pub fn phase_label(phase: VtolPhase) -> &'static str {
    match phase {
        VtolPhase::Hover => t("flightsim.phase.hover"),
        VtolPhase::Transition => t("flightsim.phase.transition"),
        VtolPhase::Cruise => t("flightsim.phase.cruise"),
        VtolPhase::BackTransition => t("flightsim.phase.back_transition"),
    }
}

pub fn status_label(sim: &Sim) -> (&'static str, bool) {
    match sim.status {
        FlightStatus::OnGround => (t("flightsim.status.on_ground"), false),
        FlightStatus::Airborne => (t("flightsim.status.airborne"), false),
        FlightStatus::Crashed(r) => (
            match r {
                CrashReason::HardLanding => t("flightsim.crash.hard_landing"),
                CrashReason::Structure => t("flightsim.crash.structure"),
                CrashReason::PropStrike => t("flightsim.crash.prop_strike"),
                CrashReason::Overturned => t("flightsim.crash.overturned"),
                CrashReason::Overload => t("flightsim.crash.overload"),
                CrashReason::Terrain => t("flightsim.crash.terrain"),
                CrashReason::Building => t("flightsim.crash.building"),
                CrashReason::Numerical => t("flightsim.crash.numerical"),
            },
            true,
        ),
    }
}

/// Paints the HUD into `rect`.
pub fn paint(p: &Painter, rect: Rect, sim: &Sim, tk: &Tokens, paused: bool) {
    let tel = sim.telemetry;
    let s = font::BODY;

    // --- Top: mode, phase, status
    let mut top = format!("{}  ·  {}", mode_label(sim), status_label(sim).0);
    if sim.params.wing.is_some() && sim.params.lift_rotor_count() > 0 {
        top = format!("{top}  ·  {}", phase_label(sim.phase));
    }
    if sim.failsafe {
        top = format!("{top}  ·  {}", t("flightsim.hud.failsafe"));
    }
    if tel.stall > 0.5 && sim.status == FlightStatus::Airborne {
        top = format!("{top}  ·  {}", t("flightsim.hud.stall"));
    }
    if sim.brakes > 0.5 && sim.status == FlightStatus::OnGround && sim.params.has_wheels {
        top = format!("{top}  ·  {}", t("flightsim.hud.brakes"));
    }
    if paused {
        top = format!("{top}  ·  {}", t("flightsim.hud.paused"));
    }
    let g = p.layout_no_wrap(top, FontId::proportional(s), tk.text);
    let tr = Rect::from_center_size(
        pos2(rect.center().x, rect.top() + 18.0),
        g.size() + vec2(20.0, 8.0),
    );
    panel(p, tr, tk);
    let warn = sim.failsafe || tel.stall > 0.5 || status_label(sim).1;
    p.galley(
        tr.min + vec2(10.0, 4.0),
        g,
        if warn { tk.danger } else { tk.text },
    );

    // --- Attitude indicator (centre bottom)
    let r_ai = (rect.height() * 0.13).clamp(36.0, 70.0);
    let c = pos2(rect.center().x, rect.bottom() - r_ai - 34.0);
    p.circle_filled(c, r_ai + 3.0, tk.surface.gamma_multiply(0.85));
    let sky = Color32::from_rgb(70, 130, 200);
    let ground = Color32::from_rgb(140, 100, 60);
    p.circle_filled(c, r_ai, sky);
    // Horizon: rolling right turns the horizon counter-clockwise on screen; pitching up
    // moves it down. Ground = circle segment on the `n` side of the horizon line.
    let roll = tel.roll as f32;
    let off = (tel.pitch as f32 / 0.7).clamp(-1.5, 1.5) * r_ai;
    let (sr, cr) = roll.sin_cos();
    let along = vec2(cr, -sr);
    let n = vec2(sr, cr);
    let hc = c + n * off;
    if off <= -r_ai {
        p.circle_filled(c, r_ai, ground);
    } else if off < r_ai {
        let half = (r_ai * r_ai - off * off).sqrt();
        let (e1, e2) = (hc - along * half, hc + along * half);
        let ang = |v: egui::Vec2| v.y.atan2(v.x);
        let (a1, a2, an) = (ang(e1 - c), ang(e2 - c), ang(n));
        let tau = std::f32::consts::TAU;
        let mut span = (a2 - a1).rem_euclid(tau);
        if (an - a1).rem_euclid(tau) > span {
            span -= tau;
        }
        let mut pts = vec![e1];
        for i in 1..32 {
            let a = a1 + span * i as f32 / 32.0;
            pts.push(c + vec2(a.cos(), a.sin()) * r_ai);
        }
        pts.push(e2);
        p.add(egui::Shape::convex_polygon(pts, ground, Stroke::NONE));
        p.line_segment([e1, e2], Stroke::new(1.5, Color32::WHITE));
    }
    // Aircraft symbol.
    let y = Color32::from_rgb(250, 210, 60);
    p.line_segment(
        [c - vec2(r_ai * 0.5, 0.0), c - vec2(r_ai * 0.15, 0.0)],
        Stroke::new(3.0, y),
    );
    p.line_segment(
        [c + vec2(r_ai * 0.15, 0.0), c + vec2(r_ai * 0.5, 0.0)],
        Stroke::new(3.0, y),
    );
    p.circle_filled(c, 3.0, y);
    p.circle_stroke(c, r_ai, Stroke::new(1.5, tk.border));
    text(
        p,
        c + vec2(0.0, r_ai + 14.0),
        Align2::CENTER_CENTER,
        tf(
            "flightsim.hud.attitude_fmt",
            &[
                format!("{:.0}", tel.roll.to_degrees()),
                format!("{:.0}", tel.pitch.to_degrees()),
                format!("{:.0}", tel.heading.to_degrees().rem_euclid(360.0)),
            ],
        ),
        font::CAPTION,
        tk.text,
    );

    // --- Left: airspeed block, right: altitude block
    let bw = 132.0;
    let lb = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.center().y - 54.0),
        vec2(bw, 108.0),
    );
    panel(p, lb, tk);
    let lines_l = [
        (
            t("flightsim.hud.airspeed"),
            format!("{:.1} m/s", tel.airspeed),
        ),
        (
            t("flightsim.hud.groundspeed"),
            format!("{:.1} m/s", tel.groundspeed),
        ),
        (
            t("flightsim.hud.aoa"),
            format!("{:.1}°", tel.alpha.to_degrees()),
        ),
        (t("flightsim.hud.load"), format!("{:.2} g", tel.load_factor)),
    ];
    for (i, (k, v)) in lines_l.iter().enumerate() {
        let y = lb.top() + 8.0 + i as f32 * 25.0;
        text(
            p,
            pos2(lb.left() + 8.0, y),
            Align2::LEFT_TOP,
            k,
            font::CAPTION,
            tk.text_weak,
        );
        text(
            p,
            pos2(lb.right() - 8.0, y + 9.0),
            Align2::RIGHT_TOP,
            v,
            s,
            tk.text,
        );
    }
    let rb = Rect::from_min_size(
        pos2(rect.right() - 10.0 - bw, rect.center().y - 54.0),
        vec2(bw, 108.0),
    );
    panel(p, rb, tk);
    let wind = tel.wind;
    let lines_r = [
        (t("flightsim.hud.altitude"), format!("{:.1} m", tel.agl)),
        (t("flightsim.hud.vspeed"), format!("{:+.1} m/s", tel.vz_up)),
        (
            t("flightsim.hud.wind"),
            format!("{:.1} m/s", (wind.x.hypot(wind.y))),
        ),
        (t("flightsim.hud.time"), format!("{:.1} s", tel.t)),
    ];
    for (i, (k, v)) in lines_r.iter().enumerate() {
        let y = rb.top() + 8.0 + i as f32 * 25.0;
        text(
            p,
            pos2(rb.left() + 8.0, y),
            Align2::LEFT_TOP,
            k,
            font::CAPTION,
            tk.text_weak,
        );
        text(
            p,
            pos2(rb.right() - 8.0, y + 9.0),
            Align2::RIGHT_TOP,
            v,
            s,
            tk.text,
        );
    }

    // --- Bottom left: throttle and battery bars
    let bb = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.bottom() - 82.0),
        vec2(190.0, 72.0),
    );
    panel(p, bb, tk);
    let bar = |row: f32, label: &str, frac: f64, value: String, col: Color32| {
        let y = bb.top() + 8.0 + row * 30.0;
        text(
            p,
            pos2(bb.left() + 8.0, y),
            Align2::LEFT_TOP,
            label,
            font::CAPTION,
            tk.text_weak,
        );
        text(
            p,
            pos2(bb.right() - 8.0, y),
            Align2::RIGHT_TOP,
            value,
            font::CAPTION,
            tk.text,
        );
        let r = Rect::from_min_size(
            pos2(bb.left() + 8.0, y + 15.0),
            vec2(bb.width() - 16.0, 7.0),
        );
        p.rect_filled(r, 3.0, tk.border);
        let mut f = r;
        f.set_width(r.width() * frac.clamp(0.0, 1.0) as f32);
        p.rect_filled(f, 3.0, col);
    };
    bar(
        0.0,
        t("flightsim.hud.throttle"),
        sim.input.throttle,
        format!("{:.0} %", sim.input.throttle * 100.0),
        tk.accent,
    );
    let usable = sim.params.battery.dod;
    let soc_col = if tel.soc < 1.0 - usable {
        tk.danger
    } else if tel.soc < 1.0 - 0.7 * usable {
        tk.warning
    } else {
        tk.success
    };
    bar(
        1.0,
        t("flightsim.hud.battery"),
        tel.soc,
        format!(
            "{:.0} % · {:.1} V · {:.0} W",
            tel.soc * 100.0,
            tel.voltage,
            tel.power
        ),
        soc_col,
    );

    // --- Bottom right: control surface deflections (winged) or stick inputs (multirotor)
    let rows = control_rows(sim);
    let cb = Rect::from_min_size(
        pos2(rect.right() - 10.0 - 210.0, rect.bottom() - 26.0 - 96.0),
        vec2(210.0, 96.0),
    );
    panel(p, cb, tk);
    text(
        p,
        pos2(cb.left() + 8.0, cb.top() + 6.0),
        Align2::LEFT_TOP,
        if sim.params.wing.is_some() {
            t("flightsim.hud.surfaces")
        } else {
            t("flightsim.hud.sticks")
        },
        font::CAPTION,
        tk.text_weak,
    );
    for (i, (label, frac, value)) in rows.iter().enumerate() {
        let y = cb.top() + 26.0 + i as f32 * 23.0;
        text(
            p,
            pos2(cb.left() + 8.0, y),
            Align2::LEFT_TOP,
            label,
            font::CAPTION,
            tk.text,
        );
        text(
            p,
            pos2(cb.right() - 8.0, y),
            Align2::RIGHT_TOP,
            value,
            font::CAPTION,
            tk.text,
        );
        let r = Rect::from_min_size(
            pos2(cb.left() + 8.0, y + 14.0),
            vec2(cb.width() - 16.0, 5.0),
        );
        p.rect_filled(r, 2.0, tk.border);
        let cx = r.center().x;
        let f = frac.clamp(-1.0, 1.0) as f32;
        let end = cx + f * r.width() * 0.5;
        let fill = Rect::from_x_y_ranges(cx.min(end)..=cx.max(end), r.y_range());
        p.rect_filled(fill, 2.0, tk.accent);
        p.line_segment(
            [pos2(cx, r.top() - 2.0), pos2(cx, r.bottom() + 2.0)],
            Stroke::new(1.0, tk.text_weak),
        );
    }

    // --- Crash banner
    if let (label, true) = status_label(sim) {
        let msg = format!("{}  —  {}", label, t("flightsim.hud.reset_hint"));
        let g = p.layout_no_wrap(msg, FontId::proportional(font::SUBHEAD), tk.danger);
        let r =
            Rect::from_center_size(rect.center() - vec2(0.0, 40.0), g.size() + vec2(28.0, 16.0));
        p.rect_filled(r, 8.0, tk.surface);
        p.rect_stroke(r, 8.0, Stroke::new(2.0, tk.danger), StrokeKind::Inside);
        p.galley(r.min + vec2(14.0, 8.0), g, tk.danger);
    }

    // --- Disclaimer
    text(
        p,
        pos2(rect.right() - 8.0, rect.bottom() - 6.0),
        Align2::RIGHT_BOTTOM,
        t("flightsim.disclaimer_short"),
        font::CAPTION,
        tk.text_weak,
    );
}

#[cfg(test)]
mod control_rows_tests {
    use super::control_rows;

    #[test]
    fn winged_rows_show_percent_and_degrees() {
        let p = crate::project::Project::from_json(crate::project::EX_FIXED_WING).unwrap();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut sim = crate::sim::sim_for_project(&p, &m, crate::sim::StartPosition::Air);
        sim.surfaces.elevator = 0.5;
        sim.surfaces.aileron = -1.0;
        sim.surfaces.rudder = f64::NAN;
        let r = control_rows(&sim);
        assert!((r[0].1 - 0.5).abs() < 1e-12);
        assert!(r[0].2.contains("+50 %"), "{}", r[0].2);
        assert!(r[0].2.contains("°"));
        assert!(r[1].2.contains("-100 %"), "{}", r[1].2);
        assert_eq!(r[2].1, 0.0);
    }

    #[test]
    fn multirotor_rows_show_stick_inputs() {
        let p = crate::project::Project::default();
        let m = crate::calc::evaluate(&p, &crate::calc::Library::embedded());
        let mut sim = crate::sim::sim_for_project(&p, &m, crate::sim::StartPosition::Air);
        if sim.params.wing.is_some() {
            return;
        }
        sim.input.roll = 0.25;
        let r = control_rows(&sim);
        assert!(r[1].2.contains("+25 %"), "{}", r[1].2);
    }
}
