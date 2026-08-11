// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded Bootstrap reference interpreter (docs/43 section 7, docs/44
//! section 6 step 7).
//!
//! This is a semantic oracle, not an execution shim. It is the component that
//! says what a TOS Core program *means*, so every future engine — a bytecode
//! engine, a native backend — is measured against it rather than against the
//! host.
//!
//! Two rules shape it.
//!
//! **It executes verified IR only.** [`run`] takes a [`VerifiedModule`] receipt
//! and checks that the receipt names the digest of the module it was handed. A
//! receipt for a different module is not a receipt for this one, and a module
//! without one is not executable. There is no path that runs IR the verifier
//! has not seen.
//!
//! **Correctness never rests on the host.** Nothing here depends on Rust panics
//! or unwinding, host exceptions, an ambient filesystem or network, libc
//! semantics, or host threads. Integer arithmetic is checked against the
//! declared TOS type width rather than a Rust integer's, and every failure of a
//! dynamic precondition is a [`Trap`] with a stable code and the source span it
//! came from.
//!
//! Bootstrap may serialize parallel scopes (docs/43 section 7), and it does:
//! the lowered Bootstrap subset contains no unserialized concurrency, so the
//! result is one the language allows.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::string::{String, ToString};
use std::vec::Vec;

use tos_ir::{
    BinaryOp, CallTarget, Constant, IntKind, Module, Op, Operand, Place, PlaceStep, SourceRef,
    Terminator, UnaryOp,
};
use tos_verifier::VerifiedModule;

/// A runtime value.
///
/// The representation is the language's, not the host's: an integer carries the
/// TOS type that bounds it, so a check is against `i32` because the program said
/// `i32`, never because the host chose a width.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(IntKind, i128),
    Size(u128),
    Duration(u128),
    Text(String),
    Bytes(Vec<u8>),
    /// A record, tuple or array, in declared order.
    Aggregate(Vec<Value>),
    /// An enum, `Option`, `Result` or `TaskResult` value.
    Variant {
        index: usize,
        payload: Vec<Value>,
    },
    /// A closure: the body it runs and the values it captured, in order.
    Closure {
        body: usize,
        captures: Vec<Value>,
    },
    /// A scoped child task.
    ///
    /// Bootstrap serializes parallel scopes (docs/43 section 7). This engine
    /// serializes by deferring the child to its join rather than running it at
    /// the spawn, which keeps `cancel` meaningful: a child cancelled before it
    /// is joined never starts, and `Cancelled` is an outcome docs/41 section 2
    /// allows.
    Task {
        body: usize,
        captures: Vec<Value>,
        cancelled: bool,
    },
}

/// A defined failure of a dynamic language precondition (docs/41 section 7).
///
/// A trap is not an error value and cannot be caught as `Result`; it ends the
/// process. It carries a stable code and the source span the operation came
/// from, so a runtime failure names the text that caused it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trap {
    pub code: &'static str,
    pub detail: String,
    /// The source-map entry index of the operation that trapped.
    pub source: SourceRef,
}

impl Trap {
    fn new(code: &'static str, detail: impl ToString, source: SourceRef) -> Trap {
        Trap {
            code,
            detail: detail.to_string(),
            source,
        }
    }
}

/// What one run consumed and produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub value: Value,
    pub fuel_used: u128,
    pub max_call_depth: u128,
    pub tasks_started: u128,
}

/// Why a module could not be run at all, before any instruction executed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The receipt names a different module than the one supplied.
    ReceiptDoesNotMatch,
    /// The named entry function is not exported by this module.
    NoSuchEntry(String),
    /// The entry function's arity does not match the arguments supplied.
    EntryArity { expected: usize, actual: usize },
}

