TASLAK - hukukçu incelemesi önerilir / DRAFT - legal review recommended

# WerSeuS – UAV Calc
## Yasal Sorumluluk Reddi ve Kullanım Şartları / Legal Disclaimer

---

## Türkçe Bölüm: Sorumluluk Reddi Beyanı

Bu belge, "WerSeuS – UAV Calc" yazılımının ("Yazılım" veya "Program") kullanımına ilişkin yasal sorumluluk sınırlarını, teknik risk uyarılarını ve garanti koşullarını belirler.

### 1. Ön Tasarım ve Mühendislik Tahmini Niteliği
WerSeuS – UAV Calc; insansız hava araçlarının (sabit kanat, uçan kanat, multirotor, VTOL quadplane) aerodinamik, ağırlık merkezi, yapısal dayanım, batarya ve itki hesaplamalarını gerçekleştiren bir ön tasarım ve analiz aracıdır.

* **Ön Tasarım Tahmini:** Program tarafından hesaplanan tüm değerler, grafikler, kutup eğrileri, aerodinamik katsayılar ve bileşen önerileri **yalnızca ön tasarım ve kavramsal mühendislik tahmini** niteliğindedir.
* **Sertifika veya Resmi Onay Değildir:** Program çıktıları hiçbir koşulda resmi bir **Uçuşa Elverişlilik Sertifikası (Airworthiness Certificate / Approval)**, Tip Sertifikası (Type Certificate) veya yetkili mühendislik onay belgesi teşkil etmez.
* **Doğrulama Zorunluluğu:** Programdan elde edilen sonuçlar; gerçek statik yük testleri, rüzgar tüneli veya CFD analizleri, itki tezgahı ölçümleri ve kademeli yer/uçuş testleri ile yetkin bir havacılık uzmanı veya yetkili mühendis tarafından doğrulanmadan hava aracının nihai inşasında veya uçuşunda kullanılamaz. Geliştirici, sonuçların matematiksel mutlak doğruluğu veya her koşulda geçerliliği konusunda hiçbir garanti vermez.

### 2. Sivil Havacılık Mevzuatı ve Kullanıcı Yükümlülüğü
İHA tasarımı, inşası, donatımı ve işletilmesi katı yasal ve idari düzenlemelere tabidir.

* **Kullanıcı Sorumluluğu:** İHA'ların ve parçalarının üretimi, tescili, mülkiyeti, üçüncü şahıs mali mesuliyet sigortası, pilot lisansı, hava sahası uçuş izinleri ve operasyonel emniyeti ile ilgili tüm yasal yükümlülükler **münhasıran kullanıcıya aittir**.
* **Yerel ve Uluslararası Mevzuat Uyumu:** Kullanıcı; Türkiye Cumhuriyeti sınırları içerisinde **Sivil Havacılık Genel Müdürlüğü (SHGM)** tarafından yayımlanan güncel İHA mevzuat ve talimatlarına, Avrupa Birliği hava sahasında **Avrupa Havacılık Emniyeti Ajansı (EASA)** kurallarına ve diğer ülkelerde faaliyet gösterilen ilgili yetkili sivil havacılık otoritelerinin düzenlemelerine uymakla bizzat yükümlüdür. Geliştirici, hiçbir idari izin, mevzuat danışmanlığı veya yasal uyumluluk taahhüdü sağlamaz.

### 3. Donanım ve Teknik Güvenlik Riskleri

#### a. Lityum Batarya Güvenliği (LiPo / Li-ion):
Program, lityum bazlı batarya paketlerine ilişkin gerilim, deşarj derinliği (DoD) ve akım hesaplamaları sunabilir.
* Bu veriler yalnızca teorik referanstır.
* Lityum piller uygunsuz şarj, aşırı deşarj, mekanik darbe, kısa devre veya aşırı ısınma durumunda alev alma, patlama ve zehirli gaz salınımı gibi yüksek termal kaçak (thermal runaway) riskleri taşır.
* Kullanıcı; bataryaların şarj edilmesi, dengelenmesi (balancing), depolanması, yanmaz koruma çantalarında muhafaza edilmesi ve kullanımı esnasında tüm uluslararası güvenlik standartlarına ve üretici talimatlarına uymak zorundadır. Geliştirici, batarya kaynaklı yangın, patlama, yaralanma veya maddi hasarlardan sorumlu tutulamaz.

#### b. 3D Baskı Parçaların Yapısal Sınırları:
Program; hafifletilmiş basılı kabuk (LW-PLA, PLA, PETG vb.) ve karbon boru ana taşıyıcı (spar) gerilme hesaplamaları sunmaktadır.
* 3D yazıcı ile katmanlı üretilen parçaların dayanımı; kullanılan nozül sıcaklığı, katman yapışması, ekstrüzyon çarpanı, ortam nemi ve filament kalitesine aşırı derecede duyarlıdır.
* Basılı parçalar katman ayrılması (delamination), sürünme (creep), UV bozulması ve titreşim kaynaklı yorulma risklerine açıktır.
* Programın ürettiği et payı veya güvenlik katsayısı tahminleri hiçbir yapısal parçanın havada kırılmayacağını garanti etmez. Uçuş öncesi fiziksel eğilme, burulma ve titreşim testlerinin icrası kullanıcının sorumluluğundadır.

