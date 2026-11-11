# Ders 8: Kanat Kirişi ve Multirotor Kolu: Neden Kırılmaz Ya Da Titrer?

### Ne öğreneceksin
- Sabit kanatlı bir İHA'da aerodinamik kaldırma kuvvetinin kanat kökünde nasıl devasa bir eğilme momenti ($M_b$) oluşturduğunu ve ana kirişin (spar) bu yükü nasıl taşıdığını.
- Taşıyıcı kirişin kesit modülü ($Z$), azami eğilme gerilmesi ($\sigma$) ve Güvenlik Marjı ($MS = \sigma_{allow}/\sigma - 1$) hesaplarını.
- Multirotor kollarının motor itki ve tork kuvvetleri altındaki esnemesini, doğal titreşim frekansının uçuş kontrolcüsü jiroskoplarına (gyro) etkisini ve rezonansın nasıl önlendiğini.

### Anlatım
Elinizde ağır bir alışveriş poşeti taşırken kolunuzu vücudunuza yapışık tuttuğunuzda çok az yorulursunuz. Ancak aynı poşeti elinizde tutarak kolunuzu yana doğru dümdüz açtığınızı hayal edin: Birkaç saniye içinde omzunuzda dayanılmaz bir zorlanma hissedersiniz. Poşetin ağırlığı değişmemiştir ama kolunuzun uzunluğu devasa bir kaldıraç kolu yaratarak omzunuzda muazzam bir eğilme kuvveti (moment) oluşturmuştur. Uçak kanatları ve multirotor kolları da gökyüzünde tıpkı bu yana açılmış kol gibi sürekli eğilmeye ve bükülmeye zorlanır.

Sabit kanatlı bir uçak sert bir viraja girdiğinde veya türbülanslı bir rüzgâr hamlesiyle karşılaştığında, kanat boyunca oluşan kaldırma kuvveti kanat uçlarından gövdeye doğru birikir. Gövdenin ağırlığı merkezde aşağı çekerken, kanatların kaldırması yukarı çeker. Bu durum kanadın gövdeyle birleştiği kanat kökünde en yüksek seviyeye ulaşan bir **Kök Eğilme Momenti ($M_b$)** yaratır. Bir uçağın hafif dış kaplaması bu devasa eğilme kuvvetini tek başına taşıyamaz. Bu yükü taşımak için kanadın içine boylu boyunca uzanan sert bir omurga, yani **Kanat Ana Kirişi (Wing Spar)** yerleştirilir.

Havacılıkta yapılar yalnızca normal uçuş yüküne göre değil, manevra limit yük katsayısı ($n_{limit} = 3.8$) ve emniyet katsayısıyla ($FS = 1.5$) çarpılarak bulunan **nihai yük katsayısına ($n_{ult} = 3.8 \times 1.5 = 5.7$)** göre boyutlandırılır. $2.0\text{ kg}$ kütleli ($W \approx 19.6\text{ N}$) ve $1.6\text{ m}$ kanat açıklığına sahip bir İHA'da kök eğilme momenti eliptik yük dağılımıyla yaklaşık $19.0\text{ N}\cdot\text{m}$'ye ulaşır:
$$M_{b,ult} = \frac{n_{ult} W b}{3 \pi}$$

Bu momenti karşılamak için genellikle içi boş dairesel karbon fiber borular kullanılır (dış çap $D_o = 10\text{ mm}$, iç çap $D_i = 8\text{ mm}$). Borunun geometrik eğilme direnci **Kesit Modülü ($Z$)** ile hesaplanır:
$$Z = \frac{\pi}{32} \frac{D_o^4 - D_i^4}{D_o} \approx 5.80 \times 10^{-8}\text{ m}^3$$

Kirişte oluşan azami çekme/basma gerilmesi $\sigma_{max} = M_{b,ult} / Z \approx 327\text{ MPa}$ olur. Bu örnekte karbon fiber borunun izin verilen dayanımı $\sigma_{allow} = 500\text{ MPa}$ alınmıştır (gerçek değer malzeme verisine göre değişir). Yapının **Güvenlik Marjı (Margin of Safety - $MS$)**:
$$MS = \frac{\sigma_{allow}}{\sigma_{max}} - 1 = \frac{500}{327} - 1 = +0.53$$
$MS \ge 0$ olduğu için kiriş bu yükte güvenli kabul edilir.

Multirotorlarda ise motor kolları benzer birer konsol kiriştir. Kol, motorun yukarı itkisini ($T_{max}$) ve pervanenin yarattığı dönme torkunu ($Q$) taşır. Ancak multirotorda asıl tehlike kırılmaktan ziyade **Titreşim ve Rijitliktir (Stiffness)**. Dakikada on binlerce devirle dönen fırçasız motorlar yüksek frekanslı titreşim üretir. Eğer motor kolu yeterince rijit yapılmazsa, kolun doğal titreşim frekansı motor devirleriyle rezonansa girer. Kollar şiddetle titremeye başlar. Bu mekanik gürültü uçuş kontrol kartındaki hassas jiroskop (gyro) sensörlerinin ölçümünü bozar; otopilot motorlara yanlış düzeltme komutları göndererek aracın titremesine, motorların ısınmasına veya kontrolün kaybedilmesine yol açar.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu mekanik analizler iki ayrı modülde yer alır:
- **Sabit Kanat: Yapısal Analiz & Kiriş Boyutlandırma:** Uçak kütlesi, açıklık, limit yük ($n_{limit} = 3.8$) ve karbon boru çapları ($D_o = 10\text{ mm}, D_i = 8\text{ mm}$) girildiğinde, program **Kök Eğilme Momentini ($19.0\text{ N}\cdot\text{m}$)**, **Maksimum Gerilmeyi ($327\text{ MPa}$)** ve **Güvenlik Marjını ($MS = +0.53$)** hesaplar.
- **Multirotor: Kol Geometrisi & Titreşim Analizi:** Çerçeve çapı ($D_{frame}$), kol uzunluğu ($L_{arm} = \frac{D_{frame}}{2} - R_{plate}$) ve motor devri girildiğinde, program kolun eğilme esnemesini ve titreşim uyarılarını listeler.

