# Ders 5: Multirotor Nasıl Havada Durur? İtki, Pervane, Disk Yüklemesi ve Askı Gazı

### Ne öğreneceksin
- Çok rotorlu İHA'ların (multirotor) momentum teorisine göre pervanelerinden hava geçirerek nasıl itki ($T$) ürettiğini ve havada asılı (hover) kaldığını.
- Disk yüklemesi ($DL = T / A$) kavramını ve büyük pervanelerin neden daha az güç tüketerek daha verimli askı sağladığını.
- İtki-ağırlık oranının ($TW$) genel amaçlı bir multirotorda neden en az 2.0 hedeflendiğini ve askı gaz yüzdesinin doğrusal olmayan karekök formülüyle ($\text{Throttle}_{hover} \approx \sqrt{1/TW}$) nasıl hesaplandığını.

### Anlatım
Durgun bir gölde kanoyla ilerlediğinizi düşünün. Geniş bir kürekle suyu yavaşça geriye ittiğinizde tekneniz zahmetsizce ileri kayar. Elinizde küçük bir kaşık olsaydı, aynı itişi elde etmek için kaşığı suda çok hızlı ve çok daha fazla efor harcayarak çalkalamanız gerekirdi. İşte havada asılı duran (hover yapan) bir multirotorun pervaneleri de tam olarak bu kürek prensibiyle çalışır.

Bir multirotor sabit kanatlara sahip değildir; yerçekimini yenmek için ihtiyaç duyduğu tüm kaldırma kuvvetini doğrudan motorlarına bağlı dönen pervanelerle üretir. Klasik momentum teorisine (Rankine-Froude) göre, dönen her pervane diski ($A = \frac{\pi}{4} D_{prop}^2$), üstündeki durgun havayı yakalar ve pervanenin altından aşağı doğru belirli bir **İndüklenmiş Hızla ($v_i$)** fırlatır. Newton'ın etki-tepki yasası gereğince, aşağı fırlatılan bu hava kütlesi pervaneye yukarı doğru eşit bir **İtki Kuvveti ($T$)** uygular. $n$ kollu bir multirotorda ağırlık kollara eşit paylaştırılır ($T_{arm} = W / n$).

Burada tasarımın verimliliğini belirleyen en kritik aerodinamik ölçüt **Disk Yüklemesidir (Disk Loading - $DL$)**. Disk yüklemesi, pervanenin ürettiği itki kuvvetinin pervanenin süpürdüğü disk alanına oranıdır ($DL = T / A$, birimi $\text{N/m}^2$). Pervane çapını büyüttüğünüzde süpürülen disk alanı karesel olarak büyür ve disk yüklemesi dramatik şekilde düşer. Düşük disk yüklemesi demek, havanın çok yüksek hızlara çıkarılmadan büyük bir kütle halinde yavaşça aşağı üflenmesi demektir; bu da aynı itki için harcanan kinetik enerjiyi azaltarak askı verimini artırır.

Fizikte tek bir rotorun kayıpsız ideal askı gücü momentum teorisiyle şöyle ifade edilir:
$$P_{ideal} = T \cdot v_i = \frac{T^{1.5}}{\sqrt{2 \rho A}}$$

Gerçek dünyada pervanenin kayıpları Kalite Katsayısı (Figure of Merit, $FM \approx 0.65$), motor verimi ($\eta_{mot} \approx 0.85$) ve elektronik hız kontrolcü (ESC) verimi ($\eta_{esc} \approx 0.95$) devreye girdiğinde, $1.0\text{ kg}$ kütleli ($W \approx 9.81\text{ N}$) ve 5 inç pervaneli bir 250 mm quadcopter askıda kalabilmek için yaklaşık $166\text{ W}$ elektriksel güce ihtiyaç duyar.

Son olarak **İtki/Ağırlık Oranı ($TW$)** ve **Askı Gazı (Hover Throttle)** ilişkisine bakalım. Genel amaçlı bir multirotorda toplam azami itkinin aracın ağırlığının en az iki katı olması hedeflenir ($TW \ge 2.0$). Çünkü rüzgâra direnmek ve ani kaçış manevraları yapabilmek için motorda güç rezervi kalmalıdır. Pervanenin ürettiği itki dönüş devrinin karesiyle ($\omega^2$) orantılı olduğundan, askı gaz seviyesi doğrusal $1/TW$ değil, karekök ilişkisindedir:
$$\text{Throttle}_{hover} \approx \sqrt{\frac{1}{TW}}$$
$TW = 2.0$ olan bir sistemde bu basit modele göre askı gazı yaklaşık $\sqrt{0.5} \approx \%71$ çıkar (gerçek motor-pervane eğrilerinde genelde %50-70 arasındadır). $TW < 1.6$ olursa askı gazı azami değere tehlikeli şekilde yaklaşır; rüzgâra karşı koymak veya hızlı alçalmayı durdurmak (örneğin girdap halkası durumundan, vortex ring state, çıkmak) için neredeyse hiç güç payı kalmaz.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu analizler **"Multirotor (Çerçeve & Motor)"** modülünde (Sihirbazda **"Motor/İtki"** adımı) yer alır:
- Çerçeve Boyutu ($D_{frame} = 250\text{ mm}$), Kol Sayısı ($n=4$), Kalkış Kütlesi ($m = 1.0\text{ kg}$) ve Pervane Çapı ($D_{prop} = 5\text{ inç} = 0.127\text{ m}$) seçildiğinde, program kol başına düşen itkiyi ($T_{arm} \approx 2.45\text{ N}$) ve **Disk Yüklemesini ($DL = 193.5\text{ N/m}^2$)** anında hesaplar.
- Çıktı panelinde **İdeal Güç ($P_{ideal} \approx 87\text{ W}$)**, **Toplam Elektrik Askı Gücü ($P_{elec,hover} \approx 166\text{ W}$)** ve **Askı Gaz Yüzdesi** renkli göstergelerle sunulur.

