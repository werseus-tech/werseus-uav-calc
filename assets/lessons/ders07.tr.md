# Ders 7: Tutunma Kaybı (Stall): Neden Olur, Nasıl Önlenir ve Elle Fırlatma Hızı

### Ne öğreneceksin
- Sabit kanatlı bir uçakta sınır tabaka akış ayrılmasının ve kritik hücum açısının ($\alpha_{crit}$) tutunma kaybına (stall) nasıl yol açtığını.
- Düz uçuş tutunma hızının ($V_{stall}$) kanat yüklemesi ($W/S$) ve azami kaldırma katsayısı ($C_{L,max}$) ile formülsel ilişkisini.
- Elle fırlatmada (hand launch) stall'a girmemek için neden fırlatma hızının $V_{launch} \ge 1.1 - 1.2 V_{stall}$ olması gerektiğini ve fırlatma dinamiğini.

### Anlatım
Mutfak musluğundan akan suyun altına bir yemek kaşığının ters bombeli yüzeyini hafifçe değdirdiğinizi düşünün. Su akışı kaşığın kıvrımlı yüzeyine pürüzsüzce yapışarak akar ve kaşığı kendine doğru çeker. Ancak kaşığı akışa dik olacak şekilde aniden sert bir açıyla çevirirseniz, su artık yüzeyi takip edemez; her yöne sıçrayarak kaşıktan kopar. İşte bir uçağın kanadının üzerinden geçen havanın akışı da tıpkı bu su gibi davranır.

Uçak normal bir hücum açısında seyrederken, kanadın üst yüzeyindeki eğrilik havanın hızlanmasını ve emme basıncı oluşturarak kaldırma kuvveti üretmesini sağlar. Hava, kanat yüzeyine yakın ince bir sınır tabaka (boundary layer) halinde pürüzsüzce akar. Ancak pilot uçağın burnunu aşırı yukarı kaldırdığında (hücum açısı $\alpha$ arttığında), hava akışı kanadın arka kısmındaki yüksek basınç bölgesine karşı tırmanmak zorunda kalır. Hücum açısı **Kritik Hücum Açısını ($\alpha_{crit}$, genellikle $12^\circ - 15^\circ$)** aştığı anda, hava sınır tabakasının enerjisi tükenir; akış kanadın üst yüzeyinden kopar ve arkada girdaplı, çalkantılı bir iz bırakır.

Akış koptuğu anda kanadın kaldırma kuvveti aniden düşer ve basınç sürüklemesi hızla artar. Kanat uçağın ağırlığını taşıyamaz hale gelir ve uçak irtifa kaybederek düşmeye başlar. Bu olaya havacılıkta **Tutunma Kaybı (Stall)** denir. Sabit irtifada düz bir uçuşta kanadın azami kaldırma katsayısıyla ($C_{L,max}$) uçağın ağırlığını taşıyabildiği asgari hıza **Tutunma Hızı (Stall Speed - $V_{stall}$)** denir:
$$V_{stall} = \sqrt{\frac{2 (W/S)}{\rho C_{L,max}}}$$

Burada $W/S$ kanat yüklemesi ($\text{N/m}^2$), $\rho$ hava yoğunluğudur. Örneğin deniz seviyesinde ($\rho = 1.225\text{ kg/m}^3$), $2.0\text{ kg}$ kütleli ($W \approx 19.6\text{ N}$), kanat alanı $0.32\text{ m}^2$ ve $C_{L,max} = 1.125$ olan referans İHA'mızın tutunma hızı $V_{stall} = 9.43\text{ m/s}$ (yaklaşık $34\text{ km/sa}$) seviyesindedir. Uçak bu hızın altına düştüğünde kanatlar düz uçuşu sürdüremez.

Bu fiziğin sahada en kritik olduğu an **Elle Fırlatmadır (Hand Launch)**. Küçük bir sabit kanatlı İHA'yı elinizle fırlatırken uçağa ancak $6 - 8\text{ m/s}$ civarında bir başlangıç hızı kazandırabilirsiniz. Eğer uçağın tutunma hızı $9.4\text{ m/s}$ ise ve uçak fırlatıldığı anda motor yeterli ivmelenmeyi sağlayamazsa, kanatlar uçağı havada tutamaz. Bu sırada acemi bir pilot uçağın çöktüğünü görüp burnu yukarı çekmek için irtifa dümenine asılırsa, kanat anında kritik hücum açısını aşar; uçak derin bir stall'a girer ve burnu üzerine yere çakılır!

Bu nedenle güvenli bir elle fırlatma için şu iki kural şarttır:
1. Uçağın güvenli fırlatma hızı tutunma hızından en az %10-20 daha yüksek planlanmalıdır ($V_{launch} \ge 1.1 - 1.2 V_{stall}$, referans tasarımda $11.0\text{ m/s}$).
2. Motor, uçağı fırlatış anında $10^\circ$ tırmanış açısıyla ivmelendirecek yeterli statik itkiyi (İtki/Ağırlık oranı $T/W \ge 0.35 - 0.60$) üretebilmelidir.

