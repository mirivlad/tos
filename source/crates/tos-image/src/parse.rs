// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading a module image.
//!
//! This parser belongs to the verifier path and treats its input as hostile. It
//! is **total**: every path either returns a module or an [`ImageError`], no
//! input reaches a panic, an unbounded allocation or a read past the slice.
//!
//! It validates only what the *container* introduces — frame integrity,
//! canonical form, UTF-8, and references into the string and identity tables
//! the format itself defines. It does **not** check that a `TypeId` names a
//! type or that a `BlockId` names a block. Those are semantic references and
//! the verifier's to check; a parser that quietly pre-checked them would be a
//! second verifier nobody reviewed, and one whose agreement with the first
//! nothing tests.

use super::*;

/// Reads untrusted bytes into a module value the semantic verifier can check.
pub fn parse(image: &[u8], limits: &ParseLimits) -> Result<Module, ImageError> {
    let payload = unframe(image)?;
    let mut input = In {
        bytes: payload,
        at: 0,
        limits: *limits,
        strings: Vec::new(),
    };
    let module = input.module()?;
    if input.at != input.bytes.len() {
        return Err(ImageError::TrailingBytes(input.bytes.len() - input.at));
    }
    Ok(module)
}

struct In<'a> {
    bytes: &'a [u8],
    at: usize,
    limits: ParseLimits,
    strings: Vec<String>,
}

/// A decoded span endpoint, refused when it is not one an index may hold.
fn index_of(value: i128) -> Result<usize, ImageError> {
    if value < 0 || value > usize::MAX as i128 {
        return Err(ImageError::OutOfRange { what: "source map" });
    }
    Ok(value as usize)
}

