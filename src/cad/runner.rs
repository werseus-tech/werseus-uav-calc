// SPDX-License-Identifier: AGPL-3.0-or-later
//! FreeCADCmd calistirici, otomatik tespit mekanizmasi ve JSON sonuc ayristirici.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

/// FreeCAD calistirma sonucu.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CadExecutionResult {
    #[serde(default)]
    pub ok: Option<bool>,
    pub status: String,
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub volume_mm3: f64,
    #[serde(default)]
    pub param_update_ok: bool,
    #[serde(default)]
    pub num_segments: usize,
    #[serde(default)]
    pub fcstd_path: String,
    #[serde(default)]
    pub step_files: Vec<String>,
    #[serde(default)]
    pub stl_files: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub traceback: Option<String>,
    /// Montaj: toplam kütle [kg] (dolu katı hacimlerden).
    #[serde(default)]
    pub mass_kg: f64,
    /// Montaj: ağırlık merkezi [mm].
    #[serde(default)]
    pub cg_mm: Vec<f64>,
    /// Montaj: birbirine yakın parça çiftlerinin ölçümleri (sınıflandırma Rust tarafında).
    #[serde(default)]
    pub pairs: Vec<PairMetric>,
}

/// İki parçanın hacim kesişimi (çakışma).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Interference {
    pub a: String,
    pub b: String,
    pub volume_mm3: f64,
}

/// Montajdaki bir çiftin ham ölçümü: ortak hacim (ölçülemediyse < 0), ortak hacmin yüzey alanı,
/// en kısa mesafe (ölçülemediyse < 0).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PairMetric {
    pub a: String,
    pub b: String,
    #[serde(default)]
    pub volume_mm3: f64,
    #[serde(default)]
    pub area_mm2: f64,
    #[serde(default)]
    pub dist_mm: f64,
    #[serde(default)]
    pub error: Option<String>,
    /// Örnekleme ölçümü: iki parçanın da içinde ve yüzeylerden derinde bulunan nokta sayısı.
    #[serde(default)]
    pub probe_hits: u32,
    /// Örnekleme ölçümünde bulunan en büyük girişim derinliği [mm].
    #[serde(default)]
    pub probe_depth_mm: f64,
    /// Tam ortak hacmin örneklemeyle doğrulanan kısmı [mm³].
    #[serde(default)]
    pub verified_volume_mm3: f64,
}

/// Arka plan is parcacigindan arayüze gonderilen ilerleme mesajlari.
#[derive(Clone, Debug)]
pub enum CadRunnerMessage {
    Log(String),
    Success(Box<CadExecutionResult>),
    Error(String),
}

/// FreeCADCmd ve GUI calistirilabilir dosya yollari.
#[derive(Clone, Debug)]
pub struct FreeCadInstallation {
    pub cmd_path: PathBuf,
    pub gui_path: Option<PathBuf>,
    pub version: String,
}

/// Windows'ta konsol penceresi acilmasini onleyen CREATE_NO_WINDOW bayragi (0x08000000).
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// std::process::Command icin Windows'ta konsol penceresi acilmasini onleyen sarmalayici.
pub fn new_command<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Global FreeCAD algılama önbelleği: (yapılandırılmış yol dizesi, algılanan kurulum).
static CACHED_FREECAD: std::sync::Mutex<Option<(Option<String>, Option<FreeCadInstallation>)>> =
    std::sync::Mutex::new(None);

/// Önbelleği temizler (testler ve zorunlu yeniden tarama için).
pub fn clear_freecad_cache() {
    if let Ok(mut lock) = CACHED_FREECAD.lock() {
        *lock = None;
    }
}