/// Runs an entry function of a verified module.
///
/// The receipt is checked against the module's own digest first: an engine
/// accepts executable IR only with a receipt for that exact module (docs/43
/// section 5).
pub fn run(
    module: &Module,
    receipt: &VerifiedModule,
    entry: &str,
    arguments: Vec<Value>,
) -> Result<Result<Outcome, Trap>, Refusal> {
    if receipt.module_digest != tos_ir::module_digest(module) {
        return Err(Refusal::ReceiptDoesNotMatch);
    }
    let Some(index) = module
        .functions
        .iter()
        .position(|function| function.signature.name == entry)
    else {
        return Err(Refusal::NoSuchEntry(entry.to_string()));
    };
    let expected = module.functions[index].signature.parameters.len();
    if expected != arguments.len() {
        return Err(Refusal::EntryArity {
            expected,
            actual: arguments.len(),
        });
    }

    let envelope = &module.header.resource_envelope;
    let mut engine = Engine {
        module,
        fuel_limit: envelope.fuel,
        recursion_limit: envelope.recursion.max(1),
        fuel_used: 0,
        depth: 0,
        max_depth: 0,
        task_limit: envelope.tasks,
        tasks_started: 0,
    };
    Ok(engine.call(index, arguments).map(|value| Outcome {
        value,
        fuel_used: engine.fuel_used,
        max_call_depth: engine.max_depth,
        tasks_started: engine.tasks_started,
    }))
}

struct Engine<'module> {
    module: &'module Module,
    fuel_limit: u128,
    recursion_limit: u128,
    fuel_used: u128,
    depth: u128,
    max_depth: u128,
    task_limit: u128,
    tasks_started: u128,
}

/// How a block finished.
enum Exit {
    Return(Value),
    Goto(usize),
}

