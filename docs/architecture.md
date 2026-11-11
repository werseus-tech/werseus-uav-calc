# WerSeuS – UAV Calc Mimari Dokümanı (Architecture)

Bu doküman "WerSeuS – UAV Calc" uygulamasının yazılım mimarisini, veri akışını, hesaplama çekirdeğini ve arayüz yapısını özetler.

---

## 1. Modül Hiyerarşisi

Proje, UI'dan bağımsız saf matematiksel hesaplama kütüphanesi (`lib.rs`) ve eframe masaüstü kabuğundan (`main.rs`) oluşur:

```
src/
├── lib.rs            # werseus_uav_calc kütüphane kökü
├── main.rs           # İnce binary kabuğu: panic hook, CLI args, eframe başlatma
├── calc/             # Saf mühendislik hesap çekirdeği (UI bağımsız)
│   ├── aero.rs       # Helmbold 3D kaldırma eğimi, CD0 sürtünme yığılımı, kutup eğrisi
│   ├── atmosphere.rs # ISA atmosfer modeli (0-11 km)
│   ├── constraint.rs # Kısıt analizi (Stall, R/C, dönüş, kalkış, önerilen nokta)
│   ├── geometry.rs   # Kanat & kuyruk geometrisi, MAC, V-tail dönüşümü
│   ├── multirotor.rs # Momentum teorisi, hover gücü, kol yerleşimi, CG ofseti
│   ├── performance.rs# Elektrikli havada kalma ve menzil analizi
│   ├── propulsion.rs # Pervane-motor eşleşmesi, RPM ve tepe akım kontrolü
│   ├── stability.rs  # Nötr nokta, batarya konumu çözücü, statik marjin
│   ├── structure.rs  # Kök eğilme momenti, karbon boru gerilmesi
│   └── weight.rs     # Malzeme kütüphanesi, hücre veri tabanı, MTOW hesabı
├── project.rs        # Girdi durum modeli (Project, DroneType, Serde JSON)
├── report.rs         # Markdown formatında kapsamlı tasarım raporu oluşturucu
├── support.rs        # Hata yakalama (panic hook), logging, crash raporlama, gizlilik maskelemesi
├── footprint.rs      # Tek tuşla kaldırma (uninstall), dosya boyutu tarama, güvenlik denetimi
├── i18n.rs           # Çift dilli (TR / EN) çeviri altyapısı, atomik dil değişimi
└── ui/               # egui / eframe arayüz bileşenleri
    ├── wizard.rs     # Adım adım sihirbaz akışı ve kilit mantığı
    ├── schematic.rs  # 2B ölçekli kanat/kuyruk ve multirotor teknik çizimi
    └── ...           # Diğer sekme arayüzleri
```

---

## 2. Veri Akışı (Data Flow)

Uygulama tek yönlü reaktif veri akışı (unidirectional data flow) prensibiyle çalışır:

```
[ Kullanıcı Girdisi ] 
         │
         ▼
[ Project (JSON Durumu) ] ────► [ Library (cells.json, materials.json) ]
         │                                    │
         └──────────────────┬─────────────────┘
                            ▼
               [ calc::evaluate() Çekirdeği ]
                            │
                            ▼
                     [ Model Çıktısı ]
                            │
          ┌─────────────────┴─────────────────┐
          ▼                                   ▼
 [ UI Render (Tabs/Plots) ]         [ Tasarım Raporu (.md) ]
```

1. **Girdiler**: `Project` yapısı içinde SI birimlerinde tutulur. Herhangi bir kullanıcı girdisi değiştiğinde reaktif olarak `calc::evaluate(&project, &lib)` çalışır.
2. **Hesap Çekirdeği**: `calc::evaluate`, aerodinamik, itki, ağırlık, kısıt, stabilite ve multirotor çıktılarından oluşan birleştirilmiş bir `Model` üretir.
3. **Uyarılar (Diagnostics)**: Formül sınır aşımları ve tutarsızlıklar `Sev::Info`, `Sev::Warn` veya `Sev::Err` olarak `Model.warnings` listesinde toplanır.

---

## 3. Sihirbaz Kilidi Mantığı (Wizard Lock Architecture)

Kullanıcıların hatalı veya tutarsız ön tasarımlar üretmesini önlemek amacıyla iki katmanlı doğrulama mevcuttur:

