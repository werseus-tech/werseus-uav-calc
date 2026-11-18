// SPDX-License-Identifier: AGPL-3.0-or-later
//! Log analizi: askı/seyir kesimi tespiti, kesim istatistikleri, harcanan kapasite/enerji ve
//! gerilim çökmesinden iç direnç tahmini.

use super::csv::Log;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegKind {
    Hover,
    Cruise,
}

/// Tespit ayarları.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Askı: bu hızın altı (m/s).
    pub hover_max_speed: f64,
    /// Askı: irtifa kesim ortalamasından en çok bu kadar sapar (m).
    pub hover_alt_band: f64,
    /// Seyir: bu hızın üstü (m/s).
    pub cruise_min_speed: f64,
    /// Seyir: hız kesim ortalamasından en çok bu oranda sapar.
    pub cruise_speed_band: f64,
    /// Her iki tür: en çok dikey hız (m/s).
    pub max_climb_rate: f64,
    /// En kısa kesim süresi (s).
    pub min_duration: f64,
    /// Aktif sayılması için en az elektrik gücü (W) ya da gaz (%).
    pub min_power_w: f64,
    pub min_throttle_pct: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hover_max_speed: 1.5,
            hover_alt_band: 1.5,
            cruise_min_speed: 3.0,
            cruise_speed_band: 0.2,
            max_climb_rate: 1.0,
            min_duration: 5.0,
            min_power_w: 1.0,
            min_throttle_pct: 5.0,
        }
    }
}

/// Bir kesimin ortalamaları (NaN: o sinyal yok).
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub kind: SegKind,
    pub t0: f64,
    pub t1: f64,
    pub mean_v: f64,
    pub mean_i: f64,
    pub mean_p: f64,
    pub mean_thr: f64,
    pub mean_speed: f64,
    pub mean_alt: f64,
    pub mean_rpm: f64,
}

impl Segment {
    pub fn duration(&self) -> f64 {
        self.t1 - self.t0
    }
}

/// Tüm log özeti.
#[derive(Clone, Copy, Debug, Default)]
pub struct Overall {
    pub duration_s: f64,
    pub mean_p: f64,
    pub mean_i: f64,
    pub max_i: f64,
    pub max_p: f64,
    pub min_v: f64,
    pub used_mah: f64,
    pub used_wh: f64,
    /// Akımın > eşik olduğu "aktif" sürenin ortalama gücü (W); kalkış öncesi bekleme dışarıda.
    pub active_mean_p: f64,
}

/// İç direnç doğrusal uyumu: V = V0 − R·I (− k·Q).
#[derive(Clone, Copy, Debug)]
pub struct ResistanceFit {
    pub r_ohm: f64,
    pub v0: f64,
    pub r2: f64,
    pub n: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub overall: Overall,
    pub segments: Vec<Segment>,
    pub resistance: Option<ResistanceFit>,
}

// ---------------------------------------------------------------------------------------------

fn mean_finite(it: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut n) = (0.0, 0usize);
    for x in it {
        if x.is_finite() {
            s += x;
            n += 1;
        }
    }
    if n == 0 { f64::NAN } else { s / n as f64 }
}

/// Zaman penceresi [t0, t1] içindeki örnek indeks aralığı.
fn range_of(log: &Log, t0: f64, t1: f64) -> std::ops::Range<usize> {
    let a = log.t.partition_point(|&t| t < t0);
    let b = log.t.partition_point(|&t| t <= t1);
    a..b
}

/// Yamuk kuralı ile ∫ x dt; `max_gap` üstü boşluklar atlanır. x yoksa 0.
fn integrate(log: &Log, x: impl Fn(usize) -> f64, max_gap: f64) -> f64 {
    let mut acc = 0.0;
    for k in 1..log.len() {
        let dt = log.t[k] - log.t[k - 1];
        if dt <= 0.0 || dt > max_gap {
            continue;
        }
        let (a, b) = (x(k - 1), x(k));
        if a.is_finite() && b.is_finite() {
            acc += 0.5 * (a + b) * dt;
        }
    }
    acc
}

/// Harcanan kapasite (mAh) ve enerji (Wh).
pub fn used_capacity(log: &Log) -> (f64, f64) {
    if log.cur.is_empty() {
        return (0.0, 0.0);
    }
    let mah = integrate(log, |k| log.i_at(k), 5.0) / 3.6;
    let wh = if log.volt.is_empty() {
        0.0
    } else {
        integrate(log, |k| log.power_at(k), 5.0) / 3600.0
    };
    (mah, wh)
}

