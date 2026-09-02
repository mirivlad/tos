// SPDX-License-Identifier: GPL-3.0-or-later
//! `TOSBUNDLE/v1` — one immutable launch bundle over an exact image closure.
//!
//! **What a build hands over, in one contiguous artifact.** A closure is one
//! bundle and never one artifact per module: a launch is admitted whole or not
//! at all, and a target that had to gather its closure from several places could
//! be handed a different set than the one that was built.
//!
//! ```text
//! header          magic, version, counts, where everything is
//! module table    one (image, declaration) pair per closure position
//! declarations    what the build says each module is: name, identity,
//!                 exports, capability interfaces
//! images          the hostile TOSIMAGE/v1 bytes, unchanged
//! entry           which position the program starts in, and its path
//! ```
//!
//! **Everything in it is untrusted.** No receipt, no `VerifiedModuleRecord`, no
//! trusted manifest, no decoded module, no verdict of any kind: a bundle carries
//! bytes and a declaration *about* those bytes, and the target's own verifier is
//! what decides whether the declaration and the images agree. A build that lied
//! in a declaration is refused by the verifier reading the image it lied about.
//!
//! **Opaque below the runtime.** Nothing in this format is meaningful to a
//! nucleus: to ring 0 a bundle is an immutable range of bytes with a length. The
//! parser here runs in the runtime that will verify the closure, which is the
//! only component that has any business knowing what a module image is.
//!
//! **The parser is total.** Every field is read from a bounded range, every
//! offset is checked against the length that was actually handed over, and every
//! count is checked against the accepted ceilings before anything is indexed.
//! Malformed input produces a [`BundleError`], never a panic and never a read
//! outside the bundle.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

/// What every bundle starts with.
pub const MAGIC: [u8; 8] = *b"TOSBNDL\0";

/// The format version this crate writes and accepts.
pub const FORMAT_VERSION: u16 = 1;

/// The fixed header size, in bytes.
pub const HEADER_BYTES: usize = 64;

/// One module table entry: two ranges.
pub const TABLE_ENTRY_BYTES: usize = 32;

/// The accepted closure ceiling (docs/44 §2), as a bound on what may be parsed.
///
/// A count is checked against this **before** any table is walked, so a bundle
/// claiming a hundred thousand modules costs a comparison rather than an
/// allocation.
pub const MAX_MODULES: usize = 256;

/// Why a bundle was refused.
///
/// A refusal names what was wrong with the bytes, never what the caller should
/// do about it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BundleError {
    /// Shorter than a header.
    TooShort,
    /// Not a bundle.
    BadMagic,
    /// A version this parser does not accept.
    UnsupportedVersion(u16),
    /// The header declares a size this parser does not know.
    BadHeaderSize(u16),
    /// The declared total length is not the length that arrived.
    LengthMismatch { declared: u64, actual: u64 },
    /// No modules, or more than the accepted ceiling.
    ModuleCount(u32),
    /// A declared range lies outside the bundle.
    RangeOutOfBounds { at: usize },
    /// The entry position names no module of this closure.
    EntryOutOfRange { position: u32, modules: u32 },
    /// A declaration record does not decode within its own range.
    MalformedDeclaration { position: usize },
    /// Text that is not text.
    NotUtf8 { position: usize },
}

impl BundleError {
    /// A stable reason token, for a caller reporting this over an event log.
    pub fn symbol(&self) -> &'static str {
        match self {
            BundleError::TooShort => "bundle-too-short",
            BundleError::BadMagic => "bundle-bad-magic",
            BundleError::UnsupportedVersion(_) => "bundle-unsupported-version",
            BundleError::BadHeaderSize(_) => "bundle-bad-header-size",
            BundleError::LengthMismatch { .. } => "bundle-length-mismatch",
            BundleError::ModuleCount(_) => "bundle-module-count",
            BundleError::RangeOutOfBounds { .. } => "bundle-range-out-of-bounds",
            BundleError::EntryOutOfRange { .. } => "bundle-entry-out-of-range",
            BundleError::MalformedDeclaration { .. } => "bundle-malformed-declaration",
            BundleError::NotUtf8 { .. } => "bundle-not-utf8",
        }
    }
}

