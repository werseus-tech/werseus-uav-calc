# Ders 1: Uçak Nasıl Uçar? Kaldırma Kuvveti, Hücum Açısı ve Hız

### Ne öğreneceksin
- Sabit kanatlı bir İHA'nın havada tutunmasını sağlayan kaldırma kuvvetinin ($L$) ağırlıkla ($W$) olan dengesini.
- Hücum açısının ($\alpha$) kaldırma katsayısını ($C_L$) nasıl artırdığını ve aşırı açının neden tutunma kaybına (stall) yol açtığını.
- Uçuş hızının ($V$) kaldırma kuvvetine olan karesel etkisini ve düz uçuşta hız değiştikçe hücum açısının nasıl ayarlandığını.

### Anlatım
Hareket halindeki bir otomobilin penceresinden elinizi dışarı uzattığınızı hayal edin. Avucunuzu yere tamamen paralel tutarsanız eliniz rüzgârı yararak rahatça ilerler. Ancak elinizin ön kısmını hafifçe yukarı doğru eğdiğiniz anda, altından geçen havanın elinizi kuvvetlice yukarı doğru ittiğini hissedersiniz. İşte sabit kanatlı bir uçağın havada kalmasını sağlayan temel etki tam olarak budur.

Uçağın kanatları özel bir aerodinamik kesite (kanat profili / airfoil) sahiptir. Uçak ileri doğru hareket ettikçe, kanat üzerinden ve altından geçen hava akışı aşağıya doğru saptırılır. Newton'ın etki-tepki yasası ve akışkan basınç farkları gereğince, aşağı saptırılan havaya karşılık hava da kanadı yukarı doğru iter. Bu yukarı yönlü kuvvete **Kaldırma Kuvveti (Lift - $L$)** denir. Sabit irtifada düz bir uçuş için bu kaldırma kuvvetinin, yerçekiminin uçağı aşağı çeken **Ağırlığına (Weight - $W$)** tam olarak eşit olması gerekir ($L = W$).

Havanın kanada geliş doğrultusu ile kanat veter çizgisi arasındaki açıya **Hücum Açısı (Angle of Attack - $\alpha$)** denir. Tıpkı araba penceresindeki eliniz gibi, hücum açısını artırdıkça kanadın ürettiği kaldırma katsayısı ($C_L$) belirli bir sınıra kadar doğrusal olarak artar. Fakat bu açıyı gereğinden fazla büyütürseniz (genellikle 12 ile 15 derecenin üzerine çıkarırsanız), hava kanadın üst yüzeyini takip edemez, akış kopar ve kaldırma kuvveti aniden düşer. Bu tehlikeli duruma "tutunma kaybı" (stall) adı verilir.

Kaldırma kuvvetini belirleyen en güçlü etken ise **Hızdır (Airspeed - $V$)**. Kanadın hissettiği dinamik basınç hızın karesiyle ($V^2$) orantılı olarak büyür. Yani hücum açısı aynı kalmak şartıyla hızı iki katına çıkardığınızda kaldırma kuvveti dört katına çıkar. Bu nedenle bir uçak yüksek hızda seyrederken aynı ağırlığı taşımak için çok küçük bir hücum açısına ihtiyaç duyarken, iniş ve kalkış gibi düşük hızlarda ağırlığı dengeleyebilmek için hücum açısını artırmak zorundadır.

Tüm bu fiziksel ilişkileri havacılığın temel kaldırma formülüyle özetleriz:
$$L = \frac{1}{2} \rho V^2 S C_L$$

Burada $\rho$ havanın yoğunluğunu (deniz seviyesinde yaklaşık $1.225\text{ kg/m}^3$), $V$ uçağın havaya göre hızını ($\text{m/s}$), $S$ kanat alanını ($\text{m}^2$) ve $C_L$ kanadın hücum açısına bağlı kaldırma katsayısını temsil eder. Örneğin $2.0\text{ kg}$ kütleye sahip ($W \approx 19.6\text{ N}$) ve kanat alanı $0.32\text{ m}^2$ olan bir İHA, $18\text{ m/s}$ seyir hızında uçarken düz uçuşu korumak için sadece $C_L \approx 0.31$ seviyesinde küçük bir hücum açısıyla uçar.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu büyüklükleri **"Sabit Kanat: Aerodinamik & Kutup Eğrisi"** modülünde görürsün.
- **Kalkış Ağırlığı ($m_{MTOW}$)** ve **Kanat Alanı ($S$)** değerlerini girdiğinde, program uçağın ağırlığını ($W = m \cdot g$) hesaplar.
- Belirlediğin **Seyir Hızı ($V_{cr}$)** değerine göre program ekranda anlık **Dinamik Basınç ($q = \frac{1}{2}\rho V^2$)** ve düz uçuş için gereken **Seyir Kaldırma Katsayısı ($C_L$)** çıktısını üretir.