/// FreeCADCmd yolunu verilen sürüm çalıştırıcısı ile arar (süreç başlatıcı soyutlaması).
pub fn detect_freecad_with_runner<F>(
    configured_path: Option<&str>,
    mut version_runner: F,
) -> Option<FreeCadInstallation>
where
    F: FnMut(&Path) -> Option<String>,
{
    // 1. Ayarlarda acikca belirtilmis yol
    if let Some(path_str) = configured_path {
        let p = PathBuf::from(path_str.trim());
        if p.is_file() {
            let version = version_runner(&p).unwrap_or_else(|| "FreeCAD (bilinmeyen sürüm)".into());
            let gui_path = find_freecad_gui(&p);
            return Some(FreeCadInstallation {
                cmd_path: p,
                gui_path,
                version,
            });
        }
    }

    // 2. Program Files altinda ara: C:\Program Files\FreeCAD *\bin\FreeCADCmd.exe
    #[cfg(windows)]
    {
        let prog_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        if let Some(pf) = prog_files {
            if let Ok(entries) = std::fs::read_dir(&pf) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("freecad") {
                        let candidate = entry.path().join("bin").join("FreeCADCmd.exe");
                        if candidate.is_file() {
                            let version =
                                version_runner(&candidate).unwrap_or_else(|| "FreeCAD 1.1+".into());
                            let gui_path = find_freecad_gui(&candidate);
                            return Some(FreeCadInstallation {
                                cmd_path: candidate,
                                gui_path,
                                version,
                            });
                        }
                    }
                }
            }
        }

        // 3. %LOCALAPPDATA%\Programs\FreeCAD *\bin\FreeCADCmd.exe
        let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        if let Some(lad) = local_app_data {
            let programs = lad.join("Programs");
            if let Ok(entries) = std::fs::read_dir(&programs) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("freecad") {
                        let candidate = entry.path().join("bin").join("FreeCADCmd.exe");
                        if candidate.is_file() {
                            let version = version_runner(&candidate)
                                .unwrap_or_else(|| "FreeCAD (LocalAppData)".into());
                            let gui_path = find_freecad_gui(&candidate);
                            return Some(FreeCadInstallation {
                                cmd_path: candidate,
                                gui_path,
                                version,
                            });
                        }
                    }
                }
            }
        }
    }

    // 4. PATH cevre degiskeninde ara
    if let Ok(path_var) = std::env::var("PATH") {
        #[cfg(windows)]
        let separator = ';';
        #[cfg(not(windows))]
        let separator = ':';

        for dir in path_var.split(separator) {
            let dir_path = PathBuf::from(dir);
            for name in [
                "FreeCADCmd.exe",
                "FreeCADCmd",
                "freecadcmd.exe",
                "freecadcmd",
            ] {
                let candidate = dir_path.join(name);
                if candidate.is_file() {
                    let version =
                        version_runner(&candidate).unwrap_or_else(|| "FreeCAD (PATH)".into());
                    let gui_path = find_freecad_gui(&candidate);
                    return Some(FreeCadInstallation {
                        cmd_path: candidate,
                        gui_path,
                        version,
                    });
                }
            }
        }
    }

    None
}

/// FreeCADCmd yolunu önbellekli arar; `rescan` false ise ve önbellek geçerliyse süreç çalıştırmaz.
pub fn detect_freecad_cached_with_runner<F>(
    configured_path: Option<&str>,
    rescan: bool,
    version_runner: F,
) -> Option<FreeCadInstallation>
where
    F: FnMut(&Path) -> Option<String>,
{
    let norm_path = configured_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if !rescan {
        if let Ok(lock) = CACHED_FREECAD.lock() {
            if let Some((ref cached_norm, ref cached_res)) = *lock {
                if *cached_norm == norm_path {
                    return cached_res.clone();
                }
            }
        }
    }

    let detected = detect_freecad_with_runner(norm_path.as_deref(), version_runner);

    if let Ok(mut lock) = CACHED_FREECAD.lock() {
        *lock = Some((norm_path, detected.clone()));
    }

    detected
}

/// FreeCADCmd kurulumunu önbellek desteğiyle bulur (varsayılan: önbellekten oku veya ilk kez ara).
pub fn detect_freecad_cached(
    configured_path: Option<&str>,
    rescan: bool,
) -> Option<FreeCadInstallation> {
    detect_freecad_cached_with_runner(configured_path, rescan, get_freecad_version)
}

/// FreeCADCmd yolunu otomatik veya ayarlara gore bulur (önbellekli).
pub fn detect_freecad(configured_path: Option<&str>) -> Option<FreeCadInstallation> {
    detect_freecad_cached(configured_path, false)
}

/// UI kare döngüsünü kilitlemeden FreeCAD algılamasını arka plan iş parçacığında çalıştırır.
/// Arka plan işlemi bittiğinde arayüzün anında güncellenmesi için opsiyonel `egui::Context`
/// alır ve sonuç gönderildikten sonra `request_repaint()` çağırır.
pub fn detect_freecad_async(
    configured_path: Option<String>,
    ctx: Option<egui::Context>,
) -> Receiver<Option<FreeCadInstallation>> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        let res = detect_freecad_cached(configured_path.as_deref(), true);
        let _ = tx.send(res);
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
    rx
}

/// FreeCADCmd surum dizesini okur (--version, CREATE_NO_WINDOW ile gizli).
pub fn get_freecad_version(cmd_path: &Path) -> Option<String> {
    let output = new_command(cmd_path).arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed_err = stderr.trim();
    if !trimmed_err.is_empty() {
        return Some(trimmed_err.to_string());
    }
    None
}

