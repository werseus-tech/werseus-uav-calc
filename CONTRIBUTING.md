# Katkı Kılavuzu / Contributing Guide

[English version below](#contributing-guide-en)

---

## Türkçe

"WerSeuS – UAV Calc" projesine katkıda bulunmak istediğiniz için teşekkür ederiz! Açık kaynak topluluğunun desteğiyle havacılık tasarım araçlarını daha erişilebilir kılmayı amaçlıyoruz.

### 1. Geliştirme Ortamı ve Derleme

Proje **Rust 1.99+** ve Windows ortamında **x86_64-pc-windows-msvc** araç zinciri kullanılarak geliştirilmektedir.

- **Kurulum**:
  1. [rustup](https://rustup.rs) ile Rust'ı kurun.
  2. **Visual Studio 2022 Build Tools**'u "C++ ile masaüstü geliştirme" (Desktop development with C++) iş yüküyle kurun (MSVC derleyici/bağlayıcı + Windows SDK; exe ikonu ve sürüm bilgisi için `rc.exe` buradan gelir).
  3. Başka ayar gerekmez: depodaki `rust-toolchain.toml` doğru araç zincirini (`stable-x86_64-pc-windows-msvc`, `rustfmt`, `clippy`) kendiliğinden seçer ve gerekirse indirir. `rustup default` değiştirmenize gerek yoktur.
- **Statik Analiz**:
  ```powershell
  cargo clippy --all-targets -- -D warnings
  ```
- **Testleri Çalıştırma**:
  ```powershell
  cargo test
  ```
  - Canlı Ollama testi (`test_live_ollama_qwen3_quick_design`) varsayılan olarak atlanır çünkü yerel Ollama'ya `qwen3:8b` modelini yükler. Çalıştırmak için: `$env:WERSEUS_LIVE_OLLAMA="1"; cargo test test_live_ollama_qwen3_quick_design`
- **Kod Formatı Kontrolü**:
  ```powershell
  cargo fmt --check
  ```
- **Dağıtım (Release) Derlemesi**:
  ```powershell
  cargo build --release
  ```

### 2. Formül Değişikliklerinde Bilimsel Kaynak Zorunluluğu

Havacılık mühendisliği hesaplamalarının doğruluğu esastır. `src/calc/` altındaki formüllerde veya matematiksel katsayılarda yapılacak her değişiklikte:
- Tanınmış bir literatür kaynağı (Raymer, Roskam, Leishman, NASA/AIAA makalesi vb.) belirtilmelidir.
- İlgili kitap adı, basım yılı, denklem numarası ve sayfa numarası commit/PR açıklamasında ve kod içi yorumlarda yer almalıdır.
- Kaynaksız veya kişisel sezgiye dayalı formül değişiklikleri kabul edilmez.

### 3. Çok Dillilik (i18n)

Arayüze veya raporlama modüllerine eklenen her metin için:
- Hem `assets/i18n/tr.json` hem de `assets/i18n/en.json` dosyalarına karşılık gelen anahtar eklenmelidir.
- Format belirteçlerinin (`{0}`, `{1}` veya `{}`) sayı ve sırası iki dilde de tam eşit olmalıdır (`cargo test test_i18n` ile doğrulanır).

### 4. Topluluk Bileşen Veritabanına Veri Ekleme (data/)

Topluluk bileşen kütüphanesine (`data/cells.json`, `data/packs.json`, `data/motors.json`, `data/props.json`, `data/escs.json`) yeni bileşen eklerken aşağıdaki kurallara kesinlikle uyulmalıdır:
- **Üretici Veri Sayfası Zorunluluğu**: Tüm teknik değerler (gerilim, kapasite, kütle, sürekli deşarj akımı) doğrudan üreticinin resmi teknik veri sayfasından (datasheet) alınmalıdır. E-ticaret sitelerindeki (Banggood, AliExpress, yerel hobi mağazaları vb.) satıcı beyanları kabul edilmez.
- **Otomatik Kazıma (Scraping) Yasaktır**: Web scraping yöntemleriyle toplanan veya kaynağı belirsiz veri setleri kabul edilmez.
- **Zorunlu Alanlar**: Her kayıtta `id` (sabit ASCII, küçük harf + alt çizgi), `source` (üretici veri sayfası adı veya doğrudan bağlantısı) ve `verified_by` (GitHub kullanıcı adınız veya "maintainer") bulunmalıdır.
- **Fiziksel Tutarlılık (Wh/kg)**: Hücrelerin özgül enerjisi (`nominal_v * capacity_ah / kg`) elektrokimyasal olarak makul fiziksel sınırlar (50–450 Wh/kg) içinde olmalıdır.
- **Doğrulama**: Veri ekledikten sonra `powershell .\scripts\validate-data.ps1` ve `cargo test` komutlarını çalıştırarak manifest hash ve şema tutarlılığını doğrulayın.

### 5. Hata Bildirimi

- Güvenlik açıkları için lütfen doğrudan `info.wsseng@gmail.com` adresine e-posta gönderin.
- Yazılım hataları için GitHub Issues şablonlarını kullanabilirsiniz.

### 6. Katkıcı Lisans Sözleşmesi (CLA)

Tüm katkıcıların telif hakları ve ticari çift lisanslama modelinin korunabilmesi amacıyla [CLA.md](CLA.md) belgesini kabul etmesi gerekmektedir. PR açtığınızda CLA koşullarını onaylamış sayılırsınız.

---

<a name="contributing-guide-en"></a>
## English

Thank you for your interest in contributing to "WerSeuS – UAV Calc"!

### 1. Build & Toolchain

The project is built with **Rust 1.99+** using the **x86_64-pc-windows-msvc** toolchain.

- **Setup**:
  1. Install Rust with [rustup](https://rustup.rs).
  2. Install **Visual Studio 2022 Build Tools** with the "Desktop development with C++" workload (MSVC compiler/linker + Windows SDK; `rc.exe` from the SDK embeds the exe icon and version info).
  3. Nothing else to configure: the repo's `rust-toolchain.toml` selects (and downloads if needed) `stable-x86_64-pc-windows-msvc` with `rustfmt` and `clippy`. You do not need to change `rustup default`.
- **Static Analysis**:
  ```powershell
  cargo clippy --all-targets -- -D warnings
  ```
- **Run Tests**:
  ```powershell
  cargo test
  ```
  - The live Ollama test (`test_live_ollama_qwen3_quick_design`) is skipped by default because it loads `qwen3:8b` into your local Ollama. To run it: `$env:WERSEUS_LIVE_OLLAMA="1"; cargo test test_live_ollama_qwen3_quick_design`
- **Code Style Check**:
  ```powershell
  cargo fmt --check
  ```
- **Release Build**:
  ```powershell
  cargo build --release
  ```

### 2. Engineering References for Formulas

Accuracy of aerodynamic and structural calculations is paramount. Any proposed formula changes in `src/calc/` MUST provide verifiable engineering citations:
- Recognized reference textbook or peer-reviewed publication (e.g. Raymer, Roskam, Leishman, NASA/AIAA).
- Title, edition, equation number, and page number must be clearly documented.

### 3. Localization (i18n)

Any user-facing text must be added to both `assets/i18n/tr.json` and `assets/i18n/en.json`. Format parameters must match identically.

### 4. Contributing to Community Component Database (data/)

When contributing new components to the community database (`data/cells.json`, `data/packs.json`, `data/motors.json`, `data/props.json`, `data/escs.json`), the following rules are strictly enforced:
- **Manufacturer Datasheet Required**: All technical specifications (voltage, capacity, mass, continuous current) must originate directly from the manufacturer's official engineering datasheet. Reseller or hobby store claims (Banggood, AliExpress, etc.) are not accepted.
- **No Web Scraping**: Data sets scraped from online storefronts or unverified aggregators are strictly prohibited.
- **Mandatory Fields**: Every record must include `id` (ASCII lowercase + underscore), `source` (official datasheet title or link), and `verified_by` (your GitHub handle or "maintainer").
- **Physical Plausibility (Wh/kg)**: Cell specific energy (`nominal_v * capacity_ah / kg`) must fall within realistic electrochemical bounds (50–450 Wh/kg).
- **Validation**: After modifying data files, run `powershell .\scripts\validate-data.ps1` and `cargo test` to verify manifest checksums and schema integrity.

### 5. Contributor License Agreement (CLA)

All contributors must accept the terms set forth in [CLA.md](CLA.md) to enable future sustainability and dual licensing.

For questions or security concerns, contact: `info.wsseng@gmail.com`.
