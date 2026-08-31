// SPDX-License-Identifier: GPL-3.0-or-later
//! Writing a module image.
//!
//! One traversal, in the canonical section order of docs/43 §2, into a packed
//! byte payload that [`crate::frame`] then seals. Every variable-length
//! position is length-prefixed and every string is a reference into a sorted
//! table, so the same module always produces the same bytes.

use super::*;

/// Where the bytes of one image went, section by section.
///
/// Reported rather than summed, because "the image is smaller" and "the source
/// map stopped repeating itself" are different claims and a total cannot tell
/// them apart.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Layout {
    pub strings: usize,
    pub header: usize,
    pub types: usize,
    pub imports: usize,
    pub capability_imports: usize,
    pub exports: usize,
    pub constants: usize,
    pub functions: usize,
    pub source_map_identities: usize,
    pub source_map_entries: usize,
    /// What the source map would have cost with each entry's seven identity
    /// fields written inline, in this same encoding.
    pub source_map_inline_equivalent: usize,
    /// What it would cost with each entry's span written as a delta from the
    /// previous entry's start, and its end as a length.
    pub source_map_delta_equivalent: usize,
    /// What it would cost with distinct spans in a table and each entry naming
    /// one, the way identities already work.
    pub source_map_shared_span_equivalent: usize,
    /// Distinct `(start, end)` spans in the module.
    pub span_count: usize,
    /// Instructions in the module, and what their fixed per-instruction fields
    /// cost in the encoding as written.
    pub instruction_count: usize,
    /// Bytes spent on each instruction's source-map reference.
    pub instruction_source_refs: usize,
    /// Bytes spent on the result tag, the unsafe flag and the two optional
    /// string references — three of which are a byte each and empty in every
    /// instruction a checked module usually has.
    pub instruction_tags: usize,
    /// What those two would cost with the source reference written as a step
    /// from the previous instruction's and the four tags packed into one byte.
    pub instruction_packed_equivalent: usize,
    /// Distinct source-map identities in the module.
    pub identity_count: usize,
    pub string_count: usize,
    pub payload: usize,
    pub image: usize,
}

