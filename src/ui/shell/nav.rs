// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adım gezinmesinin tek kaynağı.
//!
//! Durum yeni bir yerde tutulmaz: sihirbaz modunda `Project::wizard_step` / `step_states`,
//! uzman (serbest) modda `App::tab` gerçek kaynaktır. Bu modül yalnızca bu durumu okuyup
//! sol paneldeki listeyi üretir ve seçim/İleri/Geri isteklerini mevcut kurallarla
//! (`wizard::validate_step`, adım kilidi, uzman kilidi) uygular. Böylece eski sihirbaz başlığı
//! ile yeni kabuk aynı kuralları paylaşır.
use crate::calc::Model;
use crate::project::{DroneType, Project, WizardStepState};
use crate::ui::Tab;
use crate::ui::components::StepVisual;
use crate::ui::wizard::{self, StepId};

/// Sol listede bir öğenin hedefi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavTarget {
    Step(usize),
    Tab(Tab),
}

#[derive(Clone, Debug)]
pub struct NavEntry {
    pub target: NavTarget,
    pub number: usize,
    pub title: &'static str,
    pub visual: StepVisual,
    pub selected: bool,
    pub clickable: bool,
}

/// Liste gösterimi için "Adım 7: " / "Step 7: " önekini atar (metin i18n'de olduğu gibi kalır;
/// başlıkta ve öğretmen penceresinde tam hâli kullanılır). Önek yoksa dizgi aynen döner.
pub fn strip_step_prefix(title: &'static str) -> &'static str {
    for word in ["Adım", "Step"] {
        if let Some(rest) = title.strip_prefix(word) {
            let rest = rest.trim_start();
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 {
                if let Some(after) = rest[digits..].strip_prefix(':') {
                    return after.trim_start();
                }
            }
        }
    }
    title
}

/// Sihirbaz adımları için sol liste.
pub fn wizard_entries(p: &Project, m: &Model) -> Vec<NavEntry> {
    let steps = wizard::steps_for(p.drone_type);
    let total = steps.len();
    if total == 0 {
        return Vec::new();
    }
    let max_idx = wizard::max_accessible_step_idx(p, m);
    let cur = p.wizard_step.min(max_idx).min(total - 1);
    steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let state = p
                .step_states
                .get(i)
                .copied()
                .unwrap_or(WizardStepState::NotVisited);
            let locked = i > max_idx;
            let visual = if locked {
                StepVisual::Locked
            } else if i == cur {
                StepVisual::Current
            } else {
                match state {
                    WizardStepState::Completed => StepVisual::Done,
                    WizardStepState::NeedsUpdate => StepVisual::NeedsUpdate,
                    WizardStepState::NotVisited => StepVisual::Available,
                }
            };
            // Eski adım çipleriyle aynı kural: ziyaret edilmiş ya da bir sonraki adım.
            let clickable =
                !locked && (i == cur || state != WizardStepState::NotVisited || i <= cur + 1);
            NavEntry {
                target: NavTarget::Step(i),
                number: i + 1,
                title: strip_step_prefix(s.title()),
                visual,
                selected: i == cur,
                clickable,
            }
        })
        .collect()
}

/// Uzman modunda sekmeler için sol liste.
pub fn tab_entries(dt: DroneType, tab: Tab) -> Vec<NavEntry> {
    Tab::tabs_for(dt)
        .into_iter()
        .enumerate()
        .map(|(i, t)| NavEntry {
            target: NavTarget::Tab(t),
            number: i + 1,
            title: t.label(),
            visual: if t == tab {
                StepVisual::Current
            } else {
                StepVisual::Available
            },
            selected: t == tab,
            clickable: true,
        })
        .collect()
}

pub fn entries(p: &Project, m: &Model, tab: Tab) -> Vec<NavEntry> {
    if p.expert_mode {
        tab_entries(p.drone_type, tab)
    } else {
        wizard_entries(p, m)
    }
}