/// Where a bundle is being written, and how much room there is.
///
/// **Bounded by declaration.** A backing is created at a fixed size and cannot
/// grow: a build that would exceed it is refused with what it had written so
/// far, rather than taking more of the machine than the transaction reserved.
///
/// It is deliberately not an allocator. The build writes into memory it does not
/// own and cannot extend, which is the shape the output of a build has to have
/// whatever the eventual owner of that memory turns out to be.
pub trait BundleBacking {
    /// How many bytes this backing can ever hold.
    fn capacity(&self) -> usize;

    /// Writes `bytes` at `at`, or refuses because they would not fit.
    ///
    /// Writing at an offset already written is how a header is completed once
    /// the sizes it describes are known; it is not a second pass over the
    /// payload.
    fn write_at(&mut self, at: usize, bytes: &[u8]) -> Result<(), BackingFull>;
}

/// The backing could not take what was offered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackingFull {
    /// What the write would have needed, in total bytes.
    pub needed: usize,
    /// What the backing has.
    pub capacity: usize,
}

/// A backing over memory the caller already holds.
///
/// The host and test shape. It is **not** evidence of a region, a grant or any
/// other freestanding arrangement: what owns these bytes, who may write them and
/// when they become read-only are open questions, and a slice answers none of
/// them.
pub struct SliceBacking<'a> {
    bytes: &'a mut [u8],
}

impl<'a> SliceBacking<'a> {
    pub fn new(bytes: &'a mut [u8]) -> SliceBacking<'a> {
        SliceBacking { bytes }
    }

    /// What has been written, for a caller that will now read it.
    ///
    /// The whole backing, not the bundle: how much of it a bundle occupies is
    /// what the writer's `finish` returned, and a reader is handed exactly that
    /// many bytes. A backing is not self-describing and this does not pretend
    /// otherwise.
    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }
}

impl BundleBacking for SliceBacking<'_> {
    fn capacity(&self) -> usize {
        self.bytes.len()
    }

    fn write_at(&mut self, at: usize, bytes: &[u8]) -> Result<(), BackingFull> {
        let end = at.checked_add(bytes.len()).ok_or(BackingFull {
            needed: usize::MAX,
            capacity: self.bytes.len(),
        })?;
        if end > self.bytes.len() {
            return Err(BackingFull {
                needed: end,
                capacity: self.bytes.len(),
            });
        }
        self.bytes[at..end].copy_from_slice(bytes);
        Ok(())
    }
}

/// Writes one bundle, module by module, as a build produces them.
///
/// **Streaming, because that is the point.** A writer holds one table entry per
/// module — thirty-two bytes — and nothing else: the images and the declarations
/// go straight into the backing as they are produced, so what the build's own
/// account holds never includes the products.
pub struct BundleWriter<'a> {
    backing: &'a mut dyn BundleBacking,
    at: usize,
    table: alloc::vec::Vec<Entry>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Entry {
    image_offset: u64,
    image_length: u64,
    declaration_offset: u64,
    declaration_length: u64,
}