/// Encodes one module.
///
/// Total: every `tos-ir/v1` value has an encoding, so this cannot refuse a
/// well-formed module. It is infallible by construction and says so in its
/// type.
pub fn encode(module: &Module) -> (Vec<u8>, Layout) {
    let table = collect_strings(module);
    let index: BTreeMap<&str, u32> = table
        .iter()
        .enumerate()
        .map(|(at, text)| (text.as_str(), at as u32))
        .collect();

    let mut out = Out {
        bytes: Vec::new(),
        index,
    };
    let mut layout = Layout {
        string_count: table.len(),
        ..Layout::default()
    };

    out.count(table.len());
    for text in &table {
        out.blob(text.as_bytes());
    }
    layout.strings = out.bytes.len();

    let mark = out.bytes.len();
    write_header(&mut out, &module.header);
    layout.header = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.types.len());
    for definition in &module.types {
        write_type(&mut out, definition);
    }
    layout.types = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.imports.len());
    for import in &module.imports {
        out.strref(&import.module_name);
        out.strref(&import.module_content_id);
        out.strref(&import.binding);
    }
    layout.imports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.capability_imports.len());
    for import in &module.capability_imports {
        out.strref(&import.interface);
        out.strref(&import.binding);
        out.count(import.ty);
    }
    layout.capability_imports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.exports.len());
    for signature in &module.exports {
        write_signature(&mut out, signature);
    }
    layout.exports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.constants.len());
    for constant in &module.constants {
        write_constant(&mut out, constant);
    }
    layout.constants = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.functions.len());
    for function in &module.functions {
        write_function(&mut out, function);
    }
    layout.functions = out.bytes.len() - mark;

    // The source map, with module-level identity referenced rather than
    // repeated. Logically every entry still carries the docs/43 fields;
    // physically the identical ones name a shared record.
    let identities = collect_identities(module, &out.index);
    let mark = out.bytes.len();
    out.count(identities.len());
    for identity in &identities {
        for reference in [
            identity.source_set,
            identity.path,
            identity.content_id,
            identity.frontend_identity,
            identity.language_version,
            identity.unicode_normalization_baseline,
        ] {
            out.varint(reference as u128);
        }
        out.tag(identity.profile);
    }
    layout.identity_count = identities.len();
    layout.source_map_identities = out.bytes.len() - mark;

    let placement: BTreeMap<Identity, u32> = identities
        .iter()
        .enumerate()
        .map(|(at, identity)| (*identity, at as u32))
        .collect();
    let mark = out.bytes.len();
    out.count(module.source_map.len());
    // **Spans are written as steps, not as addresses** (encoding version 2).
    // A source map walks a module's text, so consecutive entries are usually a
    // few bytes apart and a span is usually a few bytes wide, while the
    // absolute offsets they were written as reached three varint bytes each in
    // a ceiling-sized module. Measured on the fixtures this halves the section:
    // `7.88 B` an entry became `4.00 B`, and the section is half of a
    // statement-heavy image (`docs/evidence/STAGE3_BUILD_WORKSPACE.md`).
    //
    // Zigzag both times, so the encoding is total: a map that walks backwards,
    // or a span whose end precedes its start, round-trips like any other rather
    // than being a case the writer has to rule out.
    let mut previous: i128 = 0;
    for entry in &module.source_map {
        let identity = identity_of(entry, &out.index);
        let at = placement
            .get(&identity)
            .copied()
            .expect("every identity was collected from these entries");
        // The identity reference carries the parent's presence in its low bit
        // (encoding version 3). A module has few identities and most entries
        // derive from nothing, so the reference is one byte either way and the
        // separate presence tag was a byte per entry for a bit.
        out.varint(((at as u128) << 1) | u128::from(entry.derived_from.is_some()));
        let start = entry.byte_start as i128;
        out.varint(zigzag(start - previous));
        out.varint(zigzag(entry.byte_end as i128 - start));
        previous = start;
        if let Some(parent) = entry.derived_from {
            out.count(parent);
        }
    }
    layout.source_map_entries = out.bytes.len() - mark;
    layout.source_map_inline_equivalent = inline_source_map_bytes(module);
    let (delta, shared, spans) = candidate_source_map_bytes(module);
    layout.source_map_delta_equivalent = delta;
    layout.source_map_shared_span_equivalent = shared;
    layout.span_count = spans;

    let (count, refs, tags, packed) = instruction_field_bytes(module);
    layout.instruction_count = count;
    layout.instruction_source_refs = refs;
    layout.instruction_tags = tags;
    layout.instruction_packed_equivalent = packed;

    layout.payload = out.bytes.len();
    let image = frame(&out.bytes);
    layout.image = image.len();
    (image, layout)
}

/// What the source map would cost if every entry wrote its own identity, in
/// this same varint encoding, with no table and no sharing.
fn inline_source_map_bytes(module: &Module) -> usize {
    let mut total = varint_len(module.source_map.len() as u128);
    for entry in &module.source_map {
        for text in [
            &entry.source_set,
            &entry.path,
            &entry.content_id,
            &entry.frontend_identity,
            &entry.language_version,
            &entry.unicode_normalization_baseline,
        ] {
            total += varint_len(text.len() as u128) + text.len();
        }
        total += 1; // profile
        total += varint_len(entry.byte_start as u128);
        total += varint_len(entry.byte_end as u128);
        total += 1; // presence tag
        if let Some(parent) = entry.derived_from {
            total += varint_len(parent as u128);
        }
    }
    total
}

/// What two other source-map encodings would cost on this module.
///
/// **Arithmetic, not a format.** Both are computed from the entries the module
/// actually has, in the same varint encoding the writer uses, so a choice
/// between them is made from the module in hand rather than from a guess about
/// what source maps look like. Neither is written by anything.
///
/// - **delta**: the identity reference, then the start as a zigzag delta from
///   the previous entry's start, then the end as a length from its own start,
///   then the parent tag. Spans that walk forward in small steps cost two bytes
///   where they cost six;
/// - **shared spans**: distinct `(start, end)` pairs collected into a table the
///   way identities already are, with each entry naming one. Repetition across
///   entries is paid once.
fn candidate_source_map_bytes(module: &Module) -> (usize, usize, usize) {
    let mut delta = varint_len(module.source_map.len() as u128);
    let mut previous = 0i128;
    let mut spans: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    for entry in &module.source_map {
        delta += 1; // the identity reference and the parent's presence bit
        let start = entry.byte_start as i128;
        delta += varint_len(zigzag(start - previous));
        delta += varint_len(zigzag(entry.byte_end as i128 - start));
        if let Some(parent) = entry.derived_from {
            delta += varint_len(parent as u128);
        }
        previous = start;
        let next = spans.len() as u32;
        spans
            .entry((entry.byte_start, entry.byte_end))
            .or_insert(next);
    }

    let mut shared = varint_len(spans.len() as u128);
    for (start, end) in spans.keys() {
        shared += varint_len(*start as u128) + varint_len(*end as u128);
    }
    shared += varint_len(module.source_map.len() as u128);
    for entry in &module.source_map {
        let at = spans
            .get(&(entry.byte_start, entry.byte_end))
            .copied()
            .unwrap_or(0);
        shared += 1; // the identity reference
        shared += varint_len(at as u128);
        shared += 1; // presence tag
        if let Some(parent) = entry.derived_from {
            shared += varint_len(parent as u128);
        }
    }
    (delta, shared, spans.len())
}

