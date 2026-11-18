// SPDX-License-Identifier: AGPL-3.0-or-later
//! Uçuş logu CSV okuyucu: başlık tahmini (Betaflight / ArduPilot / PX4), sütun eşleme, birim
//! dönüşümü ve satır satır (akış) okuma. İkili günlük dosyaları (.bbl/.bin/.ulg) okunmaz.

use std::io::BufRead;

/// Okunacak en çok satır sayısı (aşılırsa uyarı ile kesilir).
pub const MAX_ROWS: usize = 2_000_000;

/// Eşlenebilir alanlar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Time,
    Voltage,
    Current,
    Throttle,
    Altitude,
    GroundSpeed,
    AirSpeed,
    Rpm,
}

impl Field {
    pub const ALL: [Field; 8] = [
        Field::Time,
        Field::Voltage,
        Field::Current,
        Field::Throttle,
        Field::Altitude,
        Field::GroundSpeed,
        Field::AirSpeed,
        Field::Rpm,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Auto,
    Micro,
    Milli,
    Sec,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoltUnit {
    Auto,
    Volt,
    MilliVolt,
    CentiVolt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurUnit {
    Auto,
    Amp,
    MilliAmp,
    CentiAmp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedUnit {
    Auto,
    MetersPerSec,
    KmPerHour,
    CmPerSec,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AltUnit {
    Auto,
    Meter,
    Centimeter,
    Millimeter,
}

/// Sütun eşlemesi (başlık sırası indeksleri) ve birimler.
#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    pub time: Option<usize>,
    pub voltage: Option<usize>,
    pub current: Option<usize>,
    pub throttle: Option<usize>,
    pub altitude: Option<usize>,
    pub ground_speed: Option<usize>,
    pub air_speed: Option<usize>,
    pub rpm: Option<usize>,
    pub time_unit: TimeUnit,
    pub volt_unit: VoltUnit,
    pub cur_unit: CurUnit,
    pub speed_unit: SpeedUnit,
    pub alt_unit: AltUnit,
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping {
            time: None,
            voltage: None,
            current: None,
            throttle: None,
            altitude: None,
            ground_speed: None,
            air_speed: None,
            rpm: None,
            time_unit: TimeUnit::Auto,
            volt_unit: VoltUnit::Auto,
            cur_unit: CurUnit::Auto,
            speed_unit: SpeedUnit::Auto,
            alt_unit: AltUnit::Auto,
        }
    }
}

impl Mapping {
    pub fn get(&self, f: Field) -> Option<usize> {
        match f {
            Field::Time => self.time,
            Field::Voltage => self.voltage,
            Field::Current => self.current,
            Field::Throttle => self.throttle,
            Field::Altitude => self.altitude,
            Field::GroundSpeed => self.ground_speed,
            Field::AirSpeed => self.air_speed,
            Field::Rpm => self.rpm,
        }
    }
    pub fn set(&mut self, f: Field, v: Option<usize>) {
        match f {
            Field::Time => self.time = v,
            Field::Voltage => self.voltage = v,
            Field::Current => self.current = v,
            Field::Throttle => self.throttle = v,
            Field::Altitude => self.altitude = v,
            Field::GroundSpeed => self.ground_speed = v,
            Field::AirSpeed => self.air_speed = v,
            Field::Rpm => self.rpm = v,
        }
    }
}

/// Okunmuş ve SI birimlerine çevrilmiş log. Eşlenmemiş alanların vektörü boştur.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// Saniye, 0'dan başlar, kesin artan.
    pub t: Vec<f64>,
    pub volt: Vec<f64>,
    pub cur: Vec<f64>,
    /// Yüzde (0–100).
    pub thr: Vec<f64>,
    pub alt: Vec<f64>,
    pub gs: Vec<f64>,
    pub asp: Vec<f64>,
    pub rpm: Vec<f64>,
    /// Atlanan (bozuk) satır sayısı.
    pub bad_rows: usize,
    /// Dosyadaki toplam veri satırı (boş satırlar hariç).
    pub total_rows: usize,
    /// `MAX_ROWS` nedeniyle kesildi.
    pub truncated: bool,
    /// Zaman sütunu eşlenmedi: 10 Hz varsayıldı.
    pub time_assumed: bool,
}

impl Log {
    pub fn len(&self) -> usize {
        self.t.len()
    }
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
    pub fn duration(&self) -> f64 {
        match (self.t.first(), self.t.last()) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        }
    }
    fn at(v: &[f64], k: usize) -> f64 {
        v.get(k).copied().unwrap_or(f64::NAN)
    }
    pub fn v_at(&self, k: usize) -> f64 {
        Self::at(&self.volt, k)
    }
    pub fn i_at(&self, k: usize) -> f64 {
        Self::at(&self.cur, k)
    }
    pub fn thr_at(&self, k: usize) -> f64 {
        Self::at(&self.thr, k)
    }
    pub fn alt_at(&self, k: usize) -> f64 {
        Self::at(&self.alt, k)
    }
    /// Yer hızı; yoksa hava hızı.
    pub fn gs_at(&self, k: usize) -> f64 {
        let g = Self::at(&self.gs, k);
        if g.is_finite() {
            g
        } else {
            Self::at(&self.asp, k)
        }
    }
    /// Hava hızı; yoksa yer hızı.
    pub fn asp_at(&self, k: usize) -> f64 {
        let a = Self::at(&self.asp, k);
        if a.is_finite() {
            a
        } else {
            Self::at(&self.gs, k)
        }
    }
    pub fn rpm_at(&self, k: usize) -> f64 {
        Self::at(&self.rpm, k)
    }
    /// Anlık elektrik gücü V·I (W); biri yoksa NaN.
    pub fn power_at(&self, k: usize) -> f64 {
        self.v_at(k) * self.i_at(k)
    }
    pub fn has_power(&self) -> bool {
        !self.volt.is_empty() && !self.cur.is_empty()
    }
    pub fn has_speed(&self) -> bool {
        !self.gs.is_empty() || !self.asp.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogError {
    /// Dosya boş ya da başlık satırı yok.
    Empty,
    /// Hiç geçerli veri satırı çıkmadı.
    NoValidRows,
    /// Okuma hatası (metin).
    Io(String),
}

// ---------------------------------------------------------------------------------------------
// Satır / başlık ayrıştırma
// ---------------------------------------------------------------------------------------------

fn detect_delimiter(line: &str) -> char {
    let mut best = (',', 0usize);
    for d in [',', ';', '\t'] {
        let n = line.matches(d).count();
        if n > best.1 {
            best = (d, n);
        }
    }
    best.0
}

/// Tırnak farkındalıklı alan bölme.
fn split_fields(line: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for ch in line.chars() {
        if ch == '"' {
            in_q = !in_q;
        } else if ch == delim && !in_q {
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn parse_num(s: &str, decimal_comma: bool) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return f64::NAN;
    }
    let r = if decimal_comma && s.contains(',') {
        s.replace(',', ".").parse::<f64>()
    } else {
        s.parse::<f64>()
    };
    match r {
        Ok(v) if v.is_finite() => v,
        _ => f64::NAN,
    }
}

/// İlk boş olmayan satırı başlık olarak okur. Dönüş: (başlıklar, ayraç, sonraki satır okunabilir).
fn read_header<R: BufRead>(r: &mut R) -> Result<(Vec<String>, char), LogError> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = r
            .read_line(&mut line)
            .map_err(|e| LogError::Io(e.to_string()))?;
        if n == 0 {
            return Err(LogError::Empty);
        }
        let l = line
            .trim_start_matches('\u{feff}')
            .trim_end_matches(['\r', '\n']);
        if l.trim().is_empty() {
            continue;
        }
        let delim = detect_delimiter(l);
        let h = split_fields(l, delim);
        return Ok((h, delim));
    }
}

/// Yalnızca başlık satırını okur.
pub fn read_headers<R: BufRead>(mut r: R) -> Result<Vec<String>, LogError> {
    read_header(&mut r).map(|(h, _)| h)
}

// ---------------------------------------------------------------------------------------------
// Başlık tahmini
// ---------------------------------------------------------------------------------------------

fn norm(h: &str) -> String {
    h.trim().to_lowercase()
}

/// Başlık adı bir birim ipucu içeriyor mu: parantez içi birim (ör. "(us)") ya da sonek.
fn unit_tag(n: &str) -> Option<String> {
    let open = n.rfind('(')?;
    let close = n[open..].find(')')? + open;
    Some(n[open + 1..close].trim().to_string())
}

/// Her alan için aday adlar: (tam eşleşme listesi, içerir listesi). Sıra öncelik sırasıdır.
fn candidates(f: Field) -> (&'static [&'static str], &'static [&'static str]) {
    match f {
        Field::Time => (
            &[
                "time (us)",
                "time(us)",
                "timeus",
                "time_us",
                "timestamp",
                "time (ms)",
                "timems",
                "time_ms",
                "time (s)",
                "time",
                "time_s",
                "t",
                "gps.timems",
            ],
            &["time"],
        ),
        Field::Voltage => (
            &[
                "vbatlatest (v)",
                "vbatlatest",
                "vbat (v)",
                "vbat",
                "bat.volt",
                "bat.voltr",
                "voltage_filtered_v",
                "voltage_v",
                "battery_voltage",
                "bat_volt",
                "volt",
                "voltage",
                "pack voltage (v)",
            ],
            &["vbat", "volt"],
        ),
        Field::Current => (
            &[
                "amperagelatest (a)",
                "amperagelatest",
                "amperage (a)",
                "current_filtered_a",
                "current_a",
                "bat.curr",
                "battery_current",
                "bat_curr",
                "curr",
                "current",
                "amperage",
            ],
            &["amperage", "curr"],
        ),
        Field::Throttle => (
            &[
                "ctun.tho",
                "ctun.thi",
                "rccommand[3]",
                "throttle",
                "thr",
                "throttle (%)",
                "motor[0]",
                "motor_0",
                "rc.c3",
                "rcin.c3",
                "actuator_controls_0[3]",
            ],
            &["throttle", "motor["],
        ),
        Field::Altitude => (
            &[
                "ctun.alt",
                "ctun.baralt",
                "baroalt (cm)",
                "baroalt",
                "gps_altitude",
                "gps.alt",
                "altitude",
                "alt",
                "baro_alt_meter",
                "altitude (m)",
                "relalt",
            ],
            &["alt"],
        ),
        Field::GroundSpeed => (
            &[
                "gps.spd",
                "gps_speed (m/s)",
                "gps_speed",
                "gpsspeed",
                "groundspeed",
                "vel_m_s",
                "ground_speed",
                "gps.gspd",
                "speed",
                "gs",
            ],
            &["gps_speed", "groundspeed", "gps.spd", "vel_m_s"],
        ),
        Field::AirSpeed => (
            &[
                "arsp.airspeed",
                "airspeed",
                "airspeed (m/s)",
                "indicated_airspeed_m_s",
                "true_airspeed_m_s",
                "aspd",
                "ias",
                "tas",
            ],
            &["airspeed"],
        ),
        Field::Rpm => (
            &[
                "rpm.rpm",
                "esc.rpm",
                "erpm[0]",
                "rpm",
                "motor_rpm",
                "esc_rpm",
                "rpm1",
                "esc_status.rpm",
            ],
            &["rpm"],
        ),
    }
}

