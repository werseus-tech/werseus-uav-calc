// SPDX-License-Identifier: AGPL-3.0-or-later
//! Araç güç modeli bağdaştırıcısı: mevcut çok rotorlu (`calc::multirotor`) ve sabit kanat
//! (`calc::performance`) modellerini farklı hava yoğunluğu ve kütleyle yeniden çalıştırır.
//! İkinci bir güç modeli yazılmaz; her değer o modellerin çıktısıdır.
use crate::calc::atmosphere::G;
use crate::calc::multirotor::{MultirotorIn, evaluate_multirotor};
use crate::calc::performance::{PerfIn, at_speed, eta_pt};
use crate::calc::{Model, aero};
use crate::project::{DroneType, Project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Multirotor,
    FixedWing,
    Vtol,
}

#[derive(Clone, Debug)]
pub struct Vehicle {
    pub kind: Kind,
    mr_in: MultirotorIn,
    perf_in: PerfIn,
    /// Kalkış kütlesi [kg] ve bırakılabilir faydalı yük [kg].
    pub mass0_kg: f64,
    pub payload_kg: f64,
    /// Paket enerjisi [Wh] ve proje kullanılabilir derinliği (rezerv dahil edilmiş DoD).
    pub pack_wh: f64,
    pub dod: f64,
    /// Önerilen seyir hızı [m/s].
    pub v_cruise: f64,
    cl_max: f64,
    /// Proje yoğunluğunda en uzun süre hızı (yoğunlukla ölçeklenir).
    v_loiter_ref: f64,
}

impl Vehicle {
    pub fn from_model(p: &Project, m: &Model) -> Vehicle {
        let kind = match p.drone_type {
            DroneType::Multirotor => Kind::Multirotor,
            DroneType::VtolQuadplane => Kind::Vtol,
            _ => Kind::FixedWing,
        };
        let payload_kg = if p.m_payload.is_finite() {
            p.m_payload.max(0.0)
        } else {
            0.0
        };
        let mut mr_in = m.mr_in.clone();
        let mass0 = match kind {
            Kind::Multirotor => m.mr.m_mtow,
            _ => m.weight.m_mtow,
        };
        let mass0 = if mass0.is_finite() {
            mass0.max(0.01)
        } else {
            0.01
        };
        if kind != Kind::FixedWing {
            // Bırakılan yük mr_with içinde m_payload'dan düşülür; askı kütlesi mass0 ile
            // eşleşsin diye m_payload = payload_kg yapılıp fark m_misc'e yansıtılır.
            mr_in.m_payload = payload_kg;
            let mr_mtow = evaluate_multirotor(&mr_in).m_mtow;
            mr_in.m_misc += mass0 - mr_mtow;
        }
        let v_cruise = if kind == Kind::Multirotor {
            p.mr_v_cruise
        } else {
            p.v_cr
        };
        Vehicle {
            kind,
            mr_in,
            perf_in: m.perf_in.clone(),
            mass0_kg: mass0,
            payload_kg,
            pack_wh: m.weight.pack_wh,
            dod: p.dod,
            v_cruise: if v_cruise.is_finite() && v_cruise > 0.0 {
                v_cruise
            } else {
                15.0
            },
            cl_max: m.aero.cl_max,
            v_loiter_ref: m.perf_end.v,
        }
    }

    pub fn has_hover(&self) -> bool {
        self.kind != Kind::FixedWing
    }

    /// Bırakılan yük [kg] kırpılmış (kalan faydalı yükten fazlası bırakılamaz).
    pub fn clamp_drop(&self, already_dropped: f64, want: f64) -> f64 {
        want.max(0.0)
            .min((self.payload_kg - already_dropped).max(0.0))
    }

    /// Anlık kütle [kg].
    pub fn mass_kg(&self, dropped: f64) -> f64 {
        (self.mass0_kg - dropped).max(0.01)
    }