### Mini deney
1. **Kiriş Çapı ve Gerilme Deneyi:** Sabit kanat modülünde karbon kirişin dış çapını 10 mm'den 8 mm'ye düşür (iç çap 6 mm olsun). Kesit modülünün ($Z$) yaklaşık $5.8 \times 10^{-8}$'den $3.4 \times 10^{-8}\text{ m}^3$'e indiğini, gerilmenin yaklaşık $550\text{ MPa}$'a çıktığını ve Güvenlik Marjının eksiye ($MS \approx -0.09$) düştüğünü, yani bu kirişin nihai yükte yetersiz kalacağını gözlemle.
2. **Multirotor Kol Titreşim Deneyi:** Multirotor modülünde motor kolunu uzatarak çerçeve çapını $250\text{ mm}$'den $450\text{ mm}$'ye çıkarırken kol kalınlığını aynı bırak. Konsol sehiminin kol uzunluğunun küpüyle ($L^3$) arttığını ve programın kol rijitliği / rezonans riski uyarısı verdiğini gör.

### Sık yapılan yanlışlar
1. **"Kanat kirişi (spar) ile kanat kamburluğu (camber) veya hücum açısı aynı şeydir" yanılgısı:** Kiriş aerodinamik bir şekil değildir. Kanadın içine yerleştirilen ve kaldırma kuvvetinin yarattığı eğilme yüklerini taşıyan ana yapısal taşıyıcı borudur; kaldırma değil mukavemet üretir.
2. **"Bir yapının sağlam olması için sadece malzemenin kırılmaması yeterlidir, esnemesi önemsizdir" yanılgısı:** Özellikle çok rotorlu İHA'larda kol kırılmasa bile fazla esnerse doğal frekansı motor devirleriyle örtüşebilir. Oluşan titreşim otopilotun ivmeölçer ve jiroskop ölçümlerini bozar (hatta sensörleri doyuma sokabilir) ve uçuş kontrolünü zorlaştırır.
3. **"Güvenlik marjının ($MS$) negatif olması yapının çok güvenli olduğunu gösterir" yanılgısı:** Güvenlik marjı havacılıkta $MS = (\sigma_{allow}/\sigma_{max}) - 1$ olarak tanımlanır. $MS \ge 0$ yapının emniyetli olduğunu, $MS < 0$ ise yapının hedeflenen yük altında kırılacağını veya kalıcı hasara uğrayacağını ifade eder.

### Mini test
**1. Sabit kanatlı bir İHA sert bir manevra yaparken kanat kökünde oluşan en büyük yapısal yükleme türü hangisidir?**
A) Eksenel çekme kuvveti
B) Kök eğilme momenti ($M_b$)
C) Burun çarpma basıncı
D) Güneş ışığı sıcaklık gerilmesi
*Doğru Cevap:* B
*Açıklama:* Kanat boyunca dağılan kaldırma kuvveti kanat açıklığı boyunca kaldıraç etkisi yaratarak kanadın gövdeye bağlandığı kök noktasında çok yüksek bir eğilme momenti ($M_b$) meydana getirir.

**2. Bir ana kirişte nihai yük altında oluşan gerilme $\sigma_{max} = 327\text{ MPa}$ ve malzemenin izin verilen azami dayanımı $\sigma_{allow} = 500\text{ MPa}$ ise, Güvenlik Marjı ($MS$) yaklaşık kaçtır?**
A) $-0.35$
B) $+0.53$
C) $+1.53$
D) $0.00$
*Doğru Cevap:* B
*Açıklama:* Güvenlik marjı formülü $MS = (\sigma_{allow} / \sigma_{max}) - 1$'dir. $(500 / 327) - 1 = 1.529 - 1 \approx +0.53$'tür (Doğrulanmış spesifikasyon Bölüm 9 test vektörü).

**3. Multirotor kollarının aşırı esnek ve yetersiz rijitlikte olması uçuş sırasında en çok hangi kritik soruna yol açar?**
A) Bataryanın aniden soğumasına
B) Motor devirleriyle rezonansa girerek oluşan titreşimin uçuş kontrolcüsü jiroskoplarını bozmasına
C) Pervanelerin ters yönde dönmeye başlamasına
D) Uçağın ağırlığının kendiliğinden artmasına
*Doğru Cevap:* B
*Açıklama:* Esnek kollar motorların dönüş frekansıyla rezonansa girerek şiddetli mekanik titreşim üretir. Bu titreşim uçuş kontrol kartındaki MEMS jiroskop sensörlerinde gürültüye ve filtre doyumuna sebep olarak kontrol kaybı yaratır.