/// What an instruction's fixed fields cost now, and what two changes to their
/// representation would cost.
///
/// **Arithmetic over the module, not a format.** The fields counted are the
/// ones every instruction carries whatever it does: its source-map reference,
/// its result tag, its unsafe flag and its two optional string references. The
/// candidate writes the reference as a step from the previous instruction's and
/// packs the four tags into one byte. Nothing writes either; this is what a
/// decision would be taken from.
fn instruction_field_bytes(module: &Module) -> (usize, usize, usize, usize) {
    let mut count = 0usize;
    let mut refs = 0usize;
    let mut tags = 0usize;
    let mut packed = 0usize;
    let mut previous = 0i128;
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                count += 1;
                refs += varint_len(instruction.source as u128);
                // the result tag, the unsafe flag, and one byte for each
                // optional string reference that is absent
                tags += 1 + 1 + 1 + 1;
                if instruction.result.is_some() {
                    // the value index is payload either way and is not counted
                }
                let step = instruction.source as i128 - previous;
                packed += varint_len(zigzag(step)) + 1;
                previous = instruction.source as i128;
            }
        }
    }
    (count, refs, tags, packed)
}

/// A signed step as an unsigned varint, small numbers staying small either way.
pub(crate) fn zigzag(value: i128) -> u128 {
    ((value << 1) ^ (value >> 127)) as u128
}

/// The inverse, for the reader.
pub(crate) fn unzigzag(value: u128) -> i128 {
    ((value >> 1) as i128) ^ -((value & 1) as i128)
}

fn varint_len(mut value: u128) -> usize {
    let mut bytes = 1;
    while value >= 0x80 {
        value >>= 7;
        bytes += 1;
    }
    bytes
}

/// One source-map identity, as table references.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Identity {
    source_set: u32,
    path: u32,
    content_id: u32,
    frontend_identity: u32,
    language_version: u32,
    unicode_normalization_baseline: u32,
    profile: u8,
}

fn identity_of(entry: &SourceMapEntry, index: &BTreeMap<&str, u32>) -> Identity {
    let at = |text: &str| {
        index
            .get(text)
            .copied()
            .expect("every string was collected from this module")
    };
    Identity {
        source_set: at(&entry.source_set),
        path: at(&entry.path),
        content_id: at(&entry.content_id),
        frontend_identity: at(&entry.frontend_identity),
        language_version: at(&entry.language_version),
        unicode_normalization_baseline: at(&entry.unicode_normalization_baseline),
        profile: profile_tag(entry.profile),
    }
}

fn collect_identities(module: &Module, index: &BTreeMap<&str, u32>) -> Vec<Identity> {
    let mut set = BTreeSet::new();
    for entry in &module.source_map {
        set.insert(identity_of(entry, index));
    }
    set.into_iter().collect()
}

struct Out<'a> {
    bytes: Vec<u8>,
    index: BTreeMap<&'a str, u32>,
}

impl Out<'_> {
    fn tag(&mut self, tag: u8) {
        self.bytes.push(tag);
    }

    fn flag(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    fn varint(&mut self, mut value: u128) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    fn signed(&mut self, value: i128) {
        // Zigzag, so a small negative number is a small varint.
        self.varint(((value << 1) ^ (value >> 127)) as u128);
    }

    fn count(&mut self, value: usize) {
        self.varint(value as u128);
    }

    fn blob(&mut self, value: &[u8]) {
        self.varint(value.len() as u128);
        self.bytes.extend_from_slice(value);
    }

    fn strref(&mut self, text: &str) {
        let at = self
            .index
            .get(text)
            .copied()
            .expect("every string was collected from this module");
        self.varint(at as u128);
    }

    fn opt_strref(&mut self, text: Option<&str>) {
        match text {
            Some(text) => {
                self.tag(1);
                self.strref(text);
            }
            None => self.tag(0),
        }
    }
}

