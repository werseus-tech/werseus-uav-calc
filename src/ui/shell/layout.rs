// SPDX-License-Identifier: AGPL-3.0-or-later
//! Üç panelli kabuğun saf düzen hesabı: hangi panel görünür, en çok ne kadar genişleyebilir,
//! dar pencerede neyin önce çekileceği. egui'ye bağımlı değildir; birim testlidir.
//!
//! Öncelik: merkez görünüm (en az `CENTER_MIN`) > sol gezinme > sağ analiz paneli.
//! Yer yetmezse önce sağ panel otomatik gizlenir, sonra sol panel ince adım şeridine (rail) iner.
//! Kullanıcının aç/kapa tercihi değişmez; pencere genişleyince paneller geri gelir.

pub const CENTER_MIN: f32 = 360.0;
pub const LEFT_MIN: f32 = 232.0;
pub const LEFT_MAX: f32 = 520.0;
pub const LEFT_DEFAULT: f32 = 300.0;
pub const LEFT_RAIL: f32 = 52.0;
pub const RIGHT_MIN: f32 = 300.0;
pub const RIGHT_MAX: f32 = 520.0;
pub const RIGHT_DEFAULT: f32 = 360.0;

/// Merkezde 3D görünümün yükseklik oranı sınırları.
pub const VIEWPORT_FRAC_MIN: f32 = 0.22;
pub const VIEWPORT_FRAC_MAX: f32 = 0.85;
pub const VIEWPORT_FRAC_DEFAULT: f32 = 0.48;

/// Kullanıcının panel tercihleri (oturum içinde tutulur).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellPrefs {
    pub left_open: bool,
    pub right_open: bool,
    pub viewport_open: bool,
    /// Merkez yüksekliğinin görünüme ayrılan oranı.
    pub viewport_frac: f32,
}

impl Default for ShellPrefs {
    fn default() -> Self {
        Self {
            left_open: true,
            right_open: true,
            viewport_open: true,
            viewport_frac: VIEWPORT_FRAC_DEFAULT,
        }
    }
}

/// Bu kare için düzen kararı.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    pub show_left: bool,
    /// Sol panel yalnız numaralı adım şeridi olarak mı gösterilir?
    pub left_rail: bool,
    pub left_max: f32,
    pub show_right: bool,
    pub right_max: f32,
    /// Sağ panel yer olmadığı için mi gizlendi (kullanıcı kapatmadı)?
    pub right_auto_hidden: bool,
}

pub fn compute(total_width: f32, prefs: &ShellPrefs) -> ShellLayout {
    let w = if total_width.is_finite() {
        total_width.max(0.0)
    } else {
        0.0
    };

    // Sol panel: tam ya da rail.
    let mut show_left = prefs.left_open;
    let mut left_rail = false;
    if show_left && w < LEFT_MIN + CENTER_MIN {
        left_rail = true;
        if w < LEFT_RAIL + CENTER_MIN * 0.5 {
            show_left = false;
            left_rail = false;
        }
    }
    let left_used_min = if !show_left {
        0.0
    } else if left_rail {
        LEFT_RAIL
    } else {
        LEFT_MIN
    };

    // Sağ panel: yer varsa.
    let mut show_right = prefs.right_open;
    let mut right_auto_hidden = false;
    if show_right && w < left_used_min + RIGHT_MIN + CENTER_MIN {
        show_right = false;
        right_auto_hidden = true;
    }
    let right_used_min = if show_right { RIGHT_MIN } else { 0.0 };

    let left_max = if left_rail {
        LEFT_RAIL
    } else {
        (w - CENTER_MIN - right_used_min).clamp(LEFT_MIN, LEFT_MAX)
    };
    let right_max = (w - CENTER_MIN - left_used_min).clamp(RIGHT_MIN, RIGHT_MAX);

    ShellLayout {
        show_left,
        left_rail,
        left_max,
        show_right,
        right_max,
        right_auto_hidden,
    }
}

/// Merkezdeki görünüm yüksekliği: oran sınırlanır, alt çalışma alanına en az `min_below` kalır.
pub fn viewport_height(center_height: f32, frac: f32, min_view: f32, min_below: f32) -> f32 {
    if !center_height.is_finite() || center_height <= 0.0 {
        return 0.0;
    }
    let frac = if frac.is_finite() {
        frac.clamp(VIEWPORT_FRAC_MIN, VIEWPORT_FRAC_MAX)
    } else {
        VIEWPORT_FRAC_DEFAULT
    };
    let max = (center_height - min_below).max(0.0);
    (center_height * frac).clamp(min_view.min(max), max)
}

