// SPDX-License-Identifier: AGPL-3.0-or-later
//! 'Neden bu sonuç çıktı?' statik ve Sokratik yapay zekâ açıklama motoru.
//! Hiçbir proje değerini değiştirmez; seviyeye göre derinlik sunar.

use crate::calc::Model;
use crate::i18n::Lang;
use crate::project::{DroneType, Project};
use crate::teach::lessons::lesson_for_id;
use crate::teach::state::StudentLevel;

/// Açıklanabilir kritik tasarım sonuçları.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultMetric {
    VStall,
    LdMax,
    StaticMargin,
    HoverThrottle,
    DiskLoading,
    FlightTime,
    WingLoading,
    SparStress,
}

impl ResultMetric {
    pub fn name(&self, lang: Lang) -> &'static str {
        match (self, lang) {
            (ResultMetric::VStall, Lang::Tr) => "Tutunma Hızı (Stall Hızı - V_stall)",
            (ResultMetric::VStall, Lang::En) => "Stall Speed (V_stall)",
            (ResultMetric::LdMax, Lang::Tr) => "Azami Süzülme Oranı ((L/D)_max)",
            (ResultMetric::LdMax, Lang::En) => "Maximum Lift-to-Drag Ratio ((L/D)_max)",
            (ResultMetric::StaticMargin, Lang::Tr) => "Statik Marj (Static Margin - SM)",
            (ResultMetric::StaticMargin, Lang::En) => "Static Margin (SM)",
            (ResultMetric::HoverThrottle, Lang::Tr) => "Askı Gaz Yüzdesi (Hover Throttle)",
            (ResultMetric::HoverThrottle, Lang::En) => "Hover Throttle Percentage",
            (ResultMetric::DiskLoading, Lang::Tr) => "Disk Yüklemesi (Disk Loading - DL)",
            (ResultMetric::DiskLoading, Lang::En) => "Disk Loading (DL)",
            (ResultMetric::FlightTime, Lang::Tr) => "Havada Kalma Süresi (Uçuş Süresi)",
            (ResultMetric::FlightTime, Lang::En) => "Flight Endurance (Flight Time)",
            (ResultMetric::WingLoading, Lang::Tr) => "Kanat Yüklemesi (W/S)",
            (ResultMetric::WingLoading, Lang::En) => "Wing Loading (W/S)",
            (ResultMetric::SparStress, Lang::Tr) => "Kiriş / Kol Emniyet Marjı (MS)",
            (ResultMetric::SparStress, Lang::En) => "Spar / Arm Margin of Safety (MS)",
        }
    }

    pub fn related_lesson_id(&self) -> u8 {
        match self {
            ResultMetric::VStall => 7,
            ResultMetric::LdMax => 2,
            ResultMetric::StaticMargin => 3,
            ResultMetric::HoverThrottle => 5,
            ResultMetric::DiskLoading => 5,
            ResultMetric::FlightTime => 4,
            ResultMetric::WingLoading => 6,
            ResultMetric::SparStress => 8,
        }
    }
}

/// Statik açıklama veri paketi.
#[derive(Clone, Debug, PartialEq)]
pub struct StaticExplanation {
    pub metric_name: String,
    pub current_value_str: String,
    pub formula: String,
    pub intuition: String,
    pub input_effects: Vec<String>,
    pub related_lesson_id: u8,
    pub related_lesson_title: String,
}

