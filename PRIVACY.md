TASLAK - hukukçu incelemesi önerilir / DRAFT - legal review recommended

# WerSeuS – UAV Calc
## Gizlilik ve Kişisel Verilerin Korunması Aydınlatma Metni / Privacy Notice

---

## Türkçe Bölüm: 6698 Sayılı KVKK Uyarınca Aydınlatma Metni

Bu Aydınlatma Metni, 6698 sayılı Kişisel Verilerin Korunması Kanunu ("KVKK") m. 10 uyarınca, "WerSeuS – UAV Calc" masaüstü uygulamasının ("Uygulama") kullanımı, hata ve geri bildirim raporlama süreçleri kapsamında işlenen kişisel verilere ilişkin olarak kullanıcıları bilgilendirmek amacıyla hazırlanmıştır.

### 1. Veri Sorumlusu (KVKK m. 10/a)
* **Veri Sorumlusu:** WerSeuS – UAV Calc Proje Yöneticisi ve Bireysel Geliştiricisi
* **İletişim E-posta Adresi:** `info.wsseng@gmail.com`
* **Resmi Web / Depo Adresi:** `https://github.com/werseus-tech/werseus-uav-calc`

### 2. İşlenen Kişisel Veriler ve Veri Kategorileri
WerSeuS – UAV Calc, yerel (offline) çalışan bir masaüstü mühendislik aracıdır. Uygulama içerisinde **hiçbir arka plan telemetrisi, otomatik kullanım analitiği, kullanıcı takip çerezi veya gizli veri toplama mekanizması bulunmamaktadır**. Veritabanı güncellemesi: GitHub'a yalnızca dosya indirme isteği gider; kişisel veri gönderilmez.

Kişisel veriler yalnızca kullanıcının **açık iradesi ve onayıyla**, Uygulama içerisindeki "Hata Bildir / Geri Bildirim Gönder" penceresini kullanması, bir çökme (crash) sonrası açılan bildirim penceresini onaylaması veya geliştiriciyle doğrudan e-posta ile iletişime geçmesi halinde işlenir. Bu kapsamda işlenebilecek veri kategorileri şunlardır:

1. **Sistem ve Ortam Bilgisi:**
   * Uygulama sürümü (`CARGO_PKG_VERSION`)
   * İşletim sistemi türü (`OS`) ve işlemci mimarisi (`ARCH`)
   * Seçilen arayüz dili (`TR` / `EN`)
   * Üzerinde çalışılan hava aracı platform türü (sabit kanat, multirotor vb.)
   * Olay zaman damgası (UTC)
2. **Çökme ve Hata Kayıtları (Crash Log):**
   * Panik (panic) durumunun oluştuğu kaynak kod konumu (dosya adı, satır ve sütun numarası)
   * Panik mesajı ve hata açıklaması
   * Çağrı yığın izi (call stack backtrace)
3. **Uygulama Günlükleri (Log Tail):**
   * Kullanıcının yerel cihazında `%APPDATA%\WerSeuS\logs\app.log` dosyasında tutulan son 200 satırlık olay kaydı
4. **Kullanıcı Açıklaması ve İsteğe Bağlı Proje Dosyası:**
   * Kullanıcının hata bildirim kutusuna kendi isteğiyle girdiği serbest metin açıklaması
   * Kullanıcı tarafından onay kutusu işaretlenerek rapora eklenmesi seçilen tasarım modeli (`project.json`)
5. **İletişim Verisi (E-posta):**
   * Kullanıcı hata raporunu e-posta yoluyla iletmeyi seçerse, e-posta başlığında yer alan gönderici e-posta adresi ve ad-soyad bilgisi