/// Listeden seçim. Kilitli ya da tıklanamaz öğeler yok sayılır. Değişiklik olduysa `true`.
pub fn select(p: &mut Project, tab: &mut Tab, m: &Model, target: NavTarget) -> bool {
    match target {
        NavTarget::Tab(t) => {
            if !p.expert_mode || !Tab::tabs_for(p.drone_type).contains(&t) {
                return false;
            }
            let changed = *tab != t;
            *tab = t;
            changed
        }
        NavTarget::Step(i) => {
            if p.expert_mode {
                return false;
            }
            let ok = wizard_entries(p, m)
                .get(i)
                .map(|e| e.clickable)
                .unwrap_or(false);
            if ok && p.wizard_step != i {
                p.wizard_step = i;
                true
            } else {
                false
            }
        }
    }
}

/// Geçerli sihirbaz adımı (indeks sınırlanmış).
pub fn current_index(p: &Project) -> usize {
    let n = wizard::steps_for(p.drone_type).len();
    p.wizard_step.min(n.saturating_sub(1))
}

pub fn current_step(p: &Project) -> StepId {
    wizard::steps_for(p.drone_type)
        .get(current_index(p))
        .copied()
        .unwrap_or(StepId::Summary)
}

/// "İleri" kullanılabilir mi? (eski başlıktaki kuralın aynısı)
pub fn can_go_next(p: &Project, m: &Model) -> bool {
    let steps = wizard::steps_for(p.drone_type);
    let total = steps.len();
    if total == 0 {
        return false;
    }
    let cur = current_index(p);
    let max_idx = wizard::max_accessible_step_idx(p, m);
    wizard::validate_step(steps[cur], p, m).is_ok() && cur < max_idx && cur + 1 < total
}

/// İleri: mevcut adımı tamamlandı işaretler ve sonrakine geçer.
pub fn go_next(p: &mut Project, m: &Model) -> bool {
    if !can_go_next(p, m) {
        return false;
    }
    let cur = current_index(p);
    if p.step_states.len() <= cur {
        p.step_states.resize(cur + 1, WizardStepState::NotVisited);
    }
    p.step_states[cur] = WizardStepState::Completed;
    p.wizard_step = cur + 1;
    true
}

pub fn go_back(p: &mut Project) -> bool {
    let cur = current_index(p);
    if cur == 0 {
        return false;
    }
    p.wizard_step = cur - 1;
    true
}

/// Uzman sekmesinin sihirbaz adımı karşılığı ve tersi (uyarıdan adıma gitmek için).
pub fn tab_for_step(step: StepId, dt: DroneType) -> Option<Tab> {
    Some(match step {
        StepId::Mission => Tab::Mission,
        StepId::Weight => {
            if dt == DroneType::Multirotor {
                Tab::Multirotor
            } else {
                Tab::Weight
            }
        }
        StepId::Geometry => Tab::Geometry,
        StepId::Aero => Tab::Aero,
        StepId::DesignPoint => Tab::DesignPoint,
        StepId::Endurance => Tab::Endurance,
        StepId::Propulsion => Tab::Propulsion,
        StepId::Stability => Tab::Stability,
        StepId::Structure | StepId::MrStructure => Tab::Structure,
        StepId::Summary | StepId::Outputs => Tab::Summary,
        StepId::MrFrame | StepId::MrPropulsion | StepId::MrBatteryLoiter => Tab::Multirotor,
        StepId::MrCG => {
            if dt == DroneType::Multirotor {
                Tab::Stability
            } else {
                Tab::Multirotor
            }
        }
    })
}

