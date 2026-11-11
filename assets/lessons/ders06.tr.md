# Ders 6: Tasarım Noktası: Kanat Yüklemesi (W/S) ve Güç Yüklemesi (P/W) Neden Önemli?

### Ne öğreneceksin
- Kanat yüklemesi ($W/S$, $\text{N/m}^2$) ve güç yüklemesi ($P/W$, $\text{W/N}$) kavramlarının uçak boyutlandırmasındaki temel rolünü.
- Farklı uçuş fazlarının (tutunma kaybı, tırmanma, seyir, dönüş ve elle fırlatma) tasarım kısıtlarını nasıl belirlediğini.
- Eşleştirme diyagramında (matching chart) tüm görev şartlarını sağlayan en hafif ve en verimli "Tasarım Noktası"nın nasıl seçildiğini.

### Anlatım
Bir araç tasarladığınızı düşünün. Bir şehir otomobili, bir yarış arabası ve ağır yük taşıyan bir tır aynı motor ve şasiye sahip olamaz. Yarış arabası virajları yüksek hızla dönebilmek ve ani hızlanmak için devasa bir güç-ağırlık oranına ihtiyaç duyarken, yük tırı ağır yükü taşıyacak güçlü bir şasi ve motora ihtiyaç duyar. Sabit kanatlı bir İHA tasarlarken de rastgele bir kanat veya motor seçemezsiniz; uçağın boyutlarını ve motor gücünü belirleyen iki temel ölçüt vardır: **Kanat Yüklemesi** ve **Güç Yüklemesi**.

Birinci temel parametre **Kanat Yüklemesidir (Wing Loading - $W/S$)**. Uçağın toplam ağırlığının ($W = m \cdot g$) kanat alanına ($S$) bölünmesiyle bulunur ($\text{N/m}^2$). Kanat yüklemesi uçağın ne kadar hızlı uçmak zorunda olduğunu doğrudan belirler. Kanat yüklemesi yüksek olan bir uçak havada tutunabilmek için çok hızlı uçmalıdır; bu da uçağın iniş ve tutunma (stall) hızını yükseltir. Eğer uçağınızı sahada elle fırlatarak kaldırmak ve dar bir alana güvenle indirmek istiyorsanız, tutunma hızınız belirli bir sınırı (örneğin $10\text{ m/s}$) aşamaz. Bu durum kanat yüklemesine bir üst sınır koyar:
$$(W/S)_{max} = \frac{1}{2} \rho V_{stall,req}^2 C_{L,max}$$

İkinci parametre ise **Güç Yüklemesidir (Power Loading - $P/W$)**. Uçağın birim ağırlığı başına gereken motor mil gücünü ($\text{W/N}$) ifade eder. Uçağın her bir uçuş manevrası farklı bir asgari güç gerektirir:
1. **Düz Seyir (Cruise):** Uçağın sadece havada kalıp ileri gitmesi için gereken güçtür; genellikle en düşük güç seviyesidir ($P/W \approx 2.4\text{ W/N}$).
2. **Tırmanma (Climb):** Yerçekimine karşı irtifa kazanmak (örneğin saniyede $3.5\text{ metre}$ tırmanmak) ciddi motor gücü ister ($P/W \approx 7.6\text{ W/N}$).
3. **Viraj Dönüşü (Turn):** Uçak yatarak döndüğünde kanat, ağırlığa ek olarak dönüşün gerektirdiği yükü de taşımak zorundadır (yük çarpanı $n>1$); bu da gereken kaldırmayı ve indüklenmiş sürüklemeyi artırır.
4. **Elle Fırlatma (Launch):** Fırlatıldığı andan itibaren stall hızının üzerine çıkıp hızlanabilmek için motorun yüksek itki gücü üretmesi şarttır.

Havacılık mühendisleri bu kısıtları bir araya getirmek için yatay ekseninde Kanat Yüklemesini ($W/S$), dikey ekseninde ise Güç Yüklemesini ($P/W$) gösteren bir **Kısıt Analizi Grafiği (Matching Chart)** çizer. Tutunma şartı grafiğin sağına düşey bir duvar çeker; kanat yüklemesi bu duvarın sağında olamaz. Seyir, tırmanma ve fırlatma eğrileri ise alttan yukarı doğru sınırlar çizer; motor gücü bu eğrilerin altında kalamaz.

Tüm bu sınırların kesiştiği güvenli bölgede yer alan en ideal köşe noktaya **Tasarım Noktası (Sizing Point)** denir. $2.0\text{ kg}$ kütleli ($W \approx 19.6\text{ N}$) referans İHA'mız için yapılan hesaplarda, en kritik koşul tırmanma gereksinimidir. %15 güvenlik marjıyla birlikte tasarım noktası $(P/W)_{design} \approx 8.75\text{ W/N}$ ve $W/S \approx 61.3\text{ N/m}^2$ olarak belirlenir. Bu da uçağımız için $0.32\text{ m}^2$ kanat alanı ve motor için $172\text{ W}$ mekanik mil gücü ($213\text{ W}$ elektriksel giriş gücü) anlamına gelir.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu analizler **"Sabit Kanat: Kısıt Analizi & Boyutlandırma (Matching Chart)"** modülünde yer alır:
- Görev isterlerini girersin: Azami Tutunma Hızı ($V_{stall\_req} = 10\text{ m/s}$), Seyir Hızı ($V_{cr} = 18\text{ m/s}$), Tırmanma Hızı ($RC = 3.5\text{ m/s}$) ve Fırlatma Açısı.
- Program ekranda dinamik bir grafik çizer. Grafikte Stall düşey çizgisini, Seyir, Tırmanma, Dönüş ve Fırlatma eğrilerini göstererek en hafif kanat ve en verimli motorun seçileceği **Tasarım Noktasını ($W/S = 61.3\text{ N/m}^2$, $P_{shaft} = 172\text{ W}$)** işaretler.

