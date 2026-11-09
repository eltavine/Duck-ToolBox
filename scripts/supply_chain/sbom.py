"""Attach delivered files and pinned components to a validated SPDX input inventory."""

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import stat
import tempfile
import uuid
import zipfile

from spdx_tools.spdx.parser.parse_anything import parse_file
from spdx_tools.spdx.validation.document_validator import validate_full_spdx_document
from spdx_tools.spdx.model import File, Checksum, ChecksumAlgorithm
from spdx_tools.spdx.spdx_element_utils import calculate_package_verification_code

from inputs import LITE_LOCKFILES, LOCKFILES, digest, scan
from payload import FULL_ID, LITE_ID, module_properties, validate_lite_payload

ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "https://github.com/eltavine/Duck-ToolBox"
MODULE_ID = "SPDXRef-Duck-ToolBox-Module"
SCOPE = (
    "Delivered ZIP files and explicitly embedded components are inventoried by content. "
    "Lockfile packages are declared build inputs, including development, optional and "
    "other-platform dependencies. They are not an assertion that every package's bytes "
    "remain after feature selection, static linking, R8 or WebUI tree shaking. "
    "Unknown dependency licenses remain NOASSERTION. Device-provided libraries and "
    "user-downloaded Linux environments are not distributed components."
)


def identifier(value):
    return "SPDXRef-Duck-" + hashlib.sha256(value.encode()).hexdigest()[:24]


def package(name, version, location="NOASSERTION", license_id="NOASSERTION", **extra):
    return {
        "SPDXID": identifier(f"package:{name}:{version}:{location}"),
        "name": name,
        "versionInfo": version,
        "downloadLocation": location,
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": license_id,
        "copyrightText": "NOASSERTION",
        **extra,
    }


def relate(document, source, kind, target):
    document["relationships"].append(
        {
            "spdxElementId": source,
            "relationshipType": kind,
            "relatedSpdxElement": target,
        }
    )


def archive_files(archive, variant="full"):
    files = []
    with zipfile.ZipFile(archive) as zipped:
        names = set()
        for entry in zipped.infolist():
            if variant == "lite" and (entry.is_dir() or stat.S_ISLNK(entry.external_attr >> 16)):
                raise ValueError(f"Invalid Lite payload entry: {entry.filename}")
            if entry.is_dir():
                continue
            name = entry.filename
            if (
                name in names
                or name.startswith("/")
                or ".." in PurePosixPath(name).parts
                or "\\" in name
            ):
                raise ValueError(f"Unsafe or duplicate ZIP entry: {name}")
            names.add(name)
            hashes = {
                algorithm: hashlib.new(algorithm.lower())
                for algorithm in ["SHA1", "SHA256"]
            }
            with zipped.open(entry) as stream:
                for chunk in iter(lambda: stream.read(128 * 1024), b""):
                    for hasher in hashes.values():
                        hasher.update(chunk)
            files.append(
                {
                    "SPDXID": identifier(f"payload:{name}"),
                    "fileName": f"./module/{name}",
                    "checksums": [
                        {"algorithm": algorithm, "checksumValue": hasher.hexdigest()}
                        for algorithm, hasher in hashes.items()
                    ],
                    "licenseConcluded": "NOASSERTION",
                    "copyrightText": "NOASSERTION",
                }
            )
        if "module.prop" not in names:
            raise ValueError("Missing module payload: module.prop")
        metadata = module_properties(zipped.read("module.prop").decode())
        if variant == "lite":
            validate_lite_payload(names, zipped.read, metadata)
            return files, metadata
        required = {
            "module.prop",
            "bin/duckd",
            "bin/tini-arm64",
            "webroot/index.html",
            "zygisk/arm64-v8a.so",
            "zygisk/armeabi-v7a.so",
            "play-integrity/pif-engine",
            "play-integrity/classes.dex",
            "play-integrity/build.json",
            "device-faker/device-faker-engine",
            "device-faker/build.json",
        }
        if not required <= names:
            raise ValueError(f"Missing module payloads: {sorted(required - names)}")
        for relative in ["play-integrity/build.json", "device-faker/build.json"]:
            build = json.loads(zipped.read(relative))
            component_commit = build.get("source_commit")
            if not re.fullmatch(r"[0-9a-f]{40}", component_commit or ""):
                raise ValueError(f"Missing component source identity: {relative}")
            metadata[relative] = component_commit
    return files, metadata