#### Otomatik Maskeleme ve Toplanmayan Veriler:
Uygulama kaynak kodunda (`support.rs`) yer alan gizlilik maskelemesi (`mask_sensitive_info`) uyarınca:
* Kullanıcı profili dizin yolları (`%USERPROFILE%`, `X:\Users\<kullanıcı>`, `/` ayırıcılı ve JSON'da kaçışlı `X:\\Users\\<kullanıcı>` biçimleri) otomatik olarak `~` işareti ile maskelenir.
* Bilgisayar adı (`COMPUTERNAME`) maskelenir (`[HOSTNAME]`) veya rapora eklenmez.
* Aşağıdaki API anahtarı/jeton kalıpları tespit edilerek `[REDACTED_API_KEY]` şeklinde sansürlenir: `sk-` / `sk-ant-` önekli anahtarlar (OpenAI, Anthropic; en az 16 karakter), `AIza` önekli Google anahtarları (en az 30 karakter), `ai-` önekli anahtarlar, `Bearer`, `x-api-key` ve `x-goog-api-key` başlık değerleri (en az 16 karakter) ve 32+ karakterlik onaltılık (hex) veya harf+rakam karışımı uzun gizli dizgiler.
* Bu maskeleme yöntemleri tüm kişisel bilgi biçimlerini kapsamayabilir; kullanıcı raporu göndermeden önce içeriğini incelemelidir.
* Uzun raporlarda oluşturulan ZIP dosyasındaki `report.txt` ve `project.json` dosyalarına yukarıdaki otomatik maskeleme uygulanır. Yine de ZIP dosyası uygulama tarafından otomatik olarak eklenmez; kullanıcı tarafından ayrıca eklenir ve eklemeden önce incelenmelidir.
* **Uygulama IP adresi, MAC adresi ve donanım seri numaralarını bilerek toplamaz ve rapora eklemez.**

#### Yapay Zeka (AI) Sağlayıcıları ve API Anahtarları:
Kullanıcı, Uygulama bünyesindeki isteğe bağlı yapay zeka asistanını harici bir sağlayıcı (OpenAI uyumlu, Anthropic, Google Gemini vb.) veya yerel Ollama ile kullanmayı tercih ederse; girilen API anahtarları Windows Kimlik Bilgisi Yöneticisi'nde (Windows Credential Manager) yerel olarak saklanır. Tasarım sorguları kullanıcının doğrudan kendi API anahtarı ve seçtiği sağlayıcı sunucuları arasında gerçekleşir; geliştirici bu verilere veya API anahtarlarına erişemez.

### 3. Kişisel Verilerin İşlenme Amaçları (KVKK m. 10/b)
Toplanan kişisel veriler;
* Yazılımda meydana gelen beklenmedik çökmelerin, teknik hataların ve mantıksal kusurların tespiti, analizi ve giderilmesi,
* Yazılımın kararlılığının, emniyetinin ve performansının artırılması,
* Kullanıcı tarafından yöneltilen hata bildirimlerinin ve teknik destek taleplerinin yanıtlanması,
* Açık kaynaklı ve ticari sürümlerin sürdürülebilirliğinin temini
amaçlarıyla sınırlı olarak işlenir.

### 4. Kişisel Veri Toplamanın Yöntemi ve Hukuki Sebebi (KVKK m. 10/ç)
* **Toplama Yöntemi:** Veriler, kullanıcının masaüstü uygulaması arayüzündeki rapor önizleme ekranında rapor içeriğini bizzat inceleyip onaylaması ve "E-posta ile Gönder" ya da "GitHub'da Bildir" butonlarına tıklaması veya doğrudan e-posta göndermesi suretiyle tamamen dijital ve otomatik/kısmen otomatik yöntemlerle toplanır.
* **Hukuki Sebepler:**
  * **Açık Rıza (KVKK m. 5/1):** Kullanıcının hata raporunu gönderme penceresinde bildirimi onaylaması ve kendi e-posta istemcisi veya GitHub hesabı üzerinden paylaşması.
  * **Meşru Menfaat (KVKK m. 5/2 (f)):** İlgili kişinin temel hak ve özgürlüklerine zarar vermemek kaydıyla, yazılımın güvenliğinin sağlanması, ölümcül yazılım çökmelerinin giderilmesi ve kararlılık takibinin yapılması için veri işlemenin zorunlu olması.

### 5. Kişisel Verilerin Aktarılması ve Alıcı Grupları (KVKK m. 10/c)
Kişisel verileriniz üçüncü taraflara ticari, reklam veya pazarlama amacıyla kesinlikle satılmaz veya kiralanmaz. Veriler yalnızca kullanıcının seçtiği bildirim yöntemine bağlı olarak şu alıcılara aktarılır:
1. **Google LLC (Gmail):** Kullanıcı "E-posta ile Gönder" seçeneğini kullandığında uygulama, raporu içeren bir e-posta taslağını kullanıcının varsayılan e-posta istemcisinde açar. Rapor, kullanıcı göndermeyi onayladığında kullanıcının e-posta sağlayıcısı üzerinden `info.wsseng@gmail.com` adresine (Google LLC tarafından işletilen Gmail) iletilir.
2. **GitHub, Inc. (GitHub Issues):** Kullanıcı "GitHub'da Bildir" seçeneğini kullandığında uygulama, raporu içeren bir GitHub yeni sorun sayfasını tarayıcıda açar. Rapor, kullanıcı GitHub'da gönder işlemini onayladığında GitHub üzerinde herkese açık bir hata kaydı (public issue) olarak yayınlanır. Kullanıcı bu seçeneği kullanırken rapor içeriğinde gizli kalmasını istediği kişisel veya ticari bilgilerin bulunmadığını doğrulamaktan kendisi sorumludur.
3. **Kullanıcının Seçtiği Yapay Zeka Sağlayıcısı:** AI asistanı kullanıldığı takdirde, veriler yalnızca kullanıcının doğrudan belirlediği sağlayıcıya (Ollama, OpenAI, Anthropic, Google vb.) kullanıcının kendi yetkilendirmesiyle aktarılır.

Google LLC ve GitHub, Inc. ABD merkezli şirketlerdir; bu alıcılara yapılan aktarımlar yurt dışına aktarım niteliğinde olabilir (KVKK m. 9). Bu aktarımın koşulları hukuki incelemeye tabidir.

### 6. Saklama Süresi ve İmha
* Geliştiriciye e-posta yoluyla iletilen hata raporları, teknik sorunun tespiti, giderilmesi ve geriye dönük sürüm doğrulama testlerinin tamamlanabilmesi amacıyla azami **2 yıl** süreyle arşivlenir. Bu sürenin sonunda veya kullanıcının geçerli imha talebi üzerine veriler silinir veya anonim hale getirilir.
* GitHub Issues üzerinde açılan kayıtlar açık kaynak proje yönetim süreçleri kapsamında GitHub politikalarına tabidir.
* Kullanıcı dilediği zaman yerel cihazında tutulan tüm logları (çökme günlükleri dahil) ve ayarları Uygulama içerisindeki "Programı Kaldır" sihirbazı veya `--uninstall` komutu ile silebilir (`%APPDATA%\WerSeuS` dizini temizlenir).
* Hata raporu ZIP dosyaları `Belgeler\WerSeuS\reports` klasöründe tutulur; bu klasör `%APPDATA%` dışındadır ve kullanıcı tarafından ayrıca silinmelidir.

### 7. İlgili Kişinin Hakları (KVKK m. 11)
KVKK'nın 11. maddesi uyarınca veri sahibi olarak aşağıdaki haklara sahipsiniz:
* a) Kişisel veri işlenip işlenmediğini öğrenme,
* b) Kişisel verileri işlenmişse buna ilişkin bilgi talep etme,
* c) Kişisel verilerin işlenme amacını ve bunların amacına uygun kullanılıp kullanılmadığını öğrenme,
* ç) Yurt içinde veya yurt dışında kişisel verilerin aktarıldığı üçüncü kişileri bilme,
* d) Kişisel verilerin eksik veya yanlış işlenmiş olması hâlinde bunların düzeltilmesini isteme,
* e) KVKK'nın 7. maddesinde öngörülen şartlar çerçevesinde kişisel verilerin silinmesini veya yok edilmesini isteme,
* f) (d) ve (e) bentleri uyarınca yapılan işlemlerin, kişisel verilerin aktarıldığı üçüncü kişilere bildirilmesini isteme,
* g) İşlenen verilerin münhasıran otomatik sistemler vasıtasıyla analiz edilmesi suretiyle kişinin kendisi aleyhine bir sonucun ortaya çıkmasına itiraz etme,
* ğ) Kişisel verilerin kanuna aykırı olarak işlenmesi sebebiyle zarara uğraması hâlinde zararın giderilmesini talep etme.

