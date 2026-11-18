// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pervane–gövde etkileşimi (spesifikasyon modül 3, "downwash" bölümü): mühendislik düzeyinde,
//! hızlı ve açıklanabilir modeller (CFD değildir).
//!
//! - Multirotor: rotor başına momentum teorisi indüklenmiş hızı ve iz daralması; kolların,
//!   merkez plakanın ve bileşenlerin izde kalan alanı → aşağı yük (itkinin %'si); komşu rotor uç
//!   açıklığı uyarısı ve itki kaybı; koaksiyel alt rotor cezası (bilgi). Net etkin askı itkisi ve
//!   gereken ek askı gücü.
//! - Sabit kanat / VTOL: çekici/itici pervane hava akımının (slipstream) kanat ve gövdede yarattığı
//!   ek sürükleme ve kaldırma değişimi (seyir ve düşük hız); VTOL'de askıda kaldırma rotorlarının
//!   kanada, bomlara ve gövdeye aşağı yükü ve askı itki cezası.
//!
//! Kaynaklar: Leishman, *Principles of Helicopter Aerodynamics* (2006) — momentum teorisi, aşağı
//! yük/engelleme, tandem örtüşme; McCormick, *Aerodynamics, Aeronautics, and Flight Mechanics*
//! (1995) — pervane hava akımı ve kanat etkileşimi; Hoerner, *Fluid-Dynamic Drag* (1965) — akışa
//! dik silindir/kare/levha sürükleme katsayıları. Tüm ampirik katsayılar [`Coeffs`] içinde
//! ayarlanabilir varsayılanlardır. Bu modül proje değerlerini DEĞİŞTİRMEZ.

pub mod blockage;
pub mod momentum;

use crate::cad::layout::Layout;
use crate::calc::Model;
use crate::calc::mr_structure::MrArmSectionType;
use crate::project::{DroneType, Project};
use blockage::{BlockageOut, Item, ItemKind, Pair, Rotor, Shape};
use momentum::{
    added_velocity_at, disk_area, induced_velocity_axial, slipstream_radius_at,
    slipstream_velocity, thrust_at_ideal_power,
};

/// Ayarlanabilir model katsayıları (hepsi ampirik varsayılanlardır).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    /// Kol C_D; ≤ 0 ise kol kesitinden: yuvarlak 1.2, kare 2.0 (Hoerner 1965, alt-kritik Re).
    pub cd_arm: f64,
    /// Kol genişliği [m]; ≤ 0 ise yapı adımındaki kol dış ölçüsü (yoksa CAD kol genişliği).
    pub arm_width: f64,
    /// Merkez plaka / gövde kutusu C_D (akışa dik levha ≈ 1.0–1.2).
    pub cd_body: f64,
    /// Bileşen (batarya, kamera…) C_D (küt kutu ≈ 0.9).
    pub cd_component: f64,
    /// Bileşen yoğunluğu [kg/m³]: izdüşüm kenarı ∛(m/ρ) varsayılır (projede boyut yok).
    pub component_density: f64,
    /// VTOL kanadı C_D (akışa dik kanat; tiltrotor deneyleri ≈ 1.0–1.5, Leishman 2006).
    pub cd_wing: f64,
    /// VTOL gövdesi (enine akışta silindir) C_D.
    pub cd_fuselage: f64,
    /// VTOL bomu / kuyruk C_D (silindir ≈ 1.2).
    pub cd_boom: f64,
    /// Rotor düzlemi ile altındaki yapı arasındaki düşey uzaklık [m] (motor + göbek yüksekliği).
    pub rotor_height: f64,
    /// Göbek bölgesi (akışsız kabul): iz yarıçapının oranı.
    pub hub_cutout: f64,
    /// Uç açıklığı uyarı eşiği δ = (s − D)/D.
    pub tip_warn: f64,
    /// Uçlar değerken (δ = 0) rotor başına ampirik itki kaybı oranı.
    pub tip_loss_touch: f64,
    /// Hava akımındaki kanadın kaldırma etkinliği (iz dönmesi, daralma ve sonlu açıklık için
    /// ampirik azaltma; McCormick 1995'teki ideal değer 1.0).
    pub slip_lift_factor: f64,
}

impl Default for Coeffs {
    fn default() -> Self {
        Coeffs {
            cd_arm: 0.0,
            arm_width: 0.0,
            cd_body: 1.1,
            cd_component: 0.9,
            component_density: 1000.0,
            cd_wing: 1.0,
            cd_fuselage: 0.9,
            cd_boom: 1.2,
            rotor_height: 0.035,
            hub_cutout: 0.15,
            tip_warn: 0.10,
            tip_loss_touch: 0.03,
            slip_lift_factor: 0.8,
        }
    }
}