fn pick(headers: &[String], f: Field, used: &[usize]) -> Option<usize> {
    let (exact, contains) = candidates(f);
    let normed: Vec<String> = headers.iter().map(|h| norm(h)).collect();
    for want in exact {
        if let Some(i) = normed.iter().position(|h| h == want) {
            if !used.contains(&i) {
                return Some(i);
            }
        }
    }
    for want in contains {
        if let Some(i) = normed.iter().position(|h| h.contains(want)) {
            if used.contains(&i) {
                continue;
            }
            // Zaman: "timeus" vb. dışında "gps.time" / "loopiteration" karışmasın.
            if f == Field::Voltage && (normed[i].contains("cell") || normed[i].contains("rail")) {
                continue;
            }
            if f == Field::Altitude && normed[i].contains("alt_") {
                continue;
            }
            return Some(i);
        }
    }
    None
}

/// Başlıklardan eşleme tahmini ve ad ipuçlarına göre birim seçimi.
pub fn guess_mapping(headers: &[String]) -> Mapping {
    let mut m = Mapping::default();
    let mut used: Vec<usize> = Vec::new();
    for f in Field::ALL {
        if let Some(i) = pick(headers, f, &used) {
            used.push(i);
            m.set(f, Some(i));
        }
    }
    let name = |i: Option<usize>| i.map(|i| norm(&headers[i])).unwrap_or_default();
    // Zaman
    let tn = name(m.time);
    let tag = unit_tag(&tn).unwrap_or_default();
    m.time_unit = if tag == "us" || tag == "µs" || tn.ends_with("us") || tn == "timestamp" {
        TimeUnit::Micro
    } else if tag == "ms" || tn.ends_with("ms") {
        TimeUnit::Milli
    } else if tag == "s" || tag == "sec" || tn.ends_with("_s") {
        TimeUnit::Sec
    } else {
        TimeUnit::Auto
    };
    // Gerilim
    let vn = name(m.voltage);
    let tag = unit_tag(&vn).unwrap_or_default();
    m.volt_unit = match tag.as_str() {
        "v" => VoltUnit::Volt,
        "mv" => VoltUnit::MilliVolt,
        "cv" => VoltUnit::CentiVolt,
        _ if vn.ends_with("_v") || vn.starts_with("bat.volt") => VoltUnit::Volt,
        _ => VoltUnit::Auto,
    };
    // Akım
    let cn = name(m.current);
    let tag = unit_tag(&cn).unwrap_or_default();
    m.cur_unit = match tag.as_str() {
        "a" => CurUnit::Amp,
        "ma" => CurUnit::MilliAmp,
        "ca" => CurUnit::CentiAmp,
        _ if cn.ends_with("_a") || cn.starts_with("bat.curr") => CurUnit::Amp,
        _ => CurUnit::Auto,
    };
    // Hız (yer hızı ya da hava hızı adı)
    let sn = if m.ground_speed.is_some() {
        name(m.ground_speed)
    } else {
        name(m.air_speed)
    };
    let tag = unit_tag(&sn).unwrap_or_default();
    m.speed_unit = match tag.as_str() {
        "m/s" => SpeedUnit::MetersPerSec,
        "km/h" | "kmh" | "kph" => SpeedUnit::KmPerHour,
        "cm/s" => SpeedUnit::CmPerSec,
        _ if sn.ends_with("_m_s") || sn.starts_with("gps.spd") || sn.starts_with("arsp.") => {
            SpeedUnit::MetersPerSec
        }
        _ => SpeedUnit::Auto,
    };
    // İrtifa
    let an = name(m.altitude);
    let tag = unit_tag(&an).unwrap_or_default();
    m.alt_unit = match tag.as_str() {
        "m" => AltUnit::Meter,
        "cm" => AltUnit::Centimeter,
        "mm" => AltUnit::Millimeter,
        _ if an.starts_with("ctun.") || an.ends_with("_meter") => AltUnit::Meter,
        _ if an.contains("baroalt") => AltUnit::Centimeter,
        _ => AltUnit::Auto,
    };
    m
}