pub fn overall(log: &Log) -> Overall {
    let mut o = Overall {
        duration_s: log.duration(),
        mean_p: f64::NAN,
        mean_i: f64::NAN,
        min_v: f64::NAN,
        active_mean_p: f64::NAN,
        ..Default::default()
    };
    let n = log.len();
    if !log.cur.is_empty() {
        o.mean_i = mean_finite((0..n).map(|k| log.i_at(k)));
        o.max_i = log
            .cur
            .iter()
            .copied()
            .filter(|x| x.is_finite())
            .fold(0.0, f64::max);
    }
    if !log.volt.is_empty() {
        o.min_v = log
            .volt
            .iter()
            .copied()
            .filter(|x| x.is_finite())
            .fold(f64::NAN, |a, b| if a.is_nan() { b } else { a.min(b) });
    }
    if log.has_power() {
        o.mean_p = mean_finite((0..n).map(|k| log.power_at(k)));
        o.max_p = (0..n)
            .map(|k| log.power_at(k))
            .filter(|x| x.is_finite())
            .fold(0.0, f64::max);
        // Aktif: akım en az tepe akımın %10'u (kalkış öncesi / sonrası beklemeyi dışarıda bırakır).
        let thr = (0.1 * o.max_i).max(0.5);
        o.active_mean_p = mean_finite((0..n).map(|k| {
            if log.i_at(k) >= thr {
                log.power_at(k)
            } else {
                f64::NAN
            }
        }));
    }
    let (mah, wh) = used_capacity(log);
    o.used_mah = mah;
    o.used_wh = wh;
    o
}

// ---------------------------------------------------------------------------------------------
// Kesim tespiti
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Bin {
    t0: f64,
    t1: f64,
    speed: f64,
    alt: f64,
    vz: f64,
    active: bool,
}

/// Log'u ~`step` saniyelik kutulara böler ve her kutu için ortalama hız/irtifa/dikey hız çıkarır.
fn make_bins(log: &Log, step: f64, s: &Settings) -> Vec<Bin> {
    let mut bins = Vec::new();
    if log.is_empty() {
        return bins;
    }
    let start = log.t[0];
    let end = *log.t.last().unwrap();
    let mut t = start;
    let mut prev_alt = f64::NAN;
    let mut prev_t = f64::NAN;
    while t < end + 1e-9 {
        let r = range_of(log, t, t + step - 1e-9);
        if !r.is_empty() {
            let speed = mean_finite(r.clone().map(|k| log.gs_at(k)));
            let alt = mean_finite(r.clone().map(|k| log.alt_at(k)));
            let tc = 0.5 * (log.t[r.start] + log.t[r.end - 1]);
            let vz = if alt.is_finite() && prev_alt.is_finite() && tc > prev_t {
                (alt - prev_alt) / (tc - prev_t)
            } else {
                0.0
            };
            let p = mean_finite(r.clone().map(|k| log.power_at(k)));
            let thr = mean_finite(r.clone().map(|k| log.thr_at(k)));
            let active = if p.is_finite() {
                p >= s.min_power_w
            } else if thr.is_finite() {
                thr >= s.min_throttle_pct
            } else {
                true
            };
            bins.push(Bin {
                t0: t,
                t1: t + step,
                speed,
                alt,
                vz,
                active,
            });
            prev_alt = alt;
            prev_t = tc;
        }
        t += step;
    }
    bins
}

fn finish(log: &Log, kind: SegKind, t0: f64, t1: f64) -> Segment {
    let r = range_of(log, t0, t1);
    Segment {
        kind,
        t0,
        t1,
        mean_v: mean_finite(r.clone().map(|k| log.v_at(k))),
        mean_i: mean_finite(r.clone().map(|k| log.i_at(k))),
        mean_p: mean_finite(r.clone().map(|k| log.power_at(k))),
        mean_thr: mean_finite(r.clone().map(|k| log.thr_at(k))),
        mean_speed: match kind {
            SegKind::Hover => mean_finite(r.clone().map(|k| log.gs_at(k))),
            SegKind::Cruise => mean_finite(r.clone().map(|k| log.asp_at(k))),
        },
        mean_alt: mean_finite(r.clone().map(|k| log.alt_at(k))),
        mean_rpm: mean_finite(r.map(|k| log.rpm_at(k))),
    }
}

