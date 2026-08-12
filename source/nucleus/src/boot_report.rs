// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference path, presented on the boot console.
//!
//! This is the second consumer of events the boot path already produces. The
//! first is the serial log, which is normative; this one draws the same facts
//! for a person watching the screen:
//!
//! ```text
//! real boot/runtime fact
//!         |
//!         +----> normative serial event
//!         |
//!         +----> best-effort framebuffer presentation
//! ```
//!
//! Two things follow from that shape and are the whole point of keeping it.
//! Nothing here re-derives what the pipeline did: stage progress arrives
//! through the existing [`Trace`](tos_pipeline::Trace) contract, whose
//! `entering` is emitted *before* a stage runs, and the diagnosis is read from
//! the structured [`Run`] rather than parsed back out of rendered serial text.
//! And nothing here can affect the run: this module has no return value the
//! boot path reads.

use tos_pipeline::{PipelineStage, Run, Severity};

use crate::console::{BootConsole, Text};

/// Room for a location line. Long enough for a repository path with a line and
/// column; anything longer is truncated, because the serial log is where the
/// complete diagnostic lives.
pub type Location = Text<72>;

/// What a stage is called on a screen a person reads.
///
/// These name the stages of the actual reference path, in its order. A label
/// that promised a stage the pipeline does not have would be this module
/// inventing a boot model of its own, which is exactly what it must not do.
pub fn stage_label(stage: PipelineStage) -> &'static [u8] {
    match stage {
        PipelineStage::Read => b"Reading canonical source",
        PipelineStage::Parse => b"Parsing TOS Core V1",
        PipelineStage::Check => b"Checking source",
        PipelineStage::Resolve => b"Resolving modules",
        PipelineStage::Lower => b"Lowering to tos-ir/v1",
        PipelineStage::Verify => b"Verifying tos-ir/v1",
        PipelineStage::Execute => b"Executing boot module",
    }
}

/// Draws pipeline progress on the console. It never writes to serial: the
/// normative channel has its own consumer, and a presentation that could break
/// the log would not be best-effort.
pub struct ConsoleReporter<'a, 'fb> {
    console: &'a mut BootConsole<'fb>,
    path: &'a [u8],
}

impl<'a, 'fb> ConsoleReporter<'a, 'fb> {
    pub fn new(console: &'a mut BootConsole<'fb>, path: &'a [u8]) -> Self {
        Self { console, path }
    }

    /// A stage has been entered and has not run yet.
    ///
    /// Reaching the next stage is what proves the previous one returned, so the
    /// open row is resolved here rather than being marked finished by whoever
    /// happened to know first.
    pub fn entering(&mut self, stage: PipelineStage) {
        if self.console.is_busy() {
            self.console.succeed();
        }
        let detail = match stage {
            PipelineStage::Execute => Some(self.path),
            _ => None,
        };
        self.console.begin(stage_label(stage), detail);
    }

    /// The run is over. The open row becomes the outcome it actually had.
    pub fn finished(&mut self, run: &Run) {
        match failure(run, self.path) {
            None => self.console.succeed(),
            Some((code, location)) => self.console.fail(code.as_bytes(), location.as_bytes()),
        }
    }
}

