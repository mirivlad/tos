// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded resident set and a measurement engine to drive it.
//!
//! ADR-0071 §5–§7, built to be measured rather than to be fast or complete.
//! **This is not `tos-engine` and nothing is switched onto it.** It executes
//! only what the residency fixture needs — enough to cross module boundaries,
//! suspend a caller, evict the module it is suspended in, reload it and return
//! into it — and refuses everything else by name, so a workload can never
//! quietly measure something other than what it claims.
//!
//! Two properties are the reason it exists at all:
//!
//! - **a continuation names identities, never addresses** (§6). A [`Frame`]
//!   holds a `ClosureModuleId`, a function index, a block index and an
//!   instruction index, and its own values. Nothing in it points into an image,
//!   so evicting the module a frame is suspended in is survivable — and the
//!   harness proves that by doing it rather than asserting it;
//! - **the byte bound counts module-derived state** (§7), not image bytes. A
//!   resident module costs its image, whatever was decoded from it, and its
//!   bookkeeping, and all three are inside the bound.

use std::collections::BTreeMap;

use tos_image_prototype::image;
use tos_ir::{BinaryOp, CallTarget, IntKind, Module, Op, Operand, Terminator, UnaryOp};
use tos_verifier::Limits;

use crate::closure::{ClosureModuleId, Failure, Provider, Snapshot, VerifiedModuleRecord};

/// One resident module and everything it keeps alive.
pub struct Resident {
    pub id: ClosureModuleId,
    /// The immutable snapshot that was hashed and then parsed. Held for its
    /// lifetime rather than read and dropped, so that what is resident is the
    /// artifact and not merely something derived from it — and so that
    /// releasing a resident releases the image too (§7).
    #[allow(dead_code)]
    pub snapshot: Snapshot,
    pub module: Module,
    pub image_bytes: usize,
    /// Measured, not estimated: the arena's committed delta across the parse.
    pub decoded_bytes: usize,
    /// The export index, built on first use and evicted with the module.
    ///
    /// ADR-0071 §2 keeps function resolution out of the manifest: the manifest
    /// fixes *which module*, and *which function of it* is reconstructed inside
    /// that already-fixed module. So this is resident module-derived state under
    /// §7 — inside the byte bound, and gone when the module goes.
    exports: BTreeMap<String, usize>,
    index_bytes: usize,
    /// This module's `import slot -> ClosureModuleId` mapping, resolved against
    /// the trusted closure membership on first use.
    ///
    /// ADR-0071 §2 keeps import slots out of the permanent manifest: an import
    /// slot is a property of a module, and the module's own verified artifact
    /// already states what its imports resolved to. So this is resident
    /// module-derived state under §7 — inside the byte bound, gone when the
    /// module goes.
    imports: Vec<ClosureModuleId>,
    import_bytes: usize,
    used_at: u64,
}

/// The three components of §7, and where they sit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ledger {
    pub image_bytes: usize,
    pub decoded_bytes: usize,
    /// The export index of §2 — derived from a resident module, evicted with it.
    pub index_bytes: usize,
    /// The import-slot mapping of §2, likewise.
    pub import_bytes: usize,
    pub bookkeeping_bytes: usize,
}

impl Ledger {
    pub fn total(&self) -> usize {
        self.image_bytes
            + self.decoded_bytes
            + self.index_bytes
            + self.import_bytes
            + self.bookkeeping_bytes
    }
}

/// What happened while the bound was being kept.
#[derive(Clone, Copy, Debug, Default)]
pub struct Traffic {
    pub loads: usize,
    pub evictions: usize,
    /// Evictions of a module a live frame was suspended inside. The case §6
    /// exists for, counted so that a run which never hit it cannot be reported
    /// as if it had.
    pub evictions_while_suspended: usize,
    pub reloads_of_suspended: usize,
    pub bytes_hashed: usize,
    pub hashes: usize,
    pub calls: usize,
    pub returns: usize,
    pub instructions: usize,
    pub peak_ledger: usize,
}

