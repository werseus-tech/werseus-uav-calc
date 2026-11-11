// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel dışa aktarımındaki sabit metinlerin TR/EN çevirisi.
//! Kaynak kodda ad İngilizce, açıklama notu Türkçe yazıldığı için arama iki yönlüdür.

use crate::i18n::Lang;

/// (İngilizce, Türkçe) çiftleri.
pub(crate) const PAIRS: &[(&str, &str)] = &[
    // --- Girdi/hesap adları
    ("Cruise Altitude (MSL)", "Seyir İrtifası (MSL)"),
    ("Cruise Speed (Airspeed)", "Seyir Hızı (Hava Hızı)"),
    ("Hotel & Avionics Power", "Aviyonik ve Otel Gücü"),
    ("Payload Mass", "Faydalı Yük Kütlesi"),
    ("Avionics Mass", "Aviyonik Kütlesi"),
    ("Propulsion System Mass", "İtki Sistemi Kütlesi"),
    ("Fuselage & Tail Mass", "Gövde ve Kuyruk Kütlesi"),
    ("Cell Nominal Voltage", "Hücre Nominal Gerilimi"),
    ("Cell Capacity", "Hücre Kapasitesi"),
    ("Cell Mass", "Hücre Kütlesi"),
    ("Pack Series Count (S)", "Paket Seri Sayısı (S)"),
    ("Pack Parallel Count (P)", "Paket Paralel Sayısı (P)"),
    ("Pack Assembly Overhead", "Paket Montaj Ek Yükü"),
    ("Depth of Discharge (DOD)", "Deşarj Derinliği (DOD)"),
    ("Wing Span", "Kanat Açıklığı"),
    ("Wing Planform Area", "Kanat Alanı"),
    ("Wing Taper Ratio", "Kanat Daralma Oranı"),
    ("LE Sweep Angle", "Hücum Kenarı Ok Açısı"),
    ("Wing Thickness Ratio", "Kanat Kalınlık Oranı"),
    ("2D Lift Curve Slope", "2B Taşıma Eğrisi Eğimi"),
    ("2D Max Lift Coefficient", "2B Azami Taşıma Katsayısı"),
    (
        "Misc Parasite Drag Coeff",
        "Çeşitli Parazit Sürtünme Katsayısı",
    ),
    ("Cruise Propeller Efficiency", "Seyir Pervane Verimi"),
    ("Cruise Motor Efficiency", "Seyir Motor Verimi"),
    ("Cruise ESC Efficiency", "Seyir ESC Verimi"),
    ("Horizontal Tail Area", "Yatay Kuyruk Alanı"),
    ("Vertical Tail Area", "Dikey Kuyruk Alanı"),
    ("Tail Moment Arm", "Kuyruk Moment Kolu"),
    ("Frame Diagonal Size", "Gövde Köşegen Boyutu"),
    ("Motor / Arm Count", "Motor / Kol Sayısı"),
    (
        "Coaxial Layout (1 = yes, 0 = no)",
        "Koaksiyel Düzen (1 = evet, 0 = hayır)",
    ),
    ("Coaxial Interference Factor", "Koaksiyel Girişim Çarpanı"),
    ("Propeller Diameter", "Pervane Çapı"),
    ("Max Thrust per Motor", "Motor Başına Azami İtki"),
    ("Motor Mass (each)", "Motor Kütlesi (adet)"),
    ("Frame Structure Mass", "Şasi Yapı Kütlesi"),
    ("ESC Group Total Mass", "ESC Grubu Toplam Kütlesi"),
    ("Flight Controller Mass", "Uçuş Kontrolcüsü Kütlesi"),
    ("Misc Multirotor Hardware", "Çeşitli Multirotor Donanımı"),
    ("Propeller Figure of Merit", "Pervane Kalite Faktörü (FM)"),
    ("Hover Motor Efficiency", "Askı Motor Verimi"),
    ("Hover ESC Efficiency", "Askı ESC Verimi"),
    ("ISA Temperature", "ISA Sıcaklığı"),
    ("ISA Pressure", "ISA Basıncı"),
    ("Air Density (ISA)", "Hava Yoğunluğu (ISA)"),
    ("Battery Pack Mass", "Batarya Paketi Kütlesi"),
    ("Total Motor Count", "Toplam Motor Adedi"),
    (
        "Multirotor Take-Off Mass (MTOW)",
        "Multirotor Kalkış Kütlesi (MTOW)",
    ),
    ("Multirotor Weight", "Multirotor Ağırlığı"),
    (
        "Maximum Take-Off Mass (MTOW)",
        "Azami Kalkış Kütlesi (MTOW)",
    ),
    ("Total Aircraft Weight", "Toplam Uçak Ağırlığı"),
    (
        "Empty Mass (Structure + Systems)",
        "Boş Kütle (Yapı + Sistemler)",
    ),
    ("Aspect Ratio", "Açıklık Oranı"),
    ("Wing Root Chord", "Kanat Kök Kordu"),
    ("Wing Tip Chord", "Kanat Uç Kordu"),
    (
        "Mean Aerodynamic Chord (MAC)",
        "Ortalama Aerodinamik Kord (MAC)",
    ),
    ("Wing Loading (Weight-based)", "Kanat Yüklemesi (Ağırlık)"),
    ("Wing Loading (Mass-based)", "Kanat Yüklemesi (Kütle)"),
    ("Oswald Efficiency Factor", "Oswald Verim Faktörü"),
    ("Induced Drag Factor", "İndüklenmiş Sürükleme Faktörü"),
    ("3D Maximum Lift Coefficient", "3B Azami Taşıma Katsayısı"),
    ("Stall Speed", "Stall Hızı"),
    ("Stall Speed (km/h)", "Stall Hızı (km/sa)"),
    ("Cruise Lift Coefficient", "Seyir Taşıma Katsayısı"),
    (
        "Zero-Lift Parasite Drag Coeff",
        "Sıfır Taşımada Parazit Sürtünme Katsayısı",
    ),
    (
        "Induced Drag Coefficient",
        "İndüklenmiş Sürükleme Katsayısı",
    ),
    (
        "Total Cruise Drag Coefficient",
        "Toplam Seyir Sürükleme Katsayısı",
    ),
    ("Cruise Lift-to-Drag Ratio", "Seyir Taşıma/Sürükleme Oranı"),
    (
        "Maximum Glide Ratio (L/D_max)",
        "Azami Süzülüş Oranı (L/D_maks)",
    ),
    ("Total Powertrain Efficiency", "Toplam İtki Zinciri Verimi"),
    ("Usable Battery Energy", "Kullanılabilir Batarya Enerjisi"),
    ("Aerodynamic Thrust Power", "Aerodinamik İtki Gücü"),
    ("Propulsion Electrical Power", "İtki Elektrik Gücü"),
    (
        "Total Cruise Electrical Power",
        "Toplam Seyir Elektrik Gücü",
    ),
    ("Cruise Flight Endurance", "Seyir Havada Kalma Süresi"),
    ("Cruise Range", "Seyir Menzili"),
    ("Hover Thrust per Motor", "Motor Başına Askı İtkisi"),
    (
        "Hover Thrust per Motor (gram)",
        "Motor Başına Askı İtkisi (gram)",
    ),
    ("Single Propeller Disk Area", "Tek Pervane Disk Alanı"),
    ("Total Rotor Disk Area", "Toplam Rotor Disk Alanı"),
    ("Disk Loading", "Disk Yüklemesi"),
    ("Induced Inflow Velocity", "İndüklenmiş Hız"),
    ("Total Ideal Induced Power", "Toplam İdeal İndüklenmiş Güç"),
    ("Total Shaft Mechanical Power", "Toplam Şaft Mekanik Gücü"),
    ("Hover Motor Electrical Power", "Askı Motor Elektrik Gücü"),
    ("Total Hover Electrical Power", "Toplam Askı Elektrik Gücü"),
    (
        "Total Max Available Thrust",
        "Toplam Azami Kullanılabilir İtki",
    ),
    ("Thrust-to-Weight Ratio", "İtki/Ağırlık Oranı"),
    ("Hover Throttle Estimate", "Askı Gaz Tahmini"),
    ("Hover Endurance Time", "Askıda Kalma Süresi"),
    (
        "Center of Gravity (from Nose)",
        "Ağırlık Merkezi (Burundan)",
    ),
    ("Neutral Point (from Nose)", "Nötr Nokta (Burundan)"),
    ("Static Margin", "Statik Marj"),
    ("Spar Ultimate Bending Moment", "Spar Nihai Eğilme Momenti"),
    ("Max Spar Bending Stress", "Azami Spar Eğilme Gerilmesi"),
    ("Structural Margin of Safety", "Yapısal Emniyet Marjı"),
    ("Adjacent Tip-to-Tip Spacing", "Bitişik Motor Aralığı"),
    ("Blade Tip Clearance", "Pala Ucu Açıklığı"),
    (
        "CG Planar Offset from Center",
        "Merkezden Düzlemsel AM Sapması",
    ),
    ("Wing Span (b)", "Kanat Açıklığı (b)"),
    ("Wing Area (S)", "Kanat Alanı (S)"),
    ("Aspect Ratio (AR)", "Açıklık Oranı (AR)"),
    ("Wing Loading (M/S)", "Kanat Yüklemesi (M/S)"),
    ("Cruise L/D", "Seyir L/D"),
    // --- Notlar (kaynak Türkçe)
    ("Design cruise altitude", "Tasarım seyir irtifası"),
    ("Design cruise airspeed", "Tasarım hava seyir hızı"),
    (
        "Constant draw of avionics, camera, telemetry",
        "Aviyonik kartlar, kamera, telemetri sabit tüketim",
    ),
    (
        "Gimbal, camera or external payload",
        "Gimbal, kamera veya harici yük",
    ),
    ("FC, GPS, receiver, telemetry", "FC, GPS, alıcı, telemetri"),
    (
        "Pusher motor, ESC and propeller",
        "İtici motor, ESC ve pervane",
    ),
    (
        "Fuselage shell, tail and fasteners",
        "Gövde kabuğu, kuyruk ve bağlantı elemanları",
    ),
    (
        "Rated capacity of a single cell",
        "Tek hücre anma kapasitesi",
    ),
    ("Mass of a single cell", "Tek hücre kütlesi"),
    ("Cells in series", "Seri hücre adedi"),
    ("Cells in parallel", "Paralel hücre adedi"),
    (
        "Strip, wiring, BMS and packaging factor",
        "Şerit, kablo, BMS ve paketleme payı çarpanı",
    ),
    (
        "Usable fraction of battery capacity (typ. 80%)",
        "Kullanılabilir batarya kapasite oranı (tipik %80)",
    ),
    ("Tip-to-tip wing span", "Uçtan uca kanat açıklığı"),
    ("Wing reference area", "Kanat referans alanı"),
    (
        "Tip chord / root chord (c_t / c_r)",
        "Uç kordu / kök kordu oranı (c_t / c_r)",
    ),
    ("Leading-edge sweep angle", "Hücum kenarı ok açısı"),
    (
        "Airfoil 2D lift curve slope (2π per rad)",
        "Profil 2B taşıma eğrisi eğimi (2π rad)",
    ),
    (
        "Airfoil 2D maximum lift coefficient",
        "Profil 2B azami taşıma katsayısı",
    ),
    (
        "Landing gear, camera protrusion parasite drag",
        "İniş takımı, kamera çıkıntısı parazit sürtünme",
    ),
    (
        "Propulsive propeller cruise aerodynamic efficiency",
        "İtici pervane seyir aerodinamik verimi",
    ),
    (
        "Brushless pusher motor electromechanical efficiency",
        "Fırçasız itici motor elektromekanik verimi",
    ),
    (
        "Electronic speed controller electrical efficiency",
        "Elektronik hız kontrolcü elektriksel verimi",
    ),
    (
        "Horizontal stabilizer planform area",
        "Yatay stabilizör planform alanı",
    ),
    (
        "Vertical stabilizer planform area",
        "Dikey stabilizör planform alanı",
    ),
    (
        "Distance between wing AC and tail AC",
        "Kanat AC ile kuyruk AC arası mesafe",
    ),
    (
        "Distance between opposite motor centers",
        "Karşılıklı motor merkezleri mesafesi",
    ),
    (
        "2 motors per arm when coaxial",
        "Koaksiyelde kol başına 2 motor",
    ),
    (
        "1.0 when not coaxial; lower-rotor power factor when coaxial",
        "Koaksiyel olmayanda 1.0; koaksiyelde alt rotor güç çarpanı",
    ),
    (
        "Manufacturer full-throttle bench thrust",
        "Üretici tam gaz tezgâh itkisi",
    ),
    (
        "Mass of a single lift motor",
        "Tek bir kaldırma motoru kütlesi",
    ),
    (
        "Arms, plates and hardware mass",
        "Kollar, plakalar ve donanım kütlesi",
    ),
    ("Mass of all lift ESCs", "Tüm kaldırma ESC'leri kütlesi"),
    ("FC, power module, GPS", "FC, güç modülü, GPS"),
    (
        "Wiring, screws, landing gear allowance",
        "Kablaj, vidalar, iniş ayağı payı",
    ),
    (
        "Propeller hover efficiency factor (typ. 0.60-0.75)",
        "Pervane askı verim faktörü (tipik 0.60-0.75)",
    ),
    (
        "Motor electromechanical efficiency at hover RPM",
        "Askı devrinde motor elektromekanik verimi",
    ),
    (
        "ESC electrical efficiency at hover current",
        "Askı akımında ESC elektriksel verimi",
    ),
    (
        "T = T0 - L·h (ISA standard lapse rate)",
        "T = T0 - L·h (ISA standart lapse rate)",
    ),
    ("ρ = p / (R_air · T)", "ρ = p / (R_hava · T)"),
    (
        "m_bat = n_s · n_p · m_cell · k_pack",
        "m_bat = n_s · n_p · m_hücre · k_paket",
    ),
    (
        "MTOW = m_frame + n·m_motor + m_esc + m_fc + m_bat + m_payload + m_misc",
        "MTOW = m_şasi + n·m_motor + m_esc + m_fc + m_bat + m_yük + m_ek",
    ),
    (
        "n_motor = n_arm · (1 + coaxial)",
        "n_motor = n_kol · (1 + koaksiyel)",
    ),
    (
        "Empty mass = m_avionics + m_propulsion + m_fuselage_tail + m_wing",
        "Boş kütle = m_aviyonik + m_itki + m_gövde_kuyruk + m_kanat",
    ),
    (
        "MTOW = m_empty + m_payload + m_battery",
        "MTOW = m_boş + m_faydalı_yük + m_batarya",
    ),
    (
        "3D C_Lmax with finite-span and taper corrections",
        "3B kanat sonlu açıklık ve daralma düzeltmeli C_Lmax",
    ),
    (
        "Fuselage, wing, tail parasite drag build-up",
        "Gövde, kanat, kuyruk parazit sürtünme yığılımı (build-up)",
    ),
    (
        "η_total = η_esc · η_motor · η_prop",
        "η_total = η_esc · η_motor · η_pervane",
    ),
    (
        "E_usable = n_s · V_cell · n_p · C_cell · DOD",
        "E_kul = n_s · V_hücre · n_p · C_hücre · DOD",
    ),
    (
        "P_total = P_prop,el + P_hotel (avionics)",
        "P_toplam = P_itki,el + P_otel (aviyonik)",
    ),
    (
        "t = (E_usable [Wh] / P_total [W]) · 60 min",
        "t_uçuş = (E_usable [Wh] / P_total [W]) · 60 dk",
    ),
    (
        "R = V_cr · t (still-air level-flight range)",
        "R = V_cr · t_uçuş (rüzgârsız düz hat menzili)",
    ),
    (
        "Total motor count (2x arms when coaxial)",
        "Toplam motor adedi (koaksiyelde 2x kol)",
    ),
    (
        "A_total = n_arm · A_single",
        "A_toplam = n_kol · A_tek_disk",
    ),
    ("DL = W / A_total", "DL = W / A_toplam"),
    (
        "P_ideal = κ · W^1.5 / √(2·ρ·A_total)",
        "P_ideal = κ · W^1.5 / √(2·ρ·A_toplam)",
    ),
    ("P_shaft = P_ideal / FM", "P_şaft = P_ideal / FM"),
    (
        "P_el,motor = P_shaft / (η_motor · η_esc)",
        "P_el,motor = P_şaft / (η_motor · η_esc)",
    ),
    (
        "P_total = P_el,motor + P_hotel",
        "P_toplam = P_el,motor + P_otel",
    ),
    (
        "T_max = n_motors · T_motor,max",
        "T_azami = n_motors · T_motor,max",
    ),
    (
        "T/W = T_max / W (target ≥ 1.8)",
        "T/W = T_azami / W (hedef ≥ 1.8)",
    ),
    (
        "Throttle ≈ √(T_hover / T_max) = √(1 / (T/W))",
        "Gaz ≈ √(T_hover / T_max) = √(1 / (T/W))",
    ),
    (
        "t = (E_usable [Wh] / P_hover [W]) · 60 min",
        "t_askı = (E_usable [Wh] / P_hover [W]) · 60 dk",
    ),
    (
        "Balance of component masses and moment arms",
        "Bileşen kütle ve moment kolu dengesi",
    ),
    (
        "Balance of wing and tail lift centers",
        "Kanat ve kuyruk kaldırma merkezi dengesi",
    ),
    (
        "SM = (x_np - x_cg) / c̄ (target 8% - 15%)",
        "SM = (x_np - x_cg) / c̄ (hedef %8 - %15)",
    ),
    (
        "Limit load factor x 1.5 safety factor",
        "Limit yük faktörü x 1.5 emniyet katsayısı",
    ),
    (
        "MS = σ_allow / σ_max - 1 (must be > 0)",
        "MS = σ_izin / σ_max - 1 (> 0 olmalıdır)",
    ),
    (
        "Distance between adjacent motor centers",
        "Bitişik motor merkezleri mesafesi",
    ),
    (
        "Net gap between blade tips (collision margin)",
        "Pala uçları arası net açıklık (çarpışma payı)",
    ),
    (
        "Offset of the CG from the geometric center",
        "Ağırlık merkezinin geometrik merkezden sapması",
    ),
    (
        "Maximum take-off mass (MTOW)",
        "Azami Kalkış Kütlesi (MTOW)",
    ),
    (
        "Empty mass of structure, hardware and systems",
        "Gövde, donanım ve sistemler boş kütlesi",
    ),
    ("Payload mass", "Faydalı yük kütlesi"),
    ("Battery pack mass", "Batarya paketi kütlesi"),
    ("Aspect ratio (b² / S)", "Kanat açıklık oranı (b² / S)"),
    (
        "Flight mass per unit wing area",
        "Alan başına düşen uçuş kütlesi",
    ),
    (
        "Cruise lift-to-drag ratio",
        "Seyir süzülüş taşıma/sürükleme oranı",
    ),
    ("Level-flight endurance", "Düz uçuş havada kalma süresi"),
    ("Still-air cruise range", "Rüzgârsız seyir menzili"),
    ("Within target stability band", "Hedef stabilite bandında"),
    ("Outside the target band", "Hedef bandın dışında"),
    (
        "Thrust / weight ratio (at nominal voltage)",
        "İtki / Ağırlık oranı (nominal gerilimde)",
    ),
    (
        "Estimated hover throttle level",
        "Askıda kalma tahmini gaz seviyesi",
    ),
    ("Hover endurance", "Askıda kalma süresi (hover endurance)"),
    (
        "Total electrical hover power consumption",
        "Toplam elektriksel askı güç tüketimi",
    ),
];

/// Metni arayüz diline çevirir; eşleşme yoksa olduğu gibi döner.
pub(super) fn loc(lang: Lang, s: &str) -> String {
    let found = match lang {
        Lang::Tr => PAIRS.iter().find(|(en, _)| *en == s).map(|(_, tr)| *tr),
        Lang::En => PAIRS.iter().find(|(_, tr)| *tr == s).map(|(en, _)| *en),
    };
    found.unwrap_or(s).to_string()
}
