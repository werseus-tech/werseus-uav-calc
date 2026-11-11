// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saf sayı/metin biçimlendirme yardımcıları (ondalık ayırıcı her zaman nokta).

/// The "missing value" placeholder.
pub const MISSING: &str = "—";

/// Decimals by magnitude of |x|: >= 100 -> 0, >= 10 -> 1, >= 1 -> 2, otherwise 3. Non-finite -> 0.
pub fn auto_decimals(x: f64) -> usize {
    if x.is_nan() || x.is_infinite() {
        return 0;
    }
    let ax = x.abs();
    if ax >= 100.0 {
        0
    } else if ax >= 10.0 {
        1
    } else if ax >= 1.0 {
        2
    } else {
        3
    }
}

/// Removes the minus sign from a formatted "negative zero" such as "-0.00".
fn strip_negative_zero(s: String) -> String {
    match s.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_string(),
        _ => s,
    }
}

/// Format a number with the given number of decimal places.
/// Non-finite values produce MISSING; negative zero is printed without the sign.
pub fn fmt_num(x: f64, dec: usize) -> String {
    if !x.is_finite() {
        return MISSING.to_string();
    }
    strip_negative_zero(format!("{:.*}", dec, x))
}

/// Like `fmt_num` but the integer part is grouped in thousands with a normal space.
pub fn fmt_grouped(x: f64, dec: usize) -> String {
    let s = fmt_num(x, dec);
    if s == MISSING {
        return s;
    }
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.as_str()),
    };
    let (int_part, frac_part) = match body.find('.') {
        Some(idx) => (&body[..idx], &body[idx..]),
        None => (body, ""),
    };
    let digits: Vec<char> = int_part.chars().collect();
    let mut out = String::with_capacity(s.len() + digits.len() / 3);
    if negative {
        out.push('-');
    }
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(*c);
    }
    out.push_str(frac_part);
    out
}

/// Mass given in kg: |kg| < 1.0 -> grams with 0 decimals and " g"; otherwise kg with 2 decimals and " kg".
pub fn fmt_mass_kg(kg: f64) -> String {
    if !kg.is_finite() {
        return MISSING.to_string();
    }
    if kg.abs() < 1.0 {
        format!("{} g", fmt_num(kg * 1000.0, 0))
    } else {
        format!("{} kg", fmt_num(kg, 2))
    }
}

/// Length given in metres: |m| >= 1.0 -> 2 decimals " m"; otherwise millimetres, 0 decimals " mm".
pub fn fmt_length_m(m: f64) -> String {
    if !m.is_finite() {
        return MISSING.to_string();
    }
    if m.abs() >= 1.0 {
        format!("{} m", fmt_num(m, 2))
    } else {
        format!("{} mm", fmt_num(m * 1000.0, 0))
    }
}

/// Duration given in minutes with caller-supplied unit labels.
/// Rounds minutes to nearest whole minute first.
pub fn fmt_duration_min(min: f64, h_label: &str, min_label: &str) -> String {
    if min.is_nan() || min.is_infinite() || min < 0.0 {
        return MISSING.to_string();
    }
    if min < 1.0 {
        return format!("<1 {}", min_label);
    }
    let whole_min = min.round();
    let hours = (whole_min / 60.0).floor() as i64;
    let mins = (whole_min % 60.0).round() as i64;
    if hours == 0 {
        format!("{} {}", mins, min_label)
    } else {
        format!("{} {} {} {}", hours, h_label, mins, min_label)
    }
}

/// Fraction to percent with given decimals. Non-finite -> MISSING.
pub fn fmt_percent(frac: f64, dec: usize) -> String {
    if !frac.is_finite() {
        return MISSING.to_string();
    }
    format!("{}%", fmt_num(frac * 100.0, dec))
}

/// Append unit to a formatted value. Empty unit -> value unchanged; empty or MISSING value -> MISSING.
pub fn with_unit(value: &str, unit: &str) -> String {
    if value.is_empty() || value == MISSING {
        return MISSING.to_string();
    }
    if unit.is_empty() {
        return value.to_string();
    }
    format!("{} {}", value, unit)
}

/// Truncate string to `max_chars` Unicode scalar values, appending "…" if longer.
/// `max_chars == 0` -> empty string.
pub fn ellipsize(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        return s.to_string();
    }
    let mut result: String = chars[..max_chars - 1].iter().collect();
    result.push('…');
    result
}