### Mini deney
1. **Pervane Büyütme Deneyi:** Programda çerçeve izin veriyorsa pervane çapını 5 inçten (127 mm) 6 inçe (152.4 mm) çıkar. Disk alanının genişlemesiyle Disk Yüklemesinin ($DL$) $193.5\text{ N/m}^2$'den yaklaşık $134.4\text{ N/m}^2$'ye düştüğünü ve $1.0\text{ kg}$'lık drone'u havada tutmak için gereken elektrik gücünün yaklaşık %15-17 azaldığını gözlemle ($P \propto 1/\sqrt{A}$).
2. **Yetersiz İtki/Ağırlık Oranı Deneyi:** Pervaneyi 5 inçte tutarak İHA kütlesini ağır bir faydalı yükle $1.0\text{ kg}$'dan $1.5\text{ kg}$'a yükselt. $TW$ oranının $1.33$'e gerilediğini, askı gaz yüzdesinin %80'in üzerine çıktığını ve programın sarı-kırmızı renkte **"İtki/ağırlık oranı düşük (T/W < 1.6)"** ve **"Askı gaz yüzdesi yüksek (> %60)"** uyarıları verdiğini gör.

### Sık yapılan yanlışlar
1. **"Askı gazı doğrusal olarak $1/TW$'dir; $TW=2$ ise gaz tam %50'dedir" yanılgısı:** Pervane itkisi motor devrinin karesiyle orantılıdır. Bu nedenle gaz-itki ilişkisi doğrusal değildir; basit modelde karekök eğrisi kullanılır ($\text{Throttle} \approx \sqrt{1/TW}$).
2. **"Daha büyük pervane takmak disk yüklemesini artırır" yanılgısı:** Tam tersidir. Disk yüklemesi birim alana düşen itkidir ($DL = T / A$). Pervane çapı büyüdükçe disk alanı karesel büyür; dolayısıyla disk yüklemesi AZALIR. Düşük disk yüklemesi ise askı verimini artırır.
3. **"İtki/Ağırlık oranı 1.1 olsa bile drone ağırlığından fazla itki ürettiği için rahatça uçar" yanılgısı:** $TW = 1.1$ olan bir drone sadece tamamen rüzgârsız laboratuvar ortamında zar zor havalanabilir. Dış ortamda en ufak bir rüzgâr hamlesinde irtifa kaybeder, hızlı alçalmayı durduracak güç payı kalmadığı için toparlayamaz ve kontrolü kaybedebilir. Genel amaçlı multirotorlar için yaygın tasarım eşiği $TW \ge 2.0$'dır.

### Mini test
**1. $1.0\text{ kg}$ kalkış kütlesine sahip 4 motorlu bir quadcopterin motor başına üretebileceği azami statik itki $5.0\text{ N}$ ise, bu aracın İtki-Ağırlık Oranı ($TW$) yaklaşık kaçtır? ($g \approx 9.81\text{ m/s}^2$)**
A) 1.02
B) 2.04
C) 4.00
D) 0.51
*Doğru Cevap:* B
*Açıklama:* 4 motorun ürettiği toplam azami itki $T_{max,tot} = 4 \times 5.0\text{ N} = 20.0\text{ N}$'dur. Aracın ağırlığı $W = m \cdot g = 1.0 \times 9.81 = 9.81\text{ N}$'dur. İtki-ağırlık oranı $TW = 20.0 / 9.81 \approx 2.04$'tür.

**2. Çok rotorlu bir İHA'da pervane çapı büyütüldüğünde disk yüklemesi ($DL$) ve askı güç verimliliği nasıl değişir?**
A) Disk yüklemesi artar, güç verimliliği düşer
B) Disk yüklemesi azalır, güç verimliliği artar (daha az güçle havada kalır)
C) Disk yüklemesi ve güç verimliliği değişmez
D) Pervane büyüdükçe motor gücü sıfırlanır
*Doğru Cevap:* B
*Açıklama:* Disk alanı ($A = \frac{\pi}{4} D^2$) çapın karesiyle büyür. Böylece disk yüklemesi ($DL = T/A$) azalır. Düşük disk yüklemesi indüklenmiş hava hızını düşürerek kinetik enerji kaybını önler ve askı güç verimliliğini artırır.

**3. Genel amaçlı güvenli bir multirotor tasarımında asgari İtki-Ağırlık Oranı ($TW$) ne kadar olmalıdır?**
A) En az 0.8
B) En az 1.1
C) En az 2.0
D) En az 10.0
*Doğru Cevap:* C
*Açıklama:* Yaygın tasarım kuralına göre (örneğin Quan 2017), rüzgâra karşı koymak, manevra yapabilmek ve hızlı alçalmadan toparlanabilmek için güç payı bırakmak amacıyla genel amaçlı multirotorlarda $TW \ge 2.0$ hedeflenir.
