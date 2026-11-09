"""Verify immutable release inputs, verify attestations, then publish all assets together."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zipfile

from payload import ARTIFACT_PREFIX, FULL_ID, LITE_ID, module_properties


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def gh(*arguments):
    return subprocess.check_output(["gh", *map(str, arguments)], text=True, timeout=300)


def read_payload(directory, commit, module_id=FULL_ID):
    record = json.loads((directory / "release.json").read_text())
    if (
        record.get("schema") != 2
        or record.get("source_commit") != commit
        or record.get("module_id") != module_id
        or not re.fullmatch(r"[0-9a-f]{40}", commit)
    ):
        raise ValueError("Release source identity mismatch")
    name = record["name"]
    if not re.fullmatch(r"[0-9]{8}T[0-9]{12}Z", record.get("release_timestamp", "")):
        raise ValueError("Missing immutable release timestamp")
    if not re.fullmatch(rf"{ARTIFACT_PREFIX[module_id]}-[A-Za-z0-9._-]+", name):
        raise ValueError("Unsafe module artifact name")
    expected = {f"{name}.zip", f"{name}.spdx.json"}
    if (
        record["archive"] != f"{name}.zip"
        or record["sbom"] != f"{name}.spdx.json"
        or set(record["sha256"]) != expected
    ):
        raise ValueError("Unexpected release files")
    for filename, checksum in record["sha256"].items():
        path = directory / filename
        if (
            path.is_symlink()
            or not re.fullmatch(r"[0-9a-f]{64}", checksum)
            or digest(path) != checksum
        ):
            raise ValueError(f"Release checksum mismatch: {filename}")
    with zipfile.ZipFile(directory / record["archive"]) as archive:
        metadata = module_properties(archive.read("module.prop").decode())
    if (
        metadata.get("id") != module_id
        or metadata.get("version") != record["version"]
        or metadata.get("versionCode") != record["version_code"]
        or not re.fullmatch(r"[0-9]+", record["version_code"])
        or (module_id == LITE_ID and "updateJson" in metadata)
    ):
        raise ValueError("ZIP module metadata does not match the release record")
    document = json.loads((directory / record["sbom"]).read_text())
    module = next(
        p for p in document["packages"] if p["SPDXID"] == "SPDXRef-Duck-ToolBox-Module"
    )
    if (
        module["name"] != module_id
        or module["versionInfo"] != record["version"]
        or module["packageFileName"] != record["archive"]
    ):
        raise ValueError("SBOM module version or filename mismatch")
    if module["checksums"] != [
        {"algorithm": "SHA256", "checksumValue": record["sha256"][record["archive"]]}
    ]:
        raise ValueError("SBOM does not describe the release archive")
    return record


def read_release(directory, commit):
    full = read_payload(directory / "full", commit, FULL_ID)
    lite = read_payload(directory / "lite", commit, LITE_ID)
    if (full["version"], full["version_code"]) != (lite["version"], lite["version_code"]):
        raise ValueError("Full and Lite versions must match")
    return {"full": full, "lite": lite}


def verify_bundle(archive, bundle, repository, commit, sbom=None):
    arguments = [
        "attestation",
        "verify",
        str(archive),
        "--bundle",
        str(bundle),
        "--repo",
        repository,
        "--signer-workflow",
        f"{repository}/.github/workflows/ci.yml",
        "--source-ref",
        "refs/heads/main",
        "--source-digest",
        commit,
        "--deny-self-hosted-runners",
        "--format",
        "json",
    ]
    if sbom is not None:
        arguments.extend(["--predicate-type", "https://spdx.dev/Document/v2.3"])
    verified = json.loads(gh(*arguments))
    if not verified:
        raise ValueError("No verified attestation")
    if sbom is not None:
        expected = json.loads(sbom.read_text())
        if not any(
            item["verificationResult"]["statement"]["predicate"] == expected
            for item in verified
        ):
            raise ValueError(
                "Verified SBOM predicate differs from the distributed SBOM"
            )


def notes(root, records, repository, commit, channel):
    if channel == "stable":
        body = (root / "CHANGELOG.md").read_text()
    else:
        body = f"Canary module built from commit {commit}.\n\n"
    body += f"\nCI run: https://github.com/{repository}/actions/runs/{os.environ['GITHUB_RUN_ID']}\n"
    for component in ["device-faker", "play-integrity"]:
        body += f"\nGPLv3 {component} corresponding source: https://github.com/{repository}/tree/{commit}/components/{component}\n"
    body += (
        f"\n- Full (default update): `{records['full']['archive']}`\n"
        f"- Lite (manual installation and updates): `{records['lite']['archive']}`\n\n"
        "Lite includes the Tricky Store manager, RKP workbench and device RKP issuer. "
        "It uses a separate module ID. Uninstall the other edition and reboot before switching.\n\n"
        "Each installable ZIP has build-provenance and SPDX 2.3 SBOM attestations. "
        "The SBOM inventories delivered files and pinned source/build inputs; it is not a measured binary dependency graph.\n\n"
        "Verify the downloaded ZIP with GitHub CLI:\n\n```sh\n"
        f"gh attestation verify '{records['full']['archive']}' --repo {repository} "
        f"--signer-workflow {repository}/.github/workflows/ci.yml "
        f"--source-digest {commit} --source-ref refs/heads/main --deny-self-hosted-runners\n```\n\n"
        "For the SBOM attestation, add `--predicate-type https://spdx.dev/Document/v2.3`. "
        "Substitute the Lite filename to verify Lite. The matching Sigstore bundles can also be supplied with `--bundle`.\n"
    )
    return body


def verified_assets(directory, record, provenance, sbom_bundle, repository, commit):
    archive = directory / record["archive"]
    sbom = directory / record["sbom"]
    verify_bundle(archive, provenance, repository, commit)
    verify_bundle(sbom, provenance, repository, commit)
    verify_bundle(archive, sbom_bundle, repository, commit, sbom)
    assets = [archive, sbom]
    for source, suffix in [(provenance, "provenance"), (sbom_bundle, "sbom")]:
        destination = directory / f"{record['name']}.{suffix}.sigstore.json"
        shutil.copyfile(source, destination)
        assets.append(destination)
    return assets


def publish(root, directory, bundles, channel, repository, commit):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid repository")
    records = read_release(directory, commit)
    record = records["full"]
    if channel == "stable":
        if any(r["name"] != f"{ARTIFACT_PREFIX[r['module_id']]}-{r['version']}" for r in records.values()):
            raise ValueError("Canary metadata cannot be published as stable")
        tag = f"v{record['version']}"
        flags = ["--latest"]
    else:
        version = re.fullmatch(
            r"([A-Za-z0-9._-]+) \(([0-9]+)-canary\)", record["version"]
        )
        if (
            not version
            or record["version_code"] != version[2]
            or any(r["name"] != f"{ARTIFACT_PREFIX[r['module_id']]}-{version[1]}-{version[2]}-canary" for r in records.values())
        ):
            raise ValueError("Nightly publication requires matching canary metadata")
        tag = f"nightly-{commit[:8]}-{record['release_timestamp']}"
        flags = ["--prerelease", "--latest=false"]
    assets = []
    for variant, edition in records.items():
        assets.extend(verified_assets(directory / variant, edition, *bundles[variant], repository, commit))
    checksums = directory / "SHA256SUMS"
    checksums.write_text("".join(f"{digest(path)}  {path.name}\n" for path in assets))
    assets.append(checksums)
    with tempfile.TemporaryDirectory(prefix="duck-release-") as temporary:
        body = Path(temporary) / "notes.md"
        body.write_text(notes(root, records, repository, commit, channel))
        # Assets are attached while draft, before immutable-release publication locks them.
        try:
            gh(
                "release",
                "create",
                tag,
                *assets,
                "--repo",
                repository,
                "--target",
                commit,
                "--title",
                tag,
                "--notes-file",
                body,
                "--draft",
                *flags,
            )
        except subprocess.CalledProcessError:
            draft = json.loads(
                gh(
                    "release",
                    "view",
                    tag,
                    "--repo",
                    repository,
                    "--json",
                    "isDraft,targetCommitish,assets",
                )
            )
            allowed = {path.name for path in assets}
            if (
                not draft["isDraft"]
                or draft["targetCommitish"] != commit
                or any(a["name"] not in allowed for a in draft["assets"])
            ):
                raise ValueError(
                    "Refusing to replace a public release or an unrelated draft"
                )
            gh("release", "upload", tag, *assets, "--repo", repository, "--clobber")
            gh(
                "release",
                "edit",
                tag,
                "--repo",
                repository,
                "--title",
                tag,
                "--notes-file",
                body,
            )
    release = json.loads(
        gh("release", "view", tag, "--repo", repository, "--json", "assets")
    )
    uploaded = {a["name"]: (a["size"], a["digest"]) for a in release["assets"]}
    if uploaded != {path.name: (path.stat().st_size, f"sha256:{digest(path)}") for path in assets}:
        raise ValueError("Draft release assets are incomplete or their digests differ")
    matches = [
        asset["url"] for asset in release["assets"] if asset["name"] == record["archive"]
    ]
    if len(matches) != 1:
        raise ValueError("Published release did not return the named module ZIP")
    gh("release", "edit", tag, "--repo", repository, "--draft=false", *flags)
    return matches[0]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=Path("dist"))
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--provenance", type=Path)
    parser.add_argument("--sbom-bundle", type=Path)
    parser.add_argument("--lite-provenance", type=Path)
    parser.add_argument("--lite-sbom-bundle", type=Path)
    parser.add_argument("--channel", choices=["stable", "nightly"])
    arguments = parser.parse_args()
    commit = os.environ["GITHUB_SHA"]
    if arguments.check_only:
        read_release(arguments.directory, commit)
    else:
        if (
            not arguments.provenance
            or not arguments.sbom_bundle
            or not arguments.lite_provenance
            or not arguments.lite_sbom_bundle
            or not arguments.channel
        ):
            parser.error("Publishing requires both editions' attestation bundles and a channel")
        asset = publish(
            Path(__file__).resolve().parents[2],
            arguments.directory,
            {
                "full": (arguments.provenance, arguments.sbom_bundle),
                "lite": (arguments.lite_provenance, arguments.lite_sbom_bundle),
            },
            arguments.channel,
            os.environ["GITHUB_REPOSITORY"],
            commit,
        )
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"asset-url={asset}\n")