### Programda nerede görürsün
**WerSeuS – UAV Calc** programında bu analizler **"Sabit Kanat: Tutunma & Elle Fırlatma Analizi"** modülünde yer alır:
- Kanat Profilinin 2 boyutlu taşıma katsayısı ($C_{l,max\_2d} = 1.25 \implies C_{L,max} = 1.125$) ve Kanat Yüklemesi ($W/S = 61.3\text{ N/m}^2$) girildiğinde, program **Deniz Seviyesi Tutunma Hızını ($V_{stall} = 9.43\text{ m/s}$)** ve **500 m İrtifa Tutunma Hızını ($V_{stall} = 9.66\text{ m/s}$)** gösterir.
- Fırlatma tırmanış açısı ($\gamma = 10^\circ$) ve ivme ($a = 1.0\text{ m/s}^2$) tanımlandığında program güvenli **Elle Fırlatma Hızını ($V_{launch} = 11.0\text{ m/s}$)** ve gereken asgari **Fırlatma İtki Oranını ($T/W = 0.348$)** hesaplar.

### Mini deney
1. **Kanat Yüklemesi ve Tutunma Hızı Deneyi:** Kanat alanını ($S$) $0.32\text{ m}^2$'den $0.20\text{ m}^2$'ye düşür. Kanat yükünün ($W/S$) fırladığını, tutunma hızının ($V_{stall}$) $9.43\text{ m/s}$'den $11.9\text{ m/s}$'ye çıktığını ve gereken elle fırlatma hızının insan gücüyle atılamayacak $14\text{ m/s}$ ($50\text{ km/sa}$) seviyesine yükseldiğini gözlemle.
2. **İrtifa Etkisi Deneyi:** Uçuş irtifasını deniz seviyesinden $2000\text{ metre}$ye çıkar ($\rho \approx 1.007\text{ kg/m}^3$). Havanın yoğunluğu azaldığı için uçağın tutunma hızının $10.4\text{ m/s}$'ye yükseldiğini ve yüksek irtifada uçağın stall olmaması için daha hızlı uçması gerektiğini gör.

### Sık yapılan yanlışlar
1. **"Stall uçağın motorunun bozulup susmasıdır" yanılgısı:** Stall motorla değil, kanat aerodinamiğiyle ilgilidir. Motor tam gaz çalışırken dahi hücum açısı kritik sınırı aşarsa kanattaki akış kopar ve uçak stall olur.
2. **"Multirotorlar da elle fırlatılırken sabit kanatlar gibi stall olur" yanılgısı:** Multirotorlarda sabit taşıyıcı kanat yoktur; kaldırmayı doğrudan dönen motorlar üretir ve yerden dikey olarak kalkarlar. Stall, ileri hızla hava akışı üreten sabit kanatlı uçaklara özgü aerodinamik bir sınır tabaka olgusudur.
3. **"Uçak elle fırlatılır fırlatılmaz irtifa kazanması için burun hemen yukarı çekilmelidir" yanılgısı:** Fırlatma anında uçak henüz tutunma hızının üzerine güvenle çıkmamıştır. Burun erkenden fazla yukarı çekilirse hücum açısı kritik değeri aşar ve uçak stall olup yere çakılır. Fırlatmadan hemen sonra uçak hafif yukarı bir yörüngede (yaklaşık $10^\circ$ tırmanış) tutulur ve motorun uçağı güvenli hıza ulaştırması beklenir.

### Mini test
**1. Sabit kanatlı bir uçakta "Tutunma Kaybı" (Stall) fiziksel olarak ne zaman gerçekleşir?**
A) Batarya tamamen bittiğinde
B) Kanadın hücum açısı kritik açıyı ($\alpha_{crit}$) aşıp hava akımı kanat üst yüzeyinden koptuğunda
C) Uçak rüzgârı arkasına aldığında
D) Uçağın ağırlığı kanat alanından küçük olduğunda
*Doğru Cevap:* B
*Açıklama:* Stall, kanadın hücum açısının kritik sınırı aşmasıyla üst yüzeydeki sınır tabakanın kanattan kopması ve kaldırma kuvvetinin aniden çökmesidir.

**2. Kanat yüklemesi $W/S = 61.3\text{ N/m}^2$, azami kaldırma katsayısı $C_{L,max} = 1.125$ ve hava yoğunluğu $\rho = 1.225\text{ kg/m}^3$ olan bir uçağın deniz seviyesindeki tutunma hızı ($V_{stall}$) yaklaşık kaç $\text{m/s}$'dir?**
A) $4.5\text{ m/s}$
B) $9.4\text{ m/s}$
C) $18.0\text{ m/s}$
D) $25.2\text{ m/s}$
*Doğru Cevap:* B
*Açıklama:* Formülde yerine koyarsak: $V_{stall} = \sqrt{\frac{2 \times 61.3}{1.225 \times 1.125}} = \sqrt{\frac{122.6}{1.3781}} = \sqrt{88.96} \approx 9.43\text{ m/s}$ (Doğrulanmış spesifikasyon Bölüm 4 test vektörü).

**3. Deniz seviyesinde tutunma hızı $V_{stall} = 9.4\text{ m/s}$ olan $2\text{ kg}$'lık bir İHA'nın elle güvenli bir şekilde fırlatılabilmesi için hedef fırlatma hızı ($V_{launch}$) yaklaşık ne olmalıdır?**
A) $3.0\text{ m/s}$
B) $6.0\text{ m/s}$
C) Yaklaşık $11.0\text{ m/s}$ (tutunma hızının yaklaşık %15-20 üzerinde)
D) $0\text{ m/s}$ (hareketsiz bırakılmalıdır)
*Doğru Cevap:* C
*Açıklama:* Fırlatma anında stall riskini bertaraf etmek için yaygın kural fırlatma hızının tutunma hızından en az %10 ila %20 daha yüksek tutulmasıdır ($V_{launch} \ge 1.1 - 1.2 V_{stall}$). $9.43 \times 1.17 \approx 11.0\text{ m/s}$ güvenli tasarım değeridir.