### 4. Yapay Zeka Asistanı Çıktıları
Uygulama bünyesinde sunulan veya harici entegre edilen yapay zeka asistanı özellikleri deneysel niteliktedir.
* Yapay zeka modelleri tarafından üretilen tasarım parametreleri, kodlar, formül açıklamaları veya mühendislik önerileri yanıltıcı, hatalı, eksik veya havacılık emniyeti açısından sakıncalı olabilir.
* Kullanıcı, yapay zeka tarafından sağlanan hiçbir veriyi bağımsız teknik doğrulama yapmaksızın uygulamamalıdır. Geliştirici, yapay zeka çıktılarından kaynaklanabilecek doğrudan veya dolaylı kayıplardan sorumlu tutulamaz.

### 5. AGPL-3.0 Lisansı Kapsamında Garanti Reddi ve Sorumluluk Sınırlaması
WerSeuS – UAV Calc, **GNU Affero General Public License v3 veya üzeri (AGPL-3.0-or-later)** lisansı altında dağıtılmaktadır.

* **Garanti Reddi (AGPL-3.0 Madde 15 - Disclaimer of Warranty):**
  Yürürlükteki mevzuatın izin verdiği azami ölçüde, Program "OLDUĞU GİBİ" ("AS IS") sağlanmaktadır. Telif hakkı sahipleri ve katkıcılar; SATILABİLİRLİK VE BELİRLİ BİR AMACA UYGUNLUK DAHİL ANCAK BUNLARLA SINIRLI OLMAMAK ÜZERE, AÇIK VEYA ZIMNİ HİÇBİR GARANTİ VERMEZ. Programın kalitesi, doğruluğu, performansı ve kullanımından kaynaklanabilecek tüm riskler kullanıcıya aittir. Programın kusurlu olduğunun anlaşılması durumunda, gerekli tüm servis, tamir veya düzeltme masrafları kullanıcı tarafından karşılanır.
* **Sorumluluk Sınırlaması (AGPL-3.0 Madde 16 - Limitation of Liability):**
  Telif hakkı sahipleri veya Programı yukarıda izin verildiği şekilde değiştiren ve/veya dağıtan herhangi bir taraf; haksız fiil, sözleşme veya başka bir hukuki nedene dayansın, bu tür zararların olasılığı önceden bildirilmiş olsa dahi, Programın kullanımından veya kullanılamamasından kaynaklanan GENEL, ÖZEL, ARIZİ, CEZAİ, DOĞRUDAN VEYA DOLAYLI HİÇBİR ZARARDAN (veri kaybı, verilerin yanlış işlenmesi, hava aracı kaza veya kırım zararları, kişisel yaralanma, üçüncü şahıslara veya mülklere verilen zararlar, kâr veya gelir kaybı, iş kesintisi dahil ancak bunlarla sınırlı olmamak üzere) SORUMLU TUTULAMAZ.

### 6. Veri Gizliliği ve İletişim
* Uygulama otomatik telemetri içermez. Hata ve geri bildirim raporları yalnızca kullanıcının açık onayı ile gönderilir.
* Rapor gönderimi sırasında kişisel dizinler ve `sk-` önekli API anahtarları otomatik olarak maskelenir (maskeleme tüm biçimleri kapsamaz; kullanıcı raporu göndermeden önce incelemelidir); uygulama bilgisayar adı ve IP adresini rapora eklemez.
* İletişim ve destek: `info.wsseng@gmail.com`

---

## English Section: Legal Disclaimer and Limitation of Liability

This document defines the technical risk warnings, regulatory compliance boundaries, and liability limitations applicable to the use of the "WerSeuS – UAV Calc" software ("the Software" or "the Program").

### 1. Preliminary Design & Engineering Estimation Nature
WerSeuS – UAV Calc is a conceptual engineering analysis and sizing tool for small unmanned aerial vehicles (fixed-wing, flying wing, multirotor, and hybrid VTOL quadplanes).

* **Preliminary Estimates Only:** All calculations, graphs, drag polars, aerodynamic parameters, weight breakdowns, and component estimates produced by the Program are **strictly preliminary conceptual estimates**.
* **Not an Airworthiness Certificate:** The output of this Program does **not** constitute an Airworthiness Certificate, Type Certificate, Engineering Approval, or official flight clearance of any kind.
* **Independent Verification Required:** The data obtained from this Program must not be relied upon for flight without comprehensive independent verification by a qualified aerospace engineer, supported by physical static load testing, thrust bench validation, CFD or wind tunnel analysis, and gradual ground and flight testing. The developer makes no representation or warranty regarding the absolute mathematical accuracy or operational safety of the calculated outputs.