impl Engine<'_> {
    /// Charges one unit of fuel, trapping when the declared budget is gone.
    ///
    /// docs/41 section 6 makes fuel the accounting that bounds work. The budget
    /// is the module's declared limit, so exhaustion is deterministic: the same
    /// program on the same input runs out at the same operation.
    fn spend(&mut self, source: SourceRef) -> Result<(), Trap> {
        self.fuel_used += 1;
        if self.fuel_used > self.fuel_limit {
            return Err(Trap::new(
                "RUNTIME_FUEL_EXHAUSTED",
                std::format!("the declared budget of {} is spent", self.fuel_limit),
                source,
            ));
        }
        Ok(())
    }

    /// Calls a function and writes back what it changed through a borrow.
    ///
    /// A `borrow mut` parameter names the caller's place, and the borrow rules
    /// guarantee no other alias is live for the duration of the call, so
    /// copying in and copying out is observationally the same as a reference
    /// and needs no aliasing machinery to be correct.
    fn call_with_writeback(
        &mut self,
        index: usize,
        arguments: Vec<Value>,
        operands: &[Operand],
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        let modes: Vec<tos_ir::PassMode> = self.module.functions[index]
            .signature
            .parameters
            .iter()
            .map(|parameter| parameter.mode)
            .collect();
        let (result, finals) = self.call_capturing(index, arguments)?;
        for (position, mode) in modes.iter().enumerate() {
            if *mode != tos_ir::PassMode::MutableBorrow {
                continue;
            }
            let (Some(Operand::Value(slot)), Some(Some(value))) =
                (operands.get(position), finals.get(position))
            else {
                continue;
            };
            if let Some(target) = values.get_mut(*slot) {
                *target = Some(value.clone());
            }
        }
        let _ = source;
        Ok(result)
    }

    fn call(&mut self, index: usize, arguments: Vec<Value>) -> Result<Value, Trap> {
        Ok(self.call_capturing(index, arguments)?.0)
    }

    /// Calls a function and returns its result with the final state of its
    /// parameter slots, which is what a borrow writes back.
    fn call_capturing(
        &mut self,
        index: usize,
        arguments: Vec<Value>,
    ) -> Result<(Value, Vec<Option<Value>>), Trap> {
        let function = &self.module.functions[index];
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        if self.depth > self.recursion_limit {
            let source = function.source;
            self.depth -= 1;
            return Err(Trap::new(
                "RUNTIME_RECURSION_LIMIT",
                std::format!("the declared depth of {} is exceeded", self.recursion_limit),
                source,
            ));
        }

        let mut values: Vec<Option<Value>> = std::vec![None; function.values.len()];
        for (slot, argument) in arguments.into_iter().enumerate() {
            if slot < values.len() {
                values[slot] = Some(argument);
            }
        }

        let mut block = 0usize;
        let mut steps = 0u128;
        let outcome = loop {
            steps += 1;
            if steps > self.fuel_limit.saturating_add(1) {
                break Err(Trap::new(
                    "RUNTIME_FUEL_EXHAUSTED",
                    "control did not leave the function within its budget",
                    function.blocks[block].source,
                ));
            }
            match self.run_block(index, block, &mut values) {
                Ok(Exit::Return(value)) => break Ok(value),
                Ok(Exit::Goto(next)) => block = next,
                Err(trap) => break Err(trap),
            }
        };
        self.depth -= 1;
        outcome.map(|value| (value, values))
    }

    fn run_block(
        &mut self,
        function_index: usize,
        block_index: usize,
        values: &mut [Option<Value>],
    ) -> Result<Exit, Trap> {
        // Instructions and the terminator are read from the module each time
        // rather than held across a call, so a nested call cannot observe a
        // stale borrow of the table it is also reading.
        let count = self.module.functions[function_index].blocks[block_index]
            .instructions
            .len();
        for position in 0..count {
            let instruction = self.module.functions[function_index].blocks[block_index]
                .instructions[position]
                .clone();
            self.spend(instruction.source)?;
            let produced = self.evaluate(&instruction.op, values, instruction.source)?;
            if let (Some(slot), Some(value)) = (instruction.result, produced) {
                if slot < values.len() {
                    values[slot] = Some(value);
                }
            }
        }
        let terminator = self.module.functions[function_index].blocks[block_index]
            .terminator
            .clone();
        let source = self.module.functions[function_index].blocks[block_index].source;
        self.terminate(&terminator, values, source)
    }

    fn terminate(
        &mut self,
        terminator: &Terminator,
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<Exit, Trap> {
        match terminator {
            Terminator::Return(operand) => {
                // Every executed operation costs, terminators included: fuel is
                // the accounting for work done, and leaving a scope is work.
                self.spend(source)?;
                let value = match operand {
                    Some(operand) => self.operand(operand, values, source)?,
                    None => Value::Unit,
                };
                Ok(Exit::Return(value))
            }
            Terminator::Branch { target, .. } => {
                // A back edge is where a loop consumes fuel (docs/41 section 6).
                self.spend(source)?;
                Ok(Exit::Goto(*target))
            }
            Terminator::BranchIf {
                condition,
                true_target,
                false_target,
                ..
            } => {
                self.spend(source)?;
                let Value::Bool(taken) = self.operand(condition, values, source)? else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a branch condition is not a bool",
                        source,
                    ));
                };
                Ok(Exit::Goto(if taken { *true_target } else { *false_target }))
            }
            Terminator::MatchEnum { subject, arms } => {
                self.spend(source)?;
                let value = self.operand(subject, values, source)?;
                let Value::Variant { index, .. } = value else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a match subject is not a variant",
                        source,
                    ));
                };
                let Some((_, target)) = arms.iter().find(|(variant, _)| *variant == index) else {
                    // The verifier proved the map complete, so this is a
                    // representation defect rather than a program outcome.
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        std::format!("no arm covers variant {index}"),
                        source,
                    ));
                };
                Ok(Exit::Goto(*target))
            }
            Terminator::PropagateError { result, ok_target } => {
                self.spend(source)?;
                let value = self.operand(result, values, source)?;
                let Value::Variant { index, payload } = &value else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "`?` applied to a value that is not a Result",
                        source,
                    ));
                };
                // docs/40 section 4: `?` propagates the matching `Err` from the
                // nearest enclosing return scope, which is this function.
                if *index == 1 {
                    return Ok(Exit::Return(Value::Variant {
                        index: 1,
                        payload: payload.clone(),
                    }));
                }
                Ok(Exit::Goto(*ok_target))
            }
            Terminator::Trap(code) => Err(Trap::new(
                "RUNTIME_TRAP",
                std::format!("the program reached {code}"),
                source,
            )),
        }
    }

    fn evaluate(
        &mut self,
        op: &Op,
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<Option<Value>, Trap> {
        let produced = match op {
            Op::Const(constant) => Some(self.constant(*constant, source)?),
            Op::Aggregate { operands, .. } => {
                let mut elements = Vec::new();
                for operand in operands {
                    elements.push(self.operand(operand, values, source)?);
                }
                Some(Value::Aggregate(elements))
            }
            Op::Variant {
                index, operands, ..
            } => {
                let mut payload = Vec::new();
                for operand in operands {
                    payload.push(self.operand(operand, values, source)?);
                }
                Some(Value::Variant {
                    index: *index,
                    payload,
                })
            }
            // A read copies the value at a place; a move takes it. Both observe
            // the same location, and the verifier already proved that a moved
            // place is not read again on the same path.
            Op::Read { place } | Op::Move { place } | Op::Borrow { place, .. } => {
                Some(self.read_place(place, values, source)?)
            }
            Op::Write { place, value } => {
                let value = self.operand(value, values, source)?;
                self.write_place(place, value, values, source)?;
                None
            }
            Op::Drop { .. } => None,
            Op::Binary { op, left, right } => {
                let left = self.operand(left, values, source)?;
                let right = self.operand(right, values, source)?;
                Some(binary(*op, left, right, source)?)
            }
            Op::Unary { op, operand } => {
                let operand = self.operand(operand, values, source)?;
                Some(unary(*op, operand, source)?)
            }
            Op::Widen { operand, to } => {
                let operand = self.operand(operand, values, source)?;
                let Value::Int(_, magnitude) = operand else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a widening conversion applied to a non-integer",
                        source,
                    ));
                };
                Some(Value::Int(*to, magnitude))
            }
            Op::Call { target, operands } => {
                let mut arguments = Vec::new();
                for operand in operands {
                    arguments.push(self.operand(operand, values, source)?);
                }
                match target {
                    CallTarget::Local(index) => {
                        Some(self.call_with_writeback(*index, arguments, operands, values, source)?)
                    }
                    CallTarget::Imported { .. } => {
                        return Err(Trap::new(
                            "RUNTIME_UNRESOLVED_IMPORT",
                            "a cross-module call needs the imported module's IR",
                            source,
                        ))
                    }
                    CallTarget::Predeclared(name) => {
                        Some(self.predeclared(name, arguments, source)?)
                    }
                }
            }
            Op::Closure { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(capture, values, source)?);
                }
                Some(Value::Closure {
                    body: *body,
                    captures: held,
                })
            }
            Op::CallValue { callee, operands } => {
                let callee = self.operand(callee, values, source)?;
                let Value::Closure { body, captures } = callee else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a value call applied to something that is not a closure",
                        source,
                    ));
                };
                // The body declares its own parameters first, then its
                // captures, which is the order the lowerer built it in.
                let mut arguments = Vec::new();
                for operand in operands {
                    arguments.push(self.operand(operand, values, source)?);
                }
                arguments.extend(captures);
                Some(self.call(body, arguments)?)
            }
            Op::Spawn { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(capture, values, source)?);
                }
                self.tasks_started += 1;
                if self.tasks_started > self.task_limit {
                    return Err(Trap::new(
                        "RUNTIME_TASK_LIMIT",
                        std::format!("the declared task budget of {} is spent", self.task_limit),
                        source,
                    ));
                }
                Some(Value::Task {
                    body: *body,
                    captures: held,
                    cancelled: false,
                })
            }
            Op::Join { task } | Op::Await { task } => {
                let handle = self.operand(task, values, source)?;
                let Value::Task {
                    body,
                    captures,
                    cancelled,
                } = handle
                else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a join applied to something that is not a task",
                        source,
                    ));
                };
                // `Cancelled` and `Completed` are the two outcomes docs/41
                // section 2 defines, and joining consumes the handle either way.
                Some(if cancelled {
                    Value::Variant {
                        index: 1,
                        payload: Vec::new(),
                    }
                } else {
                    Value::Variant {
                        index: 0,
                        payload: std::vec![self.call(body, captures)?],
                    }
                })
            }
            Op::Cancel { task } => {
                // docs/41 section 2: an idempotent cooperative request that
                // consumes no ownership. The parent still has to join.
                if let Operand::Value(slot) = task {
                    if let Some(Some(Value::Task { cancelled, .. })) = values.get_mut(*slot) {
                        *cancelled = true;
                    }
                }
                None
            }
            Op::RegisterCleanup { .. } => {
                // ADR-0035: registering reads, borrows and moves nothing. It is
                // what the `cleanup` limit counts, and the count is bounded by
                // the verifier before execution begins.
                None
            }
            Op::RunCleanups { calls } => {
                for call in calls {
                    let mut arguments = Vec::new();
                    for capture in &call.captures {
                        arguments.push(self.operand(capture, values, source)?);
                    }
                    // ADR-0035: a cleanup acts on the scope it runs in, so what
                    // it leaves is what the next one and the scope observe.
                    self.call_with_writeback(call.body, arguments, &call.captures, values, source)?;
                }
                None
            }
            Op::Atomic { .. } | Op::Capability { .. } | Op::Resource { .. } => {
                return Err(Trap::new(
                    "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                    "this reference engine does not yet execute that operation family",
                    source,
                ))
            }
        };
        Ok(produced)
    }

    fn constant(&self, index: usize, source: SourceRef) -> Result<Value, Trap> {
        let Some(constant) = self.module.constants.get(index) else {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a constant index is outside the table",
                source,
            ));
        };
        Ok(match constant {
            Constant::Unit => Value::Unit,
            Constant::Bool(value) => Value::Bool(*value),
            Constant::Int(kind, value) => Value::Int(*kind, *value),
            Constant::Size(value) => Value::Size(*value),
            Constant::Duration(value) => Value::Duration(*value),
            Constant::Text(value) => Value::Text(value.clone()),
            Constant::Bytes(value) => Value::Bytes(value.clone()),
        })
    }

    fn operand(
        &self,
        operand: &Operand,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        match operand {
            Operand::Value(index) => match values.get(*index).and_then(|slot| slot.clone()) {
                Some(value) => Ok(value),
                None => Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    std::format!("value {index} is read before it is defined"),
                    source,
                )),
            },
            Operand::Constant(index) => self.constant(*index, source),
        }
    }

    fn read_place(
        &self,
        place: &Place,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        let mut current = match values.get(place.root).and_then(|slot| slot.clone()) {
            Some(value) => value,
            None => {
                return Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    std::format!("value {} is read before it is defined", place.root),
                    source,
                ))
            }
        };
        for step in &place.path {
            let step = self.resolve_step(step, values, source)?;
            current = step_into(current, &step, source)?;
        }
        Ok(current)
    }

    /// Replaces a computed index with the constant it evaluates to.
    ///
    /// A dynamic index names a value of type `size`; reading it here is what
    /// turns a place the checker analysed conservatively into the one location
    /// the program actually touches.
    fn resolve_step(
        &self,
        step: &PlaceStep,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<PlaceStep, Trap> {
        let PlaceStep::DynamicIndex(value) = step else {
            return Ok(step.clone());
        };
        let index = match values.get(*value).and_then(|slot| slot.clone()) {
            Some(Value::Size(index)) => index,
            Some(Value::Int(_, index)) if index >= 0 => index as u128,
            Some(_) => {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an index is not a size",
                    source,
                ))
            }
            None => {
                return Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    std::format!("index value {value} is read before it is defined"),
                    source,
                ))
            }
        };
        let Ok(index) = u64::try_from(index) else {
            return Err(Trap::new(
                "RUNTIME_INDEX_OUT_OF_RANGE",
                "an index does not fit an element position",
                source,
            ));
        };
        Ok(PlaceStep::Index(Some(index)))
    }

    fn write_place(
        &self,
        place: &Place,
        value: Value,
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<(), Trap> {
        let path: Vec<PlaceStep> = place
            .path
            .iter()
            .map(|step| self.resolve_step(step, values, source))
            .collect::<Result<_, _>>()?;
        if path.is_empty() {
            if let Some(slot) = values.get_mut(place.root) {
                *slot = Some(value);
            }
            return Ok(());
        }
        let Some(Some(root)) = values.get_mut(place.root) else {
            return Err(Trap::new(
                "RUNTIME_UNINITIALIZED_VALUE",
                std::format!("value {} is written before it is defined", place.root),
                source,
            ));
        };
        write_into(root, &path, value, source)
    }

    /// The predeclared V1 operations (docs/39 section 2).
    fn predeclared(
        &self,
        name: &str,
        arguments: Vec<Value>,
        source: SourceRef,
    ) -> Result<Value, Trap> {
        if let Some(target) = name.strip_prefix("to_") {
            let Some(kind) = IntKind::parse(target) else {
                return Err(Trap::new(
                    "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                    std::format!("unknown checked conversion {name}"),
                    source,
                ));
            };
            let Some(Value::Int(_, magnitude)) = arguments.first().cloned() else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "a checked conversion applied to a non-integer",
                    source,
                ));
            };
            // docs/40 section 3: a checked narrowing is `Result<T,
            // ConversionError>`, never a silent truncation.
            return Ok(match fits(kind, magnitude) {
                true => Value::Variant {
                    index: 0,
                    payload: std::vec![Value::Int(kind, magnitude)],
                },
                false => Value::Variant {
                    index: 1,
                    payload: std::vec![Value::Unit],
                },
            });
        }
        let wrapping = match name {
            "wrapping_add" => Some(BinaryOp::Add),
            "wrapping_sub" => Some(BinaryOp::Subtract),
            "wrapping_mul" => Some(BinaryOp::Multiply),
            _ => None,
        };
        if let Some(op) = wrapping {
            let (Some(Value::Int(kind, left)), Some(Value::Int(_, right))) =
                (arguments.first().cloned(), arguments.get(1).cloned())
            else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "wrapping arithmetic applied to a non-integer",
                    source,
                ));
            };
            let raw = match op {
                BinaryOp::Add => left.wrapping_add(right),
                BinaryOp::Subtract => left.wrapping_sub(right),
                _ => left.wrapping_mul(right),
            };
            return Ok(Value::Int(kind, wrap(kind, raw)));
        }
        Err(Trap::new(
            "RUNTIME_OPERATION_NOT_IMPLEMENTED",
            std::format!("{name} is not a predeclared V1 operation this engine runs"),
            source,
        ))
    }
}

