// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canlı 2D şema çizimi: egui Painter ile anında güncellenen ölçekli üstten görünüm ve ölçü çizgileri.
//! Desteklenen türler: Klasik kuyruk, V-kuyruk, Uçan kanat, Multirotor (X, +, H), VTOL Quadplane.

use crate::calc::Model;
use crate::calc::geometry::TailKind;
use crate::calc::multirotor::MultirotorLayout;
use crate::i18n::t;
use crate::project::{DroneType, Project};
use crate::ui::panel_kit::tinted_frame;
use crate::ui::theme::{self, Tokens, radius};
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2, epaint::PathShape};

/// Bu karede etkin temanın tokenları (çizim işlevleri renkleri yalnız buradan alır).
fn tk_of(painter: &egui::Painter) -> &'static Tokens {
    theme::tokens_ctx(painter.ctx())
}

/// Gövde/yüzey dolgusu: koyu temada hafif açık, açık temada beyaz (zeminden ayrışsın).
fn body_fill(tk: &Tokens) -> Color32 {
    if tk.dark { tk.surface_alt } else { tk.surface }
}

pub fn show(ui: &mut Ui, p: &Project, m: &Model, height: f32) {
    let tk = theme::tokens(ui);
    tinted_frame(tk, tk.surface).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong(t("schematic_title"));
            ui.label(
                egui::RichText::new(format!("— {}", p.drone_type.label())).color(tk.text_weak),
            );
            if p.drone_type.is_multirotor() && m.mr.prop_overlap {
                ui.colored_label(tk.danger, format!("[!] {}", t("schematic_prop_overlap")));
            }
        });
        ui.add_space(theme::space::XS);

        let width = ui.available_width().max(300.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(width, height), Sense::hover());
        let rect = response.rect;

        // Arka plan: temanın görünüm zemini
        painter.rect_filled(rect, theme::corner(radius::M), tk.viewport_bg);
        painter.rect_stroke(
            rect,
            theme::corner(radius::M),
            Stroke::new(1.0, tk.border),
            egui::StrokeKind::Inside,
        );

        // Hafif ızgara çizgileri
        draw_grid(&painter, rect);

        match p.drone_type {
            DroneType::Multirotor => draw_multirotor(&painter, rect, p, m),
            DroneType::FixedWingConventional => {
                draw_fixed_wing(&painter, rect, p, m, TailKind::Conventional, false)
            }
            DroneType::FixedWingVTail => {
                draw_fixed_wing(&painter, rect, p, m, TailKind::VTail, false)
            }
            DroneType::FlyingWing => draw_flying_wing(&painter, rect, p, m),
            DroneType::VtolQuadplane => draw_fixed_wing(&painter, rect, p, m, p.tail_kind, true),
        }
    });
}

fn draw_grid(painter: &egui::Painter, rect: Rect) {
    let step = 40.0;
    let stroke = Stroke::new(0.5, tk_of(painter).viewport_grid);
    let mut x = rect.left() + step;
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        x += step;
    }
    let mut y = rect.top() + step;
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
        y += step;
    }
}

// ============================================================================
// DÜNYA -> EKRAN KOORDİNAT DÖNÜŞÜMÜ
// x_w: İleri (+) / Burun yönü
// y_w: Sağa (+) / Sancak yönü
// Ekran: x_s = center.x + y_w * scale, y_s = center.y - x_w * scale
// ============================================================================

struct Viewport {
    center: Pos2,
    scale: f32, // piksel / metre
}

impl Viewport {
    fn to_screen(&self, x_w: f64, y_w: f64) -> Pos2 {
        Pos2::new(
            self.center.x + (y_w as f32) * self.scale,
            self.center.y - (x_w as f32) * self.scale,
        )
    }

    fn to_screen_vec(&self, dx_w: f64, dy_w: f64) -> Vec2 {
        Vec2::new((dy_w as f32) * self.scale, -(dx_w as f32) * self.scale)
    }
}

// ============================================================================
// MULTIROTOR ÇİZİMİ
// ============================================================================

