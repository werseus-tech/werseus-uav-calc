# Ders 2: Sürükleme, Süzülme Oranı (L/D) ve Açıklık Oranı (Aspect Ratio)

### Ne öğreneceksin
- Bir uçağın karşılaştığı parazit sürükleme ($C_{D0}$) ile kaldırma kuvvetinin bedeli olan indüklenmiş sürükleme ($C_{Di}$) arasındaki temel farkı.
- Kanat açıklık oranının ($AR = b^2/S$) kanat ucu girdaplarını ve indüklenmiş sürüklemeyi nasıl azalttığını.
- Süzülme oranının ($L/D$) aerodinamik verimliliği, motorsuz süzülme mesafesini ve menzili nasıl belirlediğini.

### Anlatım
Okyanuslar üzerinde saatlerce neredeyse hiç kanat çırpmadan süzülebilen bir albatros ile şehir parkında pırpır kanat çırpan bir serçeyi karşılaştırın. Albatrosun gövdesine kıyasla upuzun ve incecik kanatları vardır; serçenin ise kısa ve tombul kanatları bulunur. Albatrosun bu ince ve uzun kanat yapısı, az enerji harcayarak çok uzun mesafe süzülmesini sağlar. Sabit kanatlı bir İHA tasarlarken karşımıza çıkan en önemli aerodinamik seçimlerden biri de kanadın bu oranını belirlemektir.

Uçan her cisim havada ilerlerken hava moleküllerinin direnciyle karşılaşır. Bu geriye doğru çekici kuvvete **Sürükleme (Drag - $D$)** denir ve uçaklarda iki ana bileşenden oluşur. Birincisi **Parazit Sürükleme ($C_{D0}$)**: Uçağın gövdesi, kuyruğu ve kanat yüzeylerinin havayla sürtünmesinden ve şekil direncinden kaynaklanır. Tıpkı bisiklet sürerken rüzgâra karşı dik oturduğunuzda hissettiğiniz direnç gibidir; sürükleme kuvveti hızın karesiyle ($V^2$) büyür.

İkinci sürükleme türü ise havacılığın en ilginç kavramlarından biri olan **İndüklenmiş Sürüklemedir ($C_{Di}$)**. Bir kanat kaldırma kuvveti üretirken, kanadın altındaki yüksek basınçlı hava kanat uçlarından dolanarak üstteki düşük basınçlı bölgeye kaçmak ister. Bu kaçış, kanat uçlarında geriye doğru uzanan dönen hava girdapları (kanat ucu girdapları / wingtip vortices) oluşturur. Bu girdaplar kanadın arkasındaki havayı aşağı yönlü saptırır (downwash); böylece kaldırma kuvveti hafifçe geriye yatar ve uçağı geriye çeken bir bileşen oluşur. Yani indüklenmiş sürükleme, kanadın kaldırma kuvveti üretmesinin kaçınılmaz fiziksel faturasıdır.

İşte tam bu noktada **Açıklık Oranı (Aspect Ratio - $AR$)** devreye girer. Kanat açıklığının karesinin kanat alanına oranına ($AR = b^2 / S$) açıklık oranı denir. Kanadı albatros gibi uzun ve ince yaptığınızda (yüksek $AR$), kanat uçlarının etkisi kanadın küçük bir bölümüne sınırlanır ve downwash azalır. Böylece indüklenmiş sürükleme belirgin şekilde düşer.

Kaldırma kuvvetinin toplam sürüklemeye oranı uçağın aerodinamik verimini, yani **Süzülme Oranını (Lift-to-Drag Ratio - $L/D$)** verir. Kutup eğrisi formülüyle ifade edersek:
$$C_D = C_{D0} + k C_L^2 \quad \text{burada} \quad k = \frac{1}{\pi e AR}$$

Uçağın azami süzülme oranı ise $(L/D)_{max} = \frac{1}{2\sqrt{C_{D0} k}}$ formülüyle hesaplanır. Örneğin referans tasarımımızda $AR = 8.0$ ve parazit sürükleme $C_{D0} = 0.0268$ iken azami süzülme oranı $(L/D)_{max} \approx 13.8$'dir. Bunun anlamı şudur: Uçak $1000\text{ metre}$ irtifada motoru kapansa bile, sakin havada yere temas edene kadar en iyi durumda yatayda yaklaşık $13.8\text{ kilometre}$ ileriye süzülebilir. Aynı oran, düz uçuşta her $1\text{ N}$ sürüklemeye karşı $13.8\text{ N}$ ağırlığın taşındığı anlamına gelir.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu hesaplamalar **"Sabit Kanat: Kanat Geometrisi & Kutup Eğrisi"** modülünde yer alır:
- **Kanat Açıklığı ($b = 1.60\text{ m}$)** ve **Kanat Alanı ($S = 0.32\text{ m}^2$)** girdiğinde, program açıklık oranını ($AR = 8.0$) otomatik hesaplar.
- Gövde ve bileşenlerin ıslak alanlarından hesaplanan parazit sürükleme ($C_{D0} \approx 0.0268$) ve indüklenmiş sürükleme faktörü ($k \approx 0.0488$) birleşerek uçağın **Azami Süzülme Oranını ($(L/D)_{max} \approx 13.8$)** ve **En İyi Menzil Hızını ($V_{range} \approx 11.9\text{ m/s}$)** ekrana yansıtır.

