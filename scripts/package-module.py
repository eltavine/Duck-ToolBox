#!/usr/bin/env python3
"""Validate and package the module, including the separately licensed engine."""
import argparse
import io
import os
import tempfile
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zipfile
import tarfile

ROOT = Path(__file__).resolve().parents[1]
MODULE = ROOT / "module"


def require(path):
    if not path.is_file() or not path.stat().st_size:
        raise ValueError(f"Missing required payload: {path.relative_to(ROOT)}")


def elf(path, width, machine):
    require(path)
    header = path.read_bytes()[:20]
    if header[:4] != b"\x7fELF" or header[4] != width or header[5] != 1 or struct.unpack_from("<H", header, 18)[0] != machine:
        raise ValueError(f"Incorrect ELF ABI: {path}")


def check_source_checkout(commit, root=ROOT):
    """Only distribute a binary whose GPL engine is available in the pinned Git tree."""
    if not isinstance(commit, str) or len(commit) != 40 or any(c not in "0123456789abcdef" for c in commit):
        raise ValueError("Missing or invalid corresponding-source commit")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if head != commit:
        raise ValueError("Engine source commit differs from the packaging checkout")
    dirty = subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"],
        cwd=root, text=True,
    ).strip()
    if dirty:
        raise ValueError("Corresponding-source checkout contains unpublished changes: " + dirty)
    components = subprocess.check_output(
        ["git", "ls-tree", "-r", commit, "--", "components/play-integrity", "components/device-faker", "components/zygisk-runtime", "components/language-spoof"],
        cwd=root, text=True,
    )
    if any(line.startswith("160000 ") for line in components.splitlines()):
        raise ValueError("Embedded components must contain source files, not Git submodules")


def validate_source_archive(archive, commit, root=ROOT):
    tracked = subprocess.check_output(["git", "archive", "--format=tar", commit], cwd=root)
    with tarfile.open(archive) as delivered, tarfile.open(fileobj=io.BytesIO(tracked)) as expected:
        prefix = "Duck-ToolBox-source/"
        marker = delivered.extractfile(prefix + "SOURCE_COMMIT")
        if marker is None or marker.read().decode().strip() != commit:
            raise ValueError("Corresponding-source archive differs from the binary commit")
        for member in expected:
            if not member.isfile():
                continue
            original = expected.extractfile(member).read()
            actual_member = delivered.getmember(prefix + member.name)
            if not actual_member.isfile():
                raise ValueError("Corresponding-source entry is not a regular file: " + member.name)
            actual = delivered.extractfile(actual_member).read()
            # Cargo vendor adds source replacement tables to the committed config.
            matches = actual.startswith(original + b"\n") if member.name == ".cargo/config.toml" else actual == original
            if not matches or (actual_member.mode & 0o111) != (member.mode & 0o111):
                raise ValueError("Corresponding-source file differs from the build commit: " + member.name)


