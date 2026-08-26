// SPDX-License-Identifier: GPL-3.0-or-later
//! The measurement-only compact module image (ADR-0070 §6).
//!
//! A library as well as a binary so that the ADR-0071 residency harness can
//! encode and parse the same images without a second copy of the codec. That is
//! the whole reason it is exposed: two encoders would be two things to keep in
//! agreement, and the evidence would stop being about one format.
//!
//! **This is not a production format.** The magic is `TOSIMGx0`, the encoding
//! version is `0`, its payload coverage is partial by declaration, and
//! ADR-0070 §7 gates production engine integration on a format that covers
//! 100 % of `tos-ir/v1` and closes docs/43 §1 in full.

pub mod image;
