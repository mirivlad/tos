// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS capsule builder (host CLI).
//!
//! Usage:
//!   tos-capsule-tool [--git-commit <commit-id> | --identity <64-hex-0x0>]
//!                   [--licence <notices.txt>] [--meta <out.json>]
//!                   --out <capsule.bin> MANIFEST
//!
//! MANIFEST is a text file with one `<capsule-path>\t<source-file>` per line.
//! The boot-canonical path `/system/boot/init.tos` is flagged automatically.
//! Output is deterministic for identical inputs.
//!
//! Identity gate (`--git-commit`): binds the capsule to a real Git commit.
//!   * resolves the commit id (accepts `HEAD`, a full 40/64-hex SHA-1/SHA-256,
//!     or a short prefix);
//!   * verifies the commit object exists (`git cat-file -e <id>^{commit}`);
//!   * writes `source_identity_kind = SRC_KIND_GIT` and digest = SHA-256 of the
//!     raw commit object id (binary, 20 or 32 bytes), per
//!     `interfaces/boot/CAPSULE_FORMAT_V1.md` §6;
//!   * verifies that the commit contains exactly the source bytes that are
//!     placed into the capsule (`git cat-file blob <id>:<repo-path>` must equal
//!     the local file for every manifest entry);
//!   - `--meta` then records commit, per-file source paths + content SHA-256,
//!     builder version, ABI version and the output digest.
//!
//! `--identity <64-hex>`: detached-source-set identity (kind 2) for isolated
//! format vectors. It is permitted for format fixtures but is NOT an identity
//! gate; provenance for real builds must use `--git-commit`.

use std::fs;
use std::process::Command;

use tos_capsule::build::{Builder, FileSpec};
use tos_capsule::{parse, ARCH_SPEC_VERSION, BUILDER_VERSION, SRC_KIND_GIT};
use tos_hash::sha256;