fn draw_multirotor(painter: &egui::Painter, rect: Rect, p: &Project, m: &Model) {
    let d_frame = if p.mr_frame_size > 0.05 {
        p.mr_frame_size
    } else {
        0.250
    };
    let d_prop = if p.mr_d_prop > 0.02 {
        p.mr_d_prop
    } else {
        0.127
    };
    let r_plate = if p.mr_r_plate > 0.01 {
        p.mr_r_plate
    } else {
        0.035
    };

    // Sınırlar: çerçeve + pervane yarıçapı + %25 marj
    let r_total = d_frame * 0.5 + d_prop * 0.5;
    let margin = r_total * 0.30;
    let bound = r_total + margin;

    let scale_x = rect.width() / ((bound * 2.0) as f32);
    let scale_y = rect.height() / ((bound * 2.0) as f32);
    let scale = scale_x.min(scale_y);

    let vp = Viewport {
        center: rect.center(),
        scale,
    };

    let coords = &m.mr.motor_coords;
    let tk = tk_of(painter);

    // 1. Kolların çizimi
    let arm_stroke = Stroke::new(6.0 * (scale / 400.0).clamp(0.6, 2.5), tk.border_strong);
    if p.mr_layout == MultirotorLayout::H && coords.len() == 4 {
        // H düzeni: sol ve sağ boyuna kiriş + orta köprü
        let p_fl = vp.to_screen(coords[0].0, coords[0].1);
        let p_fr = vp.to_screen(coords[1].0, coords[1].1);
        let p_rr = vp.to_screen(coords[2].0, coords[2].1);
        let p_rl = vp.to_screen(coords[3].0, coords[3].1);

        // Sol boyuna kol
        painter.line_segment([p_fl, p_rl], arm_stroke);
        // Sağ boyuna kol
        painter.line_segment([p_fr, p_rr], arm_stroke);
        // Orta köprü
        let mid_l = Pos2::new((p_fl.x + p_rl.x) * 0.5, (p_fl.y + p_rl.y) * 0.5);
        let mid_r = Pos2::new((p_fr.x + p_rr.x) * 0.5, (p_fr.y + p_rr.y) * 0.5);
        painter.line_segment([mid_l, mid_r], arm_stroke);
    } else {
        let center_pos = vp.to_screen(0.0, 0.0);
        for &(mx, my) in coords {
            let m_pos = vp.to_screen(mx, my);
            painter.line_segment([center_pos, m_pos], arm_stroke);
        }
    }

    // 2. Merkez plaka
    let center_pos = vp.to_screen(0.0, 0.0);
    let r_plate_px = (r_plate as f32) * scale;
    painter.circle_filled(center_pos, r_plate_px, body_fill(tk));
    painter.circle_stroke(center_pos, r_plate_px, Stroke::new(1.5, tk.text_weak));

    // 3. Ön Yön Oku (Heading)
    let arrow_len = r_plate * 1.5;
    let arrow_tip = vp.to_screen(arrow_len, 0.0);
    painter.arrow(
        center_pos,
        vp.to_screen_vec(arrow_len, 0.0),
        Stroke::new(2.5, tk.success),
    );
    painter.text(
        Pos2::new(arrow_tip.x, arrow_tip.y - 12.0),
        egui::Align2::CENTER_CENTER,
        t("schematic_front"),
        FontId::proportional(11.0),
        tk.success,
    );

    // 4. Pervaneler ve Motorlar
    let r_prop_px = ((d_prop * 0.5) as f32) * scale;
    let r_motor_px = 6.0 * (scale / 400.0).clamp(0.6, 2.0);

    for (i, &(mx, my)) in coords.iter().enumerate() {
        let m_pos = vp.to_screen(mx, my);

        // Pervane dairesi
        let (prop_fill, prop_stroke) = if m.mr.prop_overlap {
            (tk.danger.gamma_multiply(0.25), Stroke::new(2.0, tk.danger))
        } else {
            (tk.accent.gamma_multiply(0.14), Stroke::new(1.2, tk.accent))
        };

        painter.circle_filled(m_pos, r_prop_px, prop_fill);
        painter.circle_stroke(m_pos, r_prop_px, prop_stroke);

        // Motor gövdesi (Koaksiyelde üst ve alt iki motor iç içe / belirgin)
        if p.mr_coaxial {
            // Alt motor (dış daire)
            painter.circle_filled(m_pos, r_motor_px + 4.0, tk.warning.gamma_multiply(0.6));
            painter.circle_stroke(m_pos, r_motor_px + 4.0, Stroke::new(1.2, tk.warning));
            // Üst motor (iç daire)
            painter.circle_filled(m_pos, r_motor_px, tk.warning);
            painter.circle_stroke(m_pos, r_motor_px, Stroke::new(1.0, tk.bg));

            // Motor numaraları (M_üst / M_alt)
            let m_top = 2 * i + 1;
            let m_bot = 2 * i + 2;
            painter.text(
                Pos2::new(m_pos.x, m_pos.y + r_motor_px + 12.0),
                egui::Align2::CENTER_CENTER,
                format!("M{}/M{}", m_top, m_bot),
                FontId::proportional(10.0),
                tk.text_weak,
            );
        } else {
            painter.circle_filled(m_pos, r_motor_px, tk.warning);
            painter.circle_stroke(m_pos, r_motor_px, Stroke::new(1.0, tk.bg));

            // Motor numarası
            painter.text(
                Pos2::new(m_pos.x, m_pos.y + r_motor_px + 8.0),
                egui::Align2::CENTER_CENTER,
                format!("M{}", i + 1),
                FontId::proportional(10.0),
                tk.text_weak,
            );
        }
    }

    // 5. Ölçü Çizgileri
    // Çerçeve diyagonal / çap ölçüsü (n=3 için karşı motor varsaymadan gerçek çap)
    if !coords.is_empty() {
        let p0 = vp.to_screen(coords[0].0, coords[0].1);
        let p_opp = if coords.len().is_multiple_of(2) {
            let idx_opp = coords.len() / 2;
            vp.to_screen(coords[idx_opp].0, coords[idx_opp].1)
        } else {
            // Tek kollu düzenlerde (n=3 Tricopter / Y6) merkezden geçen tam karşı çember noktası
            vp.to_screen(-coords[0].0, -coords[0].1)
        };
        draw_dim_line(
            painter,
            p0,
            p_opp,
            Vec2::new(20.0, 0.0),
            &format!("D_frame = {:.0} mm", d_frame * 1000.0),
            tk.info,
        );

        // Komşu motor aralığı (Motor 0 ile Motor 1 arası)
        if coords.len() >= 2 {
            let p1 = vp.to_screen(coords[1].0, coords[1].1);
            draw_dim_line(
                painter,
                p0,
                p1,
                Vec2::new(0.0, -22.0),
                &format!("S_adj = {:.1} mm", m.mr.s_adj * 1000.0),
                tk.success,
            );
        }
    }

    // 6. Ağırlık Merkezi (AG) ve Geometrik Merkez
    let geom_pos = vp.to_screen(m.mr.geom_center_x, m.mr.geom_center_y);
    let cg_pos = vp.to_screen(m.mr.cg_x, m.mr.cg_y);

    draw_cg_marker(painter, cg_pos);

    if m.mr.cg_offset_d > 0.001 {
        // AG ile geometrik merkez arası kesikli çizgi
        painter.line_segment([geom_pos, cg_pos], Stroke::new(1.0, tk.warning));
        let mid = Pos2::new((geom_pos.x + cg_pos.x) * 0.5, (geom_pos.y + cg_pos.y) * 0.5);
        painter.text(
            Pos2::new(mid.x + 10.0, mid.y),
            egui::Align2::LEFT_CENTER,
            format!("Δ = {:.1} mm", m.mr.cg_offset_d * 1000.0),
            FontId::proportional(10.0),
            if m.mr.cg_offset_ok {
                tk.text_weak
            } else {
                tk.danger
            },
        );
    }
}

