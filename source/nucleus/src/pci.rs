// SPDX-License-Identifier: GPL-3.0-or-later
//! The PCI hardware mechanism, and nothing above it.
//!
//! `SYSTEM_ABI_V1` §2.1 admits a narrowly capability-gated hardware mechanism
//! primitive where five conditions hold, and ADR-0079 §6 is the decision that
//! this is one. What lives here is the part no textual service can perform: a
//! configuration transaction that requires ring 0, and the object that says
//! which function a caller is entitled to perform it against.
//!
//! **What deliberately does not live here.** This module cannot tell a VirtIO
//! block device from a serial controller, does not enumerate, does not match a
//! driver to a device, and holds no opinion about which module should own
//! anything. It answers "read offset 8 of the function this capability names"
//! and nothing else. Discovery is reading identifiers, which a textual service
//! does through operation 25; deciding what those identifiers mean is policy,
//! and ADR-0051 leaves it open.
//!
//! **The backend is Configuration Mechanism #1** (`0xCF8`/`0xCFC`), which is an
//! implementation fact rather than a public one (ADR-0079 §7). The ports are
//! unreachable from CPL 3 and stay so: IOPL is 0, the TSS admits no port at
//! ring 3, and no mapping of them exists. The address/data pair is a single
//! global window, so it is owned here and used under the nucleus's own
//! serialisation — which costs nothing, because the dispatcher is single-context
//! with interrupts masked. Replacing this with ECAM later changes this file and
//! nothing a textual module can observe.

use core::arch::asm;

/// The configuration address port of Mechanism #1.
const CONFIG_ADDRESS: u16 = 0xCF8;
/// The configuration data port. A read or write of 1, 2 or 4 bytes lands at
/// `CONFIG_DATA + (offset & 3)`, which is why the offset's low two bits are
/// carried to the port and masked out of the address.
const CONFIG_DATA: u16 = 0xCFC;

/// Conventional configuration space, which is what this mechanism reaches.
///
/// Extended configuration space is not "not implemented yet" here: Mechanism #1
/// cannot express an offset above this, so the contract promises exactly what
/// the mechanism delivers (`PLATFORM_INTERFACE_V1` §4) and refuses the rest
/// rather than wrapping into a different register.
pub const CONVENTIONAL_CONFIG_BYTES: u64 = 256;

/// How many PCI functions may be assigned at once.
///
/// A fixed nucleus bound over statically reserved slots, in the class of
/// `MAX_CAPABILITIES`: the table decides what callers may reach, and a table
/// sized by its users is not a bound.
pub const MAX_ASSIGNMENTS: usize = 8;

/// The architectural ranges. Named rather than written as literals at the point
/// of use, because a bus, a device and a function are three different widths and
/// a reader should not have to remember which is five bits.
const MAX_BUS: u64 = 255;
const MAX_DEVICE: u64 = 31;
const MAX_FUNCTION: u64 = 7;

/// One PCI bus scope: the root of every later PCI authority.
///
/// **Minted at the boot/platform boundary and by nothing else** (`CAPABILITY_V1`
/// §2's third origin class, ADR-0079 §9). There is no operation in any contract
/// that returns one, because a bus is not derived from anything a process holds
/// and is not something only its creator can name — it is a fact about the
/// machine, and the only lawful moment to name it is the moment the launcher
/// decides what the first process may reach.
#[derive(Clone, Copy)]
struct Bus {
    segment: u16,
    first_bus: u8,
    last_bus: u8,
}

/// One assignment of one function, which is the middle of three facts that are
/// easy to conflate (ADR-0079 §10).
///
/// The device exists whether or not this is here. A capability is a process's
/// *name* for this, with its own handle generation. This is the claim itself:
/// exclusive under its root while it lives, and carrying a generation of its own
/// so that releasing a function and claiming it again is a different assignment
/// rather than the same one continued.
#[derive(Clone, Copy)]
struct Assignment {
    segment: u16,
    bus: u8,
    device: u8,
    function: u8,
    /// Advances when the slot is reused, so a handle held across a release and
    /// a re-claim resolves to nothing rather than to the new occupant.
    generation: u32,
    /// How many capabilities name this assignment.
    ///
    /// **The assignment is exclusive; the capability is not.** One claim exists
    /// per function, and `capability_attenuate` may make a second, narrower name
    /// for it — which is what a later split between a bus manager and a driver
    /// needs. The claim ends when the last name goes, and not before.
    names: u32,
    /// How many derived hardware objects exist under this assignment
    /// (ADR-0081 §14).
    ///
    /// **The assignment outlives its function handles when it must.** It stays
    /// live while *either* a capability names it or a descendant exists, so
    /// releasing the last `FunctionConfig` does not let the same BDF be claimed
    /// again while a mapping is still reaching it — and a manager releasing its
    /// own handle does not destroy a driver's window. Only when both reach zero
    /// does the claim end and the generation advance.
    ///
    /// Written to be generic: an IRQ or DMA object will be a descendant of the
    /// same assignment under the same invariant.
    descendants: u32,
    /// Each BAR's base and extent, measured once at claim time (§3).
    bars: [Bar; BARS],
    /// Where this function's MSI-X structures are, read once at claim time.
    ///
    /// **Read before any capability of this function is granted**, because the
    /// two refusals it feeds (ADR-0082 §5) must hold for the first mapping and
    /// the first configuration write, not from the second onwards.
    msix: MsiX,
    live: bool,
}

