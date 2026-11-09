# Release provenance and software inventory

Canary and Stable releases publish Full and Lite together. Each ZIP has:

- `<module>.spdx.json`: SPDX 2.3 inventory of delivered files, embedded components and declared source/build dependencies.
- `<module>.provenance.sigstore.json`: GitHub build-provenance attestation for the ZIP and SBOM file.
- `<module>.sbom.sigstore.json`: GitHub SBOM attestation whose subject is the same ZIP.

The shared `SHA256SUMS` file records hashes for both ZIPs, both inventories and
all four attestation bundles. `duck-toolbox-*.zip` remains the Full update asset.
`duck-lite-*.zip` is the manual Lite installation and update asset.

The ZIP is finalized and validated before inventory generation or signing. The signing
steps do not rewrite it. The SBOM is a separate attachment, so it does not need to
contain its own hash. Both attestations are verified before release publication.

## Verify a downloaded release

Use a current [GitHub CLI](https://cli.github.com/manual/gh_attestation_verify).
Replace `MODULE.zip` and `SOURCE_COMMIT` with the downloaded filename and the full
source commit from the release notes:

```sh
gh attestation verify MODULE.zip \
  --repo eltavine/Duck-ToolBox \
  --signer-workflow eltavine/Duck-ToolBox/.github/workflows/ci.yml \
  --source-digest SOURCE_COMMIT \
  --source-ref refs/heads/main \
  --deny-self-hosted-runners
```

For the SBOM attestation, use the same command with:

```sh
--predicate-type https://spdx.dev/Document/v2.3
```

GitHub CLI retrieves attestations by artifact digest. The downloaded bundles may
instead be supplied with `--bundle MODULE.provenance.sigstore.json` or
`--bundle MODULE.sbom.sigstore.json`, retaining the same identity and predicate
checks. These signatures establish the claimed source/workflow identity and
artifact digest; they do not establish that the source is free of defects.
The downloaded `.spdx.json` file can also be verified using the first command,
replacing `MODULE.zip` with its filename. Its digest is a subject of the provenance
attestation; the SBOM attestation separately binds its contents to the module ZIP.

## Inventory coverage

For Full, Syft 1.55.0 scans the four application Cargo lockfiles, the WebUI pnpm
lockfile, the Android Gradle lockfile and supporting manifests. For Lite, it scans
the shared workspace Cargo lockfile, WebUI pnpm lockfile and their manifests.
Every locked package must appear with evidence tied to its own lockfile. The
scanner checks each lockfile's SHA-256 against the checkout. All documents in a pnpm
lockfile stream are included, including package-manager dependencies.
Cargo registry archive checksums and exact Git source revisions are retained.
Git dependencies' Package URLs include their VCS source, and Maven group IDs are
included when checking dependency identities.

The Full inventory is supplemented with the embedded Play Integrity, Device Faker,
language and Zygisk components, their upstream source identities, source dependencies
pinned by the native build script, and the precompiled Tini release and checksum.
Unknown dependency licenses remain `NOASSERTION`; existing component notices govern
licensing. All files actually delivered inside the installation ZIP have SHA-256
hashes. SPDX also records its required SHA-1 file hashes and package verification
code; attestation subjects and download checksums use SHA-256.

Lockfile packages are declared **build inputs**, including development, optional
and other-platform dependencies. This inventory does not claim that each package
survives Cargo feature selection, static linking, R8 or frontend tree shaking.
Device-provided vendor libraries and Linux environments downloaded later by the
user are outside the distributed module. It is not a measured binary linkage graph.
The complete document is checked with the SPDX tools validator and must fit
GitHub's 16 MiB SBOM limit.

## CI and publication boundaries

PR workflows build, inventory, validate and upload artifacts with read-only repository
permissions. Only main push/manual runs enter the publishing job, which has
`contents: write`, `id-token: write` and `attestations: write`. Stable's reusable-workflow
caller grants the same scopes. The publishing job uses the existing Telegram
environment. No signing key or additional personal access token is required.

The legacy artifact named `duck-toolbox-<version>-<count>-canary` remains a directory
archive for the existing nightly.link updater. It is repackaged by Actions and is
not the attested ZIP. The companion `*-verified` Actions artifact contains the exact
installation ZIP and SBOM for each edition. Extract the desired ZIP before installing
or verifying it.
PR artifacts have an SBOM but no official release attestations.

The publisher checks both editions' downloaded artifact hashes, versions and source
identity. It verifies each signed bundle against the repository, workflow and main
commit, and compares each verified SBOM predicate with its distributed SBOM. The
draft is published after GitHub's uploaded asset digests match the local files. This
also supports GitHub's optional immutable releases setting. A partial upload may
resume only an unpublished draft for the same source commit and expected assets;
an existing public release is never overwritten. Telegram selects the ZIP by exact
filename rather than release attachment order.
The canary tag timestamp is recorded with the packaged inputs, so retrying the
publication job resumes the same draft instead of inventing another tag.

## Maintain the verification tools

External Actions in both release workflows are pinned to full commits, with updates
reviewed through the existing Dependabot configuration. Syft's pinned version is
shared by the workflow and inventory check. Python validation dependencies are CI
tools only, locked with hashes in `scripts/supply_chain/requirements.txt`.
To update the validator lock from its direct requirements:

```sh
uv pip compile scripts/supply_chain/requirements.in --python-version 3.12 \
  --generate-hashes --no-header --no-annotate \
  --output-file scripts/supply_chain/requirements.txt
```

Run the supply-chain unit tests, actionlint, SPDX validation against a complete
module ZIP, and the source line-limit gate before publishing changes to this flow.
