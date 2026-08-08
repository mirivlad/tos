// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic TOS capsule builder (host CLI).
//!
//! Usage:
//!   tos-capsule-tool [--git-commit <commit-id> | --detached]
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
//!   * writes `source_identity_kind = SRC_KIND_GIT`, `source_oid_alg` +
//!     `source_oid_length` (SHA-1 20B / SHA-256 32B) and the raw commit
//!     object id in `source_identity_value`, per
//!     `interfaces/boot/CAPSULE_FORMAT_V1.md` §6;
//!   * verifies that the commit contains exactly the source bytes that are
//!     placed into the capsule (`git cat-file blob <id>:<repo-path>` must equal
//!     the local file for every manifest entry);
//!   - `--meta` then records commit, per-file source paths + content SHA-256,
//!     builder version, ABI version and the output digest.
//!
//! `--detached`: computes the ADR-0018 detached-source-set identity from the
//! canonical manifest paths and content digests. It never accepts a
//! caller-selected detached digest.

use std::fs;
use std::process::Command;

use tos_capsule::build::{Builder, FileSpec};
use tos_capsule::{
    parse, ARCH_SPEC_VERSION, BUILDER_VERSION, OID_ALG_NONE, OID_ALG_SHA1, OID_ALG_SHA256,
    OID_LEN_SHA1, OID_LEN_SHA256, SRC_KIND_DETACHED, SRC_KIND_GIT,
};
use tos_hash::sha256;

struct Args {
    git_commit: Option<String>,
    detached: bool,
    out: String,
    licence: Option<String>,
    meta: Option<String>,
    manifest: String,
}

fn parse_args() -> Args {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut git_commit = None;
    let mut detached = false;
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
            "--detached" => {
                detached = true;
                i += 1;
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
            "usage: tos-capsule-tool (--git-commit <id> | --detached) --out <bin> [--licence f] [--meta f] MANIFEST"
        );
        std::process::exit(2);
    }
    if git_commit.is_none() && !detached {
        eprintln!("error: requires either --git-commit or --detached");
        std::process::exit(2);
    }
    if git_commit.is_some() && detached {
        eprintln!("error: --git-commit and --detached are mutually exclusive");
        std::process::exit(2);
    }
    Args {
        git_commit,
        detached,
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

fn spdx_expression(bytes: &[u8], path: &str) -> Result<String, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("source material '{path}' is not UTF-8 SPDX text"))?;
    let marker = "SPDX-License-Identifier:";
    let expression = text
        .lines()
        .find_map(|line| line.find(marker).map(|at| &line[at + marker.len()..]))
        .map(|value| value.trim().trim_end_matches("-->").trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("source material '{path}' has no SPDX-License-Identifier"))?;
    Ok(expression.to_string())
}

fn notice_spdx_identifiers(bytes: &[u8]) -> Result<Vec<String>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "licence notice is not valid UTF-8 SPDX text".to_string())?;
    let marker = "SPDX-License-Identifier:";
    let mut identifiers: Vec<String> = text
        .lines()
        .filter_map(|line| line.find(marker).map(|at| &line[at + marker.len()..]))
        .map(|value| value.trim().trim_end_matches("-->").trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    identifiers.sort();
    identifiers.dedup();
    if identifiers.is_empty() {
        return Err("licence notice has no SPDX-License-Identifier".to_string());
    }
    Ok(identifiers)
}

/// Resolved git identity for the capsule source.
struct Face {
    commit: String,
    oid_bytes: Vec<u8>,
}