fn write_header(out: &mut Out<'_>, header: &Header) {
    out.strref(&header.schema_id);
    out.strref(&header.language_version);
    out.strref(&header.unicode_normalization_baseline);
    out.tag(profile_tag(header.profile));
    out.strref(&header.module_name);
    out.strref(&header.source_set);
    out.strref(&header.path);
    out.strref(&header.content_id);
    out.strref(&header.dependency_digest);
    out.strref(&header.frontend_identity);
    out.strref(&header.source_map_revision);
    let envelope = &header.resource_envelope;
    for limit in [
        envelope.fuel,
        envelope.stack,
        envelope.allocation,
        envelope.tasks,
        envelope.workers,
        envelope.sync,
        envelope.shared,
        envelope.cleanup,
        envelope.recursion,
        envelope.imports,
    ] {
        out.varint(limit);
    }
    out.strref(&header.capability_interface_digest);
}

fn write_type(out: &mut Out<'_>, definition: &TypeDef) {
    match definition {
        TypeDef::Unit => out.tag(0),
        TypeDef::Bool => out.tag(1),
        TypeDef::Int(kind) => {
            out.tag(2);
            out.tag(int_tag(*kind));
        }
        TypeDef::Size => out.tag(3),
        TypeDef::Duration => out.tag(4),
        TypeDef::Text => out.tag(5),
        TypeDef::Bytes => out.tag(6),
        TypeDef::ConversionError => out.tag(7),
        TypeDef::Event => out.tag(8),
        TypeDef::Semaphore => out.tag(9),
        TypeDef::Barrier => out.tag(10),
        TypeDef::Latch => out.tag(11),
        TypeDef::AtomicBool => out.tag(12),
        TypeDef::AtomicU32 => out.tag(13),
        TypeDef::AtomicU64 => out.tag(14),
        TypeDef::Option(inner) => {
            out.tag(15);
            out.count(*inner);
        }
        TypeDef::Task(inner) => {
            out.tag(16);
            out.count(*inner);
        }
        TypeDef::TaskResult(inner) => {
            out.tag(17);
            out.count(*inner);
        }
        TypeDef::Shared(inner) => {
            out.tag(18);
            out.count(*inner);
        }
        TypeDef::Region(inner) => {
            out.tag(19);
            out.count(*inner);
        }
        TypeDef::DmaRegion(inner) => {
            out.tag(20);
            out.count(*inner);
        }
        TypeDef::Mutex(inner) => {
            out.tag(21);
            out.count(*inner);
        }
        TypeDef::RwLock(inner) => {
            out.tag(22);
            out.count(*inner);
        }
        TypeDef::Channel(inner) => {
            out.tag(23);
            out.count(*inner);
        }
        TypeDef::Slice(inner) => {
            out.tag(24);
            out.count(*inner);
        }
        TypeDef::Result(ok, error) => {
            out.tag(25);
            out.count(*ok);
            out.count(*error);
        }
        TypeDef::Array(element, length) => {
            out.tag(26);
            out.count(*element);
            out.varint(*length as u128);
        }
        TypeDef::Tuple(elements) => {
            out.tag(27);
            out.count(elements.len());
            for element in elements {
                out.count(*element);
            }
        }
        TypeDef::Function(parameters, result) => {
            out.tag(28);
            out.count(parameters.len());
            for parameter in parameters {
                out.count(*parameter);
            }
            out.count(*result);
        }
        TypeDef::Capability(interface) => {
            out.tag(29);
            out.strref(interface);
        }
        TypeDef::Nominal {
            module_content_id,
            export_name,
            kind,
            fields,
            variants,
        } => {
            out.tag(30);
            out.strref(module_content_id);
            out.strref(export_name);
            out.tag(match kind {
                NominalKind::Record => 0,
                NominalKind::Enum => 1,
            });
            out.count(fields.len());
            for field in fields {
                out.count(*field);
            }
            out.count(variants.len());
            for variant in variants {
                out.strref(&variant.name);
                out.count(variant.payload.len());
                for payload in &variant.payload {
                    out.count(*payload);
                }
            }
        }
        // ADR-0036 guards and ADR-0037 mutable regions take tags past the
        // highest allocated one, exactly as the digest scheme does.
        TypeDef::MutexGuard(inner) => {
            out.tag(31);
            out.count(*inner);
        }
        TypeDef::ReadGuard(inner) => {
            out.tag(32);
            out.count(*inner);
        }
        TypeDef::WriteGuard(inner) => {
            out.tag(33);
            out.count(*inner);
        }
        TypeDef::RegionMut(inner) => {
            out.tag(34);
            out.count(*inner);
        }
        TypeDef::DmaRegionMut(inner) => {
            out.tag(35);
            out.count(*inner);
        }
    }
}