/// FreeCADCmd ile ayni dizindeki FreeCAD.exe (GUI) ikilisini bulur.
pub fn find_freecad_gui(cmd_path: &Path) -> Option<PathBuf> {
    let gui_names = ["FreeCAD.exe", "FreeCAD", "freecad.exe", "freecad"];
    for name in gui_names {
        let p = cmd_path.with_file_name(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

static LAUNCHER_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Windows 8.3 kisa dosya yolunu alir (ASCII olmayan kullanici profilleri / dizinler icin).
#[cfg(windows)]
pub fn get_short_path_name(path: &Path) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe extern "system" {
        fn GetShortPathNameW(
            lpszLongPath: *const u16,
            lpszShortPath: *mut u16,
            cchBuffer: u32,
        ) -> u32;
    }

    unsafe {
        let req_size = GetShortPathNameW(wide_path.as_ptr(), std::ptr::null_mut(), 0);
        if req_size == 0 {
            return None;
        }
        let mut buffer: Vec<u16> = vec![0; req_size as usize];
        let res = GetShortPathNameW(wide_path.as_ptr(), buffer.as_mut_ptr(), req_size);
        if res == 0 || res >= req_size {
            return None;
        }
        let short_slice = &buffer[..res as usize];
        Some(PathBuf::from(OsString::from_wide(short_slice)))
    }
}

/// Verilen gecici dizinden ASCII-only calisma dizini secer; ASCII yol elde edilemezse hata verir.
pub fn choose_ascii_cad_dir(temp: &Path, short: Option<PathBuf>) -> Result<PathBuf, String> {
    let is_ascii = |p: &Path| p.to_string_lossy().is_ascii();
    if is_ascii(temp) {
        return Ok(temp.join("werseus_cad"));
    }
    if let Some(sp) = short {
        if is_ascii(&sp) {
            return Ok(sp.join("werseus_cad"));
        }
    }
    Err(format!(
        "ASCII karakterli gecici CAD dizini elde edilemedi (gecici dizin: {}).          FreeCADCmd ASCII olmayan yollari acamaz; lutfen TEMP ortam degiskenini ASCII bir klasore          (ornegin C:/Temp) yonlendirin. / No ASCII-only temp directory is available;          set the TEMP environment variable to an ASCII-only folder such as C:/Temp.",
        temp.display()
    ))
}

/// FreeCADCmd icin her zaman ASCII karakterli gecici dizin dondurur (alinamazsa hata).
pub fn get_ascii_temp_cad_dir() -> Result<PathBuf, String> {
    let temp = std::env::temp_dir();
    #[cfg(windows)]
    let short = get_short_path_name(&temp);
    #[cfg(not(windows))]
    let short: Option<PathBuf> = None;
    choose_ascii_cad_dir(&temp, short)
}

/// FreeCADCmd Windows'ta komut satirindan ASCII olmayan dosya yollarini acamaz ([Unknown file]).
/// Bu nedenle gercek betigi UTF-8 olarak okuyup calistiran bir ASCII baslatici betik uretilir.
pub fn create_launcher_script(target_script: &Path) -> Result<PathBuf, String> {
    let dir = get_ascii_temp_cad_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Gecici CAD dizini olusturulamadi: {}", e))?;

    let count = LAUNCHER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let pid = std::process::id();
    let launcher_name = format!("run_{}_{}.py", pid, count);
    let launcher_path = dir.join(launcher_name);

    let target_str = target_script.to_string_lossy().replace('\\', "/");
    let target_py_literal =
        serde_json::to_string(&target_str).unwrap_or_else(|_| format!("\"{}\"", target_str));

    let content = format!(
        r#"# -*- coding: utf-8 -*-
# FreeCAD ASCII Baslatici Betigi - WerSeuS UAV Calc
import sys, os, traceback, json

target_script = {target_py_literal}

try:
    with open(target_script, "r", encoding="utf-8") as f:
        code = f.read()
    exec(compile(code, target_script, "exec"), globals())
except Exception as e:
    tb = traceback.format_exc()
    err_json = {{
        "ok": False,
        "status": "error",
        "part": "unknown",
        "valid": False,
        "volume_mm3": 0.0,
        "param_update_ok": False,
        "num_segments": 0,
        "fcstd_path": "",
        "step_files": [],
        "stl_files": [],
        "error": str(e),
        "traceback": tb,
    }}
    print("__WERSEUS_CAD_JSON_START__" + json.dumps(err_json) + "__WERSEUS_CAD_JSON_END__")
    sys.exit(1)
"#
    );

    std::fs::write(&launcher_path, content)
        .map_err(|e| format!("Baslatici betik yazilamadi: {}", e))?;

    Ok(launcher_path)
}

/// Süreç çıktısından (stdout, yoksa stderr) FreeCAD JSON sonucunu çıkarır; hata sonuçlarını
/// ve JSON üretilemeyen durumu `Err` olarak döndürür.
pub fn interpret_cad_output(stdout: &str, stderr: &str) -> Result<CadExecutionResult, String> {
    let res = parse_cad_json_output(stdout).or_else(|| parse_cad_json_output(stderr));

    match res {
        Some(cad_res) => {
            if cad_res.status == "error"
                || cad_res.ok == Some(false)
                || (!cad_res.valid && cad_res.volume_mm3 <= 0.0)
            {
                let err_msg = cad_res
                    .error
                    .unwrap_or_else(|| "Geometrik kati uretilemedi (invalid shape)".into());
                if let Some(tb) = cad_res.traceback {
                    Err(format!(
                        "FreeCAD Hatasi: {}\n\nPython Traceback:\n{}",
                        err_msg, tb
                    ))
                } else {
                    Err(format!("FreeCAD Hatasi: {}", err_msg))
                }
            } else {
                Ok(cad_res)
            }
        }
        None => Err(format!(
            "FreeCAD JSON ciktisi uretilemedi.\n--- stdout ---\n{}\n--- stderr ---\n{}",
            stdout, stderr
        )),
    }
}

/// FreeCAD betigini calistirip stdout'taki JSON sonucunu ayristirir (senkron).
pub fn run_freecad_script_sync(
    cmd_path: &Path,
    script_path: &Path,
) -> Result<CadExecutionResult, String> {
    let launcher_path = create_launcher_script(script_path)?;

    let output_res = new_command(cmd_path).arg(&launcher_path).output();

    let _ = std::fs::remove_file(&launcher_path);

    let output = output_res.map_err(|e| format!("FreeCADCmd baslatilamadi: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    interpret_cad_output(&stdout, &stderr)
}

/// FreeCAD betigini UI'yi kilitlemeden arka planda calistirir.
pub fn run_freecad_script_async(
    cmd_path: PathBuf,
    script_path: PathBuf,
) -> Receiver<CadRunnerMessage> {
    let (tx, rx): (Sender<CadRunnerMessage>, Receiver<CadRunnerMessage>) = channel();

    thread::spawn(move || {
        let _ = tx.send(CadRunnerMessage::Log(format!(
            "FreeCADCmd baslatiliyor: {}",
            cmd_path.display()
        )));
        let _ = tx.send(CadRunnerMessage::Log(format!(
            "Betik: {}",
            script_path.display()
        )));

        match run_freecad_script_sync(&cmd_path, &script_path) {
            Ok(res) => {
                let _ = tx.send(CadRunnerMessage::Log(format!(
                    "Basari: Parca: {}, Hacim: {:.2} mm3, Gecerli: {}",
                    res.part, res.volume_mm3, res.valid
                )));
                let _ = tx.send(CadRunnerMessage::Success(Box::new(res)));
            }
            Err(e) => {
                let _ = tx.send(CadRunnerMessage::Log(format!("HATA: {}", e)));
                let _ = tx.send(CadRunnerMessage::Error(e));
            }
        }
    });

    rx
}

// ---------------------------------------------------------------------------------------------
// Iptal edilebilir, zaman asimli calistirma
// ---------------------------------------------------------------------------------------------

/// CAD uretimi sure sinirlari [saniye]. Ayarlardan degistirilebilir (`AppSettings.cad_limits`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CadLimits {
    /// Parca basina sinir (varsayilan 5 dk).
    pub part_secs: u64,
    /// Montaj + cakisma kontrolu siniri (varsayilan 15 dk).
    pub assembly_secs: u64,
    /// Tum uretimin toplam siniri (varsayilan 60 dk).
    pub total_secs: u64,
}

impl Default for CadLimits {
    fn default() -> Self {
        Self {
            part_secs: 300,
            assembly_secs: 900,
            total_secs: 3600,
        }
    }
}

impl CadLimits {
    /// En az 1 dk, en cok 12 sa (bozuk ayar dosyasi sureci hemen oldurmesin / sonsuza uzatmasin).
    pub const MIN_SECS: u64 = 60;
    pub const MAX_SECS: u64 = 12 * 3600;

    pub fn clamped(self) -> Self {
        let c = |v: u64| v.clamp(Self::MIN_SECS, Self::MAX_SECS);
        Self {
            part_secs: c(self.part_secs),
            assembly_secs: c(self.assembly_secs),
            total_secs: c(self.total_secs),
        }
    }
}

/// Paylasilabilir iptal bayragi (arayuz iptal ister, arka plan is parcacigi sureci sonlandirir).
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Betigin stdout'a yazdigi asama satirlarinin on eki: `__WERSEUS_STAGE__:<kimlik>[:<n>/<toplam>]`.
pub const STAGE_PREFIX: &str = "__WERSEUS_STAGE__:";

/// Betikten gelen bir asama bildirimi. Kimlik yalniz `[a-z0-9_]` (en cok 40 karakter) olabilir;
/// baska bir sey asama satiri sayilmaz (betik ciktisi kullaniciya hic dogrudan gosterilmez).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageInfo {
    pub id: String,
    pub done: Option<u32>,
    pub total: Option<u32>,
}

/// Bir stdout satirini asama bildirimi olarak ayristirir.
pub fn parse_stage_line(line: &str) -> Option<StageInfo> {
    let rest = line.trim().strip_prefix(STAGE_PREFIX)?;
    let mut it = rest.splitn(2, ':');
    let id = it.next()?;
    if id.is_empty()
        || id.len() > 40
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let (mut done, mut total) = (None, None);
    if let Some(frac) = it.next() {
        let mut f = frac.splitn(2, '/');
        done = f.next().and_then(|v| v.trim().parse::<u32>().ok());
        total = f.next().and_then(|v| v.trim().parse::<u32>().ok());
        if total.is_none() {
            done = None;
        }
    }
    Some(StageInfo {
        id: id.to_string(),
        done,
        total,
    })
}

/// Kontrollu calistirmanin sonucu.
#[derive(Debug)]
pub enum ProcessEnd {
    Finished {
        exit_ok: bool,
        stdout: String,
        stderr: String,
    },
    Cancelled,
    TimedOut,
    SpawnFailed(String),
}

/// Surec ciktisi icin sinirli birikim: en cok `MAX_OUTPUT_LINES` satir / ~`MAX_OUTPUT_BYTES` bayt
/// tutulur, eskiler atilir (hata iletileri icin son satirlar korunur). Her zaman en az son satir
/// tutulur.
pub const MAX_OUTPUT_LINES: usize = 2000;
pub const MAX_OUTPUT_BYTES: usize = 1_000_000;

#[derive(Debug, Default)]
pub struct TailBuffer {
    lines: std::collections::VecDeque<String>,
    bytes: usize,
    dropped: usize,
}

impl TailBuffer {
    pub fn push(&mut self, line: &str) {
        self.bytes += line.len() + 1;
        self.lines.push_back(line.to_string());
        while self.lines.len() > 1
            && (self.lines.len() > MAX_OUTPUT_LINES || self.bytes > MAX_OUTPUT_BYTES)
        {
            if let Some(old) = self.lines.pop_front() {
                self.bytes -= old.len() + 1;
                self.dropped += 1;
            }
        }
    }

    /// Atilan satir sayisi.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    /// Tutulan satirlar, her biri satir sonu ile biter.
    pub fn into_text(self) -> String {
        let mut out = String::with_capacity(self.bytes);
        for l in self.lines {
            out.push_str(&l);
            out.push('\n');
        }
        out
    }
}

/// Sureci (ve Windows'ta alt sureclerini) sonlandirir; hatalar yutulur (surec zaten bitmis olabilir).
fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = new_command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_line_reader<R: std::io::Read + Send + 'static>(
    pipe: R,
    is_stdout: bool,
    tx: Sender<(bool, String)>,
) {
    thread::spawn(move || {
        use std::io::BufRead;
        let mut rd = std::io::BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match rd.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    if tx.send((is_stdout, line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Komutu arka planda calistirir; iptal istenirse veya sure dolarsa sureci sonlandirir.
/// stdout satirlari `on_line` ile (cagiran is parcacigi uzerinde) iletilir.
pub fn run_process_controlled(
    mut cmd: Command,
    timeout: std::time::Duration,
    cancel: &CancelFlag,
    mut on_line: impl FnMut(&str),
) -> ProcessEnd {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ProcessEnd::SpawnFailed(e.to_string()),
    };
    let (tx, rx) = channel::<(bool, String)>();
    if let Some(o) = child.stdout.take() {
        spawn_line_reader(o, true, tx.clone());
    }
    if let Some(e) = child.stderr.take() {
        spawn_line_reader(e, false, tx.clone());
    }
    drop(tx);

    let (mut out, mut err) = (TailBuffer::default(), TailBuffer::default());
    let mut take = |on_line: &mut dyn FnMut(&str), msg: (bool, String)| {
        let (is_out, line) = msg;
        if is_out {
            on_line(&line);
            out.push(&line);
        } else {
            err.push(&line);
        }
    };

    let start = Instant::now();
    let status = loop {
        while let Ok(msg) = rx.try_recv() {
            take(&mut on_line, msg);
        }
        if cancel.is_cancelled() {
            kill_process_tree(&mut child);
            return ProcessEnd::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {}
            Err(e) => {
                kill_process_tree(&mut child);
                return ProcessEnd::SpawnFailed(e.to_string());
            }
        }
        if start.elapsed() >= timeout {
            kill_process_tree(&mut child);
            return ProcessEnd::TimedOut;
        }
        thread::sleep(Duration::from_millis(30));
    };
    // Surec bitti: okuyucularin kalan satirlari bosaltmasini (en cok 5 sn) bekle.
    let drain_start = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => take(&mut on_line, msg),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if drain_start.elapsed() > Duration::from_secs(5) {
                    break;
                }
            }
        }
    }
    ProcessEnd::Finished {
        exit_ok: status.success(),
        stdout: out.into_text(),
        stderr: err.into_text(),
    }
}

/// Iptal edilebilir FreeCAD calistirmasinin hata turu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CadRunError {
    Cancelled,
    TimedOut,
    Failed(String),
}

/// FreeCAD betigini zaman asimi ve iptal destegiyle calistirir; betigin asama satirlarini
/// `on_stage` ile iletir. Sonuc yorumu `run_freecad_script_sync` ile aynidir.
pub fn run_freecad_script_controlled(
    cmd_path: &Path,
    script_path: &Path,
    timeout: std::time::Duration,
    cancel: &CancelFlag,
    mut on_stage: impl FnMut(StageInfo),
) -> Result<CadExecutionResult, CadRunError> {
    let launcher_path = create_launcher_script(script_path).map_err(CadRunError::Failed)?;
    let mut cmd = new_command(cmd_path);
    cmd.arg(&launcher_path);
    let end = run_process_controlled(cmd, timeout, cancel, |line| {
        if let Some(st) = parse_stage_line(line) {
            on_stage(st);
        }
    });
    let _ = std::fs::remove_file(&launcher_path);
    match end {
        ProcessEnd::Cancelled => Err(CadRunError::Cancelled),
        ProcessEnd::TimedOut => Err(CadRunError::TimedOut),
        ProcessEnd::SpawnFailed(e) => Err(CadRunError::Failed(format!(
            "FreeCADCmd baslatilamadi: {}",
            e
        ))),
        ProcessEnd::Finished { stdout, stderr, .. } => {
            interpret_cad_output(&stdout, &stderr).map_err(CadRunError::Failed)
        }
    }
}

/// Stdout veya stderr icindeki __WERSEUS_CAD_JSON_START__ ... __WERSEUS_CAD_JSON_END__ blogunu ayristirir.
pub fn parse_cad_json_output(output: &str) -> Option<CadExecutionResult> {
    const START_TAG: &str = "__WERSEUS_CAD_JSON_START__";
    const END_TAG: &str = "__WERSEUS_CAD_JSON_END__";

    let start_idx = output.find(START_TAG)?;
    let after_start = &output[start_idx + START_TAG.len()..];
    let end_idx = after_start.find(END_TAG)?;
    let json_str = &after_start[..end_idx].trim();

    serde_json::from_str::<CadExecutionResult>(json_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_dir_selection_and_failure() {
        let ok = choose_ascii_cad_dir(Path::new("C:/Temp"), None).unwrap();
        assert!(ok.to_string_lossy().is_ascii());
        let non_ascii = Path::new("C:/Users/\u{015f}ener/Temp");
        // ASCII olmayan temp + ASCII kisa yol -> kisa yol kullanilir
        let short =
            choose_ascii_cad_dir(non_ascii, Some(PathBuf::from("C:/Users/SENER~1/Temp"))).unwrap();
        assert!(short.to_string_lossy().contains("SENER~1"));
        // ASCII yol elde edilemezse acik hata (ASCII olmayan yola dusulmez)
        assert!(choose_ascii_cad_dir(non_ascii, None).is_err());
        assert!(choose_ascii_cad_dir(non_ascii, Some(non_ascii.to_path_buf())).is_err());
    }

    #[test]
    fn test_parse_cad_json_output() {
        let raw = r#"
Some log output from FreeCAD
Opening document...
__WERSEUS_CAD_JSON_START__{
    "ok": true,
    "status": "ok",
    "part": "TestWing",
    "valid": true,
    "volume_mm3": 123456.78,
    "param_update_ok": true,
    "num_segments": 3,
    "fcstd_path": "C:/out/TestWing.FCStd",
    "step_files": ["C:/out/TestWing.step"],
    "stl_files": ["C:/out/TestWing.stl"]
}__WERSEUS_CAD_JSON_END__
Final FreeCAD cleanup...
"#;
        let res = parse_cad_json_output(raw).expect("JSON ciktisi ayristirilamadi");
        assert_eq!(res.status, "ok");
        assert_eq!(res.ok, Some(true));
        assert_eq!(res.part, "TestWing");
        assert!(res.valid);
        assert!((res.volume_mm3 - 123456.78).abs() < 1e-2);
        assert!(res.param_update_ok);
        assert_eq!(res.num_segments, 3);
        assert_eq!(res.step_files.len(), 1);
    }

    #[test]
    fn test_parse_cad_json_error_with_traceback() {
        let raw_err = r#"
__WERSEUS_CAD_JSON_START__{
    "ok": false,
    "status": "error",
    "part": "Test_Plaka",
    "valid": false,
    "volume_mm3": 0.0,
    "param_update_ok": false,
    "num_segments": 0,
    "fcstd_path": "",
    "step_files": [],
    "stl_files": [],
    "error": "BRepCheck_Analyzer::Init() - NULL shape",
    "traceback": "Traceback (most recent call last):\n  File \"script.py\", line 42, in <module>\nValueError: NULL shape"
}__WERSEUS_CAD_JSON_END__
"#;
        let res = parse_cad_json_output(raw_err).expect("Hata JSON'i ayristirilamadi");
        assert_eq!(res.status, "error");
        assert_eq!(res.ok, Some(false));
        assert!(!res.valid);
        assert_eq!(
            res.error.as_deref(),
            Some("BRepCheck_Analyzer::Init() - NULL shape")
        );
        assert!(
            res.traceback
                .as_ref()
                .unwrap()
                .contains("ValueError: NULL shape")
        );
    }

    #[test]
    fn test_create_launcher_script() {
        let fake_target = PathBuf::from("target/cad_test/Türkçe Klasör/WerSeuS_Tasarım_1_Kanat.py");
        let launcher = create_launcher_script(&fake_target).expect("Başlatıcı oluşturulamadı");
        assert!(launcher.is_file(), "Başlatıcı diskte mevcut olmalı");

        let content = std::fs::read_to_string(&launcher).expect("Başlatıcı okunamadı");
        assert!(content.contains("# -*- coding: utf-8 -*-"));
        assert!(content.contains("target_script ="));
        assert!(
            content.contains("WerSeuS_Tasar\\u0131m_1_Kanat.py")
                || content.contains("WerSeuS_Tasarım_1_Kanat.py")
        );
        assert!(content.contains("__WERSEUS_CAD_JSON_START__"));

        let _ = std::fs::remove_file(launcher);
    }

    #[test]
    fn test_detection_caching_does_not_spawn_repeated_processes() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        clear_freecad_cache();

        let temp_dir =
            std::env::temp_dir().join(format!("werseus_fc_cache_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let fake_bin = temp_dir.join("FreeCADCmd.exe");
        let _ = std::fs::write(&fake_bin, b"fake binary");
        let fake_bin_str = fake_bin.to_string_lossy().to_string();

        let counter = Arc::new(AtomicUsize::new(0));
        let c_clone = counter.clone();

        // 1. İlk çağrı: önbellek boş, sahte çalıştırıcı 1 kez çağrılmalı
        let res1 = detect_freecad_cached_with_runner(Some(&fake_bin_str), false, |_path| {
            c_clone.fetch_add(1, Ordering::SeqCst);
            Some("FreeCAD 1.1.0-test".into())
        });
        assert!(res1.is_some(), "Sahte kurulum tespit edilmeli");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "İlk çağrıda çalıştırıcı 1 kez çalışmalı"
        );

        // 2. Ardışık 5 çağrı (UI kare döngüsü benzetimi):
        // rescan = false olduğundan çalıştırıcı ASLA tekrar çağrılmamalı (sayaç 1 kalmalı)
        for _ in 0..5 {
            let res = detect_freecad_cached_with_runner(Some(&fake_bin_str), false, |_path| {
                counter.fetch_add(1, Ordering::SeqCst);
                Some("FreeCAD 1.1.0-test".into())
            });
            assert!(res.is_some());
            assert_eq!(
                counter.load(Ordering::SeqCst),
                1,
                "Önbellekli çağrıda süreç başlatılmamalı!"
            );
        }

        // 3. Kullanıcı 'Yeniden tara' / 'rescan = true' yaptığında: çalıştırıcı tekrar çalışmalı
        let res_rescan = detect_freecad_cached_with_runner(Some(&fake_bin_str), true, |_path| {
            counter.fetch_add(1, Ordering::SeqCst);
            Some("FreeCAD 1.1.0-test".into())
        });
        assert!(res_rescan.is_some());
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "Zorunlu taramada çalıştırıcı 2. kez çalışmalı"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        clear_freecad_cache();
    }

    /// Uzun süren, zararsız sahte süreç (FreeCAD yerine): ~30 sn bekler.
    fn slow_fake_process() -> Command {
        #[cfg(windows)]
        {
            let mut c = new_command("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = new_command("sleep");
            c.arg("30");
            c
        }
    }

    fn echo_fake_process(text: &str) -> Command {
        #[cfg(windows)]
        {
            let mut c = new_command("cmd");
            c.args(["/C", "echo", text]);
            c
        }
        #[cfg(not(windows))]
        {
            let mut c = new_command("sh");
            c.args(["-c", &format!("echo {text}")]);
            c
        }
    }

    #[test]
    fn controlled_process_can_be_cancelled() {
        let cancel = CancelFlag::new();
        let c2 = cancel.clone();
        let t0 = std::time::Instant::now();
        let h = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(400));
            c2.cancel();
        });
        let end = run_process_controlled(
            slow_fake_process(),
            std::time::Duration::from_secs(60),
            &cancel,
            |_| {},
        );
        h.join().unwrap();
        assert!(matches!(end, ProcessEnd::Cancelled), "{end:?}");
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(15),
            "iptal süreci hızla sonlandırmalı"
        );
    }

    #[test]
    fn tail_buffer_keeps_only_last_lines() {
        let mut b = TailBuffer::default();
        for i in 0..(MAX_OUTPUT_LINES + 500) {
            b.push(&format!("satir {i}"));
        }
        assert_eq!(b.dropped(), 500);
        let t = b.into_text();
        assert_eq!(t.lines().count(), MAX_OUTPUT_LINES);
        assert!(t.starts_with(
            "satir 500
"
        ));
        assert!(t.ends_with(&format!(
            "satir {}
",
            MAX_OUTPUT_LINES + 499
        )));
    }

    #[test]
    fn tail_buffer_is_byte_bounded_and_keeps_last_line() {
        let mut b = TailBuffer::default();
        let big = "x".repeat(300_000);
        for _ in 0..10 {
            b.push(&big);
        }
        b.push("SON HATA");
        let t = b.into_text();
        assert!(t.len() <= MAX_OUTPUT_BYTES + 300_001, "{}", t.len());
        assert!(t.ends_with(
            "SON HATA
"
        ));
        // Tek dev satir bile tutulur (son satir hicbir zaman atilmaz).
        let mut c = TailBuffer::default();
        c.push(&"y".repeat(MAX_OUTPUT_BYTES * 2));
        assert_eq!(c.dropped(), 0);
    }

    #[test]
    fn controlled_process_times_out() {
        let t0 = std::time::Instant::now();
        let end = run_process_controlled(
            slow_fake_process(),
            std::time::Duration::from_millis(500),
            &CancelFlag::new(),
            |_| {},
        );
        assert!(matches!(end, ProcessEnd::TimedOut), "{end:?}");
        assert!(t0.elapsed() < std::time::Duration::from_secs(15));
    }

    #[test]
    fn controlled_process_reports_stage_lines_and_output() {
        let mut stages = Vec::new();
        let end = run_process_controlled(
            echo_fake_process("__WERSEUS_STAGE__:pairs:2/5"),
            std::time::Duration::from_secs(30),
            &CancelFlag::new(),
            |l| {
                if let Some(s) = parse_stage_line(l) {
                    stages.push(s);
                }
            },
        );
        match end {
            ProcessEnd::Finished {
                exit_ok, stdout, ..
            } => {
                assert!(exit_ok);
                assert!(stdout.contains("__WERSEUS_STAGE__:pairs:2/5"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            stages,
            vec![StageInfo {
                id: "pairs".into(),
                done: Some(2),
                total: Some(5)
            }]
        );
    }

    #[test]
    fn spawn_failure_is_reported() {
        let end = run_process_controlled(
            new_command("werseus_no_such_program_xyz"),
            std::time::Duration::from_secs(5),
            &CancelFlag::new(),
            |_| {},
        );
        assert!(matches!(end, ProcessEnd::SpawnFailed(_)));
    }

    #[test]
    fn stage_line_parsing_is_strict() {
        assert_eq!(
            parse_stage_line("__WERSEUS_STAGE__:load"),
            Some(StageInfo {
                id: "load".into(),
                done: None,
                total: None
            })
        );
        assert_eq!(
            parse_stage_line("  __WERSEUS_STAGE__:pairs:3/12 \r"),
            Some(StageInfo {
                id: "pairs".into(),
                done: Some(3),
                total: Some(12)
            })
        );
        // kimlik yalnız [a-z0-9_]; betik çıktısı aşama sayılmaz
        assert_eq!(parse_stage_line("__WERSEUS_STAGE__:Bad Id"), None);
        assert_eq!(parse_stage_line("__WERSEUS_STAGE__:"), None);
        assert_eq!(parse_stage_line("__WERSEUS_STAGE__:../../x"), None);
        assert_eq!(parse_stage_line("hello __WERSEUS_STAGE__:load"), None);
        // eksik toplam: sayaç atılır
        assert_eq!(
            parse_stage_line("__WERSEUS_STAGE__:pairs:3").unwrap().done,
            None
        );
    }

    #[test]
    fn cad_limits_defaults_clamp_and_old_files() {
        let d = CadLimits::default();
        assert_eq!((d.part_secs, d.assembly_secs), (300, 900));
        let c = CadLimits {
            part_secs: 0,
            assembly_secs: u64::MAX,
            total_secs: 100,
        }
        .clamped();
        assert_eq!(c.part_secs, CadLimits::MIN_SECS);
        assert_eq!(c.assembly_secs, CadLimits::MAX_SECS);
        assert_eq!(c.total_secs, 100);
        // eksik alanlar varsayılana düşer
        let l: CadLimits = serde_json::from_str(r#"{"part_secs": 120}"#).unwrap();
        assert_eq!(
            (l.part_secs, l.assembly_secs, l.total_secs),
            (120, 900, 3600)
        );
    }

    #[test]
    fn interpret_output_matches_sync_semantics() {
        assert!(interpret_cad_output("no json", "").is_err());
        let ok = r#"__WERSEUS_CAD_JSON_START__{"ok":true,"status":"ok","valid":true,"volume_mm3":5.0}__WERSEUS_CAD_JSON_END__"#;
        assert!(interpret_cad_output(ok, "").is_ok());
        assert!(interpret_cad_output("", ok).is_ok());
        let bad = r#"__WERSEUS_CAD_JSON_START__{"ok":false,"status":"error","error":"x"}__WERSEUS_CAD_JSON_END__"#;
        assert!(interpret_cad_output(bad, "").unwrap_err().contains("x"));
    }

    #[test]
    fn test_detect_freecad_async_with_ctx() {
        let ctx = egui::Context::default();
        let rx = detect_freecad_async(None, Some(ctx));
        let _ = rx.recv();
    }

    #[cfg(windows)]
    #[test]
    fn test_create_no_window_constant() {
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }
}
