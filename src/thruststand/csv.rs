// SPDX-License-Identifier: AGPL-3.0-or-later
//! Itki tezgahi CSV okuyucusu: ayirac ve ondalik virgul otomatik bulunur, kotu satirlar atlanip
//! sayilir. Tyto / RCbenchmark (Series 1580/1585) basliklari otomatik eslenir; baska dosyalar icin
//! sutun eslemesi elle yapilir. Saf kod, UI yok.

/// Kuvvet birimi (itki sutunu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForceUnit {
    Gram,
    Kgf,
    Newton,
}

impl ForceUnit {
    pub const ALL: [ForceUnit; 3] = [ForceUnit::Gram, ForceUnit::Kgf, ForceUnit::Newton];
    pub fn label(self) -> &'static str {
        match self {
            ForceUnit::Gram => "g",
            ForceUnit::Kgf => "kgf",
            ForceUnit::Newton => "N",
        }
    }
    /// Birimdeki degeri Newton'a cevirir.
    pub fn to_newton(self, v: f64) -> f64 {
        const G0: f64 = 9.80665;
        match self {
            ForceUnit::Gram => v * G0 / 1000.0,
            ForceUnit::Kgf => v * G0,
            ForceUnit::Newton => v,
        }
    }
}

/// Gaz (ESC sinyali) birimi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrottleUnit {
    /// Mikrosaniye (1000-2000 us darbe genisligi).
    Micros,
    /// Yuzde (0-100).
    Percent,
}

impl ThrottleUnit {
    pub const ALL: [ThrottleUnit; 2] = [ThrottleUnit::Micros, ThrottleUnit::Percent];
    pub fn label(self) -> &'static str {
        match self {
            ThrottleUnit::Micros => "µs",
            ThrottleUnit::Percent => "%",
        }
    }
    /// 0..1 araligina cevirir (1000 us = 0, 2000 us = 1).
    pub fn to_fraction(self, v: f64) -> f64 {
        match self {
            ThrottleUnit::Micros => ((v - 1000.0) / 1000.0).clamp(0.0, 1.0),
            ThrottleUnit::Percent => (v / 100.0).clamp(0.0, 1.0),
        }
    }
}

/// Ham tablo: baslik + hucre metinleri.
#[derive(Clone, Debug, Default)]
pub struct RawTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub separator: char,
    pub decimal_comma: bool,
}

/// Hangi sutun neyi gosteriyor (sutun dizini).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mapping {
    pub thrust: Option<usize>,
    pub rpm: Option<usize>,
    pub volt: Option<usize>,
    pub curr: Option<usize>,
    pub throttle: Option<usize>,
}

/// Birimler ve devir duzeltmesi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Units {
    pub force: ForceUnit,
    pub throttle: ThrottleUnit,
    /// Devir sutunu elektriksel devir ise (Tyto "Electrical Speed") mekanik devir =
    /// elektriksel / kutup cifti sayisi.
    pub rpm_electrical: bool,
    pub pole_pairs: f64,
}

impl Default for Units {
    fn default() -> Self {
        Units {
            force: ForceUnit::Gram,
            throttle: ThrottleUnit::Micros,
            rpm_electrical: false,
            pole_pairs: 7.0,
        }
    }
}

/// Otomatik eslemenin sonucu.
#[derive(Clone, Debug, Default)]
pub struct AutoMap {
    pub mapping: Mapping,
    pub units: Units,
    /// Basliklar bilinen bir tezgah dokumu gibi gorunuyor (Tyto / RCbenchmark).
    pub recognized: bool,
}

/// Bir olcum satiri (SI: N, dev/dk, V, A, gaz 0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub thrust_n: f64,
    pub rpm: Option<f64>,
    pub volt: Option<f64>,
    pub curr: Option<f64>,
    pub throttle: Option<f64>,
}

impl Sample {
    /// Elektrik gucu P = V * I (W); ikisi de varsa.
    pub fn power_w(&self) -> Option<f64> {
        match (self.volt, self.curr) {
            (Some(v), Some(i)) => Some(v * i),
            _ => None,
        }
    }
}

/// Orneklerin cikarilma sonucu.
#[derive(Clone, Debug, Default)]
pub struct ParseReport {
    pub samples: Vec<Sample>,
    /// Atlanan (bozuk / eksik) satir sayisi.
    pub skipped: usize,
    pub total_rows: usize,
}

