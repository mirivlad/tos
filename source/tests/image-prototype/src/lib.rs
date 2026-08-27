// SPDX-License-Identifier: GPL-3.0-or-later
//! The measurement-only compact module image (ADR-0070 §6).
//!
//! A library as well as a binary so that the ADR-0071 residency harness can
//! encode and parse the same images without a second copy of the codec. That is
//! the whole reason it is exposed: two encoders would be two things to keep in
//! agreement, and the evidence would stop being about one format.
//!
//! **Superseded, and kept as a historical measurement fixture.** The production
//! format is `tos-image` (`TOSIMAGE`, encoding version 1), which covers 100 % of
//! `tos-ir/v1` and closes docs/43 §1. This one is `TOSIMGx0`, encoding version
//! `0`, with payload coverage that is partial by declaration; it exists because
//! `STAGE3_COMPACT_IMAGE_P1.md` and `STAGE3_MODULE_RESIDENCY_P1.md` were
//! measured on it, and a measurement whose fixture has been replaced is not
//! reproducible. The engine never executes it, and nothing new should be built
//! on it.

pub mod image;