fn fin(x: f64) -> bool {
    x.is_finite()
}

fn pos(x: f64) -> bool {
    x.is_finite() && x > 0.0
}

impl Coeffs {
    /// Etkin kol C_D'si.
    pub fn arm_cd(&self, p: &Project) -> f64 {
        if pos(self.cd_arm) {
            self.cd_arm
        } else {
            match p.mr_arm_section {
                MrArmSectionType::Round => 1.2,
                MrArmSectionType::Square => 2.0,
            }
        }
    }

    /// Etkin kol genişliği [m].
    pub fn arm_w(&self, p: &Project, layout_w_mm: f64) -> f64 {
        if pos(self.arm_width) {
            self.arm_width
        } else if pos(p.mr_arm_d_o) && p.mr_arm_d_o > 0.003 && p.mr_arm_d_o < 0.2 {
            p.mr_arm_d_o
        } else if pos(layout_w_mm) {
            layout_w_mm / 1000.0
        } else {
            0.02
        }
    }
}

/// Tablo satırı: aynı adlı parçaların toplamı.
#[derive(Clone, Debug)]
pub struct ItemRow {
    pub name: String,
    pub kind: ItemKind,
    pub count: usize,
    /// Kapalı alan [m²].
    pub area: f64,
    /// Aşağı yük [N].
    pub force: f64,
    /// Toplam itkinin yüzdesi.
    pub pct: f64,
    pub cd: f64,
}

/// Koaksiyel alt rotor cezası (bilgi; askı gücünde zaten hesaba katılmıştır).
#[derive(Clone, Copy, Debug)]
pub struct CoaxInfo {
    pub kappa: f64,
    /// İndüklenmiş güç artışı yüzdesi (κ − 1)·100.
    pub penalty_pct: f64,
}

/// Askı (multirotor veya VTOL kaldırma rotorları) sonuçları.
#[derive(Clone, Debug)]
pub struct HoverOut {
    pub rotors: Vec<Rotor>,
    pub items: Vec<Item>,
    pub block: BlockageOut,
    pub rows: Vec<ItemRow>,
    pub pairs: Vec<Pair>,
    pub coax: Option<CoaxInfo>,
    pub rho: f64,
    /// Ağırlık (gereken net itki) [N].
    pub weight: f64,
    /// Rotor başına indüklenmiş hız ve uzak iz hızı [m/s].
    pub v_i: f64,
    pub v_far: f64,
    /// Yapı derinliğindeki iz yarıçapı ve uzak iz yarıçapı [m].
    pub r_slip_struct: f64,
    pub r_slip_far: f64,
    /// Aşağı yük oranı (0..1).
    pub download_frac: f64,
    /// Rotor etkileşimi itki kaybı oranı (0..1).
    pub interaction_frac: f64,
    /// Net etkin itki oranı = 1 − aşağı yük − etkileşim.
    pub net_frac: f64,
    /// Ağırlığı taşımak için rotorların üretmesi gereken toplam itki [N].
    pub thrust_required: f64,
    /// Kayıpsız askı elektrik gücü ve ek güç [W].
    pub p_hover: f64,
    pub extra_power: f64,
    /// Ek gücün yüzdesi.
    pub extra_power_pct: f64,
    /// Askı süresindeki tahmini azalma yüzdesi (aviyonik dahil toplam güce göre).
    pub endurance_drop_pct: f64,
}

/// Sabit kanat pervane hava akımı (bir hız noktası).
#[derive(Clone, Copy, Debug, Default)]
pub struct SlipPoint {
    pub v: f64,
    pub thrust: f64,
    pub v_i: f64,
    /// Uzak hava akımı hızı V + 2v_i ve kanattaki yerel hız [m/s].
    pub v_slip: f64,
    pub v_at_wing: f64,
    /// Kanattaki hava akımı yarıçapı [m].
    pub r_at_wing: f64,
    /// Hava akımındaki kanat alanı [m²] ve kanat alanına oranı.
    pub s_imm: f64,
    pub frac_wing: f64,
    /// Ek sürükleme [N]: kanat, gövde, toplam; seyir sürüklemesine oranı (%).
    pub dd_wing: f64,
    pub dd_fuse: f64,
    pub dd_total: f64,
    pub dd_pct: f64,
    /// Kaldırma değişimi [N] ve ağırlığa oranı (%).
    pub dl: f64,
    pub dl_pct: f64,
    pub cl: f64,
}

