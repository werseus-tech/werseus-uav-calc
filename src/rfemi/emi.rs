// SPDX-License-Identifier: AGPL-3.0-or-later
//! Anten yerleşimi ve EMI kural denetimi: yerleşim geometrisinden mesafeler, karbon gölgelemesi,
//! polarizasyon uyumsuzluğu ve harmonik çakışmaları. Kurallar basit ön denetimdir; ölçümün yerini tutmaz.

use super::link::{Link, LinkKind, compute};
use crate::cad::layout::Layout;
use crate::cad::outputs::PartId;
use crate::i18n::t;
use crate::project::{DroneType, Project};

/// Milimetre cinsinden 3B nokta (x: gövde ekseni burundan kuyruğa, y: yanal, z: yukarı).
pub type P3 = [f64; 3];

/// Antenin takılabileceği yerler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AntennaLoc {
    Nose,
    Tail,
    WingtipL,
    WingtipR,
    FuselageTop,
    Belly,
    ArmEnd,
    /// Gövde / merkez plaka içi (kapalı gövde).
    Inside,
}

impl AntennaLoc {
    pub const ALL: [AntennaLoc; 8] = [
        AntennaLoc::Nose,
        AntennaLoc::Tail,
        AntennaLoc::WingtipL,
        AntennaLoc::WingtipR,
        AntennaLoc::FuselageTop,
        AntennaLoc::Belly,
        AntennaLoc::ArmEnd,
        AntennaLoc::Inside,
    ];
    pub fn key(self) -> &'static str {
        match self {
            AntennaLoc::Nose => "rfemi.loc.nose",
            AntennaLoc::Tail => "rfemi.loc.tail",
            AntennaLoc::WingtipL => "rfemi.loc.wingtip_l",
            AntennaLoc::WingtipR => "rfemi.loc.wingtip_r",
            AntennaLoc::FuselageTop => "rfemi.loc.top",
            AntennaLoc::Belly => "rfemi.loc.belly",
            AntennaLoc::ArmEnd => "rfemi.loc.arm_end",
            AntennaLoc::Inside => "rfemi.loc.inside",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    Motor,
    Esc,
    Battery,
    PowerLine,
    Vtx,
}

impl SourceKind {
    pub fn key(self) -> &'static str {
        match self {
            SourceKind::Motor => "rfemi.src.motor",
            SourceKind::Esc => "rfemi.src.esc",
            SourceKind::Battery => "rfemi.src.battery",
            SourceKind::PowerLine => "rfemi.src.power",
            SourceKind::Vtx => "rfemi.src.vtx",
        }
    }
}

/// Gürültü kaynağı: nokta ya da (güç kablosu için) doğru parçası.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub kind: SourceKind,
    pub index: usize,
    pub a: P3,
    pub b: Option<P3>,
}

impl Source {
    pub fn label(&self) -> String {
        match self.kind {
            SourceKind::Battery | SourceKind::Vtx => t(self.kind.key()).to_string(),
            _ => format!("{} {}", t(self.kind.key()), self.index + 1),
        }
    }
}

/// Yerleşimden türetilen anten noktaları ve gürültü kaynakları (mm).
#[derive(Clone, Debug)]
pub struct Scene {
    pub anchors: Vec<(AntennaLoc, P3)>,
    /// Motorlar, ESC'ler, akü ve güç kabloları (VTX kullanıcı seçimine göre eklenir).
    pub sources: Vec<Source>,
    pub length_mm: f64,
    pub radius_mm: f64,
    pub multirotor: bool,
}

fn lerp(a: P3, b: P3, s: f64) -> P3 {
    [
        a[0] + (b[0] - a[0]) * s,
        a[1] + (b[1] - a[1]) * s,
        a[2] + (b[2] - a[2]) * s,
    ]
}

/// Nokta-nokta mesafesi (mm).
pub fn dist(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Noktanın doğru parçasına en kısa mesafesi (mm).
pub fn dist_to_segment(p: P3, a: P3, b: P3) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let s = if l2 < 1e-12 {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / l2).clamp(0.0, 1.0)
    };
    dist(p, lerp(a, b, s))
}