/// Where a function keeps the structures that decide where its interrupts go.
///
/// **This is PCI mechanism and not device knowledge.** Capability id `0x11` and
/// the layout of its four words are in the PCI specification; nothing here knows
/// what the device behind them is, and a gate asserts that ring 0 contains no
/// VirtIO vocabulary.
#[derive(Clone, Copy)]
struct MsiX {
    /// The capability's offset in configuration space, or zero when the function
    /// has none. Zero is unambiguous: the first 64 bytes are the standard header
    /// and no capability can start there.
    capability: u8,
    /// Which BAR the table lives in, and where in it.
    table_bar: u8,
    table_offset: u64,
    /// Which BAR the pending-bit array lives in, and where in it. Often the same
    /// BAR as the table and sometimes not, so both are kept.
    pba_bar: u8,
    pba_offset: u64,
    /// How many entries the table has. The specification stores this as
    /// *N − 1*; what is kept here is N, so that arithmetic over it reads as the
    /// count it is.
    entries: u16,
}

impl MsiX {
    const ABSENT: Self = Self {
        capability: 0,
        table_bar: 0,
        table_offset: 0,
        pba_bar: 0,
        pba_offset: 0,
        entries: 0,
    };

    /// Whether the function has one at all.
    fn present(&self) -> bool {
        self.capability != 0 && self.entries != 0
    }

    /// The bytes the table occupies: sixteen per entry, by the specification.
    fn table_extent(&self) -> (u64, u64) {
        (
            self.table_offset,
            u64::from(self.entries) * MSIX_ENTRY_BYTES,
        )
    }

    /// The bytes the pending-bit array occupies: one bit per entry, rounded up
    /// to whole 64-bit words, because that is the unit the array is defined in.
    fn pba_extent(&self) -> (u64, u64) {
        let words = u64::from(self.entries).div_ceil(64);
        (self.pba_offset, words * 8)
    }
}

/// One MSI-X table entry: message address low, high, message data, vector
/// control.
const MSIX_ENTRY_BYTES: u64 = 16;
/// The capability identifier the PCI specification assigns to MSI-X.
const CAP_ID_MSIX: u64 = 0x11;
/// Where the capability list starts, and the bounds a well-formed one stays in.
const CAPABILITY_POINTER: u64 = 0x34;
const FIRST_CAPABILITY: u64 = 0x40;
const LAST_CAPABILITY: u64 = 0xFC;
/// How far a capability walk will go before it decides the list is malformed.
/// Conventional space cannot hold more than this many four-byte capabilities, so
/// a device answering a loop is bounded rather than able to hang ring 0.
const MAX_CAPABILITY_STEPS: usize = 48;
/// The MSI-X capability's own words, from its start.
const MSIX_MESSAGE_CONTROL: u64 = 0x02;
const MSIX_TABLE_WORD: u64 = 0x04;
const MSIX_PBA_WORD: u64 = 0x08;
/// How many bytes the whole capability structure occupies.
const MSIX_CAPABILITY_BYTES: u64 = 12;
/// The low three bits of the table and PBA words select a BAR; the rest is a
/// byte offset that is always eight-byte aligned.
const MSIX_BIR_MASK: u64 = 0x7;

/// How many base-address registers a function has.
const BARS: usize = 6;

/// One base-address register, as the nucleus measured it.
///
/// **Measured once, at claim time, under the exclusivity Stage 4A established**
/// (ADR-0081 §13). Sizing writes to the register and reads it back, which is
/// only safe because nothing else can hold this function; doing it per mapping
/// would repeat a destructive probe on a device somebody may already be using.
#[derive(Clone, Copy)]
struct Bar {
    /// The physical address the device decodes, with the type bits removed.
    base: u64,
    /// How many bytes it covers. Zero when the BAR is unimplemented.
    length: u64,
    /// Whether this is a memory BAR. An I/O BAR is refused in this stage.
    memory: bool,
}

impl Bar {
    const EMPTY: Self = Self {
        base: 0,
        length: 0,
        memory: false,
    };
}

impl Assignment {
    const EMPTY: Self = Self {
        segment: 0,
        bus: 0,
        device: 0,
        function: 0,
        // Generations start at one, so a handle of all zeros — the value of a
        // register nobody wrote — names nothing here either.
        generation: 1,
        descendants: 0,
        bars: [Bar::EMPTY; BARS],
        msix: MsiX::ABSENT,
        names: 0,
        live: false,
    };
}

