// SPDX-License-Identifier: GPL-3.0-or-later
//! The one entry an execution uses to turn image bytes into a verified module.
//!
//! ADR-0070 §1 fixes the chain and ADR-0071 §1 fixes when it runs:
//!
//! ```text
//! immutable image bytes
//!       -> hostile parser (tos-image)
//!       -> semantic tos-ir/v1 Module
//!       -> independent verifier
//!       -> { receipt, exact artifact digest, reconstructed module }
//! ```
//!
//! Three properties this module exists to hold, none of which a caller can
//! arrange for itself:
//!
//! - **the digest is taken here, over the exact bytes that were parsed.** Not
//!   over a copy, not over what a caller says it read. A digest computed
//!   somewhere else is a claim about bytes rather than a fact about them;
//! - **the verifier reaches its own conclusion** from the module the parser
//!   reconstructed. Nothing the image said about identity is input to identity;
//! - **only this function mints a [`VerifiedImage`].** Its fields are private
//!   and one of them is of a private type, so no other crate can build one by
//!   struct literal, by `Default`, or by any other route. A frontend cannot
//!   hand an engine a verified result; it can hand it bytes.

use alloc::string::String;

use tos_image::{ImageError, ParseLimits};
use tos_ir::Module;

use crate::{verify, Finding, Limits, ResolutionSnapshot, VerifiedModule};

/// The accepted ceilings, as the parser's bounds.
///
/// The numbers do not change crossing this line; what changes is who owns them.
/// docs/44 §2 publishes them, [`Limits`] declares them, and the parser is handed
/// them as data so that a format reading untrusted bytes does not depend on the
/// verifier that will read what it produces.
fn parse_limits(limits: &Limits) -> ParseLimits {
    ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    }
}

/// Proof that one exact byte sequence held one verified module.
///
/// Not constructible outside this module: the fields are private and `seal` has
/// a private type, so there is no literal, no `Default` and no builder anywhere
/// else that produces one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedImage {
    receipt: VerifiedModule,
    artifact_digest: [u8; 32],
    module: Module,
    seal: Seal,
}

/// Uninhabitable outside this module by construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Seal;

impl VerifiedImage {
    /// The semantic receipt: which module this is, and which verifier said so.
    pub fn receipt(&self) -> &VerifiedModule {
        &self.receipt
    }

    /// The exact artifact identity of the bytes that were parsed and verified.
    ///
    /// Distinct from the receipt's semantic digest, and computed here rather
    /// than accepted from anyone. A later reload compares against this and
    /// against nothing else (ADR-0071 §5).
    pub fn artifact_digest(&self) -> &[u8; 32] {
        &self.artifact_digest
    }

    /// The same digest as `sha256:<hex>`, for a log or a record.
    pub fn artifact_digest_text(&self) -> String {
        let mut hex = [0u8; 64];
        tos_hash::hex(&self.artifact_digest, &mut hex);
        alloc::format!(
            "sha256:{}",
            core::str::from_utf8(&hex).expect("hex output is ASCII")
        )
    }

    /// The module the parser reconstructed and the verifier checked.
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// Takes the module out, leaving the receipt behind.
    ///
    /// What a launch does: the materialized module is released and the receipt
    /// and digest survive it (ADR-0071 §2).
    pub fn into_parts(self) -> (VerifiedModule, [u8; 32], Module) {
        (self.receipt, self.artifact_digest, self.module)
    }
}

/// Why an image was not turned into a verified module.
///
/// Two stages, and the difference matters: a `Parser` refusal means the bytes
/// were not a module of this format, and a `Verifier` refusal means they were a
/// well-formed module that does not hold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageRefusal {
    Parser(ImageError),
    Verifier(Finding),
}