/// A resident set bounded by count **and** by module-derived bytes.
pub struct ResidentSet {
    bound_count: usize,
    bound_bytes: usize,
    live: Vec<Resident>,
    clock: u64,
    pub traffic: Traffic,
    /// Positions whose reload is a reload of a suspended frame's module.
    suspended_reload_watch: Vec<ClosureModuleId>,
    arena_committed: fn() -> usize,
}

impl ResidentSet {
    pub fn new(bound_count: usize, bound_bytes: usize, arena_committed: fn() -> usize) -> Self {
        assert!(bound_count >= 1, "ADR-0071 §7: the minimum bound is one");
        ResidentSet {
            bound_count,
            bound_bytes,
            live: Vec::new(),
            clock: 0,
            traffic: Traffic::default(),
            suspended_reload_watch: Vec::new(),
            arena_committed,
        }
    }

    pub fn ledger(&self) -> Ledger {
        Ledger {
            image_bytes: self.live.iter().map(|r| r.image_bytes).sum(),
            decoded_bytes: self.live.iter().map(|r| r.decoded_bytes).sum(),
            index_bytes: self.live.iter().map(|r| r.index_bytes).sum(),
            import_bytes: self.live.iter().map(|r| r.import_bytes).sum(),
            bookkeeping_bytes: core::mem::size_of::<ResidentSet>()
                + self.live.capacity() * core::mem::size_of::<Resident>(),
        }
    }

    fn find(&self, id: ClosureModuleId) -> Option<usize> {
        self.live.iter().position(|r| r.id == id)
    }

    pub fn module_of(&self, id: ClosureModuleId) -> Option<&Module> {
        self.find(id).map(|at| &self.live[at].module)
    }

    /// Which module a resident caller's import slot names.
    ///
    /// Resolved against the trusted closure membership, once, when the caller is
    /// resident. Not module search: the answer can only be a member of the
    /// closure the manifest already fixed, and a slot naming anything else has
    /// no answer at all.
    fn import_of(
        &mut self,
        id: ClosureModuleId,
        slot: usize,
        manifest: &crate::closure::VerifiedClosureManifest,
    ) -> Option<ClosureModuleId> {
        let at = self.find(id)?;
        if self.live[at].imports.is_empty() && !self.live[at].module.imports.is_empty() {
            let before = (self.arena_committed)();
            let mut resolved = Vec::with_capacity(self.live[at].module.imports.len());
            for declared in &self.live[at].module.imports {
                let content_id = crate::closure::identity_digest(&declared.module_content_id);
                resolved.push(manifest.resolve(&declared.module_name, &content_id)?);
            }
            self.live[at].imports = resolved;
            self.live[at].import_bytes = (self.arena_committed)().saturating_sub(before);
        }
        self.live[at].imports.get(slot).copied()
    }

    /// Which function of a resident module an export name reaches.
    ///
    /// Reconstructed inside the module the manifest already fixed, and cached
    /// as resident state. This is not module search: the module is not being
    /// chosen here, it was chosen at launch and the provider cannot widen it.
    fn export_of(&mut self, id: ClosureModuleId, name: &str) -> Option<usize> {
        let at = self.find(id)?;
        if self.live[at].exports.is_empty() {
            let before = (self.arena_committed)();
            let mut exports = BTreeMap::new();
            for (index, function) in self.live[at].module.functions.iter().enumerate() {
                exports
                    .entry(function.signature.name.clone())
                    .or_insert(index);
            }
            self.live[at].exports = exports;
            self.live[at].index_bytes = (self.arena_committed)().saturating_sub(before);
        }
        self.live[at].exports.get(name).copied()
    }