/// Askı ve seyir kesimlerini bulur. Hız sinyali yoksa boş döner.
pub fn detect_segments(log: &Log, s: &Settings) -> Vec<Segment> {
    if !log.has_speed() || log.len() < 2 {
        return Vec::new();
    }
    let dur = log.duration();
    // ~1 s kutu; çok uzun loglarda kutu sayısı ≤ ~20 bin.
    let step = (dur / 20_000.0).max(1.0);
    let bins = make_bins(log, step, s);
    let mut out = Vec::new();
    // Aday sınıfı: Some(kind) ya da None.
    let class = |b: &Bin| -> Option<SegKind> {
        if !b.active || !b.speed.is_finite() || b.vz.abs() > s.max_climb_rate {
            return None;
        }
        if b.speed < s.hover_max_speed {
            Some(SegKind::Hover)
        } else if b.speed >= s.cruise_min_speed {
            Some(SegKind::Cruise)
        } else {
            None
        }
    };
    let mut cur: Option<(SegKind, f64, f64, f64, usize)> = None; // (tür, t0, t1, toplam, n)
    let close = |cur: &mut Option<(SegKind, f64, f64, f64, usize)>, out: &mut Vec<Segment>| {
        if let Some((kind, t0, t1, _, _)) = cur.take() {
            if t1 - t0 + 1e-9 >= s.min_duration {
                out.push(finish(log, kind, t0, t1));
            }
        }
    };
    for b in &bins {
        match class(b) {
            None => close(&mut cur, &mut out),
            Some(kind) => {
                let fits = match &cur {
                    Some((k, _, _, sum, n)) if *k == kind => {
                        let mean = sum / *n as f64;
                        match kind {
                            SegKind::Cruise => (b.speed - mean).abs() <= s.cruise_speed_band * mean,
                            SegKind::Hover => {
                                // Hava durumu: irtifa kesim ortalamasından sapmasın (irtifa yoksa geçerli).
                                true
                            }
                        }
                    }
                    _ => false,
                };
                let alt_ok = match (&cur, kind) {
                    (Some((_, t0, _, _, _)), SegKind::Hover) => {
                        let m = mean_finite(range_of(log, *t0, b.t0).map(|k| log.alt_at(k)));
                        !(m.is_finite()
                            && b.alt.is_finite()
                            && (b.alt - m).abs() > s.hover_alt_band)
                    }
                    _ => true,
                };
                if fits && alt_ok {
                    if let Some(c) = cur.as_mut() {
                        c.2 = b.t1.min(*log.t.last().unwrap());
                        c.3 += b.speed;
                        c.4 += 1;
                    }
                } else {
                    close(&mut cur, &mut out);
                    cur = Some((kind, b.t0, b.t1.min(*log.t.last().unwrap()), b.speed, 1));
                }
            }
        }
    }
    close(&mut cur, &mut out);
    out
}

// ---------------------------------------------------------------------------------------------
// İç direnç
// ---------------------------------------------------------------------------------------------

/// Gauss yok etme ile n×n sistem çözümü; tekilse None.
#[allow(clippy::needless_range_loop)]
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-12 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for c in (0..n).rev() {
        let mut s = b[c];
        for k in c + 1..n {
            s -= a[c][k] * x[k];
        }
        x[c] = s / a[c][c];
    }
    Some(x)
}

/// En küçük kareler: y = Σ βj·xj. R² ile birlikte.
fn lsq(rows: &[(Vec<f64>, f64)]) -> Option<(Vec<f64>, f64)> {
    let m = rows.first()?.0.len();
    let mut ata = vec![vec![0.0; m]; m];
    let mut atb = vec![0.0; m];
    for (x, y) in rows {
        for i in 0..m {
            atb[i] += x[i] * y;
            for j in 0..m {
                ata[i][j] += x[i] * x[j];
            }
        }
    }
    let beta = solve(ata, atb)?;
    let ymean = rows.iter().map(|r| r.1).sum::<f64>() / rows.len() as f64;
    let (mut ss_res, mut ss_tot) = (0.0, 0.0);
    for (x, y) in rows {
        let yh: f64 = x.iter().zip(&beta).map(|(a, b)| a * b).sum();
        ss_res += (y - yh).powi(2);
        ss_tot += (y - ymean).powi(2);
    }
    let r2 = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        1.0
    };
    Some((beta, r2))
}