### 8. Başvuru Usulü ve İletişim
Yukarıda belirtilen haklarınızı kullanmak için taleplerinizi kimliğinizi tevsik edici belgelerle birlikte `info.wsseng@gmail.com` e-posta adresine iletebilirsiniz. Başvurularınız KVKK m. 13 ve Veri Sorumlusuna Başvuru Usul ve Esasları Hakkında Tebliğ hükümleri uyarınca incelenerek en geç 30 (otuz) gün içinde ücretsiz olarak sonuçlandırılacaktır.

---

## English Section: GDPR Privacy Notice (Regulation (EU) 2016/679)

This Privacy Notice is provided in accordance with Article 13 of the General Data Protection Regulation ("GDPR") for users of the "WerSeuS – UAV Calc" Windows desktop application ("the Software").

### 1. Data Controller
* **Controller:** Individual Developer / Maintainer of "WerSeuS – UAV Calc"
* **Contact Email:** `info.wsseng@gmail.com`
* **Repository / Website:** `https://github.com/werseus-tech/werseus-uav-calc`

### 2. Categories of Data Processed
WerSeuS – UAV Calc is designed as an offline-first desktop engineering tool. **The Software contains no background telemetry, no automated analytics, no user tracking, and no background network beacons.** Database update: Only file download requests are sent to GitHub; no personal data is transmitted.