/// The one bus this machine's accepted mechanism reaches.
///
/// `None` until the launcher mints it, which is the state every boot that does
/// not endow PCI authority stays in. A nucleus with no root cannot be asked for
/// one: there is no operation that makes it, so a boot without this constant is
/// a boot in which no process can reach a device at all.
static mut ROOT: Option<Bus> = None;

static mut ASSIGNMENTS: [Assignment; MAX_ASSIGNMENTS] = [Assignment::EMPTY; MAX_ASSIGNMENTS];

/// The assignment table.
///
/// # Safety
///
/// The nucleus is single-context and the dispatcher runs with interrupts
/// masked, so there is never a second live reference to this static.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn table() -> &'static mut [Assignment; MAX_ASSIGNMENTS] {
    // SAFETY: the function's contract. Reached through a raw pointer so that no
    // reference to the static itself is ever formed.
    unsafe { &mut *core::ptr::addr_of_mut!(ASSIGNMENTS) }
}

/// Names one bus object. There is one, and the index says so rather than being
/// implied — the same shape `Object::Endpoint` uses, and the same reason: a kind
/// that names its object by index reads the same whether there is one or many.
pub const ROOT_BUS: u32 = 0;

/// Mints the root bus authority for this boot.
///
/// **The only door.** Called once by the launcher, from the constant that
/// decides what the boot process is endowed with, and never from a dispatcher.
/// `SYSTEM_ABI_V1` has no operation that reaches this function, which is what
/// makes the root a platform root rather than something a process could ask for.
///
/// **A production build never calls it, and that is the policy working rather
/// than code going unused.** `system.boot.init` requests no PCI authority, and
/// the launcher's rule is to grant nothing a module did not ask for (ADR-0055),
/// so on a canonical boot no root is minted at all — a boot in which no process
/// can reach a device. The allow says that rather than letting the warning imply
/// the function is surplus.
#[allow(dead_code)]
pub fn endow_root(segment: u16, first_bus: u8, last_bus: u8) -> Option<u32> {
    if first_bus > last_bus {
        return None;
    }
    if root().is_some() {
        // A second root would be a second, unattributable ancestry for
        // everything derived from it. One boot, one mint.
        return None;
    }
    // SAFETY: single-context nucleus; this runs before the first process is
    // entered and there is no other writer.
    unsafe {
        core::ptr::addr_of_mut!(ROOT).write(Some(Bus {
            segment,
            first_bus,
            last_bus,
        }));
    }
    Some(ROOT_BUS)
}

/// The root bus, read without forming a reference to the static that holds it.
fn root() -> Option<Bus> {
    // SAFETY: single-context nucleus; a copy of a value written before any
    // process runs, taken through a raw pointer.
    unsafe { core::ptr::addr_of!(ROOT).read() }
}

/// Whether a bus object index names a live bus.
pub fn bus_is_live(index: u32) -> bool {
    index == ROOT_BUS && root().is_some()
}

/// The scope a bus capability carries, for the launch and audit record.
///
/// `CAPABILITY_V1` §2 requires a root's scope and identity to be nameable
/// rather than assumed, and this is what names it.
pub fn bus_scope(index: u32) -> Option<(u16, u8, u8)> {
    if index != ROOT_BUS {
        return None;
    }
    root().map(|bus| (bus.segment, bus.first_bus, bus.last_bus))
}

/// Why a claim was refused, in the shape the dispatcher turns into a status.
pub enum ClaimRefused {
    /// A bus, device or function outside its architectural range. A fact about
    /// the argument, knowable without any authority.
    BadArgument,
    /// Inside its range, but outside the scope this capability was granted.
    /// A different answer from the one above on purpose: one says the caller
    /// asked for something that cannot exist, the other says it asked for
    /// something it may not reach.
    OutOfScope,
    /// The function is already assigned under this root, or the table is full.
    Limit,
}

