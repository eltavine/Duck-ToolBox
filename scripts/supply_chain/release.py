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


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def gh(*arguments):
    return subprocess.check_output(["gh", *map(str, arguments)], text=True, timeout=300)


def read_payload(directory, commit):
    record = json.loads((directory / "release.json").read_text())
    if (
        record.get("schema") != 1
        or record.get("source_commit") != commit
        or not re.fullmatch(r"[0-9a-f]{40}", commit)
    ):
        raise ValueError("Release source identity mismatch")
    name = record["name"]
    if not re.fullmatch(r"[0-9]{8}T[0-9]{12}Z", record.get("release_timestamp", "")):
        raise ValueError("Missing immutable release timestamp")
    if not re.fullmatch(r"duck-toolbox-[A-Za-z0-9._-]+", name):
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
    document = json.loads((directory / record["sbom"]).read_text())
    module = next(
        p for p in document["packages"] if p["SPDXID"] == "SPDXRef-Duck-ToolBox-Module"
    )
    if (
        module["versionInfo"] != record["version"]
        or module["packageFileName"] != record["archive"]
    ):
        raise ValueError("SBOM module version or filename mismatch")
    if module["checksums"] != [
        {"algorithm": "SHA256", "checksumValue": record["sha256"][record["archive"]]}
    ]:
        raise ValueError("SBOM does not describe the release archive")
    return record


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


def notes(root, record, repository, commit, channel):
    if channel == "stable":
        body = (root / "CHANGELOG.md").read_text()
    else:
        body = f"Canary module built from commit {commit}.\n\n"
    body += f"\nCI run: https://github.com/{repository}/actions/runs/{os.environ['GITHUB_RUN_ID']}\n"
    for component in ["device-faker", "play-integrity"]:
        body += f"\nGPLv3 {component} corresponding source: https://github.com/{repository}/tree/{commit}/components/{component}\n"
    body += (
        "\nThe installable ZIP has build-provenance and SPDX 2.3 SBOM attestations. "
        "The SBOM inventories delivered files and pinned source/build inputs; it is not a measured binary dependency graph.\n\n"
        "Verify the downloaded ZIP with GitHub CLI:\n\n```sh\n"
        f"gh attestation verify '{record['archive']}' --repo {repository} "
        f"--signer-workflow {repository}/.github/workflows/ci.yml "
        f"--source-digest {commit} --source-ref refs/heads/main --deny-self-hosted-runners\n```\n\n"
        "For the SBOM attestation, add `--predicate-type https://spdx.dev/Document/v2.3`. "
        "The attached Sigstore bundles can also be supplied with `--bundle`.\n"
    )
    return body


def publish(root, directory, provenance, sbom_bundle, channel, repository, commit):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid repository")
    record = read_payload(directory, commit)
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
    checksums = directory / "SHA256SUMS"
    checksums.write_text("".join(f"{digest(path)}  {path.name}\n" for path in assets))
    assets.append(checksums)
    if channel == "stable":
        if record["name"] != f"duck-toolbox-{record['version']}":
            raise ValueError("Canary metadata cannot be published as stable")
        tag = f"v{record['version']}"
        flags = ["--latest"]
    else:
        version = re.fullmatch(
            r"([A-Za-z0-9._-]+) \(([0-9]+)-canary\)", record["version"]
        )
        if (
            not version
            or record["name"] != f"duck-toolbox-{version[1]}-{version[2]}-canary"
        ):
            raise ValueError("Nightly publication requires matching canary metadata")
        tag = f"nightly-{commit[:8]}-{record['release_timestamp']}"
        flags = ["--prerelease", "--latest=false"]
    with tempfile.TemporaryDirectory(prefix="duck-release-") as temporary:
        body = Path(temporary) / "notes.md"
        body.write_text(notes(root, record, repository, commit, channel))
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
    uploaded = {a["name"]: a["size"] for a in release["assets"]}
    if uploaded != {path.name: path.stat().st_size for path in assets}:
        raise ValueError("Draft release assets are incomplete")
    matches = [
        asset["url"] for asset in release["assets"] if asset["name"] == archive.name
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
    parser.add_argument("--channel", choices=["stable", "nightly"])
    arguments = parser.parse_args()
    commit = os.environ["GITHUB_SHA"]
    if arguments.check_only:
        read_payload(arguments.directory, commit)
    else:
        if (
            not arguments.provenance
            or not arguments.sbom_bundle
            or not arguments.channel
        ):
            parser.error("Publishing requires both attestation bundles and a channel")
        asset = publish(
            Path(__file__).resolve().parents[2],
            arguments.directory,
            arguments.provenance,
            arguments.sbom_bundle,
            arguments.channel,
            os.environ["GITHUB_REPOSITORY"],
            commit,
        )
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"asset-url={asset}\n")
