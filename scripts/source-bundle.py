#!/usr/bin/env python3
"""Accompany local module distributions with reproducible corresponding source."""
import argparse
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def create(commit, destination, offline=False):
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if head != commit:
        raise ValueError("Source bundle must use the binary build commit")
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="toolbox-source-") as directory:
        stage = Path(directory)
        source = stage / "Duck-ToolBox-source"
        source.mkdir()
        archive = subprocess.check_output(["git", "archive", "--format=tar", commit], cwd=ROOT)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tracked:
            tracked.extractall(source, filter="data")
        # Cargo's own vendor operation validates crate checksums and materializes Git sources.
        command = ["cargo", "vendor", "--locked", "--versioned-dirs",
                   "--manifest-path", str(ROOT / "components/device-faker/engine/Cargo.toml"),
                   "--sync", str(ROOT / "components/device-faker/manager/Cargo.toml"),
                   "--sync", str(ROOT / "components/play-integrity/Cargo.toml"),
                   "--sync", str(ROOT / "Cargo.toml")]
        if offline:
            command.append("--offline")
        command.append(str(source / "third-party/rust"))
        config = subprocess.check_output(command, cwd=ROOT, text=True)
        # Avoid absolute paths into the temporary build directory in the delivered config.
        config = config.replace(str(source / "third-party/rust"), "third-party/rust")
        config_path = source / ".cargo/config.toml"
        config_path.write_text(config_path.read_text() + "\n" + config)
        native = ROOT / "target/play-integrity/vendor"
        for name in ["json", "local_cxa_atexit_finalize_impl", "shadowhook", "hiddenapibypass"]:
            path = native / name
            if not (path / ".revision").is_file():
                raise ValueError(f"Pinned native source is missing: {name}")
            shutil.copytree(path, source / "third-party/native" / name,
                            ignore=shutil.ignore_patterns(".git", ".gradle", ".cxx", "build"))
        (source / "SOURCE_COMMIT").write_text(commit + "\n")
        (source / "SOURCE.json").write_text(json.dumps({"commit": commit, "format": 1,
            "build": "python3 scripts/build-play-integrity.py",
            "requirements": "Rust toolchain, JDK 21, SDK 37 / Build Tools 37.0.0, NDK 30.0.16248370"}, indent=2) + "\n")
        with tarfile.open(destination, "w:gz", compresslevel=6) as output:
            output.add(source, arcname=source.name)
    return destination


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    print(create(args.commit, args.output, args.offline))


if __name__ == "__main__":
    main()