/// WCAG 2.x contrast ratio between two sRGB colours (1.0 ..= 21.0).
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    fn rel_luminance(rgb: [u8; 3]) -> f64 {
        let channels: [f64; 3] = [
            rgb[0] as f64 / 255.0,
            rgb[1] as f64 / 255.0,
            rgb[2] as f64 / 255.0,
        ];
        let linear: [f64; 3] = channels.map(|c| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    }

    let la = rel_luminance(a);
    let lb = rel_luminance(b);
    let (l1, l2) = if la >= lb { (la, lb) } else { (lb, la) };
    (l1 + 0.05) / (l2 + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_decimals() {
        assert_eq!(auto_decimals(123.456), 0);
        assert_eq!(auto_decimals(15.0), 1);
        assert_eq!(auto_decimals(5.0), 2);
        assert_eq!(auto_decimals(0.5), 3);
        assert_eq!(auto_decimals(f64::NAN), 0);
        assert_eq!(auto_decimals(f64::INFINITY), 0);
        assert_eq!(auto_decimals(100.0), 0);
        assert_eq!(auto_decimals(99.99), 1);
    }

    #[test]
    fn test_fmt_num_basic() {
        assert_eq!(fmt_num(1234567.891, 2), "1234567.89");
        assert_eq!(fmt_num(1.25, 2), "1.25");
        assert_eq!(fmt_num(0.123, 1), "0.1");
        assert_eq!(fmt_num(0.0, 0), "0");
    }

    #[test]
    fn test_fmt_num_non_finite() {
        assert_eq!(fmt_num(f64::NAN, 2), MISSING);
        assert_eq!(fmt_num(f64::INFINITY, 2), MISSING);
        assert_eq!(fmt_num(f64::NEG_INFINITY, 2), MISSING);
    }

    #[test]
    fn test_fmt_num_negative_zero() {
        assert_eq!(fmt_num(-0.0001, 2), "0.00");
        assert_eq!(fmt_num(-0.0, 0), "0");
    }

    #[test]
    fn test_fmt_grouped_examples() {
        assert_eq!(fmt_grouped(1234567.891, 2), "1 234 567.89");
        assert_eq!(fmt_grouped(-12345.0, 0), "-12 345");
        assert_eq!(fmt_grouped(999.0, 0), "999");
    }

    #[test]
    fn test_fmt_grouped_small() {
        assert_eq!(fmt_grouped(0.5, 1), "0.5");
    }

    #[test]
    fn test_fmt_mass_kg_examples() {
        assert_eq!(fmt_mass_kg(0.912), "912 g");
        assert_eq!(fmt_mass_kg(1.25), "1.25 kg");
        assert_eq!(fmt_mass_kg(f64::INFINITY), MISSING);
        assert_eq!(fmt_mass_kg(f64::NAN), MISSING);
        assert_eq!(fmt_mass_kg(0.9996), "1000 g");
    }

    #[test]
    fn test_fmt_length_m_examples() {
        assert_eq!(fmt_length_m(1.4), "1.40 m");
        assert_eq!(fmt_length_m(0.85), "850 mm");
        assert_eq!(fmt_length_m(0.0), "0 mm");
        assert_eq!(fmt_length_m(f64::NAN), MISSING);
    }

    #[test]
    fn test_fmt_duration_min_examples() {
        assert_eq!(fmt_duration_min(75.4, "h", "min"), "1 h 15 min");
        assert_eq!(fmt_duration_min(42.0, "h", "min"), "42 min");
        assert_eq!(fmt_duration_min(120.0, "h", "min"), "2 h 0 min");
        assert_eq!(fmt_duration_min(0.5, "h", "min"), "<1 min");
        assert_eq!(fmt_duration_min(-5.0, "h", "min"), MISSING);
        assert_eq!(fmt_duration_min(f64::NAN, "h", "min"), MISSING);
    }

    #[test]
    fn test_fmt_percent_examples() {
        assert_eq!(fmt_percent(0.123, 1), "12.3%");
        assert_eq!(fmt_percent(f64::NAN, 2), MISSING);
        assert_eq!(fmt_percent(1.0, 0), "100%");
    }

    #[test]
    fn test_with_unit() {
        assert_eq!(with_unit("12.3", "m/s"), "12.3 m/s");
        assert_eq!(with_unit("12.3", ""), "12.3");
        assert_eq!(with_unit(MISSING, "m/s"), MISSING);
        assert_eq!(with_unit("", "m/s"), MISSING);
    }

    #[test]
    fn test_ellipsize() {
        assert_eq!(ellipsize("çğışöü", 4), "çğı…");
        assert_eq!(ellipsize("abc", 5), "abc");
        assert_eq!(ellipsize("abc", 0), "");
        assert_eq!(ellipsize("abc", 3), "abc");
        assert_eq!(ellipsize("abcdef", 4), "abc…");
    }

    #[test]
    fn test_contrast_ratio_black_white() {
        let black: [u8; 3] = [0, 0, 0];
        let white: [u8; 3] = [255, 255, 255];
        let ratio = contrast_ratio(black, white);
        assert!((ratio - 21.0).abs() < 0.01, "expected ~21.0, got {}", ratio);
    }

    #[test]
    fn test_contrast_ratio_white_white() {
        let white: [u8; 3] = [255, 255, 255];
        let ratio = contrast_ratio(white, white);
        assert!((ratio - 1.0).abs() < 0.001, "expected 1.0, got {}", ratio);
    }

    #[test]
    fn test_contrast_ratio_symmetric() {
        let a: [u8; 3] = [128, 64, 32];
        let b: [u8; 3] = [255, 255, 0];
        let r1 = contrast_ratio(a, b);
        let r2 = contrast_ratio(b, a);
        assert!((r1 - r2).abs() < 1e-12, "not symmetric: {} vs {}", r1, r2);
    }
}