/// İtici/çekici pervane hava akımı sonuçları.
#[derive(Clone, Debug)]
pub struct SlipOut {
    pub pusher: bool,
    pub d_prop: f64,
    /// Pervane diskinin x konumu (ileri +) ve kanat çeyrek kordu x'i [m].
    pub x_prop: f64,
    pub x_wing: f64,
    pub cruise: SlipPoint,
    pub low: SlipPoint,
    /// Çizim için kanat ve gövde.
    pub items: Vec<Item>,
}

/// Tüm sonuçlar.
#[derive(Clone, Debug)]
pub struct DownwashOut {
    pub drone_type: DroneType,
    pub hover: Option<HoverOut>,
    pub slip: Option<SlipOut>,
}

/// Ana giriş: projeyi (değiştirmeden) okur, sonuçları döndürür.
pub fn evaluate(p: &Project, m: &Model, c: &Coeffs) -> DownwashOut {
    let layout = Layout::new(p, m);
    let hover = match p.drone_type {
        DroneType::Multirotor => multirotor_hover(p, m, c, &layout),
        DroneType::VtolQuadplane => vtol_hover(p, m, c, &layout),
        _ => None,
    };
    let slip = if p.drone_type.has_wing() {
        fixed_wing_slip(p, m, c, &layout)
    } else {
        None
    };
    DownwashOut {
        drone_type: p.drone_type,
        hover,
        slip,
    }
}

fn rho_of(m: &Model) -> f64 {
    if pos(m.atmo.rho) {
        m.atmo.rho
    } else {
        crate::calc::atmosphere::RHO0
    }
}

fn multirotor_hover(p: &Project, m: &Model, c: &Coeffs, lay: &Layout) -> Option<HoverOut> {
    let r = 0.5 * p.mr_d_prop;
    let coords = &m.mr.motor_coords;
    let w = m.mr.w;
    if !pos(r) || coords.is_empty() || !pos(w) {
        return None;
    }
    let n_arm = coords.len();
    // Koaksiyelde iki rotor aynı kolda: kol başına itki çiftin toplamıdır.
    let t_arm = w / n_arm as f64;
    let rotors: Vec<Rotor> = coords
        .iter()
        .filter(|(x, y)| fin(*x) && fin(*y))
        .map(|&(x, y)| Rotor {
            x,
            y,
            r,
            thrust: t_arm,
        })
        .collect();
    let h = if pos(c.rotor_height) {
        c.rotor_height
    } else {
        0.035
    };
    let mut items = Vec::new();
    // 1) Merkez plaka (en üstte; altındakileri gölgeler).
    let r_plate = if pos(p.mr_r_plate) {
        p.mr_r_plate
    } else {
        lay.mr_plate.plate_radius_mm / 1000.0
    };
    if pos(r_plate) {
        items.push(Item {
            name: crate::i18n::t("downwash.item_body").to_string(),
            kind: ItemKind::Body,
            shape: Shape::Circle {
                c: [0.0, 0.0],
                r: r_plate,
            },
            z: h,
            cd: c.cd_body,
        });
    }
    // 2) Kollar: merkezden motora şerit (H düzeninde de radyal kabul edilir — basitleştirme).
    let aw = c.arm_w(p, lay.mr_arm.width_mm);
    let cd_arm = c.arm_cd(p);
    for rot in &rotors {
        items.push(Item {
            name: crate::i18n::t("downwash.item_arms").to_string(),
            kind: ItemKind::Arm,
            shape: Shape::Segment {
                a: [0.0, 0.0],
                b: [rot.x, rot.y],
                w: aw,
            },
            z: h,
            cd: cd_arm,
        });
    }
    // 3) Bileşenler: plakanın altında, kare izdüşüm ∛(m/ρ).
    let rho_c = if pos(c.component_density) {
        c.component_density
    } else {
        1000.0
    };
    for comp in &p.mr_components {
        if !(pos(comp.mass) && fin(comp.x) && fin(comp.y)) {
            continue;
        }
        let side = (comp.mass / rho_c).cbrt();
        let name = if comp.name.trim().is_empty() {
            crate::i18n::t("downwash.item_component").to_string()
        } else {
            comp.name.clone()
        };
        items.push(Item {
            name,
            kind: ItemKind::Component,
            shape: Shape::Rect {
                x0: comp.x - 0.5 * side,
                x1: comp.x + 0.5 * side,
                y0: comp.y - 0.5 * side,
                y1: comp.y + 0.5 * side,
            },
            z: h + 0.03,
            cd: c.cd_component,
        });
    }
    let coax = if p.mr_coaxial {
        let k = m.mr.coax_kappa;
        Some(CoaxInfo {
            kappa: k,
            penalty_pct: (k - 1.0) * 100.0,
        })
    } else {
        None
    };
    let p_hover = m.mr.p_elec_hover_total;
    let p_total = m.mr.p_total_hover;
    Some(finish_hover(
        rotors,
        items,
        coax,
        rho_of(m),
        w,
        p_hover,
        p_total,
        h,
        c,
    ))
}

