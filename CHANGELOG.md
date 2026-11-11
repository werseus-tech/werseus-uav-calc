# Değişiklik Günlüğü (Changelog)

Bu projedeki tüm önemli değişiklikler bu dosyada belgelenecektir.
Format [Keep a Changelog](https://keepachangelog.com/tr/1.1.0/) standardına dayanmaktadır.

## [Unreleased]

## [0.0.1] - 2026-11-11

İlk herkese açık sürüm (Windows x64, Türkçe ve İngilizce).

### Added
- **Platform türleri**: konvansiyonel sabit kanat, V-kuyruk, uçan kanat, multirotor (3/4/6/8 kol; X, +, H, koaksiyel) ve VTOL quadplane.
- **Adım adım sihirbaz ve Uzman Modu**: sıralı, doğrulamalı tasarım adımları; tüm adımlar geçerli olunca serbest sekme gezintisi. Yeni başlayanlar için "Basit" mod (tüm adımlarda ve tüm drone türlerinde) ve "Neden bu sonuç?" açıklamaları.
- **Yeni arayüz**: üç panelli düzen (girdiler solda, canlı analiz sonuçları sağda, görünüm/grafik/formüller ortada), açık ve koyu tema, drone türü seçim kartları.
- **CAD tarzı 3D görünüm**: yörünge kamerası, perspektif/ortografik izdüşüm, İzometrik/Üst/Ön/Yan hazır görünümler; 3D başlatılamazsa 2D şemaya otomatik dönüş. Ölçekli 2D teknik çizim (CG/NP göstergeli) de var.
- **Tam Ayarlar sayfası**: dil, tema, 3D ızgara ve kamera varsayılanı, arayüz ölçeği, yapay zekâ, FreeCAD yolu ve süre sınırları, veri ve gizlilik, veritabanı, Hakkında.
- **Öğretmen modu**: öğrenci seviyesi seçimi, adım derslerinde "Bana öğret", etkileşimli mini testler, "Neden bu sonuç çıktı?" açıklamaları ve sınıf raporu.
- **İsteğe bağlı yapay zekâ yardımcısı**: yerel Ollama modelleri ile OpenAI uyumlu, Anthropic ve Gemini bulut sağlayıcıları. Varsayılan olarak kapalıdır; öneriler yalnız onayınızla uygulanır, sınır denetiminden geçer ve geri alınabilir. API anahtarları Windows Kimlik Bilgisi Yöneticisi'nde saklanır.
- **Rüzgâr tüneli (ön tasarım düzeyi)**: 2D profil için panel yöntemi ve sınır tabaka, 3D kanat için girdap kafesi yöntemi (VLM); Cp dağılımı, kutup eğrileri, açıklık boyunca dağılım ve "programın formülleri ile tünel" karşılaştırması. Proje değerlerini değiştirmez; arka planda çalışır ve iptal edilebilir.
- **FreeCAD parça ve montaj çıktıları** (FreeCAD 1.1, isteğe bağlı): drone türüne göre parça seti için parametrik `.FCStd` ve `.STEP` dosyaları, bağlantı noktalarına göre yerleşimli montaj dosyası, malzeme listesi (`BOM.md`, `BOM.csv`) ve montaj raporu. Çakışma/temas kontrolü, bağlantısız parça uyarısı ve CAD kütlesini onayınızla tasarıma aktarma. NACA profil önizlemesi.
- **Excel dışa aktarma**: girdiler, hesaplar ve özet sayfalarıyla, formüllü `.xlsx` hesap tablosu (TR/EN).
- **Topluluk bileşen veritabanı**: hücre, batarya paketi, motor, pervane ve ESC verileri programın içinde gelir; güncelleme yalnız onayınızla, SHA-256 ve şema doğrulamasıyla yapılır.
- **Genel ve özel bataryalar**: hücre sayısı, kapasite, Wh/kg ve sürekli/pik akıma göre şablonlar ve kullanıcı tanımlı batarya girişi. Multirotor için gövde/kol yapı adımı; boru seçiminde sehim kriteri.
- **Multirotor hesap çekirdeği**: Leishman momentum teorisiyle askı gücü (ideal ve elektriksel), disk yükü, komşu pervane açıklığı, ağırlık merkezi analizi.
- **Dil ve raporlama**: tam Türkçe/İngilizce arayüz (otomatik dil algılama, anlık değiştirme), Markdown tasarım raporu, 6 hazır örnek proje (`examples/`).
- **Güvenlik ve bakım**: çökme günlüğü (`%APPDATA%\WerSeuS\logs\`), yol/anahtar maskelemeli, kullanıcı onaylı hata bildirimi (e-posta veya GitHub Issue), tek tuşla program kaldırma (`--uninstall`). Ekran okuyucu desteği (accesskit), exe ikonu ve sürüm bilgisi.
- **Dağıtım**: `scripts/build-release.ps1` ile zip paketi ve SHA-256 sağlama dosyası; etiketle tetiklenen otomatik GitHub Release iş akışı.
- **Lisans ve yasal belgeler**: AGPL-3.0-or-later `LICENSE`, ticari lisans, CLA, gizlilik (KVKK/GDPR) ve sorumluluk reddi belgeleri (taslak, hukukçu incelemesi önerilir).

### Changed
- Derleme `x86_64-pc-windows-msvc` araç zinciriyle yapılıyor (`rust-toolchain.toml` ile sabit); C çalışma zamanı statik bağlı, VC++ Redistributable gerekmez. Derlemek için rustup + Visual Studio 2022 Build Tools gerekir.
- CI MSVC araç zincirine geçirildi; `clippy` zorunlu adım. gnu araç zinciri yamaları kaldırıldı.
- Release profili tam optimizasyonla (`opt-level = 3`, `lto = "thin"`, `strip = true`) yapılandırıldı; hesap modülleri kütüphane (`lib.rs`) olarak ayrıldı.

### Fixed
- Multirotor ve uçan kanatta gövde dengesi ve statik marjin sınırları; kanat/kuyruk profili ayrımı (yatay, dikey, V kuyruk, uçan kanat winglet).
- Basit mod aerodinamiği: kanat etiketleri, `CL_max` için Reynolds bağımlılığı, kamber hesabı, birim ve sınır değerleri.
- Özel batarya akımı: sabit kanatta paralel hücre sayısının akımı katlama hatası.
- HTTPS bağlantıları Windows SChannel (`native-tls`) ile yapılıyor; TLS başlatılamazsa açık hata gösteriliyor.
- API anahtarı silme (Kimlik Bilgisi Yöneticisi'nden temizlenir), program kaldırmada başarısız adımların listelenmesi, hata raporu ve günlüklerde yol/kullanıcı/bilgisayar adı maskelemesi.
- FreeCAD çıktıları: Unicode (ör. Türkçe karakterli) yollarda ASCII geçici dizin; çıkışta FreeCAD iptali, zaman aşımı iletisi, süreç çıktısı sınırı.
- NaN değerli girdilerde sahne kararlılığı, winglet ok açısı sınırı, bayat GL hatası temizliği.
- Veri dosyalarında satır sonu (`.gitattributes`), dosya adı izin listesi, atomik yazma; testlerde canlı Ollama yalnız `WERSEUS_LIVE_OLLAMA=1` ile çalışır.
