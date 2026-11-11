# Ders 3: Ağırlık Merkezi, Nötr Nokta ve Statik Marj: Uçak Neden Dengede Durur?

### Ne öğreneceksin
- Sabit kanatlı bir uçakta Ağırlık Merkezi ($x_{cg}$), Aerodinamik Merkez ($x_{ac}$) ve Nötr Nokta ($x_{np}$) kavramlarının fiziksel anlamını.
- Pozitif boyuna statik kararlılık için Statik Marjın ($SM = (x_{np} - x_{cg}) / MAC$) neden %5 ile %15 arasında olması gerektiğini.
- Çok rotorlu sistemlerde (multirotor) ağırlık merkezi kaçıklığının ($\Delta_{cg} \le \%3$) motor yük dengesine ve uçuş güvenliğine etkisini.

### Anlatım
Klasik bir dart okunu veya rüzgâr gülünü gözünüzün önüne getirin. Bir dart okunu elinizden fırlattığınızda neden daima ağır olan metal ucu önde, hafif tüyleri ise arkada gider? Oku bilerek ters fırlatsanız bile havada hemen dönerek burnunu ileri çevirir. Çünkü tüyler ağırlık merkezinin gerisinde kaldığı için, ok yön değiştirdiğinde tüylerdeki hava kuvveti oku yeniden akışa hizalayan bir moment oluşturur. Sabit kanatlı bir uçağın gökyüzünde takla atmadan dengede kalmasının ardındaki temel sır da tam olarak bu dart oku prensibidir.

Uçağın tüm kütlesinin dengelendiği fiziksel noktaya **Ağırlık Merkezi (Center of Gravity - $x_{cg}$)** denir. Kanat profilinin kaldırma kuvveti ürettiği aerodinamik merkez ise genellikle kanat veter çizgisinin %25'i civarındadır ($x_{ac\_w}$). Ancak uçağa bir gövde ve en önemlisi arka tarafa yatay bir dengeleyici kuyruk (horizontal stabilizer) eklediğimizde, tüm uçağın aerodinamik kaldırma merkezi arkaya doğru kayar. Hücum açısı değiştiğinde net yunuslama (pitch) momentinin değişmediği (hücum açısından bağımsız kaldığı) bu noktaya **Nötr Nokta (Neutral Point - $x_{np}$)** adı verilir.

Bir uçağın doğal olarak dengeli uçabilmesi (pozitif boyuna statik kararlılık) için, **Ağırlık Merkezi mutlaka Nötr Noktanın ÖNÜNDE yer almalıdır ($x_{cg} < x_{np}$)**. Uçuş sırasında ani bir rüzgâr hamlesi uçağın burnunu yukarı kaldırırsa, nötr noktada (ağırlık merkezinin arkasında) oluşan ilave kaldırma kuvveti bir kaldıraç gibi davranarak uçağın burnunu kendiliğinden aşağı iter ve uçağı eski dengeli konumuna döndürür ($C_{m\alpha} < 0$). Eğer ağırlık merkezi nötr noktanın arkasına kayarsa, rüzgâr uçağın burnunu kaldırdığında uçak kendiliğinden daha da yukarı döner; pilotun güçlükle kontrol edebileceği bir takla/stall riski doğar.

Bu kararlılık payını ölçmek için havacılıkta **Statik Marj (Static Margin - $SM$)** kullanılır:
$$SM = \frac{x_{np} - x_{cg}}{MAC}$$

Burada $MAC$ uçağın ortalama aerodinamik kanat veter uzunluğudur. Öğrenci ve hobi amaçlı İHA tasarımlarında hedef statik marj genellikle kanat veterinin **%8 ile %12'si** (varsayılan olarak %10'u) civarında seçilir. Statik marj %15'in üzerine çıkarsa uçak aşırı burun-ağır olur ve burnunu kaldırmak için irtifa dümenine aşırı yük bindirir; %5'in altına inerse uçak aşırı hassas ve kontrolü zor hale gelir.

Multirotorlarda ise durum farklıdır. Hoverda (askıda) sabit kanat ve aerodinamik nötr nokta yoktur. Ancak orada da kural basittir: Ağırlık merkezi, motorların geometrik simetri merkezinin tam ortasında $(0,0)$ olmalıdır. Ağırlık merkezinin merkezden kaçıklığı için pratik bir kural, çerçeve boyutunun (motorlar arası köşegen) %3'ünü aşmamasıdır ($d_{cg} \le 0.03 \cdot D_{frame}$). Daha fazla kayarsa bir taraftaki motorlar sürekli fazladan çalışmak zorunda kalır, daha çok ısınır ve otopilotun manevra için kullanabileceği güç payı azalır.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu parametreler **"Ağırlık Merkezi & Stabilite"** modülünde yer alır:
- Kanat ve kuyruk ölçülerini girdiğinde program yatay kuyruk hacim katsayısını ($V_H = S_h l_h / (S \cdot MAC)$) ve uçağın **Nötr Noktasını ($x_{np} = 51.2\%\text{ MAC}$)** hesaplar.
- Hedeflenen kararlılık marjı **Statik Marj ($SM = \%10$)** olarak tanımlandığında, program hedef ağırlık merkezi konumunu **$x_{cg\_target} = 41.2\%\text{ MAC}$** olarak sabitler.
- Programdaki "Batarya Konum Çözücüsü", hedef ağırlık merkezini sağlamak için bataryanın gövdede nereye konması gerektiğini ($x_{batt}$) milimetre cinsinden hesaplar.