/// VTOL: kaldırma motoru yuvaları (CAD yerleşimi) üzerindeki rotorlar; kanat, gövde, bomlar ve
/// yatay kuyruk izde kalabilir. Ağırlık sabit kanat MTOW'u; güç momentum teorisinden.
fn vtol_hover(p: &Project, m: &Model, c: &Coeffs, lay: &Layout) -> Option<HoverOut> {
    let r = 0.5 * p.mr_d_prop;
    let w = m.weight.w;
    if !pos(r) || !pos(w) {
        return None;
    }
    let mounts: Vec<[f64; 2]> = lay
        .instances
        .iter()
        .filter(|i| i.label.starts_with("LiftMount_"))
        .map(|i| [-i.frame.t[0] / 1000.0, i.frame.t[1] / 1000.0])
        .filter(|q| fin(q[0]) && fin(q[1]))
        .collect();
    if mounts.is_empty() {
        return None;
    }
    let n = mounts.len();
    let t_rot = w / n as f64;
    let rotors: Vec<Rotor> = mounts
        .iter()
        .map(|q| Rotor {
            x: q[0],
            y: q[1],
            r,
            thrust: t_rot,
        })
        .collect();
    let h = if pos(c.rotor_height) {
        c.rotor_height
    } else {
        0.035
    };
    let mut items = airframe_items(p, m, c, h);
    // Bomlar: aynı taraftaki ön ve arka motor arasında.
    let od = if pos(lay.boom.od_mm) {
        lay.boom.od_mm / 1000.0
    } else {
        0.016
    };
    for side in [-1.0, 1.0] {
        let same: Vec<&[f64; 2]> = mounts.iter().filter(|q| q[1] * side > 0.0).collect();
        if same.len() >= 2 {
            let (mut a, mut b) = (*same[0], *same[0]);
            for q in &same {
                if q[0] > a[0] {
                    a = **q;
                }
                if q[0] < b[0] {
                    b = **q;
                }
            }
            items.push(Item {
                name: crate::i18n::t("downwash.item_booms").to_string(),
                kind: ItemKind::Boom,
                shape: Shape::Segment { a, b, w: od },
                z: h + 0.5 * od,
                cd: c.cd_boom,
            });
        }
    }
    // Askı gücü: P = n·T^1.5/√(2ρA) / (FM·η_motor·η_ESC).
    let rho = rho_of(m);
    let a = disk_area(r);
    let fm = if pos(p.fm) { p.fm.clamp(0.1, 1.0) } else { 0.6 };
    let eta = (if pos(p.eta_mot) { p.eta_mot } else { 0.85 })
        * (if pos(p.eta_esc) { p.eta_esc } else { 0.95 });
    let p_ideal = n as f64 * t_rot.powf(1.5) / (2.0 * rho * a).sqrt();
    let p_hover = p_ideal / (fm * eta.clamp(0.1, 1.0));
    let p_total = p_hover + if pos(p.p_hotel) { p.p_hotel } else { 0.0 };
    Some(finish_hover(
        rotors, items, None, rho, w, p_hover, p_total, h, c,
    ))
}

/// Sabit kanatlı gövdenin üstten izdüşümü (x ileri +, burun x = 0): kanat, gövde, yatay kuyruk.
fn airframe_items(p: &Project, m: &Model, c: &Coeffs, h: f64) -> Vec<Item> {
    let mut items = Vec::new();
    let half_d = if pos(p.d_fuse) { 0.5 * p.d_fuse } else { 0.0 };
    if pos(p.b) && pos(m.wing.c_r) {
        items.push(Item {
            name: crate::i18n::t("downwash.item_wing").to_string(),
            kind: ItemKind::Wing,
            shape: Shape::Wing {
                x_le_root: -(if fin(p.x_root_le) { p.x_root_le } else { 0.0 }),
                c_root: m.wing.c_r,
                taper: if fin(p.taper) {
                    p.taper.clamp(0.0, 1.5)
                } else {
                    1.0
                },
                half_span: 0.5 * p.b,
                tan_sweep_le: if fin(p.sweep_le_deg) {
                    p.sweep_le_deg.clamp(-60.0, 60.0).to_radians().tan()
                } else {
                    0.0
                },
                y_min: half_d,
            },
            z: h,
            cd: c.cd_wing,
        });
    }
    if pos(p.l_fuse) && pos(half_d) {
        items.push(Item {
            name: crate::i18n::t("downwash.item_fuselage").to_string(),
            kind: ItemKind::Fuselage,
            shape: Shape::Rect {
                x0: -p.l_fuse,
                x1: 0.0,
                y0: -half_d,
                y1: half_d,
            },
            z: h + half_d,
            cd: c.cd_fuselage,
        });
    }
    // Yatay kuyruk (klasik kuyrukta): rüzgâr tüneliyle aynı AR = 4 varsayımı.
    if p.drone_type != DroneType::FlyingWing
        && p.tail_kind == crate::calc::geometry::TailKind::Conventional
        && pos(p.s_h)
        && pos(p.l_fuse)
    {
        let bt = (4.0 * p.s_h).sqrt();
        let ct = p.s_h / bt;
        items.push(Item {
            name: crate::i18n::t("downwash.item_tail").to_string(),
            kind: ItemKind::Tail,
            shape: Shape::Rect {
                x0: -p.l_fuse,
                x1: -p.l_fuse + ct,
                y0: -0.5 * bt,
                y1: 0.5 * bt,
            },
            z: h,
            cd: c.cd_wing,
        });
    }
    items
}

