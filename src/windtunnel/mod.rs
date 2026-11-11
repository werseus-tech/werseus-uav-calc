// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rüzgâr tüneli (ön tasarım düzeyi): 2D panel + sınır tabaka, 3D VLM. Proje değerlerini
//! DEĞİŞTİRMEZ; yalnızca karşılaştırma sonuçları üretir.
// Sayısal çekirdek: `!(x > 0.0)` kalıbı NaN'ı da reddetmek için bilinçlidir; indeksli döngüler
// matris/panel formüllerini kaynaklardaki (Katz & Plotkin) biçimiyle okunur tutar.
#![allow(clippy::neg_cmp_op_on_partial_ord, clippy::needless_range_loop)]
pub mod airfoil;
pub mod boundary_layer;
pub mod linalg;
pub mod panel;
pub mod polar;
pub mod vlm;
