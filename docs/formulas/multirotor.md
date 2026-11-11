> **Doğrulama Durumu**: Bu belgedeki multirotor ve momentum teorisi formülleri Leishman helikopter aerodinamiği literatürü ile doğrulanmış, 250 mm ve 275 mm X-Quad referans tasarımlarıyla birim testlerde teyit edilmiştir.

# Multirotor ve VTOL Ön Tasarım Formül Spesifikasyonu (SI Birimleri)

## Düzeltmeler (Doğrulama Bulguları Revizyonu)

Bu spesifikasyon, bağımsız doğrulama bulguları doğrultusunda aşağıdaki kritik düzeltmelerle güncellenmiştir:

1. **Ağırlık Merkezi (AG) Referansı:** Multirotor için burun datumu yerine çerçeve geometrik merkezi $(0,0)$ referanslı ayrı bileşen tablosu tanımlandı. Simetrik motor ve çerçeve kütleleri otomatik eklenir; AG sapması $\Delta_{cg}$ bu tablodan hesaplanır.
2. **Doğrusal Olmayan Askı Gazı:** Askı gazı doğrusal $1/TW$ yerine itki-hız karesel ilişkisini yansıtan $\text{Throttle}_{hover} \approx \sqrt{1/TW}$ olarak güncellendi.
3. **Tutarlı Momentum ve Güç Modeli:** İdeal ve mil gücü hesaplarında kol başına tutarlı model: Kol başına itki $T_{kol} = W / n_{kol}$, koaksiyel çiftte ideal güç $P_{ideal} = (T_{kol})^{1.5} / \sqrt{2 \rho A} \times \kappa_{int}$, mil gücü $P_{shaft} = P_{ideal} / FM$, elektrik gücü $P_{elec} = P_{shaft} / (\eta_{mot} \eta_{esc})$.
4. **Disk Alanı:** Koaksiyel çiftlerde aerodinamik süpürme alanı olarak kol başına bir disk alanı $A = \frac{\pi}{4} D_{prop}^2$ esas alınır.
5. **Girişim Güç Çarpanı ($\kappa_{int}$):** Sabit 0.85 değeri ve Leishman denklem 7.28 hatalı atfı kaldırıldı; kullanıcı tarafından girilebilir $\kappa_{int}$ (varsayılan 1.28, "varsayım; ölçümle doğrulayın") parametresi tanımlandı.
6. **Şematik Koaksiyel Çizimi:** Y6/X8 koaksiyel konfigürasyonlarında her kolda üst ve alt motorlar ($M_1 \dots M_n$) ayrı halkalarla görselleştirilir.
7. **H-Düzeni Parametreleri:** Boyuna ($L_x$) ve enine ($W_y$) motor aralıkları bağımsız kullanıcı girdisi yapıldı; $n=3$ için $D_{frame}$ ölçü çizgisi karşıt motor varsayımı yerine gerçek merkezden geçen çapı gösterecek şekilde düzeltildi.
8. **İleri Uçuş Menzil Fiziği:** Sabit %85 askı gücü kaba tahmininin yanına Glauert iteratif indüklenmiş hız çözümü, gövde parazit sürükleme gücü ($P_{para} = \frac{1}{2} \rho V^3 f$) ve maksimum menzil hızının sayısal araması eklendi.
9. **Aritmetik Düzeltmeler (§8.3):** Yuvarlama ve formül tutarsızlıkları giderildi ($P_{elec,hover} = 166.059\text{ W}$, $P_{hover} = 171.059\text{ W}$, $P_{elec,motor} = 41.515\text{ W}$, $P_{cruise} = 146.150\text{ W}$).

---

Bu spesifikasyon, **WerSeuS – UAV Calc** projesinde multirotor ve VTOL dikey kaldırma sistemlerinin ön tasarımı için kullanılacak doğrulanmış matematiksel modelleri, formülleri ve birim test vektörlerini tanımlar.

Tüm denklemler saf fonksiyon olarak SI birimlerinde ($m, kg, s, N, W, rad, Pa$) uygulanır. Kullanıcı arayüzünde pratik birimler ($mm, g, inç, dk, km/h$) gösterilir.

---

## Literatür ve Kaynaklar