    /// Makes `id` resident, evicting whatever the bounds require.
    ///
    /// `suspended` is the modules live frames are suspended in — not a
    /// protection, the opposite: they may be evicted, and that is what §6
    /// claims is safe. It is passed so the eviction can be *counted*.
    pub fn ensure(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn Provider,
        records: &[VerifiedModuleRecord],
        limits: &Limits,
        suspended: &[ClosureModuleId],
    ) -> Result<(), Failure> {
        self.clock += 1;
        if let Some(at) = self.find(id) {
            self.live[at].used_at = self.clock;
            return Ok(());
        }

        // Room by count first, so the load never overshoots the count bound.
        while self.live.len() >= self.bound_count {
            self.evict_one(suspended);
        }

        if self.suspended_reload_watch.contains(&id) {
            self.traffic.reloads_of_suspended += 1;
            self.suspended_reload_watch.retain(|watched| *watched != id);
        }

        let resident = self.load(id, provider, records, limits)?;
        self.live.push(resident);

        // Then room by bytes. The just-loaded module is never the victim: an
        // execution must be able to make progress with one resident module
        // (§7), so a bound too small for a single module fails the execution
        // rather than evicting what it is about to use.
        while self.ledger().total() > self.bound_bytes && self.live.len() > 1 {
            self.evict_one_except(id, suspended);
        }

        self.traffic.peak_ledger = self.traffic.peak_ledger.max(self.ledger().total());
        Ok(())
    }

    fn evict_one(&mut self, suspended: &[ClosureModuleId]) {
        let Some(victim) = self.least_recent(None) else {
            return;
        };
        self.evict_at(victim, suspended);
    }

    fn evict_one_except(&mut self, keep: ClosureModuleId, suspended: &[ClosureModuleId]) {
        let Some(victim) = self.least_recent(Some(keep)) else {
            return;
        };
        self.evict_at(victim, suspended);
    }

    fn least_recent(&self, keep: Option<ClosureModuleId>) -> Option<usize> {
        self.live
            .iter()
            .enumerate()
            .filter(|(_, r)| Some(r.id) != keep)
            .min_by_key(|(_, r)| r.used_at)
            .map(|(at, _)| at)
    }

    fn evict_at(&mut self, at: usize, suspended: &[ClosureModuleId]) {
        let resident = self.live.remove(at);
        self.traffic.evictions += 1;
        if suspended.contains(&resident.id) {
            self.traffic.evictions_while_suspended += 1;
            if !self.suspended_reload_watch.contains(&resident.id) {
                self.suspended_reload_watch.push(resident.id);
            }
        }
        // Image, decoded module and the residency entry all go together.
        // Releasing the image and keeping what was decoded from it would put
        // the execution under the byte bound while holding thirty-three times
        // the image in derived state.
        drop(resident);
    }

    /// One load: obtain the immutable snapshot, hash **that exact snapshot**,
    /// compare against the trusted artifact digest, and only then parse it.
    ///
    /// ADR-0071 §5. The semantic digest is not recomputed and the verifier does
    /// not run again: trust was established at launch and is carried by the
    /// artifact digest, which is a commitment to this exact byte sequence.
    fn load(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn Provider,
        records: &[VerifiedModuleRecord],
        limits: &Limits,
    ) -> Result<Resident, Failure> {
        let position = id.position();
        let snapshot = provider.image(id).ok_or(Failure::Missing(position))?;

        // The snapshot is immutable, so these are the same bytes throughout:
        // hashed here, parsed below, executed after. There is no window in
        // which they could differ, because there is no way to write them.
        self.traffic.hashes += 1;
        self.traffic.bytes_hashed += snapshot.len();
        let digest = tos_hash::sha256(&snapshot);
        if digest != records[position].artifact_digest {
            return Err(Failure::ArtifactDigest { module: position });
        }

        let before = (self.arena_committed)();
        let module = image::parse(&snapshot, limits).map_err(|error| Failure::Parser {
            module: position,
            error,
        })?;
        let decoded_bytes = (self.arena_committed)().saturating_sub(before);

        self.traffic.loads += 1;
        Ok(Resident {
            id,
            image_bytes: snapshot.len(),
            snapshot,
            module,
            decoded_bytes,
            exports: BTreeMap::new(),
            index_bytes: 0,
            imports: Vec::new(),
            import_bytes: 0,
            used_at: self.clock,
        })
    }
}