/// Çıktılar adımında görünüm payı: altındaki çalışma alanı (düğmeler, ilerleme, dosya listesi)
/// asıl içeriktir; görünüm toplamın en çok ~%40'ını alır, çalışma alanına en az 220 px kalır.
/// Dönen üçlü: (varsayılan, en az, en çok) görünüm yüksekliği.
pub fn outputs_viewport_limits(center_height: f32) -> (f32, f32, f32) {
    if !center_height.is_finite() || center_height <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let max = (center_height * 0.40)
        .min((center_height - 220.0).max(0.0))
        .max(0.0);
    let min = 120.0_f32.min(max);
    let def = (center_height * 0.30).clamp(min, max);
    (def, min, max)
}

/// Üst çubuk dar mı? (dar ise düğmeler kısa etiketle gösterilir)
pub fn topbar_compact(width: f32) -> bool {
    !width.is_finite() || width < 1240.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs() -> ShellPrefs {
        ShellPrefs::default()
    }

    #[test]
    fn outputs_viewport_leaves_room_for_the_workspace() {
        for h in [200.0_f32, 400.0, 560.0, 700.0, 900.0, 1400.0] {
            let (def, min, max) = outputs_viewport_limits(h);
            assert!(min <= def && def <= max, "{h}: {min} {def} {max}");
            assert!(
                h - max >= 220.0_f32.min(h),
                "{h}: çalışma alanı {}",
                h - max
            );
            assert!(max <= h * 0.40 + 0.01);
        }
        assert_eq!(outputs_viewport_limits(f32::NAN), (0.0, 0.0, 0.0));
        assert_eq!(outputs_viewport_limits(-5.0), (0.0, 0.0, 0.0));
    }

    #[test]
    fn wide_window_shows_all_panels() {
        let l = compute(1600.0, &prefs());
        assert!(l.show_left && !l.left_rail && l.show_right && !l.right_auto_hidden);
        assert!(l.left_max <= LEFT_MAX && l.left_max >= LEFT_MIN);
        assert!(l.right_max <= RIGHT_MAX && l.right_max >= RIGHT_MIN);
        // Merkez her durumda en az CENTER_MIN alır.
        assert!(1600.0 - l.left_max - RIGHT_MIN >= CENTER_MIN - 1e-3);
        assert!(1600.0 - l.right_max - LEFT_MIN >= CENTER_MIN - 1e-3);
    }

    #[test]
    fn minimum_window_keeps_center_and_both_panels() {
        // Uygulamanın en küçük penceresi 900 px: üç panel sığar (232+300+360 = 892).
        let l = compute(900.0, &prefs());
        assert!(l.show_left && l.show_right);
        assert!((l.left_max - LEFT_MIN).abs() < 21.0);
    }

    #[test]
    fn narrow_window_hides_right_first_then_rails_left() {
        let l = compute(800.0, &prefs());
        assert!(l.show_left && !l.left_rail);
        assert!(!l.show_right && l.right_auto_hidden);

        let l = compute(560.0, &prefs());
        assert!(l.show_left && l.left_rail);
        assert_eq!(l.left_max, LEFT_RAIL);
        assert!(!l.show_right);

        let l = compute(200.0, &prefs());
        assert!(!l.show_left && !l.show_right);
    }

    #[test]
    fn user_closed_panels_stay_closed() {
        let p = ShellPrefs {
            left_open: false,
            right_open: false,
            ..prefs()
        };
        let l = compute(1600.0, &p);
        assert!(!l.show_left && !l.show_right && !l.right_auto_hidden);
        // Sol kapalıyken sağ panel daha geniş olabilir.
        let p = ShellPrefs {
            left_open: false,
            ..prefs()
        };
        let l = compute(1000.0, &p);
        assert!(l.show_right);
        assert!((l.right_max - RIGHT_MAX).abs() < 1e-3);
    }

    #[test]
    fn bad_width_is_safe() {
        let l = compute(f32::NAN, &prefs());
        assert!(!l.show_left && !l.show_right);
        let l = compute(-50.0, &prefs());
        assert!(!l.show_left);
    }

    #[test]
    fn viewport_height_is_bounded() {
        let h = viewport_height(800.0, 0.5, 180.0, 160.0);
        assert!((h - 400.0).abs() < 1e-3);
        // Oran üst sınıra kırpılır ve alt alana yer kalır.
        let h = viewport_height(800.0, 2.0, 180.0, 160.0);
        assert!(h <= 640.0 + 1e-3);
        // Çok kısa merkez: negatif yok.
        let h = viewport_height(100.0, 0.5, 180.0, 160.0);
        assert!((0.0..=100.0).contains(&h));
        assert_eq!(viewport_height(f32::NAN, 0.5, 180.0, 160.0), 0.0);
        let h = viewport_height(800.0, f32::NAN, 180.0, 160.0);
        assert!((h - 800.0 * VIEWPORT_FRAC_DEFAULT).abs() < 1e-3);
    }

    #[test]
    fn topbar_compacts_when_narrow() {
        assert!(topbar_compact(1000.0));
        assert!(!topbar_compact(1600.0));
        assert!(topbar_compact(f32::NAN));
    }
}