### 2. Civil Aviation Regulatory Compliance & User Responsibility
The design, construction, modification, and operation of unmanned aerial systems are governed by strict national and international aviation laws.

* **User Responsibility:** The user assumes **sole and exclusive legal and operational responsibility** for airframe fabrication, registration, serial identification, third-party liability insurance, remote pilot certification, airspace authorizations, and flight safety.
* **Jurisdictional Compliance:** The user must strictly comply with all applicable civil aviation regulations, including the regulations issued by the **Directorate General of Civil Aviation (SHGM)** in Turkey, the rules of the **European Union Aviation Safety Agency (EASA)** in the European Union, and the competent national aviation authorities in other jurisdictions. The developer provides no regulatory compliance verification, administrative filings, or legal counsel.

### 3. Hardware Safety and Technical Hazards

#### a. Lithium Battery Safety (LiPo / Li-ion):
The Program provides theoretical battery pack energy, C-rating, and voltage calculations.
* These values are purely mathematical estimations.
* Lithium-based batteries carry severe hazards of thermal runaway, toxic fume emission, fire, and explosion resulting from overcharging, excessive current draw, deep discharge, physical puncture, or elevated temperature.
* The user is solely responsible for implementing adequate fireproof containment, active cell balancing, temperature monitoring, and compliant charging/storage protocols in line with international battery safety standards. The developer disclaims all liability for battery fire, explosion, bodily injury, or property damage.

#### b. Structural Limits of 3D Printed Airframes:
The Program calculates skin weight and carbon fiber tube spar bending stresses (e.g., using lightweight foaming PLA, standard PLA, PETG, etc.).
* The mechanical strength of fused filament fabrication (FFF/3D printing) components depends heavily on printing temperature, layer-to-layer adhesion, flow rate, ambient humidity, and extrusion alignment.
* Printed airframe parts are susceptible to anisotropic weakness, inter-layer delamination, mechanical creep under tension, ultraviolet degradation, and vibration fatigue.
* Structural safety margins calculated by the Software do not guarantee that parts will not suffer catastrophic in-flight structural failure. The user must conduct empirical destructive and non-destructive load tests prior to flight.

### 4. Artificial Intelligence Assistant Advice
Any artificial intelligence functionality bundled with or connected to the Software is purely experimental.
* Recommendations, formulas, source code snippets, or parameters generated by AI models may be erroneous, incomplete, hallucinated, or aerodynamically unsafe.
* Users must critically review and verify all AI-generated advice through professional engineering judgment before applying it to physical airframes. The developer assumes no liability for damages arising from AI suggestions.

### 5. AGPL-3.0 Disclaimer of Warranty and Limitation of Liability
WerSeuS – UAV Calc is distributed under the **GNU Affero General Public License version 3 or later (AGPL-3.0-or-later)**.

* **Disclaimer of Warranty (AGPL-3.0 Section 15):**
  TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, THE PROGRAM IS PROVIDED "AS IS" WITHOUT WARRANTY OF ANY KIND, EITHER EXPRESSED OR IMPLIED, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE. THE ENTIRE RISK AS TO THE QUALITY, SAFETY, AND PERFORMANCE OF THE PROGRAM IS WITH THE USER. SHOULD THE PROGRAM PROVE DEFECTIVE, THE USER ASSUMES THE COST OF ALL NECESSARY SERVICING, REPAIR, OR CORRECTION.
* **Limitation of Liability (AGPL-3.0 Section 16):**
  IN NO EVENT UNLESS REQUIRED BY APPLICABLE LAW OR AGREED TO IN WRITING WILL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY WHO MODIFIES AND/OR CONVEYS THE PROGRAM AS PERMITTED UNDER THE LICENSE, BE LIABLE TO THE USER FOR DAMAGES, INCLUDING ANY GENERAL, SPECIAL, INCIDENTAL, PUNITIVE, DIRECT, OR CONSEQUENTIAL DAMAGES ARISING OUT OF THE USE OR INABILITY TO USE THE PROGRAM (INCLUDING BUT NOT LIMITED TO LOSS OF DATA, INACCURATE CALCULATIONS, AIRFRAME DAMAGE, HARDWARE LOSS, THIRD-PARTY PROPERTY DAMAGE, PERSONAL INJURY, LOSS OF PROFITS, BUSINESS INTERRUPTION, OR LOSS SUSTAINED BY THE USER OR THIRD PARTIES), EVEN IF SUCH HOLDER OR OTHER PARTY HAS BEEN ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

### 6. Data Privacy and Inquiries
* The Software operates locally with no background telemetry. Diagnostic crash logs and feedback are transmitted only upon express user confirmation.
* Automated privacy masking removes local user directory paths and `sk-` prefixed API keys (masking may not cover all forms of personal information; users should review reports before sending). The Software does not intentionally add IP addresses or computer names to reports.
* Inquiries and technical reports: `info.wsseng@gmail.com`