impl Scene {
    pub fn new(p: &Project, lay: &Layout) -> Scene {
        let multirotor = p.drone_type == DroneType::Multirotor;
        let length = lay.fuselage.length_mm.max(1.0);
        let radius = (lay.fuselage.max_diameter_mm / 2.0).max(5.0);
        let motors: Vec<P3> = lay
            .instances
            .iter()
            .filter(|i| {
                matches!(
                    i.id,
                    PartId::MotorMount | PartId::LiftMotorMount | PartId::MrMotorMount
                )
            })
            .map(|i| i.frame.t)
            .collect();
        let finite = |q: P3| q.iter().all(|v| v.is_finite());
        let mut motors: Vec<P3> = motors.into_iter().filter(|q| finite(*q)).collect();
        let (center, mut anchors, length_mm, radius_mm);
        if multirotor {
            let r_plate = lay.mr_plate.plate_radius_mm.max(20.0);
            let reach = motors
                .iter()
                .map(|m| (m[0] * m[0] + m[1] * m[1]).sqrt())
                .fold(r_plate, f64::max);
            if motors.is_empty() {
                motors.push([reach, 0.0, 0.0]);
            }
            let m0 = motors[0];
            let r_m0 = (m0[0] * m0[0] + m0[1] * m0[1]).sqrt();
            let arm_end = if r_m0 > 1e-6 {
                [
                    m0[0] + 25.0 * m0[0] / r_m0,
                    m0[1] + 25.0 * m0[1] / r_m0,
                    m0[2],
                ]
            } else {
                [m0[0] + 25.0, m0[1], m0[2]]
            };
            center = [0.0, 0.0, 5.0];
            length_mm = 0.0;
            radius_mm = r_plate;
            anchors = vec![
                (AntennaLoc::Nose, [-r_plate, 0.0, 0.0]),
                (AntennaLoc::Tail, [r_plate, 0.0, 0.0]),
                (AntennaLoc::WingtipL, [0.0, reach, 0.0]),
                (AntennaLoc::WingtipR, [0.0, -reach, 0.0]),
                (AntennaLoc::FuselageTop, [0.0, 0.0, 40.0]),
                (AntennaLoc::Belly, [0.0, 0.0, -15.0]),
                (AntennaLoc::ArmEnd, arm_end),
                (AntennaLoc::Inside, center),
            ];
        } else {
            if motors.is_empty() {
                motors.push([-30.0, 0.0, 0.0]);
            }
            let wg = lay.wing.geom();
            let span = wg.span.max(1.0);
            let dih = lay.wing.dihedral_deg.to_radians();
            let x_tip = p.x_root_le * 1000.0 + wg.le_x_at(span) + 0.25 * wg.chord_at(span);
            let (y_tip, z_tip) = (span * dih.cos(), span * dih.sin());
            let x_mid = 0.45 * length;
            center = [0.4 * length, 0.0, 0.0];
            length_mm = length;
            radius_mm = radius;
            let lift = lay
                .instances
                .iter()
                .find(|i| i.id == PartId::LiftMotorMount)
                .map(|i| i.frame.t)
                .filter(|q| finite(*q))
                .map(|q| {
                    let sign = if q[1] >= 0.0 { 1.0 } else { -1.0 };
                    [q[0], q[1] + 25.0 * sign, q[2]]
                });
            anchors = vec![
                (AntennaLoc::Nose, [0.0, 0.0, 0.0]),
                (AntennaLoc::Tail, [length, 0.0, 0.0]),
                (AntennaLoc::WingtipL, [x_tip, y_tip, z_tip]),
                (AntennaLoc::WingtipR, [x_tip, -y_tip, z_tip]),
                (AntennaLoc::FuselageTop, [x_mid, 0.0, radius]),
                (AntennaLoc::Belly, [x_mid, 0.0, -radius]),
                (
                    AntennaLoc::ArmEnd,
                    lift.unwrap_or([length + 25.0, 0.0, 0.0]),
                ),
                (AntennaLoc::Inside, center),
            ];
        }
        anchors.retain(|(_, q)| finite(*q));
        let mut sources = Vec::new();
        for (k, m) in motors.iter().enumerate() {
            sources.push(Source {
                kind: SourceKind::Motor,
                index: k,
                a: *m,
                b: None,
            });
        }
        // ESC motorun yakınında, motordan akü yönünde %20 içeridedir (basit varsayım).
        let escs: Vec<P3> = motors.iter().map(|m| lerp(*m, center, 0.2)).collect();
        sources.push(Source {
            kind: SourceKind::Battery,
            index: 0,
            a: center,
            b: None,
        });
        for (k, e) in escs.iter().enumerate() {
            sources.push(Source {
                kind: SourceKind::Esc,
                index: k,
                a: *e,
                b: None,
            });
            sources.push(Source {
                kind: SourceKind::PowerLine,
                index: k,
                a: center,
                b: Some(*e),
            });
        }
        Scene {
            anchors,
            sources,
            length_mm,
            radius_mm,
            multirotor,
        }
    }