/// One source material and its deterministic provenance record.
struct Material {
    capsule_path: String,
    repository_path: String,
    content_sha256: [u8; 32],
    spdx_expression: String,
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
        return Err(format!(
            "commit '{git_commit}' resolved to unexpected oid length {}",
            full.len()
        ));
    }
    let oid_bytes = hex_to_bytes(&full);
    if oid_bytes.is_empty() {
        return Err(format!("commit '{git_commit}' produced an empty oid"));
    }
    // `git cat-file -e` verifies the commit object exists in the object DB.
    let _ = git(&["cat-file", "-e", &format!("{full}^{{commit}}")])
        .map_err(|e| format!("commit {full} object not found: {e}"))?;
    Ok(Face {
        commit: full,
        oid_bytes,
    })
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

    if args.meta.is_some() && args.licence.is_none() {
        eprintln!("error: --meta requires --licence for v1 provenance");
        std::process::exit(2);
    }

    let mut b = Builder::new();
    let face: Option<Face>;

    if let Some(commit) = &args.git_commit {
        let f = resolve_identity(commit).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(2);
        });
        // source_identity_value = raw commit object id (20 bytes SHA-1 or
        // 32 bytes SHA-256); source_oid_alg/length identify the algorithm.
        let (alg, len) = match f.oid_bytes.len() {
            20 => (OID_ALG_SHA1, OID_LEN_SHA1),
            32 => (OID_ALG_SHA256, OID_LEN_SHA256),
            n => {
                eprintln!("error: commit oid has unsupported length {n}");
                std::process::exit(2);
            }
        };
        let mut value = [0u8; 32];
        value[..f.oid_bytes.len()].copy_from_slice(&f.oid_bytes);
        b.source_identity_kind = SRC_KIND_GIT;
        b.source_oid_alg = alg;
        b.source_oid_length = len;
        b.source_identity_value = value;
        face = Some(f);
    } else {
        // ADR-0018: `Builder::build()` derives the detached identity after it
        // has canonicalised the manifest file-table order and content digests.
        // No caller-selected detached value crosses this CLI boundary.
        debug_assert!(args.detached, "parse_args validates detached mode");
        b.source_identity_kind = SRC_KIND_DETACHED;
        b.source_oid_alg = OID_ALG_NONE;
        b.source_oid_length = 0;
        face = None;
    }

    let ml = fs::read_to_string(&args.manifest).expect("read manifest");
    // The sidecar records canonical source materials in the same sorted order
    // as Builder::build(). It never preserves local output paths or time.
    let mut materials: Vec<Material> = Vec::new();
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
            verify_committed(&face.commit, src, &content).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(2);
            });
        }
        let spdx_expression = if args.meta.is_some() {
            spdx_expression(&content, src).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(2);
            })
        } else {
            String::new()
        };
        materials.push(Material {
            capsule_path: cpath.to_string(),
            repository_path: src.to_string(),
            content_sha256: sha256(&content),
            spdx_expression,
        });
        b.add(FileSpec::new(cpath, &content));
    }

    let licence_notice = args.licence.as_ref().map(|path| {
        let bytes = fs::read(path).expect("read licence notices");
        let identifiers = if args.meta.is_some() {
            notice_spdx_identifiers(&bytes).unwrap_or_else(|e| {
                eprintln!("error: {e}");
                std::process::exit(2);
            })
        } else {
            Vec::new()
        };
        (bytes, identifiers)
    });
    if let Some((bytes, _)) = &licence_notice {
        b.set_licence_notice(bytes.clone());
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
        materials.sort_by(|left, right| left.capsule_path.cmp(&right.capsule_path));
        let (notice, notice_identifiers) = licence_notice
            .as_ref()
            .expect("--meta requires --licence above");
        for material in &materials {
            if !notice_identifiers
                .iter()
                .any(|id| id == &material.spdx_expression)
            {
                eprintln!(
                    "error: source material '{}' SPDX '{}' is absent from licence notice",
                    material.repository_path, material.spdx_expression
                );
                std::process::exit(2);
            }
        }

        let header = cap.header();
        let mut json = String::new();
        json.push_str(
            "{\n  \"format\": \"tos-capsule-provenance-v1\",\n  \"schema_version\": 1,\n",
        );
        json.push_str(&format!(
            "  \"artifact\": {{\n    \"sha256\": \"{cd_hex}\",\n    \"capsule_format\": {{\n      \"uuid\": \"2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71\",\n      \"version\": {}\n    }},\n    \"architecture_spec_version\": \"{arch}\",\n    \"builder\": {{ \"implementation\": \"tos-capsule-tool\", \"version\": {} }},\n    \"target\": {{\n      \"architecture\": \"x86_64\",\n      \"loader_abi\": \"x86_64-unknown-uefi\",\n      \"nucleus_boot_abi\": {{\n        \"minimum\": {{ \"major\": 1, \"minor\": 0 }},\n        \"maximum\": {{ \"major\": 1, \"minor\": 0 }}\n      }}\n    }}\n  }},\n",
            header.format_version, header.builder_version
        ));
        if let Some(face) = &face {
            let alg_name = match b.source_oid_alg {
                OID_ALG_SHA1 => "sha1",
                OID_ALG_SHA256 => "sha256",
                _ => "unknown",
            };
            json.push_str(&format!(
                "  \"source_identity\": {{\n    \"kind\": \"git-commit\",\n    \"source_commit\": \"{}\",\n    \"oid_algorithm\": \"{}\",\n    \"oid_length\": {},\n    \"raw_oid\": \"{}\"\n  }},\n",
                face.commit,
                alg_name,
                b.source_oid_length,
                to_hex(&b.source_identity_value[..b.source_oid_length as usize])
            ));
        } else {
            json.push_str(&format!(
                "  \"source_identity\": {{\n    \"kind\": \"detached-source-set\",\n    \"digest_algorithm\": \"sha256\",\n    \"digest\": \"{}\"\n  }},\n",
                to_hex(&header.source_identity_value)
            ));
        }
        json.push_str("  \"materials\": [\n");
        for (i, material) in materials.iter().enumerate() {
            let repository_path = if face.is_some() {
                format!(
                    ", \"repository_path\": \"{}\"",
                    escape_json(&material.repository_path)
                )
            } else {
                String::new()
            };
            json.push_str(&format!(
                "    {{\"role\": \"canonical-source\", \"capsule_path\": \"{}\"{}, \"content_sha256\": \"{}\", \"spdx_expression\": \"{}\"}}{}\n",
                escape_json(&material.capsule_path),
                repository_path,
                to_hex(&material.content_sha256),
                escape_json(&material.spdx_expression),
                if i + 1 == materials.len() {
                    ""
                } else {
                    ","
                }
            ));
        }
        json.push_str("  ],\n");
        json.push_str("  \"build\": {\n    \"identity_mode\": ");
        json.push_str(if face.is_some() {
            "\"git-commit\""
        } else {
            "\"detached-source-set\""
        });
        json.push_str(",\n    \"licence_notice_included\": true,\n    \"reproducibility_grade\": \"R0\"\n  },\n");
        json.push_str(&format!(
            "  \"licence_notice\": {{\n    \"sha256\": \"{}\",\n    \"spdx_identifiers\": [",
            to_hex(&sha256(notice))
        ));
        for (i, identifier) in notice_identifiers.iter().enumerate() {
            json.push_str(&format!(
                "\"{}\"{}",
                escape_json(identifier),
                if i + 1 == notice_identifiers.len() {
                    ""
                } else {
                    ", "
                }
            ));
        }
        json.push_str("]\n  }\n}\n");
        fs::write(m, json).expect("write meta");
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