1. **J. Gordon Leishman**, *Principles of Helicopter Aerodynamics*, 2nd Edition, Cambridge University Press, 2006.
   - Bölüm 2: Momentum Teorisi (Momentum Theory and Wake Analysis in Hover).
   - Denklem (2.13) ve (2.16): İndüklenmiş hız $v_i = \sqrt{T / (2 \rho A)}$ ve ideal askı gücü $P_{ideal} = T v_i = T^{1.5} / \sqrt{2 \rho A}$.
   - Bölüm 2.14: Kalite Faktörü (Figure of Merit) $FM = P_{ideal} / P_{shaft} \approx 0.60 - 0.75$.
   - Bölüm 3.4 & 7.3: Koaksiyel rotorlarda aerodinamik etkileşim ve indüklenmiş güç artış çarpanı $\kappa_{int} \approx 1.28$ (girişim faktörü, kullanıcı girdisi, varsayım; ölçümle doğrulayın).
   - Bölüm 3.2: İleri uçuşta translasyonel kaldırma (translational lift / ETL) ve Glauert indüklenmiş akış denklemleri.

2. **T. Bresciani**, *Modelling, identification and control of a quadrotor helicopter*, Lund University, 2008.
   - Bölüm 2.1: Gövde geometrisi ve motor koordinat dönüşümleri (X ve + konfigürasyonları).
   - Bölüm 2.2: İtki, rotor torku ve motor dinamikleri, gaz kelebeği - itki ilişkisi.

3. **Quan Quan**, *Introduction to Multicopter Design and Control*, Springer, 2017.
   - Bölüm 3: Çok Rotorlu İHA Yapısal Tasarımı ve Bileşen Seçimi.
   - Denklem (3.1) - (3.6): Çerçeve boyutu ($D_{frame}$), komşu motor aralığı, pervane boşluğu ve azami pervane çapı.
   - Bölüm 3.3: İtki-ağırlık oranı ($T/W$) gereksinimleri (genel amaçlı $\ge 2.0$, yarış/akrobasi $\ge 3.0 - 4.0$).
   - Bölüm 3.5: Batarya enerjisi, boşalma derinliği (DoD) ve havada kalma süresi hesabı.
   - Bölüm 4.2: Ağırlık merkezi (CG) eksantrisitesi ve motor yük dengesizliği toleransı ($< \%3 D_{frame}$).

---

## 1. Çerçeve Geometrisi ve Düzen

### 1.1 Semboller ve Birimler

| Sembol | Tanım | Birim | Varsayılan | Kaynak |
|---|---|---|---|---|
| $n$ | Kol sayısı (Arms) | - | 4 | Quan (2017) |
| $n_{motors}$ | Toplam motor sayısı | - | 4 | Quan (2017) |
| $D_{frame}$ | Çerçeve motor-motor çapraz mesafesi | $m$ ($mm$) | 0.250 m | Quan Bölüm 3 |
| $L_x$ | H düzeni ön-arka motor aralığı | $m$ ($mm$) | $D_{frame}/\sqrt{2}$ | Kullanıcı girdisi |
| $W_y$ | H düzeni sol-sağ motor aralığı | $m$ ($mm$) | $D_{frame}/\sqrt{2}$ | Kullanıcı girdisi |
| $c_{layout}$ | Düzen (X, Plus (+), H) | enum | X | Bresciani (2008) |
| $is_{coax}$ | Koaksiyel motor seçeneği (Y6, X8) | bool | false | Leishman Bölüm 7 |
| $\kappa_{int}$ | Koaksiyel girişim güç çarpanı | - | 1.28 | Kullanıcı girdisi (varsayım) |
| $f$ | Gövde eşdeğer parazit düz plaka alanı | $m^2$ | 0.010 m² | Kullanıcı girdisi |
| $\delta_{clear}$ | Komşu pervane ucu güvenlik boşluğu | $m$ ($mm$) | 0.010 m (10 mm) | Quan Bölüm 3 |
| $D_{prop}$ | Seçili pervane çapı | $m$ (inç) | 0.127 m (5 inç) | Standart bileşen |
| $R_{plate}$ | Gövde merkez plakası yarıçapı | $m$ ($mm$) | 0.035 m (35 mm) | 3D baskı boyutu |

### 1.2 Denklemler

