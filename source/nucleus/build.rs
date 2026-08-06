// SPDX-License-Identifier: GPL-3.0-or-later
//! Link the nucleus as a raw binary image: no ELF container, `boot_entry`
//! first (see linker.ld). The UEFI loader copies the image to executable
//! pages and jumps to its first byte, per boot ABI v1.

fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("linker.ld");
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-link-arg-bins=-T{}", ld.display());
    println!("cargo:rustc-link-arg-bins=--oformat=binary");
}
