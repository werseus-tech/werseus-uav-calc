# Ders 4: Batarya Temelleri: Wh, mAh, C Değeri, Enerji Yoğunluğu ve Havada Kalma Süresi

### Ne öğreneceksin
- Batarya kapasitesi (mAh), nominal gerilim (V) ve toplam depolanan enerji (Wh) arasındaki doğru matematiksel ilişkiyi.
- "C değeri"nin (C-rating) bir uçuş süresi olmadığını, bataryanın güvenle verebileceği azami deşarj akımı katsayısı olduğunu.
- Boşalma derinliği (DoD) ve batarya enerji yoğunluğunun (Wh/kg) İHA'nın havada kalma süresini nasıl doğrudan belirlediğini.

### Anlatım
Elektrikli bir İHA tasarlarken bataryayı uçağın yakıt deposu olarak düşünebilirsiniz. Ancak sıvı yakıtlı depoların aksine, bataryalar boşaldıkça hafiflemez; ağırlıkları uçuş boyunca tamamen sabit kalır. Bu nedenle bir İHA'nın havada ne kadar süre kalabileceğini hesaplarken bataryanın taşıdığı net enerji ile gövdeye getirdiği ağırlık yükünü birlikte düşünmek gerekir: daha fazla batarya daha fazla enerji demektir, ama uçağı da ağırlaştırır.

Havacılıkta batarya kapasitesini değerlendirirken en sık yapılan hata yalnızca **miliamper-saat (mAh)** değerine bakmaktır. Kapasite (mAh veya Ah) bataryanın depoladığı elektrik yükünü gösterir (örneğin 1 Ah, 1 amperi bir saat verebilmek demektir) ancak gerilimi hesaba katmaz. Oysa iş yapan fiziksel büyüklük enerjidir ve **Watt-saat (Wh)** ile ölçülür. Bir bataryanın enerjisi, nominal gerilimi ile amper-saat cinsinden kapasitesinin çarpımıdır ($E = V \times Ah$). Örneğin $3.7\text{ V}$ nominal gerilime sahip $10000\text{ mAh}$ ($10\text{ Ah}$) tek hücreli bir batarya yalnızca $37\text{ Wh}$ enerji taşırken, $14.8\text{ V}$ ($4\text{S}$) geriliminde $5000\text{ mAh}$ bir batarya tam $74\text{ Wh}$ enerji depolar. Görüldüğü gibi mAh değeri daha düşük olan batarya, yüksek gerilimi sayesinde iki kat daha fazla enerjiye sahiptir!

Peki bataryaların üzerinde gördüğümüz **C değeri (C-Rating)** ne anlama gelir? Bu değer yaygın inanışın aksine bir "uçuş süresi" değildir. C değeri, bataryanın hücrelerini aşırı ısıtıp bozmadan güvenle verebileceği **azami sürekli deşarj akımı katsayısıdır**. $4.0\text{ Ah}$ kapasiteli ve $10\text{C}$ deşarj oranına sahip bir batarya, sürekli olarak en fazla $4.0\text{ Ah} \times 10 = 40\text{ Amper}$ akım sağlayabilir. Uçağınızın havada ne kadar kalacağı bu C katsayısına değil, motorunuzun seyir veya askı sırasında bataryadan gerçekte kaç Watt güç çektiğine bağlıdır.

Uçuş süresi hesabında diğer hayati parametre **Boşalma Derinliğidir (Depth of Discharge - DoD)**. Lityum tabanlı bataryalar uçuşta sıfıra kadar boşaltılmaz. Hücreleri aşırı deşarj hasarından ve ani gerilim düşüşünden korumak için yaygın uygulama olarak yaklaşık %20'lik bir rezerv bırakılır ($DoD = \%80$ veya $0.80$). Kullanılabilir enerji $E_{usable} = E_{pack} \times DoD$ formülüyle bulunur. Ayrıca bataryanın hücre seviyesindeki enerji yoğunluğu (örneğin $215\text{ Wh/kg}$), kablolama, lehim ve koruma kılıfları eklendiğinde paket seviyesinde yaklaşık $170 - 190\text{ Wh/kg}$ mertebesine geriler.

Tüm bu büyüklükler netleştiğinde, İHA'nın havada kalma süresi (endurance) doğrudan fiziksel güç dengesiyle hesaplanır:
$$t = \frac{E_{usable}}{P_{total,elec}} \quad [\text{saat}]$$

