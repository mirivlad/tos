// SPDX-License-Identifier: GPL-3.0-or-later
//! Link the runtime image as a raw binary at a fixed user address, for the same
//! reason the nucleus is linked that way: a flat image has no loader to apply
//! relocations, so its entry must be its first byte and its calls must be
//! direct.

fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("linker.ld");
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-link-arg-bins=-no-pie");
    println!("cargo:rustc-link-arg-bins=-T{}", ld.display());
    println!("cargo:rerun-if-env-changed=TOS_RUNTIME_IMAGE_ELF");
    if std::env::var_os("TOS_RUNTIME_IMAGE_ELF").is_none() {
        println!("cargo:rustc-link-arg-bins=--oformat=binary");
    }
}