### Mini deney
1. **Açıklık Oranı Deneyi:** Kanat alanını $S = 0.32\text{ m}^2$ sabit tutarak kanat açıklığını $b = 1.60\text{ m}$'den $1.20\text{ m}$'ye düşür. $AR$ değerinin 8.0'dan 4.5'e gerilediğini, $k$ faktörünün yükseldiğini ve uçağın azami süzülme oranının ($(L/D)_{max}$) yaklaşık $13.8$'den $10.4$'e düştüğünü gözlemle.
2. **Parazit Sürükleme Deneyi:** Açıklığı $1.60\text{ m}$'de bırakıp uçağın dışına büyük, akım çizgisine uymayan bir kutu eklediğini düşün (program $C_{D0}$ değerini gövde ve bileşen ölçülerinden kendisi hesaplar; sen yalnızca ölçüleri değiştirirsin). Programın hesapladığı $C_{D0}$ yükseldikçe $(L/D)_{max}$ değerinin düştüğünü ve seyir için gereken elektrik gücünün arttığını gözlemle. Örneğin $C_{D0}$ $0.0268$'den $0.0400$'a çıkarsa $(L/D)_{max}$ yaklaşık $11.3$'e iner.

### Sık yapılan yanlışlar
1. **"Süzülme oranı ($L/D$) 10 ise, uçak 1 birim ileri gitmek için 10 birim yukarı tırmanır" yanılgısı:** Tam tersidir. $L/D = 10$, uçağın motorsuz bir süzülüşte kaybettiği her 1 metre irtifaya karşılık yatayda 10 metre ileri gideceğini ifade eder. Ya da uçağın ürettiği her 1 Newton sürüklemeye karşı 10 Newton kaldırma kuvveti oluşturduğunu gösterir.
2. **"Kanat açıklık oranını ($AR$) ne kadar büyütürsek uçak her yönden o kadar mükemmel olur" yanılgısı:** Açıklık oranını aşırı büyütmek (örneğin $AR > 14$) kanat kökünde devasa eğilme momentleri yaratır, kanadı ağırlaştırır ve yapısal esnemelere (aeroelastik çırpınma/flutter) neden olur. Küçük İHA'larda 7-10 arası oranlar ağırlık ve aerodinamik arasında en dengeli çözümdür.
3. **"Uçak ne kadar hızlı uçarsa sürükleme her zaman o kadar artar" yanılgısı:** Parazit sürükleme kuvveti hızla artarken, indüklenmiş sürükleme kuvveti hız arttıkça azalır. Çok düşük hızlarda indüklenmiş sürükleme o kadar büyüktür ki toplam sürükleme yüksek olur. Toplam sürükleme kuvveti, $(L/D)_{max}$ hızında minimuma iner; bu hızın hem altında hem üstünde sürükleme daha yüksektir.

### Mini test
**1. Sabit kanatlı bir İHA'da kanat açıklık oranının ($AR = b^2/S$) artırılması öncelikle hangi sürükleme türünü azaltır?**
A) Gövde parazit sürüklemesini
B) Kanat profili sürtünmesini
C) Kanat ucu girdaplarından kaynaklanan indüklenmiş sürüklemeyi
D) Ses üstü dalga sürüklemesini
*Doğru Cevap:* C
*Açıklama:* Açıklık oranının artırılması kanat uçlarını birbirinden uzaklaştırır ve kanat ucu girdaplarının toplam kanat üzerindeki etkisini zayıflatarak indüklenmiş sürüklemeyi ($C_{Di} = k C_L^2$) doğrudan azaltır.

**2. Süzülme oranı $(L/D) = 15$ olan bir İHA, rüzgârsız havada $500\text{ metre}$ irtifada motorunu kaybederse süzülerek yatayda en fazla kaç kilometre uzağa gidebilir?**
A) $1.5\text{ km}$
B) $7.5\text{ km}$
C) $15.0\text{ km}$
D) $30.0\text{ km}$
*Doğru Cevap:* B
*Açıklama:* Süzülme mesafesi formülü $R = h \times (L/D)$'dir. $500\text{ m} \times 15 = 7500\text{ metre} = 7.5\text{ kilometre}$.

**3. Bir İHA'nın toplam sürükleme kuvvetinin minimum olduğu ve $(L/D)_{max}$ seviyesine ulaştığı hızda parazit sürükleme ($C_{D0}$) ile indüklenmiş sürükleme ($C_{Di}$) arasındaki ilişki nedir?**
A) Parazit sürükleme sıfırdır
B) İndüklenmiş sürükleme parazit sürüklemenin üç katıdır
C) Parazit sürükleme ile indüklenmiş sürükleme birbirine eşittir ($C_{D0} = C_{Di}$)
D) İndüklenmiş sürükleme sıfırdır
*Doğru Cevap:* C
*Açıklama:* İdeal parabolik kutup eğrisinde azami süzülme oranının elde edildiği noktada ($C_L = \sqrt{C_{D0}/k}$), parazit sürükleme ile indüklenmiş sürükleme tam olarak birbirine eşittir ($C_{D0} = k C_L^2$).
