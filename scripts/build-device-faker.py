#!/usr/bin/env python3
"""Build the pinned Device Faker source and its independent management process."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ENGINE = ROOT / "components/device-faker"
SOURCE = ENGINE / "engine"
TARGET = ROOT / "target/device-faker"


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def check_source():
    if not (SOURCE / "Cargo.toml").is_file():
        raise ValueError("Embedded Device Faker source is missing")
    return (ENGINE / "UPSTREAM_REVISION").read_text().strip()


def prepare_native():
    source = TARGET / "source"
    if source.exists():
        shutil.rmtree(source)
    shutil.copytree(SOURCE, source)
    manifest = source / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace('crate-type = ["cdylib"]', 'crate-type = ["staticlib"]'))
    return source


def notices(payload, source, commit):
    destination = ROOT / "module/licenses/device-faker"
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    shutil.copy2(SOURCE / "LICENSE", destination / "GPL-3.0.txt")
    shutil.copy2(SOURCE / "NOTICES", destination / "NOTICE")
    shutil.copy2(ENGINE / "UPSTREAM_REVISION", destination / "UPSTREAM_REVISION")
    for manifest in [source / "Cargo.toml", ENGINE / "manager/Cargo.toml"]:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--format-version=1", "--manifest-path", str(manifest)], cwd=ROOT,
        ))
        for package in metadata["packages"]:
            parent = Path(package["manifest_path"]).parent
            for path in parent.iterdir():
                if path.is_file() and path.name.upper().startswith(("LICENSE", "NOTICE", "COPYING")):
                    target = destination / "dependencies" / f'{package["name"]}-{package["version"]}' / path.name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, target)
    (destination / "SOURCE.txt").write_text(
        f"Corresponding GPLv3 source and adaptations (Git commit {commit}):\n"
        f"https://github.com/eltavine/Duck-ToolBox/tree/{commit}/components/device-faker\n"
        f"Build script: https://github.com/eltavine/Duck-ToolBox/blob/{commit}/scripts/build-device-faker.py\n"
        "Included source archive: webroot/licenses/corresponding-source.tar.gz\n"
        "All engine source is included in this repository. Run the build script.\n"
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host-only", action="store_true")
    parser.add_argument("--sdk", default=os.environ.get("ANDROID_HOME", str(Path.home() / "Library/Android/sdk")))
    args = parser.parse_args()
    revision = check_source()
    marker = ROOT / "SOURCE_COMMIT"
    commit = marker.read_text().strip() if marker.is_file() else subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    base_env = {**os.environ, "DEVICE_FAKER_BUILD": commit}
    run("cargo", "build", "--locked", "--manifest-path", ENGINE / "manager/Cargo.toml", "--target-dir", ROOT / "target/device-faker-manager", env=base_env)
    if args.host_only:
        return
    ndk = Path(args.sdk) / "ndk/30.0.16248370"
    bins = list((ndk / "toolchains/llvm/prebuilt").glob("*/bin"))
    if not bins:
        raise ValueError("Install pinned Android NDK 30.0.16248370 first")
    toolchain = bins[0]
    suffix = ".cmd" if os.name == "nt" else ""
    linker = toolchain / f"aarch64-linux-android26-clang{suffix}"
    env = {**base_env,
           "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER": str(linker),
           "CC_aarch64_linux_android": str(linker),
           "AR_aarch64_linux_android": str(toolchain / ("llvm-ar.exe" if os.name == "nt" else "llvm-ar"))}
    source = prepare_native()
    for manifest, target in [(source / "Cargo.toml", TARGET / "native"),
                             (ENGINE / "manager/Cargo.toml", ROOT / "target/device-faker-manager")]:
        run("cargo", "build", "--locked", "--release", "--manifest-path", manifest,
            "--target-dir", target, "--target", "aarch64-linux-android", env=env)
    payload = ROOT / "module/device-faker"
    if payload.exists():
        shutil.rmtree(payload)
    payload.mkdir(parents=True)
    static = TARGET / "libdevicefaker.a"
    objcopy = toolchain / ("llvm-objcopy.exe" if os.name == "nt" else "llvm-objcopy")
    run(objcopy, "--redefine-sym", "zygisk_module_entry=duck_faker_module_entry",
        "--redefine-sym", "zygisk_companion_entry=duck_faker_companion",
        TARGET / "native/aarch64-linux-android/release/libzygisk.a", static)
    shutil.copy2(ROOT / "target/device-faker-manager/aarch64-linux-android/release/device-faker-engine", payload / "device-faker-engine")
    (payload / "device-faker-engine").chmod(0o755)
    notices(payload, source, commit)
    files = [p for p in payload.rglob("*") if p.is_file()]
    (payload / "build.json").write_text(json.dumps({
        "upstream_revision": revision, "source_commit": commit,
        "files": {p.relative_to(ROOT / "module").as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in files},
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