fn step_into(value: Value, step: &PlaceStep, source: SourceRef) -> Result<Value, Trap> {
    match (value, step) {
        (Value::Aggregate(elements), PlaceStep::Field(index)) => elements
            .get(*index)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_TYPE_CONFUSION", "field out of range", source)),
        (Value::Variant { payload, .. }, PlaceStep::Field(index)) => payload
            .get(*index)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_TYPE_CONFUSION", "payload out of range", source)),
        (Value::Aggregate(elements), PlaceStep::Index(Some(index))) => elements
            .get(*index as usize)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_INDEX_OUT_OF_RANGE", "index out of range", source)),
        (Value::Bytes(bytes), PlaceStep::Index(Some(index))) => bytes
            .get(*index as usize)
            .map(|byte| Value::Int(IntKind::U8, *byte as i128))
            .ok_or_else(|| Trap::new("RUNTIME_INDEX_OUT_OF_RANGE", "index out of range", source)),
        (_, PlaceStep::Index(None) | PlaceStep::DynamicIndex(_)) => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "an index step reached execution without a value",
            source,
        )),
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "a place step does not apply to this value",
            source,
        )),
    }
}

fn write_into(
    target: &mut Value,
    path: &[PlaceStep],
    value: Value,
    source: SourceRef,
) -> Result<(), Trap> {
    let Some((step, rest)) = path.split_first() else {
        *target = value;
        return Ok(());
    };
    let slot = match (target, step) {
        (Value::Aggregate(elements), PlaceStep::Field(index)) => elements.get_mut(*index),
        (Value::Variant { payload, .. }, PlaceStep::Field(index)) => payload.get_mut(*index),
        (Value::Aggregate(elements), PlaceStep::Index(Some(index))) => {
            elements.get_mut(*index as usize)
        }
        _ => {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a place step does not apply to this value",
                source,
            ))
        }
    };
    let Some(slot) = slot else {
        return Err(Trap::new(
            "RUNTIME_INDEX_OUT_OF_RANGE",
            "a written place is out of range",
            source,
        ));
    };
    write_into(slot, rest, value, source)
}