/// One suspended activation. Identities and values, never a pointer.
pub struct Frame {
    pub module: ClosureModuleId,
    pub function: usize,
    pub block: usize,
    pub instruction: usize,
    values: Vec<Option<i128>>,
    pending_result: Option<usize>,
}

/// Which function a call reaches: an index inside the caller's own module, or
/// an export name to resolve inside the callee.
///
/// The name is **owned**. It is copied out of the caller's module before the
/// callee is made resident, because making the callee resident may evict the
/// caller — and a continuation that held a borrow into an image would not
/// survive that (§6).
enum Callee {
    /// A function of the caller's own module, by index.
    Local(usize),
    /// An export of whatever module the caller's import slot names. Both halves
    /// are resolved outside the step, against resident derived state (§2).
    Import { slot: usize, export: String },
}

/// A call target after both halves are resolved.
enum Resolved {
    Index(usize),
    Export(String),
}

enum Transfer {
    Call {
        from: ClosureModuleId,
        target: Callee,
        arguments: Vec<i128>,
        result: Option<usize>,
    },
    Return(Option<i128>),
}

/// Runs the closure's entry to completion under a bounded resident set.
pub fn run(
    manifest: &crate::closure::VerifiedClosureManifest,
    records: &[VerifiedModuleRecord],
    provider: &dyn Provider,
    limits: &Limits,
    set: &mut ResidentSet,
) -> Result<i128, Failure> {
    let (entry, entry_function) = manifest.entry();
    let mut frames = vec![new_frame(
        entry,
        Resolved::Index(entry_function),
        Vec::new(),
        set,
        provider,
        records,
        limits,
        &[],
    )?];
    let answer: Option<i128>;

    loop {
        let top = frames.len() - 1;
        let id = frames[top].module;
        let suspended: Vec<ClosureModuleId> =
            frames[..top].iter().map(|frame| frame.module).collect();
        set.ensure(id, provider, records, limits, &suspended)?;
        let module = set
            .module_of(id)
            .expect("the module was just made resident");

        let (transfer, ran) = step(module, &mut frames[top])?;
        set.traffic.instructions += ran;
        match transfer {
            Transfer::Call {
                from,
                target,
                arguments,
                result,
            } => {
                // The caller is still resident: nothing has been evicted since
                // it was stepped. Its import mapping is built or read here.
                let (callee, resolved) = match target {
                    Callee::Local(index) => (from, Resolved::Index(index)),
                    Callee::Import { slot, export } => (
                        set.import_of(from, slot, manifest)
                            .ok_or(Failure::Unsupported(
                                "an import slot the closure membership does not name",
                            ))?,
                        Resolved::Export(export),
                    ),
                };
                set.traffic.calls += 1;
                frames[top].pending_result = result;
                // Every frame is suspended now, the caller included: it is
                // waiting inside this call. Loading the callee may evict the
                // module the caller is suspended in, and that is the case §6
                // exists for — so it is passed here to be counted, never to be
                // protected.
                let suspended: Vec<ClosureModuleId> =
                    frames.iter().map(|frame| frame.module).collect();
                let frame = new_frame(
                    callee, resolved, arguments, set, provider, records, limits, &suspended,
                )?;
                frames.push(frame);
            }
            Transfer::Return(value) => {
                set.traffic.returns += 1;
                frames.pop();
                match frames.last_mut() {
                    None => {
                        answer = value;
                        break;
                    }
                    Some(caller) => {
                        if let (Some(slot), Some(value)) = (caller.pending_result.take(), value) {
                            if slot >= caller.values.len() {
                                return Err(Failure::Unsupported("result value out of range"));
                            }
                            caller.values[slot] = Some(value);
                        }
                    }
                }
            }
        }
    }

    answer.ok_or(Failure::Unsupported("the entry returned no value"))
}

