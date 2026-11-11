# Yapay Zeka Asistanı Kullanım Kılavuzu / AI Assistant User Guide

[Türkçe](#türkçe-kullanım-kılavuzu) | [English](#english-user-guide)

---

## Türkçe Kullanım Kılavuzu

### 1. Giriş ve Temel İlkeler

WerSeuS – UAV Calc bünyesindeki Yapay Zeka Asistanı, İHA kavramsal tasarım sürecinde mühendis ve tasarımcılara doğal dil desteği, otomatik parametre optimizasyonu, adım bazlı açıklamalar ve tasarım uyarılarını giderme desteği sunan **tamamen isteğe bağlı** bir yardımcı modüldür.

Asistan aşağıdaki katı güvenlik ve kontrol prensipleriyle çalışır:
- **Varsayılan Olarak Kapalıdır:** Yapay zeka etkinleştirilmediği sürece hiçbir ağ çağrısı (HTTP/API) yapılmaz; yazılım tümüyle çevrimdışı çalışmaya devam eder.
- **Değerleri Kendiliğinden Asla Değiştirmez:** Asistan projedeki parametreleri tek taraflı olarak güncellemez. Tüm öneriler yapılandırılmış bir "Öneri Kartı" (`Proposal`) olarak listelenir (Parametre adı, eski değer → önerilen yeni değer, değişim gerekçesi). Kullanıcı dilerse tek tek seçerek ("Seçilenleri Uygula"), dilerse tümünü birden ("Tümünü Uygula") onaylayabilir.
- **Tek Tıkla Geri Alma (Undo):** Uygulanan tüm öneriler geri alma yığınına kaydedilir; "Geri Al" düğmesiyle anında önceki değerlere dönülebilir.
- **Fizik ve Sınır Denetimi:** Asistanın önerdiği tüm parametreler tek tek `AI_FIELDS` doğrulama süzgecinden (min/max aralıkları, tür denetimi) ve sihirbaz doğrulama kurallarından geçirilir. Tanınmayan veya fiziksel sınırları aşan değerler otomatik olarak reddedilir ve kullanıcıya gerekçesiyle bildirilir.
- **Mühendislik Doğrulaması Uyarısı:** Yapay zeka çıktıları istatistiki tahmin modellerine dayanır; aerodinamik, yapısal, itki ve stabilite hesaplarının mühendislik doğrulamasının, uçuş güvenliği testlerinin veya rüzgar tüneli/CFD analizlerinin yerini tutmaz.

---

### 2. Desteklenen Sağlayıcılar ve Kurulum

Yapay Zeka Ayarlarına üst menüden **Yapay Zeka > Ayarlar...** veya asistan panelinin üst köşesindeki dişli (⚙) simgesinden erişilebilir.

#### A. Ollama (Yerel ve Gizlilik Odaklı — Önerilen)
Bilgisayarınızda yerel olarak çalışan açık kaynaklı LLM motorudur. Tasarım verileriniz internete çıkmaz, tamamen cihazınızda işlenir.
- **Kurulum:**
  1. Ollama uygulamasını resmi kaynaklarından sisteminize indirip kurun.
  2. Komut istemcisi (Terminal / PowerShell) üzerinden dilediğiniz modeli çekin:
     ```bash
     ollama run qwen3:8b
     ```
     (İsteğe bağlı olarak `nemotron-3.5-lightning` veya diğer modeller de kullanılabilir).
  3. Ollama arka planda `http://127.0.0.1:11434` adresinde çalışacaktır.
  4. WerSeuS ayarlarında Sağlayıcı olarak **Ollama (Yerel / Local)** seçin. "Modelleri Getir" düğmesine basarak sisteminizde yüklü modelleri otomatik listeleyebilir ve seçebilirsiniz.

#### B. OpenAI Uyumlu Uç Noktalar
Yerel veya özel sunucularda çalışan OpenAI API standardındaki uç noktaları (LM Studio, vLLM, LocalAI, OpenRouter veya kurumsal API sunucuları) destekler.
- **Kurulum:**
  1. Sunucunuzun temel URL adresini girin (Örn: `http://localhost:1234/v1` veya servis sağlayıcınızın API URL'i).
  2. Model adını tanımlayın (Örn: `gpt-4o-mini`, `local-model` vb.).
  3. Gerekliyse API anahtarınızı girin.

#### C. Anthropic Claude
Anthropic Claude modelleri (`claude-sonnet-5-5` vb.) üzerinden yüksek akıl yürütme kabiliyeti sağlar.
- **Kurulum:**
  1. Anthropic konsolu üzerinden bir API erişim anahtarı temin edin.
  2. WerSeuS ayarlarında sağlayıcıyı Anthropic olarak belirleyin.
  3. API anahtarınızı yapıştırın.

#### D. Google Gemini
Google Gemini modelleri (`gemini-3.8-flash` vb.) üzerinden hızlı analiz ve optimizasyon desteği sunar.
- **Kurulum:**
  1. Google AI Studio platformundan bir API anahtarı üretin.
  2. WerSeuS ayarlarında Gemini sağlayıcısını seçip anahtarınızı kaydedin.

---

### 3. API Anahtarı ve Veri Güvenliği

- **Proje Dosyalarına Asla Yazılmaz:** WerSeuS `.json` uzantılı proje kayıt dosyalarına API anahtarlarını **asla** kaydetmez. Tasarım dosyalarınızı iş arkadaşlarınızla veya harici ekiplerle paylaştığınızda anahtarlarınızın sızma riski yoktur.
- **Saklama Yöntemleri:**
  - *Windows Kimlik Bilgisi Yöneticisi (Windows Credential Manager):* Anahtar işletim sisteminin güvenli kimlik kasasında saklanır. Uygulama yeniden açıldığında tekrar anahtar girmeniz gerekmez.
  - *Yalnızca Bu Oturum (Session-only Memory):* Anahtar yalnızca program açıkken RAM bellekte tutulur, disk sürücüsüne hiçbir iz bırakılmaz. Program kapatıldığında silinir.
- **Arayüz ve Günlük Güvenliği:** API anahtarları ekranda daima parola biçiminde maskelenir (●●●●). Uygulama günlüklerine, hata izlerine veya dışa aktarılan çökme raporlarına anahtarlar kesinlikle yazılmaz.
- **Bulut Veri Aktarımı ve Maliyet Bildirimi:** Bulut sağlayıcı (Anthropic, Gemini vb.) seçildiğinde, kullanıcının onayı alınmadan istek gönderilmez. İlk kullanımda bulut veri transferi ve API maliyet uyarısı onaylatılır.

---

### 4. Asistan Özellikleri

1. **Hızlı Tasarım (Quick Design):**
   - Asistan paneline doğal dille görevinizi yazın:
     *Örnek:* "1.2 kg kamera taşıyan, 45 dakika havada kalabilen, 18 m/s seyir hızında uçacak kompozit bir haritalama İHA'sı tasarla."
   - Asistan uygun İHA türünü seçer, kanat açıklığı, kanat alanı, batarya hücre konfigürasyonu, motor gücü ve pervane çapı için tutarlı başlangıç parametreleri önerir.
2. **Bu Adımı Açıkla / Sonuçları Yorumla (Explain Step):**
   - Hangi sekmede (Görev, Ağırlık, Geometri, Aerodinamik, İtki, Stabilite, Yapı vb.) bulunuyorsanız, tek tıkla mevcut girdi ve hesaplama sonuçlarını mühendislik mantığıyla yorumlar, darboğazları ve iyileştirme fırsatlarını açıklar.
3. **Uyarıyı Düzelt (Fix Warning):**
   - Özet (Summary) sekmesinde yer alan aerodinamik, itki veya ağırlık uyarılarının yanındaki `🤖 [Uyarıyı Düzelt]` düğmesine basıldığında, asistan o uyarının sebebini çözmek üzere ilgili tasarım parametresini revize eden karşı öneri oluşturur.

---

## English User Guide

### 1. Introduction & Core Principles

The AI Assistant in WerSeuS – UAV Calc is a **strictly optional** companion module engineered to support UAV designers and aeronautical engineers with natural language task formulation, automatic parameter trade-offs, step explanations, and design warning resolution.

Core Operating Principles:
- **Disabled by Default:** Zero network calls or HTTP background threads are initiated unless AI is explicitly toggled on by the user. The application remains 100% functional offline.
- **Non-Intrusive Proposals:** The AI never alters project parameters autonomously. All recommendations are presented in a structured "Proposal Card" (Field name, old value → recommended value, rationale). The user can selectively apply individual items or apply all with one click.
- **Full Undo Support:** Applied parameter modifications are pushed onto an in-memory undo stack, allowing instant rollback.
- **Strict Physical Range Validation:** Every suggested parameter is validated against the typed `AI_FIELDS` schema (enforcing valid bounds, units, and wizard constraints). Unknown fields or out-of-bounds suggestions are rejected and displayed as invalid.
- **Engineering Responsibility Disclaimer:** AI outputs are heuristic and statistical in nature. They do NOT replace rigorous aerodynamic/structural engineering verification, CFD, wind-tunnel experimentation, or flight safety checks.

---

### 2. Supported Providers & Setup

Settings are accessible from **AI > AI Settings...** in the top menu bar or via the gear icon (⚙) inside the assistant panel.

#### A. Ollama (Local & Privacy-Focused — Recommended)
Runs open-source LLMs locally on your workstation. Design parameters never leave your local machine.
- **Setup:**
  1. Install Ollama on your system.
  2. Pull and start your preferred model via CLI:
     ```bash
     ollama run qwen3:8b
     ```
  3. Ollama will listen locally at `http://127.0.0.1:11434`.
  4. In WerSeuS AI Settings, select **Ollama (Yerel / Local)**. Click "Fetch Models" to automatically enumerate available local models.

#### B. OpenAI-Compatible Endpoints
Supports local or remote servers conforming to the OpenAI Chat Completions API specification (LM Studio, vLLM, LocalAI, OpenRouter, etc.).
- Configure the Server Base URL (e.g. `http://localhost:1234/v1`).
- Enter the Model Identifier and optional API key.

#### C. Anthropic Claude
Enables high-order reasoning via Claude models (`claude-sonnet-5-5`, etc.).
- Obtain an API key from the Anthropic console.
- Enter the key and choose your model name.

#### D. Google Gemini
Provides fast cloud reasoning via Gemini models (`gemini-3.8-flash`, etc.).
- Obtain an API key from Google AI Studio.
- Enter the key and select the Gemini model name.

---

### 3. API Key & Security Guarantee

- **Never Stored in Project Files:** API credentials are **never** serialized into `.json` project files. Projects can be freely shared without secret leakage risk.
- **Secure Storage Options:**
  - *Windows Credential Manager:* Credentials are encrypted and stored in Windows Credential Store across reboots.
  - *Session-only Memory:* Credentials reside solely in process RAM and are discarded immediately upon application exit.
- **Masking & Log Scrubbing:** Keys are masked in the UI (●●●●) and strictly excluded from application logs, crash dumps, and bug reports.
- **Cloud Notice:** A one-time consent dialogue warns the user about cloud data transfer and external API billing prior to any remote request.

---

### 4. Assistant Capabilities

1. **Quick Design:** Express mission requirements in plain text (e.g., *"Design an electric fixed-wing mapping drone carrying a 1.2 kg camera with 45 min endurance at 18 m/s cruise speed"*). The AI proposes coherent dimensions, wing loading, battery size, and propulsion parameters.
2. **Explain Step / Interpret Results:** Instant contextual review of inputs, aerodynamic outputs, and margins for the currently active design tab.
3. **Fix Warning:** Direct 1-click remediation from the Summary tab warning list, prompting the assistant to calculate a compensatory parameter shift.