/// Claims one function within a bus capability's scope (operation 24).
///
/// Exclusive: a function already assigned is refused while that assignment
/// lives. The exclusivity is a property of this operation rather than of the
/// capability model, which is why several capabilities may still name the
/// assignment it produces.
pub fn claim(
    bus_index: u32,
    bus: u64,
    device: u64,
    function: u64,
) -> Result<(u32, u32), ClaimRefused> {
    if bus > MAX_BUS || device > MAX_DEVICE || function > MAX_FUNCTION {
        return Err(ClaimRefused::BadArgument);
    }
    let Some((segment, first_bus, last_bus)) = bus_scope(bus_index) else {
        return Err(ClaimRefused::OutOfScope);
    };
    let bus = bus as u8;
    if bus < first_bus || bus > last_bus {
        return Err(ClaimRefused::OutOfScope);
    }
    let device = device as u8;
    let function = function as u8;
    // SAFETY: single-context nucleus with interrupts masked; this is the only
    // writer and nothing else holds the table.
    let table = unsafe { table() };
    // Exclusivity first, and over the whole table rather than the free slot:
    // a second claim of a live function is refused whether or not there is room
    // for it, because the refusal is about the function and not about capacity.
    if table
        .iter()
        .any(|entry| entry.live && entry.matches(segment, bus, device, function))
    {
        return Err(ClaimRefused::Limit);
    }
    let index = table
        .iter()
        .position(|entry| !entry.live)
        .ok_or(ClaimRefused::Limit)?;
    let entry = &mut table[index];
    entry.segment = segment;
    entry.bus = bus;
    entry.device = device;
    entry.function = function;
    entry.names = 0;
    entry.descendants = 0;
    entry.bars = [Bar::EMPTY; BARS];
    entry.msix = MsiX::ABSENT;
    entry.live = true;
    let measured = *entry;
    let generation = entry.generation;
    // **Before the caller can hold anything.** Where this function's interrupt
    // structures live is what two refusals are made of (ADR-0082 §5), and a
    // refusal that only worked from the second mapping onwards would not be one.
    // Reading it here also puts it under the same exclusivity as BAR sizing:
    // nothing else holds this function, so nothing else is walking its
    // capability list at the same time.
    table[index].msix = find_msix(&measured);
    // Sizing happens **once, here**, under the exclusivity this claim just
    // established (ADR-0081 §13). Nothing else can hold this function, so
    // nothing else can be probing it; and a later mapping never repeats the
    // probe, because a destructive read-modify-restore on a device somebody is
    // using is not a thing to do twice.
    table[index].bars = size_bars(&measured);
    Ok((index as u32, generation))
}

/// Measures every base-address register of a freshly claimed function.
///
/// The PCI-standard probe: write all-ones, read back which bits the device left
/// clear, and restore the original value exactly. It is mechanism only ring 0
/// can perform safely, and it teaches the nucleus **PCI BAR mechanics and
/// nothing about any device class**.
///
/// Memory decoding is disabled for the duration and restored afterwards,
/// including on every refusal path: a device whose BAR reads all-ones while it
/// is still decoding could answer a bus cycle from an address it does not own.
fn size_bars(entry: &Assignment) -> [Bar; BARS] {
    let mut bars = [Bar::EMPTY; BARS];
    let command = read_config(entry, COMMAND, 2) as u16;
    write_config(entry, COMMAND, 2, u64::from(command & !DECODE_BITS));
    let mut at = 0usize;
    while at < BARS {
        let offset = BAR0 + (at as u64) * 4;
        let original = read_config(entry, offset, 4);
        write_config(entry, offset, 4, 0xFFFF_FFFF);
        let probed = read_config(entry, offset, 4);
        write_config(entry, offset, 4, original);
        // An unimplemented BAR reads back as zero and never becomes authority.
        if probed == 0 {
            at += 1;
            continue;
        }
        // Bit 0 says which space it decodes. An I/O BAR is refused in this
        // stage — it is not memory and cannot be mapped — so it is recorded
        // with `memory` clear and no extent, and a request for it refuses.
        if original & 1 != 0 {
            at += 1;
            continue;
        }
        let sixty_four = (original >> 1) & 0x3 == 0x2;
        let mut base = original & !0xF;
        let mut mask = probed & !0xF;
        if sixty_four {
            // A 64-bit BAR is a pair, and the pair is one register: the high
            // half is measured with the low one and the slot above it is not a
            // BAR of its own.
            if at + 1 >= BARS {
                at += 1;
                continue;
            }
            let high_offset = offset + 4;
            let high_original = read_config(entry, high_offset, 4);
            write_config(entry, high_offset, 4, 0xFFFF_FFFF);
            let high_probed = read_config(entry, high_offset, 4);
            write_config(entry, high_offset, 4, high_original);
            base |= high_original << 32;
            mask |= high_probed << 32;
        }
        // The extent is the low run of clear bits, plus one. Checked, so a
        // device answering nonsense produces no extent rather than a wrap.
        let length = (!mask).wrapping_add(1);
        if length != 0 && base != 0 {
            bars[at] = Bar {
                base,
                length,
                memory: true,
            };
        }
        at += if sixty_four { 2 } else { 1 };
    }
    write_config(entry, COMMAND, 2, u64::from(command));
    bars
}

/// Walks a function's capability list for MSI-X, reading nothing else.
///
/// **Bounded three ways**, because a capability list is data a device supplies
/// and ring 0 walking device-supplied data is exactly where a hang belongs to
/// the device rather than to the driver: the pointer must lie in the range the
/// specification puts capabilities in, the walk stops after
/// [`MAX_CAPABILITY_STEPS`], and a pointer that does not advance is treated as
/// the end rather than followed round again.
///
/// A function with no capability list, or one with no MSI-X capability, produces
/// [`MsiX::ABSENT`] — which is not an error. Most functions have no MSI-X, and
/// the two refusals it feeds simply have nothing to refuse.
fn find_msix(entry: &Assignment) -> MsiX {
    // Bit 4 of the status register says whether there is a capability list at
    // all. A device without one may leave 0x34 holding anything.
    const STATUS: u64 = 0x06;
    const HAS_CAPABILITY_LIST: u64 = 1 << 4;
    if read_config(entry, STATUS, 2) & HAS_CAPABILITY_LIST == 0 {
        return MsiX::ABSENT;
    }
    let mut at = read_config(entry, CAPABILITY_POINTER, 1) & !0x3;
    let mut steps = 0;
    while steps < MAX_CAPABILITY_STEPS {
        if !(FIRST_CAPABILITY..=LAST_CAPABILITY).contains(&at) {
            return MsiX::ABSENT;
        }
        if read_config(entry, at, 1) == CAP_ID_MSIX {
            return read_msix_at(entry, at);
        }
        let next = read_config(entry, at + 1, 1) & !0x3;
        // A pointer that does not move is a malformed list, and following it
        // would be the loop this bound exists to prevent reaching its limit for
        // no reason.
        if next == at {
            return MsiX::ABSENT;
        }
        at = next;
        steps += 1;
    }
    MsiX::ABSENT
}

