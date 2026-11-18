// SPDX-License-Identifier: AGPL-3.0-or-later
//! RF bağ bütçesi, anten yerleşimi ve elektromanyetik uyumluluk (EMI) denetimi.
//! Saf hesap kodu: arayüz bağımlılığı yok. Sayılar tasarım öncesi tarama (ön boyutlandırma)
//! içindir; gerçek menzil anten deseni, çevre ve alıcı kalitesine göre değişir.

pub mod emi;
pub mod link;