/// Whether a magnitude is representable in a TOS integer type.
fn fits(kind: IntKind, magnitude: i128) -> bool {
    let (width, signed) = kind.shape();
    if signed {
        let bound = 1i128 << (width - 1);
        magnitude >= -bound && magnitude < bound
    } else {
        magnitude >= 0 && magnitude < (1i128 << width)
    }
}

/// Reduces a magnitude into a TOS integer type, for explicit wrapping contracts.
fn wrap(kind: IntKind, magnitude: i128) -> i128 {
    let (width, signed) = kind.shape();
    let modulus = 1i128 << width;
    let reduced = magnitude.rem_euclid(modulus);
    if signed && reduced >= modulus / 2 {
        reduced - modulus
    } else {
        reduced
    }
}

fn binary(op: BinaryOp, left: Value, right: Value, source: SourceRef) -> Result<Value, Trap> {
    if op.is_comparison() {
        return compare(op, left, right, source);
    }
    if let (Value::Size(left), Value::Size(right)) = (&left, &right) {
        return size_arithmetic(op, *left, *right, source);
    }
    let (Value::Int(kind, left), Value::Int(_, right)) = (&left, &right) else {
        return Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "arithmetic applied to a non-integer",
            source,
        ));
    };
    let (kind, left, right) = (*kind, *left, *right);
    let (width, _) = kind.shape();
    let raw = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Subtract => left.checked_sub(right),
        BinaryOp::Multiply => left.checked_mul(right),
        BinaryOp::Divide | BinaryOp::Remainder => {
            if right == 0 {
                return Err(Trap::new(
                    "RUNTIME_DIVISION_BY_ZERO",
                    "division or remainder by zero",
                    source,
                ));
            }
            if op == BinaryOp::Divide {
                left.checked_div(right)
            } else {
                left.checked_rem(right)
            }
        }
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            // docs/40 section 3: a shift count is nonnegative and strictly
            // smaller than the width.
            if right < 0 || right >= i128::from(width) {
                return Err(Trap::new(
                    "RUNTIME_INVALID_SHIFT",
                    std::format!("shift count {right} is not below the width {width}"),
                    source,
                ));
            }
            if op == BinaryOp::ShiftLeft {
                left.checked_shl(right as u32)
            } else {
                left.checked_shr(right as u32)
            }
        }
        BinaryOp::BitAnd => Some(left & right),
        BinaryOp::BitOr => Some(left | right),
        BinaryOp::BitXor => Some(left ^ right),
        _ => None,
    };
    let Some(raw) = raw else {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked operation left the representable range",
            source,
        ));
    };
    // The host's width is not the program's: the result must fit the type the
    // source declared, or the operation overflowed whatever the host could do.
    if !fits(kind, raw) {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            std::format!("{raw} does not fit {}", kind.spelled()),
            source,
        ));
    }
    Ok(Value::Int(kind, raw))
}

