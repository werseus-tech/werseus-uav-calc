// SPDX-License-Identifier: AGPL-3.0-or-later
//! Multirotor ve VTOL dikey kaldırma hesaplama modülü (saf fonksiyonlar, SI birimleri).
//! Formül spesifikasyonu: docs/formulas/multirotor.md.
//! Kaynaklar: Leishman (2006), Bresciani (2008), Quan (2017).

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub const G: f64 = 9.80665;
pub const RHO0: f64 = 1.2250;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MultirotorLayout {
    #[default]
    X,
    Plus,
    H,
}

impl MultirotorLayout {
    pub const ALL: [MultirotorLayout; 3] = [
        MultirotorLayout::X,
        MultirotorLayout::Plus,
        MultirotorLayout::H,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            MultirotorLayout::X => crate::i18n::t("mr.layout.x"),
            MultirotorLayout::Plus => crate::i18n::t("mr.layout.plus"),
            MultirotorLayout::H => crate::i18n::t("mr.layout.h"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultirotorIn {
    pub d_frame: f64,
    pub arm_count: u32,
    pub layout: MultirotorLayout,
    pub coaxial: bool,
    pub prop_clearance: f64,
    pub d_prop: f64,
    pub r_plate: f64,
    pub m_frame: f64,
    pub m_motor: f64,
    pub m_esc_tot: f64,
    pub m_fc: f64,
    pub m_battery: f64,
    pub m_payload: f64,
    pub m_misc: f64,
    pub pack_wh: f64,
    pub dod: f64,
    pub pack_voltage: f64,
    pub pack_max_cont_a: f64,
    pub t_max_motor: f64,
    pub fm: f64,
    pub eta_motor: f64,
    pub eta_esc: f64,
    pub profile_power_ratio: f64,
    pub coax_kappa: f64,
    pub h_lx: f64,
    pub h_wy: f64,
    pub flat_plate_area: f64,
    pub p_avionics: f64,
    pub v_cruise: f64,
    pub rho: f64,
    pub comps_mass_x_y: Vec<(f64, f64, f64)>,
}

#[derive(Clone, Debug, Default)]
pub struct MultirotorOut {
    pub s_adj: f64,
    pub d_prop_max: f64,
    pub d_prop_max_inch: f64,
    pub l_arm: f64,
    pub n_motors: u32,
    pub n_eff: f64,
    /// Kullanılan koaksiyel girişim çarpanı (koaksiyel değilse 1.0).
    pub coax_kappa: f64,
    pub m_mtow: f64,
    pub w: f64,
    pub t_hover_motor: f64,
    pub t_hover_total: f64,
    pub v_i: f64,
    pub a_disk_single: f64,
    pub a_disk_total: f64,
    pub p_ideal_motor: f64,
    pub p_ideal_total: f64,
    pub p_shaft_motor: f64,
    pub p_shaft_total: f64,
    pub p_elec_motor: f64,
    pub p_elec_hover_total: f64,
    pub disk_loading: f64,
    pub t_max_total: f64,
    pub tw_ratio: f64,
    pub hover_throttle: f64,
    pub e_usable_wh: f64,
    pub p_total_hover: f64,
    pub hover_endurance_min: f64,
    pub p_total_cruise: f64,
    pub p_total_cruise_85: f64,
    pub cruise_endurance_min: f64,
    pub range_km: f64,
    pub v_best_range: f64,
    pub range_best_km: f64,
    pub cg_x: f64,
    pub cg_y: f64,
    pub geom_center_x: f64,
    pub geom_center_y: f64,
    pub cg_offset_d: f64,
    pub cg_offset_frac: f64,
    pub cg_offset_ok: bool,
    pub prop_overlap: bool,
    pub prop_clearance_ok: bool,
    pub i_hover: f64,
    pub i_hover_ok: bool,
    pub motor_coords: Vec<(f64, f64)>,
}

fn safe_div(n: f64, d: f64) -> f64 {
    if !n.is_finite() || !d.is_finite() || d.abs() < 1e-12 {
        0.0
    } else {
        n / d
    }
}

/// Komşu motor aralığı: D_frame * sin(pi / n).
/// H düzeni için: Lx ve Wy verilmişse min(Lx, Wy), değilse kare varsayımıyla D_frame / sqrt(2).
pub fn adjacent_spacing(
    d_frame: f64,
    arm_count: u32,
    layout: MultirotorLayout,
    h_lx: f64,
    h_wy: f64,
) -> f64 {
    if !d_frame.is_finite() || d_frame <= 0.0 || arm_count < 3 {
        return 0.0;
    }
    match layout {
        MultirotorLayout::H if arm_count == 4 => {
            if h_lx > 0.0 && h_wy > 0.0 {
                h_lx.min(h_wy)
            } else {
                d_frame / std::f64::consts::SQRT_2
            }
        }
        _ => d_frame * (PI / (arm_count as f64)).sin(),
    }
}

/// Azami pervane çapı = S_adj - clearance.
pub fn max_prop_diameter(s_adj: f64, clearance: f64) -> f64 {
    (s_adj - clearance).max(0.0)
}

/// Kol uzunluğu: merkez plaka kenarından motor eksenine.
pub fn arm_length(d_frame: f64, r_plate: f64) -> f64 {
    (d_frame * 0.5 - r_plate).max(0.0)
}

/// Motorların koordinatları (x: ileri +, y: sağ +).
pub fn motor_positions(
    d_frame: f64,
    arm_count: u32,
    layout: MultirotorLayout,
    h_lx: f64,
    h_wy: f64,
) -> Vec<(f64, f64)> {
    if !d_frame.is_finite() || d_frame <= 0.0 || arm_count == 0 {
        return Vec::new();
    }
    let r = d_frame * 0.5;
    let mut coords = Vec::with_capacity(arm_count as usize);

    if layout == MultirotorLayout::H && arm_count == 4 {
        // H düzeni: sol ve sağ iki boyuna kiriş, ortada bağlantı köprüsü
        let (half_x, half_y) = if h_lx > 0.0 && h_wy > 0.0 {
            (h_lx * 0.5, h_wy * 0.5)
        } else {
            let half = r / std::f64::consts::SQRT_2;
            (half, half)
        };
        coords.push((half_x, -half_y)); // Ön Sol
        coords.push((half_x, half_y)); // Ön Sağ
        coords.push((-half_x, half_y)); // Arka Sağ
        coords.push((-half_x, -half_y)); // Arka Sol
        return coords;
    }

    // Açı ofseti: X düzeni için eksenler arası 45°, Plus için 0° (önde kol var)
    let base_offset = match layout {
        MultirotorLayout::X => {
            // n=4 ise kollar 45, 135, 225, 315 derece
            PI / (arm_count as f64)
        }
        MultirotorLayout::Plus => 0.0,
        MultirotorLayout::H => PI / (arm_count as f64),
    };

    for i in 0..arm_count {
        let angle = base_offset + (i as f64) * (2.0 * PI / (arm_count as f64));
        let x = r * angle.cos();
        let y = r * angle.sin();
        coords.push((x, y));
    }
    coords
}

pub fn evaluate_multirotor(inp: &MultirotorIn) -> MultirotorOut {
    let arm_cnt = match inp.arm_count {
        3 | 4 | 6 | 8 => inp.arm_count,
        n if n < 4 => 3,
        n if n < 6 => 4,
        n if n < 8 => 6,
        _ => 8,
    };
    let n_motors = if inp.coaxial { arm_cnt * 2 } else { arm_cnt };
    let coax_kappa = if inp.coaxial {
        if inp.coax_kappa > 0.0 {
            inp.coax_kappa.clamp(1.0, 2.5)
        } else {
            1.28
        }
    } else {
        1.0
    };
    let n_eff = if inp.coaxial {
        (n_motors as f64) / coax_kappa
    } else {
        n_motors as f64
    };

    let s_adj = adjacent_spacing(inp.d_frame, arm_cnt, inp.layout, inp.h_lx, inp.h_wy);
    let d_prop_max = max_prop_diameter(s_adj, inp.prop_clearance);
    let d_prop_max_inch = d_prop_max / 0.0254;
    let l_arm = arm_length(inp.d_frame, inp.r_plate);

    // Kütle toplamı
    let m_mtow = inp.m_frame
        + (n_motors as f64) * inp.m_motor
        + inp.m_esc_tot
        + inp.m_fc
        + inp.m_battery
        + inp.m_payload
        + inp.m_misc;
    let w = m_mtow * G;

    // Momentum teorisi askı hesabı (kol başına tek disk alanı, koaksiyelde κ_int çarpanı)
    let t_hover_total = w;
    let t_arm = safe_div(w, arm_cnt as f64);
    let t_hover_motor = safe_div(w, n_motors as f64);

    let d_prop = inp.d_prop.max(0.001);
    let a_disk_single = 0.25 * PI * d_prop * d_prop;
    let a_disk_total = (arm_cnt as f64) * a_disk_single;

    let rho = if inp.rho > 0.1 { inp.rho } else { RHO0 };
    let denom_vi = 2.0 * rho * a_disk_single;

    // İdeal güç:
    // Koaksiyel olmayan: P_ideal_rotor = T_arm^1.5 / sqrt(2 ρ A)
    // Koaksiyel çift: P_ideal_çift = T_arm^1.5 / sqrt(2 ρ A) * κ_int
    let p_ideal_arm = if denom_vi > 0.0 && t_arm > 0.0 {
        (t_arm.powf(1.5) / denom_vi.sqrt()) * (if inp.coaxial { coax_kappa } else { 1.0 })
    } else {
        0.0
    };
    let p_ideal_total = (arm_cnt as f64) * p_ideal_arm;
    let p_ideal_motor = safe_div(p_ideal_total, n_motors as f64);

    let v_i = if denom_vi > 0.0 && t_hover_motor > 0.0 {
        (t_hover_motor / denom_vi).sqrt()
    } else {
        0.0
    };

    let fm = inp.fm.clamp(0.1, 1.0);
    let p_shaft_total = safe_div(p_ideal_total, fm);
    let p_shaft_motor = safe_div(p_shaft_total, n_motors as f64);

    let eta_mot = inp.eta_motor.clamp(0.1, 1.0);
    let eta_esc = inp.eta_esc.clamp(0.1, 1.0);
    let eta_elec = eta_mot * eta_esc;
    let p_elec_hover_total = safe_div(p_shaft_total, eta_elec);
    let p_elec_motor = safe_div(p_elec_hover_total, n_motors as f64);

    let disk_loading = safe_div(w, a_disk_total);

    // Azami itki ve T/W
    let t_max_motor = inp.t_max_motor.max(0.0);
    let t_max_total = (n_motors as f64) * t_max_motor;
    let tw_ratio = safe_div(t_max_total, w);
    // İtki ≈ ω² ile ölçeklendiği için throttle_hover ≈ sqrt(T_hover / T_max) = sqrt(1/TW)
    let hover_throttle = if tw_ratio > 0.0 {
        (1.0 / tw_ratio).sqrt()
    } else {
        0.0
    };

    // Batarya ve havada kalma
    let dod = inp.dod.clamp(0.1, 1.0);
    let e_usable_wh = inp.pack_wh * dod;
    let p_avionics = inp.p_avionics.max(0.0);
    let p_total_hover = p_elec_hover_total + p_avionics;
    let hover_endurance_hours = safe_div(e_usable_wh, p_total_hover);
    let hover_endurance_min = hover_endurance_hours * 60.0;

    // İleri uçuş güç-hız modeli (Glauert indüklenmiş hız çözümü + profil gücü + parazit sürtünme)
    let f_area = if inp.flat_plate_area > 0.0 {
        inp.flat_plate_area
    } else {
        0.01
    };
    let v_cr = inp.v_cruise.max(0.0);
    let p_profile = p_shaft_total * inp.profile_power_ratio.max(0.0);

    let calc_power_v = |v: f64| -> (f64, f64) {
        let d_parasite = 0.5 * rho * v * v * f_area;
        let t_req = (w * w + d_parasite * d_parasite).sqrt();
        let v_i0_sq = if a_disk_total > 0.0 && rho > 0.0 {
            t_req / (2.0 * rho * a_disk_total)
        } else {
            0.0
        };
        let mut vi_v = v_i0_sq.sqrt();
        for _ in 0..8 {
            let v_eff = (v * v + vi_v * vi_v).sqrt().max(1e-4);
            vi_v = v_i0_sq / v_eff;
        }
        let p_ind = t_req * vi_v * (if inp.coaxial { coax_kappa } else { 1.0 });
        let p_par = d_parasite * v;
        let p_sh = safe_div(p_ind, fm) + p_profile + p_par;
        let p_el = safe_div(p_sh, eta_elec) + p_avionics;
        (p_sh, p_el)
    };

    let (_, p_cruise_glauert) = calc_power_v(v_cr);
    let p_total_cruise_85 = 0.85 * p_elec_hover_total + p_avionics;
    let p_total_cruise = p_cruise_glauert;
    let cruise_endurance_hours = safe_div(e_usable_wh, p_total_cruise);
    let cruise_endurance_min = cruise_endurance_hours * 60.0;
    let range_km = v_cr * cruise_endurance_hours * 3.6;

    // Sayısal optimum menzil hızı arama: V in [5, 35] m/s
    let mut best_range_km = range_km;
    let mut v_best_range = v_cr;
    let mut step_v = 5.0;
    while step_v <= 35.0 {
        let (_, p_el) = calc_power_v(step_v);
        let r_km = step_v * safe_div(e_usable_wh, p_el) * 3.6;
        if r_km > best_range_km {
            best_range_km = r_km;
            v_best_range = step_v;
        }
        step_v += 0.5;
    }

    // Ağırlık merkezi & motor geometrisi (Datum = çerçeve merkezi)
    let motor_coords = motor_positions(inp.d_frame, arm_cnt, inp.layout, inp.h_lx, inp.h_wy);
    let geom_center_x = if !motor_coords.is_empty() {
        motor_coords.iter().map(|&(x, _)| x).sum::<f64>() / (motor_coords.len() as f64)
    } else {
        0.0
    };
    let geom_center_y = if !motor_coords.is_empty() {
        motor_coords.iter().map(|&(_, y)| y).sum::<f64>() / (motor_coords.len() as f64)
    } else {
        0.0
    };

    // AG kütle listesi: kullanıcı bileşenleri + çerçeve (0,0) + motorlar (kol koordinatlarında)
    let mut all_cg_comps = inp.comps_mass_x_y.clone();
    if inp.m_frame > 0.0 {
        all_cg_comps.push((inp.m_frame, 0.0, 0.0));
    }
    if inp.m_motor > 0.0 {
        let m_per_arm = if inp.coaxial {
            2.0 * inp.m_motor
        } else {
            inp.m_motor
        };
        for &(mx, my) in &motor_coords {
            all_cg_comps.push((m_per_arm, mx, my));
        }
    }

    let total_comp_mass: f64 = all_cg_comps.iter().map(|(m, _, _)| *m).sum();
    let (cg_x, cg_y) = if total_comp_mass > 0.0 {
        let mx: f64 = all_cg_comps.iter().map(|(m, x, _)| m * x).sum();
        let my: f64 = all_cg_comps.iter().map(|(m, _, y)| m * y).sum();
        (mx / total_comp_mass, my / total_comp_mass)
    } else {
        (geom_center_x, geom_center_y)
    };

    let dx = cg_x - geom_center_x;
    let dy = cg_y - geom_center_y;
    let cg_offset_d = (dx * dx + dy * dy).sqrt();
    let cg_offset_frac = safe_div(cg_offset_d, inp.d_frame);
    let cg_offset_ok = cg_offset_frac <= 0.03;
    let prop_overlap = d_prop > s_adj;
    let prop_clearance_ok = d_prop <= d_prop_max;

    let v_pack = inp.pack_voltage.max(0.1);
    let i_hover = safe_div(p_elec_hover_total, v_pack);
    let i_hover_ok = inp.pack_max_cont_a <= 0.0 || i_hover <= inp.pack_max_cont_a;

    MultirotorOut {
        s_adj,
        d_prop_max,
        d_prop_max_inch,
        l_arm,
        n_motors,
        n_eff,
        coax_kappa,
        m_mtow,
        w,
        t_hover_motor,
        t_hover_total,
        v_i,
        a_disk_single,
        a_disk_total,
        p_ideal_motor,
        p_ideal_total,
        p_shaft_motor,
        p_shaft_total,
        p_elec_motor,
        p_elec_hover_total,
        disk_loading,
        t_max_total,
        tw_ratio,
        hover_throttle,
        e_usable_wh,
        p_total_hover,
        hover_endurance_min,
        p_total_cruise,
        p_total_cruise_85,
        cruise_endurance_min,
        range_km,
        v_best_range,
        range_best_km: best_range_km,
        cg_x,
        cg_y,
        geom_center_x,
        geom_center_y,
        cg_offset_d,
        cg_offset_frac,
        cg_offset_ok,
        prop_overlap,
        prop_clearance_ok,
        i_hover,
        i_hover_ok,
        motor_coords,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_250mm_quad_x_spacing() {
        // 250 mm quad X için komşu aralık = 176.8 mm
        let s = adjacent_spacing(0.250, 4, MultirotorLayout::X, 0.0, 0.0);
        let s_mm = s * 1000.0;
        assert!(
            (s_mm - 176.777).abs() < 0.05,
            "Beklenen 176.8 mm, bulunan {:.2}",
            s_mm
        );
    }

    #[test]
    fn test_275mm_quad_x_spacing() {
        // 275 mm quad X için komşu aralık = 194.5 mm
        let s = adjacent_spacing(0.275, 4, MultirotorLayout::X, 0.0, 0.0);
        let s_mm = s * 1000.0;
        assert!(
            (s_mm - 194.454).abs() < 0.05,
            "Beklenen 194.5 mm, bulunan {:.2}",
            s_mm
        );
    }

    #[test]
    fn test_h_layout_spacing_and_positions() {
        // H düzeni: Lx = 200 mm, Wy = 160 mm
        let s = adjacent_spacing(0.250, 4, MultirotorLayout::H, 0.200, 0.160);
        assert!(
            (s - 0.160).abs() < 1e-4,
            "H düzeninde komşu aralık min(Lx, Wy) olmalı: {}",
            s
        );

        let coords = motor_positions(0.250, 4, MultirotorLayout::H, 0.200, 0.160);
        assert_eq!(coords.len(), 4);
        assert!((coords[0].0 - 0.100).abs() < 1e-4);
        assert!((coords[0].1 - (-0.080)).abs() < 1e-4);
        assert!((coords[1].0 - 0.100).abs() < 1e-4);
        assert!((coords[1].1 - 0.080).abs() < 1e-4);
    }

    #[test]
    fn test_coaxial_kappa_power_scaling() {
        let base_inp = MultirotorIn {
            d_frame: 0.250,
            arm_count: 4,
            layout: MultirotorLayout::X,
            coaxial: false,
            prop_clearance: 0.010,
            d_prop: 0.1270,
            r_plate: 0.035,
            m_frame: 0.120,
            m_motor: 0.030,
            m_esc_tot: 0.030,
            m_fc: 0.040,
            m_battery: 0.320,
            m_payload: 0.300,
            m_misc: 0.070,
            pack_wh: 60.48,
            dod: 0.80,
            pack_voltage: 14.4,
            pack_max_cont_a: 45.0,
            t_max_motor: 7.5,
            fm: 0.65,
            eta_motor: 0.85,
            eta_esc: 0.95,
            profile_power_ratio: 0.15,
            coax_kappa: 1.28,
            h_lx: 0.0,
            h_wy: 0.0,
            flat_plate_area: 0.01,
            p_avionics: 5.0,
            v_cruise: 12.0,
            rho: 1.2250,
            comps_mass_x_y: vec![(0.320, 0.0, 0.0)],
        };
        let out_single = evaluate_multirotor(&base_inp);

        let mut coax_inp = base_inp.clone();
        coax_inp.coaxial = true;
        coax_inp.coax_kappa = 1.28;
        let out_coax = evaluate_multirotor(&coax_inp);

        assert_eq!(out_coax.n_motors, 8);
        // Kol başına tek disk alanı: a_disk_total aynı kalmalı
        assert!((out_coax.a_disk_total - out_single.a_disk_total).abs() < 1e-5);
        // İdeal güç kappa_int (1.28) ile ölçeklenmeli (aynı W durumunda)
        // Coaxial'da ilave 4 motor kütlesi eklenir (4 * 0.030 = 0.120 kg), bu yüzden W biraz artar.
        assert!(out_coax.p_ideal_total > out_single.p_ideal_total);
    }

    #[test]
    fn test_mr_cg_with_components_and_motors() {
        let inp = MultirotorIn {
            d_frame: 0.250,
            arm_count: 4,
            layout: MultirotorLayout::X,
            coaxial: false,
            prop_clearance: 0.010,
            d_prop: 0.1270,
            r_plate: 0.035,
            m_frame: 0.120,
            m_motor: 0.030,
            m_esc_tot: 0.030,
            m_fc: 0.040,
            m_battery: 0.320,
            m_payload: 0.300,
            m_misc: 0.070,
            pack_wh: 60.48,
            dod: 0.80,
            pack_voltage: 14.4,
            pack_max_cont_a: 45.0,
            t_max_motor: 7.5,
            fm: 0.65,
            eta_motor: 0.85,
            eta_esc: 0.95,
            profile_power_ratio: 0.15,
            coax_kappa: 1.28,
            h_lx: 0.0,
            h_wy: 0.0,
            flat_plate_area: 0.01,
            p_avionics: 5.0,
            v_cruise: 12.0,
            rho: 1.2250,
            comps_mass_x_y: vec![
                (0.320, 0.0, 0.0), // Pil merkezde
                (0.040, 0.0, 0.0), // FC merkezde
                (0.300, 0.0, 0.0), // Payload merkezde
                (0.030, 0.0, 0.0), // ESC merkezde
                (0.020, 0.0, 0.0), // Alıcı merkezde
            ],
        };
        let out = evaluate_multirotor(&inp);
        // Simetrik bileşenler ve motorlarla AG tam merkezde (0,0) olmalı, %3 ofset uyarısı vermemeli
        assert!(
            out.cg_offset_d < 1e-4,
            "Simetrik tasarımda CG ofseti 0 olmalı: {}",
            out.cg_offset_d
        );
        assert!(out.cg_offset_ok);
    }

    #[test]
    fn test_momentum_theory_worked_example() {
        // docs/formulas/multirotor.md Bölüm 8.3 çözümlü örnek vektörü
        let inp = MultirotorIn {
            d_frame: 0.250,
            arm_count: 4,
            layout: MultirotorLayout::X,
            coaxial: false,
            prop_clearance: 0.010,
            d_prop: 0.1270, // 5 inç
            r_plate: 0.035,
            m_frame: 0.120,
            m_motor: 0.030, // 4 * 0.030 = 0.120 kg
            m_esc_tot: 0.030,
            m_fc: 0.040,
            m_battery: 0.320,
            m_payload: 0.300,
            m_misc: 0.070,
            // Toplam kütle = 0.120 + 0.120 + 0.030 + 0.040 + 0.320 + 0.300 + 0.070 = 1.000 kg
            pack_wh: 60.48,
            dod: 0.80,
            pack_voltage: 14.4,
            pack_max_cont_a: 45.0,
            t_max_motor: 7.5, // 4 * 7.5 = 30 N
            fm: 0.65,
            eta_motor: 0.85,
            eta_esc: 0.95,
            profile_power_ratio: 0.15,
            coax_kappa: 1.28,
            h_lx: 0.0,
            h_wy: 0.0,
            flat_plate_area: 0.01,
            p_avionics: 5.0,
            v_cruise: 12.0,
            rho: 1.2250,
            comps_mass_x_y: vec![(1.000, 0.0, 0.0)],
        };

        let out = evaluate_multirotor(&inp);

        assert!((out.m_mtow - 1.000).abs() < 1e-4);
        assert!((out.w - 9.80665).abs() < 1e-3);
        assert!((out.t_hover_motor - 2.45166).abs() < 1e-3);

        // İdeal güç: ~21.79 W tek motor, ~87.16 W toplam
        assert!(
            (out.p_ideal_motor - 21.79).abs() < 0.1,
            "p_ideal_motor: {}",
            out.p_ideal_motor
        );
        assert!(
            (out.p_ideal_total - 87.16).abs() < 0.3,
            "p_ideal_total: {}",
            out.p_ideal_total
        );

        // Mil gücü: FM=0.65 -> ~33.53 W tek motor
        assert!(
            (out.p_shaft_motor - 33.52).abs() < 0.1,
            "p_shaft_motor: {}",
            out.p_shaft_motor
        );

        // Spec §8.3 aritmetik düzeltmesi:
        // P_elec_hover 166.059 W, P_hover 171.059 W, P_elec_motor 41.515 W, P_cruise 146.150 W
        assert!(
            (out.p_elec_motor - 41.515).abs() < 0.05,
            "p_elec_motor: {}",
            out.p_elec_motor
        );
        assert!(
            (out.p_elec_hover_total - 166.059).abs() < 0.05,
            "p_elec_hover_total: {}",
            out.p_elec_hover_total
        );
        assert!(
            (out.p_total_hover - 171.059).abs() < 0.05,
            "p_total_hover: {}",
            out.p_total_hover
        );
        assert!(
            (out.p_total_cruise_85 - 146.150).abs() < 0.05,
            "p_total_cruise_85: {}",
            out.p_total_cruise_85
        );

        // Disk yüklemesi: ~193.5 N/m^2
        assert!(
            (out.disk_loading - 193.5).abs() < 0.5,
            "disk_loading: {}",
            out.disk_loading
        );

        // Havada kalma süresi: E_usable=48.384 Wh / 171.059 W = 0.28285 h = 16.97 dk
        assert!(
            (out.hover_endurance_min - 16.97).abs() < 0.1,
            "hover_endurance_min: {}",
            out.hover_endurance_min
        );

        // İtki-ağırlık oranı: 30 / 9.80665 = 3.059
        assert!((out.tw_ratio - 3.059).abs() < 0.05);

        // Askı gazı doğrusal değil: sqrt(1/TW) = sqrt(1/3.059) = 0.5717 ≈ %57.2
        assert!(
            (out.hover_throttle - 0.572).abs() < 0.02,
            "hover_throttle: {}",
            out.hover_throttle
        );
    }

    #[test]
    fn test_nan_and_zero_guards() {
        let inp = MultirotorIn {
            d_frame: 0.0,
            arm_count: 0,
            layout: MultirotorLayout::X,
            coaxial: false,
            prop_clearance: f64::NAN,
            d_prop: 0.0,
            r_plate: 0.0,
            m_frame: 0.0,
            m_motor: 0.0,
            m_esc_tot: 0.0,
            m_fc: 0.0,
            m_battery: 0.0,
            m_payload: 0.0,
            m_misc: 0.0,
            pack_wh: 0.0,
            dod: 0.0,
            pack_voltage: 0.0,
            pack_max_cont_a: 0.0,
            t_max_motor: 0.0,
            fm: 0.0,
            eta_motor: 0.0,
            eta_esc: 0.0,
            profile_power_ratio: 0.0,
            coax_kappa: 0.0,
            h_lx: 0.0,
            h_wy: 0.0,
            flat_plate_area: 0.0,
            p_avionics: 0.0,
            v_cruise: 0.0,
            rho: 0.0,
            comps_mass_x_y: vec![],
        };
        let out = evaluate_multirotor(&inp);
        assert!(out.m_mtow >= 0.0);
        assert!(out.w >= 0.0);
        assert!(out.p_elec_hover_total.is_finite());
        assert!(out.hover_endurance_min.is_finite());
    }

    #[test]
    fn test_default_multirotor_cg_balanced() {
        let p = crate::project::Project {
            drone_type: crate::project::DroneType::Multirotor,
            mr_components: crate::project::default_mr_components(),
            ..crate::project::Project::default()
        };
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        assert!(
            m.mr.cg_offset_frac < 0.01,
            "Varsayılan multirotor AG sapması %1 altında olmalı (< %1): {:.3}% ({} mm)",
            m.mr.cg_offset_frac * 100.0,
            m.mr.cg_offset_d * 1000.0
        );
        assert!(m.mr.cg_offset_ok, "Varsayılan AG ofseti uygun olmalı");
        assert!(
            m.warnings
                .iter()
                .all(|w| w.msg_key != "warn.mr.cg_offset_high")
        );
    }
}