1. **Komşu Motor Merkezleri Arasındaki Mesafe ($S_{adj}$):**
   Eşit aralıklı $n$ kollu dairesel yerleşimde (X veya +):
   $$S_{adj} = D_{frame} \cdot \sin\left(\frac{\pi}{n}\right)$$
   - 4 kollu (Quad) için: $S_{adj} = D_{frame} \cdot \sin(\pi/4) = D_{frame} \cdot \frac{\sqrt{2}}{2} \approx 0.70711 \cdot D_{frame}$.
   - 6 kollu (Hexa) için: $S_{adj} = D_{frame} \cdot \sin(\pi/6) = 0.50000 \cdot D_{frame}$.
   - 8 kollu (Octo) için: $S_{adj} = D_{frame} \cdot \sin(\pi/8) \approx 0.38268 \cdot D_{frame}$.
   - 3 kollu (Tricopter/Y6) için: $S_{adj} = D_{frame} \cdot \sin(\pi/3) \approx 0.86603 \cdot D_{frame}$.

   *Not 1:* H-Quad düzeninde kullanıcı $L_x$ ve $W_y$ değerlerini bağımsız belirler; çerçeve diyagonal boyutu $D_{frame} = \sqrt{L_x^2 + W_y^2}$ olur. En kısa komşu mesafe $S_{adj} = \min(L_x, W_y)$ alınır.
   *Not 2:* Tek kollu (ör. $n=3$) konfigürasyonlarda şematik $D_{frame}$ ölçü çizgisi karşıt kolda motor varmış gibi varsaymaz; merkez noktasından karşıya uzanan gerçek çapraz çember çapını ($2 \cdot R_{arm}$) gösterir.

2. **Azami Pervane Çapı ($D_{prop,max}$):**
   Pervanelerin birbirine çarpmaması için güvenlik boşluğu çıkarılır:
   $$D_{prop,max} = S_{adj} - \delta_{clear}$$
   İnç cinsinden:
   $$D_{prop,max,inch} = \frac{D_{prop,max}}{0.0254}$$

3. **Kol Uzunluğu ($L_{arm}$):**
   Merkez plaka sınırından motor merkezine kadar olan serbest kol uzunluğu:
   $$L_{arm} = \frac{D_{frame}}{2} - R_{plate}$$

4. **3D Baskı Tabla Kısıtı:**
   Kobra S1 tablası $250 \times 250\text{ mm}$ (marjla $240\text{ mm}$).
   Eğer $L_{arm} > 0.240\text{ m}$ veya $2 \cdot R_{plate} > 0.240\text{ m}$ ise:
   Bölünmüş (segmentli) baskı uyarısı verilir.

---

## 2. Kütle Dökümü (Mass Breakdown)

Toplam kalkış kütlesi ($m_{mtow}$):
$$m_{mtow} = m_{frame} + n_{motors} \cdot m_{motor} + m_{esc\_tot} + m_{fc} + m_{batt} + m_{payload} + m_{misc}$$

Burada:
- $m_{frame}$: Çerçeve karbon fiber / 3D baskı plaka ve kollar ($kg$).
- $m_{motor}$: Tek motor kütlesi ($kg$).
- $m_{esc\_tot}$: ESC grubu kütlesi (bireysel $n_{motors} \cdot m_{esc}$ veya 4-in-1 kartı).
- $m_{fc}$: Uçuş kontrolcüsü, GPS, PDB ve alıcı ($kg$).
- $m_{batt}$: Batarya paketi kütlesi ($kg$), hücre veritabanından ($n_s \times n_p \times m_{cell} \times k_{pack}$).
- $m_{payload}$: Kamera, gimbal veya sensör yükü ($kg$).
- $m_{misc}$: Kablolama, vidalar, konektörler ve iniş takımı ($kg$).

Toplam ağırlık:
$$W = m_{mtow} \cdot g \quad (g = 9.80665\text{ m/s}^2)$$

---

## 3. Momentum Teorisi ve Askı Aerodinamiği

### 3.1 Rotor Alanı ve Kol Başına İtki Dağılımı

Tek bir pervanenin disk süpürme alanı:
$$A = \frac{\pi}{4} D_{prop}^2$$

Çok rotorlu sistemde ağırlık tüm kollara eşit dağıtılır ($W = m_{mtow} \cdot g$). Kol başına düşen askı itkisi:
$$T_{arm} = \frac{W}{n_{arms}}$$