#[allow(clippy::too_many_arguments)]
fn new_frame(
    module: ClosureModuleId,
    target: Resolved,
    arguments: Vec<i128>,
    set: &mut ResidentSet,
    provider: &dyn Provider,
    records: &[VerifiedModuleRecord],
    limits: &Limits,
    suspended: &[ClosureModuleId],
) -> Result<Frame, Failure> {
    set.ensure(module, provider, records, limits, suspended)?;
    // The module is already fixed by the manifest; only the function is looked
    // up here, and only inside that module.
    let function = match target {
        Resolved::Index(index) => index,
        Resolved::Export(name) => set.export_of(module, &name).ok_or(Failure::Unsupported(
            "an export the callee does not declare",
        ))?,
    };
    let body = set
        .module_of(module)
        .expect("just ensured")
        .functions
        .get(function)
        .ok_or(Failure::Unsupported(
            "call to a function index not in the table",
        ))?;
    let mut values = vec![None; body.values.len()];
    for (at, value) in arguments.iter().enumerate() {
        if at < values.len() {
            values[at] = Some(*value);
        }
    }
    Ok(Frame {
        module,
        function,
        block: 0,
        instruction: 0,
        values,
        pending_result: None,
    })
}

/// Runs one frame until it transfers control.
fn step(module: &Module, frame: &mut Frame) -> Result<(Transfer, usize), Failure> {
    let mut ran = 0usize;
    let function = module
        .functions
        .get(frame.function)
        .ok_or(Failure::Unsupported("function index not in the table"))?;
    loop {
        let block = function
            .blocks
            .get(frame.block)
            .ok_or(Failure::Unsupported("block index not in the table"))?;

        while frame.instruction < block.instructions.len() {
            let at = frame.instruction;
            let instruction = &block.instructions[at];
            ran += 1;

            if let Op::Call { target, operands } = &instruction.op {
                let mut arguments = Vec::with_capacity(operands.len());
                for operand in operands {
                    arguments.push(read(module, frame, operand)?);
                }
                // Resume past the call, so the frame that comes back does not
                // run it again.
                frame.instruction = at + 1;
                let callee_target = match target {
                    CallTarget::Local(index) => Callee::Local(*index),
                    // Neither half is resolved here: the module comes from the
                    // caller's resident import mapping and the function from
                    // the callee's resident export index, and both need the
                    // resident set (§2, §7).
                    CallTarget::Imported { import, name } => Callee::Import {
                        slot: *import,
                        export: name.clone(),
                    },
                    CallTarget::Predeclared(_) => {
                        return Err(Failure::Unsupported("Op::Call to a predeclared operation"))
                    }
                };
                return Ok((
                    Transfer::Call {
                        from: frame.module,
                        target: callee_target,
                        arguments,
                        result: instruction.result,
                    },
                    ran,
                ));
            }

            let produced = evaluate(module, frame, &instruction.op)?;
            if let (Some(slot), Some(value)) = (instruction.result, produced) {
                if slot >= frame.values.len() {
                    return Err(Failure::Unsupported("result value out of range"));
                }
                frame.values[slot] = Some(value);
            }
            frame.instruction = at + 1;
        }

        match &block.terminator {
            Terminator::Return(None) => return Ok((Transfer::Return(None), ran)),
            Terminator::Return(Some(operand)) => {
                return Ok((Transfer::Return(Some(read(module, frame, operand)?)), ran))
            }
            Terminator::Branch { target, arguments } if arguments.is_empty() => {
                frame.block = *target;
                frame.instruction = 0;
            }
            Terminator::BranchIf {
                condition,
                true_target,
                true_arguments,
                false_target,
                false_arguments,
            } if true_arguments.is_empty() && false_arguments.is_empty() => {
                let taken = read(module, frame, condition)? != 0;
                frame.block = if taken { *true_target } else { *false_target };
                frame.instruction = 0;
            }
            _ => {
                return Err(Failure::Unsupported(
                    "a terminator this harness does not run",
                ))
            }
        }
    }
}