def embedded_components(root, commit, variant="full"):
    components = [
        package("duckd", commit, f"{REPOSITORY}/tree/{commit}/crates", "MIT"),
        package("webui", commit, f"{REPOSITORY}/tree/{commit}/ui", "MIT"),
    ]
    if variant == "lite":
        return components
    components.extend([
        package(
            "zygisk-runtime",
            commit,
            f"{REPOSITORY}/tree/{commit}/components/zygisk-runtime",
            "GPL-3.0-only",
        ),
        package(
            "language-spoof",
            commit,
            f"{REPOSITORY}/tree/{commit}/components/language-spoof",
            "GPL-3.0-only",
        ),
    ])
    for name in ["play-integrity", "device-faker"]:
        upstream = json.loads((root / f"components/{name}/UPSTREAM.json").read_text())
        components.append(
            package(
                name,
                commit,
                f"{REPOSITORY}/tree/{commit}/components/{name}",
                upstream["license"],
                sourceInfo=f"Adapted source; upstream {upstream['repository']} at {upstream['revision']}",
            )
        )
    spec = importlib.util.spec_from_file_location(
        "pif_build", root / "scripts/build-play-integrity.py"
    )
    build = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(build)
    for name, (repository, revision) in build.DEPENDENCIES.items():
        components.append(
            package(
                name,
                revision,
                f"https://github.com/{repository}/tree/{revision}",
                primaryPackagePurpose="SOURCE",
                sourceInfo="Pinned build input fetched by scripts/build-play-integrity.py; bundled notices govern licensing.",
            )
        )
    tini = json.loads((root / "components/linux-env/tini.json").read_text())
    components.append(
        package(
            "tini-static-arm64",
            tini["version"],
            tini["asset"],
            tini["license"],
            checksums=[{"algorithm": "SHA256", "checksumValue": tini["sha256"]}],
        )
    )
    return components


def assemble(document, root, archive, commit, variant="full"):
    files, metadata = archive_files(archive, variant)
    if variant == "full" and (metadata.get("id") != FULL_ID or any(
        metadata[path] != commit
        for path in ["play-integrity/build.json", "device-faker/build.json"]
    )):
        raise ValueError("Archive component source does not match the SBOM checkout")
    checksum = digest(archive)
    document["name"] = archive.name
    document["documentNamespace"] = f"{REPOSITORY}/sbom/{checksum}/{uuid.uuid4()}"
    document["documentComment"] = SCOPE
    document["relationships"] = [
        r for r in document["relationships"] if r["relationshipType"] != "DESCRIBES"
    ]
    module = package(
        metadata["id"],
        metadata["version"],
        checksums=[{"algorithm": "SHA256", "checksumValue": checksum}],
        primaryPackagePurpose="ARCHIVE",
        packageFileName=archive.name,
    )
    module["SPDXID"] = MODULE_ID
    module["filesAnalyzed"] = True
    verification = calculate_package_verification_code(
        [
            File(
                item["fileName"],
                item["SPDXID"],
                checksums=[
                    Checksum(
                        ChecksumAlgorithm.SHA1, item["checksums"][0]["checksumValue"]
                    )
                ],
            )
            for item in files
        ]
    )
    module["packageVerificationCode"] = {
        "packageVerificationCodeValue": verification.value
    }
    for item in document["packages"]:
        item["comment"] = (
            "Declared source/build inventory; inclusion does not establish presence in the final binaries."
        )
        relate(document, item["SPDXID"], "BUILD_DEPENDENCY_OF", MODULE_ID)
    components = embedded_components(root, commit, variant)
    document["packages"].extend([module, *components])
    relate(document, "SPDXRef-DOCUMENT", "DESCRIBES", MODULE_ID)
    for item in components:
        if item.get("primaryPackagePurpose") == "SOURCE":
            relate(document, item["SPDXID"], "BUILD_DEPENDENCY_OF", MODULE_ID)
        else:
            relate(document, MODULE_ID, "CONTAINS", item["SPDXID"])
    for item in files:
        relate(document, MODULE_ID, "CONTAINS", item["SPDXID"])
    document.setdefault("files", []).extend(files)
    if variant == "full":
        tini = next(item for item in components if item["name"] == "tini-static-arm64")
        delivered_tini = next(
            (item for item in files if item["fileName"] == "./module/bin/tini-arm64"), None
        )
        if not delivered_tini or tini["checksums"][0] not in delivered_tini["checksums"]:
            raise ValueError("Delivered Tini does not match its pinned hash")
    return metadata


def validate(path):
    if path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("SBOM exceeds GitHub's 16 MiB attestation limit")
    errors = validate_full_spdx_document(parse_file(str(path)))
    if errors:
        raise ValueError(
            "Invalid SPDX document: "
            + "; ".join(error.validation_message[:400] for error in errors[:3])
        )


def generate(root, archive, syft, commit, variant="full"):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("An immutable source commit is required")
    with tempfile.TemporaryDirectory(prefix="duck-sbom-") as directory:
        lockfiles = LITE_LOCKFILES if variant == "lite" else LOCKFILES
        document = scan(root, Path(directory) / "inputs", syft, commit, lockfiles)
    metadata = assemble(document, root, archive, commit, variant)
    output = archive.with_suffix(".spdx.json")
    output.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    validate(output)
    record = {
        "schema": 2,
        "release_timestamp": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ"),
        "source_commit": commit,
        "module_id": LITE_ID if variant == "lite" else FULL_ID,
        "archive": archive.name,
        "sbom": output.name,
        "version": metadata["version"],
        "version_code": metadata["versionCode"],
        "name": archive.stem,
        "sha256": {archive.name: digest(archive), output.name: digest(output)},
    }
    (archive.parent / "release.json").write_text(json.dumps(record, indent=2) + "\n")
    (archive.parent / "SHA256SUMS").write_text(
        "".join(f"{value}  {name}\n" for name, value in record["sha256"].items())
    )
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--syft", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--variant", choices=["full", "lite"], default="full")
    arguments = parser.parse_args()
    generate(ROOT, arguments.archive, arguments.syft, arguments.commit, arguments.variant)