// ============================================================================
// SABİT KANAT & VTOL QUADPLANE ÇİZİMİ
// ============================================================================

fn draw_fixed_wing(
    painter: &egui::Painter,
    rect: Rect,
    p: &Project,
    m: &Model,
    tail_kind: TailKind,
    is_vtol: bool,
) {
    let tk = tk_of(painter);
    let b = if p.b > 0.2 { p.b } else { 1.6 };
    let c_r = if m.wing.c_r > 0.05 { m.wing.c_r } else { 0.25 };
    let c_t = if m.wing.c_t > 0.02 { m.wing.c_t } else { 0.15 };
    let sweep_rad = p.sweep_le_deg.to_radians();
    let x_le_tip = (b * 0.5) * sweep_rad.tan();
    let l_fuse = if p.l_fuse > 0.3 { p.l_fuse } else { 0.90 };
    let d_fuse = if p.d_fuse > 0.03 { p.d_fuse } else { 0.11 };
    let x_root_le = if p.x_root_le > 0.05 {
        p.x_root_le
    } else {
        0.20
    };

    // Sınırlar
    let bound_y = b * 0.5 + 0.15;
    let bound_x_front = x_root_le + 0.15;
    let bound_x_rear = (l_fuse - x_root_le) + 0.20;

    let total_width = bound_y * 2.0;
    let total_length = bound_x_front + bound_x_rear;

    let scale_x = rect.width() / (total_width as f32);
    let scale_y = rect.height() / (total_length as f32);
    let scale = scale_x.min(scale_y);

    let offset_x_center = (bound_x_front - bound_x_rear) * 0.5;
    let vp = Viewport {
        center: Pos2::new(
            rect.center().x,
            rect.center().y + (offset_x_center as f32) * scale,
        ),
        scale,
    };

    // 1. Gövde
    let _nose_pos = vp.to_screen(x_root_le, 0.0);
    let tail_pos = vp.to_screen(x_root_le - l_fuse, 0.0);
    let fuse_w_px = (d_fuse as f32) * scale * 0.5;

    // Gövde poligonu
    let fuse_pts = vec![
        vp.to_screen(x_root_le, 0.0),
        vp.to_screen(x_root_le - 0.15 * l_fuse, d_fuse * 0.5),
        vp.to_screen(x_root_le - 0.70 * l_fuse, d_fuse * 0.45),
        vp.to_screen(x_root_le - l_fuse, d_fuse * 0.15),
        vp.to_screen(x_root_le - l_fuse, -d_fuse * 0.15),
        vp.to_screen(x_root_le - 0.70 * l_fuse, -d_fuse * 0.45),
        vp.to_screen(x_root_le - 0.15 * l_fuse, -d_fuse * 0.5),
    ];
    painter.add(PathShape::convex_polygon(
        fuse_pts,
        body_fill(tk),
        Stroke::new(1.2, tk.text_weak),
    ));

    // 2. Kanat Planformu
    let half_b = b * 0.5;
    // Sağ kanat
    let r_wing = vec![
        vp.to_screen(0.0, fuse_w_px as f64 / scale as f64),
        vp.to_screen(-x_le_tip, half_b),
        vp.to_screen(-x_le_tip - c_t, half_b),
        vp.to_screen(-c_r, fuse_w_px as f64 / scale as f64),
    ];
    // Sol kanat
    let l_wing = vec![
        vp.to_screen(0.0, -fuse_w_px as f64 / scale as f64),
        vp.to_screen(-x_le_tip, -half_b),
        vp.to_screen(-x_le_tip - c_t, -half_b),
        vp.to_screen(-c_r, -fuse_w_px as f64 / scale as f64),
    ];

    let wing_fill = tk.accent_soft;
    let wing_stroke = Stroke::new(1.5, tk.accent);
    painter.add(PathShape::convex_polygon(r_wing, wing_fill, wing_stroke));
    painter.add(PathShape::convex_polygon(l_wing, wing_fill, wing_stroke));

    // 4. MAC Çizgisi (Mean Aerodynamic Chord)
    let y_mac = m.wing.y_mac;
    let x_mac_le = -m.wing.x_le_mac;
    let mac = m.wing.mac;
    painter.line_segment(
        [
            vp.to_screen(x_mac_le, y_mac),
            vp.to_screen(x_mac_le - mac, y_mac),
        ],
        Stroke::new(1.8, tk.success),
    );
    painter.text(
        vp.to_screen(x_mac_le - mac * 0.5, y_mac + 0.04),
        egui::Align2::LEFT_CENTER,
        t("schematic_mac"),
        FontId::proportional(10.0),
        tk.success,
    );

    // 5. Kuyruk
    let tail_x = -p.l_h;
    let tail_fill = tk.accent_soft;
    let tail_stroke = Stroke::new(1.2, tk.info);

    match tail_kind {
        TailKind::Conventional => {
            let b_h = (p.s_h * 4.0).sqrt().max(0.20);
            let c_h = p.s_h / b_h;
            let half_bh = b_h * 0.5;
            let h_pts = vec![
                vp.to_screen(tail_x, half_bh),
                vp.to_screen(tail_x - c_h, half_bh),
                vp.to_screen(tail_x - c_h, -half_bh),
                vp.to_screen(tail_x, -half_bh),
            ];
            painter.add(PathShape::convex_polygon(h_pts, tail_fill, tail_stroke));

            // Dikey stabilize merkezi omurga
            let fin_pts = [
                vp.to_screen(tail_x + 0.05, 0.0),
                vp.to_screen(tail_x - c_h - 0.02, 0.0),
            ];
            painter.line_segment([fin_pts[0], fin_pts[1]], Stroke::new(3.0, tk.info));
        }
        TailKind::VTail => {
            let b_v_proj = (p.s_vtail * 3.5).sqrt().max(0.22) * p.gamma_v_deg.to_radians().cos();
            let c_vt = p.s_vtail / (b_v_proj * 1.2);
            let half_bv = b_v_proj * 0.5;

            let r_vtail = vec![
                vp.to_screen(tail_x, 0.0),
                vp.to_screen(tail_x - 0.06, half_bv),
                vp.to_screen(tail_x - 0.06 - c_vt, half_bv),
                vp.to_screen(tail_x - c_vt, 0.0),
            ];
            let l_vtail = vec![
                vp.to_screen(tail_x, 0.0),
                vp.to_screen(tail_x - 0.06, -half_bv),
                vp.to_screen(tail_x - 0.06 - c_vt, -half_bv),
                vp.to_screen(tail_x - c_vt, 0.0),
            ];
            painter.add(PathShape::convex_polygon(r_vtail, tail_fill, tail_stroke));
            painter.add(PathShape::convex_polygon(l_vtail, tail_fill, tail_stroke));
        }
        TailKind::FlyingWing => {}
    }

    // 6. VTOL Ekstraları: İki Motor Bomu + 4 Dikey Motor + 1 İtici Motor
    if is_vtol {
        let boom_y = half_b * 0.38;
        let boom_front_x = 0.15;
        let boom_rear_x = -c_r - 0.25;

        let boom_stroke = Stroke::new(4.0, tk.border_strong);
        // Sağ bom
        painter.line_segment(
            [
                vp.to_screen(boom_front_x, boom_y),
                vp.to_screen(boom_rear_x, boom_y),
            ],
            boom_stroke,
        );
        // Sol bom
        painter.line_segment(
            [
                vp.to_screen(boom_front_x, -boom_y),
                vp.to_screen(boom_rear_x, -boom_y),
            ],
            boom_stroke,
        );

        // 4 Dikey Kaldırma Motoru
        let vtol_m_pts = [
            (boom_front_x, boom_y),
            (boom_rear_x, boom_y),
            (boom_front_x, -boom_y),
            (boom_rear_x, -boom_y),
        ];

        let r_vtol_prop_px = ((p.mr_d_prop * 0.5) as f32) * scale;
        for &(mx, my) in &vtol_m_pts {
            let m_pos = vp.to_screen(mx, my);
            painter.circle_filled(m_pos, r_vtol_prop_px, tk.accent.gamma_multiply(0.16));
            painter.circle_stroke(m_pos, r_vtol_prop_px, Stroke::new(1.2, tk.accent));
            painter.circle_filled(m_pos, 5.0, tk.warning);
        }

        // İtici motor (gövde arkasında)
        let pusher_pos = vp.to_screen(tail_pos.x as f64 / scale as f64, 0.0);
        painter.circle_filled(pusher_pos, 4.0, tk.danger);
    }

    // 7. AG ve NP Noktaları
    // m.stab.cg_nose.x burun datumunda. Kanat kök hücum kenarı datumuna çevirelim:
    let x_cg_w = x_root_le - m.stab.cg_nose.x;
    let y_cg_w = m.stab.cg_nose.y;
    let x_np_w = x_root_le - m.stab.x_np_nose;

    let cg_screen = vp.to_screen(x_cg_w, y_cg_w);
    let np_screen = vp.to_screen(x_np_w, 0.0);

    draw_cg_marker(painter, cg_screen);
    draw_np_marker(painter, np_screen);

    // 8. Ölçü Çizgileri
    // Kanat açıklığı
    let left_tip = vp.to_screen(-x_le_tip, -half_b);
    let right_tip = vp.to_screen(-x_le_tip, half_b);
    draw_dim_line(
        painter,
        left_tip,
        right_tip,
        Vec2::new(0.0, -25.0),
        &format!("b = {:.0} mm", b * 1000.0),
        tk.info,
    );

    // Kök veter
    let root_le = vp.to_screen(0.0, 0.0);
    let root_te = vp.to_screen(-c_r, 0.0);
    draw_dim_line(
        painter,
        root_le,
        root_te,
        Vec2::new(18.0, 0.0),
        &format!("c_r = {:.0} mm", c_r * 1000.0),
        tk.success,
    );
}