Disk yüklemesi kol başına tek disk alanı $A$ esas alınarak tanımlanır:
$$DL = \frac{W}{n_{arms} \cdot A} = \frac{T_{arm}}{A} \quad [N/m^2]$$

Toplam fiziksel motor sayısı $n_{motors}$:
- Tek rotorlu kol: $n_{motors} = n_{arms}$.
- Koaksiyel kol (Y6, X8): $n_{motors} = 2 \cdot n_{arms}$.

### 3.2 İdeal Askı Gücü (Tutarlı Kol/Rotor Modeli)

Momentum teorisine göre tek bir rotorun kayıpsız indüklenmiş hızı $v_i = \sqrt{T / (2 \rho A)}$ ve ideal gücü $P_{ideal} = T^{1.5} / \sqrt{2 \rho A}$'dır.

Kol başına ideal askı gücü:
1. **Standart (Koaksiyel Olmayan) Kol:**
   $$P_{ideal,arm} = \frac{T_{arm}^{1.5}}{\sqrt{2 \rho A}}$$
2. **Koaksiyel Rotor Çifti (Y6, X8):**
   Koaksiyel düzende alt rotor üst rotorun izinde çalıştığından ilave indüklenmiş güç kaybı oluşur. Bu kayıp aerodinamik girişim güç çarpanı $\kappa_{int}$ ile modellenir (kullanıcı girdisi, varsayılan 1.28, "varsayım; ölçümle doğrulayın"):
   $$P_{ideal,arm} = \frac{T_{arm}^{1.5}}{\sqrt{2 \rho A}} \cdot \kappa_{int}$$

Tüm kolların toplam ideal gücü:
$$P_{ideal,tot} = n_{arms} \cdot P_{ideal,arm}$$

### 3.3 Mil Gücü ve Kalite Faktörü (Figure of Merit - FM)

Profil sürtünmesi ve uç girdap kayıpları rotor kalite faktörü ($FM \approx 0.65$) ile modellenir:
$$P_{shaft,tot} = \frac{P_{ideal,tot}}{FM}$$
Motor başına mekanik mil gücü:
$$P_{shaft,motor} = \frac{P_{shaft,tot}}{n_{motors}}$$

### 3.4 Elektrik Gücü ve Verimler

Fırçasız DC motor verimi $\eta_{motor}$ (varsayılan 0.85) ve elektronik hız kontrolcü verimi $\eta_{esc}$ (varsayılan 0.95):
$$\eta_{elec} = \eta_{motor} \cdot \eta_{esc} \approx 0.85 \times 0.95 = 0.8075$$

Tüm motorların toplam elektrik askı gücü:
$$P_{elec,hover,tot} = \frac{P_{shaft,tot}}{\eta_{elec}}$$

Motor başına düşen elektrik gücü:
$$P_{elec,motor} = \frac{P_{elec,hover,tot}}{n_{motors}}$$

---

## 4. İtki/Ağırlık Oranı ve Askı Gaz Yüzdesi

### 4.1 Azami İtki ve $T/W$ Oranı

Tek motorun üretebileceği azami statik itki $T_{max,motor}$ (kullanıcı girdisi veya motor spec tablosu).
Toplam azami itki:
$$T_{max,tot} = n_{motors} \cdot T_{max,motor}$$

İtki / Ağırlık Oranı:
$$TW = \frac{T_{max,tot}}{m_{mtow} \cdot g}$$

Tasarım kuralları (Quan 2017, Bölüm 3.3):
- Genel amaçlı / kamera platformu: $TW \ge 2.0$ (rüzgar direnci ve kontrol otoritesi için).
- Yarış / akrobasi: $TW \ge 3.0 - 5.0$.
- Uyarı eşiği: $TW < 1.6$ ise acil uyarı (kontrol kaybı ve alçalırken vortex ring riski).

### 4.2 Askı Gaz Yüzdesi Tahmini

Rotor itkisi açısal hızın karesi ($\omega^2$) ve gaz kelebeği sinyali ile karesel orantılı olduğundan, doğrusal yaklaşım ($1/TW$) yerine doğrusal olmayan karekök yaklaşımı kullanılır:
$$\text{Throttle}_{hover} \approx \sqrt{\frac{T_{hover,tot}}{T_{max,tot}}} = \sqrt{\frac{1}{TW}}$$
Yüzde olarak: $\% \text{Throttle} = \text{Throttle}_{hover} \times 100$.