### Mini deney
1. **Tırmanma Hızı Değişimi Deneyi:** Programda tırmanma hızı gereksinimini ($RC$) $3.5\text{ m/s}$'den $5.0\text{ m/s}$'ye yükselt. Tırmanma eğrisinin yukarı kaydığını ve gereken motor mil gücünün $172\text{ W}$'tan yaklaşık $220\text{ W}$'a çıktığını gözlemle.
2. **Tutunma Hızı Kısıtı Deneyi:** Daha dar bir sahaya iniş yapabilmek için tutunma hızı sınırını ($V_{stall\_req}$) $10\text{ m/s}$'den $8\text{ m/s}$'ye düşür. Sağdaki düşey stall sınır çizgisinin sola kaydığını ($C_{L,max} \approx 1.125$ için $(W/S)_{max} \approx 44\text{ N/m}^2$), bu şartı sağlamak için kanat alanının ($S$) en az yaklaşık $0.45\text{ m}^2$ olması gerektiğini gör (kesin sayıyı program, ayarlı $C_{L,max}$ değerine göre hesaplar).

### Sık yapılan yanlışlar
1. **"500 gramlık bir İHA'yı uçurmak için 1 Watt motor gücü yeterlidir (1 kg için 2 Watt kuralı)" yanılgısı:** Bu kural fiziğe uymaz. Örneğin $2\text{ kg}$'lık sabit kanatlı referans İHA tırmanışta yaklaşık $213\text{ W}$ elektriksel güce, $1\text{ kg}$'lık bir multirotor ise askıda yaklaşık $166\text{ W}$ güce ihtiyaç duyar.
2. **"Kanat alanını ne kadar küçültürsek uçak o kadar az sürüklenir, bu yüzden kanat hep en küçük seçilmelidir" yanılgısı:** Kanat alanı aşırı küçültülürse kanat yüklemesi çok artar. Uçağın tutunma hızı yükselir; elle fırlatmak zorlaşır ve iniş hızı çok yüksek olacağından iniş kazası riski artar.
3. **"Gereğinden çok güçlü bir motor takarsak tasarım kısıtlarını hesaplamaya gerek kalmaz" yanılgısı:** Büyük motor; daha ağır motor, daha ağır ESC ve daha büyük bir batarya demektir. Bu ekstra kütle toplam ağırlığı artırır, kanat yüklemesini yükseltir ve uçuş süresini kısaltan bir döngü başlatır.

### Mini test
**1. Sabit kanatlı bir İHA tasarımında uçağın azami tutunma hızını ($V_{stall\_req}$) belirleyen en temel kısıt parametresi hangisidir?**
A) Gövde boyu
B) Kanat yüklemesi ($W/S$)
C) Bataryanın C değeri
D) Pervane rengi
*Doğru Cevap:* B
*Açıklama:* Tutunma hızı formülü $V_{stall} = \sqrt{\frac{2 (W/S)}{\rho C_{L,max}}}$'dir. İstenen tutunma hızını aşmamak için kanat yüklemesine ($W/S$) doğrudan bir üst sınır getirilir.

**2. Ağırlığı $W = 19.6\text{ N}$ ($2.0\text{ kg}$) olan bir İHA için tırmanma kısıtından hesaplanan tasarım güç yüklemesi $(P/W)_{design} = 8.75\text{ W/N}$ ise, motorun üretmesi gereken mekanik mil gücü ($P_{shaft}$) yaklaşık ne kadardır?**
A) $17.2\text{ W}$
B) $87.5\text{ W}$
C) $172\text{ W}$
D) $384\text{ W}$
*Doğru Cevap:* C
*Açıklama:* Mil gücü $P_{shaft} = (P/W)_{design} \times W = 8.75\text{ W/N} \times 19.6\text{ N} \approx 171.5\text{ W} \approx 172\text{ W}$'dır (Doğrulanmış spesifikasyon Bölüm 5 test vektörü).

**3. Bir İHA'nın kısıt analizi grafiğinde (matching chart) "Tasarım Noktası" neyi temsil eder?**
A) Uçağın en yüksek hıza ulaştığı tek noktayı
B) Tüm uçuş kısıtlarını (tutunma, seyir, tırmanma, dönüş, fırlatma) aynı anda sağlayan en hafif ve en verimli kanat ve motor eşleşmesini
C) Bataryanın tamamen boşaldığı irtifayı
D) Kanadın kırıldığı maksimum yük noktasını
*Doğru Cevap:* B
*Açıklama:* Tasarım noktası, görev isterlerini (kalkış, tırmanma, seyir, stall) sağlayan geçerli çözüm bölgesinde motor gücünü ve kanat boyutunu optimize eden en verimli koordinattır.