Personal data is collected exclusively when the user **voluntarily and explicitly** chooses to submit a crash or feedback report, or contacts the developer directly via email:

1. **System & Environment Data:**
   * Software version (`CARGO_PKG_VERSION`)
   * Operating system (`OS`) and CPU architecture (`ARCH`)
   * Configured UI language (`TR` / `EN`)
   * Airframe platform type (fixed-wing, multirotor, VTOL quadplane, etc.)
   * Event timestamp (UTC)
2. **Crash & Diagnostic Logs:**
   * Source code panic location (file, line, and column)
   * Panic payload message
   * Call stack backtrace captured upon panic
3. **Application Logs:**
   * The most recent 200 lines from the local `%APPDATA%\WerSeuS\logs\app.log` file
4. **User-Provided Feedback & Optional Project Data:**
   * Free-text description entered by the user in the feedback dialog
   * Project file in JSON format (`project.json`), included only if the user explicitly checks the corresponding consent box
5. **Contact Information:**
   * Sender email address and name if the user submits the report via email

#### Automated Masking & Excluded Information:
Pursuant to the privacy implementation in `support.rs` (`mask_sensitive_info`):
* Local user profile paths (`%USERPROFILE%`, `X:\Users\<username>`, the `/`-separated form and the JSON-escaped form `X:\\Users\\<username>`) are automatically sanitized and replaced with `~`.
* Computer hostnames (`COMPUTERNAME`) are masked (`[HOSTNAME]`) or excluded.
* The following API key / token patterns are detected and replaced with `[REDACTED_API_KEY]`: `sk-` / `sk-ant-` keys (OpenAI, Anthropic; at least 16 characters), Google keys starting with `AIza` (at least 30 characters), `ai-` prefixed keys, `Bearer`, `x-api-key` and `x-goog-api-key` header values (at least 16 characters), and long (32+ characters) hexadecimal or letter-and-digit secret strings.
* These masking methods may not cover all forms of personal information; the user should review the report before sending it.
* When a long report is saved as a ZIP file, the automatic masking above is also applied to `report.txt` and `project.json` inside the ZIP. The ZIP file is still not attached automatically; the user attaches it manually and should review it before sending.
* **The Software does not intentionally collect, log, or add IP addresses, MAC addresses, or hardware serial numbers to reports.**