impl<'a> BundleWriter<'a> {
    /// Starts a bundle, reserving the header the finish will complete.
    pub fn new(backing: &'a mut dyn BundleBacking) -> BundleWriter<'a> {
        BundleWriter {
            backing,
            at: HEADER_BYTES,
            table: alloc::vec::Vec::new(),
        }
    }

    /// How many bytes have been written so far.
    pub fn written(&self) -> usize {
        self.at
    }

    /// How many modules have been placed.
    pub fn modules(&self) -> usize {
        self.table.len()
    }

    /// Places one module: what the build says it is, and its image bytes.
    ///
    /// The declaration is written before the image so that a reader walking the
    /// bundle in order meets what a module *claims* before the bytes that have
    /// to justify it — and neither is trusted for being where it is.
    pub fn module(
        &mut self,
        declaration: &ModuleClaim<'_>,
        image: &[u8],
    ) -> Result<(), BackingFull> {
        let declaration_offset = self.at;
        self.declaration(declaration)?;
        let declaration_length = self.at - declaration_offset;
        let image_offset = self.at;
        self.append(image)?;
        self.table.push(Entry {
            image_offset: image_offset as u64,
            image_length: image.len() as u64,
            declaration_offset: declaration_offset as u64,
            declaration_length: declaration_length as u64,
        });
        Ok(())
    }

    /// Completes the bundle: the entry, the module table and the header.
    ///
    /// Returns what the bundle occupies. Nothing is written after this, and the
    /// bytes are a complete artifact only once it has returned.
    pub fn finish(mut self, entry_position: usize, entry_path: &str) -> Result<usize, BackingFull> {
        let entry_path_offset = self.at;
        self.append(entry_path.as_bytes())?;
        let table_offset = self.at;
        for entry in core::mem::take(&mut self.table) {
            let mut record = [0u8; TABLE_ENTRY_BYTES];
            record[0..8].copy_from_slice(&entry.image_offset.to_le_bytes());
            record[8..16].copy_from_slice(&entry.image_length.to_le_bytes());
            record[16..24].copy_from_slice(&entry.declaration_offset.to_le_bytes());
            record[24..32].copy_from_slice(&entry.declaration_length.to_le_bytes());
            self.append(&record)?;
        }
        let modules = (self.at - table_offset) / TABLE_ENTRY_BYTES;
        let total = self.at;

        let mut header = [0u8; HEADER_BYTES];
        header[0..8].copy_from_slice(&MAGIC);
        header[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
        header[10..12].copy_from_slice(&(HEADER_BYTES as u16).to_le_bytes());
        header[12..16].copy_from_slice(&(modules as u32).to_le_bytes());
        header[16..24].copy_from_slice(&(total as u64).to_le_bytes());
        header[24..32].copy_from_slice(&(table_offset as u64).to_le_bytes());
        header[32..36].copy_from_slice(&(entry_position as u32).to_le_bytes());
        header[36..40].copy_from_slice(&(entry_path.len() as u32).to_le_bytes());
        header[40..48].copy_from_slice(&(entry_path_offset as u64).to_le_bytes());
        self.backing.write_at(0, &header)?;
        Ok(total)
    }

    fn declaration(&mut self, claim: &ModuleClaim<'_>) -> Result<(), BackingFull> {
        let mut fixed = [0u8; 16];
        fixed[0..4].copy_from_slice(&(claim.name.len() as u32).to_le_bytes());
        fixed[4..8].copy_from_slice(&(claim.content_id.len() as u32).to_le_bytes());
        fixed[8..12].copy_from_slice(&(claim.exports.len() as u32).to_le_bytes());
        fixed[12..16].copy_from_slice(&(claim.capabilities.len() as u32).to_le_bytes());
        self.append(&fixed)?;
        self.append(claim.name.as_bytes())?;
        self.append(claim.content_id.as_bytes())?;
        for text in claim.exports.iter().chain(claim.capabilities.iter()) {
            self.append(&(text.len() as u32).to_le_bytes())?;
            self.append(text.as_bytes())?;
        }
        Ok(())
    }

    fn append(&mut self, bytes: &[u8]) -> Result<(), BackingFull> {
        self.backing.write_at(self.at, bytes)?;
        self.at += bytes.len();
        Ok(())
    }
}

/// What a build says one module is.
///
/// A claim, and named one: the target holds every image to the claim made about
/// it, and a claim that the image does not support is a refusal rather than a
/// correction.
pub struct ModuleClaim<'a> {
    pub name: &'a str,
    pub content_id: &'a str,
    pub exports: alloc::vec::Vec<&'a str>,
    pub capabilities: alloc::vec::Vec<&'a str>,
}

/// A parsed bundle: a view over bytes nobody trusts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bundle<'a> {
    bytes: &'a [u8],
    modules: usize,
    table_offset: usize,
    entry_position: usize,
    entry_path: &'a str,
}

