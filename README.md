# WerSeuS – UAV Calc

[![CI](https://img.shields.io/badge/CI-windows--msvc-blue.svg)](https://github.com/werseus-tech/werseus-uav-calc)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/License-AGPL--3.0--or--later-blue.svg)](https://github.com/werseus-tech/werseus-uav-calc)
[![Version](https://img.shields.io/badge/Version-v0.0.1-green.svg)](https://github.com/werseus-tech/werseus-uav-calc)

Küçük, elektrikli sabit kanatlı (klasik ve V-kuyruk), uçan kanat (flying wing), çok rotorlu (multirotor: 3/4/6/8 kol, X/+/H, koaksiyel) ve hibrit VTOL Quadplane İHA'ların ön tasarımı ve aerodinamik/yapısal analizi için açık kaynaklı masaüstü mühendislik yazılımı (Rust 1.99, eframe/egui).

[English version below](#english-summary)

---

## İndir / Download

**[En son sürümü indir / Download the latest release](https://github.com/werseus-tech/werseus-uav-calc/releases/latest)** (Windows 10/11, 64-bit)

1. Sürüm sayfasından `WerSeuS-UAV-Calc-v…-windows-x64.zip` dosyasını indirin.
2. Zip'i istediğiniz bir klasöre çıkarın.
3. `werseus-uav-calc.exe` dosyasını çalıştırın. Kurulum gerekmez.

*Download the zip from the release page, extract it, and run `werseus-uav-calc.exe`. No installation needed.*

> **Windows SmartScreen uyarısı / warning:** `.exe` dijital olarak imzalı değildir; Windows "Windows bilgisayarınızı korudu" uyarısı gösterebilir. **Ek bilgi → Yine de çalıştır** seçeneğini kullanın. İndirdiğiniz dosyayı sürümle birlikte gelen `SHA256SUMS.txt` ile doğrulayabilirsiniz: `Get-FileHash .\WerSeuS-UAV-Calc-v0.0.1-windows-x64.zip -Algorithm SHA256`.
> *The exe is unsigned. If SmartScreen warns you, choose **More info → Run anyway**, and verify the zip with `Get-FileHash` against `SHA256SUMS.txt`.*

---

## Öne Çıkan Özellikler

- **Geniş Platform Desteği**: Konvansiyonel sabit kanat, V-kuyruk, uçan kanat (flying wing), döner kanat (multirotor: tricopter, quadcopter, hexacopter, octocopter; X, +, H ve koaksiyel) ve hibrit VTOL Quadplane.
- **Sihirbaz Modu (Wizard) & Uzman Modu**: Sıralı ve doğrulamalı adım adım ön tasarım sihirbazı. Tüm adımlar geçerli olunca açılan serbest Uzman Modu sekme gezintisi.
- **Doğrulanmış Hesap Çekirdeği**:
  - Helmbold 3B kaldırma eğimi, Hoerner Oswald e, sıfır kaldırma sürüklemesi ($C_{D0}$) dökümü ve sürükleme kutbu ($C_D = C_{D0} + k C_L^2$).
  - Leishman momentum teorisi ile multirotor askı (hover) gücü, disk yükü, komşu pervane aralığı ve ileri uçuş aerodinamik direnç hesabı.
  - Boyuna denge (nötr nokta, batarya konumu çözücü, statik marjin).
  - Kabuk ve karbon boru kök eğilme gerilmesi ve güvenlik katsayısı.
- **Üç Panelli Arayüz**: Girdiler solda, canlı analiz sonuçları sağda, görünüm/grafik/formüller ortada; açık ve koyu tema, tam Ayarlar sayfası.
- **Canlı Görünümler**: Ölçekli 2D vektörel teknik çizim (kanat/kuyruk geometrisi, motor koordinatları, CG/NP göstergeleri) ve CAD tarzı 3D görünüm (perspektif/ortografik, hazır görünümler).
- **Öğretmen Modu**: Seviye seçimi, adım derslerinde "Bana öğret", mini testler, "Neden bu sonuç çıktı?" açıklamaları ve sınıf raporu.
- **İsteğe Bağlı Yapay Zekâ Yardımcısı**: Yerel Ollama veya OpenAI uyumlu/Anthropic/Gemini bulut sağlayıcıları. Varsayılan olarak kapalıdır; öneriler yalnız onayınızla uygulanır ve geri alınabilir.
- **Rüzgâr Tüneli (ön tasarım düzeyi)**: 2D profil için panel yöntemi ve sınır tabaka, 3D kanat için girdap kafesi yöntemi (VLM); programın formülleriyle karşılaştırma.
- **FreeCAD Çıktıları (isteğe bağlı, FreeCAD 1.1)**: Parça bazlı parametrik `.FCStd`/`.STEP`, montaj dosyası, malzeme listesi (BOM) ve çakışma/temas kontrolü.
- **Excel Dışa Aktarma** ve **Topluluk Bileşen Veritabanı** (motor, pervane, ESC, hücre, batarya paketi; güncelleme yalnız onayınızla).
- **Tasarım Raporu Dışa Aktarma**: Tek tuşla tüm girdileri, çıktıları, uyarıları ve kullanılan matematiksel formülleri Markdown (`.md`) olarak kaydetme.
- **Hazır Örnek Projeler**: Hazır örnek projeler (Sabit Kanat 2 kg, Uçan Kanat 1200 mm, Quad 250, Quad 275, Hexa 550, VTOL Quadplane).
- **Çift Dilli Arayüz (i18n)**: Türkçe ve İngilizce tam dil desteği, otomatik sistem dili algılama ve anlık menüden dil değiştirme.
- **Kullanıcı Onaylı Hata Raporlama**: Çökme yakalayıcı (`panic hook`), gizlilik maskelemesi (yol ve API anahtarı temizliği) ve kullanıcı kontrollü e-posta/GitHub Issue bildirimi.
- **Tek Tuşla Güvenli Kaldırma (Uninstall)**: Programın sistemde bıraktığı ayar, günlük ve önbellek dosyalarını temizleme sihirbazı veya `--uninstall` komut satırı parametresi.

---

## İndirme ve Kurulum

Windows 10/11 x64 için hazır dağıtım paketi [Releases](https://github.com/werseus-tech/werseus-uav-calc/releases/latest) sayfasındadır (yukarıdaki "İndir" bölümüne bakın). Kurulum veya harici DLL gerektirmez. FreeCAD çıktıları için FreeCAD 1.1 ve yapay zekâ yardımcısı için bir model/sağlayıcı isteğe bağlıdır.

---

## Kaynak Koddan Derleme

Proje **Rust 1.99+** ve Windows ortamında **x86_64-pc-windows-msvc** araç zinciri ile derlenir. Gerekenler: [rustup](https://rustup.rs) ile Rust ve **Visual Studio 2022 Build Tools** ("C++ ile masaüstü geliştirme" iş yükü). Araç zincirini depodaki `rust-toolchain.toml` kendiliğinden seçer.

```powershell
# 1. Depoyu klonlayın
git clone https://github.com/werseus-tech/werseus-uav-calc
cd werseus-uav-calc

# 2. Araç zinciri rust-toolchain.toml ile otomatik seçilir (MSVC); kontrol:
rustup show active-toolchain

# 3. Testleri çalıştırın
cargo test

# 4. Release ikilisini derleyin
cargo build --release

# 5. Dağıtım zip arşivini üretin
.\scripts\build-release.ps1
```

Zip arşivi ve `SHA256SUMS.txt` `dist/` altında oluşturulur (`dist/WerSeuS-UAV-Calc-v<sürüm>-windows-x64.zip`).

---

## Programı Kaldırma (Uninstall)

Programın bıraktığı izleri temizlemek için iki yöntem mevcuttur:

1. **Arayüzden**: Yardım menüsü → **"Programı kaldır…"** seçeneğine tıklayın.
   - Açılan pencerede silmek istediğiniz kategorileri seçin (ayarlar, günlükler, önbellek, program dosyaları).
   - Kullanıcının kendi proje dosyaları ve FreeCAD çıktıları varsayılan olarak **korunur**.
   - Onay kutusuna büyük harflerle `KALDIR` yazıp onaylayın.
2. **Komut Satırından**:
   ```powershell
   .\werseus-uav-calc.exe --uninstall
   ```

---

## Lisans ve Ticari Model

Bu yazılım **AGPL-3.0-or-later** (GNU Affero General Public License v3 veya sonraki sürümler) ile lisanslanmıştır.

- Hobi, eğitim, araştırma ve AGPL koşullarına (değişiklik yapılan kaynak kodun kullanıcılarla paylaşılması) uyan ticari kullanımlar için **ücretsizdir**.
- **Ticari Lisans**: AGPL-3.0 yükümlülüklerine tabi olmadan yazılımı kapalı kaynaklı ürünlerine entegre etmek veya SaaS olarak sunmak isteyen kurumlar için ayrı ücretli ticari lisans seçeneği mevcuttur.
- Ticari lisans başvurusu ve kurumsal destek için: `info.wsseng@gmail.com`

Lisans ve yasal belgeler:
- [LICENSE](LICENSE): AGPL-3.0 lisans metni
- [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md): ticari lisans açıklaması
- [CLA.md](CLA.md): katkıda bulunan lisans sözleşmesi (CLA)
- [PRIVACY.md](PRIVACY.md): gizlilik ve KVKK aydınlatma metni
- [DISCLAIMER.md](DISCLAIMER.md): sorumluluk reddi

---

## Önemli Sorumluluk Reddi (Disclaimer)

WerSeuS – UAV Calc tarafından üretilen sonuçlar **ön tasarım mühendislik tahminleridir**. Hiçbir çıktı uçuşa elverişlilik sertifikası veya emniyet garantisi niteliği taşımaz. Gerçek kütle, ağırlık merkezi, motor/pervane itkisi ve yapısal dayanım değerleri yer testleri, statik yüklemeler ve kademeli uçuş testleriyle yetkin uzmanlarca doğrulanmalıdır. Kullanıcı kendi ülkesindeki sivil havacılık mevzuatına (SHGM, EASA, FAA) uymakla yükümlüdür. Ayrıntılar için [DISCLAIMER.md](DISCLAIMER.md) dosyasına bakın.

---

<a name="english-summary"></a>
## English Summary

**WerSeuS – UAV Calc** is a lightweight Windows desktop engineering tool written in Rust and `egui` for the preliminary design and analysis of small electric fixed-wing aircraft (conventional, V-tail, flying wing), multirotors (3/4/6/8 arms, X/+/H/coaxial), and hybrid VTOL Quadplanes.

### Key Features
- **Guided Design Wizard & Expert Mode**: Sequential parameter verification with prerequisite step locking, alongside free-form tab navigation for validated projects.
- **Aeronautical & Multirotor Physics**: Momentum theory actuator disk modeling, parasitic drag estimation, 3D lift slope (Helmbold), Oswald efficiency, longitudinal static margin, and spar bending stress.
- **Three-Panel UI, 2D & 3D Views**: Inputs on the left, live analysis on the right; light/dark theme, a full Settings page, scaled 2D schematics and a CAD-style 3D view.
- **Teacher Mode**: Level selection, per-step lessons, mini quizzes, "why this result?" explanations and a class report.
- **Optional AI Assistant**: Local Ollama or cloud providers (OpenAI-compatible, Anthropic, Gemini). Off by default; suggestions are applied only with your approval and can be undone.
- **Wind Tunnel (preliminary level)**: 2D panel method with boundary layer and a 3D vortex lattice method, compared against the program's own formulas.
- **FreeCAD Outputs (optional, FreeCAD 1.1)**: Per-part parametric `.FCStd`/`.STEP`, assembly, bill of materials and interference check.
- **Excel Export** and a **Community Component Database** (updates only with your approval).
- **Markdown Report Generation**: Instant export of complete design parameters, calculations, and mathematical citations to a single Markdown file.
- **Offline & Self-Contained**: Standalone Windows x64 binary with a statically linked C runtime (no VC++ redistributable needed).
- **Full Localization (TR / EN)**: Dynamic language switching and OS locale detection.
- **User-Controlled Bug Reporting**: Panic logging to `%APPDATA%\WerSeuS\logs\` with privacy masking and direct mailto / GitHub issue integration.
- **Single-Click Uninstaller**: Automated clean removal of application settings, cache, logs, and binaries (`--uninstall`).

### Build Instructions
```powershell
# Prerequisites: Rust (rustup) + Visual Studio 2022 Build Tools ("Desktop development with C++").
# The MSVC toolchain is selected automatically by rust-toolchain.toml; verify:
rustup show active-toolchain

# Run tests
cargo test

# Build release
cargo build --release

# Package distribution
.\scripts\build-release.ps1
```

### Licensing
Licensed under **AGPL-3.0-or-later**. For proprietary and commercial licensing exempt from AGPL terms, please contact: `info.wsseng@gmail.com`.

Legal documents: [LICENSE](LICENSE), [COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md), [CLA.md](CLA.md), [PRIVACY.md](PRIVACY.md), [DISCLAIMER.md](DISCLAIMER.md).