#[allow(clippy::too_many_arguments)]
fn finish_hover(
    rotors: Vec<Rotor>,
    items: Vec<Item>,
    coax: Option<CoaxInfo>,
    rho: f64,
    weight: f64,
    p_hover: f64,
    p_total: f64,
    h: f64,
    c: &Coeffs,
) -> HoverOut {
    let block = blockage::blockage(&rotors, &items, rho, c.hub_cutout);
    let download_frac = block.fraction().clamp(0.0, 0.9);
    let pairs = blockage::pairs(&rotors, c.tip_warn, c.tip_loss_touch, 0.5);
    let interaction_frac = blockage::interaction_fraction(&pairs, rotors.len());
    let lost = (download_frac + interaction_frac).clamp(0.0, 0.9);
    let net_frac = 1.0 - lost;
    // Dikey denge: yalnız gövde bloklaması (download) itkiyi artırır; uç açıklığı kaybı sabit
    // güçte itki verimini düşürür, gereken itkiyi (N) değil gücü etkiler.
    let thrust_required = weight / (1.0 - download_frac);
    // İdeal güç ∝ T^1.5 (Leishman 2006, denklem 2.19).
    let ratio = (1.0 / net_frac).powf(1.5) - 1.0;
    let p_hover = if fin(p_hover) { p_hover.max(0.0) } else { 0.0 };
    let extra_power = p_hover * ratio;
    let p_total = if fin(p_total) && p_total > 0.0 {
        p_total
    } else {
        p_hover
    };
    let endurance_drop_pct = if p_total > 0.0 {
        100.0 * (1.0 - p_total / (p_total + extra_power))
    } else {
        0.0
    };
    // Tablo satırları (aynı adlı parçalar toplanır, ilk görülme sırası korunur).
    let mut rows: Vec<ItemRow> = Vec::new();
    let mut counted: Vec<bool> = vec![false; items.len()];
    for hit in &block.hits {
        let it = &items[hit.item];
        let idx = match rows
            .iter()
            .position(|r| r.name == it.name && r.kind == it.kind)
        {
            Some(i) => i,
            None => {
                rows.push(ItemRow {
                    name: it.name.clone(),
                    kind: it.kind,
                    count: 0,
                    area: 0.0,
                    force: 0.0,
                    pct: 0.0,
                    cd: it.cd,
                });
                rows.len() - 1
            }
        };
        if !counted[hit.item] {
            counted[hit.item] = true;
            rows[idx].count += 1;
        }
        rows[idx].area += hit.area;
        rows[idx].force += hit.force;
    }
    for r in &mut rows {
        r.pct = if block.total_thrust > 0.0 {
            100.0 * r.force / block.total_thrust
        } else {
            0.0
        };
    }
    let (v_i, r0) = rotors
        .first()
        .map(|r| {
            (
                momentum::induced_velocity_hover(r.thrust, rho, disk_area(r.r)),
                r.r,
            )
        })
        .unwrap_or((0.0, 0.0));
    HoverOut {
        rotors,
        items,
        block,
        rows,
        pairs,
        coax,
        rho,
        weight,
        v_i,
        v_far: 2.0 * v_i,
        r_slip_struct: slipstream_radius_at(0.0, v_i, r0, h),
        r_slip_far: r0 / 2f64.sqrt(),
        download_frac,
        interaction_frac,
        net_frac,
        thrust_required,
        p_hover,
        extra_power,
        extra_power_pct: 100.0 * ratio,
        endurance_drop_pct,
    }
}