impl In<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn byte(&mut self, what: &'static str) -> Result<u8, ImageError> {
        let byte = *self.bytes.get(self.at).ok_or(ImageError::Truncated(what))?;
        self.at += 1;
        Ok(byte)
    }

    fn flag(&mut self) -> Result<bool, ImageError> {
        match self.byte("flag")? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(ImageError::UnknownTag {
                family: "flag",
                tag,
            }),
        }
    }

    /// A canonical, bounded varint.
    ///
    /// Non-minimal encodings are refused rather than accepted and normalized:
    /// accepting two spellings of one value is how a canonical form stops being
    /// one.
    fn varint(&mut self) -> Result<u128, ImageError> {
        let mut value: u128 = 0;
        let mut shift: u32 = 0;
        let mut taken = 0usize;
        loop {
            let byte = self.byte("varint")?;
            taken += 1;
            let payload = u128::from(byte & 0x7f);
            if taken == MAX_VARINT_BYTES && payload > 0b11 {
                return Err(ImageError::VarintOverflow);
            }
            value |= payload << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                if taken > 1 && byte == 0 {
                    return Err(ImageError::NonCanonicalVarint);
                }
                return Ok(value);
            }
            if taken == MAX_VARINT_BYTES {
                return Err(ImageError::VarintOverflow);
            }
        }
    }

    /// A table count: bounded by its declared limit **and** by the bytes that
    /// remain, before anything is allocated from it.
    ///
    /// The second bound is what makes a forged count harmless. Every entry of
    /// every table costs at least one byte, so a count larger than the bytes
    /// left cannot be honoured whatever the limit says, and the reader learns
    /// that before it reserves anything.
    fn count(&mut self, what: &'static str, limit: usize) -> Result<usize, ImageError> {
        let count = self.varint()?;
        if count > limit as u128 {
            return Err(ImageError::CountExceedsLimit { what, count, limit });
        }
        let count = count as usize;
        if count > self.remaining() {
            return Err(ImageError::Truncated(what));
        }
        Ok(count)
    }

    /// An index into a semantic table. Bounded to `usize` and nothing more:
    /// whether it names anything is the verifier's question.
    fn index(&mut self) -> Result<usize, ImageError> {
        let value = self.varint()?;
        if value > usize::MAX as u128 {
            return Err(ImageError::IndexOverflow);
        }
        Ok(value as usize)
    }

    fn wide(&mut self) -> Result<u128, ImageError> {
        self.varint()
    }

    fn signed(&mut self) -> Result<i128, ImageError> {
        let zigzag = self.varint()?;
        Ok(((zigzag >> 1) as i128) ^ -((zigzag & 1) as i128))
    }

    fn blob(&mut self, what: &'static str) -> Result<&[u8], ImageError> {
        let length = self.varint()?;
        if length > self.remaining() as u128 {
            return Err(ImageError::Truncated(what));
        }
        let length = length as usize;
        let bytes = &self.bytes[self.at..self.at + length];
        self.at += length;
        Ok(bytes)
    }

    fn strref(&mut self) -> Result<String, ImageError> {
        let at = self.varint()?;
        if at >= self.strings.len() as u128 {
            return Err(ImageError::OutOfRange {
                what: "string table",
            });
        }
        Ok(self.strings[at as usize].clone())
    }

    fn opt_strref(&mut self) -> Result<Option<String>, ImageError> {
        match self.byte("optional string")? {
            0 => Ok(None),
            1 => Ok(Some(self.strref()?)),
            tag => Err(ImageError::UnknownTag {
                family: "optional string",
                tag,
            }),
        }
    }

    fn module(&mut self) -> Result<Module, ImageError> {
        self.string_table()?;
        let header = self.header()?;

        let count = self.count("types", self.limits.table_entries)?;
        let mut types = Vec::with_capacity(count);
        for _ in 0..count {
            types.push(self.type_definition()?);
        }

        let count = self.count("imports", self.limits.modules)?;
        let mut imports = Vec::with_capacity(count);
        for _ in 0..count {
            imports.push(Import {
                module_name: self.strref()?,
                module_content_id: self.strref()?,
                binding: self.strref()?,
            });
        }

        let count = self.count("capability imports", self.limits.table_entries)?;
        let mut capability_imports = Vec::with_capacity(count);
        for _ in 0..count {
            capability_imports.push(CapabilityImport {
                interface: self.strref()?,
                binding: self.strref()?,
                ty: self.index()?,
            });
        }

        let count = self.count("exports", self.limits.table_entries)?;
        let mut exports = Vec::with_capacity(count);
        for _ in 0..count {
            exports.push(self.signature()?);
        }

        let count = self.count("constants", self.limits.table_entries)?;
        let mut constants = Vec::with_capacity(count);
        for _ in 0..count {
            constants.push(self.constant()?);
        }

        let count = self.count("functions", self.limits.table_entries)?;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            functions.push(self.function()?);
        }

        let source_map = self.source_map()?;

        Ok(Module {
            header,
            types,
            imports,
            capability_imports,
            exports,
            constants,
            functions,
            source_map,
        })
    }

    /// The string table, checked for canonical order as it is read.
    fn string_table(&mut self) -> Result<(), ImageError> {
        let count = self.count("string table", MAX_STRINGS)?;
        let mut strings = Vec::with_capacity(count);
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let bytes = self.blob("string")?;
            let text = core::str::from_utf8(bytes).map_err(|_| ImageError::BadUtf8)?;
            if let Some(previous) = &previous {
                if previous.as_str() >= text {
                    return Err(ImageError::NonCanonicalTable("string table"));
                }
            }
            let owned = String::from(text);
            previous = Some(owned.clone());
            strings.push(owned);
        }
        self.strings = strings;
        Ok(())
    }

    fn profile(&mut self) -> Result<Profile, ImageError> {
        profile_of(self.byte("Profile")?)
    }

    fn header(&mut self) -> Result<Header, ImageError> {
        let schema_id = self.strref()?;
        let language_version = self.strref()?;
        let unicode_normalization_baseline = self.strref()?;
        let profile = self.profile()?;
        let module_name = self.strref()?;
        let source_set = self.strref()?;
        let path = self.strref()?;
        let content_id = self.strref()?;
        let dependency_digest = self.strref()?;
        let frontend_identity = self.strref()?;
        let source_map_revision = self.strref()?;
        let resource_envelope = ResourceEnvelope {
            fuel: self.wide()?,
            stack: self.wide()?,
            allocation: self.wide()?,
            tasks: self.wide()?,
            workers: self.wide()?,
            sync: self.wide()?,
            shared: self.wide()?,
            cleanup: self.wide()?,
            recursion: self.wide()?,
            imports: self.wide()?,
        };
        let capability_interface_digest = self.strref()?;
        Ok(Header {
            schema_id,
            language_version,
            unicode_normalization_baseline,
            profile,
            module_name,
            source_set,
            path,
            content_id,
            dependency_digest,
            frontend_identity,
            source_map_revision,
            resource_envelope,
            capability_interface_digest,
        })
    }

    fn type_definition(&mut self) -> Result<TypeDef, ImageError> {
        Ok(match self.byte("TypeDef")? {
            0 => TypeDef::Unit,
            1 => TypeDef::Bool,
            2 => TypeDef::Int(int_kind(self.byte("IntKind")?)?),
            3 => TypeDef::Size,
            4 => TypeDef::Duration,
            5 => TypeDef::Text,
            6 => TypeDef::Bytes,
            7 => TypeDef::ConversionError,
            8 => TypeDef::Event,
            9 => TypeDef::Semaphore,
            10 => TypeDef::Barrier,
            11 => TypeDef::Latch,
            12 => TypeDef::AtomicBool,
            13 => TypeDef::AtomicU32,
            14 => TypeDef::AtomicU64,
            15 => TypeDef::Option(self.index()?),
            16 => TypeDef::Task(self.index()?),
            17 => TypeDef::TaskResult(self.index()?),
            18 => TypeDef::Shared(self.index()?),
            19 => TypeDef::Region(self.index()?),
            20 => TypeDef::DmaRegion(self.index()?),
            21 => TypeDef::Mutex(self.index()?),
            22 => TypeDef::RwLock(self.index()?),
            23 => TypeDef::Channel(self.index()?),
            24 => TypeDef::Slice(self.index()?),
            25 => TypeDef::Result(self.index()?, self.index()?),
            26 => {
                let element = self.index()?;
                let length = self.varint()?;
                if length > u128::from(u64::MAX) {
                    return Err(ImageError::IndexOverflow);
                }
                TypeDef::Array(element, length as u64)
            }
            27 => {
                let count = self.count("tuple elements", self.limits.fields)?;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count {
                    elements.push(self.index()?);
                }
                TypeDef::Tuple(elements)
            }
            28 => {
                let count = self.count("function parameters", self.limits.parameters)?;
                let mut parameters = Vec::with_capacity(count);
                for _ in 0..count {
                    parameters.push(self.index()?);
                }
                TypeDef::Function(parameters, self.index()?)
            }
            29 => TypeDef::Capability(self.strref()?),
            30 => {
                let module_content_id = self.strref()?;
                let export_name = self.strref()?;
                let kind = match self.byte("NominalKind")? {
                    0 => NominalKind::Record,
                    1 => NominalKind::Enum,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "NominalKind",
                            tag,
                        })
                    }
                };
                let count = self.count("fields", self.limits.fields)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(self.index()?);
                }
                let count = self.count("variants", self.limits.fields)?;
                let mut variants = Vec::with_capacity(count);
                for _ in 0..count {
                    let name = self.strref()?;
                    let payload_count = self.count("variant payload", self.limits.fields)?;
                    let mut payload = Vec::with_capacity(payload_count);
                    for _ in 0..payload_count {
                        payload.push(self.index()?);
                    }
                    variants.push(Variant { name, payload });
                }
                TypeDef::Nominal {
                    module_content_id,
                    export_name,
                    kind,
                    fields,
                    variants,
                }
            }
            31 => TypeDef::MutexGuard(self.index()?),
            32 => TypeDef::ReadGuard(self.index()?),
            33 => TypeDef::WriteGuard(self.index()?),
            34 => TypeDef::RegionMut(self.index()?),
            35 => TypeDef::DmaRegionMut(self.index()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "TypeDef",
                    tag,
                })
            }
        })
    }

    fn signature(&mut self) -> Result<Signature, ImageError> {
        let name = self.strref()?;
        let visibility = match self.byte("Visibility")? {
            0 => Visibility::Private,
            1 => Visibility::Public,
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Visibility",
                    tag,
                })
            }
        };
        let is_async = self.flag()?;
        let count = self.count("parameters", self.limits.parameters)?;
        let mut parameters = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.strref()?;
            let ty = self.index()?;
            let mode = match self.byte("PassMode")? {
                0 => PassMode::Owned,
                1 => PassMode::SharedBorrow,
                2 => PassMode::MutableBorrow,
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "PassMode",
                        tag,
                    })
                }
            };
            parameters.push(Parameter { name, ty, mode });
        }
        let result = self.index()?;
        let count = self.count("effects", self.limits.table_entries)?;
        let mut effects = Vec::with_capacity(count);
        for _ in 0..count {
            effects.push(self.strref()?);
        }
        Ok(Signature {
            name,
            visibility,
            is_async,
            parameters,
            result,
            effects,
        })
    }

    fn constant(&mut self) -> Result<Constant, ImageError> {
        Ok(match self.byte("Constant")? {
            0 => Constant::Unit,
            1 => Constant::Bool(self.flag()?),
            2 => {
                let kind = int_kind(self.byte("IntKind")?)?;
                Constant::Int(kind, self.signed()?)
            }
            3 => Constant::Size(self.wide()?),
            4 => Constant::Duration(self.wide()?),
            5 => Constant::Text(self.strref()?),
            6 => Constant::Bytes(self.blob("constant bytes")?.to_vec()),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Constant",
                    tag,
                })
            }
        })
    }

    fn function(&mut self) -> Result<Function, ImageError> {
        let signature = self.signature()?;
        let origin = match self.byte("FunctionOrigin")? {
            0 => FunctionOrigin::Declared,
            1 => FunctionOrigin::LoweredBody,
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "FunctionOrigin",
                    tag,
                })
            }
        };
        let source = self.index()?;
        let stack_contribution = self.wide()?;
        let fuel_contribution = self.wide()?;
        let cleanup_contribution = self.wide()?;
        let count = self.count("ssa values", self.limits.table_entries)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.index()?);
        }
        let count = self.count("blocks", self.limits.blocks_per_function)?;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(self.block()?);
        }
        Ok(Function {
            signature,
            origin,
            source,
            stack_contribution,
            fuel_contribution,
            cleanup_contribution,
            values,
            blocks,
        })
    }

    fn block(&mut self) -> Result<Block, ImageError> {
        let count = self.count("block parameters", self.limits.parameters)?;
        let mut parameters = Vec::with_capacity(count);
        for _ in 0..count {
            parameters.push(self.index()?);
        }
        let count = self.count("instructions", self.limits.instructions_per_block)?;
        let mut instructions = Vec::with_capacity(count);
        for _ in 0..count {
            instructions.push(self.instruction()?);
        }
        let terminator = self.terminator()?;
        let source = self.index()?;
        Ok(Block {
            parameters,
            instructions,
            terminator,
            source,
        })
    }

    fn instruction(&mut self) -> Result<Instruction, ImageError> {
        let result = match self.byte("instruction result")? {
            0 => None,
            1 => Some(self.index()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "instruction result",
                    tag,
                })
            }
        };
        let ty = self.index()?;
        let op = self.op()?;
        let source = self.index()?;
        let unsafe_block = self.flag()?;
        let runtime_contract = self.opt_strref()?;
        let unsafe_interface = self.opt_strref()?;
        Ok(Instruction {
            result,
            ty,
            op,
            source,
            runtime_contract,
            unsafe_block,
            unsafe_interface,
        })
    }

    fn operand(&mut self) -> Result<Operand, ImageError> {
        Ok(match self.byte("Operand")? {
            0 => Operand::Value(self.index()?),
            1 => Operand::Constant(self.index()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Operand",
                    tag,
                })
            }
        })
    }

    fn operands(&mut self) -> Result<Vec<Operand>, ImageError> {
        let count = self.count("operands", MAX_OPERANDS)?;
        let mut operands = Vec::with_capacity(count);
        for _ in 0..count {
            operands.push(self.operand()?);
        }
        Ok(operands)
    }

    fn place(&mut self) -> Result<Place, ImageError> {
        let root = self.index()?;
        let count = self.count("place path", MAX_OPERANDS)?;
        let mut path = Vec::with_capacity(count);
        for _ in 0..count {
            path.push(match self.byte("PlaceStep")? {
                0 => PlaceStep::Field(self.index()?),
                1 => {
                    let value = self.varint()?;
                    if value > u128::from(u64::MAX) {
                        return Err(ImageError::IndexOverflow);
                    }
                    PlaceStep::Index(Some(value as u64))
                }
                2 => PlaceStep::Index(None),
                3 => PlaceStep::DynamicIndex(self.index()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "PlaceStep",
                        tag,
                    })
                }
            });
        }
        Ok(Place { root, path })
    }

    fn op(&mut self) -> Result<Op, ImageError> {
        Ok(match self.byte("Op")? {
            0 => Op::Const(self.index()?),
            1 => Op::Aggregate {
                ty: self.index()?,
                operands: self.operands()?,
            },
            2 => Op::Variant {
                ty: self.index()?,
                index: self.index()?,
                operands: self.operands()?,
            },
            3 => Op::Read {
                place: self.place()?,
            },
            4 => Op::Move {
                place: self.place()?,
            },
            5 => Op::Write {
                place: self.place()?,
                value: self.operand()?,
            },
            6 => Op::Borrow {
                place: self.place()?,
                kind: match self.byte("BorrowKind")? {
                    0 => BorrowKind::Shared,
                    1 => BorrowKind::Mutable,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "BorrowKind",
                            tag,
                        })
                    }
                },
            },
            7 => Op::Drop {
                place: self.place()?,
            },
            8 => Op::Binary {
                op: binary_op(self.byte("BinaryOp")?)?,
                left: self.operand()?,
                right: self.operand()?,
            },
            9 => Op::Unary {
                op: match self.byte("UnaryOp")? {
                    0 => UnaryOp::Negate,
                    1 => UnaryOp::Not,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "UnaryOp",
                            tag,
                        })
                    }
                },
                operand: self.operand()?,
            },
            10 => Op::Widen {
                operand: self.operand()?,
                to: int_kind(self.byte("IntKind")?)?,
            },
            11 => {
                let target = match self.byte("CallTarget")? {
                    0 => CallTarget::Local(self.index()?),
                    1 => CallTarget::Imported {
                        import: self.index()?,
                        name: self.strref()?,
                    },
                    2 => CallTarget::Predeclared(self.strref()?),
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "CallTarget",
                            tag,
                        })
                    }
                };
                Op::Call {
                    target,
                    operands: self.operands()?,
                }
            }
            12 => Op::Spawn {
                body: self.index()?,
                captures: self.operands()?,
            },
            13 => Op::Join {
                task: self.operand()?,
            },
            14 => Op::Await {
                task: self.operand()?,
            },
            15 => Op::Cancel {
                task: self.operand()?,
            },
            16 => {
                let operation = atomic_op(self.byte("AtomicOp")?)?;
                let target = self.operand()?;
                let operands = self.operands()?;
                let order = memory_order(self.byte("MemoryOrder")?)?;
                let failure_order = match self.byte("failure order")? {
                    0 => None,
                    1 => Some(memory_order(self.byte("MemoryOrder")?)?),
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "failure order",
                            tag,
                        })
                    }
                };
                Op::Atomic {
                    operation,
                    target,
                    operands,
                    order,
                    failure_order,
                }
            }
            17 => {
                let import = self.index()?;
                let count = self.count("further imports", self.limits.modules)?;
                let mut further_imports = Vec::with_capacity(count);
                for _ in 0..count {
                    further_imports.push(self.index()?);
                }
                Op::Capability {
                    import,
                    further_imports,
                    right: self.strref()?,
                    operands: self.operands()?,
                }
            }
            18 => Op::Resource {
                kind: resource_kind(self.byte("ResourceKind")?)?,
                amount: self.operand()?,
                release: self.flag()?,
            },
            19 => Op::RegisterCleanup {
                body: self.index()?,
            },
            20 => {
                let count = self.count("cleanup calls", MAX_OPERANDS)?;
                let mut calls = Vec::with_capacity(count);
                for _ in 0..count {
                    calls.push(CleanupCall {
                        body: self.index()?,
                        captures: self.operands()?,
                    });
                }
                Op::RunCleanups { calls }
            }
            21 => Op::Closure {
                body: self.index()?,
                captures: self.operands()?,
            },
            22 => Op::CallValue {
                callee: self.operand()?,
                operands: self.operands()?,
            },
            23 => Op::Share {
                operand: self.operand()?,
            },
            24 => {
                let mode = lock_mode(self.byte("LockMode")?)?;
                Op::Lock {
                    object: self.operand()?,
                    mode,
                }
            }
            tag => return Err(ImageError::UnknownTag { family: "Op", tag }),
        })
    }

    fn terminator(&mut self) -> Result<Terminator, ImageError> {
        Ok(match self.byte("Terminator")? {
            0 => Terminator::Return(match self.byte("return value")? {
                0 => None,
                1 => Some(self.operand()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "return value",
                        tag,
                    })
                }
            }),
            1 => Terminator::Branch {
                target: self.index()?,
                arguments: self.operands()?,
            },
            2 => Terminator::BranchIf {
                condition: self.operand()?,
                true_target: self.index()?,
                true_arguments: self.operands()?,
                false_target: self.index()?,
                false_arguments: self.operands()?,
            },
            3 => {
                let subject = self.operand()?;
                let count = self.count("match arms", MAX_OPERANDS)?;
                let mut arms = Vec::with_capacity(count);
                for _ in 0..count {
                    arms.push((self.index()?, self.index()?));
                }
                Terminator::MatchEnum { subject, arms }
            }
            4 => Terminator::PropagateError {
                result: self.operand()?,
                ok_target: self.index()?,
            },
            5 => Terminator::Trap(self.strref()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Terminator",
                    tag,
                })
            }
        })
    }

    /// The source map: an identity table, then entries that reference it.
    ///
    /// De-interning here restores the full `docs/43` fields on every entry. The
    /// contract's content does not change with this encoding; its repetition
    /// does.
    fn source_map(&mut self) -> Result<Vec<SourceMapEntry>, ImageError> {
        let count = self.count("identity table", self.limits.source_map_entries)?;
        let mut identities = Vec::with_capacity(count);
        let mut previous: Option<[u128; 7]> = None;
        for _ in 0..count {
            let mut references = [0u128; 7];
            for slot in references.iter_mut().take(6) {
                *slot = self.varint()?;
            }
            let profile = self.byte("Profile")?;
            references[6] = u128::from(profile);
            if let Some(previous) = previous {
                if previous >= references {
                    return Err(ImageError::NonCanonicalTable("identity table"));
                }
            }
            previous = Some(references);
            let mut resolved = Vec::with_capacity(6);
            for reference in references.iter().take(6) {
                if *reference >= self.strings.len() as u128 {
                    return Err(ImageError::OutOfRange {
                        what: "string table",
                    });
                }
                resolved.push(*reference as usize);
            }
            let profile = profile_of(profile)?;
            identities.push((resolved, profile));
        }

        let count = self.count("source map", self.limits.source_map_entries)?;
        let mut entries = Vec::with_capacity(count);
        // Spans arrive as steps (encoding version 2): the start as a signed
        // step from the previous entry's start, the end as a signed step from
        // its own start. Both are refused if they leave the range an index may
        // hold, so a malformed map is an error rather than a wrapped number.
        let mut previous: i128 = 0;
        for _ in 0..count {
            let at = self.varint()?;
            if at >= identities.len() as u128 {
                return Err(ImageError::OutOfRange {
                    what: "identity table",
                });
            }
            let (references, profile) = &identities[at as usize];
            let start = previous + crate::write::unzigzag(self.varint()?);
            let end = start + crate::write::unzigzag(self.varint()?);
            let byte_start = index_of(start)?;
            let byte_end = index_of(end)?;
            previous = start;
            let derived_from = match self.byte("derived_from")? {
                0 => None,
                1 => Some(self.index()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "derived_from",
                        tag,
                    })
                }
            };
            entries.push(SourceMapEntry {
                source_set: self.strings[references[0]].clone(),
                path: self.strings[references[1]].clone(),
                content_id: self.strings[references[2]].clone(),
                frontend_identity: self.strings[references[3]].clone(),
                language_version: self.strings[references[4]].clone(),
                profile: *profile,
                unicode_normalization_baseline: self.strings[references[5]].clone(),
                byte_start,
                byte_end,
                derived_from,
            });
        }
        Ok(entries)
    }
}