/// Reads one MSI-X capability structure, whose offset the walk above found.
fn read_msix_at(entry: &Assignment, capability: u64) -> MsiX {
    // The capability must fit inside conventional space with room for its four
    // words. A device claiming one at an offset where it cannot fit is
    // malformed, and a partial read would be worse than no capability at all.
    if capability + MSIX_CAPABILITY_BYTES > CONVENTIONAL_CONFIG_BYTES {
        return MsiX::ABSENT;
    }
    let control = read_config(entry, capability + MSIX_MESSAGE_CONTROL, 2);
    // The specification stores the table size as N − 1 in the low eleven bits.
    let entries = ((control & 0x7FF) + 1) as u16;
    let table = read_config(entry, capability + MSIX_TABLE_WORD, 4);
    let pba = read_config(entry, capability + MSIX_PBA_WORD, 4);
    MsiX {
        capability: capability as u8,
        table_bar: (table & MSIX_BIR_MASK) as u8,
        table_offset: table & !MSIX_BIR_MASK,
        pba_bar: (pba & MSIX_BIR_MASK) as u8,
        pba_offset: pba & !MSIX_BIR_MASK,
        entries,
    }
}

/// The command register, and the bits that let a function answer bus cycles.
const COMMAND: u64 = 0x04;
/// Bus Master Enable: whether the function may issue its own memory
/// transactions. **Nucleus-owned** (ADR-0082 §5), because an MSI-X message is
/// one of those transactions and so is every byte of DMA.
const BUS_MASTER: u64 = 1 << 2;
const BAR0: u64 = 0x10;
/// I/O-space and memory-space decoding.
const DECODE_BITS: u16 = 0b11;

/// A configuration read against an assignment, for the nucleus's own use.
fn read_config(entry: &Assignment, offset: u64, width: u64) -> u64 {
    if entry.segment != 0 || !access_is_valid(offset, width) {
        return 0;
    }
    let address = address_of(entry, offset);
    let port = CONFIG_DATA + (offset as u16 & 3);
    // SAFETY: as in `config_read` — fixed registers of the declared profile,
    // reachable only from ring 0, in the single context that uses them.
    unsafe {
        out_u32(CONFIG_ADDRESS, address);
        match width {
            1 => u64::from(in_u8(port)),
            2 => u64::from(in_u16(port)),
            _ => u64::from(in_u32(port)),
        }
    }
}

/// A configuration write against an assignment, for the nucleus's own use.
fn write_config(entry: &Assignment, offset: u64, width: u64, value: u64) {
    if entry.segment != 0 || !access_is_valid(offset, width) {
        return;
    }
    let address = address_of(entry, offset);
    let port = CONFIG_DATA + (offset as u16 & 3);
    // SAFETY: as above.
    unsafe {
        out_u32(CONFIG_ADDRESS, address);
        match width {
            1 => out_u8(port, value as u8),
            2 => out_u16(port, value as u16),
            _ => out_u32(port, value as u32),
        }
    }
}

/// Why a mapping request was refused.
pub enum BarRefused {
    /// A BAR index outside the architectural range, or an unaligned or
    /// overflowing request. A fact about the argument.
    BadArgument,
    /// The assignment has gone, or the BAR is unimplemented, is an I/O BAR, or
    /// does not contain the requested range.
    OutOfScope,
}