fn write_signature(out: &mut Out<'_>, signature: &Signature) {
    out.strref(&signature.name);
    out.tag(match signature.visibility {
        Visibility::Private => 0,
        Visibility::Public => 1,
    });
    out.flag(signature.is_async);
    out.count(signature.parameters.len());
    for parameter in &signature.parameters {
        out.strref(&parameter.name);
        out.count(parameter.ty);
        out.tag(match parameter.mode {
            PassMode::Owned => 0,
            PassMode::SharedBorrow => 1,
            PassMode::MutableBorrow => 2,
        });
    }
    out.count(signature.result);
    out.count(signature.effects.len());
    for effect in &signature.effects {
        out.strref(effect);
    }
}

fn write_constant(out: &mut Out<'_>, constant: &Constant) {
    match constant {
        Constant::Unit => out.tag(0),
        Constant::Bool(value) => {
            out.tag(1);
            out.flag(*value);
        }
        Constant::Int(kind, value) => {
            out.tag(2);
            out.tag(int_tag(*kind));
            out.signed(*value);
        }
        Constant::Size(value) => {
            out.tag(3);
            out.varint(*value);
        }
        Constant::Duration(value) => {
            out.tag(4);
            out.varint(*value);
        }
        Constant::Text(value) => {
            out.tag(5);
            out.strref(value);
        }
        Constant::Bytes(value) => {
            out.tag(6);
            out.blob(value);
        }
    }
}

fn write_function(out: &mut Out<'_>, function: &Function) {
    write_signature(out, &function.signature);
    out.tag(match function.origin {
        FunctionOrigin::Declared => 0,
        FunctionOrigin::LoweredBody => 1,
    });
    out.count(function.source);
    out.varint(function.stack_contribution);
    out.varint(function.fuel_contribution);
    out.varint(function.cleanup_contribution);
    out.count(function.values.len());
    for ty in &function.values {
        out.count(*ty);
    }
    out.count(function.blocks.len());
    for block in &function.blocks {
        write_block(out, block);
    }
}

fn write_block(out: &mut Out<'_>, block: &Block) {
    out.count(block.parameters.len());
    for parameter in &block.parameters {
        out.count(*parameter);
    }
    out.count(block.instructions.len());
    for instruction in &block.instructions {
        write_instruction(out, instruction);
    }
    write_terminator(out, &block.terminator);
    out.count(block.source);
}

fn write_instruction(out: &mut Out<'_>, instruction: &Instruction) {
    match instruction.result {
        Some(value) => {
            out.tag(1);
            out.count(value);
        }
        None => out.tag(0),
    }
    out.count(instruction.ty);
    write_op(out, &instruction.op);
    out.count(instruction.source);
    out.flag(instruction.unsafe_block);
    out.opt_strref(instruction.runtime_contract.as_deref());
    out.opt_strref(instruction.unsafe_interface.as_deref());
}

fn write_operand(out: &mut Out<'_>, operand: &Operand) {
    match operand {
        Operand::Value(value) => {
            out.tag(0);
            out.count(*value);
        }
        Operand::Constant(constant) => {
            out.tag(1);
            out.count(*constant);
        }
    }
}

fn write_operands(out: &mut Out<'_>, operands: &[Operand]) {
    out.count(operands.len());
    for operand in operands {
        write_operand(out, operand);
    }
}