impl<'a> Bundle<'a> {
    /// Validates the framing of a bundle and returns a view over it.
    ///
    /// Structure only. That the images are modules, that a declaration is true
    /// of the image beside it, that the closure is the one anybody wanted — none
    /// of that is decided here, and this returning `Ok` is not evidence about
    /// any of it.
    pub fn parse(bytes: &'a [u8]) -> Result<Bundle<'a>, BundleError> {
        if bytes.len() < HEADER_BYTES {
            return Err(BundleError::TooShort);
        }
        if bytes[0..8] != MAGIC {
            return Err(BundleError::BadMagic);
        }
        let version = u16_at(bytes, 8);
        if version != FORMAT_VERSION {
            return Err(BundleError::UnsupportedVersion(version));
        }
        let header_bytes = u16_at(bytes, 10);
        if header_bytes as usize != HEADER_BYTES {
            return Err(BundleError::BadHeaderSize(header_bytes));
        }
        let modules = u32_at(bytes, 12);
        if modules == 0 || modules as usize > MAX_MODULES {
            return Err(BundleError::ModuleCount(modules));
        }
        let total = u64_at(bytes, 16);
        if total != bytes.len() as u64 {
            return Err(BundleError::LengthMismatch {
                declared: total,
                actual: bytes.len() as u64,
            });
        }
        let table_offset = u64_at(bytes, 24);
        let table_bytes = (modules as usize) * TABLE_ENTRY_BYTES;
        let table_offset = range_of(bytes, table_offset, table_bytes as u64, 0)?;
        let entry_position = u32_at(bytes, 32);
        if entry_position >= modules {
            return Err(BundleError::EntryOutOfRange {
                position: entry_position,
                modules,
            });
        }
        let entry_path_length = u32_at(bytes, 36) as u64;
        let entry_path_offset = u64_at(bytes, 40);
        let at = range_of(bytes, entry_path_offset, entry_path_length, 0)?;
        let entry_path = core::str::from_utf8(&bytes[at..at + entry_path_length as usize])
            .map_err(|_| BundleError::NotUtf8 { position: 0 })?;

        let bundle = Bundle {
            bytes,
            modules: modules as usize,
            table_offset,
            entry_position: entry_position as usize,
            entry_path,
        };
        // Every range in the table is checked now rather than when it is first
        // read: a caller that verified module three should not discover at
        // module four that the bundle was never well-formed.
        for position in 0..bundle.modules {
            let entry = bundle.entry(position);
            range_of(bytes, entry.image_offset, entry.image_length, position)?;
            let at = range_of(
                bytes,
                entry.declaration_offset,
                entry.declaration_length,
                position,
            )?;
            decode_declaration(&bytes[at..at + entry.declaration_length as usize], position)?;
        }
        Ok(bundle)
    }

    /// Validates the bundle at the **start of a container that may be larger
    /// than it**, and returns a view over exactly the bundle.
    ///
    /// **A region is a container; an artifact is a prefix of one.** A bundle
    /// arrives in whole frames because that is what memory is handed out in,
    /// and the bytes after it are the region's rather than the bundle's. A
    /// reader handed the whole region cannot simply call [`Bundle::parse`]:
    /// that requires the declared total to equal the slice, which is the right
    /// rule for a caller that already knows where the artifact ends and the
    /// wrong question to ask of a container.
    ///
    /// So the declared total is read first and **bounded by what was actually
    /// handed over** before anything is indexed. A bundle claiming more than
    /// the container holds is refused here, with the same
    /// [`BundleError::LengthMismatch`] a short slice produces — the hostile
    /// case is a bundle that says it is bigger than it is, and it is refused by
    /// comparison rather than by trust. Everything after that bound is
    /// [`Bundle::parse`] over the prefix, unchanged: same magic check, same
    /// version check, same total walk of the table.
    ///
    /// This adds no field and changes no byte of `TOSBUNDLE/v1`. What it adds is
    /// a way to *read* one out of a container, which the format already
    /// describes and had no entry point for.
    pub fn parse_prefix(bytes: &'a [u8]) -> Result<Bundle<'a>, BundleError> {
        if bytes.len() < HEADER_BYTES {
            return Err(BundleError::TooShort);
        }
        let declared = u64_at(bytes, 16);
        if declared < HEADER_BYTES as u64 || declared > bytes.len() as u64 {
            return Err(BundleError::LengthMismatch {
                declared,
                actual: bytes.len() as u64,
            });
        }
        Bundle::parse(&bytes[..declared as usize])
    }

    /// How many modules the declared closure has.
    pub fn modules(&self) -> usize {
        self.modules
    }

    /// Which position the program starts in.
    pub fn entry_position(&self) -> usize {
        self.entry_position
    }

    /// The canonical path the build says the entry is stored at.
    ///
    /// A declared input of the run, carried so a report can name what ran. It
    /// resolves nothing: the closure is already exact, and no lookup uses this.
    pub fn entry_path(&self) -> &'a str {
        self.entry_path
    }

    /// The image bytes at a position, as a window into the bundle.
    pub fn image(&self, position: usize) -> Option<&'a [u8]> {
        if position >= self.modules {
            return None;
        }
        let entry = self.entry(position);
        let at = entry.image_offset as usize;
        Some(&self.bytes[at..at + entry.image_length as usize])
    }

