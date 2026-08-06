// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS capsule builder (host CLI).
//!
//! Usage:
//!   tos-capsule-tool --identity <64-hex> --out <capsule.bin>
//!                   [--licence <notices.txt>] [--meta <data.json>] MANIFEST
//!
//! MANIFEST is a text file with one `<capsule-path>\t<source-file>` per line.
//! The boot-canonical path `/system/boot/init.tos` is flagged automatically.
//! Output is deterministic for identical inputs.

use std::fs;

use tos_capsule::build::{Builder, FileSpec};
use tos_capsule::parse;
use tos_hash::sha256;

struct Args {
    identity: String,
    out: String,
    licence: Option<String>,
    meta: Option<String>,
    manifest: String,
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut identity = String::new();
    let mut out = String::new();
    let mut licence = None;
    let mut meta = None;
    let mut manifest = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--identity" => {
                identity = a[i + 1].clone();
                i += 2;
            }
            "--out" => {
                out = a[i + 1].clone();
                i += 2;
            }
            "--licence" => {
                licence = Some(a[i + 1].clone());
                i += 2;
            }
            "--meta" => {
                meta = Some(a[i + 1].clone());
                i += 2;
            }
            other => {
                manifest = Some(other.to_string());
                i += 1;
            }
        }
    }
    if identity.len() != 64 || out.is_empty() || manifest.is_none() {
        eprintln!(
            "usage: tos-capsule-tool --identity <64hex> --out <bin> [--licence f] [--meta f] MANIFEST"
        );
        std::process::exit(2);
    }
    Args {
        identity,
        out,
        licence,
        meta,
        manifest: manifest.unwrap(),
    }
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let mut v = Vec::new();
    let b = hex.as_bytes();
    for chunk in b.chunks(2) {
        let hi = (chunk[0] as char).to_digit(16).unwrap();
        let lo = (chunk[1] as char).to_digit(16).unwrap();
        v.push(((hi << 4) | lo) as u8);
    }
    v
}

fn main() {
    let args = parse_args();
    let identity = hex_to_bytes(&args.identity);
    if identity.len() != 32 {
        eprintln!("identity must be 32 bytes (64 hex)");
        std::process::exit(2);
    }
    let id_digest: [u8; 32] = identity.try_into().unwrap();

    let mut b = Builder::new();
    b.source_identity_digest = id_digest;

    let ml = fs::read_to_string(&args.manifest).expect("read manifest");
    for line in ml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cpath, src) = line.split_once('\t').expect("manifest tab-separated");
        let content = fs::read(src).expect("read content file");
        b.add(FileSpec::new(cpath, &content));
    }

    if let Some(l) = &args.licence {
        b.set_licence_notice(fs::read(l).expect("read licence notices"));
    }

    let bytes = b.build().expect("build capsule");
    fs::write(&args.out, &bytes).expect("write capsule");

    // Round-trip note: the builder is a dumb layout tool; the parser is the
    // authority. Invalid vectors are intentionally built here.
    let cap = match parse(&bytes) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("note: built capsule rejected by parser: {e:?}");
            return;
        }
    };
    let cd = sha256(&bytes);
    let mut cd_hex = String::new();
    for x in cd {
        cd_hex.push_str(&format!("{x:02x}"));
    }

    let boot = cap.boot_file().expect("boot file");
    let mut h = tos_hash::Sha256::new();
    h.update(boot.content);
    let bd = h.finalize();
    let mut bd_hex = String::new();
    for x in bd {
        bd_hex.push_str(&format!("{x:02x}"));
    }

    println!("capsule_sha256={cd_hex} files={} arch=0.2.1 builder=1", cap.file_count());

    if let Some(m) = &args.meta {
        let json = format!(
            "{{\n  \"capsule_sha256\": \"{cd_hex}\",\n  \"file_count\": {},\n  \
             \"boot_text_sha256\": \"{bd_hex}\",\n  \"architecture\": \"0.2.1\",\n  \
             \"builder\": 1\n}}\n",
            cap.file_count()
        );
        fs::write(m, json).expect("write meta");
    }
}