Burada $P_{total,elec}$, motorların çektiği itki gücü ile otopilot ve sensörlerin tükettiği sabit aviyonik gücün toplamıdır. Örneğin $1.0\text{ kg}$ kütleli bir multirotor askıda kalmak için toplam $171\text{ W}$ elektrik gücü çekiyorsa ve üzerinde $48.4\text{ Wh}$ kullanılabilir enerjiye sahip bir batarya bulunuyorsa, havada kalma süresi $t = 48.4 / 171 \approx 0.283\text{ saat}$, yani yaklaşık **17 dakika** olur.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında batarya hesaplamaları **"Güç Sistemi & Batarya Boyutlandırma"** modülünde yer alır:
- Seçtiğin hücre kimyası (örneğin lityum iyon 21700 silindirik hücreler veya LiPo paketler), seri ve paralel hücre sayısı ($n_s, n_p$) ve hücre kapasitesi girildiğinde program batarya ağırlığını ($m_{batt}$) ve nominal enerjisini ($E_{pack}$) hesaplar.
- Belirlenen boşalma derinliğine ($DoD = 0.80$) göre **Kullanılabilir Enerji ($E_{usable}$)** ve çekilen elektriksel güce göre **Askıda Kalma Süresi ($t_{hover}$)** ya da **Sabit Kanat Seyir Süresi ($t_{cruise}$)** ile menzil anında gösterilir.

### Mini deney
1. **Boşalma Derinliği (DoD) Deneyi:** Programda bataryanın kullanılabilir derinliğini ($DoD$) %80'den %60'a düşür. Kullanılabilir enerjinin ve uçağın uçuş süresinin tam %25 oranında azaldığını gözlemle.
2. **Aşırı Batarya Yükleme Deneyi:** $1.0\text{ kg}$'lık bir multirotorda uçuş süresini uzatmak amacıyla batarya kapasitesini 2 katına çıkar (batarya kütlesi 300 gramdan 600 grama çıksın). Toplam İHA kütlesi $1.3\text{ kg}$'a yükselir. Askı gücü yaklaşık $W^{1.5}$ ile arttığı için ($1.3^{1.5} \approx 1.48$) havada kalma süresinin 2 katına çıkmadığını, yalnızca yaklaşık %35 arttığını (azalan getiri) gözlemle.

### Sık yapılan yanlışlar
1. **"C değeri 20 olan batarya uçağı 20 dakika uçurur" yanılgısı:** C değeri bir süre değil, azami güvenli akım çarpanıdır. 20C'lik bir batarya, kapasitesinin 20 katı kadar yüksek akım çekilmesine hücreleri zarar görmeden izin verir. Havada kalma süresini motorun çektiği gerçek güç belirler.
2. **"Batarya kapasitesini (mAh) iki katına çıkarırsam havada kalma süresi de kesinlikle iki katına çıkar" yanılgısı:** Eklenen batarya ağırlığı uçağı ağırlaştırır. Ağırlaşan İHA'yı havada tutmak için motorlar daha yüksek güç çekmek zorunda kalır; bu nedenle süre artışı doğrusal olmaz ve bir noktadan sonra uçak havalanamaz hale gelir.
3. **"LiPo veya Li-ion bataryalar uçuşta son damlasına kadar (%100) boşaltılabilir" yanılgısı:** Lityum hücreler nominal limitlerinin altına indirildiğinde kimyasal bozulmaya uğrar, şişebilir, yangın riski oluşturabilir ve kapasitesini kaybedebilir. Bu yüzden yaygın tasarım uygulaması yaklaşık %20 rezerv bırakmaktır ($DoD = 0.80$).

### Mini test
**1. $14.8\text{ V}$ nominal gerilime ve $4000\text{ mAh}$ kapasiteye sahip 4S bir batarya paketinin toplam nominal enerjisi kaç Watt-saattir (Wh)?**
A) $14.8\text{ Wh}$
B) $59.2\text{ Wh}$
C) $592\text{ Wh}$
D) $4000\text{ Wh}$
*Doğru Cevap:* B
*Açıklama:* $4000\text{ mAh} = 4.0\text{ Ah}$'dir. Enerji formülü $E = V \times Ah = 14.8\text{ V} \times 4.0\text{ Ah} = 59.2\text{ Wh}$ olarak hesaplanır.

**2. Kapasitesi $5000\text{ mAh}$ ($5.0\text{ Ah}$) ve sürekli deşarj oranı $25\text{C}$ olan bir bataryadan güvenle çekilebilecek azami sürekli akım kaç Amperdir?**
A) $5\text{ A}$
B) $25\text{ A}$
C) $125\text{ A}$
D) $500\text{ A}$
*Doğru Cevap:* C
*Açıklama:* Azami akım $I_{max} = C \times \text{Kapasite} = 25 \times 5.0\text{ Ah} = 125\text{ Amper}$'dir.

**3. Bir İHA'nın bataryasındaki kullanılabilir enerji $E_{usable} = 96.7\text{ Wh}$ ve seyir sırasındaki toplam elektrik güç tüketimi $P_{total,elec} = 72.1\text{ W}$ ise, bu İHA'nın seyir süresi yaklaşık ne kadardır?**
A) 20 dakika
B) 45 dakika
C) 80 dakika (yaklaşık 1.34 saat)
D) 150 dakika
*Doğru Cevap:* C
*Açıklama:* Uçuş süresi $t = E_{usable} / P = 96.7\text{ Wh} / 72.1\text{ W} \approx 1.34\text{ saat}$'tir. $1.34 \times 60 \approx 80.4\text{ dakika}$ eder (Doğrulanmış spesifikasyon Bölüm 6 test vektörü).
