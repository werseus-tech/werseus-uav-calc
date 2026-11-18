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
        let (center, mut anchors);
        if multirotor {
            let r_plate = lay.mr_plate.plate_radius_mm.max(20.0);
            let reach = motors
                .iter()
                .map(|m| (m[0] * m[0] + m[1] * m[1]).sqrt())
                .fold(r_plate, f64::max);
            if motors.is_empty() {
                motors.push([reach, 0.0, 0.0]);
            }
            center = [0.0, 0.0, 5.0];
            anchors = vec![
                (AntennaLoc::Nose, [r_plate, 0.0, 0.0]),
                (AntennaLoc::Tail, [-r_plate, 0.0, 0.0]),
                (AntennaLoc::WingtipL, [0.0, reach, 0.0]),
                (AntennaLoc::WingtipR, [0.0, -reach, 0.0]),
                (AntennaLoc::FuselageTop, [0.0, 0.0, 40.0]),
                (AntennaLoc::Belly, [0.0, 0.0, -15.0]),
                (AntennaLoc::ArmEnd, motors[0]),
                (AntennaLoc::Inside, center),
            ];
        } else {
            if motors.is_empty() {
                motors.push([0.0, 0.0, 0.0]);
            }
            let wg = lay.wing.geom();
            let span = wg.span.max(1.0);
            let dih = lay.wing.dihedral_deg.to_radians();
            let x_tip = p.x_root_le * 1000.0 + wg.le_x_at(span) + 0.25 * wg.chord_at(span);
            let (y_tip, z_tip) = (span * dih.cos(), span * dih.sin());
            let x_mid = 0.45 * length;
            center = [0.4 * length, 0.0, 0.0];
            let lift = lay
                .instances
                .iter()
                .find(|i| i.id == PartId::LiftMotorMount)
                .map(|i| i.frame.t);
            anchors = vec![
                (AntennaLoc::Nose, [0.0, 0.0, 0.0]),
                (AntennaLoc::Tail, [length, 0.0, 0.0]),
                (AntennaLoc::WingtipL, [x_tip, y_tip, z_tip]),
                (AntennaLoc::WingtipR, [x_tip, -y_tip, z_tip]),
                (AntennaLoc::FuselageTop, [x_mid, 0.0, radius]),
                (AntennaLoc::Belly, [x_mid, 0.0, -radius]),
                (AntennaLoc::ArmEnd, lift.unwrap_or([length, 0.0, 0.0])),
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
            length_mm: length,
            radius_mm: radius,
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

/// Bir TX'in n. harmoniği alıcı bandına düşüyor mu?
pub fn harmonic_hits(f_tx_mhz: f64, n: u32, f_rx_mhz: f64, bw_rx_mhz: f64) -> bool {
    (f_tx_mhz * n as f64 - f_rx_mhz).abs() <= bw_rx_mhz / 2.0
}

fn f0(x: f64) -> String {
    format!("{x:.0}")
}
fn f1(x: f64) -> String {
    format!("{x:.1}")
}

pub fn analyze(links: &[Link], scene: &Scene, cfg: &EmiConfig) -> Report {
    let mut rep = Report::default();
    let sources = scene.sources_with_vtx(cfg.vtx_loc);

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
                AntennaLoc::Belly if l.kind != LinkKind::Video => rep.warnings.push(Warning {
                    severity: if gnss {
                        Severity::Danger
                    } else {
                        Severity::Warn
                    },
                    key: "rfemi.w.carbon_belly",
                    args: vec![l.name.clone(), loc],
                }),
                AntennaLoc::FuselageTop if l.kind == LinkKind::Video => {
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

    // Harmonikler ve aynı banttaki kaynak/alıcı çiftleri.
    for (i, tx) in links.iter().enumerate() {
        if !tx.kind.onboard_tx() {
            continue;
        }
        for (j, rx) in links.iter().enumerate() {
            if i == j || !rx.kind.onboard_rx() {
                continue;
            }
            for n in 2..=3u32 {
                if harmonic_hits(tx.f_mhz, n, rx.f_mhz, rx.bw_mhz) {
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
            if (tx.f_mhz - rx.f_mhz).abs() <= (tx.bw_mhz + rx.bw_mhz) / 2.0 {
                rep.warnings.push(Warning {
                    severity: Severity::Warn,
                    key: "rfemi.w.co_band",
                    args: vec![tx.name.clone(), rx.name.clone()],
                });
            }
            if rx.kind == LinkKind::Gnss && (1100.0..=1400.0).contains(&tx.f_mhz) {
                rep.warnings.push(Warning {
                    severity: Severity::Warn,
                    key: "rfemi.w.near_gnss",
                    args: vec![tx.name.clone(), f0(tx.f_mhz)],
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
        assert!(harmonic_hits(433.0, 2, 868.0, 7.0));
        assert!(harmonic_hits(433.92, 2, 868.0, 7.0));
        assert!(!harmonic_hits(433.0, 3, 868.0, 7.0));
        // 1.3 GHz ikinci harmonik 2.6 GHz, 2.4 GHz bandının dışında; 1.25 GHz içinde.
        assert!(!harmonic_hits(1300.0, 2, 2440.0, 80.0));
        assert!(harmonic_hits(1220.0, 2, 2440.0, 80.0));
        // 5.8 GHz harmoniği hiçbir bağın bandına düşmez.
        for b in Band::ALL {
            let i = b.info();
            assert!(!harmonic_hits(5800.0, 2, i.f_mhz, i.bw_mhz), "{}", i.label);
        }
    }

    #[test]
    fn harmonic_warning_between_links() {
        let mut tel = Link::typical(LinkKind::Telemetry);
        tel.f_mhz = 433.0;
        let rc = Link::typical(LinkKind::Rc); // 868 MHz
        let rep = analyze(&[tel, rc], &manual_scene(), &EmiConfig::default());
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
        let mut cfg = EmiConfig {
            vtx_loc: AntennaLoc::Belly, // (250, 0, -50): 10 cm
            ..EmiConfig::default()
        };
        let has = |cfg: &EmiConfig| {
            analyze(std::slice::from_ref(&gnss), &sc, cfg)
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
        gnss.loc = AntennaLoc::Inside;
        let rep = analyze(std::slice::from_ref(&gnss), &sc, &cfg);
        assert!(
            rep.warnings
                .iter()
                .any(|w| w.key == "rfemi.w.carbon_inside")
        );
        let mut rc = Link::typical(LinkKind::Rc);
        rc.pol_angle_deg = 70.0;
        let rep = analyze(&[rc], &sc, &EmiConfig::default());
        assert!(rep.warnings.iter().any(|w| w.key == "rfemi.w.pol_mismatch"));
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
                    assert!(w.args.iter().all(|a| !a.contains("NaN")), "{}", w.key);
                    assert_ne!(t(w.key), w.key, "çeviri eksik: {}", w.key);
                }
            }
            let (x0, x1, y0, y1) = plan_bounds(&sc, &[]);
            assert!([x0, x1, y0, y1].iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0);
        }
    }
}