// ============================================================================
// UÇAN KANAT ÇİZİMİ
// ============================================================================

fn draw_flying_wing(painter: &egui::Painter, rect: Rect, p: &Project, m: &Model) {
    let tk = tk_of(painter);
    let b = if p.b > 0.2 { p.b } else { 1.2 };
    let c_r = if m.wing.c_r > 0.05 { m.wing.c_r } else { 0.28 };
    let c_t = if m.wing.c_t > 0.02 { m.wing.c_t } else { 0.14 };
    let sweep_rad = p.sweep_le_deg.to_radians();
    let x_le_tip = (b * 0.5) * sweep_rad.tan();

    let bound_y = b * 0.5 + 0.15;
    let bound_x = x_le_tip + c_r + 0.15;

    let scale_x = rect.width() / ((bound_y * 2.0) as f32);
    let scale_y = rect.height() / (bound_x as f32);
    let scale = scale_x.min(scale_y);

    let vp = Viewport {
        center: Pos2::new(
            rect.center().x,
            rect.center().y - (x_le_tip * 0.35_f64) as f32 * scale,
        ),
        scale,
    };

    let half_b = b * 0.5;

    // Kanat gövdesi
    let wing_pts = vec![
        vp.to_screen(0.0, 0.0),
        vp.to_screen(-x_le_tip, half_b),
        vp.to_screen(-x_le_tip - c_t, half_b),
        vp.to_screen(-c_r, 0.0),
        vp.to_screen(-x_le_tip - c_t, -half_b),
        vp.to_screen(-x_le_tip, -half_b),
    ];
    painter.add(PathShape::convex_polygon(
        wing_pts,
        tk.accent_soft,
        Stroke::new(1.5, tk.accent),
    ));

    // Elevonlar (Firar kenarında outer %70)
    let el_in_y = half_b * 0.30;
    let el_in_te = -(x_le_tip * 0.30) - (c_r + (c_t - c_r) * 0.30);
    let el_chord = 0.035;

    let el_r = vec![
        vp.to_screen(el_in_te, el_in_y),
        vp.to_screen(-x_le_tip - c_t, half_b),
        vp.to_screen(-x_le_tip - c_t + el_chord, half_b),
        vp.to_screen(el_in_te + el_chord, el_in_y),
    ];
    let el_l = vec![
        vp.to_screen(el_in_te, -el_in_y),
        vp.to_screen(-x_le_tip - c_t, -half_b),
        vp.to_screen(-x_le_tip - c_t + el_chord, -half_b),
        vp.to_screen(el_in_te + el_chord, -el_in_y),
    ];
    let el_stroke = Stroke::new(1.0, tk.warning);
    painter.add(PathShape::convex_polygon(el_r, body_fill(tk), el_stroke));
    painter.add(PathShape::convex_polygon(el_l, body_fill(tk), el_stroke));

    // Kanat ucu winglet'leri
    let winglet_len = c_t * 1.1;
    painter.line_segment(
        [
            vp.to_screen(-x_le_tip + 0.02, half_b),
            vp.to_screen(-x_le_tip - winglet_len, half_b),
        ],
        Stroke::new(3.5, tk.warning),
    );
    painter.line_segment(
        [
            vp.to_screen(-x_le_tip + 0.02, -half_b),
            vp.to_screen(-x_le_tip - winglet_len, -half_b),
        ],
        Stroke::new(3.5, tk.warning),
    );

    // MAC, AG ve NP
    let y_mac = m.wing.y_mac;
    let x_mac_le = -m.wing.x_le_mac;
    let mac = m.wing.mac;
    painter.line_segment(
        [
            vp.to_screen(x_mac_le, y_mac),
            vp.to_screen(x_mac_le - mac, y_mac),
        ],
        Stroke::new(1.5, tk.success),
    );

    let x_cg_w = -m.stab.x_cg_rel;
    let x_np_w = -m.stab.np.x_np;

    draw_cg_marker(painter, vp.to_screen(x_cg_w, 0.0));
    draw_np_marker(painter, vp.to_screen(x_np_w, 0.0));

    // Açıklık ölçüsü
    let left_tip = vp.to_screen(-x_le_tip, -half_b);
    let right_tip = vp.to_screen(-x_le_tip, half_b);
    draw_dim_line(
        painter,
        left_tip,
        right_tip,
        Vec2::new(0.0, -25.0),
        &format!("b = {:.0} mm", b * 1000.0),
        tk.info,
    );
}