// ---------------------------------------------------------------------------------------------
// Birim çözümleme
// ---------------------------------------------------------------------------------------------

fn finite_minmax(v: &[f64]) -> Option<(f64, f64)> {
    let mut it = v.iter().copied().filter(|x| x.is_finite());
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), x| (lo.min(x), hi.max(x))))
}

/// Zaman → saniye çarpanı.
pub fn time_scale(u: TimeUnit, raw: &[f64]) -> f64 {
    match u {
        TimeUnit::Micro => 1e-6,
        TimeUnit::Milli => 1e-3,
        TimeUnit::Sec => 1.0,
        TimeUnit::Auto => {
            let span = finite_minmax(raw).map(|(a, b)| b - a).unwrap_or(0.0);
            if span > 1e8 {
                1e-6
            } else if span > 1e5 {
                1e-3
            } else {
                1.0
            }
        }
    }
}

pub fn volt_scale(u: VoltUnit, raw: &[f64]) -> f64 {
    match u {
        VoltUnit::Volt => 1.0,
        VoltUnit::MilliVolt => 1e-3,
        VoltUnit::CentiVolt => 1e-2,
        VoltUnit::Auto => {
            let mx = finite_minmax(raw).map(|x| x.1).unwrap_or(0.0);
            if mx <= 200.0 {
                1.0
            } else if mx < 8000.0 {
                1e-2
            } else {
                1e-3
            }
        }
    }
}