*Not:* Bu formül fırçasız motor ve pervane eşleşmesi için yaklaşık bir tahmindir; motorun gerçek itki-gaz eğrisine göre kalibre edilmelidir.
*Uyarı eşiği:* $\text{Throttle}_{hover} > \%60$ ise uyarı (motorlar aşırı ısınıp manevra payı kalmaz).

---

## 5. Enerji, Havada Kalma ve İleri Uçuş Menzili

### 5.1 Kullanılabilir Batarya Enerjisi

Batarya paketi nominal enerjisi:
$$E_{pack} = n_s \cdot V_{cell,nom} \cdot n_p \cdot C_{cell,Ah} \quad [Wh]$$
Kullanılabilir enerji (boşalma derinliği $DoD \approx 0.80$ ile):
$$E_{usable} = E_{pack} \cdot DoD \quad [Wh]$$

### 5.2 Toplam Askı Gücü ve Havada Kalma Süresi

Sistem toplam elektrik gücü, motorlara giden güç ile aviyonik sabit gücünün toplamıdır:
$$P_{tot,hover} = P_{elec,hover,tot} + P_{avionics} \quad [W]$$

Askıda kalma süresi (Hover Endurance):
$$t_{hover} = \frac{E_{usable}}{P_{tot,hover}} \quad [saat]$$
Dakika cinsinden:
$$t_{hover,min} = t_{hover} \cdot 60$$

### 5.3 İleri Uçuş Gücü ve Menzil Modeli

Multirotor ileri uçuşa geçtiğinde pervaneler temiz hava akışına girer (Effective Translational Lift - ETL) ve indüklenmiş güç azalır; ancak gövde parazit sürükleme gücü hızın küpüyle ($V^3$) artar.

1. **Ana Model — Glauert İndüklenmiş Hız, Rotor Profil Gücü ve Parazit Sürükleme Modeli:**
   İleri uçuşta $V$ hızındaki indüklenmiş hız $v_i$, Glauert denkleminin iteratif çözümüyle bulunur:
   $$D_{parasite} = \frac{1}{2} \rho V^2 f$$
   $$T_{req} = \sqrt{W^2 + D_{parasite}^2}$$
   $$v_{i0}^2 = \frac{T_{req}}{2 \rho A_{tot}}$$
   İterasyon ($k = 0 \dots 8$ adım):
   $$v_{i,k+1} = \frac{v_{i0}^2}{\sqrt{V^2 + v_{i,k}^2}}$$
   - İndüklenmiş güç: $P_{ind}(V) = T_{req} \cdot v_i \cdot (\text{koaksiyel ise } \kappa_{int} \text{ aksi halde } 1.0)$
   - Parazit gövde gücü: $P_{par}(V) = D_{parasite} \cdot V = \frac{1}{2} \rho V^3 f$ (burada $f$ eşdeğer parazit düz plaka alanıdır, varsayılan $0.010\text{ m}^2$).
   - Profil gücü ($P_{profile}$): Rotor verisi olmadığından kanat profil sürüklemesi askı mil gücünün sabit bir yüzdesi olarak modellenir ($k_{prof}$ kullanıcı girdisi, varsayılan %15):
     $$P_{profile} = k_{prof} \cdot P_{shaft,tot}$$
   - Toplam mekanik mil gücü:
     $$P_{shaft}(V) = \frac{P_{ind}(V)}{FM} + P_{profile} + P_{par}(V)$$
   - Toplam elektrik gücü:
     $$P_{tot,cruise}(V) = \frac{P_{shaft}(V)}{\eta_{elec}} + P_{avionics}$$
   - İleri uçuş süresi ve menzil:
     $$t_{cruise} = \frac{E_{usable}}{P_{tot,cruise}(V_{cr})} \quad [saat]$$
     $$\text{Range}_{km} = V_{cr} \cdot t_{cruise} \cdot 3.6$$

   Sayısal azami menzil hızı $V_{best}$, birim enerji başına mesafeyi maksimize eden hızdır ($V \in [5, 35]\text{ m/s}$):
   $$V_{best} = \arg\max_{V} \left( \frac{V}{P_{tot,cruise}(V)} \right)$$