/// Bir uyarının ait olduğu adım (alan anahtarı + ileti anahtarından).
pub fn step_for_warning(area_key: &str, msg_key: &str, dt: DroneType) -> Option<StepId> {
    let mr = dt == DroneType::Multirotor;
    let s = match area_key {
        "area.mission" => StepId::Mission,
        "area.weight" => StepId::Weight,
        "area.geometry" => StepId::Geometry,
        "area.aero" => StepId::Aero,
        "area.design_point" => StepId::DesignPoint,
        "area.range" => {
            if mr {
                StepId::MrBatteryLoiter
            } else {
                StepId::Endurance
            }
        }
        "area.propulsion" => StepId::Propulsion,
        "area.stability" => {
            if mr {
                StepId::MrCG
            } else {
                StepId::Stability
            }
        }
        "area.structure" => StepId::Structure,
        "area.mr_structure" => StepId::MrStructure,
        "area.multirotor" => {
            if msg_key.contains("overlap") || msg_key.contains("clearance") {
                StepId::MrFrame
            } else if msg_key.contains("current") {
                StepId::MrBatteryLoiter
            } else if msg_key.contains("cg_offset") {
                StepId::MrCG
            } else {
                StepId::MrPropulsion
            }
        }
        _ => return None,
    };
    // Adım bu türde yoksa (ör. sabit kanatta MrCG) eşleme yok.
    if wizard::steps_for(dt).contains(&s) {
        Some(s)
    } else {
        None
    }
}