/// The reference ABI width `size` arithmetic is checked in.
///
/// docs/40 section 3 says `size` arithmetic is checked in the target ABI and
/// that portable source must not assume its width. The reference engine has to
/// pick one to be a semantic oracle at all, and it says so here rather than
/// inheriting whatever the host happens to use.
const SIZE_WIDTH_BITS: u32 = 64;

fn size_arithmetic(
    op: BinaryOp,
    left: u128,
    right: u128,
    source: SourceRef,
) -> Result<Value, Trap> {
    let raw = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Subtract => left.checked_sub(right),
        BinaryOp::Multiply => left.checked_mul(right),
        BinaryOp::Divide | BinaryOp::Remainder => {
            if right == 0 {
                return Err(Trap::new(
                    "RUNTIME_DIVISION_BY_ZERO",
                    "division or remainder by zero",
                    source,
                ));
            }
            if op == BinaryOp::Divide {
                left.checked_div(right)
            } else {
                left.checked_rem(right)
            }
        }
        BinaryOp::BitAnd => Some(left & right),
        BinaryOp::BitOr => Some(left | right),
        BinaryOp::BitXor => Some(left ^ right),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            if right >= u128::from(SIZE_WIDTH_BITS) {
                return Err(Trap::new(
                    "RUNTIME_INVALID_SHIFT",
                    "shift count is not below the size width",
                    source,
                ));
            }
            if op == BinaryOp::ShiftLeft {
                left.checked_shl(right as u32)
            } else {
                left.checked_shr(right as u32)
            }
        }
        _ => None,
    };
    let Some(raw) = raw else {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked size operation left the representable range",
            source,
        ));
    };
    if raw >= (1u128 << SIZE_WIDTH_BITS) {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked size operation left the target ABI width",
            source,
        ));
    }
    Ok(Value::Size(raw))
}