/// Turns immutable image bytes into a verified module, or refuses.
///
/// The digest is computed over `image` itself, so the bytes hashed are the
/// bytes parsed — a caller cannot hand one slice to be hashed and another to be
/// read, because there is only one slice.
pub fn verify_image(
    image: &[u8],
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
) -> Result<VerifiedImage, ImageRefusal> {
    let artifact_digest = tos_hash::sha256(image);
    let module = tos_image::parse(image, &parse_limits(limits)).map_err(ImageRefusal::Parser)?;
    let receipt = verify(&module, snapshot, limits).map_err(ImageRefusal::Verifier)?;
    Ok(VerifiedImage {
        receipt,
        artifact_digest,
        module,
        seal: Seal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn sample() -> Module {
        use tos_ir::*;
        Module {
            header: Header {
                schema_id: String::from(tos_ir::SCHEMA_ID),
                language_version: String::from(tos_ir::LANGUAGE_VERSION),
                unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
                profile: Profile::Bootstrap,
                module_name: String::from("app.image"),
                source_set: String::from("tos-verifier-tests"),
                path: String::from("app/image.tos"),
                content_id: String::from("sha256:content"),
                dependency_digest: String::from("sha256:dependencies"),
                frontend_identity: String::from("tos-core-reference/0.1.0"),
                source_map_revision: String::from(tos_ir::SOURCE_MAP_REVISION),
                resource_envelope: ResourceEnvelope {
                    fuel: 1000,
                    stack: 1024,
                    ..ResourceEnvelope::default()
                },
                capability_interface_digest: String::from("sha256:capabilities"),
            },
            types: vec![TypeDef::Int(IntKind::I32)],
            imports: Vec::new(),
            capability_imports: Vec::new(),
            exports: Vec::new(),
            constants: vec![Constant::Int(IntKind::I32, 7)],
            functions: vec![Function {
                signature: Signature {
                    name: String::from("answer"),
                    visibility: Visibility::Public,
                    is_async: false,
                    parameters: Vec::new(),
                    result: 0,
                    effects: Vec::new(),
                },
                origin: FunctionOrigin::Declared,
                source: 0,
                stack_contribution: 0,
                fuel_contribution: 0,
                cleanup_contribution: 0,
                values: vec![0],
                blocks: vec![Block {
                    parameters: Vec::new(),
                    instructions: vec![Instruction {
                        result: Some(0),
                        ty: 0,
                        op: Op::Const(0),
                        source: 0,
                        runtime_contract: None,
                        unsafe_block: false,
                        unsafe_interface: None,
                    }],
                    terminator: Terminator::Return(Some(Operand::Value(0))),
                    source: 0,
                }],
            }],
            source_map: vec![SourceMapEntry {
                source_set: String::from("tos-verifier-tests"),
                path: String::from("app/image.tos"),
                content_id: String::from("sha256:content"),
                frontend_identity: String::from("tos-core-reference/0.1.0"),
                language_version: String::from(tos_ir::LANGUAGE_VERSION),
                profile: Profile::Bootstrap,
                unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
                byte_start: 0,
                byte_end: 4,
                derived_from: None,
            }],
        }
    }

    /// The whole path, and the two digests it produces.
    #[test]
    fn a_well_formed_image_verifies_and_binds_both_digests() {
        let module = sample();
        let (image, _) = tos_image::encode(&module);
        let verified = verify_image(&image, &ResolutionSnapshot::default(), &Limits::default())
            .expect("the sample verifies");

        assert_eq!(verified.module(), &module, "the module is reconstructed");
        assert_eq!(
            verified.receipt().module_digest,
            tos_ir::module_digest(&module),
            "the receipt binds to the module the verifier traversed"
        );
        assert_eq!(
            verified.artifact_digest(),
            &tos_hash::sha256(&image),
            "the artifact digest is over the exact bytes that were parsed"
        );
        assert!(verified.artifact_digest_text().starts_with("sha256:"));
    }

    /// Malformed bytes stop at the parser, not at the verifier.
    #[test]
    fn a_malformed_image_is_refused_by_the_parser() {
        let (mut image, _) = tos_image::encode(&sample());
        image[0] ^= 0xff;
        match verify_image(&image, &ResolutionSnapshot::default(), &Limits::default()) {
            Err(ImageRefusal::Parser(ImageError::BadMagic)) => {}
            other => panic!("a wrong magic was not a parser refusal: {other:?}"),
        }
    }

    /// Corrupted **and resealed** bytes reach the parser, which is the case a
    /// digest cannot help with: an attacker who writes the bytes writes the
    /// digest.
    #[test]
    fn a_resealed_mutation_is_still_refused_or_still_verified() {
        let (good, _) = tos_image::encode(&sample());
        let mut refused = 0usize;
        let mut verified = 0usize;
        for at in tos_image::FRAME_HEADER..good.len() - tos_image::DIGEST_BYTES {
            let mut bad = good.clone();
            bad[at] ^= 0x40;
            tos_image::reseal(&mut bad);
            match verify_image(&bad, &ResolutionSnapshot::default(), &Limits::default()) {
                Ok(image) => {
                    verified += 1;
                    // Whatever it turned out to be, the receipt is about *that*
                    // module and the digest about *those* bytes.
                    assert_eq!(
                        image.receipt().module_digest,
                        tos_ir::module_digest(image.module())
                    );
                    assert_eq!(image.artifact_digest(), &tos_hash::sha256(&bad));
                }
                Err(_) => refused += 1,
            }
        }
        assert!(refused > 0, "some mutations must be refused");
        assert_eq!(
            refused + verified,
            good.len() - tos_image::FRAME_HEADER - tos_image::DIGEST_BYTES
        );
    }

    /// An unresealed mutation never reaches the parser.
    #[test]
    fn a_wrong_artifact_is_refused_before_parsing() {
        let (mut image, _) = tos_image::encode(&sample());
        let at = tos_image::FRAME_HEADER + 3;
        image[at] ^= 0x01;
        match verify_image(&image, &ResolutionSnapshot::default(), &Limits::default()) {
            Err(ImageRefusal::Parser(ImageError::WrongDigest)) => {}
            other => panic!("a corrupted image was not refused for its digest: {other:?}"),
        }
    }

    /// A well-formed image of a module that does not hold stops at the verifier.
    #[test]
    fn a_semantically_invalid_image_is_refused_by_the_verifier() {
        let mut module = sample();
        // A type index outside the table: perfectly encodable, and not a module.
        module.functions[0].blocks[0].instructions[0].ty = 99;
        let (image, _) = tos_image::encode(&module);
        match verify_image(&image, &ResolutionSnapshot::default(), &Limits::default()) {
            Err(ImageRefusal::Verifier(finding)) => {
                assert!(finding.code.starts_with('V'), "{}", finding.code);
            }
            other => panic!("an invalid module was not a verifier refusal: {other:?}"),
        }
    }

    /// An unknown encoding version and an unknown tag both fail closed here.
    #[test]
    fn unknown_versions_and_tags_fail_closed() {
        let (good, _) = tos_image::encode(&sample());

        let mut bad = good.clone();
        bad[11] = 7;
        assert_eq!(
            verify_image(&bad, &ResolutionSnapshot::default(), &Limits::default()),
            Err(ImageRefusal::Parser(ImageError::UnknownEncodingVersion(7)))
        );

        let mut bad = good.clone();
        bad[15] = 7;
        assert_eq!(
            verify_image(&bad, &ResolutionSnapshot::default(), &Limits::default()),
            Err(ImageRefusal::Parser(ImageError::UnknownSchemaVersion(7)))
        );
    }
}