/// The physical range a mapping request names, validated against the live
/// assignment's own BAR state (ADR-0081 §13).
///
/// **The caller never supplies a physical address.** It names a BAR index and a
/// page-aligned window inside it; the base comes from what the device reported
/// and what this nucleus measured, and a request that is not entirely inside
/// that extent is refused rather than clamped.
pub fn bar_window(
    index: u32,
    generation: u32,
    bar: u64,
    offset: u64,
    length: u64,
) -> Result<u64, BarRefused> {
    let Some(entry) = assignment(index, generation) else {
        return Err(BarRefused::OutOfScope);
    };
    if bar >= BARS as u64 {
        return Err(BarRefused::BadArgument);
    }
    if length == 0 || !offset.is_multiple_of(FRAME_SIZE) || !length.is_multiple_of(FRAME_SIZE) {
        return Err(BarRefused::BadArgument);
    }
    let window = entry.bars[bar as usize];
    if !window.memory || window.length == 0 {
        return Err(BarRefused::OutOfScope);
    }
    let Some(end) = offset.checked_add(length) else {
        return Err(BarRefused::BadArgument);
    };
    if end > window.length {
        return Err(BarRefused::OutOfScope);
    }
    // **A window that reaches this function's MSI-X structures is refused**
    // (ADR-0082 §5). The table is where a message address and a message data
    // word live, so a process that could write it could deliver an interrupt of
    // its choosing to a vector of its choosing — authority nobody granted, taken
    // by writing a number. The pending-bit array goes with it: it is the other
    // half of one structure, and a page granting one usually grants both.
    //
    // `E_NO_CAPABILITY` rather than `E_BAD_ARGUMENT`, by the rule that already
    // refuses an I/O BAR: the request is well formed, and what it names is not
    // something this capability confers.
    if overlaps_msix(&entry, bar as u8, offset, length) {
        return Err(BarRefused::OutOfScope);
    }
    let Some(base) = window.base.checked_add(offset) else {
        return Err(BarRefused::BadArgument);
    };
    Ok(base)
}

/// Whether a window of one BAR reaches this function's MSI-X table or its
/// pending-bit array.
///
/// Overlap and not containment: a page that merely *touches* the table is a page
/// through which the table can be written, so the test is whether the two ranges
/// intersect at all.
fn overlaps_msix(entry: &Assignment, bar: u8, offset: u64, length: u64) -> bool {
    if !entry.msix.present() {
        return false;
    }
    let Some(end) = offset.checked_add(length) else {
        // An overflowing request is refused for its own reason a line later;
        // saying "it does not overlap" here would be answering a question about
        // a range that does not exist.
        return true;
    };
    let reaches = |structure_bar: u8, (start, bytes): (u64, u64)| -> bool {
        if structure_bar != bar || bytes == 0 {
            return false;
        }
        match start.checked_add(bytes) {
            Some(structure_end) => offset < structure_end && start < end,
            // A structure whose extent overflows is one this nucleus cannot
            // bound, and an unbounded structure is refused whole.
            None => true,
        }
    };
    reaches(entry.msix.table_bar, entry.msix.table_extent())
        || reaches(entry.msix.pba_bar, entry.msix.pba_extent())
}

/// Whether a configuration write may proceed (ADR-0082 §5).
///
/// Two structures in conventional space decide things that are the nucleus's to
/// decide, and both are inside the range operation 26 reaches. Refusing the
/// write is the only honest answer: a write that silently kept some bits and
/// dropped others would leave a caller unable to tell what the device now holds.
///
/// **Reads are untouched.** Where a table lives and whether a function is a bus
/// master are facts about hardware, and ADR-0079 already settles that a fact is
/// not authority.
fn write_is_permitted(entry: &Assignment, offset: u64, width: u64, value: u64) -> bool {
    // The MSI-X capability structure: its message control word enables, disables
    // and masks the whole mechanism, and its table and PBA words say where the
    // structures are. A caller that could move them could move the table out
    // from under the refusal above.
    if entry.msix.capability != 0 {
        let start = u64::from(entry.msix.capability);
        let end = start + MSIX_CAPABILITY_BYTES;
        if offset < end && start < offset + width {
            return false;
        }
    }
    // Bus Master Enable. Not "the command register" — a driver has ordinary
    // business there, memory-space decoding among it — but this one bit, which
    // is what lets the function issue the memory writes that an MSI-X message
    // and every byte of DMA are made of. A write that leaves it as it found it
    // is allowed, so a caller reading the register and writing it back is not
    // refused for having touched a bit it did not change.
    if offset < COMMAND + 2 && COMMAND < offset + width {
        let current = read_config(entry, COMMAND, 2);
        // Where in the written value the command register's bits fall, so that a
        // one-byte write at 0x04 and a four-byte write at 0x04 are judged on the
        // same bit of the same register.
        let shift = (COMMAND.saturating_sub(offset)) * 8;
        let written = value >> shift;
        if (written ^ current) & BUS_MASTER != 0 {
            return false;
        }
    }
    true
}

/// A page, as this mechanism measures one.
const FRAME_SIZE: u64 = 4096;