/// Gerilim çökmesi → iç direnç. Önce V = V0 − R·I − k·Q (Q: o ana dek çekilen Ah, doluluk
/// düşüşünü ayırır); tekilse V = V0 − R·I. Akım değişimi yetersizse (<2 A ya da <%20) None.
pub fn fit_resistance(log: &Log) -> Option<ResistanceFit> {
    if !log.has_power() || log.len() < 20 {
        return None;
    }
    let n = log.len();
    let stride = (n / 100_000).max(1);
    let mut q = 0.0; // Ah
    let mut rows3: Vec<(Vec<f64>, f64)> = Vec::new();
    let mut rows2: Vec<(Vec<f64>, f64)> = Vec::new();
    let (mut imin, mut imax) = (f64::INFINITY, f64::NEG_INFINITY);
    for k in 0..n {
        if k > 0 {
            let dt = log.t[k] - log.t[k - 1];
            let (a, b) = (log.i_at(k - 1), log.i_at(k));
            if dt > 0.0 && dt <= 5.0 && a.is_finite() && b.is_finite() {
                q += 0.5 * (a + b) * dt / 3600.0;
            }
        }
        if k % stride != 0 {
            continue;
        }
        let (v, i) = (log.v_at(k), log.i_at(k));
        if !v.is_finite() || !i.is_finite() || v <= 0.0 {
            continue;
        }
        imin = imin.min(i);
        imax = imax.max(i);
        rows3.push((vec![1.0, i, q], v));
        rows2.push((vec![1.0, i], v));
    }
    if rows2.len() < 20 {
        return None;
    }
    let span = imax - imin;
    let imean = rows2.iter().map(|r| r.0[1]).sum::<f64>() / rows2.len() as f64;
    if span < 2.0 || span < 0.2 * imean.abs() {
        return None;
    }
    let from3 = lsq(&rows3).map(|(b, r2)| (b[0], -b[1], r2));
    let pick = match from3 {
        Some((v0, r, r2)) if r > 0.0 => Some((v0, r, r2)),
        _ => lsq(&rows2).map(|(b, r2)| (b[0], -b[1], r2)),
    };
    let (v0, r, r2) = pick?;
    if !(r.is_finite() && r > 0.0) {
        return None;
    }
    Some(ResistanceFit {
        r_ohm: r,
        v0,
        r2,
        n: rows2.len(),
    })
}