struct Args {
    git_commit: Option<String>,
    identity: Option<String>,
    out: String,
    licence: Option<String>,
    meta: Option<String>,
    manifest: String,
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut git_commit = None;
    let mut identity = None;
    let mut out = String::new();
    let mut licence = None;
    let mut meta = None;
    let mut manifest = None;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--git-commit" => {
                git_commit = Some(a[i + 1].clone());
                i += 2;
            }
            "--identity" => {
                identity = Some(a[i + 1].clone());
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
    if out.is_empty() || manifest.is_none() {
        eprintln!(
            "usage: tos-capsule-tool (--git-commit <id> | --identity <64hex>) --out <bin> [--licence f] [--meta f] MANIFEST"
        );
        std::process::exit(2);
    }
    if git_commit.is_none() && identity.is_none() {
        eprintln!("error: requires either --git-commit or --identity");
        std::process::exit(2);
    }
    if git_commit.is_some() && identity.is_some() {
        eprintln!("error: --git-commit and --identity are mutually exclusive");
        std::process::exit(2);
    }
    Args {
        git_commit,
        identity,
        out,
        licence,
        meta,
        manifest: manifest.unwrap(),
    }
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git {} failed: {}", args.join(" "), err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(hex.len() / 2);
    let b = hex.as_bytes();
    for chunk in b.chunks(2) {
        let hi = (chunk[0] as char).to_digit(16).unwrap();
        let lo = (chunk[1] as char).to_digit(16).unwrap();
        v.push(((hi << 4) | lo) as u8);
    }
    v
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Resolved git identity for the capsule source.
struct Face {
    commit: String,
    oid_bytes: Vec<u8>,
}

/// Resolve and verify a git identity for the capsule source.
fn resolve_identity(git_commit: &str) -> Result<Face, String> {
    let oid = git(&["rev-parse", &format!("{git_commit}^{{commit}}")])
        .map_err(|e| format!("cannot resolve commit '{git_commit}': {e}"))?;
    // Full object id (40 for SHA-1, 64 for SHA-256).
    let full = if oid.len() == 40 || oid.len() == 64 {
        oid.clone()
    } else {
        git(&["rev-parse", &oid])?
    };
    if full.len() != 40 && full.len() != 64 {
        return Err(format!("commit '{git_commit}' resolved to unexpected oid length {}", full.len()));
    }
    let oid_bytes = hex_to_bytes(&full);
    if oid_bytes.is_empty() {
        return Err(format!("commit '{git_commit}' produced an empty oid"));
    }
    // `git cat-file -e` verifies the commit object exists in the object DB.
    let _ = git(&["cat-file", "-e", &format!("{full}^{{commit}}")])
        .map_err(|e| format!("commit {full} object not found: {e}"))?;
    Ok(Face { commit: full, oid_bytes })
}

/// Verify the given local file bytes are exactly the bytes committed at
/// `commit:repo_path`.
fn verify_committed(commit: &str, repo_path: &str, local: &[u8]) -> Result<(), String> {
    let out = Command::new("git")
        .args(["cat-file", "blob", &format!("{commit}:{repo_path}")])
        .output()
        .map_err(|e| format!("git cat-file failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "repo path '{repo_path}' is not under commit {commit}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if out.stdout != local {
        return Err(format!(
            "repo path '{repo_path}' differs at {commit}: capsule bytes do not match the commit"
        ));
    }
    Ok(())
}

fn main() {
    let args = parse_args();

    let mut b = Builder::new();
    let face: Option<Face>;

    if let Some(commit) = &args.git_commit {
        let f = resolve_identity(commit).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(2);
        });
        // source_identity_digest = SHA-256 of the raw commit object id.
        let digest = sha256(&f.oid_bytes);
        b.source_identity_kind = SRC_KIND_GIT;
        b.source_identity_digest = digest;
        face = Some(f);
    } else {
        let identity = args.identity.as_ref().expect("identity checked");
        if identity.len() != 64 {
            eprintln!("identity must be 32 bytes (64 hex)");
            std::process::exit(2);
        }
        let bytes = hex_to_bytes(identity);
        if bytes.len() != 32 {
            eprintln!("identity must be 32 bytes (64 hex)");
            std::process::exit(2);
        }
        let mut id_digest = [0u8; 32];
        id_digest.copy_from_slice(&bytes);
        b.source_identity_digest = id_digest;
        face = None;
    }

    let ml = fs::read_to_string(&args.manifest).expect("read manifest");
    // Track (repo_path, content_sha256) for provenance when in git mode.
    let mut manifest_rows: Vec<(String, [u8; 32])> = Vec::new();
    for line in ml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cpath, src) = line.split_once('\t').expect("manifest tab-separated");
        let content = fs::read(src).expect("read content file");
        if let Some(face) = &face {
            // src is interpreted as both the local path and the repo-relative
            // path for the identity gate.
            verify_committed(&face.commit, src, &content)
                .unwrap_or_else(|e| {
                    eprintln!("error: {e}");
                    std::process::exit(2);
                });
        }
        manifest_rows.push((src.to_string(), sha256(&content)));
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
    let cd_hex = to_hex(&cd);

    let boot = cap.boot_file().expect("boot file");
    let mut h = tos_hash::Sha256::new();
    h.update(boot.content);
    let bd = h.finalize();
    let bd_hex = to_hex(&bd);

    let arch = format!(
        "{}.{}.{}",
        (ARCH_SPEC_VERSION >> 16) & 0xff,
        (ARCH_SPEC_VERSION >> 8) & 0xff,
        ARCH_SPEC_VERSION & 0xff
    );

    println!(
        "capsule_sha256={cd_hex} files={} arch={arch} builder={BUILDER_VERSION}",
        cap.file_count()
    );

    if let Some(m) = &args.meta {
        let mut json = String::new();
        json.push_str(&format!("{{\n  \"capsule_sha256\": \"{cd_hex}\",\n  \"file_count\": {},\n  \"boot_text_sha256\": \"{bd_hex}\",\n  \"architecture\": \"{arch}\",\n  \"builder_version\": {BUILDER_VERSION},\n", cap.file_count()));
        if let Some(face) = &face {
            json.push_str(&format!(
                "  \"identity\": {{\n    \"kind\": \"git-commit\",\n    \"commit\": \"{}\",\n    \"algorithm\": \"sha256(commit-oid)\",\n    \"source_identity_digest\": \"{}\"\n  }},\n",
                face.commit,
                to_hex(&b.source_identity_digest)
            ));
        }
        json.push_str("  \"sources\": [\n");
        for (i, (path, ch)) in manifest_rows.iter().enumerate() {
            json.push_str(&format!(
                "    {{\"repo_path\": \"{}\", \"content_sha256\": \"{}\"}}{}\n",
                escape_json(path),
                to_hex(ch),
                if i + 1 == manifest_rows.len() { "" } else { "," }
            ));
        }
        json.push_str("  ]\n}\n");
        fs::write(m, json).expect("write meta");
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}