### Mini deney
1. **Kararsızlık Deneyi:** Programda batarya konumunu ($x_{batt}$) adım adım arkaya kaydırarak ağırlık merkezini ($x_{cg}$) nötr noktanın gerisine taşı (örneğin $SM = -\%5$ olana kadar). Programın kırmızı renkli **"Statik marj ... yeşil bandın (...) dışında. AG çok arkada: bataryayı öne alın."** uyarısı verdiğini gözlemle (basit modda: **"kararsız: bu haliyle uçmaz"**).
2. **Kuyruk Boyutu Deneyi:** Yatay kuyruk alanını ($S_h$) $0.055\text{ m}^2$'den $0.030\text{ m}^2$'ye düşür. Nötr noktanın ($x_{np}$) kanada doğru öne kaydığını ve uçağın dengesini koruyabilmek için bataryayı gövdenin daha da burnuna itmek zorunda kaldığını fark et.

### Sık yapılan yanlışlar
1. **"Ağırlık merkezini ne kadar öne alırsak uçak o kadar güvenli uçar" yanılgısı:** Ağırlık merkezini aşırı öne almak ($SM > \%20$), uçağı bir çekiç gibi burun-ağır yapar. İrtifa dümeni uçağın burnunu kaldırmaya yetmez; inişte veya elle fırlatmada uçak hızla burnu üzerine çakılabilir.
2. **"Nötr nokta uçağın gövde boyunun tam orta noktasıdır" yanılgısı:** Nötr nokta geometrik bir uzunluk ortası değildir. Kanat alanı, kuyruk alanı, kanat profili taşıma eğimi ve kuyruk kolunun aerodinamik etkileşimiyle belirlenen hassas bir aerodinamik merkezdir.
3. **"Multirotorda otopilot olduğu için bataryayı kafamıza göre yerleştirebiliriz" yanılgısı:** Otopilot ağırlık merkezi kaçıklığını dengelemek için yük taşıyan taraftaki motorlara sürekli daha fazla gaz verir. Bu durum motorların daha çok ısınmasına, güç payının azalmasına ve rüzgâr hamlesinde motorların doyuma ulaşıp kontrolün kaybedilmesine yol açabilir.

### Mini test
**1. Sabit kanatlı bir İHA'nın pozitif boyuna statik kararlılığa sahip olması (rüzgâr hamlesinde kendi dengesini bulması) için temel şart nedir?**
A) Ağırlık merkezinin ($x_{cg}$) nötr noktanın ($x_{np}$) önünde olması ($x_{cg} < x_{np}$)
B) Ağırlık merkezinin uçağın en arkasında olması
C) Kanat alanının gövde alanından küçük olması
D) Motorun uçağın arkasına takılması
*Doğru Cevap:* A
*Açıklama:* Pozitif boyuna kararlılık için ağırlık merkezi nötr noktanın önünde bulunmalıdır. Böylece rüzgâr burnu yukarı kaldırdığında arkadaki nötr noktada artan kaldırma kuvveti burnu aşağı indiren düzeltici bir moment ($C_{m\alpha} < 0$) üretir.

**2. Ortalama aerodinamik veteri $MAC = 200\text{ mm}$ olan bir uçakta nötr nokta $x_{np} = 100\text{ mm}$ ve ağırlık merkezi $x_{cg} = 80\text{ mm}$ olarak ölçülmüştür. Bu uçağın Statik Marjı ($SM$) yüzde kaçtır?**
A) %5
B) %10
C) %20
D) -%10
*Doğru Cevap:* B
*Açıklama:* Statik marj formülü $SM = (x_{np} - x_{cg}) / MAC$'dir. $(100 - 80) / 200 = 20 / 200 = 0.10$, yani tam olarak **%10**'dur.

**3. Multirotor bir İHA'da ağırlık merkezinin çerçeve merkezinden kaçıklığı en fazla ne kadar olmalıdır?**
A) Çerçeve çapının %25'i kadar
B) Çerçeve çapının %10'u kadar
C) Çerçeve motor-motor çapraz mesafesinin en fazla %3'ü kadar ($d_{cg} \le 0.03 \cdot D_{frame}$)
D) Ağırlık merkezinin konumu multirotorda hiçbir önem taşımaz
*Doğru Cevap:* C
*Açıklama:* Çok rotorlu sistemlerde motor yük dengesizliğini ve kontrol otoritesi kaybını önlemek için ağırlık merkezi eksantrisitesi çerçeve çapraz mesafesinin %3'ünü aşmamalıdır.
