// SPDX-License-Identifier: GPL-3.0-or-later
//! What the framing accepts, and what it refuses.

use super::*;

use alloc::vec;
use alloc::vec::Vec;

fn claim<'a>(name: &'a str, content_id: &'a str, exports: &[&'a str]) -> ModuleClaim<'a> {
    ModuleClaim {
        name,
        content_id,
        exports: exports.to_vec(),
        capabilities: Vec::new(),
    }
}

/// A two-module bundle, and what it occupies.
fn written(room: usize) -> (Vec<u8>, usize) {
    let mut backing = vec![0u8; room];
    let total = {
        let mut slice = SliceBacking::new(&mut backing);
        let mut writer = BundleWriter::new(&mut slice);
        writer
            .module(
                &claim("system.lib.math", "sha256:aa", &["double"]),
                b"IMAGE-0",
            )
            .expect("the first module fits");
        writer
            .module(
                &claim("system.boot.init", "sha256:bb", &["main", "other"]),
                b"IMAGE-1-LONGER",
            )
            .expect("the second module fits");
        writer
            .finish(1, "system/boot/init.tos")
            .expect("the bundle completes")
    };
    backing.truncate(total);
    (backing, total)
}

#[test]
fn a_written_bundle_reads_back_as_what_was_written() {
    let (bytes, total) = written(4096);
    let bundle = Bundle::parse(&bytes).expect("the bundle parses");

    assert_eq!(bundle.modules(), 2);
    assert_eq!(bundle.entry_position(), 1);
    assert_eq!(bundle.entry_path(), "system/boot/init.tos");
    assert_eq!(bundle.image(0), Some(&b"IMAGE-0"[..]));
    assert_eq!(bundle.image(1), Some(&b"IMAGE-1-LONGER"[..]));
    assert_eq!(
        bundle.image(2),
        None,
        "a position the closure does not have"
    );

    let first = bundle.declaration(0).expect("the first declaration");
    assert_eq!(first.name, "system.lib.math");
    assert_eq!(first.content_id, "sha256:aa");
    assert_eq!(first.exports().collect::<Vec<_>>(), vec!["double"]);
    assert_eq!(first.capabilities().count(), 0);

    let second = bundle.declaration(1).expect("the second declaration");
    assert_eq!(second.exports().collect::<Vec<_>>(), vec!["main", "other"]);
    assert_eq!(
        bytes.len(),
        total,
        "the finish reports exactly what the bundle occupies"
    );
}

#[test]
fn a_backing_too_small_refuses_rather_than_growing() {
    let mut backing = vec![0u8; HEADER_BYTES + 8];
    let mut slice = SliceBacking::new(&mut backing);
    let mut writer = BundleWriter::new(&mut slice);
    let full = writer
        .module(
            &claim("system.lib.math", "sha256:aa", &["double"]),
            b"IMAGE-0",
        )
        .expect_err("the module does not fit");
    assert_eq!(full.capacity, HEADER_BYTES + 8);
    assert!(full.needed > full.capacity, "and it says what it needed");
}

#[test]
fn framing_that_does_not_describe_itself_is_refused() {
    let (bytes, _) = written(4096);

    assert_eq!(Bundle::parse(&bytes[..16]), Err(BundleError::TooShort));

    let mut wrong_magic = bytes.clone();
    wrong_magic[0] = b'X';
    assert_eq!(Bundle::parse(&wrong_magic), Err(BundleError::BadMagic));

    let mut wrong_version = bytes.clone();
    wrong_version[8..10].copy_from_slice(&99u16.to_le_bytes());
    assert_eq!(
        Bundle::parse(&wrong_version),
        Err(BundleError::UnsupportedVersion(99))
    );

    let mut truncated = bytes.clone();
    truncated.pop();
    assert!(matches!(
        Bundle::parse(&truncated),
        Err(BundleError::LengthMismatch { .. })
    ));

    let mut too_many = bytes.clone();
    too_many[12..16].copy_from_slice(&((MAX_MODULES + 1) as u32).to_le_bytes());
    assert!(matches!(
        Bundle::parse(&too_many),
        Err(BundleError::ModuleCount(_))
    ));

    let mut no_entry = bytes.clone();
    no_entry[32..36].copy_from_slice(&7u32.to_le_bytes());
    assert!(matches!(
        Bundle::parse(&no_entry),
        Err(BundleError::EntryOutOfRange { .. })
    ));
}

/// A range that points outside the bundle is refused before it is read.
#[test]
fn an_image_range_outside_the_bundle_is_refused() {
    let (bytes, _) = written(4096);
    let bundle = Bundle::parse(&bytes).expect("the bundle parses");
    let table_offset = u64_at(&bytes, 24) as usize;

    let mut past_the_end = bytes.clone();
    past_the_end[table_offset + 8..table_offset + 16]
        .copy_from_slice(&(bytes.len() as u64 + 1).to_le_bytes());
    assert!(matches!(
        Bundle::parse(&past_the_end),
        Err(BundleError::RangeOutOfBounds { .. })
    ));

    let mut overflowing = bytes.clone();
    overflowing[table_offset..table_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(
        Bundle::parse(&overflowing),
        Err(BundleError::RangeOutOfBounds { .. })
    ));

    assert_eq!(
        bundle.modules(),
        2,
        "the untampered bundle is still what it was"
    );
}

/// A declaration whose lengths do not add up is refused for the whole bundle.
#[test]
fn a_declaration_that_does_not_decode_refuses_the_bundle() {
    let (bytes, _) = written(4096);
    let table_offset = u64_at(&bytes, 24) as usize;
    let declaration_at = u64_at(&bytes, table_offset + 16) as usize;

    let mut lying_name = bytes.clone();
    lying_name[declaration_at..declaration_at + 4].copy_from_slice(&4096u32.to_le_bytes());
    assert!(matches!(
        Bundle::parse(&lying_name),
        Err(BundleError::MalformedDeclaration { .. })
    ));

    let mut lying_count = bytes.clone();
    lying_count[declaration_at + 8..declaration_at + 12].copy_from_slice(&9u32.to_le_bytes());
    assert!(matches!(
        Bundle::parse(&lying_count),
        Err(BundleError::MalformedDeclaration { .. })
    ));
}