// ============================================================================
// YARDIMCI İŞARETÇİ VE ÖLÇÜ ÇİZGİLERİ
// ============================================================================

fn draw_dim_line(
    painter: &egui::Painter,
    p1: Pos2,
    p2: Pos2,
    offset: Vec2,
    label: &str,
    color: Color32,
) {
    let stroke = Stroke::new(1.0, color);
    let p1_off = Pos2::new(p1.x + offset.x, p1.y + offset.y);
    let p2_off = Pos2::new(p2.x + offset.x, p2.y + offset.y);

    // Uzatma çizgileri
    painter.line_segment(
        [p1, p1_off],
        Stroke::new(
            0.6,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 100),
        ),
    );
    painter.line_segment(
        [p2, p2_off],
        Stroke::new(
            0.6,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 100),
        ),
    );

    // Ana ölçü çizgisi
    painter.line_segment([p1_off, p2_off], stroke);

    // Uç okları veya kene çizgileri
    let dir = (p2_off - p1_off).normalized();
    let norm = Vec2::new(-dir.y, dir.x);
    let tick = 4.0;
    painter.line_segment([p1_off - norm * tick, p1_off + norm * tick], stroke);
    painter.line_segment([p2_off - norm * tick, p2_off + norm * tick], stroke);

    // Metin etiketi
    let mid = Pos2::new((p1_off.x + p2_off.x) * 0.5, (p1_off.y + p2_off.y) * 0.5);
    let text_pos = mid + norm * 8.0;
    painter.text(
        text_pos,
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        color,
    );
}

