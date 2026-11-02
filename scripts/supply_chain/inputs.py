"""Bounded source inventory passed to Syft; build outputs are never scanned as source."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tomllib
from urllib.parse import quote, unquote

import yaml

SYFT_VERSION = "1.55.0"
LOCKFILES = (
    "Cargo.lock",
    "components/play-integrity/Cargo.lock",
    "components/device-faker/engine/Cargo.lock",
    "components/device-faker/manager/Cargo.lock",
    "ui/pnpm-lock.yaml",
    "components/play-integrity/android/app/gradle.lockfile",
)
MANIFESTS = ("Cargo.toml", "ui/package.json")


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def expected_packages(root):
    expected = {relative: set() for relative in LOCKFILES}
    for relative in LOCKFILES[:4]:
        with (root / relative).open("rb") as stream:
            expected[relative].update(
                (p["name"], p["version"]) for p in tomllib.load(stream)["package"]
            )
    for node in yaml.safe_load_all((root / LOCKFILES[4]).read_text()):
        if str(node["lockfileVersion"]) != "9.0":
            raise ValueError("Unsupported pnpm lockfile schema")
        for key in node["packages"]:
            name, separator, version = key.split("(", 1)[0].rpartition("@")
            if not separator or not name or not version:
                raise ValueError(f"Unrecognized pnpm package: {key}")
            expected[LOCKFILES[4]].add((name, version))
    for line in (root / LOCKFILES[5]).read_text().splitlines():
        if not line or line.startswith("#") or line.startswith("empty="):
            continue
        group, name, version = line.partition("=")[0].split(":")
        expected[LOCKFILES[5]].add((f"{group}:{name}", version))
    return expected


def package_evidence(document):
    files = {
        item["SPDXID"]: item["fileName"].removeprefix("./")
        for item in document.get("files", [])
    }
    packages = {item["SPDXID"]: item for item in document.get("packages", [])}
    for relationship in document.get("relationships", []):
        relative = files.get(relationship["relatedSpdxElement"])
        package = packages.get(relationship["spdxElementId"])
        if (
            relative in LOCKFILES
            and package
            and relationship.get("comment", "").startswith("evident-by:")
        ):
            yield relative, package


def validate_inventory(document, root):
    if document.get("spdxVersion") != "SPDX-2.3":
        raise ValueError("Expected SPDX 2.3 from Syft")
    evidence = {
        item["fileName"].removeprefix("./"): item for item in document.get("files", [])
    }
    for relative in LOCKFILES:
        checksums = evidence.get(relative, {}).get("checksums", [])
        if not any(
            c["algorithm"] == "SHA256" and c["checksumValue"] == digest(root / relative)
            for c in checksums
        ):
            raise ValueError(f"Missing or mismatched lockfile evidence: {relative}")
    found = {relative: set() for relative in LOCKFILES}
    for relative, package in package_evidence(document):
        name = package["name"]
        if relative == LOCKFILES[5]:
            purls = [
                r["referenceLocator"]
                for r in package.get("externalRefs", [])
                if r["referenceType"] == "purl"
                and r["referenceLocator"].startswith("pkg:maven/")
            ]
            if len(purls) != 1:
                continue
            name = unquote(
                purls[0].removeprefix("pkg:maven/").split("@", 1)[0]
            ).replace("/", ":", 1)
        found[relative].add((name, package.get("versionInfo")))
    for relative, expected in expected_packages(root).items():
        missing = expected - found[relative]
        if not expected or missing:
            raise ValueError(f"Incomplete {relative} inventory: {sorted(missing)[:10]}")


def enrich_cargo_sources(document, root):
    locked = {}
    for relative in LOCKFILES[:4]:
        with (root / relative).open("rb") as stream:
            entries = tomllib.load(stream)["package"]
        locked[relative] = {}
        for entry in entries:
            key = (entry["name"], entry["version"])
            if key in locked[relative]:
                raise ValueError(f"Ambiguous Cargo source identity: {relative}: {key}")
            locked[relative][key] = entry
    for relative, package in package_evidence(document):
        if relative not in locked:
            continue
        entry = locked[relative][(package["name"], package["versionInfo"])]
        source = entry.get("source")
        if source:
            package["sourceInfo"] = (
                package.get("sourceInfo", "") + f"; {relative}: {source}"
            )
        if source and source.startswith("git+"):
            repository, revision = source.rsplit("#", 1)
            vcs = repository.split("?", 1)[0] + "@" + revision
            package["downloadLocation"] = vcs
            for reference in package.get("externalRefs", []):
                if reference["referenceType"] == "purl":
                    reference["referenceLocator"] = (
                        reference["referenceLocator"].split("?", 1)[0]
                        + "?vcs_url="
                        + quote(vcs, safe="")
                    )
        if entry.get("checksum"):
            package["checksums"] = [
                {"algorithm": "SHA256", "checksumValue": entry["checksum"]}
            ]


def scan(root, stage, syft, commit):
    version = json.loads(
        subprocess.check_output([syft, "version", "-o", "json"], text=True, timeout=15)
    )
    if version["version"] != SYFT_VERSION:
        raise ValueError(f"Use pinned Syft {SYFT_VERSION}")
    stage.mkdir(parents=True, exist_ok=False)
    for relative in (*LOCKFILES, *MANIFESTS):
        destination = stage / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / relative, destination)
    output = subprocess.check_output(
        [
            syft,
            "scan",
            f"dir:{stage}",
            "--source-name",
            "duck-toolbox-build-inputs",
            "--source-version",
            commit,
            "--quiet",
            "-o",
            "spdx-json",
        ],
        timeout=180,
    )
    document = json.loads(output)
    validate_inventory(document, root)
    enrich_cargo_sources(document, root)
    return document