fn write_place(out: &mut Out<'_>, place: &Place) {
    out.count(place.root);
    out.count(place.path.len());
    for step in &place.path {
        match step {
            PlaceStep::Field(index) => {
                out.tag(0);
                out.count(*index);
            }
            PlaceStep::Index(Some(index)) => {
                out.tag(1);
                out.varint(*index as u128);
            }
            PlaceStep::Index(None) => out.tag(2),
            PlaceStep::DynamicIndex(value) => {
                out.tag(3);
                out.count(*value);
            }
        }
    }
}

fn write_op(out: &mut Out<'_>, op: &Op) {
    match op {
        Op::Const(constant) => {
            out.tag(0);
            out.count(*constant);
        }
        Op::Aggregate { ty, operands } => {
            out.tag(1);
            out.count(*ty);
            write_operands(out, operands);
        }
        Op::Variant {
            ty,
            index,
            operands,
        } => {
            out.tag(2);
            out.count(*ty);
            out.count(*index);
            write_operands(out, operands);
        }
        Op::Read { place } => {
            out.tag(3);
            write_place(out, place);
        }
        Op::Move { place } => {
            out.tag(4);
            write_place(out, place);
        }
        Op::Write { place, value } => {
            out.tag(5);
            write_place(out, place);
            write_operand(out, value);
        }
        Op::Borrow { place, kind } => {
            out.tag(6);
            write_place(out, place);
            out.tag(match kind {
                BorrowKind::Shared => 0,
                BorrowKind::Mutable => 1,
            });
        }
        Op::Drop { place } => {
            out.tag(7);
            write_place(out, place);
        }
        Op::Binary { op, left, right } => {
            out.tag(8);
            out.tag(binary_tag(*op));
            write_operand(out, left);
            write_operand(out, right);
        }
        Op::Unary { op, operand } => {
            out.tag(9);
            out.tag(match op {
                UnaryOp::Negate => 0,
                UnaryOp::Not => 1,
            });
            write_operand(out, operand);
        }
        Op::Widen { operand, to } => {
            out.tag(10);
            write_operand(out, operand);
            out.tag(int_tag(*to));
        }
        Op::Call { target, operands } => {
            out.tag(11);
            match target {
                CallTarget::Local(index) => {
                    out.tag(0);
                    out.count(*index);
                }
                CallTarget::Imported { import, name } => {
                    out.tag(1);
                    out.count(*import);
                    out.strref(name);
                }
                CallTarget::Predeclared(name) => {
                    out.tag(2);
                    out.strref(name);
                }
            }
            write_operands(out, operands);
        }
        Op::Spawn { body, captures } => {
            out.tag(12);
            out.count(*body);
            write_operands(out, captures);
        }
        Op::Join { task } => {
            out.tag(13);
            write_operand(out, task);
        }
        Op::Await { task } => {
            out.tag(14);
            write_operand(out, task);
        }
        Op::Cancel { task } => {
            out.tag(15);
            write_operand(out, task);
        }
        Op::Atomic {
            operation,
            target,
            operands,
            order,
            failure_order,
        } => {
            out.tag(16);
            out.tag(atomic_tag(*operation));
            write_operand(out, target);
            write_operands(out, operands);
            out.tag(order_tag(*order));
            match failure_order {
                Some(order) => {
                    out.tag(1);
                    out.tag(order_tag(*order));
                }
                None => out.tag(0),
            }
        }
        Op::Capability {
            import,
            further_imports,
            right,
            operands,
        } => {
            out.tag(17);
            out.count(*import);
            // Every capability the operation requires is written, in order: an
            // artifact that required a second one must not encode the same as
            // one that did not.
            out.count(further_imports.len());
            for import in further_imports {
                out.count(*import);
            }
            out.strref(right);
            write_operands(out, operands);
        }
        Op::Resource {
            kind,
            amount,
            release,
        } => {
            out.tag(18);
            out.tag(resource_tag(*kind));
            write_operand(out, amount);
            out.flag(*release);
        }
        Op::RegisterCleanup { body } => {
            out.tag(19);
            out.count(*body);
        }
        Op::RunCleanups { calls } => {
            out.tag(20);
            out.count(calls.len());
            for call in calls {
                out.count(call.body);
                write_operands(out, &call.captures);
            }
        }
        Op::Closure { body, captures } => {
            out.tag(21);
            out.count(*body);
            write_operands(out, captures);
        }
        Op::CallValue { callee, operands } => {
            out.tag(22);
            write_operand(out, callee);
            write_operands(out, operands);
        }
        Op::Share { operand } => {
            out.tag(23);
            write_operand(out, operand);
        }
        Op::Lock { object, mode } => {
            out.tag(24);
            out.tag(lock_tag(*mode));
            write_operand(out, object);
        }
    }
}