/// Belirli bir adıma git (uyarıdan ya da "Neden?" bağlantısından). Sihirbazda kilitli
/// adıma gidilemez; o zaman erişilebilir son adıma gidilir. Değişiklik olduysa `true`.
pub fn go_to_step(p: &mut Project, tab: &mut Tab, m: &Model, step: StepId) -> bool {
    if p.expert_mode {
        if let Some(t) = tab_for_step(step, p.drone_type) {
            if Tab::tabs_for(p.drone_type).contains(&t) && *tab != t {
                *tab = t;
                return true;
            }
        }
        return false;
    }
    let steps = wizard::steps_for(p.drone_type);
    let Some(idx) = steps.iter().position(|s| *s == step) else {
        return false;
    };
    let target = idx.min(wizard::max_accessible_step_idx(p, m));
    if p.wizard_step != target {
        p.wizard_step = target;
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(dt: DroneType) -> (Project, Model) {
        let mut p = Project::default();
        p.set_drone_type(dt);
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        wizard::sync_wizard_steps(&mut p, &m);
        (p, m)
    }

    #[test]
    fn list_titles_have_no_step_prefix() {
        assert_eq!(
            strip_step_prefix("Adım 7: Yapı & Kol Boyutlandırma"),
            "Yapı & Kol Boyutlandırma"
        );
        assert_eq!(strip_step_prefix("Step 7: Structure"), "Structure");
        assert_eq!(strip_step_prefix("Görev & Atmosfer"), "Görev & Atmosfer");
        assert_eq!(strip_step_prefix("Adım sayısı"), "Adım sayısı");
        for dt in DroneType::ALL {
            let (p, m) = setup(dt);
            for e in wizard_entries(&p, &m) {
                assert!(
                    !e.title.starts_with("Adım") && !e.title.starts_with("Step"),
                    "{}",
                    e.title
                );
            }
        }
    }

    #[test]
    fn entries_follow_wizard_steps_for_every_type() {
        for dt in DroneType::ALL {
            let (p, m) = setup(dt);
            let e = wizard_entries(&p, &m);
            assert_eq!(e.len(), wizard::steps_for(dt).len());
            assert_eq!(e.iter().filter(|x| x.selected).count(), 1);
            assert_eq!(e[0].visual, StepVisual::Current);
            // Sıradaki adım tıklanabilir, iki sonrası (ziyaret edilmemiş) değil.
            assert!(e[1].clickable);
            assert!(!e[2].clickable);
        }
    }

    #[test]
    fn next_and_back_mirror_legacy_header() {
        let (mut p, m) = setup(DroneType::FixedWingConventional);
        assert!(go_next(&mut p, &m));
        assert_eq!(p.wizard_step, 1);
        assert_eq!(p.step_states[0], WizardStepState::Completed);
        let e = wizard_entries(&p, &m);
        assert_eq!(e[0].visual, StepVisual::Done);
        assert_eq!(e[1].visual, StepVisual::Current);
        assert!(go_back(&mut p));
        assert_eq!(p.wizard_step, 0);
        assert!(!go_back(&mut p));
    }

    #[test]
    fn invalid_step_locks_following_steps() {
        let mut p = Project::default();
        p.m_avionics = 0.0; // Ağırlık adımı (1) geçersiz
        let lib = crate::calc::Library::embedded();
        let m = crate::calc::evaluate(&p, &lib);
        wizard::sync_wizard_steps(&mut p, &m);
        p.wizard_step = 1;
        let e = wizard_entries(&p, &m);
        assert!(e[2..].iter().all(|x| x.visual == StepVisual::Locked));
        assert!(!can_go_next(&p, &m));
        assert!(!go_next(&mut p, &m));
        let mut tab = Tab::Summary;
        assert!(!select(&mut p, &mut tab, &m, NavTarget::Step(5)));
        assert_eq!(p.wizard_step, 1);
        // Kilitli adıma "git" isteği erişilebilir son adımda kalır.
        go_to_step(&mut p, &mut tab, &m, StepId::Structure);
        assert_eq!(p.wizard_step, 1);
    }

    #[test]
    fn select_respects_mode() {
        let (mut p, m) = setup(DroneType::FixedWingConventional);
        let mut tab = Tab::Summary;
        assert!(select(&mut p, &mut tab, &m, NavTarget::Step(1)));
        assert!(!select(&mut p, &mut tab, &m, NavTarget::Tab(Tab::Aero)));
        p.expert_mode = true;
        assert!(select(&mut p, &mut tab, &m, NavTarget::Tab(Tab::Aero)));
        assert_eq!(tab, Tab::Aero);
        // Bu türde olmayan sekme seçilemez.
        assert!(!select(
            &mut p,
            &mut tab,
            &m,
            NavTarget::Tab(Tab::Multirotor)
        ));
        let e = entries(&p, &m, tab);
        assert_eq!(e.len(), Tab::tabs_for(p.drone_type).len());
        assert!(
            e.iter()
                .any(|x| x.selected && x.target == NavTarget::Tab(Tab::Aero))
        );
    }

    #[test]
    fn warnings_map_to_existing_steps() {
        let fw = DroneType::FixedWingConventional;
        let mr = DroneType::Multirotor;
        assert_eq!(
            step_for_warning("area.geometry", "x", fw),
            Some(StepId::Geometry)
        );
        assert_eq!(
            step_for_warning("area.stability", "x", mr),
            Some(StepId::MrCG)
        );
        assert_eq!(
            step_for_warning("area.multirotor", "warn.mr.prop_overlap", mr),
            Some(StepId::MrFrame)
        );
        assert_eq!(
            step_for_warning("area.multirotor", "warn.mr.hover_current_high", mr),
            Some(StepId::MrBatteryLoiter)
        );
        assert_eq!(step_for_warning("area.general", "x", fw), None);
        // Sabit kanatta multirotor adımı yok.
        assert_eq!(
            step_for_warning("area.multirotor", "warn.mr.tw_low", fw),
            None
        );
        // Gerçek modeldeki tüm uyarıların alanı tanınır (genel hariç).
        for dt in DroneType::ALL {
            let (p, m) = setup(dt);
            for w in &m.warnings {
                if w.area_key != "area.general" {
                    let _ = step_for_warning(w.area_key, w.msg_key, p.drone_type);
                }
            }
        }
    }

    #[test]
    fn go_to_step_in_expert_mode_switches_tab() {
        let (mut p, m) = setup(DroneType::FixedWingConventional);
        p.expert_mode = true;
        let mut tab = Tab::Summary;
        assert!(go_to_step(&mut p, &mut tab, &m, StepId::Stability));
        assert_eq!(tab, Tab::Stability);
        for s in [
            StepId::Mission,
            StepId::Weight,
            StepId::Geometry,
            StepId::Outputs,
        ] {
            assert!(tab_for_step(s, p.drone_type).is_some());
        }
    }
}
