use anyhow::Result;
use clap::Subcommand;
use duck_core::{
    BoxFuture, ClapFeature, CommandOutput, CommandResult, Context, FeatureInfo, IntoCommandResult,
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    error::{self, MaterialError},
    files::{self, InputFile, Outcome},
    keybox, seed,
};

pub static FEATURE: ClapFeature<Command> = ClapFeature::new(
    FeatureInfo {
        id: "tee-attestation",
        summary: "Offline keybox extraction and RKP identity verification",
        contract: 1,
    },
    dispatch,
);

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Report implemented offline capabilities; no device access or network requests.
    Status,
    /// Extract and verify keybox XML or a captured PEM bundle.
    Keybox {
        #[arg(required_unless_present = "stdin_json", conflicts_with = "stdin_json")]
        path: Option<String>,
        #[arg(
            long,
            value_enum,
            default_value = "auto",
            conflicts_with = "stdin_json"
        )]
        format: keybox::Format,
        /// Save verified material and its report in a private output directory.
        #[arg(long, conflicts_with = "stdin_json")]
        export: bool,
        #[arg(long)]
        stdin_json: bool,
    },
    /// Compare candidate identity material to the signing leaf of a verified CSR.
    Seed {
        #[arg(required_unless_present = "stdin_json", conflicts_with = "stdin_json")]
        path: Option<String>,
        #[arg(
            long,
            required_unless_present = "stdin_json",
            conflicts_with = "stdin_json"
        )]
        csr: Option<String>,
        #[arg(long, value_enum, default_value = "raw", conflicts_with = "stdin_json")]
        format: seed::Format,
        #[arg(
            long,
            value_enum,
            default_value = "ed25519-seed",
            conflicts_with = "stdin_json"
        )]
        derivation: seed::Derivation,
        /// Byte offset of a 32-byte candidate in a raw capture.
        #[arg(long, conflicts_with = "stdin_json")]
        offset: Option<usize>,
        /// Export only if the candidate matches the CSR's signing identity.
        #[arg(long, conflicts_with = "stdin_json")]
        export: bool,
        #[arg(long)]
        stdin_json: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyboxInput {
    source: InputFile,
    #[serde(default)]
    format: keybox::Format,
    #[serde(default)]
    export: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedInput {
    source: InputFile,
    csr: InputFile,
    #[serde(default)]
    format: seed::Format,
    #[serde(default)]
    derivation: seed::Derivation,
    offset: Option<usize>,
    #[serde(default)]
    export: bool,
}

fn keybox(ctx: &Context, input: KeyboxInput) -> Result<Outcome<keybox::Report>> {
    let bytes = input.source.read(&ctx.paths)?;
    let (xml, report) = keybox::extract(&bytes, input.format)?;
    let artifacts = input
        .export
        .then(|| files::export(&ctx.paths, "keybox.xml", xml.as_bytes(), &report))
        .transpose()?;
    Ok(Outcome { report, artifacts })
}

fn seed(ctx: &Context, input: SeedInput) -> Result<Outcome<seed::Report>> {
    let bytes = input.source.read(&ctx.paths)?;
    let candidate = seed::candidate(&bytes, input.format, input.offset)?;
    let csr = input.csr.read(&ctx.paths)?;
    let report = seed::verify(&candidate, &csr, input.derivation)?;
    if input.export && !report.identity_matches {
        return Err(MaterialError::Mismatch.into());
    }
    let artifacts = input
        .export
        .then(|| files::export(&ctx.paths, "identity.bin", candidate.as_slice(), &report))
        .transpose()?;
    Ok(Outcome { report, artifacts })
}

fn dispatch<'a>(command: Command, ctx: &'a Context) -> BoxFuture<'a, CommandResult> {
    Box::pin(async move {
        match command {
            Command::Status => CommandOutput::new(
                "tee-attestation.status",
                json!({
                    "mode": "offline", "device_extraction": false,
                    "keybox_formats": ["auto", "xml", "pem"],
                    "seed_derivations": ["ed25519-seed", "p256-seed", "p256-scalar"],
                    "max_input_bytes": files::MAX_INPUT,
                    "max_inline_bytes": files::MAX_INLINE,
                }),
            ),
            Command::Keybox {
                path,
                format,
                export,
                stdin_json,
            } => {
                let input = if stdin_json {
                    files::read_json()
                } else {
                    Ok(KeyboxInput {
                        source: InputFile::Path {
                            path: path.unwrap(),
                        },
                        format,
                        export,
                    })
                };
                input
                    .and_then(|input| keybox(ctx, input))
                    .into_command("tee-attestation.keybox", error::code)
            }
            Command::Seed {
                path,
                csr,
                format,
                derivation,
                offset,
                export,
                stdin_json,
            } => {
                let input = if stdin_json {
                    files::read_json()
                } else {
                    Ok(SeedInput {
                        source: InputFile::Path {
                            path: path.unwrap(),
                        },
                        csr: InputFile::Path { path: csr.unwrap() },
                        format,
                        derivation,
                        offset,
                        export,
                    })
                };
                input
                    .and_then(|input| seed(ctx, input))
                    .into_command("tee-attestation.seed", error::code)
            }
        }
    })
}