fn read(module: &Module, frame: &Frame, operand: &Operand) -> Result<i128, Failure> {
    match operand {
        Operand::Value(id) => frame
            .values
            .get(*id)
            .copied()
            .flatten()
            .ok_or(Failure::Unsupported("a value read before it was defined")),
        Operand::Constant(id) => match module.constants.get(*id) {
            Some(tos_ir::Constant::Int(_, value)) => Ok(*value),
            Some(tos_ir::Constant::Bool(value)) => Ok(i128::from(*value)),
            Some(tos_ir::Constant::Size(value)) | Some(tos_ir::Constant::Duration(value)) => {
                Ok(*value as i128)
            }
            Some(_) => Err(Failure::Unsupported(
                "a constant this harness does not model",
            )),
            None => Err(Failure::Unsupported("a constant index not in the table")),
        },
    }
}

fn evaluate(module: &Module, frame: &Frame, op: &Op) -> Result<Option<i128>, Failure> {
    Ok(match op {
        Op::Const(id) => Some(read(module, frame, &Operand::Constant(*id))?),
        Op::Binary { op, left, right } => {
            let left = read(module, frame, left)?;
            let right = read(module, frame, right)?;
            Some(binary(*op, left, right)?)
        }
        Op::Unary { op, operand } => {
            let value = read(module, frame, operand)?;
            Some(match op {
                UnaryOp::Negate => value
                    .checked_neg()
                    .ok_or(Failure::Unsupported("negation overflow"))?,
                UnaryOp::Not => i128::from(value == 0),
            })
        }
        Op::Widen { operand, to } => {
            let value = read(module, frame, operand)?;
            Some(widen(value, *to))
        }
        // A place with an empty path is the value itself, which is all this
        // fixture produces. A field or index step would need a value model with
        // aggregates, and inventing one for a residency measurement would be
        // measuring an interpreter rather than a residency scheme.
        Op::Read { place } | Op::Move { place } if place.path.is_empty() => {
            Some(read(module, frame, &Operand::Value(place.root))?)
        }
        other => {
            return Err(Failure::Unsupported(match other {
                Op::Read { .. } | Op::Move { .. } => "a place with a path",
                Op::Aggregate { .. } => "Op::Aggregate",
                Op::Variant { .. } => "Op::Variant",
                Op::Write { .. } => "Op::Write",
                Op::Borrow { .. } => "Op::Borrow",
                Op::Drop { .. } => "Op::Drop",
                _ => "an operation this harness does not run",
            }))
        }
    })
}

fn binary(op: BinaryOp, left: i128, right: i128) -> Result<i128, Failure> {
    let overflow = Failure::Unsupported("checked arithmetic overflow in the fixture");
    Ok(match op {
        BinaryOp::Add => left.checked_add(right).ok_or(overflow)?,
        BinaryOp::Subtract => left.checked_sub(right).ok_or(overflow)?,
        BinaryOp::Multiply => left.checked_mul(right).ok_or(overflow)?,
        BinaryOp::Divide => left.checked_div(right).ok_or(overflow)?,
        BinaryOp::Remainder => left.checked_rem(right).ok_or(overflow)?,
        BinaryOp::BitAnd => left & right,
        BinaryOp::BitOr => left | right,
        BinaryOp::BitXor => left ^ right,
        BinaryOp::Equal => i128::from(left == right),
        BinaryOp::NotEqual => i128::from(left != right),
        BinaryOp::Less => i128::from(left < right),
        BinaryOp::LessOrEqual => i128::from(left <= right),
        BinaryOp::Greater => i128::from(left > right),
        BinaryOp::GreaterOrEqual => i128::from(left >= right),
        BinaryOp::LogicalAnd => i128::from(left != 0 && right != 0),
        BinaryOp::LogicalOr => i128::from(left != 0 || right != 0),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            let places = u32::try_from(right).map_err(|_| Failure::Unsupported("shift count"))?;
            if op == BinaryOp::ShiftLeft {
                left.checked_shl(places).ok_or(overflow)?
            } else {
                left.checked_shr(places).ok_or(overflow)?
            }
        }
    })
}

fn widen(value: i128, to: IntKind) -> i128 {
    let (width, signed) = to.shape();
    if width >= 128 {
        return value;
    }
    if signed {
        let shift = 128 - width;
        (value << shift) >> shift
    } else {
        let mask = (1i128 << width) - 1;
        value & mask
    }
}