pub fn cur_scale(u: CurUnit, raw: &[f64], name: &str) -> f64 {
    match u {
        CurUnit::Amp => 1.0,
        CurUnit::MilliAmp => 1e-3,
        CurUnit::CentiAmp => 1e-2,
        CurUnit::Auto => {
            let mx = finite_minmax(raw)
                .map(|x| x.1.max(x.0.abs()))
                .unwrap_or(0.0);
            if mx <= 300.0 {
                1.0
            } else if name.to_lowercase().contains("amperage") {
                1e-2
            } else {
                1e-3
            }
        }
    }
}

pub fn speed_scale(u: SpeedUnit) -> f64 {
    match u {
        SpeedUnit::Auto | SpeedUnit::MetersPerSec => 1.0,
        SpeedUnit::KmPerHour => 1.0 / 3.6,
        SpeedUnit::CmPerSec => 0.01,
    }
}

pub fn alt_scale(u: AltUnit) -> f64 {
    match u {
        AltUnit::Auto | AltUnit::Meter => 1.0,
        AltUnit::Centimeter => 0.01,
        AltUnit::Millimeter => 0.001,
    }
}

/// Gaz sütununu yüzdeye çevirir: ≤1 → ×100; ≤100 → olduğu gibi; 1000–2000 µs PWM → (x−1000)/10;
/// aksi (DShot ham, 0–2047) → x/20.47.
pub fn throttle_to_percent(raw: &mut [f64]) {
    let Some((lo, hi)) = finite_minmax(raw) else {
        return;
    };
    let f: Box<dyn Fn(f64) -> f64> = if hi <= 1.0 + 1e-9 && lo >= -1e-9 {
        Box::new(|x| x * 100.0)
    } else if hi <= 100.0 + 1e-9 {
        Box::new(|x| x)
    } else if lo >= 900.0 && hi <= 2100.0 {
        Box::new(|x| (x - 1000.0) / 10.0)
    } else {
        Box::new(|x| x / 20.47)
    };
    for x in raw.iter_mut() {
        if x.is_finite() {
            *x = f(*x).clamp(0.0, 100.0);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Akış okuma
// ---------------------------------------------------------------------------------------------

/// CSV'yi satır satır okur, yalnızca eşlenen sütunları tutar (bellek tasarrufu), birimleri çevirir.
pub fn read_log<R: BufRead>(mut r: R, map: &Mapping, max_rows: usize) -> Result<Log, LogError> {
    let (headers, delim) = read_header(&mut r)?;
    let decimal_comma = delim == ';';
    let fields = Field::ALL;
    // Ham sütun vektörleri (zaman dahil), eşlenmeyenler boş kalır.
    let mut raw: Vec<Vec<f64>> = vec![Vec::new(); fields.len()];
    let idx: Vec<Option<usize>> = fields
        .iter()
        .map(|f| map.get(*f).filter(|&i| i < headers.len()))
        .collect();
    let max_idx = idx.iter().flatten().copied().max();
    let mut log = Log::default();
    let mut raw_t: Vec<f64> = Vec::new();
    let mut line = String::new();
    let mut row_no = 0usize;
    loop {
        line.clear();
        let n = match r.read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                // Geçersiz UTF-8 vb.: o satırı bozuk say, devam etmeyi dene.
                if e.kind() == std::io::ErrorKind::InvalidData {
                    log.total_rows += 1;
                    log.bad_rows += 1;
                    continue;
                }
                return Err(LogError::Io(e.to_string()));
            }
        };
        if n == 0 {
            break;
        }
        let l = line.trim_end_matches(['\r', '\n']);
        if l.trim().is_empty() {
            continue;
        }
        if log.total_rows >= max_rows {
            log.truncated = true;
            break;
        }
        log.total_rows += 1;
        let cells = split_fields(l, delim);
        if let Some(mx) = max_idx {
            if cells.len() <= mx && idx[0].is_some_and(|ti| cells.len() <= ti) {
                log.bad_rows += 1;
                continue;
            }
        }
        // Zaman
        let tv = match idx[0] {
            Some(ti) => cells
                .get(ti)
                .map(|c| parse_num(c, decimal_comma))
                .unwrap_or(f64::NAN),
            None => {
                log.time_assumed = true;
                row_no as f64 * 0.1
            }
        };
        if !tv.is_finite() || raw_t.last().is_some_and(|&p| tv <= p) {
            log.bad_rows += 1;
            continue;
        }
        row_no += 1;
        raw_t.push(tv);
        for (k, ix) in idx.iter().enumerate().skip(1) {
            if let Some(ci) = ix {
                let v = cells
                    .get(*ci)
                    .map(|c| parse_num(c, decimal_comma))
                    .unwrap_or(f64::NAN);
                raw[k].push(v);
            }
        }
    }
    if raw_t.is_empty() {
        return Err(if log.total_rows == 0 {
            LogError::Empty
        } else {
            LogError::NoValidRows
        });
    }
    let ts = if log.time_assumed {
        1.0
    } else {
        time_scale(map.time_unit, &raw_t)
    };
    let t0 = raw_t[0];
    log.t = raw_t.iter().map(|x| (x - t0) * ts).collect();
    let name_of = |f: Field| {
        map.get(f)
            .and_then(|i| headers.get(i))
            .cloned()
            .unwrap_or_default()
    };
    let scaled = |v: &mut Vec<f64>, s: f64| {
        if s != 1.0 {
            v.iter_mut().for_each(|x| *x *= s);
        }
    };
    let mut take = |k: usize| std::mem::take(&mut raw[k]);
    let mut volt = take(1);
    let mut cur = take(2);
    let mut thr = take(3);
    let mut alt = take(4);
    let mut gs = take(5);
    let mut asp = take(6);
    let rpm = take(7);
    let vs = volt_scale(map.volt_unit, &volt);
    scaled(&mut volt, vs);
    let cs = cur_scale(map.cur_unit, &cur, &name_of(Field::Current));
    scaled(&mut cur, cs);
    throttle_to_percent(&mut thr);
    scaled(&mut alt, alt_scale(map.alt_unit));
    scaled(&mut gs, speed_scale(map.speed_unit));
    scaled(&mut asp, speed_scale(map.speed_unit));
    log.volt = volt;
    log.cur = cur;
    log.thr = thr;
    log.alt = alt;
    log.gs = gs;
    log.asp = asp;
    log.rpm = rpm;
    Ok(log)
}

/// Bellekteki metinden okur (testler/örnekler için).
pub fn read_log_str(s: &str, map: &Mapping) -> Result<Log, LogError> {
    read_log(std::io::Cursor::new(s.as_bytes()), map, MAX_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn maps_betaflight_headers() {
        let h = hs(&[
            "loopIteration",
            "time (us)",
            "axisP[0]",
            "rcCommand[3]",
            "vbatLatest (V)",
            "amperageLatest (A)",
            "motor[0]",
            "GPS_speed (m/s)",
            "BaroAlt (cm)",
            "eRPM[0]",
        ]);
        let m = guess_mapping(&h);
        assert_eq!(m.time, Some(1));
        assert_eq!(m.voltage, Some(4));
        assert_eq!(m.current, Some(5));
        assert_eq!(m.throttle, Some(3));
        assert_eq!(m.ground_speed, Some(7));
        assert_eq!(m.altitude, Some(8));
        assert_eq!(m.rpm, Some(9));
        assert_eq!(m.time_unit, TimeUnit::Micro);
        assert_eq!(m.volt_unit, VoltUnit::Volt);
        assert_eq!(m.cur_unit, CurUnit::Amp);
        assert_eq!(m.alt_unit, AltUnit::Centimeter);
        assert_eq!(m.speed_unit, SpeedUnit::MetersPerSec);
    }

    #[test]
    fn maps_betaflight_motor0_when_no_rccommand() {
        let h = hs(&["time (us)", "vbatLatest (V)", "motor[0]"]);
        let m = guess_mapping(&h);
        assert_eq!(m.throttle, Some(2));
    }

    #[test]
    fn maps_ardupilot_headers() {
        let h = hs(&[
            "TimeUS",
            "BAT.Volt",
            "BAT.Curr",
            "GPS.Spd",
            "CTUN.Alt",
            "ARSP.Airspeed",
            "CTUN.ThO",
            "RPM.RPM",
        ]);
        let m = guess_mapping(&h);
        assert_eq!(m.time, Some(0));
        assert_eq!(m.voltage, Some(1));
        assert_eq!(m.current, Some(2));
        assert_eq!(m.ground_speed, Some(3));
        assert_eq!(m.altitude, Some(4));
        assert_eq!(m.air_speed, Some(5));
        assert_eq!(m.throttle, Some(6));
        assert_eq!(m.rpm, Some(7));
        assert_eq!(m.time_unit, TimeUnit::Micro);
        assert_eq!(m.alt_unit, AltUnit::Meter);
    }

    #[test]
    fn maps_px4_headers() {
        let h = hs(&[
            "timestamp",
            "voltage_v",
            "current_a",
            "discharged_mah",
            "vel_m_s",
            "baro_alt_meter",
        ]);
        let m = guess_mapping(&h);
        assert_eq!(m.time, Some(0));
        assert_eq!(m.voltage, Some(1));
        assert_eq!(m.current, Some(2));
        assert_eq!(m.ground_speed, Some(4));
        assert_eq!(m.altitude, Some(5));
        assert_eq!(m.time_unit, TimeUnit::Micro);
        assert_eq!(m.volt_unit, VoltUnit::Volt);
    }

    #[test]
    fn unit_conversions() {
        // mV, mA, ms
        let csv = "time (ms),vbat (mV),curr (mA)\n0,12000,1000\n1000,11900,2000\n2000,11800,3000\n";
        let m = guess_mapping(&read_headers(std::io::Cursor::new(csv)).unwrap());
        assert_eq!(m.time_unit, TimeUnit::Milli);
        assert_eq!(m.volt_unit, VoltUnit::MilliVolt);
        assert_eq!(m.cur_unit, CurUnit::MilliAmp);
        let log = read_log_str(csv, &m).unwrap();
        assert!((log.t[2] - 2.0).abs() < 1e-9);
        assert!((log.volt[0] - 12.0).abs() < 1e-9);
        assert!((log.cur[1] - 2.0).abs() < 1e-9);
        // km/h ve cm → m
        let csv2 = "time (s),gps_speed (km/h),baroalt (cm)\n0,36,100\n1,72,200\n";
        let m2 = guess_mapping(&hs(&["time (s)", "gps_speed (km/h)", "baroalt (cm)"]));
        let l2 = read_log_str(csv2, &m2).unwrap();
        assert!((l2.gs[0] - 10.0).abs() < 1e-9);
        assert!((l2.alt[1] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn auto_units_from_data() {
        // Birimsiz başlıklar: µs süre, cV gerilim, cA akım (Betaflight ham).
        let csv =
            "time,vbatlatest,amperagelatest\n0,1260,1000\n1000000,1250,2000\n2000000,1240,1500\n";
        let m = guess_mapping(&hs(&["time", "vbatlatest", "amperagelatest"]));
        let log = read_log_str(csv, &m).unwrap();
        assert!((log.volt[0] - 12.6).abs() < 1e-9, "{}", log.volt[0]);
        assert!((log.cur[1] - 20.0).abs() < 1e-9, "{}", log.cur[1]);
        assert!(log.t[2] > 0.0);
    }

    #[test]
    fn bad_rows_skipped_and_counted() {
        let csv = "t,v,i\n0,12,1\n1,12,1\nabc,12,1\n2,,3\n1.5,12,1\n3,12,x\n";
        let m = guess_mapping(&hs(&["time (s)", "vbat (V)", "amperage (A)"]));
        let log = read_log_str(csv, &m).unwrap();
        // "abc" zaman bozuk, "1.5" geri giden zaman bozuk.
        assert_eq!(log.bad_rows, 2);
        assert_eq!(log.total_rows, 6);
        assert_eq!(log.len(), 4);
        assert!(log.volt[2].is_nan());
        assert!(log.cur[3].is_nan());
    }

    #[test]
    fn row_cap_truncates() {
        let mut csv = String::from("t,v,i\n");
        for k in 0..50 {
            csv.push_str(&format!("{k},12,1\n"));
        }
        let m = guess_mapping(&hs(&["t", "v", "i"]));
        let log = read_log(std::io::Cursor::new(csv.as_bytes()), &m, 10).unwrap();
        assert!(log.truncated);
        assert_eq!(log.len(), 10);
    }

    #[test]
    fn empty_and_garbage_do_not_panic() {
        let m = Mapping::default();
        assert_eq!(read_log_str("", &m).unwrap_err(), LogError::Empty);
        assert_eq!(read_log_str("\n\n  \n", &m).unwrap_err(), LogError::Empty);
        let g = "\u{0}\u{1}garbage,,,\n#@!$%\n1,2\n,,,\n";
        let _ = read_log_str(g, &guess_mapping(&hs(&["a", "b"])));
        let m2 = guess_mapping(&hs(&["t", "v"]));
        let r = read_log_str("t,v\nx,y\nz,w\n", &m2);
        assert_eq!(r.unwrap_err(), LogError::NoValidRows);
        // Başlık yalnız; ikili benzeri içerik.
        let bin = String::from_utf8_lossy(&[0xff, 0xfe, 0x00, 0x01, b'\n', 0x80, 0x81]).to_string();
        let _ = read_log_str(&bin, &m2);
    }

    #[test]
    fn semicolon_and_decimal_comma() {
        let csv = "Time (s);BAT.Volt;BAT.Curr\n0;12,5;1,5\n1;12,4;2,5\n";
        let h = read_headers(std::io::Cursor::new(csv)).unwrap();
        assert_eq!(h.len(), 3);
        let m = guess_mapping(&h);
        let log = read_log_str(csv, &m).unwrap();
        assert!((log.volt[0] - 12.5).abs() < 1e-9);
        assert!((log.cur[1] - 2.5).abs() < 1e-9);
    }

    #[test]
    fn missing_columns_ok_and_time_assumed() {
        let csv = "vbat\n12\n11.9\n11.8\n";
        let m = guess_mapping(&hs(&["vbat"]));
        let log = read_log_str(csv, &m).unwrap();
        assert!(log.time_assumed);
        assert!(log.cur.is_empty());
        assert!(!log.has_power());
        assert!((log.t[1] - 0.1).abs() < 1e-9);
    }

    #[test]
    fn throttle_normalisation() {
        let mut a = vec![0.0, 0.5, 1.0];
        throttle_to_percent(&mut a);
        assert!((a[1] - 50.0).abs() < 1e-9);
        let mut b = vec![1000.0, 1500.0, 2000.0];
        throttle_to_percent(&mut b);
        assert!((b[1] - 50.0).abs() < 1e-9);
        let mut c = vec![48.0, 1023.5, 2047.0];
        throttle_to_percent(&mut c);
        assert!((c[2] - 100.0).abs() < 1e-6);
    }
}
