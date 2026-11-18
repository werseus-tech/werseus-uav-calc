// SPDX-License-Identifier: AGPL-3.0-or-later
//! Uçuş logu (kara kutu) CSV içe aktarma ve tasarım-gerçek karşılaştırması.
//! Saf kod: ayrıştırma (`csv`), kesim/enerji/iç direnç analizi (`segments`), tasarımla kıyas (`compare`).

pub mod compare;
pub mod csv;
pub mod segments;