#### Artificial Intelligence Credentials:
If the user enables the optional AI assistant feature with an external provider (OpenAI-compatible, Anthropic, Google Gemini) or local Ollama, all API keys are stored locally in the Windows Credential Manager. Queries are transmitted directly from the user's computer to the selected provider. The developer does not route, proxy, inspect, or store any AI communications or credentials.

### 3. Purposes of Processing
Personal data is processed strictly for the following purposes:
* Diagnosing, reproducing, and resolving software defects, crashes, and unhandled panics,
* Improving the stability, security, and performance of the Software,
* Responding to user feedback, bug reports, and technical support requests,
* Ensuring the ongoing maintenance of both open-source and commercial releases.

### 4. Legal Bases for Processing (GDPR Art. 6(1))
* **Consent (GDPR Art. 6(1)(a)):** The primary legal basis for processing is the user's explicit and voluntary consent, granted when reviewing the report preview and choosing to send it via email or post it to GitHub.
* **Legitimate Interests (GDPR Art. 6(1)(f)):** The developer has a legitimate interest in analyzing fatal software exceptions to maintain the integrity, security, and stability of the Software, provided that user privacy is safeguarded through automated path masking and pre-transmission report review.

### 5. Recipients and Third-Party Processors
Data is never sold, leased, or transferred for commercial advertising or tracking. Transmission occurs solely based on the user's chosen submission channel:
1. **Google LLC (Gmail):** When the user chooses "Send by Email", the Software opens an email draft containing the report in the user's default email client. Once the user confirms sending, the report is transmitted via the user's email provider to `info.wsseng@gmail.com` (Gmail, operated by Google LLC).
2. **GitHub, Inc. (GitHub Issues):** When the user chooses to report via GitHub, the Software opens a pre-filled new-issue page in the browser. The report becomes a publicly visible issue only after the user submits it on GitHub. Users are advised to review the report to ensure no proprietary or sensitive personal information is included prior to submission.
3. **Selected AI Providers:** Direct interaction only if enabled by the user with their own account.

Google LLC and GitHub, Inc. are US-based companies; data sent to them may be processed outside Turkey and the EEA (international transfer under KVKK Art. 9 and GDPR Arts. 44–49, subject to legal review).

### 6. Retention and Erasure
* Reports received via email are retained for a maximum of **2 years** for regression analysis and patch validation, after which they are permanently deleted or anonymized, unless earlier erasure is requested.
* Issues published on GitHub are subject to GitHub's policies and remain publicly accessible.
* Users can completely erase all local data, logs, and configuration files on their machine at any time using the in-app uninstaller ("Programı Kaldır" wizard) or the `--uninstall` command, which cleans `%APPDATA%\WerSeuS`.
* Error report ZIP files are stored in `Documents\WerSeuS\reports`, outside `%APPDATA%`, and must be deleted separately by the user.

### 7. Data Subject Rights (GDPR Arts. 15–22)
Under the GDPR, you have the following rights regarding your personal data:
* **Right of Access (Art. 15):** The right to obtain confirmation and a copy of personal data processed.
* **Right to Rectification (Art. 16):** The right to have inaccurate or incomplete data corrected.
* **Right to Erasure / "To Be Forgotten" (Art. 17):** The right to have your personal data erased.
* **Right to Restriction of Processing (Art. 18):** The right to restrict data processing under certain conditions.
* **Right to Data Portability (Art. 20):** The right to receive your data in a structured, commonly used, and machine-readable format.
* **Right to Object (Art. 21):** The right to object to processing based on legitimate interests.
* **Right to Withdraw Consent (Art. 7(3)):** Where processing is based on consent, the right to withdraw consent at any time without affecting the lawfulness of prior processing.
* **Right to Lodge a Complaint (Art. 77):** The right to lodge a complaint with an EU data protection supervisory authority.

### 8. Contact & Exercise of Rights
To exercise any of your rights or request data erasure, please contact the developer at:
**Email:** `info.wsseng@gmail.com`
All requests will be handled promptly and free of charge within 30 days.
