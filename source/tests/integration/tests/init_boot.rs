// SPDX-License-Identifier: GPL-3.0-or-later
//! The canonical boot text, from capsule bytes to an executed result.
//!
//! docs/44 section 6 step 10 is the real `/system/boot/init.tos` executing
//! through the ordinary production path. This test takes the boot content out
//! of the golden capsule — the same bytes the loader hands the nucleus, carried
//! through the capsule format rather than read off the filesystem — and runs it
//! through the reference path.
//!
//! Reading the capsule rather than the file is the point. The file on disk is
//! covered where the pipeline is tested; what is proved here is that the bytes
//! that actually reach a booting machine are a TOS Core module and that they
//! execute. A capsule builder that mangled the boot text would pass every test
//! that reads the source directly.

use tos_capsule::parse;
use tos_pipeline::{execute, render, Request, Run, Silent, Unreachable};

const CAPSULE: &[u8] = include_bytes!("../../vectors/capsule-v1/valid-001.bin");

/// What `main` returns: 1*2 + 2*3 + ... + 8*9.
const EXPECTED: i128 = 240;

#[test]
fn the_canonical_boot_text_is_a_tos_core_module_that_runs() {
    let capsule = parse(CAPSULE).expect("the golden capsule parses");
    let boot = capsule.boot_file().expect("a canonical boot file");
    assert_eq!(boot.name, b"/system/boot/init.tos");

    let request = Request {
        source_set: "tos-capsule-golden",
        // The capsule stores an absolute canonical path; the module root is the
        // repository root, so the module-relative path is the same without its
        // leading separator.
        path: "system/boot/init.tos",
        bytes: boot.content,
        entry: "main",
    };
    let run = execute(&request, Vec::new(), &mut Silent, &mut Unreachable);
    let Run::Completed(completion) = &run else {
        panic!(
            "the canonical boot text must execute: {:?}",
            render::events(&run)
        );
    };
    assert_eq!(completion.receipt.module_name, "system.boot.init");
    assert_eq!(
        completion.value,
        tos_engine::Value::Int(tos_ir::IntKind::I32, EXPECTED)
    );
    // The engine ran this exact module, not one that merely looked like it.
    assert_eq!(
        completion.receipt.module_digest,
        tos_ir::module_digest(&{
            let source = tos_core::SourceReader::read(boot.content).expect("transport-valid");
            let schema = tos_core::Parser::parse_schema(&source)
                .into_accepted()
                .expect("parses");
            tos_core::lower_module(
                &source,
                &schema,
                &tos_core::ModuleContext {
                    source_set: String::from("tos-capsule-golden"),
                    path: String::from("system/boot/init.tos"),
                    content_id: tos_pipeline::content_id(source.bytes()),
                    dependency_digest: tos_pipeline::list_digest(&[]),
                    capability_interface_digest: tos_pipeline::list_digest(&[]),
                },
            )
            .expect("lowers")
        })
    );
}