    /// What the build claims about the module at a position.
    pub fn declaration(&self, position: usize) -> Option<ModuleDeclaration<'a>> {
        if position >= self.modules {
            return None;
        }
        let entry = self.entry(position);
        let at = entry.declaration_offset as usize;
        let bytes = &self.bytes[at..at + entry.declaration_length as usize];
        decode_declaration(bytes, position).ok()
    }

    /// The whole artifact, for a caller that stores or hands it on.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    fn entry(&self, position: usize) -> Entry {
        let at = self.table_offset + position * TABLE_ENTRY_BYTES;
        Entry {
            image_offset: u64_at(self.bytes, at),
            image_length: u64_at(self.bytes, at + 8),
            declaration_offset: u64_at(self.bytes, at + 16),
            declaration_length: u64_at(self.bytes, at + 24),
        }
    }
}

/// What a bundle says about one module.
#[derive(Clone, Copy, Debug)]
pub struct ModuleDeclaration<'a> {
    pub name: &'a str,
    pub content_id: &'a str,
    exports: &'a [u8],
    export_count: usize,
    capabilities: &'a [u8],
    capability_count: usize,
}

impl<'a> ModuleDeclaration<'a> {
    /// The export names, in the order the build wrote them.
    pub fn exports(&self) -> TextIter<'a> {
        TextIter {
            bytes: self.exports,
            at: 0,
            left: self.export_count,
        }
    }

    /// The capability interfaces the module declares it imports.
    pub fn capabilities(&self) -> TextIter<'a> {
        TextIter {
            bytes: self.capabilities,
            at: 0,
            left: self.capability_count,
        }
    }
}

/// Walks a run of length-prefixed names that the parser already validated.
pub struct TextIter<'a> {
    bytes: &'a [u8],
    at: usize,
    left: usize,
}

impl<'a> Iterator for TextIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let length = u32_at(self.bytes, self.at) as usize;
        let start = self.at + 4;
        self.at = start + length;
        core::str::from_utf8(&self.bytes[start..self.at]).ok()
    }
}

/// Decodes one declaration record, refusing anything that does not fit exactly.
fn decode_declaration(bytes: &[u8], position: usize) -> Result<ModuleDeclaration<'_>, BundleError> {
    let malformed = BundleError::MalformedDeclaration { position };
    if bytes.len() < 16 {
        return Err(malformed);
    }
    let name_length = u32_at(bytes, 0) as usize;
    let content_length = u32_at(bytes, 4) as usize;
    let export_count = u32_at(bytes, 8) as usize;
    let capability_count = u32_at(bytes, 12) as usize;
    let mut at = 16;
    let name = text_at(bytes, at, name_length, position)?;
    at += name_length;
    let content_id = text_at(bytes, at, content_length, position)?;
    at += content_length;

    let exports_start = at;
    at = skip_texts(bytes, at, export_count, position)?;
    let exports_end = at;
    let capabilities_start = at;
    at = skip_texts(bytes, at, capability_count, position)?;
    if at != bytes.len() {
        return Err(malformed);
    }
    Ok(ModuleDeclaration {
        name,
        content_id,
        exports: &bytes[exports_start..exports_end],
        export_count,
        capabilities: &bytes[capabilities_start..at],
        capability_count,
    })
}

/// Walks `count` length-prefixed names, checking every one is inside `bytes`.
fn skip_texts(
    bytes: &[u8],
    mut at: usize,
    count: usize,
    position: usize,
) -> Result<usize, BundleError> {
    let malformed = BundleError::MalformedDeclaration { position };
    for _ in 0..count {
        if at + 4 > bytes.len() {
            return Err(malformed);
        }
        let length = u32_at(bytes, at) as usize;
        at += 4;
        text_at(bytes, at, length, position)?;
        at += length;
    }
    Ok(at)
}

fn text_at(bytes: &[u8], at: usize, length: usize, position: usize) -> Result<&str, BundleError> {
    let end = at
        .checked_add(length)
        .ok_or(BundleError::MalformedDeclaration { position })?;
    if end > bytes.len() {
        return Err(BundleError::MalformedDeclaration { position });
    }
    core::str::from_utf8(&bytes[at..end]).map_err(|_| BundleError::NotUtf8 { position })
}

/// Checks that a declared range lies inside the bundle, and returns its start.
fn range_of(bytes: &[u8], offset: u64, length: u64, at: usize) -> Result<usize, BundleError> {
    let end = offset
        .checked_add(length)
        .ok_or(BundleError::RangeOutOfBounds { at })?;
    if end > bytes.len() as u64 {
        return Err(BundleError::RangeOutOfBounds { at });
    }
    Ok(offset as usize)
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    let mut value = [0u8; 8];
    value.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(value)
}

#[cfg(test)]
mod tests;
