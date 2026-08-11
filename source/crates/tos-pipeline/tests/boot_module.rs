// SPDX-License-Identifier: GPL-3.0-or-later
//! The repository's canonical boot module, through the ordinary path.
//!
//! `source/system/boot/init.tos` is the text the nucleus executes at boot. It
//! is bound here so that a change to it is checked by the same pipeline the
//! boot path uses, on the host, before anything reaches QEMU — and so that the
//! number the boot log reports has one place it is derived from.

use tos_pipeline::{execute, render, Request, Run, Silent};

const BOOT_TEXT: &str = include_str!("../../../system/boot/init.tos");
const BOOT_PATH: &str = "system/boot/init.tos";

/// What `main` returns: 1*2 + 2*3 + ... + 8*9.
const EXPECTED: i128 = 240;

fn boot_request() -> Request<'static> {
    Request {
        source_set: "tos-repository",
        path: BOOT_PATH,
        bytes: BOOT_TEXT.as_bytes(),
        entry: "main",
    }
}

#[test]
fn the_canonical_boot_module_runs_through_every_stage() {
    let run = execute(&boot_request(), Vec::new(), &mut Silent);
    let Run::Completed(completion) = &run else {
        panic!("the boot module must complete: {:?}", render::events(&run));
    };
    assert_eq!(completion.receipt.module_name, "system.boot.init");
    assert_eq!(
        completion.value,
        tos_engine::Value::Int(tos_ir::IntKind::I32, EXPECTED)
    );
}

#[test]
fn the_boot_module_stays_inside_every_resource_it_declares() {
    // The engine enforces these before the effect; asserting them here means a
    // change to the module that quietly needs more is caught on the host rather
    // than by a boot that traps.
    let run = execute(&boot_request(), Vec::new(), &mut Silent);
    let Run::Completed(completion) = run else {
        panic!("the boot module must complete");
    };
    let accounting = &completion.accounting;
    assert!(accounting.fuel_used <= accounting.fuel_limit);
    assert!(accounting.max_call_depth <= accounting.recursion_limit);
    assert!(accounting.allocation_peak <= accounting.allocation_limit);
    assert!(accounting.workers_reserved <= accounting.worker_limit);
    // A module that consumed nothing did not run.
    assert!(accounting.fuel_used > 0);
    assert!(accounting.max_call_depth > 0);
}

#[test]
fn the_boot_log_reports_the_answer_the_module_computes() {
    let run = execute(&boot_request(), Vec::new(), &mut Silent);
    let events = render::events(&run);
    assert!(
        events
            .iter()
            .any(|event| event == "TOS.RUN.COMPLETED value=i32:240"),
        "{events:?}"
    );
}