1. **Sihirbaz Modu (Varsayılan)**:
   - Adımlar sıralıdır (Görev -> Ağırlık -> Geometri -> Aerodinamik -> Tasarım Noktası -> Menzil -> İtki -> Denge -> Yapı -> Özet).
   - Kullanıcı mevcut adımı geçerli kılmadan bir sonraki adıma geçemez (`validate_step`).
   - Önceden tamamlanmış bir adıma dönülüp bir parametre değiştirilirse, dirty-tracking mekanizması sonraki adımları otomatik olarak `NeedsUpdate` durumuna çeker.
2. **Uzman Modu (Expert Mode)**:
   - Serbest sekme gezintisine olanak tanır.
   - Yalnızca **tüm adımlar geçerli (`all_steps_valid`)** olduğunda etkinleştirilebilir.
   - Herhangi bir parametre geçersiz hale gelirse uzman modu otomatik olarak kilitlenir ve kullanıcıyı ilk geçersiz adıma yönlendirir.

---

## 4. Çok Dillilik (i18n)

- Metinler `assets/i18n/tr.json` ve `assets/i18n/en.json` dosyalarından derleme anında ikiliye gömülür (`include_str!`).
- Dinamik yer tutucular `{0}`, `{1}` veya `{}` şeklinde biçimlendirilir (`tf(...)`).
- Çeviri anahtarlarının eksiksizliği, TR ve EN anahtar eşitliği ile format belirteci sayıları `test_i18n_table_completeness` birim testiyle CI seviyesinde zorunlu tutulur.

---

## 5. Sistem İzleri ve Kaldırma (Footprint & Uninstallation)

- `src/footprint.rs` modülü uygulamanın yerel sistemde bıraktığı tüm izleri (`%APPDATA%\WerSeuS`, `%LOCALAPPDATA%\WerSeuS`, `Belgeler\WerSeuS\reports`, `Belgeler\WerSeuS\cad` ve program ikilisi) yönetir.
- **Güvenli Silme**: Sürücü kökü (`C:\`), `Windows`, `Program Files`, `Desktop`, `Documents` gibi kritik sistem yolları otomatik olarak reddedilir (`is_dangerous_path`).
- **Kullanıcı Verisi Koruması**: Kullanıcının özel proje dosyaları ve FreeCAD çıktıları varsayılan olarak korunur; silme işlemi iki aşamalı kullanıcı onayı (`KALDIR`) gerektirir.
- **Kendini Silme (Self-Deletion)**: Çalışan ikili Windows üzerinde kilitli olduğundan, uygulama kapanmadan önce ayrık bir arka plan süreci (`cmd.exe /c timeout /t 2 & rmdir /s /q ...`) tetikler.

---

## 6. Hata ve Çökme Yönetimi (Crash & Support)

- `src/support.rs` modülü `std::panic::set_hook` ile beklenmedik çökmeleri yakalar ve `%APPDATA%\WerSeuS\logs\crash-<timestamp>.log` dosyasına yazar.
- **Gizlilik Maskelemesi**: Kullanıcı profil yolları (`%USERPROFILE%` -> `~`), API anahtarları ve hassas dizgeler rapor oluşturulmadan önce maskelenir.
- **Kullanıcı Onayı**: Hiçbir telemetri veya arka plan iletimi yapılmaz; kullanıcı açıkça onaylamadıkça hiçbir veri dışarıya aktarılmaz.
- **Gönderim Kanalları**: Masaüstü e-posta istemcisi (`mailto:`) ve GitHub Issue URL üreticisi aracılığıyla raporlama desteklenir. Gövde boyutu sınırını aşan raporlar için otomatik ZIP arşivi oluşturulur.


---

## Derleme Araç Zinciri (Build Toolchain)

- Araç zinciri `rust-toolchain.toml` ile `stable-x86_64-pc-windows-msvc` olarak sabitlenmiştir (Visual Studio 2022 Build Tools, C++ iş yükü gerekir). Eski gnu yamaları (`vendor/`, `third_party/`, `[patch.crates-io]`) kaldırılmıştır.
- `.cargo/config.toml` C çalışma zamanını statik bağlar (`+crt-static`); exe yalnızca Windows sistem DLL'lerine bağımlıdır.
- `build.rs` (`winresource`) exe ikonunu (`assets/icon/app_icon.ico`) ve sürüm bilgisini gömer; `.ico`, `scripts/make-icon.ps1` ile PNG'den üretilir.
- Arayüz `eframe` + `glow` (OpenGL) ile çizilir; erişilebilirlik için `accesskit` etkindir. HTTPS `ureq` + `native-tls` (Windows SChannel) kullanır.