fn draw_cg_marker(painter: &egui::Painter, pos: Pos2) {
    let tk = tk_of(painter);
    let r = 8.0;
    // Dolu daire (uyarı rengi) ve zemin renkli çerçeve
    painter.circle_filled(pos, r, tk.warning);
    painter.circle_stroke(pos, r, Stroke::new(1.2, tk.bg));

    // Çapraz çizgiler
    painter.line_segment(
        [
            Pos2::new(pos.x - r - 3.0, pos.y),
            Pos2::new(pos.x + r + 3.0, pos.y),
        ],
        Stroke::new(1.2, tk.text),
    );
    painter.line_segment(
        [
            Pos2::new(pos.x, pos.y - r - 3.0),
            Pos2::new(pos.x, pos.y + r + 3.0),
        ],
        Stroke::new(1.2, tk.text),
    );

    painter.text(
        Pos2::new(pos.x + r + 4.0, pos.y - r - 2.0),
        egui::Align2::LEFT_BOTTOM,
        t("schematic_cg"),
        FontId::proportional(10.0),
        tk.warning,
    );
}

fn draw_np_marker(painter: &egui::Painter, pos: Pos2) {
    let tk = tk_of(painter);
    let r = 6.0;
    painter.circle_filled(pos, r, tk.accent);
    painter.circle_stroke(pos, r, Stroke::new(1.2, tk.bg));

    painter.text(
        Pos2::new(pos.x + r + 4.0, pos.y + r + 2.0),
        egui::Align2::LEFT_TOP,
        t("schematic_np"),
        FontId::proportional(10.0),
        tk.accent,
    );
}