fn compare(op: BinaryOp, left: Value, right: Value, source: SourceRef) -> Result<Value, Trap> {
    if let (Value::Bool(left), Value::Bool(right)) = (&left, &right) {
        return Ok(Value::Bool(match op {
            BinaryOp::LogicalAnd => *left && *right,
            BinaryOp::LogicalOr => *left || *right,
            BinaryOp::Equal => left == right,
            BinaryOp::NotEqual => left != right,
            _ => {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an ordering comparison applied to bool",
                    source,
                ))
            }
        }));
    }
    let ordering = match (&left, &right) {
        (Value::Int(_, left), Value::Int(_, right)) => left.cmp(right),
        (Value::Size(left), Value::Size(right)) => left.cmp(right),
        (Value::Duration(left), Value::Duration(right)) => left.cmp(right),
        (Value::Text(left), Value::Text(right)) => left.cmp(right),
        (Value::Bytes(left), Value::Bytes(right)) => left.cmp(right),
        _ => {
            return Ok(Value::Bool(match op {
                BinaryOp::Equal => left == right,
                BinaryOp::NotEqual => left != right,
                _ => {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "an ordering comparison applied to values that are not ordered",
                        source,
                    ))
                }
            }))
        }
    };
    Ok(Value::Bool(match op {
        BinaryOp::Equal => ordering.is_eq(),
        BinaryOp::NotEqual => ordering.is_ne(),
        BinaryOp::Less => ordering.is_lt(),
        BinaryOp::LessOrEqual => ordering.is_le(),
        BinaryOp::Greater => ordering.is_gt(),
        BinaryOp::GreaterOrEqual => ordering.is_ge(),
        _ => {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a logical operator applied to non-bool operands",
                source,
            ))
        }
    }))
}

