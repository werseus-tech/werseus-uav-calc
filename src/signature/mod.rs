// SPDX-License-Identifier: AGPL-3.0-or-later
//! İz (signature) ve beka analizi (spesifikasyon modül 17): radar kesit alanı (RCS), kızılötesi
//! (IR) iz ve GNSS kaybında konum sapması (drift).
//!
//! Hepsi **eğitim amaçlı, mühendislik düzeyinde yaklaşık** modellerdir: kapalı formlu ders kitabı
//! formülleriyle küçük İHA tasarım ödünleşimlerini (malzeme, ısı kaynaklarının yerleşimi, IMU
//! sınıfı) karşılaştırmak içindir; belirli bir radar, kamera ya da sensör için tahmin değildir.
//! Hesaplar proje değerlerini değiştirmez.
//!
//! - [`rcs`]: gövdeyi basit şekillere (düz plaka, silindir, küre) ayırıp fiziksel optik (PO)
//!   yaklaşımıyla RCS (Knott, Shaeffer & Tuley; Skolnik).
//! - [`ir`]: motor/ESC/pil ısı kayıpları → kararlı yüzey sıcaklığı → Planck bant ışıması.
//! - [`drift`]: IMU sınıfı ve yardımcı sensörlere göre ölü hesap (dead-reckoning) hata bütçesi.

pub mod drift;
pub mod ir;
pub mod rcs;

/// Standart yerçekimi ivmesi [m/s²].
pub const G0: f64 = 9.80665;

/// Doğrusal değeri desibele çevirir (`10·log10(x)`); x ≤ 0 için çok küçük bir taban kullanılır.
pub fn to_db(x: f64) -> f64 {
    10.0 * x.max(1e-30).log10()
}

/// Desibelden doğrusala (`10^(dB/10)`).
pub fn from_db(db: f64) -> f64 {
    10f64.powf(db / 10.0)
}