/// Yapay zekâ kapalıyken sunulan statik açıklama üreticisi.
pub fn explain_metric(
    metric: ResultMetric,
    p: &Project,
    m: &Model,
    level: StudentLevel,
    lang: Lang,
) -> StaticExplanation {
    let lesson_id = metric.related_lesson_id();
    let lesson_title = lesson_for_id(lesson_id, lang)
        .map(|l| l.title)
        .unwrap_or_else(|| format!("Ders {lesson_id}"));

    let metric_name = metric.name(lang).to_string();

    match metric {
        ResultMetric::VStall => {
            let v_stall = m.aero.v_stall;
            let current_value_str = format!("{:.2} m/s ({:.1} km/h)", v_stall, v_stall * 3.6);
            let formula = "V_stall = √( 2 · (W/S) / (ρ · C_L,max) )".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Uçak havadayken yerçekimine yenilmemek için kanadın altından ve üstünden hava akmalıdır. Uçak ne kadar ağırsa ve kanadı ne kadar küçükse, havada tutunabilmek için o kadar HIZLI uçmak zorundadır. Bu hızın altına düşerse kanat tutunamaz ve uçak burun aşağı düşer."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "To stay airborne, air must flow over the wings to generate lift equal to weight. The heavier the aircraft or the smaller the wings, the faster it must fly to stay in the air. Below this speed, airflow separates and the plane stalls."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Tutunma hızı, kanadın azami kaldırma katsayısında (C_L,max) taşıyabileceği asgari yatay hızdır. Kanat yükü (W/S) arttıkça veya irtifada hava yoğunluğu (ρ) azaldıkça tutunma hızı artar. Güvenli seyir için bu hızın en az 1.3 katı seçilmelidir."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Stall speed is the minimum airspeed at which the wing generates enough lift at C_L,max to balance weight. Higher wing loading or lower air density at altitude increases stall speed. Cruise speed is typically at least 1.3x stall speed."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Sınır tabaka ayrılma dinamiği: V_stall karesel olarak kanat yüküne ve ters orantılı olarak dinamik basınç katsayısına bağlıdır. dV_stall/dS = -(V_stall)/(2S) duyarlılığı vardır; kanat alanını büyütmek stall hızını doğrusal değil karekök oranında düşürür."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Boundary layer separation dynamics: V_stall scales with √(W/S) and 1/√(ρ·C_L,max). Sensitivity dV_stall/dS = -V_stall/(2S) indicates diminishing returns when expanding wing area to suppress stall speed."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Kalkış Kütlesi (MTOW = {:.2} kg): Kütle arttıkça kanat yükü artar ve stall hızı yükselir.",
                        m.weight.m_mtow
                    ),
                    format!(
                        "• Kanat Alanı (S = {:.3} m²): Kanat büyüdükçe kaldırma alanı genişler ve stall hızı düşer.",
                        p.s
                    ),
                    format!(
                        "• Uçuş İrtifası (h = {:.0} m): İrtifa arttıkça hava seyrekleşir (ρ = {:.3} kg/m³), bu da stall hızını artırır.",
                        p.h, m.aero.rho
                    ),
                    format!(
                        "• Kanat Profili (C_L,max = {:.3}): Daha kambur profil kaldırma üretir ve stall hızını düşürür.",
                        m.aero.cl_max
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Takeoff Mass (MTOW = {:.2} kg): Higher mass increases wing loading and drives stall speed up.",
                        m.weight.m_mtow
                    ),
                    format!(
                        "• Wing Area (S = {:.3} m²): Larger wing lowers wing loading and reduces stall speed.",
                        p.s
                    ),
                    format!(
                        "• Flight Altitude (h = {:.0} m): Thinner air at higher altitudes (ρ = {:.3} kg/m³) increases stall speed.",
                        p.h, m.aero.rho
                    ),
                    format!(
                        "• Airfoil (C_L,max = {:.3}): Higher maximum lift coefficient allows slower flight.",
                        m.aero.cl_max
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::LdMax => {
            let ld = m.aero.ld_max;
            let current_value_str = format!("{:.1}", ld);
            let formula = "(L/D)_max = 1 / ( 2 · √(C_D0 · k) )".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Süzülme oranı, uçağın motoru dursa bile 1 metre irtifa kaybında ileriye doğru kaç metre gidebileceğini söyler. Bu değer 14 ise, 100 metre yüksekten bırakılan uçak motorsuz 1400 metre süzülür! Uzun ve ince kanatlar kanat ucu girdaplarını azaltarak bu oranı yükseltir."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Glide ratio tells you how many meters forward the aircraft travels for every meter of altitude lost with zero thrust. If this is 14, gliding from 100 meters high covers 1,400 meters horizontally! Long slender wings reduce tip vortices and boost this ratio."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Azami süzülme oranı, parazit sürükleme (C_D0) ile kaldırmadan doğan indüklenmiş sürüklemenin (C_Di = k·C_L²) eşitlendiği noktada elde edilir. Açıklık oranı (AR) arttıkça k küçülür ve süzülme verimi artar."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Maximum lift-to-drag ratio occurs where parasite drag equals induced drag. Higher aspect ratio (AR) reduces induced factor k, yielding higher aerodynamic efficiency and range."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Kutup eğrisi optimizasyonu: C_L_range = √(C_D0/k). Optimum seyir hızı V_range bu noktaya karşılık gelir. Islak alanın gövde sürtünmesine etkisi parazit sürüklemeyi doğrudan belirler."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Drag polar optimality: C_L_range = √(C_D0/k). The best-range speed V_range occurs precisely at (L/D)_max, governed by parasite wetted friction and Oswald span efficiency factor e."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Kanat Açıklığı (b = {:.2} m, AR = {:.1}): Kanat uzadıkça girdap kayıpları azalır, L/D artar.",
                        p.b, m.wing.ar
                    ),
                    format!(
                        "• Parazit Sürükleme (C_D0 = {:.4}): Gövde ve pürüzler arttıkça süzülme oranı düşer.",
                        m.aero.cd0
                    ),
                    format!(
                        "• İndüklenmiş Faktör (k = {:.4}): Düşük k değeri yüksek kanat verimliliği sağlar.",
                        m.aero.k
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Wingspan (b = {:.2} m, AR = {:.1}): Longer wings suppress tip vortices and raise L/D.",
                        p.b, m.wing.ar
                    ),
                    format!(
                        "• Parasite Drag (C_D0 = {:.4}): Fuselage frontal area and skin friction reduce glide ratio.",
                        m.aero.cd0
                    ),
                    format!(
                        "• Induced Drag Factor (k = {:.4}): Lower k yields higher aerodynamic efficiency.",
                        m.aero.k
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::StaticMargin => {
            let sm = m.stab.sm * 100.0;
            let current_value_str = format!("%{:.1}", sm);
            let formula = "SM = (x_np - x_cg) / MAC".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Tıpkı arkasında tüy olan bir dart oku gibi, uçağın da dengede kalması için ağır olan Ağırlık Merkezinin, havanın tutunduğu Nötr Noktanın ÖNÜNDE olması gerekir. Ağırlık merkezi arkaya kaçarsa uçak havada takla atar ve kontrol edilemez!"
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Just like a dart with feathers at the back, an airplane requires its Center of Gravity to be IN FRONT of its aerodynamic Neutral Point. If the CG drifts behind the neutral point, the aircraft flips uncontrollably!"
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Statik marj, uçağın boyuna kararlılık payıdır. Önerilen yeşil bant %8 - %12 (varsayılan %10)'dur. %5'in altı aşırı hassasiyet ve diverjans riski yaratır; %15'in üstü uçağı aşırı burun-ağır yapar."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Static margin is the measure of longitudinal static stability. The recommended green band is 8% to 12% MAC (default 10%). Less than 5% invites pitch instability; over 15% requires excessive elevator deflection."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Cm_alpha türevi: C_m_alpha = -C_L_alpha_w * SM < 0 şartı geri getirici moment üretir. Yatay kuyruk hacim katsayısı V_H nötr noktayı geriye çekerek kararlılığı artırır."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Restoring pitch moment derivative: C_m_alpha = -C_L_alpha * SM. Tail volume ratio V_H drives neutral point aft, enlarging the permissible CG envelope."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Batarya Konumu (x_batt = {:.1} mm): Bataryayı öne çekmek CG'yi öne alır ve SM'i artırır.",
                        m.stab.batt_x_nose * 1000.0
                    ),
                    format!(
                        "• Yatay Kuyruk Alanı (S_h = {:.4} m²): Kuyruk büyüdükçe nötr nokta geriye gider, stabilite artar.",
                        p.s_h
                    ),
                    format!(
                        "• Kuyruk Kolu (l_h = {:.2} m): Uzun kuyruk gövdesi boyuna kararlılığı güçlendirir.",
                        p.l_h
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Battery Position (x_batt = {:.1} mm): Moving battery forward shifts CG forward and raises SM.",
                        m.stab.batt_x_nose * 1000.0
                    ),
                    format!(
                        "• Horizontal Tail Area (S_h = {:.4} m²): Larger tail pushes neutral point rearward, boosting stability.",
                        p.s_h
                    ),
                    format!(
                        "• Tail Arm (l_h = {:.2} m): Longer tail moment arm increases restoring pitch damping.",
                        p.l_h
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::HoverThrottle => {
            let throttle = m.mr.hover_throttle * 100.0;
            let current_value_str = format!("%{:.1}", throttle);
            let formula = "Throttle_hover ≈ √( 1 / TW )".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Multirotor havada asılı kalırken motorların gaz kolunun neresinde durduğunu gösterir. %50 civarı idealdir; %80'in üzerine çıkarsa motorlar zorlanır, ısınır ve ani bir rüzgarda drone yükselemez."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Indicates the throttle position required for a multirotor to hover steadily. Around 50% is ideal; above 80%, motors overheat and lack the reserve power needed to counter wind gusts."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "İtki motor devrinin karesiyle orantılı olduğundan gaz-itki ilişkisi doğrusal değildir, karekök eğrisini takip eder. TW ≥ 2.0 tasarımı askı gazını %50-70 bandında tutar."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Because thrust scales quadratically with motor RPM, throttle exhibits a square-root relation. Designing for TW ≥ 2.0 keeps hover throttle safely in the 50-70% operating range."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Rankine-Froude momentum teorisi ve motor Kv/voltaj kısıtı: Hover throttle = (T_hover / T_max)^0.5. Yüksek askı gazı otopilot kontrol doyumuna (control saturation) yol açabilir."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Rankine-Froude actuator disk model: Throttle_hover = (T_hover / T_max)^0.5. Elevated hover throttle limits autopilot attitude recovery headroom."
                }
            }
            .to_string();

            let prop_dia_in = p.mr_d_prop / 0.0254;
            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• İtki-Ağırlık Oranı (T/W = {:.2}): Yüksek T/W gaz kolunu aşağı çeker, motorları rahatlatır.",
                        m.mr.tw_ratio
                    ),
                    format!(
                        "• Multirotor Kütlesi (MTOW = {:.2} kg): Kütle arttıkça kol başına gereken itki artar.",
                        m.mr.m_mtow
                    ),
                    format!(
                        "• Pervane Çapı (D_prop = {:.1} inç): Büyük pervane daha düşük devirde aynı itkiyi verir.",
                        prop_dia_in
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Thrust-to-Weight Ratio (T/W = {:.2}): Higher T/W lowers required hover throttle.",
                        m.mr.tw_ratio
                    ),
                    format!(
                        "• UAV Mass (MTOW = {:.2} kg): Heavier craft requires more thrust per motor.",
                        m.mr.m_mtow
                    ),
                    format!(
                        "• Propeller Diameter (D_prop = {:.1} in): Larger diameter delivers thrust at lower RPM.",
                        prop_dia_in
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::DiskLoading => {
            let dl = m.mr.disk_loading;
            let current_value_str = format!("{:.1} N/m²", dl);
            let formula = "DL = T / A_disk,   P_ideal = T^1.5 / √(2·ρ·A)".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Büyük bir kürekle az sayıda yavaş kürek çekmek, küçük bir kaşıkla çılgınca su çırpmaktan çok daha az enerji harcar! Büyük pervaneler havayı daha geniş bir alana yayarak yavaşça aşağı üfler; bu da pili çok daha az tüketir."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Rowing with a wide oar takes far less effort than splashing frantically with a small spoon! Large propellers push a wider column of air gently downwards, saving huge amounts of battery energy."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Disk yüklemesi birim disk alanına düşen itkidir. Çap büyüdükçe disk alanı karesel büyür ve DL düşer. Düşük DL askıda kalma verimini önemli ölçüde artırır."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Disk loading is thrust divided by swept rotor area. Expanding diameter grows area quadratically and lowers DL, substantially improving hover efficiency."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Aktüatör disk teorisi: İndüklenmiş güç P_i = T * v_i = T * √(DL / (2ρ)). DL düştükçe indüklenmiş hava hızı düşer ve kinetik kayıp azalır."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Actuator disk theory: Induced hover power P_i = T · √(DL / (2ρ)). Minimizing disk loading reduces kinetic wake energy losses."
                }
            }
            .to_string();

            let prop_dia_in = p.mr_d_prop / 0.0254;
            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Pervane Çapı ({:.1} inç): Çap büyüdükçe disk alanı karesel büyür, DL düşer.",
                        prop_dia_in
                    ),
                    format!(
                        "• Kol / Motor Sayısı (n = {}): Kol sayısı arttıkça rotor başına itki payı azalır.",
                        p.mr_arm_count
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Propeller Diameter ({:.1} in): Expanding diameter squares disk area and cuts DL.",
                        prop_dia_in
                    ),
                    format!(
                        "• Rotor Count (n = {}): More rotors distribute total weight across more disk area.",
                        p.mr_arm_count
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::FlightTime => {
            let t_min = if p.drone_type == DroneType::Multirotor {
                m.mr.hover_endurance_min
            } else {
                m.perf_range.endurance_min()
            };
            let current_value_str = format!("{:.1} dk", t_min);
            let formula = "t = E_usable / P_electric = (E_pack · DoD) / P_electric".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Uçuş süresi bir su deposu gibidir. Bataryadaki kullanılabilir enerji deponun hacmi, motorların çektiği güç ise açık olan musluğun debisidir. Depoyu büyütmek veya motorun daha az güç çekmesini sağlamak uçuşu uzatır."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Flight endurance is like a water tank. Usable battery energy is the volume of the tank, and electrical power drawn by motors is the faucet flow rate. Enlarge the tank or reduce power consumption to fly longer."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Uçuş süresi batarya kapasitesi (Wh), Boşalma Derinliği (%80 DoD) ve motor-pervane verimiyle belirlenen elektriksel güç çekimine bağlıdır. Bataryayı aşırı büyütmek kütleyi artırıp verimi düşürebilir."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Endurance depends on usable battery energy (Wh at 80% DoD) divided by total electrical power. Adding too much battery weight causes diminishing returns due to higher required power."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Breguet menzil ve havada kalma denklemi: t = (E_spec · eta_total / g) · (C_L^1.5 / C_D) · (1/√W). Batarya kütle oranı f_batt için optimum aralık genellikle %30 - %45 arasındadır."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Breguet electric endurance equation: t = (E_spec · η_total / g) · (C_L^1.5 / C_D) · (1/√W). The optimum battery mass fraction f_batt typically peaks between 30% and 45%."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Batarya Kapasitesi (E = {:.1} Wh): Daha büyük batarya daha fazla enerji demektir.",
                        m.weight.pack_wh
                    ),
                    "• Boşalma Derinliği (DoD = %80): Batarya ömrü ve rezerv için %20 pay bırakılır.".to_string(),
                    "• Boş Kütle: Yapı hafifletilirse uçuş için gereken güç düşer ve süre uzar.".to_string(),
                ],
                Lang::En => vec![
                    format!(
                        "• Battery Capacity (E = {:.1} Wh): Larger pack stores more total energy.",
                        m.weight.pack_wh
                    ),
                    "• Depth of Discharge (DoD = 80%): 20% safety margin preserved for emergency go-arounds.".to_string(),
                    "• Empty Mass: Lighter structural mass reduces power draw and extends endurance.".to_string(),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::WingLoading => {
            let ws = m.ws;
            let current_value_str = format!("{:.1} N/m²", ws);
            let formula = "W/S = (m · g) / S".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Kanadın her bir metrekaresine kaç Newton ağırlık düştüğünü ifade eder. Düşük kanat yükü uçağın yavaşça ve kolayca süzülmesini sağlar; yüksek kanat yükü ise uçağın fırtınaya karşı hızlı ve kararlı uçmasını sağlar."
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Measures how many Newtons of weight each square meter of wing area must support. Low wing loading yields docile slow gliding; high wing loading cuts through turbulence at speed."
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Kanat yüklemesi tutunma hızı kısıtını ve fırlatma kolaylığını doğrudan belirler. Hobi İHA'larında tipik olarak 40 - 80 N/m² hedeflenir."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Wing loading dictates stall constraints and hand-launch capability. Hobby/student UAVs typically target 40 to 80 N/m²."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Kısıt analizi eşleştirme diyagramı (Matching Chart): (W/S)_max = 0.5 · ρ · V_stall_req² · C_L,max kuralı tasarım alanının sol sınırını belirler."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Constraint analysis matching chart: (W/S)_max = 0.5 · ρ · V_stall_req² · C_L,max establishes the vertical stall boundary."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    format!(
                        "• Uçak Ağırlığı (W = {:.1} N): Kütle arttıkça kanat yükü artar.",
                        m.weight.m_mtow * 9.81
                    ),
                    format!(
                        "• Kanat Alanı (S = {:.3} m²): Alan büyüdükçe yük azalır.",
                        p.s
                    ),
                ],
                Lang::En => vec![
                    format!(
                        "• Aircraft Weight (W = {:.1} N): Greater weight increases wing loading.",
                        m.weight.m_mtow * 9.81
                    ),
                    format!(
                        "• Wing Area (S = {:.3} m²): Larger wing surface relieves loading.",
                        p.s
                    ),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
        ResultMetric::SparStress => {
            let ms = if p.drone_type == DroneType::Multirotor {
                m.mr_st.ms
            } else {
                m.st.ms
            };
            let current_value_str = format!("MS = {:+.2}", ms);
            let formula = "MS = (σ_allow / σ_max) - 1   (MS ≥ 0 olmalı)".to_string();

            let intuition = match (level, lang) {
                (StudentLevel::Beginner, Lang::Tr) => {
                    "Uçağın kanat kirişi veya multirotor kolunun havadaki sert manevralara kırılmadan dayanıp dayanamayacağını ölçer. Güvenlik marjı pozitif (+0.2, +0.5 vb.) ise parça güvenlidir; sıfırın altındaysa parça kırılabilir!"
                }
                (StudentLevel::Beginner, Lang::En) => {
                    "Evaluates whether the wing spar or multirotor arm withstands high-G flight maneuvers without fracturing. A positive margin of safety (+0.2, +0.5) is safe; below zero implies structural failure!"
                }
                (StudentLevel::Intermediate, Lang::Tr) => {
                    "Maksimum eğilme gerilmesi (σ_max = M_b / Z) izin verilen malzeme gerilmesini aşmamalıdır. Emniyet katsayısı (FS = 1.5) uygulanır."
                }
                (StudentLevel::Intermediate, Lang::En) => {
                    "Bending stress (σ_max = M_b / Z) must remain below allowable yield strength under ultimate load (FS = 1.5)."
                }
                (StudentLevel::Advanced, Lang::Tr) => {
                    "Kesit modülü Z = (π/32)·(D_o⁴ - D_i⁴)/D_o ve kök momenti M_b = (n_ult · W · b) / (3π). Rijitlik ise titreşim rezonansı frekansını belirler."
                }
                (StudentLevel::Advanced, Lang::En) => {
                    "Section modulus Z = (π/32)·(D_o⁴ - D_i⁴)/D_o and root moment M_b = (n_ult · W · b) / (3π). Spar flexural rigidity determines natural modal frequencies."
                }
            }
            .to_string();

            let input_effects = match lang {
                Lang::Tr => vec![
                    "• Limit Yük Katsayısı: Manevralarda taşıyıcı boruya binen G kuvvetini belirler.".to_string(),
                    "• Boru Çapı ve Kalınlığı: Dış çap büyüdükçe kesit modülü kübik/dördüncü dereceden artar ve gerilme hızla düşer.".to_string(),
                ],
                Lang::En => vec![
                    "• Limit Load Factor: Dictates G-forces experienced during pull-up maneuvers.".to_string(),
                    "• Tube Diameter & Thickness: Section modulus scales with diameter to the fourth power.".to_string(),
                ],
            };

            StaticExplanation {
                metric_name,
                current_value_str,
                formula,
                intuition,
                input_effects,
                related_lesson_id: lesson_id,
                related_lesson_title: lesson_title,
            }
        }
    }
}

/// Yapay zekâ AÇIKKEN Sokratik öğretmen sistem istemi ve kullanıcı istemini oluşturur.
/// KESİNLİKLE hiçbir değer değişikliği önermez veya uygulamaz (`changes: []`).
pub fn build_socratic_teacher_prompts(
    metric: ResultMetric,
    p: &Project,
    m: &Model,
    level: StudentLevel,
    lang: Lang,
) -> (String, String) {
    let expl = explain_metric(metric, p, m, level, lang);

    let system_prompt = match lang {
        Lang::Tr => format!(
            "Sen 'WerSeuS – UAV Calc' programının sabırlı, öğrenciyi düşündüren Sokratik İHA tasarımı öğretmenisin.\n\
             Öğrenci Seviyesi: {} ({})\n\n\
             PEDAGOJİK KURALLAR:\n\
             1. Doğrudan hazır formül veya ezber cevap verme. Sokratik yöntemle öğrenciye yönlendirici sorular sorarak sonuca kendisinin ulaşmasını sağla (örneğin: 'Sence kanat alanını büyütürsek tutunma hızımız sence nasıl etkilenir?').\n\
             2. KESİNLİKLE hiçbir proje değerini DEĞİŞTİRME ve değer değişikliği ÖNERME ('şunu şuna ayarla' deme). 'changes' dizisi DAİMA BOŞ olmalıdır ([]).\n\
             3. Sayı UYDURMA. Yalnızca bağlamda verilen doğrulanmış proje sayılarını kullan.\n\
             4. Çıktı formatı tam olarak şu JSON olmalıdır:\n\
             {{\n  \"explanation\": \"Sokratik öğretmen yönlendirmesi ve soru.\",\n  \"changes\": []\n}}\n",
            level.label(Lang::Tr),
            level.description(Lang::Tr)
        ),
        Lang::En => format!(
            "You are the patient, inquisitive Socratic UAV design teacher of 'WerSeuS – UAV Calc'.\n\
             Student Level: {} ({})\n\n\
             PEDAGOGICAL RULES:\n\
             1. Do NOT lecture with blunt formulas or give away direct answers. Guide the student using Socratic inquiry with thought-provoking questions (e.g. 'What do you expect happens to stall speed if we enlarge wing area?').\n\
             2. STRICTLY DO NOT propose or apply any project parameter modifications. The 'changes' array MUST BE EMPTY ([]).\n\
             3. DO NOT hallucinate numbers. Use only the exact calculated engineering data provided in context.\n\
             4. The output must strictly be JSON:\n\
             {{\n  \"explanation\": \"Socratic teacher guidance and question.\",\n  \"changes\": []\n}}\n",
            level.label(Lang::En),
            level.description(Lang::En)
        ),
    };

    let user_prompt = match lang {
        Lang::Tr => format!(
            "ÖĞRENCİ SORUSU: 'Neden {} sonucum {} çıktı?'\n\n\
             MEVCUT PROJE SAYILARI:\n\
             - Platform Türü: {:?}\n\
             - Kalkış Kütlesi (MTOW): {:.2} kg\n\
             - Kanat Alanı (S): {:.3} m², Açıklık (b): {:.2} m\n\
             - İrtifa: {:.0} m (Hava yoğunluğu: {:.3} kg/m³)\n\
             - İlgili Formül: {}\n\
             - İlgili Ders: {}\n\n\
             Lütfen öğrenci seviyesine ({:?}) uygun, Sokratik yönlendirici bir açıklama yap.",
            expl.metric_name,
            expl.current_value_str,
            p.drone_type,
            m.weight.m_mtow,
            p.s,
            p.b,
            p.h,
            m.aero.rho,
            expl.formula,
            expl.related_lesson_title,
            level
        ),
        Lang::En => format!(
            "STUDENT INQUIRY: 'Why did my {} evaluate to {}?'\n\n\
             CURRENT PROJECT NUMBERS:\n\
             - Platform Type: {:?}\n\
             - Takeoff Mass (MTOW): {:.2} kg\n\
             - Wing Area (S): {:.3} m², Wingspan (b): {:.2} m\n\
             - Altitude: {:.0} m (Air density: {:.3} kg/m³)\n\
             - Governing Formula: {}\n\
             - Associated Lesson: {}\n\n\
             Please provide Socratic guidance tailored to the student's level ({:?}).",
            expl.metric_name,
            expl.current_value_str,
            p.drone_type,
            m.weight.m_mtow,
            p.s,
            p.b,
            p.h,
            m.aero.rho,
            expl.formula,
            expl.related_lesson_title,
            level
        ),
    };

    (system_prompt, user_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Library, evaluate};

    #[test]
    fn test_explain_metric_and_immutability() {
        let p = Project::default();
        let lib = Library::embedded();
        let m = evaluate(&p, &lib);

        for level in StudentLevel::ALL {
            for lang in [Lang::Tr, Lang::En] {
                let expl = explain_metric(ResultMetric::VStall, &p, &m, level, lang);
                assert!(!expl.metric_name.is_empty());
                assert!(!expl.current_value_str.is_empty());
                assert!(!expl.formula.is_empty());
                assert!(!expl.intuition.is_empty());
                assert!(!expl.input_effects.is_empty());
                assert_eq!(expl.related_lesson_id, 7);

                let (sys, user) =
                    build_socratic_teacher_prompts(ResultMetric::VStall, &p, &m, level, lang);
                assert!(sys.contains("Sokratik") || sys.contains("Socratic"));
                assert!(sys.contains("\"changes\": []"));
                assert!(!user.is_empty());
            }
        }
    }

    #[test]
    fn level_changes_text_and_prompt_uses_only_project_numbers() {
        let p = Project::default();
        let m = evaluate(&p, &Library::embedded());
        for lang in [Lang::Tr, Lang::En] {
            let b = explain_metric(ResultMetric::VStall, &p, &m, StudentLevel::Beginner, lang);
            let a = explain_metric(ResultMetric::VStall, &p, &m, StudentLevel::Advanced, lang);
            assert_ne!(b.intuition, a.intuition, "seviye metni etkilemeli");
            let (sys, user) = build_socratic_teacher_prompts(
                ResultMetric::VStall,
                &p,
                &m,
                StudentLevel::Beginner,
                lang,
            );
            assert!(sys.contains("[]"));
            assert!(user.contains(&format!("{:.2}", m.weight.m_mtow)));
            assert!(user.contains(&b.current_value_str));
        }
    }

    #[test]
    fn teacher_settings_roundtrip_json_without_disk() {
        let mut t = crate::teach::TeacherSettings::default();
        t.level = StudentLevel::Advanced;
        t.set_lesson_completed(3, true);
        let json = serde_json::to_string(&t).unwrap();
        let back: crate::teach::TeacherSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
        // Eski ayar dosyası (teacher alanı yok) varsayılanla yüklenmeli
        let old: crate::teach::TeacherSettings = serde_json::from_str("{}").unwrap();
        assert!(old.student_name.is_empty());
    }
}