/// Burundaki çekici (ya da kuyruktaki itici) pervanenin hava akımı.
fn fixed_wing_slip(p: &Project, m: &Model, c: &Coeffs, lay: &Layout) -> Option<SlipOut> {
    let d = p.d_prop;
    let w = m.weight.w;
    if !(pos(d) && pos(w) && pos(p.s) && pos(p.b) && pos(m.wing.c_r)) {
        return None;
    }
    let pusher = lay.motor_mount.mount_type == "pusher";
    let r = 0.5 * d;
    let rho = rho_of(m);
    let a = disk_area(r);
    let x_le = if fin(p.x_root_le) { p.x_root_le } else { 0.0 };
    let l_fuse = if pos(p.l_fuse) { p.l_fuse } else { 0.0 };
    let x_prop = if pusher { -l_fuse } else { 0.0 };
    let x_wing = -(x_le + 0.25 * m.wing.c_r);
    // Diskten kanada akış yönündeki uzaklık (+: kanat diskin arkasında, −: önünde).
    let z_wing = x_prop - x_wing;
    let z_fuse = x_prop + 0.5 * l_fuse;
    let half_d = if pos(p.d_fuse) { 0.5 * p.d_fuse } else { 0.0 };
    let taper = if fin(p.taper) {
        p.taper.clamp(0.0, 1.5)
    } else {
        1.0
    };
    let half_span = 0.5 * p.b;
    // Kanat profil sürüklemesi (planform alanı başına) ve gövde sürükleme alanı: sürükleme
    // dökümünün ilk iki satırı (0: kanat, 1: gövde; `calc::aero`).
    let cd0_wing = m.aero.items.first().map(|i| i.cd0).unwrap_or(0.0);
    let cd0_fuse_s = m.aero.items.get(1).map(|i| i.cd0 * p.s).unwrap_or(0.0);
    let cd0_wing = if fin(cd0_wing) {
        cd0_wing.max(0.0)
    } else {
        0.0
    };
    let cd0_fuse_s = if fin(cd0_fuse_s) {
        cd0_fuse_s.max(0.0)
    } else {
        0.0
    };
    let lift_k = if fin(c.slip_lift_factor) {
        c.slip_lift_factor.clamp(0.0, 1.5)
    } else {
        0.8
    };

    let point = |v: f64, thrust: f64, drag_ref: f64| -> SlipPoint {
        if !(pos(v) && pos(thrust)) {
            return SlipPoint::default();
        }
        let vi = induced_velocity_axial(thrust, v, rho, a);
        // Yalnız diskin akış yönündeki (z > 0) parçaları hava akımındadır; diskin önündeki
        // yüzeyler (itici düzende kanat ve gövde) akımın dışındadır.
        let (dv_w, r_w) = if z_wing > 0.0 {
            (
                added_velocity_at(vi, r, z_wing),
                slipstream_radius_at(v, vi, r, z_wing),
            )
        } else {
            (0.0, r)
        };
        // Hava akımındaki kanat: |y| ∈ [d/2, r_w] (pervane gövde ekseninde).
        let y1 = r_w.min(half_span);
        let mut s_imm = 0.0;
        if z_wing > 0.0 && y1 > half_d {
            let n = 40;
            let dy = (y1 - half_d) / n as f64;
            for k in 0..n {
                let y = half_d + (k as f64 + 0.5) * dy;
                let (_, ch) = blockage::wing_station(0.0, m.wing.c_r, taper, half_span, 0.0, y);
                s_imm += 2.0 * ch * dy;
            }
        }
        let q = 0.5 * rho * v * v;
        let v_w = v + dv_w;
        let dq_w = 0.5 * rho * v_w * v_w - q;
        // Gövde: kesitin hava akımında kalan oranı ≈ min(1, r/(d/2)).
        let (dv_f, r_f) = if z_fuse > 0.0 {
            (
                added_velocity_at(vi, r, z_fuse),
                slipstream_radius_at(v, vi, r, z_fuse),
            )
        } else {
            (0.0, r)
        };
        let f_frac = if z_fuse > 0.0 && half_d > 0.0 {
            (r_f / half_d).min(1.0)
        } else {
            0.0
        };
        let dq_f = 0.5 * rho * (v + dv_f).powi(2) - q;
        let dd_wing = cd0_wing * s_imm * dq_w;
        let dd_fuse = cd0_fuse_s * f_frac * dq_f;
        let dd_total = dd_wing + dd_fuse;
        if !pos(q) {
            return SlipPoint::default();
        }
        let cl = w / (q * p.s);
        let dl = lift_k * cl * s_imm * dq_w;
        SlipPoint {
            v,
            thrust,
            v_i: vi,
            v_slip: slipstream_velocity(v, vi),
            v_at_wing: v_w,
            r_at_wing: r_w,
            s_imm,
            frac_wing: s_imm / p.s,
            dd_wing,
            dd_fuse,
            dd_total,
            dd_pct: if pos(drag_ref) {
                100.0 * dd_total / drag_ref
            } else {
                0.0
            },
            dl,
            dl_pct: 100.0 * dl / w,
            cl,
        }
    };
    // Seyir: itki = sürükleme (W / (L/D)).
    let cruise = point(p.v_cr, m.prop.t_cr, m.prop.t_cr);
    // Düşük hız: 1.2·V_stall, tam gaz (statik ideal güç sabit) — kalkış/tırmanış durumu.
    let v_low = 1.2 * m.aero.v_stall;
    let t_low = thrust_at_ideal_power(m.prop.p_ideal, v_low, rho, a, m.prop.t_static);
    let d_low = if pos(v_low) && pos(m.aero.k) {
        let q = 0.5 * rho * v_low * v_low;
        if pos(q) {
            let cl = w / (q * p.s);
            q * p.s * (m.aero.cd0 + m.aero.k * cl * cl)
        } else {
            0.0
        }
    } else {
        0.0
    };
    let low = point(v_low, t_low, d_low);
    let h = if pos(c.rotor_height) {
        c.rotor_height
    } else {
        0.035
    };
    let items = airframe_items(p, m, c, h);
    Some(SlipOut {
        pusher,
        d_prop: d,
        x_prop,
        x_wing,
        cruise,
        low,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate as calc_eval};

    fn finite_hover(h: &HoverOut, tag: &str) {
        for (name, v) in [
            ("download", h.download_frac),
            ("inter", h.interaction_frac),
            ("net", h.net_frac),
            ("treq", h.thrust_required),
            ("extra", h.extra_power),
            ("pct", h.extra_power_pct),
            ("vi", h.v_i),
            ("rs", h.r_slip_struct),
            ("end", h.endurance_drop_pct),
        ] {
            assert!(v.is_finite(), "{tag}: {name} = {v}");
        }
        for r in &h.rows {
            assert!(
                r.force.is_finite() && r.area.is_finite(),
                "{tag}: {}",
                r.name
            );
        }
    }

    fn finite_slip(s: &SlipPoint, tag: &str) {
        for v in [
            s.v_i,
            s.v_slip,
            s.s_imm,
            s.dd_total,
            s.dd_pct,
            s.dl,
            s.dl_pct,
            s.r_at_wing,
        ] {
            assert!(v.is_finite(), "{tag}: {s:?}");
        }
    }

    #[test]
    fn all_types_and_examples_compute_without_nan() {
        let lib = Library::embedded();
        let mut cases: Vec<(String, Project)> = DroneType::ALL
            .iter()
            .map(|dt| {
                let mut p = Project::default();
                p.set_drone_type(*dt);
                (format!("{dt:?}"), p)
            })
            .collect();
        for f in [
            "fixed_wing_2kg_1600mm.json",
            "flying_wing_1200mm.json",
            "hexa_550_x.json",
            "quad_250_x.json",
            "quad_275_x.json",
            "vtol_quadplane.json",
        ] {
            cases.push((f.into(), crate::project::load_example(f).unwrap()));
        }
        for (tag, p) in &cases {
            let m = calc_eval(p, &lib);
            let before = serde_json::to_string(p).unwrap();
            let out = evaluate(p, &m, &Coeffs::default());
            assert_eq!(before, serde_json::to_string(p).unwrap());
            match p.drone_type {
                DroneType::Multirotor => {
                    let h = out.hover.as_ref().expect("multirotor askı sonucu");
                    finite_hover(h, tag);
                    assert!(out.slip.is_none());
                    assert!(
                        h.download_frac > 0.0 && h.download_frac < 0.25,
                        "{tag}: {}",
                        h.download_frac
                    );
                }
                DroneType::VtolQuadplane => {
                    finite_hover(out.hover.as_ref().expect("VTOL askı"), tag);
                    let s = out.slip.as_ref().expect("VTOL seyir");
                    finite_slip(&s.cruise, tag);
                    finite_slip(&s.low, tag);
                }
                _ => {
                    assert!(out.hover.is_none());
                    let s = out.slip.as_ref().expect("sabit kanat hava akımı");
                    finite_slip(&s.cruise, tag);
                    finite_slip(&s.low, tag);
                    assert!(s.cruise.v_slip > s.cruise.v, "{tag}");
                    assert!(s.cruise.dd_total >= 0.0 && s.cruise.dl >= 0.0, "{tag}");
                }
            }
        }
    }

    #[test]
    fn broken_inputs_do_not_panic_or_nan() {
        let lib = Library::embedded();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            p.b = f64::NAN;
            p.d_fuse = f64::NAN;
            p.v_cr = f64::NAN;
            p.mr_frame_size = 0.0;
            p.mr_d_prop = f64::NAN;
            p.mr_arm_count = 0;
            let m = calc_eval(&p, &lib);
            let out = evaluate(&p, &m, &Coeffs::default());
            if let Some(h) = &out.hover {
                finite_hover(h, "broken");
            }
            if let Some(s) = &out.slip {
                finite_slip(&s.cruise, "broken");
            }
        }
    }

    #[test]
    fn broken_coefficients_do_not_panic_or_nan() {
        // Primary geometry stays valid so the calculation bodies (not just the guards) run.
        let lib = Library::embedded();
        let bad = Coeffs {
            cd_arm: f64::NAN,
            arm_width: f64::NAN,
            cd_body: f64::NAN,
            cd_component: -1.0,
            component_density: 0.0,
            cd_wing: f64::INFINITY,
            cd_fuselage: f64::NAN,
            cd_boom: f64::NAN,
            rotor_height: f64::NAN,
            hub_cutout: f64::NAN,
            tip_warn: f64::NAN,
            tip_loss_touch: f64::NAN,
            slip_lift_factor: f64::NAN,
        };
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            let m = calc_eval(&p, &lib);
            let out = evaluate(&p, &m, &bad);
            let mut ran = false;
            if let Some(h) = &out.hover {
                finite_hover(h, "bad coeffs");
                ran = true;
            }
            if let Some(s) = &out.slip {
                finite_slip(&s.cruise, "bad coeffs");
                finite_slip(&s.low, "bad coeffs");
                ran = true;
            }
            assert!(ran || !dt.has_wing(), "{dt:?}: nothing computed");
        }
    }

    #[test]
    fn required_thrust_ignores_tip_interaction_but_power_does_not() {
        let lib = Library::embedded();
        let p = crate::project::load_example("quad_250_x.json").unwrap();
        let m = calc_eval(&p, &lib);
        let base = evaluate(&p, &m, &Coeffs::default()).hover.unwrap();
        // Crowd the tips together: interaction loss rises, blockage download does not.
        let crowded = Coeffs {
            tip_loss_touch: 0.3,
            tip_warn: 5.0,
            ..Default::default()
        };
        let h = evaluate(&p, &m, &crowded).hover.unwrap();
        assert!(h.interaction_frac > base.interaction_frac);
        let expect = h.weight / (1.0 - h.download_frac);
        assert!((h.thrust_required - expect).abs() < 1e-9 * expect);
        assert!((base.thrust_required - base.weight / (1.0 - base.download_frac)).abs() < 1e-9);
        assert!(h.extra_power_pct > base.extra_power_pct);
    }

    #[test]
    fn surfaces_upstream_of_the_propeller_are_not_in_the_slipstream() {
        let lib = Library::embedded();
        let p = crate::project::load_example("flying_wing_1200mm.json").unwrap();
        let m = calc_eval(&p, &lib);
        let lay = Layout::new(&p, &m);
        assert_eq!(lay.motor_mount.mount_type, "pusher");
        let s = evaluate(&p, &m, &Coeffs::default()).slip.unwrap();
        assert!(s.pusher);
        for q in [&s.cruise, &s.low] {
            assert_eq!(q.s_imm, 0.0, "{q:?}");
            assert_eq!(q.dd_total, 0.0, "{q:?}");
            assert_eq!(q.dl, 0.0, "{q:?}");
            assert_eq!(q.v_at_wing, q.v, "{q:?}");
        }
        // A tractor on a conventional fixed wing still immerses the wing.
        let p = crate::project::load_example("fixed_wing_2kg_1600mm.json").unwrap();
        let m = calc_eval(&p, &lib);
        let s = evaluate(&p, &m, &Coeffs::default()).slip.unwrap();
        assert!(!s.pusher && s.cruise.s_imm > 0.0);
    }

    #[test]
    fn wider_arms_raise_quad_download_and_coax_is_reported() {
        let lib = Library::embedded();
        let mut p = crate::project::load_example("quad_250_x.json").unwrap();
        let m = calc_eval(&p, &lib);
        let f = |w: f64| {
            let c = Coeffs {
                arm_width: w,
                ..Default::default()
            };
            evaluate(&p, &m, &c).hover.unwrap().download_frac
        };
        assert!(f(0.03) > f(0.015));
        p.mr_coaxial = true;
        let m = calc_eval(&p, &lib);
        let h = evaluate(&p, &m, &Coeffs::default()).hover.unwrap();
        let cx = h.coax.expect("koaksiyel bilgi");
        assert!(cx.penalty_pct > 0.0);
    }
}