/// Bir satiri tirnak kurallarina gore boler.
fn split_line(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_q = true;
        } else if c == sep {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn detect_separator(lines: &[&str]) -> char {
    // Eşitlikte önce sekme, sonra noktalı virgül: virgül ondalık ayracı olabilir.
    let mut best = (',', 0usize);
    for &s in &['\t', ';', ','] {
        let counts: Vec<usize> = lines
            .iter()
            .take(8)
            .map(|l| split_line(l, s).len().saturating_sub(1))
            .collect();
        // En sık görülen sütun sayısı; başlık/başlık dışı bir satır ayracı elemez.
        let (mode, n_mode) = counts
            .iter()
            .filter(|&&c| c > 0)
            .map(|&c| (c, counts.iter().filter(|&&d| d == c).count()))
            .max_by_key(|&(c, n)| (n, c))
            .unwrap_or((0, 0));
        if mode == 0 {
            continue;
        }
        let score = mode * 10 + n_mode;
        if score > best.1 {
            best = (s, score);
        }
    }
    best.0
}

fn parse_plain(s: &str, decimal_comma: bool) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let v: f64 = if decimal_comma {
        t.replace(',', ".").parse().ok()?
    } else {
        t.parse().ok()?
    };
    v.is_finite().then_some(v)
}

/// Metni tabloya cevirir. Bos / cop girdide bos tablo doner (panik yok).
pub fn parse_table(text: &str) -> RawTable {
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return RawTable {
            separator: ',',
            ..Default::default()
        };
    }
    let separator = detect_separator(&lines);
    let cells: Vec<Vec<String>> = lines.iter().map(|l| split_line(l, separator)).collect();
    // Baslik: ilk satir (sayisal degerleri cogunlukta degilse).
    let is_numeric_row = |r: &Vec<String>| {
        let n = r.iter().filter(|c| !c.is_empty()).count();
        let num = r.iter().filter(|c| parse_plain(c, true).is_some()).count();
        n > 0 && num * 2 > n
    };
    let (headers, body_start) = if is_numeric_row(&cells[0]) {
        ((1..=cells[0].len()).map(|i| format!("#{i}")).collect(), 0)
    } else {
        (cells[0].clone(), 1)
    };
    let rows: Vec<Vec<String>> = cells[body_start..].to_vec();
    // Ondalik virgul: ayirac virgul degilse ve sayisal hucrelerde virgul + rakam varsa.
    let decimal_comma = separator != ','
        && rows.iter().take(50).any(|r| {
            r.iter().any(|c| {
                c.contains(',')
                    && !c.contains('.')
                    && c.chars()
                        .all(|ch| ch.is_ascii_digit() || ",-+eE".contains(ch))
            })
        });
    RawTable {
        headers,
        rows,
        separator,
        decimal_comma,
    }
}

/// Basliklardan sutun eslemesi ve birim tahmini yapar.
pub fn auto_map(headers: &[String]) -> AutoMap {
    let hs: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    let find = |pred: &dyn Fn(&str) -> bool| hs.iter().position(|h| pred(h));
    let thrust = find(&|h| h.contains("thrust") || h.contains("itki") || h.contains("force"));
    // Optik devir (mekanik) tercih edilir; yoksa elektriksel; yoksa herhangi bir rpm.
    let optical = find(&|h| h.contains("optical") && h.contains("rpm"));
    let electrical = find(&|h| h.contains("electrical") && h.contains("rpm"));
    let any_rpm = find(&|h| h.contains("rpm") || h.contains("devir"));
    let rpm = optical
        .or(any_rpm.filter(|&i| Some(i) != electrical))
        .or(electrical);
    let rpm_electrical = rpm.is_some() && rpm == electrical && optical.is_none();
    let volt = find(&|h| h.starts_with("voltage") || h.contains("volt") || h.contains("gerilim"));
    let curr = find(&|h| h.starts_with("current") || h.contains("akım") || h.contains("akim"))
        .or_else(|| find(&|h| h.contains("current")));
    let throttle = find(&|h| {
        h.contains("esc signal") || h.contains("throttle") || h.contains("pwm") || h.contains("gaz")
    });
    let mut units = Units::default();
    if let Some(i) = thrust {
        let h = &hs[i];
        units.force = if h.contains("kgf")
            || h.contains("(kg)")
            || h.contains("[kg]")
            || h.ends_with(" kg")
            || h.ends_with("_kg")
        {
            ForceUnit::Kgf
        } else if h.contains("(n)")
            || h.contains("[n]")
            || h.ends_with(" n")
            || h.ends_with("_n")
            || h.contains("newton")
        {
            ForceUnit::Newton
        } else {
            ForceUnit::Gram
        };
    }
    if let Some(i) = throttle {
        units.throttle = if hs[i].contains('%')
            || hs[i].contains("yuzde")
            || hs[i].contains("yüzde")
            || hs[i].contains("percent")
        {
            ThrottleUnit::Percent
        } else {
            ThrottleUnit::Micros
        };
    }
    units.rpm_electrical = rpm_electrical;
    let recognized = hs.iter().any(|h| h.contains("motor electrical speed"))
        || hs.iter().any(|h| h.contains("motor optical speed"))
        || (thrust.is_some() && hs.iter().any(|h| h.contains("esc signal")) && volt.is_some());
    AutoMap {
        mapping: Mapping {
            thrust,
            rpm,
            volt,
            curr,
            throttle,
        },
        units,
        recognized,
    }
}