def write_module_archive(module, archive, metadata=None):
    archive = Path(archive)
    if archive.resolve().is_relative_to(module.resolve()):
        raise ValueError("Archive output must be outside the module source directory")
    archive.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=archive.parent, suffix=".zip", delete=False) as temporary:
        candidate = Path(temporary.name)
    try:
        with zipfile.ZipFile(candidate, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as output:
            for path in sorted(module.rglob("*")):
                if path.is_file():
                    if path == module / "module.prop" and metadata is not None:
                        output.writestr("module.prop", metadata)
                    else:
                        output.write(path, path.relative_to(module))
        with zipfile.ZipFile(candidate) as output:
            if output.testzip() is not None:
                raise ValueError("Module archive CRC validation failed")
        os.replace(candidate, archive)
    finally:
        candidate.unlink(missing_ok=True)


def validate_dependency_licenses(manifest):
    missing = manifest.get("unlicensed_dependencies")
    if not isinstance(missing, list) or any(not isinstance(name, str) or not name for name in missing):
        raise ValueError("Missing Device Faker dependency license metadata; rebuild the engine")
    if missing:
        raise ValueError("Device Faker dependencies have no declared license: " + ", ".join(missing))


def validate_component_notices(root=ROOT, module=MODULE):
    """Copied notices must retain the exact source texts from the build checkout."""
    pairs = [(root / "LICENSE", module / "LICENSE"),
             (root / "THIRD_PARTY_NOTICES.md", module / "THIRD_PARTY_NOTICES.md")]
    for name in ["LICENSE", "NOTICE", "README.md", "RULES.md", "UPSTREAM.json"]:
        pairs.append((root / "components/play-integrity" / name,
                      module / "licenses/play-integrity" / ("GPL-3.0.txt" if name == "LICENSE" else name)))
    for name, source in [("GPL-3.0.txt", "engine/LICENSE"), ("NOTICE", "engine/NOTICES"),
                         ("UPSTREAM_REVISION", "UPSTREAM_REVISION")]:
        pairs.append((root / "components/device-faker" / source, module / "licenses/device-faker" / name))
    for name in ["LICENSE", "NOTICE", "README.md"]:
        pairs.append((root / "components/language-spoof" / name, module / "licenses/language-spoof" / name))
    licenses = root / "components/play-integrity/licenses"
    for source in licenses.rglob("*"):
        if source.is_file():
            pairs.append((source, module / "licenses/play-integrity" / source.relative_to(licenses)))
    for source, delivered in pairs:
        if not delivered.is_file() or delivered.read_bytes() != source.read_bytes():
            raise ValueError("Missing or altered component notice: " + str(delivered))


def validate_device_faker():
    payload = MODULE / "device-faker"
    require(payload / "build.json")
    manifest = json.loads((payload / "build.json").read_text())
    validate_dependency_licenses(manifest)
    source = ROOT / "components/device-faker"
    revision = (source / "UPSTREAM_REVISION").read_text().strip()
    if manifest.get("upstream_revision") != revision:
        raise ValueError("Device Faker source revision differs from recorded provenance")
    check_source_checkout(manifest.get("source_commit"))
    for name in ["device-faker/device-faker-engine"]:
        if name not in manifest["files"]:
            raise ValueError("Missing Device Faker artifact hash: " + name)
        elf(MODULE / name, 2, 183)
    for name, digest in manifest["files"].items():
        path = MODULE / name
        require(path)
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError("Device Faker artifact hash mismatch: " + name)
    for name in ["GPL-3.0.txt", "NOTICE", "UPSTREAM_REVISION", "SOURCE.txt"]:
        require(MODULE / "licenses/device-faker" / name)
    source_url = f"https://github.com/eltavine/Duck-ToolBox/tree/{manifest['source_commit']}/components/device-faker"
    if source_url not in (MODULE / "licenses/device-faker/SOURCE.txt").read_text():
        raise ValueError("Device Faker corresponding-source notice does not match its build commit")
    if not any(source_url.encode() in path.read_bytes() for path in (MODULE / "webroot").rglob("*.js")):
        raise ValueError("WebUI Device Faker corresponding-source commit is missing or mismatched")


def validate_language_contract(manifest, module=MODULE):
    if manifest.get("embedded", {}).get("language_contract") != 1:
        raise ValueError("Root Zygisk payload has an incompatible language contract")
    marker = module / "play-integrity/language-spoof.version"
    if not marker.is_file() or marker.read_bytes() != b"1\n":
        raise ValueError("Root Zygisk payload has an incompatible language contract")


def validate():
    require(MODULE / "webroot/index.html")
    require(MODULE / "play-integrity/build.json")
    validate_component_notices()
    validate_device_faker()
    manifest = json.loads((MODULE / "play-integrity/build.json").read_text())
    upstream = json.loads((ROOT / "components/play-integrity/UPSTREAM.json").read_text())
    if manifest["upstream_revision"] != upstream["revision"]:
        raise ValueError("Engine revision differs from the recorded source")
    for name, digest in manifest["files"].items():
        path = MODULE / name
        require(path)
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError(f"Engine artifact hash mismatch: {name}")
    elf(MODULE / "bin/duckd", 2, 183)
    elf(MODULE / "bin/tini-arm64", 2, 183)
    init = json.loads((ROOT / "components/linux-env/tini.json").read_text())
    if hashlib.sha256((MODULE / "bin/tini-arm64").read_bytes()).hexdigest() != init["sha256"]:
        raise ValueError("Init helper hash differs from its pinned release")
    require(ROOT / init["notice"])
    elf(MODULE / "play-integrity/pif-engine", 2, 183)
    elf(MODULE / "zygisk/arm64-v8a.so", 2, 183)
    elf(MODULE / "zygisk/armeabi-v7a.so", 1, 40)
    dex = MODULE / "play-integrity/classes.dex"
    if not dex.read_bytes().startswith(b"dex\n"):
        raise ValueError("Invalid DEX payload")
    for name in ["GPL-3.0.txt", "NOTICE", "UPSTREAM.json", "zygisk-ISC.txt", "HiddenApiBypass-Apache-2.0.txt", "libcxx-Apache-2.0-with-LLVM-exception.txt"]:
        require(MODULE / "licenses/play-integrity" / name)
    check_source_checkout(manifest.get("source_commit"))
    source_url = (
        "https://github.com/eltavine/Duck-ToolBox/tree/"
        + manifest["source_commit"] + "/components/play-integrity"
    )
    source_notice = MODULE / "licenses/play-integrity/SOURCE.txt"
    require(source_notice)
    if source_url not in source_notice.read_text():
        raise ValueError("Corresponding-source notice does not match the engine commit")
    # The WebUI must link to the same immutable commit embedded in build.json.
    if not any(
        manifest["source_commit"].encode() in path.read_bytes()
        for path in (MODULE / "webroot").rglob("*.js")
    ):
        raise ValueError("WebUI corresponding-source commit is missing or mismatched")
    embedded = manifest.get("embedded")
    if not embedded or embedded.get("api") != 4 or embedded.get("language_contract") != 1 or embedded.get("device_faker_revision") != (ROOT / "components/device-faker/UPSTREAM_REVISION").read_text().strip():
        raise ValueError("Root Zygisk payload does not contain the recorded embedded engine")
    validate_language_contract(manifest)
    if any(path != MODULE / "module.prop" for path in MODULE.rglob("module.prop")):
        raise ValueError("The installable module must not contain a second root-manager module")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/aarch64-linux-android/release/duckd")
    parser.add_argument("--output")
    parser.add_argument("--channel", choices=["stable", "nightly"], help="Set archive metadata without modifying module/module.prop")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    binary = ROOT / args.binary
    require(binary)
    destination = MODULE / "bin/duckd"
    if binary.resolve() != destination.resolve():
        shutil.copy2(binary, destination)
    for name in ["README.md", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(ROOT / name, MODULE / name)
    for binary_path in [MODULE / "bin/duckd", MODULE / "play-integrity/pif-engine", MODULE / "device-faker/device-faker-engine"]:
        if binary_path.exists():
            binary_path.chmod(0o755)
    require(MODULE / "play-integrity/build.json")
    if not args.validate_only:
        subprocess.run([sys.executable, str(ROOT / "scripts/prepare-linux-env.py")], check=True)
    validate()
    if args.validate_only:
        validate_source_archive(MODULE / "webroot/licenses/corresponding-source.tar.gz",
                                json.loads((MODULE / "play-integrity/build.json").read_text())["source_commit"])
        return
    version = next(line.split("=", 1)[1] for line in (MODULE / "module.prop").read_text().splitlines() if line.startswith("version="))
    metadata = (MODULE / "module.prop").read_text()
    name = f"duck-toolbox-{version}"
    if args.channel == "nightly":
        code = subprocess.check_output(["git", "rev-list", "--count", "HEAD"], cwd=ROOT, text=True).strip()
        name += f"-{code}-canary"
        values = {"version": f"{version} ({code}-canary)", "versionCode": code}
        metadata = "\n".join(f"{key}={values[key]}" if (key := line.partition("=")[0]) in values else line for line in metadata.splitlines()) + "\n"
    archive = ROOT / (args.output or f"dist/{name}.zip")
    archive.parent.mkdir(parents=True, exist_ok=True)
    source_archive = MODULE / "webroot/licenses/corresponding-source.tar.gz"
    commit = json.loads((MODULE / "play-integrity/build.json").read_text())["source_commit"]
    if not source_archive.exists():
        subprocess.run([sys.executable, str(ROOT / "scripts/source-bundle.py"),
                        "--commit", commit, "--output", str(source_archive), "--offline"], check=True)
    validate_source_archive(source_archive, commit)
    with source_archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    source_archive.with_suffix(".gz.sha256").write_text(digest + "  " + source_archive.name + "\n")
    write_module_archive(MODULE, archive, metadata if args.channel else None)
    print(archive)


if __name__ == "__main__":
    main()