    pub fn point(&self, loc: AntennaLoc) -> P3 {
        self.anchors
            .iter()
            .find(|(l, _)| *l == loc)
            .map(|(_, q)| *q)
            .unwrap_or([0.0, 0.0, 0.0])
    }

    /// Kullanıcının seçtiği yerde VTX (kaynak) ekler.
    pub fn sources_with_vtx(&self, loc: AntennaLoc) -> Vec<Source> {
        let mut s = self.sources.clone();
        s.push(Source {
            kind: SourceKind::Vtx,
            index: 0,
            a: self.point(loc),
            b: None,
        });
        s
    }

    /// Video bağı varsa `loc` konumunda VTX kaynağını da içeren kaynak listesini döndürür.
    pub fn sources_for(&self, links: &[Link], vtx_loc: AntennaLoc) -> Vec<Source> {
        if links.iter().any(|l| l.kind == LinkKind::Video) {
            self.sources_with_vtx(vtx_loc)
        } else {
            self.sources.clone()
        }
    }
}

/// Ayarlanabilir eşikler (cm) ve seçenekler.
#[derive(Clone, Debug)]
pub struct EmiConfig {
    pub vtx_loc: AntennaLoc,
    /// Gövde / plakalar karbon fiber (RF'yi perdeler).
    pub carbon_body: bool,
    pub gnss_vtx_cm: f64,
    pub gnss_esc_cm: f64,
    pub gnss_motor_cm: f64,
    pub gnss_power_cm: f64,
    pub other_vtx_cm: f64,
    pub other_esc_cm: f64,
}

impl Default for EmiConfig {
    fn default() -> Self {
        EmiConfig {
            vtx_loc: AntennaLoc::Inside,
            carbon_body: false,
            gnss_vtx_cm: 15.0,
            gnss_esc_cm: 10.0,
            gnss_motor_cm: 10.0,
            gnss_power_cm: 8.0,
            other_vtx_cm: 8.0,
            other_esc_cm: 5.0,
        }
    }
}