fn unary(op: UnaryOp, operand: Value, source: SourceRef) -> Result<Value, Trap> {
    match (op, operand) {
        (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
        (UnaryOp::Negate, Value::Int(kind, magnitude)) => {
            let (_, signed) = kind.shape();
            if !signed {
                // docs/40 section 3 rejects `-x` for `uN` statically; reaching
                // here means the IR claims something the language forbids.
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "negation of an unsigned value",
                    source,
                ));
            }
            let Some(negated) = magnitude.checked_neg() else {
                return Err(Trap::new(
                    "RUNTIME_ARITHMETIC_OVERFLOW",
                    "negation left the representable range",
                    source,
                ));
            };
            if !fits(kind, negated) {
                return Err(Trap::new(
                    "RUNTIME_ARITHMETIC_OVERFLOW",
                    "negation left the representable range",
                    source,
                ));
            }
            Ok(Value::Int(kind, negated))
        }
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "a unary operator does not apply to this value",
            source,
        )),
    }
}

/// The source-map entry a trap points at, for a runtime diagnostic.
///
/// docs/43 section 6: a runtime failure names the exact source it came from.
pub fn trap_source<'module>(
    module: &'module Module,
    trap: &Trap,
) -> Option<&'module tos_ir::SourceMapEntry> {
    module.source_map.get(trap.source)
}

/// A record of what a run consumed, for the resource accounting docs/41
/// section 6 requires an engine to keep.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Accounting {
    pub fuel_used: u128,
    pub fuel_limit: u128,
    pub max_call_depth: u128,
    pub recursion_limit: u128,
    pub tasks_started: u128,
    pub task_limit: u128,
}

impl Accounting {
    pub fn of(module: &Module, outcome: &Outcome) -> Accounting {
        Accounting {
            fuel_used: outcome.fuel_used,
            fuel_limit: module.header.resource_envelope.fuel,
            max_call_depth: outcome.max_call_depth,
            recursion_limit: module.header.resource_envelope.recursion,
            tasks_started: outcome.tasks_started,
            task_limit: module.header.resource_envelope.tasks,
        }
    }
}

/// The entry points a module exports, for a driver that has to choose one.
pub fn exported_entries(module: &Module) -> BTreeMap<String, usize> {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.signature.visibility == tos_ir::Visibility::Public)
        .map(|(index, function)| (function.signature.name.clone(), index))
        .collect()
}