fn write_terminator(out: &mut Out<'_>, terminator: &Terminator) {
    match terminator {
        Terminator::Return(value) => {
            out.tag(0);
            match value {
                Some(operand) => {
                    out.tag(1);
                    write_operand(out, operand);
                }
                None => out.tag(0),
            }
        }
        Terminator::Branch { target, arguments } => {
            out.tag(1);
            out.count(*target);
            write_operands(out, arguments);
        }
        Terminator::BranchIf {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            out.tag(2);
            write_operand(out, condition);
            out.count(*true_target);
            write_operands(out, true_arguments);
            out.count(*false_target);
            write_operands(out, false_arguments);
        }
        Terminator::MatchEnum { subject, arms } => {
            out.tag(3);
            write_operand(out, subject);
            out.count(arms.len());
            for (variant, target) in arms {
                out.count(*variant);
                out.count(*target);
            }
        }
        Terminator::PropagateError { result, ok_target } => {
            out.tag(4);
            write_operand(out, result);
            out.count(*ok_target);
        }
        Terminator::Trap(code) => {
            out.tag(5);
            out.strref(code);
        }
    }
}

/// Every string the encoder will reference, gathered before anything is
/// written.
///
/// A separate traversal rather than interning as it goes, so that the table can
/// be sorted: a canonical order is one a reader can check, and first-occurrence
/// order is not.
fn collect_strings(module: &Module) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    let keep = |text: &str, set: &mut BTreeSet<String>| {
        if !set.contains(text) {
            set.insert(String::from(text));
        }
    };
    let header = &module.header;
    for text in [
        &header.schema_id,
        &header.language_version,
        &header.unicode_normalization_baseline,
        &header.module_name,
        &header.source_set,
        &header.path,
        &header.content_id,
        &header.dependency_digest,
        &header.frontend_identity,
        &header.source_map_revision,
        &header.capability_interface_digest,
    ] {
        keep(text, &mut strings);
    }
    for definition in &module.types {
        match definition {
            TypeDef::Nominal {
                module_content_id,
                export_name,
                variants,
                ..
            } => {
                keep(module_content_id, &mut strings);
                keep(export_name, &mut strings);
                for variant in variants {
                    keep(&variant.name, &mut strings);
                }
            }
            TypeDef::Capability(interface) => keep(interface, &mut strings),
            _ => {}
        }
    }
    for import in &module.imports {
        keep(&import.module_name, &mut strings);
        keep(&import.module_content_id, &mut strings);
        keep(&import.binding, &mut strings);
    }
    for import in &module.capability_imports {
        keep(&import.interface, &mut strings);
        keep(&import.binding, &mut strings);
    }
    for signature in module
        .exports
        .iter()
        .chain(module.functions.iter().map(|function| &function.signature))
    {
        keep(&signature.name, &mut strings);
        for parameter in &signature.parameters {
            keep(&parameter.name, &mut strings);
        }
        for effect in &signature.effects {
            keep(effect, &mut strings);
        }
    }
    for constant in &module.constants {
        if let Constant::Text(value) = constant {
            keep(value, &mut strings);
        }
    }
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Some(text) = &instruction.runtime_contract {
                    keep(text, &mut strings);
                }
                if let Some(text) = &instruction.unsafe_interface {
                    keep(text, &mut strings);
                }
                match &instruction.op {
                    Op::Call {
                        target: CallTarget::Imported { name, .. },
                        ..
                    }
                    | Op::Call {
                        target: CallTarget::Predeclared(name),
                        ..
                    } => keep(name, &mut strings),
                    Op::Capability { right, .. } => keep(right, &mut strings),
                    _ => {}
                }
            }
            if let Terminator::Trap(code) = &block.terminator {
                keep(code, &mut strings);
            }
        }
    }
    for entry in &module.source_map {
        for text in [
            &entry.source_set,
            &entry.path,
            &entry.content_id,
            &entry.frontend_identity,
            &entry.language_version,
            &entry.unicode_normalization_baseline,
        ] {
            keep(text, &mut strings);
        }
    }
    strings
}
