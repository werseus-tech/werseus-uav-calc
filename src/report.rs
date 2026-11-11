// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tasarım Raporu Dışa Aktarma Modülü (Markdown).
//! Seçili drone türünün tüm girdilerini, sonuçlarını, uyarılarını ve formüllerini tek bir Markdown dosyasına döker.

use crate::calc::{Model, Sev};
use crate::i18n::{Lang, t_lang, tf_lang};
use crate::project::{DroneType, Project};

/// Seçili dilde tam bir Markdown tasarım raporu oluşturur.
pub fn generate_markdown_report(project: &Project, model: &Model, lang: Lang) -> String {
    let mut md = String::with_capacity(8192);

    let title = t_lang("report.title", lang);
    let app_version = env!("CARGO_PKG_VERSION");
    let platform = project.drone_type.label();

    // 1. Başlık ve Üstbilgi
    md.push_str(&format!("# {} — {}\n\n", title, project.name));
    md.push_str(&format!(
        "**{}**: WerSeuS – UAV Calc v{} | **{}**: {}\n\n",
        t_lang("report.generator", lang),
        app_version,
        t_lang("report.platform", lang),
        platform
    ));
    md.push_str(&format!(
        "> {}\n\n---\n\n",
        t_lang("report.disclaimer_note", lang)
    ));

    // 2. Özet Çıktılar (Executive Summary)
    md.push_str(&format!("## 1. {}\n\n", t_lang("report.sec_summary", lang)));
    md.push_str("| Metric | Value | Unit |\n|---|---|---|\n");
    let mtow = if project.drone_type == DroneType::Multirotor {
        model.mr.m_mtow
    } else {
        model.weight.m_mtow
    };
    let empty_weight = (mtow - model.weight.m_battery - project.m_payload).max(0.0);
    md.push_str(&format!(
        "| {} | {:.2} | kg |\n",
        t_lang("report.mtow", lang),
        mtow
    ));
    md.push_str(&format!(
        "| {} | {:.2} | kg |\n",
        t_lang("report.m_empty", lang),
        empty_weight
    ));
    md.push_str(&format!(
        "| {} | {:.2} | kg |\n",
        t_lang("report.m_payload", lang),
        project.m_payload
    ));
    md.push_str(&format!(
        "| {} | {:.2} | kg |\n",
        t_lang("report.m_battery", lang),
        model.weight.m_battery
    ));

    if project.drone_type.is_fixed_wing() {
        md.push_str(&format!(
            "| {} | {:.1} | m/s ({:.1} km/h) |\n",
            t_lang("report.v_stall", lang),
            model.cons.v_stall,
            model.cons.v_stall * 3.6
        ));
        md.push_str(&format!(
            "| {} | {:.1} | m/s ({:.1} km/h) |\n",
            t_lang("report.v_cruise", lang),
            project.v_cr,
            project.v_cr * 3.6
        ));
        md.push_str(&format!(
            "| {} | {:.1} | min |\n",
            t_lang("report.flight_time_cr", lang),
            model.perf_cr.endurance_min()
        ));
        md.push_str(&format!(
            "| {} | {:.1} | km |\n",
            t_lang("report.range_cr", lang),
            model.perf_cr.range_km()
        ));
        md.push_str(&format!(
            "| {} | {:.1} | % MAC |\n",
            t_lang("report.static_margin", lang),
            model.stab.sm * 100.0
        ));
    }
    if project.drone_type.is_multirotor() {
        md.push_str(&format!(
            "| {} | {:.2} | - |\n",
            t_lang("report.mr_tw", lang),
            model.mr.tw_ratio
        ));
        md.push_str(&format!(
            "| {} | {:.1} | % |\n",
            t_lang("report.mr_hover_throttle", lang),
            model.mr.hover_throttle * 100.0
        ));
        md.push_str(&format!(
            "| {} | {:.1} | min |\n",
            t_lang("report.mr_hover_time", lang),
            model.mr.hover_endurance_min
        ));
        md.push_str(&format!(
            "| {} | {:.1} | W |\n",
            t_lang("report.mr_p_hover", lang),
            model.mr.p_total_hover
        ));
    }
    md.push_str("\n---\n\n");

    // 3. Girdi Parametreleri (Design Inputs)
    md.push_str(&format!("## 2. {}\n\n", t_lang("report.sec_inputs", lang)));

    md.push_str(&format!(
        "### 2.1 {}\n\n",
        t_lang("report.sub_mission", lang)
    ));
    md.push_str("| Parameter | Value | Unit |\n|---|---|---|\n");
    md.push_str(&format!("| Cruise Altitude | {:.0} | m MSL |\n", project.h));
    md.push_str(&format!("| Cruise Speed | {:.1} | m/s |\n", project.v_cr));
    md.push_str(&format!(
        "| Payload Mass | {:.3} | kg |\n",
        project.m_payload
    ));
    md.push_str(&format!("| Battery Cell | {} | - |\n", project.cell_name));
    md.push_str(&format!(
        "| Pack Config | {}S{}P | - |\n",
        project.n_series, project.n_parallel
    ));
    md.push_str(&format!(
        "| DOD (Depth of Discharge) | {:.0} | % |\n",
        project.dod * 100.0
    ));
    md.push_str(&format!(
        "| Avionics / Hotel Power | {:.1} | W |\n",
        project.p_hotel
    ));
    md.push('\n');

    if project.drone_type.is_fixed_wing() {
        md.push_str(&format!(
            "### 2.2 {}\n\n",
            t_lang("report.sub_wing_tail", lang)
        ));
        md.push_str("| Parameter | Value | Unit |\n|---|---|---|\n");
        md.push_str(&format!("| Wing Span (b) | {:.3} | m |\n", project.b));
        md.push_str(&format!("| Wing Area (S) | {:.3} | m² |\n", project.s));
        md.push_str(&format!(
            "| Aspect Ratio (AR) | {:.2} | - |\n",
            model.wing.ar
        ));
        md.push_str(&format!("| Taper Ratio | {:.2} | - |\n", project.taper));
        md.push_str(&format!(
            "| LE Sweep Angle | {:.1} | deg |\n",
            project.sweep_le_deg
        ));
        md.push_str(&format!(
            "| Wing Airfoil t/c | {:.1} | % |\n",
            project.tc_w * 100.0
        ));
        md.push_str(&format!(
            "| Tail Configuration | {:?} | - |\n",
            project.tail_kind
        ));
        if project.tail_kind != crate::calc::geometry::TailKind::FlyingWing {
            md.push_str(&format!(
                "| Horizontal Tail Area (S_h) | {:.4} | m² |\n",
                project.s_h
            ));
            md.push_str(&format!(
                "| Vertical Tail Area (S_v) | {:.4} | m² |\n",
                project.s_v
            ));
            md.push_str(&format!(
                "| Tail Moment Arm (l_h) | {:.3} | m |\n",
                project.l_h
            ));
        }
        md.push('\n');
    }

    if project.drone_type.is_multirotor() {
        md.push_str(&format!(
            "### 2.3 {}\n\n",
            t_lang("report.sub_multirotor", lang)
        ));
        md.push_str("| Parameter | Value | Unit |\n|---|---|---|\n");
        md.push_str(&format!(
            "| Frame Diagonal / Size | {:.3} | m |\n",
            project.mr_frame_size
        ));
        md.push_str(&format!(
            "| Motor / Arm Count | {} | - |\n",
            project.mr_arm_count
        ));
        md.push_str(&format!("| Frame Layout | {:?} | - |\n", project.mr_layout));
        md.push_str(&format!(
            "| Coaxial Config | {} | - |\n",
            project.mr_coaxial
        ));
        md.push_str(&format!(
            "| Propeller Diameter | {:.1} | inch ({:.3} m) |\n",
            project.mr_d_prop / 0.0254,
            project.mr_d_prop
        ));
        md.push_str(&format!(
            "| Max Thrust per Motor | {:.1} | N |\n",
            project.mr_t_max_motor
        ));
        md.push_str(&format!(
            "| Frame Mass | {:.3} | kg |\n",
            project.mr_m_frame
        ));
        md.push_str(&format!(
            "| Motor Mass (each) | {:.3} | kg |\n",
            project.mr_m_motor
        ));
        md.push('\n');
    }

    // 4. Detaylı Mühendislik Analizi
    md.push_str(&format!(
        "## 3. {}\n\n",
        t_lang("report.sec_engineering", lang)
    ));

    if project.drone_type.is_fixed_wing() {
        md.push_str(&format!("### 3.1 {}\n\n", t_lang("report.sub_aero", lang)));
        md.push_str("| Metric | Value |\n|---|---|\n");
        md.push_str(&format!(
            "| Zero-Lift Drag (CD0) | {:.4} |\n",
            model.aero.cd0
        ));
        md.push_str(&format!(
            "| Induced Drag Factor (k) | {:.4} |\n",
            model.aero.k
        ));
        md.push_str(&format!(
            "| Oswald Efficiency (e) | {:.2} |\n",
            model.aero.e
        ));
        md.push_str(&format!(
            "| Max Lift Coefficient (CLmax) | {:.2} |\n",
            model.aero.cl_max
        ));
        md.push_str(&format!(
            "| Max L/D (Glide Ratio) | {:.1} |\n",
            model.aero.ld_max
        ));
        md.push('\n');

        md.push_str(&format!(
            "### 3.2 {}\n\n",
            t_lang("report.sub_stability", lang)
        ));
        md.push_str("| Metric | Value | Unit |\n|---|---|---|\n");
        md.push_str(&format!(
            "| CG Location (from nose) | {:.3} | m |\n",
            model.stab.cg_nose.x
        ));
        md.push_str(&format!(
            "| Neutral Point (x_np) | {:.3} | m |\n",
            model.stab.np.x_np + model.stab.datum
        ));
        md.push_str(&format!(
            "| Static Margin (SM) | {:.1} | % MAC |\n",
            model.stab.sm * 100.0
        ));
        md.push_str(&format!(
            "| Target Static Margin | {:.1} | % MAC |\n",
            project.sm_target * 100.0
        ));
        md.push_str(&format!(
            "| In Acceptable Band? | {} | - |\n",
            model.stab.in_band
        ));
        md.push('\n');

        md.push_str(&format!(
            "### 3.3 {}\n\n",
            t_lang("report.sub_structure", lang)
        ));
        md.push_str("| Metric | Value | Unit |\n|---|---|---|\n");
        md.push_str(&format!(
            "| Limit Load Factor | {:.1} | g |\n",
            project.n_limit
        ));
        md.push_str(&format!(
            "| Ultimate Load Factor | {:.1} | g |\n",
            project.n_limit * project.fs
        ));
        md.push_str(&format!(
            "| Spar Bending Moment (Ultimate) | {:.1} | N·m |\n",
            model.st.m_b_ult
        ));
        md.push_str(&format!(
            "| Max Spar Stress | {:.1} | MPa |\n",
            model.st.sigma_max / 1e6
        ));
        md.push_str(&format!(
            "| Structural Margin of Safety (MS) | {:.2} | - |\n",
            model.st.ms
        ));
        md.push('\n');
    }

    if project.drone_type.is_multirotor() {
        md.push_str(&format!(
            "### 3.4 {}\n\n",
            t_lang("report.sub_mr_analysis", lang)
        ));
        md.push_str("| Metric | Value | Unit |\n|---|---|---|\n");
        md.push_str(&format!(
            "| Total Disk Area | {:.4} | m² |\n",
            model.mr.a_disk_total
        ));
        md.push_str(&format!(
            "| Disk Loading | {:.1} | N/m² |\n",
            model.mr.disk_loading
        ));
        md.push_str(&format!(
            "| Hover Power (Total Electrical) | {:.1} | W |\n",
            model.mr.p_total_hover
        ));
        md.push_str(&format!(
            "| Hover Current | {:.1} | A |\n",
            model.mr.i_hover
        ));
        md.push_str(&format!(
            "| Cruise Power (at {:.1} m/s) | {:.1} | W |\n",
            project.mr_v_cruise, model.mr.p_total_cruise
        ));
        md.push_str(&format!(
            "| Motor Tip-to-Tip Adjacent Spacing | {:.1} | mm |\n",
            model.mr.s_adj * 1000.0
        ));
        let clearance_mm = (model.mr.s_adj - project.mr_d_prop).max(0.0) * 1000.0;
        md.push_str(&format!(
            "| Blade Tip Clearance | {:.1} | mm |\n",
            clearance_mm
        ));
        md.push_str(&format!(
            "| CG Planar Offset | {:.1} | mm |\n",
            model.mr.cg_offset_d * 1000.0
        ));
        md.push('\n');
    }

    // 5. Uyarılar ve Teşhis
    md.push_str(&format!(
        "## 4. {}\n\n",
        t_lang("report.sec_warnings", lang)
    ));
    if model.warnings.is_empty() {
        md.push_str(&format!("*{}*\n\n", t_lang("report.no_warnings", lang)));
    } else {
        for w in &model.warnings {
            let sev_badge = match w.sev {
                Sev::Err => "🔴 **ERROR**",
                Sev::Warn => "🟡 **WARN**",
                Sev::Info => "ℹ️ **INFO**",
            };
            let area_str = t_lang(w.area_key, lang);
            let msg_str = tf_lang(w.msg_key, lang, &w.args);
            md.push_str(&format!("- {} [{}] {}\n", sev_badge, area_str, msg_str));
        }
        md.push('\n');
    }

    // 6. Kullanılan Formüller ve Kaynakça
    md.push_str(&format!(
        "## 5. {}\n\n",
        t_lang("report.sec_formulas", lang)
    ));
    md.push_str("### 5.1 Aerodinamik & Kutup Eğrisi\n");
    md.push_str("- **Sürtünme Kutbu (Drag Polar)**: $C_D = C_{D0} + k \\cdot C_L^2$\n");
    md.push_str("- **İndüklenmiş Sürtünme Çarpanı**: $k = \\frac{1}{\\pi \\cdot AR \\cdot e}$\n");
    md.push_str(
        "- **Oswald Verimi (Raymer)**: $e = 1.78 \\cdot (1 - 0.045 \\cdot AR^{0.68}) - 0.64$\n",
    );
    md.push_str("- **Tutunma Hızı (Stall Speed)**: $V_{stall} = \\sqrt{\\frac{2 \\cdot W}{\\rho \\cdot S \\cdot C_{Lmax}}}$\n\n");

    md.push_str("### 5.2 Multirotor Momentum Teorisi\n");
    md.push_str("- **İdeal İndüklenmiş Güç**: $P_{ideal} = \\frac{T^{1.5}}{\\sqrt{2 \\cdot \\rho \\cdot A_{disk}}}$\n");
    md.push_str("- **Gerçek Şaft Gücü**: $P_{shaft} = \\frac{P_{ideal}}{FM}$\n");
    md.push_str("- **Elektriksel Güç**: $P_{elec} = \\frac{P_{shaft}}{\\eta_{mot} \\cdot \\eta_{esc}} + P_{hotel}$\n\n");

    md.push_str("### 5.3 Boyuna Stabilite\n");
    md.push_str("- **Nötr Nokta**: $x_{np} = x_{ac,w} + \\frac{C_{L\\alpha,h}}{C_{L\\alpha,w}} \\cdot \\frac{S_h}{S} \\cdot \\frac{l_h}{c} \\cdot \\eta_h$\n");
    md.push_str("- **Statik Marjin**: $SM = \\frac{x_{np} - x_{cg}}{\\bar{c}}$\n\n");

    md.push_str("### 5.4 Kaynakça\n");
    md.push_str("1. Raymer, D. P. *Aircraft Design: A Conceptual Approach*, 6th Edition, AIAA Education Series.\n");
    md.push_str("2. Roskam, J. *Airplane Design: Part I–VIII*, DARcorporation.\n");
    md.push_str(
        "3. Leishman, J. G. *Principles of Helicopter Aerodynamics*, Cambridge University Press.\n",
    );
    md.push_str(
        "4. Brandt, S. A. et al. *Introduction to Aeronautics: A Design Perspective*, AIAA.\n\n",
    );

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::Library;

    #[test]
    fn test_markdown_report_generation_fixed_wing() {
        let p = Project::default();
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let report_tr = generate_markdown_report(&p, &m, Lang::Tr);
        assert!(report_tr.contains("WerSeuS – UAV Calc"));
        assert!(report_tr.contains("Sürtünme Kutbu"));
        assert!(report_tr.contains("MTOW"));

        let report_en = generate_markdown_report(&p, &m, Lang::En);
        assert!(report_en.contains("WerSeuS – UAV Calc"));
        assert!(report_en.contains("Executive Summary"));
    }

    #[test]
    fn test_markdown_report_generation_multirotor() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let lib = Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);

        let report = generate_markdown_report(&p, &m, Lang::Tr);
        assert!(report.contains("Momentum Teorisi"));
        assert!(report.contains("Disk"));
    }
}