    /// Dönüştürücü zinciri verimi (dikey tırmanma/alçalma terimi için).
    pub fn eta_vertical(&self) -> f64 {
        let e = match self.kind {
            Kind::FixedWing => eta_pt(&self.perf_in),
            _ => {
                self.mr_in.fm.clamp(0.1, 1.0)
                    * self.mr_in.eta_motor.clamp(0.1, 1.0)
                    * self.mr_in.eta_esc.clamp(0.1, 1.0)
            }
        };
        e.clamp(0.05, 1.0)
    }

    /// Sabit kanat / kanat üzerinde VTOL itki zinciri verimi.
    pub fn eta_fixed_wing(&self) -> f64 {
        eta_pt(&self.perf_in).clamp(0.05, 1.0)
    }

    /// Sabit (otel) yük [W].
    pub fn hotel_w(&self) -> f64 {
        match self.kind {
            Kind::FixedWing => self.perf_in.p_hotel,
            _ => self.mr_in.p_avionics,
        }
    }

    /// Sabit kanat / kanat üzerinde VTOL sabit (otel) yük [W].
    pub fn hotel_fixed_wing(&self) -> f64 {
        self.perf_in.p_hotel
    }

    fn mr_with(&self, rho: f64, dropped: f64) -> MultirotorIn {
        let mut i = self.mr_in.clone();
        i.rho = rho;
        i.m_payload = (i.m_payload - dropped).max(0.0);
        i
    }

    /// Askı bataryadan çekilen güç [W] (aviyonik dahil); sabit kanatta yok.
    pub fn hover_w(&self, rho: f64, dropped: f64) -> Option<f64> {
        if !self.has_hover() {
            return None;
        }
        Some(evaluate_multirotor(&self.mr_with(rho, dropped)).p_total_hover)
    }

    /// Çok rotorlu / askıda VTOL eksenel tırmanma (`vz_up >= 0`) veya alçalma (`vz_up < 0`) gücü [W].
    /// Tırmanmada momentum teorisi (indüklenmiş hız azalması ve rotor FM dahil), alçalmada
    /// askı gücünün %50'si alt sınırıyla tutarlı ilk terim yaklaşımı kullanılır.
    pub fn vertical_w(&self, rho: f64, dropped: f64, vz_up: f64) -> Option<f64> {
        if !self.has_hover() {
            return None;
        }
        let mr_in = self.mr_with(rho, dropped);
        let out = evaluate_multirotor(&mr_in);
        let fm = mr_in.fm.clamp(0.1, 1.0);
        let eta_elec = mr_in.eta_motor.clamp(0.1, 1.0) * mr_in.eta_esc.clamp(0.1, 1.0);
        let eta_mr = (fm * eta_elec).clamp(0.05, 1.0);
        let w = out.w.max(0.0);
        if vz_up >= 0.0 {
            if w <= 0.0 || out.p_ideal_total <= 0.0 {
                return Some(out.p_total_hover);
            }
            let kappa = out.coax_kappa.max(1.0);
            let vh = out.p_ideal_total / (w * kappa);
            let half_vz = 0.5 * vz_up;
            let vi = -half_vz + (half_vz * half_vz + vh * vh).sqrt();
            let p_ideal = w * (vz_up + kappa * vi);
            Some(p_ideal / eta_mr + mr_in.p_avionics.max(0.0))
        } else {
            let vz_down = -vz_up;
            let p_desc = out.p_total_hover - 0.5 * w * vz_down / eta_mr;
            Some(p_desc.max(0.5 * out.p_total_hover))
        }
    }

    fn perf_with(&self, rho: f64, dropped: f64) -> PerfIn {
        let mut i = self.perf_in.clone();
        let w = self.mass_kg(dropped) * G;
        i.ws *= w / i.w.max(1e-9);
        i.w = w;
        i.rho = rho;
        i
    }

