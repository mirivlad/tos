// SPDX-License-Identifier: GPL-3.0-or-later
//! Link the nucleus as a raw binary image: no ELF container, `boot_entry`
//! first (see linker.ld). The UEFI loader copies the image to executable
//! pages and jumps to its first byte, per boot ABI v1.

fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("linker.ld");
    println!("cargo:rerun-if-changed={}", ld.display());
    // -no-pie: without it LLD links a PIE by default, LLVM emits GOT
    // indirections for cross-crate calls, and the R_X86_64_RELATIVE relocs in
    // .got are NOT applied when emitting --oformat=binary — the slots stay
    // garbage and the nucleus faults on the first shared call (observed in
    // QEMU: #GP with RIP=0 after TOS.NUCLEUS.ENTRY). Static + fixed base
    // + -no-pie makes every call direct.
    println!("cargo:rustc-link-arg-bins=-no-pie");
    println!("cargo:rustc-link-arg-bins=-T{}", ld.display());
    // The audit build keeps the ELF container. Nothing else changes: the same
    // objects, the same linker script, the same symbols. It exists because the
    // production image has no symbol table by construction, and "what does this
    // binary actually call for memcpy" is a question that can only be answered
    // from an artifact that still says.
    println!("cargo:rerun-if-env-changed=TOS_NUCLEUS_ELF");
    if std::env::var_os("TOS_NUCLEUS_ELF").is_none() {
        println!("cargo:rustc-link-arg-bins=--oformat=binary");
    }
}