/// Records that a descendant hardware object was created under an assignment.
pub fn take_descendant(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if assignment(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: single-context nucleus; the index was checked by `assignment`.
    let entry = &mut unsafe { table() }[usable];
    entry.descendants = entry.descendants.checked_add(1).ok_or(())?;
    Ok(())
}

/// Drops a descendant, ending the assignment when nothing reaches it any more.
pub fn drop_descendant(index: u32, generation: u32) {
    let usable = index as usize;
    if assignment(index, generation).is_none() {
        return;
    }
    // SAFETY: as above.
    let entry = &mut unsafe { table() }[usable];
    entry.descendants = entry.descendants.saturating_sub(1);
    end_if_unreachable(entry);
}

/// Ends an assignment when neither a name nor a descendant reaches it.
///
/// The one rule, applied wherever either count falls: an assignment is live
/// while *something* reaches it, and the generation advances only when nothing
/// does. That is what makes a re-claimed BDF a different assignment.
fn end_if_unreachable(entry: &mut Assignment) {
    if entry.names != 0 || entry.descendants != 0 {
        return;
    }
    entry.live = false;
    entry.generation = entry.generation.wrapping_add(1);
    if entry.generation == 0 {
        entry.generation = 1;
    }
}

impl Assignment {
    fn matches(&self, segment: u16, bus: u8, device: u8, function: u8) -> bool {
        self.segment == segment
            && self.bus == bus
            && self.device == device
            && self.function == function
    }
}

/// The live assignment a capability names, if its generation still matches.
fn assignment(index: u32, generation: u32) -> Option<Assignment> {
    let index = index as usize;
    if index >= MAX_ASSIGNMENTS {
        return None;
    }
    // SAFETY: single-context nucleus; a read under a checked index.
    let entry = unsafe { table() }[index];
    (entry.live && entry.generation == generation).then_some(entry)
}

/// Whether the assignment a capability names is still usable authority.
pub fn is_live(index: u32, generation: u32) -> bool {
    assignment(index, generation).is_some()
}

/// Takes a name on an assignment, refusing rather than wrapping.
///
/// The same door every other counted object uses: a capability entry and any
/// other name for one object are counted the same way, so that releasing one
/// cannot mean something different depending on how it was made.
pub fn retain(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if assignment(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: single-context nucleus; the index was checked by `assignment`.
    let entry = &mut unsafe { table() }[usable];
    entry.names = entry.names.checked_add(1).ok_or(())?;
    Ok(())
}

/// Drops a name, and ends the assignment when it was the last one.
///
/// **The claim ends here and nowhere else**, so the function becomes claimable
/// again by exactly the event that made it unreachable. The generation advances
/// with it, which is what stops a handle kept across the gap from naming the
/// next claim.
pub fn release(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if assignment(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: as above.
    let entry = &mut unsafe { table() }[usable];
    entry.names = entry.names.checked_sub(1).ok_or(())?;
    // **Not necessarily the end of the assignment** (ADR-0081 §14). A derived
    // mapping keeps it live: releasing the last function handle while a driver
    // still holds a window must not let the same BDF be claimed again and
    // reached through that window.
    end_if_unreachable(entry);
    Ok(())
}

/// Ends an assignment that was never named.
///
/// A claim whose capability could not be granted is an assignment nothing holds
/// and nothing can release, so it would keep its function unclaimable for the
/// life of the boot. This is the only path that ends one at zero names without a
/// name having gone, and it exists so that a failure to grant leaves the table
/// exactly as it found it.
pub fn abandon(index: u32, generation: u32) {
    let usable = index as usize;
    if assignment(index, generation).is_none() {
        return;
    }
    // SAFETY: single-context nucleus; the index was checked by `assignment`.
    let entry = &mut unsafe { table() }[usable];
    if entry.names != 0 || entry.descendants != 0 {
        // Reached after all, so it is not this path's to end: the last name or
        // descendant going is what ends it, and undoing a claim somebody holds
        // would be worse than the leak this exists to prevent.
        return;
    }
    end_if_unreachable(entry);
}

/// The function an assignment names, for the audit record.
pub fn describe(index: u32, generation: u32) -> Option<(u16, u8, u8, u8)> {
    assignment(index, generation)
        .map(|entry| (entry.segment, entry.bus, entry.device, entry.function))
}

/// Whether an offset and a width name a legal conventional-configuration access.
///
/// Three conditions, each refused for its own reason: a width the mechanism
/// cannot perform, an offset that is not aligned to the width — the hardware
/// would answer a different register — and an access reaching past conventional
/// space, which this mechanism cannot express at all.
pub fn access_is_valid(offset: u64, width: u64) -> bool {
    matches!(width, 1 | 2 | 4)
        && offset.is_multiple_of(width)
        && offset
            .checked_add(width)
            .is_some_and(|end| end <= CONVENTIONAL_CONFIG_BYTES)
}

/// The Mechanism #1 address word for one register.
///
/// The low two bits of the offset are not part of it: they select which byte of
/// the addressed dword the data port exposes, and carrying them into the address
/// would name a different register.
fn address_of(entry: &Assignment, offset: u64) -> u32 {
    0x8000_0000
        | (u32::from(entry.bus) << 16)
        | (u32::from(entry.device) << 11)
        | (u32::from(entry.function) << 8)
        | (offset as u32 & 0xFC)
}

/// Reads conventional configuration space (operation 25).
///
/// Returns `None` when the capability's assignment has gone or the access is out
/// of bounds — both already refused by the dispatcher, and checked again here
/// because a mechanism that trusted its caller would be one whose bounds lived
/// somewhere other than where the hardware is touched.
pub fn config_read(index: u32, generation: u32, offset: u64, width: u64) -> Option<u64> {
    let entry = assignment(index, generation)?;
    if !access_is_valid(offset, width) {
        return None;
    }
    // Mechanism #1 has no segment. A root over any other one could not be served
    // by this backend, so it is refused rather than silently read from segment 0.
    if entry.segment != 0 {
        return None;
    }
    let address = address_of(&entry, offset);
    let port = CONFIG_DATA + (offset as u16 & 3);
    // SAFETY: the address and data ports are fixed registers of the declared
    // Stage 4 profile, reachable only from ring 0, and this is the single
    // context that uses them. The two accesses are one indivisible transaction
    // because the dispatcher runs with interrupts masked. Neither has a memory
    // operand.
    let value = unsafe {
        out_u32(CONFIG_ADDRESS, address);
        match width {
            1 => u64::from(in_u8(port)),
            2 => u64::from(in_u16(port)),
            _ => u64::from(in_u32(port)),
        }
    };
    Some(value)
}

/// Why a configuration write did not happen.
pub enum WriteRefused {
    /// The access is malformed, or the assignment it named has gone.
    BadArgument,
    /// Well formed, and what it names is not this capability's to change: a
    /// structure the nucleus owns because it decides where interrupts go or
    /// whether the device may reach memory at all (ADR-0082 §5).
    Reserved,
}

/// Writes conventional configuration space (operation 26).
pub fn config_write(
    index: u32,
    generation: u32,
    offset: u64,
    width: u64,
    value: u64,
) -> Result<(), WriteRefused> {
    let Some(entry) = assignment(index, generation) else {
        return Err(WriteRefused::BadArgument);
    };
    if !access_is_valid(offset, width) || entry.segment != 0 {
        return Err(WriteRefused::BadArgument);
    }
    // Two structures of this function are the nucleus's, and they are inside the
    // range this operation reaches (ADR-0082 §5). Checked before the transaction
    // rather than after it, so a refused write touches nothing.
    if !write_is_permitted(&entry, offset, width, value) {
        return Err(WriteRefused::Reserved);
    }
    let address = address_of(&entry, offset);
    let port = CONFIG_DATA + (offset as u16 & 3);
    // SAFETY: as for `config_read`. Only the low `width` bytes of `value` are
    // written; the rest is not consulted, so a caller cannot reach a wider
    // register by putting more in the argument.
    unsafe {
        out_u32(CONFIG_ADDRESS, address);
        match width {
            1 => out_u8(port, value as u8),
            2 => out_u16(port, value as u16),
            _ => out_u32(port, value as u32),
        }
    }
    Ok(())
}

// The port accessors. Each is `unsafe` because a port access is not something a
// caller can be given as a safe capability of this module: the callers above are
// the only ones, and each states why its ports are the right ones.
//
// # Safety
//
// The caller must name a port this module owns — the configuration address and
// data registers of Mechanism #1 — and must be running in the single nucleus
// context with interrupts masked, so that an address write and its data access
// are one transaction.

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn out_u8(port: u16, value: u8) {
    // SAFETY: the caller states which port it is reaching and why. No memory
    // operand.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn out_u16(port: u16, value: u16) {
    // SAFETY: as above.
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn out_u32(port: u16, value: u32) {
    // SAFETY: as above.
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
    }
}

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn in_u8(port: u16) -> u8 {
    let value: u8;
    // SAFETY: as above.
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn in_u16(port: u16) -> u16 {
    let value: u16;
    // SAFETY: as above.
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[inline]
// SAFETY: reached only from `config_read` and `config_write` above, each of
// which names the port it is reaching and why, under the module contract.
unsafe fn in_u32(port: u16) -> u32 {
    let value: u32;
    // SAFETY: as above.
    unsafe {
        asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventional_space_bounds_every_access() {
        assert!(access_is_valid(0, 4));
        assert!(access_is_valid(0x34, 1));
        assert!(access_is_valid(252, 4));
        // Past conventional space: the mechanism cannot express it, so it is
        // refused rather than wrapped into a different register.
        assert!(!access_is_valid(256, 1));
        assert!(!access_is_valid(253, 4));
        // A width the mechanism does not perform.
        assert!(!access_is_valid(0, 3));
        assert!(!access_is_valid(0, 8));
        assert!(!access_is_valid(0, 0));
        // Misalignment would address a different register than the caller named.
        assert!(!access_is_valid(1, 2));
        assert!(!access_is_valid(2, 4));
    }

    #[test]
    fn an_offsets_low_bits_select_a_byte_and_never_a_register() {
        let entry = Assignment {
            segment: 0,
            bus: 0,
            device: 4,
            function: 0,
            ..Assignment::EMPTY
        };
        // Offsets 4, 5, 6 and 7 are one dword: the address is the same and the
        // data port moves.
        assert_eq!(address_of(&entry, 4), address_of(&entry, 7));
        assert_ne!(address_of(&entry, 4), address_of(&entry, 8));
        assert_eq!(address_of(&entry, 0), 0x8000_0000 | (4 << 11));
    }
}
