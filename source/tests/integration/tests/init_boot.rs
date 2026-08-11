// SPDX-License-Identifier: GPL-3.0-or-later
//! What the canonical boot text is today, proved rather than asserted.
//!
//! docs/44 section 6 step 10 is the real `/system/boot/init.tos` executing
//! through the ordinary production path. This test does not fake that. It runs
//! the real file through the real frontend and records exactly where it stops,
//! so the claim "the canonical boot text is not TOS Core source yet" is
//! evidence rather than prose.
//!
//! `source/system/boot/init.tos` is the Stage 1 capsule's boot text. The file
//! says so itself: it is illustrative, the nucleus reads it only to verify that
//! the canonical boot file resolves and to expose its first logical line over
//! serial, and ADR-0015 gates a parser at Stage 1.5. Its bytes are Markdown
//! with an XML-comment SPDX header, not a TOS Core module.

use tos_core::{Parser, SourceReader};

const INIT: &[u8] = include_bytes!("../../../system/boot/init.tos");

#[test]
fn the_canonical_boot_text_is_not_yet_tos_core_source() {
    // It is transport-valid: UTF-8, NFC, no BOM, no bare CR. The source reader
    // accepts it, which is why the failure below is a language-level one.
    let source = SourceReader::read(INIT).expect("the boot text is transport-valid");

    let outcome = Parser::parse_schema(&source);
    assert!(
        outcome.has_errors(),
        "the boot text parsed as TOS Core; this test and the Stage 2 gate need updating"
    );
    let first = &outcome.diagnostics()[0];
    // The first byte that is not a TOS Core module header. A `.tos` file must
    // open with `module <name> version <v> profile <p>;` (docs/39 section 3).
    assert_eq!(
        first.code(),
        "E1013_UNEXPECTED_CHARACTER",
        "the boot text stops at a character that begins no lexical form"
    );
    assert!(
        outcome.into_accepted().is_none(),
        "nothing downstream may receive a schema for this file"
    );
}