/// Esleme ve birimlerle olcum orneklerini uretir. Bozuk satir: itki yok / sayi degil / negatif,
/// ya da eslenmis bir hucre dolu ama sayi degil.
pub fn build_samples(table: &RawTable, map: &Mapping, units: &Units) -> ParseReport {
    let mut rep = ParseReport {
        total_rows: table.rows.len(),
        ..Default::default()
    };
    let Some(ti) = map.thrust else {
        rep.skipped = table.rows.len();
        return rep;
    };
    let dc = table.decimal_comma;
    // Opsiyonel sutun: Ok(None) bos, Ok(Some) deger, Err bozuk.
    let opt = |row: &Vec<String>, idx: Option<usize>| -> Result<Option<f64>, ()> {
        let Some(i) = idx else { return Ok(None) };
        let Some(c) = row.get(i) else { return Ok(None) };
        if c.trim().is_empty() {
            return Ok(None);
        }
        parse_plain(c, dc).map(Some).ok_or(())
    };
    for row in &table.rows {
        let thrust = row.get(ti).and_then(|c| parse_plain(c, dc));
        let (Some(thrust), Ok(rpm), Ok(volt), Ok(curr), Ok(thr)) = (
            thrust,
            opt(row, map.rpm),
            opt(row, map.volt),
            opt(row, map.curr),
            opt(row, map.throttle),
        ) else {
            rep.skipped += 1;
            continue;
        };
        if thrust < 0.0 || rpm.is_some_and(|r| r < 0.0) {
            rep.skipped += 1;
            continue;
        }
        let rpm = rpm.map(|r| {
            if units.rpm_electrical && units.pole_pairs >= 1.0 {
                r / units.pole_pairs
            } else {
                r
            }
        });
        rep.samples.push(Sample {
            thrust_n: units.force.to_newton(thrust),
            rpm,
            volt,
            curr,
            throttle: thr.map(|v| units.throttle.to_fraction(v)),
        });
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semicolon_with_decimal_commas_and_headerless() {
        let lines = ["0,2;310,5;4800", "0,4;420,1;5200", "0,6;530,9;5600"];
        assert_eq!(detect_separator(&lines), ';');
        let lines = ["Test run, 2024", "a,b,c", "1,2,3", "4,5,6"];
        assert_eq!(detect_separator(&lines), ',');
    }

    #[test]
    fn force_unit_from_common_headers() {
        for (h, u) in [
            ("Thrust (kg)", ForceUnit::Kgf),
            ("Thrust (Newton)", ForceUnit::Newton),
            ("itki_N", ForceUnit::Newton),
            ("thrust_kg", ForceUnit::Kgf),
            ("Thrust (g)", ForceUnit::Gram),
        ] {
            let heads = vec![h.to_string(), "Voltage (V)".to_string()];
            assert_eq!(auto_map(&heads).units.force, u, "{h}");
        }
    }

    const RCB: &str = "Time (s),ESC signal (µs),Thrust (kgf),Voltage (V),Current (A),Motor Electrical Speed (RPM),Motor Optical Speed (RPM)\n\
        0,1000,0.00,16.0,0.1,0,0\n\
        1,1200,0.35,15.9,3.2,5000,5000\n\
        2,1400,0.80,15.7,8.5,\"7200\",7200\n\
        3,1600,bad,15.5,14,8800,8800\n\
        4,1800,1.60,15.3,22.0,10300,10300\n";

    #[test]
    fn rcbenchmark_headers_auto_map() {
        let t = parse_table(RCB);
        assert_eq!(t.separator, ',');
        let a = auto_map(&t.headers);
        assert!(a.recognized);
        assert_eq!(a.mapping.thrust, Some(2));
        assert_eq!(a.mapping.volt, Some(3));
        assert_eq!(a.mapping.curr, Some(4));
        assert_eq!(a.mapping.throttle, Some(1));
        // Optik devir elektrikselden tercih edilir.
        assert_eq!(a.mapping.rpm, Some(6));
        assert!(!a.units.rpm_electrical);
        assert_eq!(a.units.force, ForceUnit::Kgf);
        assert_eq!(a.units.throttle, ThrottleUnit::Micros);
    }

    #[test]
    fn electrical_only_is_flagged() {
        let h: Vec<String> = ["Thrust (N)", "Motor Electrical Speed (RPM)"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let a = auto_map(&h);
        assert!(a.units.rpm_electrical);
        assert_eq!(a.units.force, ForceUnit::Newton);
    }

    #[test]
    fn bad_rows_are_counted() {
        let t = parse_table(RCB);
        let a = auto_map(&t.headers);
        let r = build_samples(&t, &a.mapping, &a.units);
        assert_eq!(r.total_rows, 5);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.samples.len(), 4);
        assert!((r.samples[1].thrust_n - 0.35 * 9.80665).abs() < 1e-9);
        assert!((r.samples[1].throttle.unwrap() - 0.2).abs() < 1e-9);
        assert!((r.samples[1].power_w().unwrap() - 15.9 * 3.2).abs() < 1e-9);
    }

    #[test]
    fn generic_semicolon_decimal_comma() {
        let txt = "gaz;itki;devir;gerilim_V;akim_A\n0,2;310,5;4800;12,1;2,5\n0,5;800;7100;11,8;9,4\n;;;;\n";
        let t = parse_table(txt);
        assert_eq!(t.separator, ';');
        assert!(t.decimal_comma);
        let a = auto_map(&t.headers);
        assert_eq!(a.mapping.thrust, Some(1));
        assert_eq!(a.mapping.rpm, Some(2));
        let r = build_samples(&t, &a.mapping, &a.units);
        assert_eq!(r.samples.len(), 2);
        assert!((r.samples[0].volt.unwrap() - 12.1).abs() < 1e-9);
        assert_eq!(r.samples[0].thrust_n, ForceUnit::Gram.to_newton(310.5));
    }

    #[test]
    fn unit_conversions() {
        assert!((ForceUnit::Kgf.to_newton(1.0) - 9.80665).abs() < 1e-12);
        assert!((ForceUnit::Gram.to_newton(1000.0) - 9.80665).abs() < 1e-12);
        assert_eq!(ForceUnit::Newton.to_newton(3.0), 3.0);
        assert!((ThrottleUnit::Micros.to_fraction(1500.0) - 0.5).abs() < 1e-12);
        assert!((ThrottleUnit::Percent.to_fraction(25.0) - 0.25).abs() < 1e-12);
        assert_eq!(ThrottleUnit::Micros.to_fraction(900.0), 0.0);
    }

    #[test]
    fn electrical_rpm_divided_by_pole_pairs() {
        let t = parse_table("Thrust (N),Motor Electrical Speed (RPM)\n5,70000\n");
        let a = auto_map(&t.headers);
        let mut u = a.units;
        u.pole_pairs = 7.0;
        let r = build_samples(&t, &a.mapping, &u);
        assert!((r.samples[0].rpm.unwrap() - 10000.0).abs() < 1e-9);
    }

    #[test]
    fn garbage_and_empty_do_not_panic() {
        for s in [
            "",
            "\n\n",
            "\u{feff}",
            ",,,,",
            "a",
            "\"unterminated,1\n2",
            "x;y\n1;2;3;4\n;\n",
            "NaN,inf\nNaN,inf",
            "\u{0}\u{1}\u{2}",
        ] {
            let t = parse_table(s);
            let a = auto_map(&t.headers);
            let r = build_samples(&t, &a.mapping, &a.units);
            assert!(r.samples.iter().all(|x| x.thrust_n.is_finite()));
        }
        let t = parse_table("thrust,rpm\ninf,5\nNaN,3\n-2,4\n1,x\n");
        let a = auto_map(&t.headers);
        let r = build_samples(&t, &a.mapping, &a.units);
        assert_eq!(r.samples.len(), 0);
        assert_eq!(r.skipped, 4);
    }
}