/// Tam analiz.
pub fn analyze(log: &Log) -> Analysis {
    Analysis {
        overall: overall(log),
        segments: detect_segments(log, &Settings::default()),
        resistance: fit_resistance(log),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Sentetik log: `f(t) -> (v, i, gs, alt)`, 10 Hz.
    pub fn synth(secs: f64, f: impl Fn(f64) -> (f64, f64, f64, f64)) -> Log {
        let n = (secs * 10.0) as usize + 1;
        let mut l = Log::default();
        for k in 0..n {
            let t = k as f64 * 0.1;
            let (v, i, gs, alt) = f(t);
            l.t.push(t);
            l.volt.push(v);
            l.cur.push(i);
            l.gs.push(gs);
            l.alt.push(alt);
        }
        l
    }

    #[test]
    fn constant_current_mah() {
        // 10 A sabit, 360 s = 1000 mAh; 12 V → 12*10*0.1 h = 12 Wh.
        let l = synth(360.0, |_| (12.0, 10.0, 0.0, 10.0));
        let (mah, wh) = used_capacity(&l);
        assert!((mah - 1000.0).abs() < 0.5, "{mah}");
        assert!((wh - 12.0).abs() < 0.01, "{wh}");
        let o = overall(&l);
        assert!((o.mean_p - 120.0).abs() < 1e-9);
        assert!((o.used_mah - 1000.0).abs() < 0.5);
    }

    #[test]
    fn resistance_fit_recovers_known_r() {
        // V = 25 − 0.02·I − 0.001·Q(mAh) ile akım salınımı.
        let mut l = Log::default();
        let mut q_mah = 0.0;
        for k in 0..2000 {
            let t = k as f64 * 0.1;
            let i = 15.0 + 10.0 * (t * 0.7).sin() + 4.0 * (t * 0.13).cos();
            if k > 0 {
                q_mah += i * 0.1 / 3.6;
            }
            l.t.push(t);
            l.cur.push(i);
            l.volt.push(25.0 - 0.02 * i - 0.001 * q_mah);
        }
        let f = fit_resistance(&l).expect("fit");
        assert!((f.r_ohm - 0.02).abs() < 0.0015, "R = {}", f.r_ohm);
        assert!(f.r2 > 0.99);
        // Sabit akım: tahmin yok.
        let flat = synth(100.0, |_| (12.0, 10.0, 0.0, 0.0));
        assert!(fit_resistance(&flat).is_none());
    }

    #[test]
    fn detects_hover_and_cruise() {
        // 0–10 s yerde (güç yok) · 10–40 s askı (400 W) · 40–50 s geçiş (ivmelenme) · 50–110 s seyir 15 m/s (200 W)
        let l = synth(120.0, |t| {
            if t < 10.0 {
                (12.0, 0.0, 0.0, 0.0)
            } else if t < 40.0 {
                (12.0, 400.0 / 12.0, 0.1, 20.0)
            } else if t < 50.0 {
                let g = (t - 40.0) * 1.5;
                (12.0, 30.0, g, 20.0)
            } else if t < 110.0 {
                (12.0, 200.0 / 12.0, 15.0, 20.0)
            } else {
                (12.0, 0.0, 0.0, 0.0)
            }
        });
        let a = analyze(&l);
        let hov: Vec<_> = a
            .segments
            .iter()
            .filter(|s| s.kind == SegKind::Hover)
            .collect();
        let cru: Vec<_> = a
            .segments
            .iter()
            .filter(|s| s.kind == SegKind::Cruise)
            .collect();
        assert_eq!(hov.len(), 1, "{:?}", a.segments);
        assert!((hov[0].mean_p - 400.0).abs() < 5.0);
        assert!(hov[0].duration() > 25.0);
        assert!(!cru.is_empty());
        let big = cru
            .iter()
            .max_by(|a, b| a.duration().total_cmp(&b.duration()))
            .unwrap();
        assert!((big.mean_speed - 15.0).abs() < 0.5);
        assert!((big.mean_p - 200.0).abs() < 15.0);
        assert!(big.duration() > 50.0);
        // Yerdeki bekleme (güç 0) askı sayılmaz.
        assert!(hov[0].t0 >= 9.0);
    }

    #[test]
    fn no_speed_means_no_segments() {
        let mut l = synth(30.0, |_| (12.0, 5.0, 0.0, 0.0));
        l.gs.clear();
        assert!(detect_segments(&l, &Settings::default()).is_empty());
    }

    #[test]
    fn empty_log_does_not_panic() {
        let l = Log::default();
        let a = analyze(&l);
        assert!(a.segments.is_empty());
        assert!(a.resistance.is_none());
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use crate::flightlog::csv::{guess_mapping, read_headers, read_log_str};

    fn load(s: &str) -> Log {
        let h = read_headers(std::io::Cursor::new(s)).unwrap();
        read_log_str(s, &guess_mapping(&h)).unwrap()
    }

    #[test]
    fn synthetic_fixtures_from_three_ecosystems_analyse_alike() {
        let files = [
            include_str!("../../tests/fixtures/flightlog/betaflight_blackbox.csv"),
            include_str!("../../tests/fixtures/flightlog/ardupilot_export.csv"),
            include_str!("../../tests/fixtures/flightlog/px4_battery_status.csv"),
        ];
        for (n, f) in files.iter().enumerate() {
            let log = load(f);
            assert!(log.has_power() && log.has_speed(), "file {n}");
            assert_eq!(log.bad_rows, 0, "file {n}");
            let a = analyze(&log);
            let hov = a
                .segments
                .iter()
                .filter(|s| s.kind == SegKind::Hover)
                .count();
            let cru = a
                .segments
                .iter()
                .filter(|s| s.kind == SegKind::Cruise)
                .count();
            assert!(hov >= 1 && cru >= 1, "file {n}: {:?}", a.segments);
            let r = a.resistance.expect("resistance");
            assert!((r.r_ohm - 0.02).abs() < 0.006, "file {n}: R = {}", r.r_ohm);
            assert!(a.overall.used_mah > 100.0, "file {n}");
        }
    }
}