impl EmiConfig {
    /// Anten türü / kaynak çiftinin eşiği (cm); kural yoksa `None`.
    pub fn threshold_cm(&self, kind: LinkKind, src: SourceKind) -> Option<f64> {
        match (kind, src) {
            (LinkKind::Gnss, SourceKind::Vtx) => Some(self.gnss_vtx_cm),
            (LinkKind::Gnss, SourceKind::Esc) => Some(self.gnss_esc_cm),
            (LinkKind::Gnss, SourceKind::Motor) => Some(self.gnss_motor_cm),
            (LinkKind::Gnss, SourceKind::PowerLine | SourceKind::Battery) => {
                Some(self.gnss_power_cm)
            }
            (LinkKind::Video, SourceKind::Vtx) => None,
            (_, SourceKind::Vtx) => Some(self.other_vtx_cm),
            (_, SourceKind::Esc) => Some(self.other_esc_cm),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Danger,
}

/// Düz anlatımlı uyarı: `key` çeviri anahtarı, `args` yer tutucu değerleri.
#[derive(Clone, Debug)]
pub struct Warning {
    pub severity: Severity,
    pub key: &'static str,
    pub args: Vec<String>,
}

/// Bir anten ile bir gürültü kaynağı arasındaki mesafe satırı.
#[derive(Clone, Debug)]
pub struct DistRow {
    pub link: usize,
    pub source: Source,
    pub cm: f64,
    pub threshold_cm: Option<f64>,
}

impl DistRow {
    pub fn violated(&self) -> bool {
        self.threshold_cm.is_some_and(|t| self.cm < t)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub rows: Vec<DistRow>,
    pub warnings: Vec<Warning>,
}

/// Bir TX'in n. harmoniği (verici bant genişliği dahil) alıcı bandına düşüyor mu?
pub fn harmonic_hits(f_tx_mhz: f64, bw_tx_mhz: f64, n: u32, f_rx_mhz: f64, bw_rx_mhz: f64) -> bool {
    (f_tx_mhz * n as f64 - f_rx_mhz).abs() <= (n as f64 * bw_tx_mhz + bw_rx_mhz) / 2.0
}

fn f0(x: f64) -> String {
    format!("{x:.0}")
}
fn f1(x: f64) -> String {
    format!("{x:.1}")
}

pub fn analyze(links: &[Link], scene: &Scene, cfg: &EmiConfig) -> Report {
    let mut rep = Report::default();
    let sources = scene.sources_for(links, cfg.vtx_loc);

    // Mesafe kuralları.
    for (i, l) in links.iter().enumerate() {
        let ant = scene.point(l.loc);
        for s in &sources {
            let mm = match s.b {
                Some(b) => dist_to_segment(ant, s.a, b),
                None => dist(ant, s.a),
            };
            let thr = cfg.threshold_cm(l.kind, s.kind);
            if thr.is_none() {
                continue;
            }
            let row = DistRow {
                link: i,
                source: *s,
                cm: mm / 10.0,
                threshold_cm: thr,
            };
            if row.violated() {
                let key = if l.kind == LinkKind::Gnss {
                    "rfemi.w.gnss_close"
                } else {
                    "rfemi.w.ant_close"
                };
                rep.warnings.push(Warning {
                    severity: Severity::Warn,
                    key,
                    args: vec![
                        l.name.clone(),
                        s.label(),
                        f1(row.cm),
                        f1(thr.unwrap_or(0.0)),
                    ],
                });
            }
            rep.rows.push(row);
        }
    }

    // Karbon gölgelemesi.
    if cfg.carbon_body {
        for l in links {
            let loc = t(l.loc.key()).to_string();
            let gnss = l.kind == LinkKind::Gnss;
            match l.loc {
                AntennaLoc::Inside => rep.warnings.push(Warning {
                    severity: Severity::Danger,
                    key: "rfemi.w.carbon_inside",
                    args: vec![l.name.clone()],
                }),
                AntennaLoc::Belly if gnss => rep.warnings.push(Warning {
                    severity: Severity::Danger,
                    key: "rfemi.w.carbon_belly",
                    args: vec![l.name.clone(), loc],
                }),
                AntennaLoc::FuselageTop
                    if matches!(l.kind, LinkKind::Video | LinkKind::Rc | LinkKind::Telemetry) =>
                {
                    rep.warnings.push(Warning {
                        severity: Severity::Warn,
                        key: "rfemi.w.carbon_top_video",
                        args: vec![l.name.clone()],
                    })
                }
                _ => {}
            }
        }
    }

    // Bağ bütçesi ve polarizasyon.
    for l in links {
        let r = compute(l);
        if r.margin_db < 0.0 {
            rep.warnings.push(Warning {
                severity: Severity::Danger,
                key: "rfemi.w.margin_neg",
                args: vec![l.name.clone(), f1(r.margin_db)],
            });
        }
        if l.fixed_rx_dbm.is_none() && l.dist_km > 0.0 && r.clearance_m < r.min_height_m {
            rep.warnings.push(Warning {
                severity: Severity::Warn,
                key: "rfemi.w.fresnel",
                args: vec![l.name.clone(), f1(r.clearance_m), f1(r.min_height_m)],
            });
        }
        if r.pol_loss_db > 3.5 {
            rep.warnings.push(Warning {
                severity: Severity::Warn,
                key: "rfemi.w.pol_mismatch",
                args: vec![l.name.clone(), f1(r.pol_loss_db)],
            });
        }
    }

    // Harmonikler, TX-RX anten mesafesi ve GNSS yakını verici kontrolleri.
    for (i, tx) in links.iter().enumerate() {
        if !tx.kind.onboard_tx() {
            continue;
        }
        let tx_pt = scene.point(tx.loc);
        for (j, rx) in links.iter().enumerate() {
            if i == j || !rx.kind.onboard_rx() {
                continue;
            }
            for n in 2..=3u32 {
                if harmonic_hits(tx.f_mhz, tx.bw_mhz, n, rx.f_mhz, rx.bw_mhz) {
                    rep.warnings.push(Warning {
                        severity: Severity::Warn,
                        key: "rfemi.w.harmonic",
                        args: vec![
                            tx.name.clone(),
                            n.to_string(),
                            f0(tx.f_mhz),
                            f0(tx.f_mhz * n as f64),
                            rx.name.clone(),
                        ],
                    });
                }
            }
            if rx.kind == LinkKind::Gnss && (1100.0..=1400.0).contains(&tx.f_mhz) {
                rep.warnings.push(Warning {
                    severity: Severity::Warn,
                    key: "rfemi.w.near_gnss",
                    args: vec![tx.name.clone(), f0(tx.f_mhz)],
                });
            }
            let covered_by_vtx_source = tx.kind == LinkKind::Video && tx.loc == cfg.vtx_loc;
            let symmetric_dup = rx.kind.onboard_tx() && tx.kind.onboard_rx() && i > j;
            if !covered_by_vtx_source && !symmetric_dup {
                let cm = dist(tx_pt, scene.point(rx.loc)) / 10.0;
                let thr = if rx.kind == LinkKind::Gnss {
                    cfg.gnss_vtx_cm
                } else {
                    cfg.other_vtx_cm
                };
                if cm < thr {
                    let key = if rx.kind == LinkKind::Gnss {
                        "rfemi.w.gnss_close"
                    } else {
                        "rfemi.w.ant_close"
                    };
                    rep.warnings.push(Warning {
                        severity: Severity::Warn,
                        key,
                        args: vec![rx.name.clone(), tx.name.clone(), f1(cm), f1(thr)],
                    });
                }
            }
        }
    }
    // Aynı frekans bandındaki bağ çiftleri.
    for i in 0..links.len() {
        for j in (i + 1)..links.len() {
            let (a, b) = (&links[i], &links[j]);
            if (a.f_mhz - b.f_mhz).abs() <= (a.bw_mhz + b.bw_mhz) / 2.0 {
                rep.warnings.push(Warning {
                    severity: Severity::Warn,
                    key: "rfemi.w.co_band",
                    args: vec![a.name.clone(), b.name.clone()],
                });
            }
        }
    }
    rep.warnings.sort_by_key(|w| std::cmp::Reverse(w.severity));
    rep
}

/// Üstten görünüş şeması için (x, y) sınırları (mm): (min_x, max_x, min_y, max_y).
pub fn plan_bounds(scene: &Scene, extra: &[P3]) -> (f64, f64, f64, f64) {
    let mut b = (0.0f64, scene.length_mm, -scene.radius_mm, scene.radius_mm);
    let pts = scene
        .anchors
        .iter()
        .map(|(_, q)| *q)
        .chain(scene.sources.iter().flat_map(|s| [s.a, s.b.unwrap_or(s.a)]))
        .chain(extra.iter().copied());
    for q in pts {
        b.0 = b.0.min(q[0]);
        b.1 = b.1.max(q[0]);
        b.2 = b.2.min(q[1]);
        b.3 = b.3.max(q[1]);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};
    use crate::rfemi::link::Band;

    fn manual_scene() -> Scene {
        Scene {
            anchors: vec![
                (AntennaLoc::Nose, [0.0, 0.0, 0.0]),
                (AntennaLoc::Tail, [500.0, 0.0, 0.0]),
                (AntennaLoc::FuselageTop, [250.0, 0.0, 50.0]),
                (AntennaLoc::Belly, [250.0, 0.0, -50.0]),
                (AntennaLoc::Inside, [200.0, 0.0, 0.0]),
            ],
            sources: vec![],
            length_mm: 500.0,
            radius_mm: 50.0,
            multirotor: false,
        }
    }

    #[test]
    fn harmonic_433_hits_868_band() {
        assert!(harmonic_hits(433.0, 2.0, 2, 868.0, 7.0));
        assert!(harmonic_hits(433.92, 2.0, 2, 868.0, 7.0));
        assert!(!harmonic_hits(433.0, 2.0, 3, 868.0, 7.0));
        // 1.3 GHz dar bant ikinci harmonik 2.6 GHz, 2.4 GHz bandının dışında; 1.25 GHz içinde.
        assert!(!harmonic_hits(1300.0, 10.0, 2, 2440.0, 80.0));
        assert!(harmonic_hits(1220.0, 10.0, 2, 2440.0, 80.0));
        // 1.2 GHz video ön ayarı (1280 MHz, bw 100 MHz) 2.4 GHz (2440 MHz, bw 80 MHz) ile çakışır.
        let g12 = Band::Ghz1_2.info();
        let g24 = Band::Ghz2_4.info();
        assert!(harmonic_hits(
            g12.f_mhz, g12.bw_mhz, 2, g24.f_mhz, g24.bw_mhz
        ));
        // 5.8 GHz harmoniği hiçbir bağın bandına düşmez.
        let g58 = Band::Ghz5_8.info();
        for b in Band::ALL {
            let i = b.info();
            assert!(
                !harmonic_hits(g58.f_mhz, g58.bw_mhz, 2, i.f_mhz, i.bw_mhz),
                "{}",
                i.label
            );
        }
    }

    #[test]
    fn harmonic_warning_between_links() {
        let mut tel = Link::typical(LinkKind::Telemetry);
        tel.f_mhz = 433.0;
        let rc = Link::typical(LinkKind::Rc); // 868 MHz
        let rep = analyze(&[tel, rc], &manual_scene(), &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.harmonic"));

        let mut vtx = Link::typical(LinkKind::Video);
        vtx.apply_band(Band::Ghz1_2);
        let mut rc24 = Link::typical(LinkKind::Rc);
        rc24.apply_band(Band::Ghz2_4);
        let rep = analyze(&[vtx, rc24], &manual_scene(), &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.harmonic"));
    }

    #[test]
    fn vtx_near_gnss_warning() {
        let mut vtx = Link::typical(LinkKind::Video);
        vtx.f_mhz = 1280.0;
        let gnss = Link::typical(LinkKind::Gnss);
        let rep = analyze(&[vtx, gnss], &manual_scene(), &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.near_gnss"));
    }

    #[test]
    fn distance_rule_triggers_and_clears_at_threshold() {
        let sc = manual_scene();
        let mut gnss = Link::typical(LinkKind::Gnss);
        gnss.loc = AntennaLoc::FuselageTop; // (250, 0, 50)
        let mut vtx = Link::typical(LinkKind::Video);
        vtx.loc = AntennaLoc::Tail; // (500, 0, 0): 25.5 cm
        let mut cfg = EmiConfig {
            vtx_loc: AntennaLoc::Belly, // (250, 0, -50): 10 cm
            ..EmiConfig::default()
        };
        let links = [gnss, vtx];
        let has = |cfg: &EmiConfig| {
            analyze(&links, &sc, cfg)
                .warnings
                .iter()
                .any(|w| w.key == "rfemi.w.gnss_close")
        };
        cfg.gnss_vtx_cm = 10.5;
        assert!(has(&cfg));
        cfg.gnss_vtx_cm = 9.5;
        assert!(!has(&cfg));
    }

    #[test]
    fn vtx_source_absent_without_video_and_tx_antenna_checked() {
        let sc = manual_scene();
        let mut gnss = Link::typical(LinkKind::Gnss);
        gnss.name = "GNSS".into();
        gnss.loc = AntennaLoc::FuselageTop;
        let cfg = EmiConfig {
            vtx_loc: AntennaLoc::FuselageTop,
            ..EmiConfig::default()
        };
        // Video bağı yokken VTX kaynağı eklenmez.
        let rep = analyze(std::slice::from_ref(&gnss), &sc, &cfg);
        assert!(rep.rows.iter().all(|r| r.source.kind != SourceKind::Vtx));
        assert!(!rep.warnings.iter().any(|w| w.key == "rfemi.w.gnss_close"));

        // Video anteni ile GNSS aynı noktadayken cfg.vtx_loc uzakta olsa bile uyarır.
        let mut vtx = Link::typical(LinkKind::Video);
        vtx.name = "Video".into();
        vtx.loc = AntennaLoc::FuselageTop;
        let far_cfg = EmiConfig {
            vtx_loc: AntennaLoc::Tail,
            ..EmiConfig::default()
        };
        let rep = analyze(&[gnss.clone(), vtx], &sc, &far_cfg);
        assert!(
            rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.gnss_close" && w.args[1] == "Video")
        );

        // Telemetri verici anteni ile RC alıcı anteni aynı noktadayken uyarır.
        let mut tel = Link::typical(LinkKind::Telemetry);
        tel.name = "Tel".into();
        tel.loc = AntennaLoc::Tail;
        let mut rc = Link::typical(LinkKind::Rc);
        rc.name = "RC".into();
        rc.loc = AntennaLoc::Tail;
        let rep = analyze(&[tel, rc], &sc, &EmiConfig::default());
        assert!(
            rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.ant_close" && w.args[0] == "RC" && w.args[1] == "Tel")
        );
    }

    #[test]
    fn point_to_segment_distance() {
        let d = dist_to_segment([5.0, 3.0, 0.0], [0.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        assert!((d - 3.0).abs() < 1e-12);
        let d = dist_to_segment([-4.0, 3.0, 0.0], [0.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
        assert!((d - 5.0).abs() < 1e-12);
    }

    #[test]
    fn carbon_shadowing_and_polarization_warnings() {
        let sc = manual_scene();
        let mut gnss = Link::typical(LinkKind::Gnss);
        gnss.loc = AntennaLoc::Belly;
        let cfg = EmiConfig {
            carbon_body: true,
            ..EmiConfig::default()
        };
        let rep = analyze(std::slice::from_ref(&gnss), &sc, &cfg);
        assert!(
            rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.carbon_belly" && w.severity == Severity::Danger)
        );
        gnss.loc = AntennaLoc::FuselageTop;
        let rep = analyze(std::slice::from_ref(&gnss), &sc, &cfg);
        assert!(
            !rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.carbon_top_video")
        );
        gnss.loc = AntennaLoc::Inside;
        let rep = analyze(std::slice::from_ref(&gnss), &sc, &cfg);
        assert!(
            rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.carbon_inside")
        );
        // RC ve Telemetri gövde altında uyarı vermez, gövde üstünde yer istasyonu gölgelemesi verir.
        for k in [LinkKind::Rc, LinkKind::Telemetry, LinkKind::Video] {
            let mut l = Link::typical(k);
            l.loc = AntennaLoc::Belly;
            let rep = analyze(std::slice::from_ref(&l), &sc, &cfg);
            assert!(
                !rep.warnings.iter().any(|w| w.key == "rfemi.w.carbon_belly"),
                "{k:?}"
            );
            l.loc = AntennaLoc::FuselageTop;
            let rep = analyze(std::slice::from_ref(&l), &sc, &cfg);
            assert!(
                rep.warnings
                    .iter()
                    .any(|w| w.key == "rfemi.w.carbon_top_video"),
                "{k:?}"
            );
        }
        let mut rc = Link::typical(LinkKind::Rc);
        rc.pol_angle_deg = 70.0;
        let rep = analyze(&[rc], &sc, &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.pol_mismatch"));
    }

    #[test]
    fn co_band_unordered_pairs() {
        let sc = manual_scene();
        let mut t1 = Link::typical(LinkKind::Telemetry);
        t1.name = "Tel1".into();
        let mut t2 = Link::typical(LinkKind::Telemetry);
        t2.name = "Tel2".into();
        t2.loc = AntennaLoc::Tail;
        t1.loc = AntennaLoc::Nose;
        let rep = analyze(&[t1, t2], &sc, &EmiConfig::default());
        let co: Vec<_> = rep
            .warnings
            .iter()
            .filter(|w| w.key == "rfemi.w.co_band")
            .collect();
        assert_eq!(co.len(), 1);

        let mut r1 = Link::typical(LinkKind::Rc);
        r1.loc = AntennaLoc::Nose;
        let mut r2 = Link::typical(LinkKind::Rc);
        r2.loc = AntennaLoc::Tail;
        let rep = analyze(&[r1, r2], &sc, &EmiConfig::default());
        assert_eq!(
            rep.warnings
                .iter()
                .filter(|w| w.key == "rfemi.w.co_band")
                .count(),
            1
        );

        let mut v1 = Link::typical(LinkKind::Video);
        v1.loc = AntennaLoc::Nose;
        let mut v2 = Link::typical(LinkKind::Video);
        v2.loc = AntennaLoc::Tail;
        let rep = analyze(&[v1, v2], &sc, &EmiConfig::default());
        assert_eq!(
            rep.warnings
                .iter()
                .filter(|w| w.key == "rfemi.w.co_band")
                .count(),
            1
        );
    }

    #[test]
    fn multirotor_scene_uses_plate_dims_and_nose_left() {
        let lib = Library::embedded();
        let mut p = Project::default();
        p.set_drone_type(DroneType::Multirotor);
        p.l_fuse = 2.0; // Sabit kanat değeri multirotoru bozmamalı.
        let m = evaluate(&p, &lib);
        let lay = Layout::new(&p, &m);
        let sc = Scene::new(&p, &lay);
        assert_eq!(sc.length_mm, 0.0);
        assert!((sc.radius_mm - lay.mr_plate.plate_radius_mm.max(20.0)).abs() < 1e-9);
        assert!(sc.point(AntennaLoc::Nose)[0] < sc.point(AntennaLoc::Tail)[0]);
        let (_, x1, _, _) = plan_bounds(&sc, &[]);
        assert!(x1 < 500.0, "{x1}");
    }

    #[test]
    fn fresnel_warning_triggers_when_ground_antenna_low() {
        let sc = manual_scene();
        let mut rc = Link::typical(LinkKind::Rc);
        rc.h_ground_m = 0.0;
        let rep = analyze(&[rc], &sc, &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.fresnel"));
    }

    fn default_links() -> Vec<Link> {
        LinkKind::ALL
            .iter()
            .map(|k| {
                let mut l = Link::typical(*k);
                l.name = t(k.key()).to_string();
                l
            })
            .collect()
    }

    #[test]
    fn all_vehicle_types_and_samples_have_finite_results() {
        let lib = Library::embedded();
        let mut projects: Vec<Project> = Vec::new();
        for dt in DroneType::ALL {
            let mut p = Project::default();
            p.set_drone_type(dt);
            projects.push(p);
        }
        for f in [
            "fixed_wing_2kg_1600mm.json",
            "flying_wing_1200mm.json",
            "hexa_550_x.json",
            "quad_250_x.json",
            "quad_275_x.json",
            "vtol_quadplane.json",
        ] {
            projects.push(crate::project::load_example(f).unwrap());
        }
        assert!(projects.len() >= 11);
        for p in projects {
            let m = evaluate(&p, &lib);
            let lay = Layout::new(&p, &m);
            let sc = Scene::new(&p, &lay);
            for (l, q) in &sc.anchors {
                assert!(q.iter().all(|v| v.is_finite()), "{:?} {l:?}", p.drone_type);
            }
            assert_eq!(sc.anchors.len(), AntennaLoc::ALL.len());
            assert!(
                sc.point(AntennaLoc::Nose)[0] < sc.point(AntennaLoc::Tail)[0],
                "{:?}",
                p.drone_type
            );
            let arm = sc.point(AntennaLoc::ArmEnd);
            for s in sc.sources.iter().filter(|s| s.kind == SourceKind::Motor) {
                assert!(dist(arm, s.a) > 1.0, "{:?}", p.drone_type);
            }
            let links = default_links();
            for carbon in [false, true] {
                let cfg = EmiConfig {
                    carbon_body: carbon,
                    ..EmiConfig::default()
                };
                let rep = analyze(&links, &sc, &cfg);
                for r in &rep.rows {
                    assert!(r.cm.is_finite() && r.cm >= 0.0);
                }
                for w in &rep.warnings {
                    assert!(
                        w.args
                            .iter()
                            .all(|a| !a.contains("NaN") && !a.to_ascii_lowercase().contains("inf")),
                        "{}",
                        w.key
                    );
                    assert_ne!(t(w.key), w.key, "çeviri eksik: {}", w.key);
                }
            }
            let (x0, x1, y0, y1) = plan_bounds(&sc, &[]);
            assert!([x0, x1, y0, y1].iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0);
        }
        for lang in [crate::i18n::Lang::Tr, crate::i18n::Lang::En] {
            for key in [
                "rfemi.w.gnss_close",
                "rfemi.w.ant_close",
                "rfemi.w.carbon_inside",
                "rfemi.w.carbon_belly",
                "rfemi.w.carbon_top_video",
                "rfemi.w.margin_neg",
                "rfemi.w.fresnel",
                "rfemi.w.pol_mismatch",
                "rfemi.w.harmonic",
                "rfemi.w.co_band",
                "rfemi.w.near_gnss",
            ] {
                assert_ne!(crate::i18n::t_lang(key, lang), key, "çeviri eksik: {key}");
            }
        }
    }
}