### Mini deney
1. **Hız Değişimi Deneyi:** Programda seyir hızını $18\text{ m/s}$'den $12\text{ m/s}$'ye düşür. Dinamik basıncın yarıdan aza indiğini, uçağın $2\text{ kg}$ ağırlığı havada tutabilmek için gereken $C_L$ değerinin $0.31$'den yaklaşık $0.70$'e çıktığını gözlemle. Uçağın yavaş uçabilmek için daha yüksek hücum açısına ihtiyaç duyduğunu fark et.
2. **Kütle Artışı Deneyi:** Seyir hızını tekrar $18\text{ m/s}$'de sabit tut ve İHA kütlesini $2.0\text{ kg}$'dan $2.5\text{ kg}$'a çıkar. Ağırlık arttığı için gereken $C_L$ değerinin $0.31$'den yaklaşık $0.39$'a yükseldiğini, yani uçağın aynı hızda düz uçabilmek için burnunu biraz daha yukarı kaldırması gerektiğini gör.

### Sık yapılan yanlışlar
1. **"Motor uçağı doğrudan yukarı doğru kaldırır" yanılgısı:** Sabit kanatlı bir uçakta motor uçağı yalnızca ileri doğru iter (itki üretir). Uçağın yerçekimine karşı havada kalmasını sağlayan kaldırma kuvvetini, uçağın ileri hareketi sayesinde kanatların üzerinden hızla geçen hava üretir.
2. **"Hücum açısı uçağın burnunun ufukla yaptığı açıdır" yanılgısı:** Hücum açısı ($\alpha$), kanat profili veter çizgisi ile uçağın hareket ettiği yöndeki bağıl hava akımı arasındaki açıdır. Uçağın burnu yukarı bakarken uçak aşağı doğru çöküyorsa hücum açısı devasa boyutlara ulaşabilir.
3. **"Hücum açısını ne kadar artırırsan uçak o kadar iyi yükselir" yanılgısı:** Hücum açısı kritik bir sınıra ($\alpha_{crit}$, genellikle $12^\circ - 15^\circ$) kadar kaldırmayı artırır. Bu sınır aşıldığında üst yüzeydeki hava akımı kanattan tamamen kopar (tutunma kaybı / stall) ve uçak irtifa kaybetmeye başlar.

### Mini test
**1. Sabit irtifada ve düz bir hatta uçan $2.0\text{ kg}$ ağırlığındaki bir İHA için kaldırma kuvveti ($L$) ne kadar olmalıdır?**
A) $0\text{ N}$
B) Yaklaşık $19.6\text{ N}$
C) En az $50\text{ N}$
D) Motorun azami itki kuvveti kadar
*Doğru Cevap:* B
*Açıklama:* Sabit irtifada düz uçuşta düşey kuvvetler dengede olmalıdır ($L = W$). $2.0\text{ kg}$ kütlenin yerçekimi ağırlığı $W = m \cdot g = 2.0 \times 9.81 \approx 19.6\text{ N}$ olduğundan, kanat tam olarak $19.6\text{ N}$ kaldırma kuvveti üretmelidir.

**2. Diğer tüm parametreler sabit kalırken uçağın hava hızı 2 katına çıkarılırsa, kanadın ürettiği kaldırma kuvveti nasıl değişir?**
A) Yarıya iner
B) 2 katına çıkar
C) 4 katına çıkar
D) Değişmez
*Doğru Cevap:* C
*Açıklama:* Kaldırma formülünde ($L = \frac{1}{2}\rho V^2 S C_L$) hızın karesi ($V^2$) yer alır. Hız 2 katına çıktığında ($2^2 = 4$), dinamik basınç ve üretilen kaldırma kuvveti 4 katına çıkar.

**3. Uçuş sırasında hücum açısı kritik sınırın ($\alpha_{crit}$) üzerine çıkarılırsa ne gerçekleşir?**
A) Uçak ses hızını aşar
B) Hava akımı kanat üzerinden kopar ve uçak tutunma kaybına (stall) uğrar
C) Kaldırma kuvveti sonsuza doğru artar
D) Uçağın motoru kendiliğinden durur
*Doğru Cevap:* B
*Açıklama:* Kritik hücum açısı aşıldığında kanadın üst yüzeyindeki sınır tabaka akışı yüzeyden ayrılır. Bu durum kaldırma kuvvetinin aniden düşmesine ve sürüklemenin hızla artmasına yol açan "tutunma kaybı" (stall) olayıdır.