    /// Düz uçuşta bataryadan çekilen güç [W] `v` hava hızında (çok rotorlu: Glauert ileri uçuş;
    /// sabit kanat ve VTOL: kutup eğrisi).
    pub fn level_w(&self, rho: f64, dropped: f64, v: f64) -> f64 {
        let v = v.max(1.0);
        match self.kind {
            Kind::Multirotor => {
                let mut i = self.mr_with(rho, dropped);
                i.v_cruise = v;
                evaluate_multirotor(&i).p_total_cruise
            }
            _ => at_speed(v, &self.perf_with(rho, dropped)).p_total,
        }
    }

    /// Sabit kanat bekleme (loiter) hızı [m/s]: en uzun süre hızı, yoğunluk ve anlık kütleyle ölçeklenir.
    pub fn loiter_speed(&self, rho: f64, dropped: f64) -> f64 {
        let r = ((self.perf_in.rho / rho.max(0.1))
            * (self.mass_kg(dropped) / self.mass0_kg.max(0.01)))
        .sqrt();
        (self.v_loiter_ref * r).max(1.0)
    }

    /// `v` hızında CL, CLmax'ı aşıyor mu (stall uyarısı); çok rotorda hep false.
    pub fn below_stall(&self, rho: f64, dropped: f64, v: f64) -> bool {
        if self.kind == Kind::Multirotor || self.cl_max.is_nan() || self.cl_max <= 0.0 {
            return false;
        }
        let i = self.perf_with(rho, dropped);
        aero::cl_for(i.ws, rho, v.max(1.0)) > self.cl_max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn vtol_payload_drop_syncs_mr_mass_even_when_mr_payload_differs() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::VtolQuadplane);
        p.m_payload = 1.5;
        let mut m = evaluate(&p, &Library::embedded());
        // Model mr_in.m_payload proje değerinden küçük olsa ve weight.m_mtow < mr.m_mtow olsa bile:
        m.mr_in.m_payload = 0.2;
        m.weight.m_mtow = (m.mr.m_mtow - 1.0).max(2.0);
        let v = Vehicle::from_model(&p, &m);
        let rho = crate::calc::atmosphere::RHO0;
        let mr0 = evaluate_multirotor(&v.mr_with(rho, 0.0)).m_mtow;
        let mr1 = evaluate_multirotor(&v.mr_with(rho, 1.5)).m_mtow;
        assert!((mr0 - v.mass_kg(0.0)).abs() < 1e-9);
        assert!((mr1 - v.mass_kg(1.5)).abs() < 1e-9);
        assert!(v.hover_w(rho, 1.5).unwrap() < v.hover_w(rho, 0.0).unwrap());
    }

    #[test]
    fn loiter_speed_scales_with_remaining_mass() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::FixedWingConventional);
        let m = evaluate(&p, &Library::embedded());
        let v = Vehicle::from_model(&p, &m);
        let rho = v.perf_in.rho;
        let v0 = v.loiter_speed(rho, 0.0);
        let drop = (0.3 * v.mass0_kg).min(v.payload_kg.max(0.3 * v.mass0_kg));
        let v1 = v.loiter_speed(rho, drop);
        let ratio = (v.mass_kg(drop) / v.mass0_kg).sqrt();
        assert!((v1 / v0 - ratio).abs() < 1e-9);
    }

    #[test]
    fn vertical_w_uses_momentum_theory_and_fm() {
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        let m = evaluate(&p, &Library::embedded());
        let v = Vehicle::from_model(&p, &m);
        let rho = crate::calc::atmosphere::RHO0;
        let h = v.hover_w(rho, 0.0).unwrap();
        assert!((v.vertical_w(rho, 0.0, 0.0).unwrap() - h).abs() < 1e-9);
        let vz = 0.5;
        let climb = v.vertical_w(rho, 0.0, vz).unwrap();
        let w = v.mass_kg(0.0) * G;
        // Momentum teorisi küçük tırmanma hızında ΔP_ideal ≈ 0.5 * W * Vz verir; FM ile bölünür.
        let eta_mr = v.eta_vertical();
        let expected_approx = h + 0.5 * w * vz / eta_mr;
        assert!((climb - expected_approx).abs() / climb < 0.01);
        assert!(v.vertical_w(rho, 0.0, -2.0).unwrap() < h);
    }
}
