// SPDX-License-Identifier: AGPL-3.0-or-later
//! ArduPilot DataFlash (.bin) okuyucu: `flightlog-core` ile çözer ve Mission Planner CSV
//! dışa aktarımıyla aynı sütunları (`TimeUS,BAT.Volt,BAT.Curr,GPS.Spd,CTUN.Alt,CTUN.ThO,ESC.RPM`)
//! üretir; sonrası `csv` modülünün aynı eşleme/birim yolundan geçer.

use flightlog_core::{Log, ReadOptions, Table};
use std::fmt::Write;

/// İçerik ArduPilot DataFlash ise `true` (uzantıdan bağımsız).
pub fn is_dataflash(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xA3, 0x95])
}

/// (TimeUS, değer) çiftleri; ilk örnek (instance 0) kullanılır.
fn series(log: &Log, msg: &str, field: &str) -> Vec<(f64, f64)> {
    let Some(t) = log.table(msg) else {
        return Vec::new();
    };
    let t: Table = match t.instances().first() {
        Some(&i) => t.instance(i),
        None => t.clone(),
    };
    match (t.f64s("TimeUS"), t.f64s(field)) {
        (Some(ts), Some(vs)) => ts.into_iter().zip(vs).collect(),
        _ => Vec::new(),
    }
}

/// `.bin` baytlarını CSV metnine çevirir. Zaman ekseni BAT satırlarıdır; diğer seriler
/// her BAT anında son bilinen değerle (sample-and-hold) eşlenir.
pub fn bin_to_csv(bytes: &[u8]) -> Result<String, String> {
    let log = Log::read(bytes, &ReadOptions::default()).map_err(|e| e.to_string())?;
    let volt = series(&log, "BAT", "Volt");
    let cur = series(&log, "BAT", "Curr");
    if volt.is_empty() {
        return Err("BAT.Volt not logged".into());
    }
    let cols = [
        ("BAT.Curr", cur),
        ("GPS.Spd", series(&log, "GPS", "Spd")),
        ("CTUN.Alt", series(&log, "CTUN", "Alt")),
        ("CTUN.ThO", series(&log, "CTUN", "ThO")),
        ("ESC.RPM", series(&log, "ESC", "RPM")),
    ];
    let cols: Vec<_> = cols.into_iter().filter(|(_, s)| !s.is_empty()).collect();
    let mut out = String::from("TimeUS,BAT.Volt");
    for (h, _) in &cols {
        out.push(',');
        out.push_str(h);
    }
    out.push('\n');
    let mut idx = vec![0usize; cols.len()];
    for &(t, v) in &volt {
        let _ = write!(out, "{t},{v}");
        for ((_, s), k) in cols.iter().zip(idx.iter_mut()) {
            while *k + 1 < s.len() && s[*k + 1].0 <= t {
                *k += 1;
            }
            match s.get(*k) {
                Some(&(ts, x)) if ts <= t => {
                    let _ = write!(out, ",{x}");
                }
                _ => out.push(','),
            }
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flightlog::csv::{guess_mapping, read_headers, read_log_str};

    fn fmt(out: &mut Vec<u8>, ty: u8, len: u8, name: &str, format: &str, cols: &str) {
        out.extend_from_slice(&[0xA3, 0x95, 128, ty, len]);
        for (s, n) in [(name, 4), (format, 16), (cols, 64)] {
            let mut b = s.as_bytes().to_vec();
            b.resize(n, 0);
            out.extend_from_slice(&b);
        }
    }

    fn msg(out: &mut Vec<u8>, ty: u8, t: u64, vals: &[f32]) {
        out.extend_from_slice(&[0xA3, 0x95, ty]);
        out.extend_from_slice(&t.to_le_bytes());
        for v in vals {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// Küçük bir ArduPilot .bin: 3 BAT, 2 GPS, 3 CTUN mesajı.
    fn sample_bin() -> Vec<u8> {
        let mut b = Vec::new();
        fmt(
            &mut b,
            128,
            89,
            "FMT",
            "BBnNZ",
            "Type,Length,Name,Format,Columns",
        );
        fmt(&mut b, 10, 19, "BAT", "Qff", "TimeUS,Volt,Curr");
        fmt(&mut b, 11, 15, "GPS", "Qf", "TimeUS,Spd");
        fmt(&mut b, 12, 19, "CTUN", "Qff", "TimeUS,Alt,ThO");
        for (k, t) in [1_000_000u64, 1_100_000, 1_200_000].into_iter().enumerate() {
            msg(&mut b, 12, t - 50_000, &[10.0 + k as f32, 0.5]);
            msg(&mut b, 10, t, &[16.5 - 0.1 * k as f32, 20.0]);
            if k < 2 {
                msg(&mut b, 11, t + 10_000, &[5.0 * (k + 1) as f32]);
            }
        }
        b
    }

    #[test]
    fn csv_reader_alone_gets_no_power_from_bin() {
        let raw = String::from_utf8_lossy(&sample_bin()).into_owned();
        let h = read_headers(raw.as_bytes()).unwrap_or_default();
        let log = read_log_str(&raw, &guess_mapping(&h));
        assert!(log.map_or(true, |l| !l.has_power()));
    }

    #[test]
    fn bin_reads_through_csv_pipeline() {
        let bytes = sample_bin();
        assert!(is_dataflash(&bytes));
        let text = bin_to_csv(&bytes).unwrap();
        let h = read_headers(text.as_bytes()).unwrap();
        assert_eq!(
            h,
            [
                "TimeUS", "BAT.Volt", "BAT.Curr", "GPS.Spd", "CTUN.Alt", "CTUN.ThO"
            ]
        );
        let log = read_log_str(&text, &guess_mapping(&h)).unwrap();
        assert_eq!(log.len(), 3);
        assert!(log.has_power());
        assert!((log.t[2] - 0.2).abs() < 1e-9);
        assert!((log.power_at(0) - 16.5 * 20.0).abs() < 1e-3);
        assert!((log.alt_at(2) - 12.0).abs() < 1e-6);
        // GPS 1.01 s'de geldi: ilk BAT satırında yok, ikincisinde 5 m/s tutulur.
        assert!(!log.gs[0].is_finite());
        assert!((log.gs[1] - 5.0).abs() < 1e-6);
        assert!((log.thr[0] - 50.0).abs() < 1e-6);
    }
}