2. **Kaba Tahmin (Eski Model / Karşılaştırma):**
   $$P_{tot,cruise,85} = 0.85 \cdot P_{elec,hover,tot} + P_{avionics} \quad [W]$$
   *(Not: Bu model erken aşama ön tasarım için kaba bir tahmindir; düz uçuşta ETL etkisini yaklaşık temsil eder ancak hızdan ve gövde parazit alanından bağımsızdır).*

---

## 6. Ağırlık Merkezi (CG) Eksantrisitesi

Multirotor tasarımlarında referans koordinat sistemi çerçeve geometrik merkezidir ($(0,0)$, $x$ buruna doğru ileri pozitif, $y$ sağa doğru pozitif).

Multirotor bileşen tablosunda pil, uçuş kontrol kartı (FC), faydalı yük/kamera, ESC ve alıcı konumları çerçeve merkezine göre tanımlanır.
- Çerçeve yapı kütlesi $m_{frame}$ otomatik olarak $(0,0)$ noktasında yer alır.
- Motorlar geometrik kol koordinatlarında $(x_{motor,i}, y_{motor,i})$ otomatik olarak simetrik eklenir.

Toplam kütle ve ağırlık merkezi:
$$m_{tot} = \sum_{i} m_i, \quad x_{cg} = \frac{1}{m_{tot}} \sum_{i} m_i x_i, \quad y_{cg} = \frac{1}{m_{tot}} \sum_{i} m_i y_i$$

Çerçeve merkezinden kaçıklık mesafesi ($d_{cg}$):
$$d_{cg} = \sqrt{x_{cg}^2 + y_{cg}^2}$$

Eksantrisite oranı:
$$\Delta_{cg} = \frac{d_{cg}}{D_{frame}}$$

*Kural (Quan 2017 Bölüm 4.2):* $\Delta_{cg} > \%3$ ise ($d_{cg} > 0.03 \cdot D_{frame}$) uyarı verilir:
> "Ağırlık merkezi çerçeve merkezinden > %3 uzakta. Motorlar dengesiz yüklenir ve kontrol marjı düşer."

---

## 7. Sistem Uyarıları ve Tasarım Limitleri

| No | Kriter | Şart | Seviye | Uyarı Metni |
|---|---|---|---|---|
| 1 | Pervane Çakışması | $D_{prop} > S_{adj}$ | Hata | "Pervaneler fiziksel olarak çakışıyor! Pervane çapını küçültün veya çerçeveyi büyütün." |
| 2 | Pervane Boşluğu | $D_{prop} > D_{prop,max}$ | Uyarı | "Pervane uç boşluğu yetersiz (< 10 mm). Gövde titreşimi ve aerodinamik parazit artar." |
| 3 | Düşük İtki/Ağırlık | $TW < 1.6$ | Hata | "İtki/ağırlık oranı < 1.6! Rüzgarda kontrol kaybı ve sert iniş riski." |
| 4 | Yüksek Askı Gazı | $\text{Throttle}_{hover} > 0.60$ | Uyarı | "Askı gaz yüzdesi > %60. Motorlar ısınabilir ve ani kaçış manevrası için pay az." |
| 5 | Batarya Akım Aşımı | $I_{hover} > I_{pack,cont}$ | Hata | "Askı akımı bataryanın sürekli deşarj sınırını aşıyor! Hücre sayısı/C oranını artırın." |
| 6 | Ağırlık Merkezi Kayması | $d_{cg} > 0.03 \cdot D_{frame}$ | Uyarı | "Ağırlık merkezi motor merkezinden > %3 uzakta. Motorlar dengesiz yüklenir." |
| 7 | 3D Baskı Tabla Aşımı | $L_{arm} > 240\text{ mm} \lor D_{plate} > 240\text{ mm}$ | Uyarı | "Kol veya plaka boyutu 240 mm'den uzun (Kobra S1 250×250 mm tablası için segmentli baskı gerekir)." |

---

## 8. Doğrulama ve Birim Test Vektörleri