/// The code and location a failed run should show, read from the structured
/// result. `None` when the run completed.
///
/// Every variant is kept apart, because they are refusals by different
/// components and a screen that collapsed them would tell an operator to go
/// looking in the wrong place.
pub fn failure(run: &Run, path: &[u8]) -> Option<(&'static str, Location)> {
    let mut location = Location::new();
    let code = match run {
        Run::Completed(_) => return None,
        Run::SourceRejected {
            code,
            byte_offset,
            path: unit,
        } => {
            // The refused unit rather than the entry: in a set they differ, and
            // an offset into the wrong file is worse than no offset at all.
            location
                .push(unit.as_bytes())
                .push(b" byte ")
                .push_number(*byte_offset);
            code
        }
        Run::Diagnosed { stage, diagnostics } => {
            match diagnostics
                .iter()
                .find(|entry| entry.severity() == Severity::Error)
            {
                Some(entry) => {
                    let at = entry.module().map(|module| module.path().as_bytes());
                    location
                        .push(at.unwrap_or(path))
                        .push(b":")
                        .push_number(entry.start().line())
                        .push(b":")
                        .push_number(entry.start().column());
                    entry.code()
                }
                // The pipeline only reports this refusal when an error is
                // present, so this arm names the stage rather than inventing a
                // code it did not produce.
                None => {
                    location
                        .push(path)
                        .push(b" stage ")
                        .push(stage.symbol().as_bytes());
                    "REFUSED"
                }
            }
        }
        Run::NotLowered(gap) => {
            location
                .push(gap.construct.as_bytes())
                .push(b" bytes ")
                .push_number(gap.byte_start)
                .push(b"..")
                .push_number(gap.byte_end);
            "NOT_LOWERED"
        }
        Run::Unverified(finding) => {
            if finding.location.is_empty() {
                location.push(path);
            } else {
                location.push(finding.location.as_bytes());
            }
            finding.code
        }
        // The refusal's reason is on serial in full. The screen says which
        // component refused and which module it refused, which is what tells an
        // operator where to look.
        Run::Refused(_) => {
            location.push(path);
            "ENGINE_REFUSED"
        }
        Run::Trapped { code, at, .. } => {
            match at {
                Some(site) => {
                    location
                        .push(site.path.as_bytes())
                        .push(b":")
                        .push_number(site.start.line())
                        .push(b":")
                        .push_number(site.start.column());
                }
                None => {
                    location.push(path).push(b" <unmapped>");
                }
            }
            code
        }
    };
    Some((code, location))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tos_pipeline::{execute, Request, Silent, Trace};

    /// Every stage of the reference path, in the order the path fixes.
    const ORDER: [PipelineStage; 7] = [
        PipelineStage::Read,
        PipelineStage::Parse,
        PipelineStage::Check,
        PipelineStage::Resolve,
        PipelineStage::Lower,
        PipelineStage::Verify,
        PipelineStage::Execute,
    ];

    /// Records what a presentation would have been asked to show, so stage
    /// ordering can be checked against the pipeline that produced it.
    #[derive(Default)]
    struct Recorder {
        entered: Vec<PipelineStage>,
    }

    impl Trace for Recorder {
        fn entering(&mut self, stage: PipelineStage) {
            self.entered.push(stage);
        }
    }

    const PRELUDE: &str = "module system.boot.init version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ";
    const PATH: &str = "system/boot/init.tos";

    fn execute_fixture(body: &str) -> (Run, Vec<PipelineStage>) {
        let text = format!("{PRELUDE} {body}");
        let request = Request {
            source_set: "tos-boot-console-tests",
            path: PATH,
            bytes: text.as_bytes(),
            entry: "main",
        };
        let mut recorder = Recorder::default();
        let run = execute(&request, Vec::new(), &mut recorder);
        (run, recorder.entered)
    }

    const GOOD: &str = "pub fn main() -> i32 { return 7i32; }";
    const REFUSED: &str = "pub fn main() -> i32 { return true; }";

    /// A label exists for every stage and no two stages share one: a screen
    /// that named two stages the same could not report which one stalled.
    #[test]
    fn every_stage_has_its_own_label() {
        for (index, stage) in ORDER.iter().enumerate() {
            assert!(!stage_label(*stage).is_empty());
            for other in &ORDER[index + 1..] {
                assert_ne!(stage_label(*stage), stage_label(*other));
            }
        }
    }

    /// The presentation cannot claim a stage the pipeline has not entered: the
    /// only thing it is ever told is `entering`, in the pipeline's own order.
    #[test]
    fn stages_are_announced_in_the_order_the_pipeline_enters_them() {
        let (run, entered) = execute_fixture(GOOD);
        assert!(run.is_completed(), "fixture must complete: {run:?}");
        assert_eq!(entered, ORDER);
    }

    /// A refused run announces the stages up to and including the one that
    /// refused, and no further stage is ever shown.
    #[test]
    fn a_refusal_stops_the_announcements_at_the_stage_that_refused() {
        let (run, entered) = execute_fixture(REFUSED);
        let stage = run.failed_at().expect("fixture must be refused");
        assert_eq!(stage, PipelineStage::Check);
        assert_eq!(*entered.last().expect("a stage was entered"), stage);
        let reached = ORDER.iter().position(|entry| *entry == stage).unwrap();
        assert_eq!(entered, ORDER[..=reached]);
    }

    #[test]
    fn a_completed_run_has_no_failure_to_show() {
        let (run, _) = execute_fixture(GOOD);
        assert!(failure(&run, PATH.as_bytes()).is_none());
    }

    /// The diagnosis comes from the structured result: the code the frontend
    /// produced, and the position it recorded.
    #[test]
    fn a_refused_run_shows_the_code_and_position_the_frontend_recorded() {
        let (run, _) = execute_fixture(REFUSED);
        let (code, location) = failure(&run, PATH.as_bytes()).expect("a failure");
        let expected = match &run {
            Run::Diagnosed { diagnostics, .. } => diagnostics
                .iter()
                .find(|entry| entry.severity() == Severity::Error)
                .expect("an error diagnostic"),
            other => panic!("unexpected run: {other:?}"),
        };
        assert_eq!(code, expected.code());
        let text = String::from_utf8(location.as_bytes().to_vec()).expect("ascii location");
        assert!(text.contains("system/boot/init.tos"), "location: {text}");
        assert!(
            text.ends_with(&format!(
                ":{}:{}",
                expected.start().line(),
                expected.start().column()
            )),
            "location: {text}"
        );
    }

    /// Transport refusals never reach a diagnostic, so their location is the
    /// byte offset the reader stopped at.
    #[test]
    fn a_source_rejection_shows_the_byte_it_stopped_at() {
        let request = Request {
            source_set: "tos-boot-console-tests",
            path: PATH,
            bytes: "\u{feff}module system.boot.init;".as_bytes(),
            entry: "main",
        };
        let run = execute(&request, Vec::new(), &mut Silent);
        assert_eq!(run.failed_at(), Some(PipelineStage::Read));
        let (code, location) = failure(&run, PATH.as_bytes()).expect("a failure");
        assert_eq!(code, "E1002_BOM_FORBIDDEN");
        let text = String::from_utf8(location.as_bytes().to_vec()).expect("ascii location");
        assert!(text.starts_with("system/boot/init.tos byte "), "{text}");
    }
}