### 8.1 Test Vektörü 1: 250 mm Quad X Geometrisi
- $n = 4$ kol, $D_{frame} = 250.0\text{ mm} = 0.250\text{ m}$.
- $S_{adj} = 250.0 \cdot \sin(\pi/4) = 250.0 \cdot 0.70710678 = \mathbf{176.77669... \approx 176.8\text{ mm}}$.
- Pervane boşluğu $\delta_{clear} = 10.0\text{ mm} \implies D_{prop,max} = 176.7767 - 10.0 = 166.7767\text{ mm} = \mathbf{6.566\text{ inç}}$.

### 8.2 Test Vektörü 2: 275 mm Quad X Geometrisi
- $n = 4$ kol, $D_{frame} = 275.0\text{ mm} = 0.275\text{ m}$.
- $S_{adj} = 275.0 \cdot \sin(\pi/4) = 275.0 \cdot 0.70710678 = \mathbf{194.45436... \approx 194.5\text{ mm}}$.
- $D_{prop,max} = 194.4544 - 10.0 = 184.4544\text{ mm} = \mathbf{7.262\text{ inç}}$.

### 8.3 Test Vektörü 3: Momentum Teorisi Askı ve Güç Çözümü (Aritmetik Düzeltilmiş)
**Girdiler:**
- $m_{mtow} = 1.000\text{ kg} \implies W = 1.000 \times 9.80665 = 9.80665\text{ N}$.
- $n_{arms} = 4$, $n_{motors} = 4$, koaksiyel değil.
- $T_{motor} = 9.80665 / 4 = 2.4516625\text{ N}$.
- $\rho = 1.2250\text{ kg/m}^3$ (MSL, 0 m).
- Pervane çapı: $5\text{ inç} = 0.1270\text{ m}$.
- Disk alanı $A = \frac{\pi}{4} (0.127)^2 = 0.012667687...\text{ m}^2$.
- $FM = 0.65$, $\eta_{motor} = 0.85$, $\eta_{esc} = 0.95 \implies \eta_{elec} = 0.8075$.
- Batarya: 4S1P Molicel P42A (14.4 V, 4.2 Ah, 60.48 Wh), $DoD = 0.80 \implies E_{usable} = 48.384\text{ Wh}$.
- Aviyonik güç: $P_{avionics} = 5.0\text{ W}$.

**Hesap Adımları:**
1. İdeal güç:
   $$v_i = \sqrt{\frac{2.4516625}{2 \times 1.225 \times 0.012667687}} = \sqrt{78.99497} = 8.887913\text{ m/s}$$
   $$P_{ideal,motor} = 2.4516625 \times 8.887913 = 21.79025\text{ W}$$
   $$P_{ideal,tot} = 4 \times 21.79025 = 87.1610\text{ W}$$
2. Mil gücü ($FM = 0.65$):
   $$P_{shaft,motor} = 21.79025 / 0.65 = 33.52346\text{ W}$$
   $$P_{shaft,tot} = 134.0938\text{ W}$$
3. Elektrik gücü:
   $$P_{elec,motor} = 33.52346 / 0.8075 = \mathbf{41.515\text{ W}}$$
   $$P_{elec,hover,tot} = 4 \times 41.515 = \mathbf{166.059\text{ W}}$$
4. Disk yüklemesi:
   $$DL = \frac{9.80665}{4 \times 0.012667687} = 193.538\text{ N/m}^2$$
5. Havada kalma süresi:
   $$P_{tot,hover} = 166.059 + 5.0 = \mathbf{171.059\text{ W}}$$
   $$t_{hover} = \frac{48.384}{171.059} = 0.28285\text{ saat} = \mathbf{16.97\text{ dakika}}$$
6. İleri uçuş gücü ve menzil ($V_{cr} = 12\text{ m/s}$):
   - **Glauert + Profil + Parazit Ana Modeli ($f = 0.01\text{ m}^2$, $k_{prof} = 0.15$):**
     Dinamik Glauert iterasyonu ve profil gücüyle seyir gücü ve sayısal optimum menzil hesaplanır.
   - **Kaba Tahmin (Eski, %85 P_hover):**
     $$P_{tot,cruise,85} = 0.85 \times 166.059 + 5.0 = \mathbf{146.150\text{ W}}$$
     $$t_{cruise,85} = \frac{48.384}{146.150} = 0.331057\text{ saat} = \mathbf{19.86\text{ dakika}}$$
     $$\text{Range}_{85} = 12 \times 0.331057 \times 3.6 = \mathbf{14.30\text{ km}}$$
