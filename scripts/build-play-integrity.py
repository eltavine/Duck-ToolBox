#!/usr/bin/env python3
"""Build the embedded Play Integrity component and composite module payload."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import subprocess
import tarfile
import urllib.request

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent
ENGINE = ROOT / "components/play-integrity"
VENDOR = ROOT / "third-party/native" if (ROOT / "third-party/native").is_dir() else ROOT / "target/play-integrity/vendor"
DEPENDENCIES = {
    "json": ("nlohmann/json", "55f93686c01528224f448c19128836e7df245f72"),
    "local_cxa_atexit_finalize_impl": (
        "5ec1cff/local_cxa_atexit_finalize_impl", "b269136606d6f230089fbf47a0b219f7f2ccef3c"
    ),
    "shadowhook": ("bytedance/android-inline-hook", "854c775c2c3676e57a0f383597ebf420b5204161"),
    "hiddenapibypass": ("LSPosed/AndroidHiddenApiBypass", "4efc64fcada7df15e7b5ba068e774343b760270d"),
    "lsplant": ("LSPosed/LSPlant", "6ac2280d8d88e14f0864c26ac97c1dc96713bde1"),
    "dex_builder": ("LSPosed/DexBuilder", "ac7fb2230954ee311808bad469b0db501f31bfb8"),
    "parallel_hashmap": ("greg7mdp/parallel-hashmap", "0cd57d29a959256ed66b2afdd1009928fc625d09"),
}


def run(*args, **kwargs):
    subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def dependencies():
    VENDOR.mkdir(parents=True, exist_ok=True)
    for name, (repo, commit) in DEPENDENCIES.items():
        destination = VENDOR / name
        if (destination / ".revision").exists() and (destination / ".revision").read_text() == commit:
            continue
        archive = VENDOR / f"{name}-{commit}.tar.gz"
        if not archive.exists():
            with urllib.request.urlopen(f"https://codeload.github.com/{repo}/tar.gz/{commit}", timeout=60) as response:
                archive.write_bytes(response.read())
        if destination.exists():
            shutil.rmtree(destination)
        destination.mkdir()
        with tarfile.open(archive) as source:
            for member in source.getmembers():
                parts = Path(member.name).parts[1:]
                if not parts:
                    continue
                member.name = "/".join(parts)
                source.extract(member, destination, filter="data")
        (destination / ".revision").write_text(commit)
    # Materialize the pinned LSPlant submodules without depending on a Git checkout.
    for source, destination in [
        ("dex_builder", "lsplant/lsplant/src/main/jni/external/dex_builder"),
        ("parallel_hashmap", "lsplant/lsplant/src/main/jni/external/dex_builder/external/parallel_hashmap"),
    ]:
        target = VENDOR / destination
        if not (target / ".revision").exists() or (target / ".revision").read_text() != (VENDOR / source / ".revision").read_text():
            if target.exists():
                shutil.rmtree(target)
            shutil.copytree(VENDOR / source, target)
    # PIF deliberately disables shadowhook's linker initialization. Keep the patch
    # reproducible from this build script and its pinned upstream revision.
    linker = VENDOR / "shadowhook/shadowhook/src/main/cpp/sh_linker.c"
    value = linker.read_text()
    if "sh_linker_init_dummy" not in value:
        linker.write_text(value.replace("sh_linker_init", "sh_linker_init_dummy"))
    symbols = VENDOR / "shadowhook/shadowhook/src/main/cpp/third_party/xdl/xdl.c"
    value = symbols.read_text()
    if '#include "xdl-prefix.inc"' not in value:
        symbols.write_text(value + '\n// Modified 2026-10-31: expose ART prefix lookup.\n#include "xdl-prefix.inc"\n')


def source_revision():
    """The exact Git revision used for this engine build, not a moving branch."""
    marker = ROOT / "SOURCE_COMMIT"
    if marker.is_file():
        return marker.read_text().strip()
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host-only", action="store_true")
    parser.add_argument("--dependencies-only", action="store_true")
    parser.add_argument("--sdk", default=os.environ.get("ANDROID_HOME", str(Path.home() / "Library/Android/sdk")))
    args = parser.parse_args()
    dependencies()
    if args.dependencies_only:
        return
    env = {**os.environ, "PIF_VENDOR_DIR": str(VENDOR), "ANDROID_HOME": args.sdk}
    run("cargo", "build", "--locked", "--manifest-path", ENGINE / "Cargo.toml", "--target-dir", ROOT / "target/pif-engine", env=env)
    if args.host_only:
        return
    ndk = Path(args.sdk) / "ndk/30.0.16248370"
    if not ndk.is_dir():
        raise SystemExit(f"Install the pinned NDK first: sdkmanager 'ndk;{ndk.name}'")
    toolchains = list((ndk / "toolchains/llvm/prebuilt").glob("*/bin"))
    if not toolchains:
        raise SystemExit("NDK host toolchain missing")
    toolchain = toolchains[0]
    target = "aarch64-linux-android"
    linker = toolchain / (f"{target}26-clang.cmd" if os.name == "nt" else f"{target}26-clang")
    env.update({
        "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER": str(linker),
        "CC_aarch64_linux_android": str(linker),
        "AR_aarch64_linux_android": str(toolchain / ("llvm-ar.exe" if os.name == "nt" else "llvm-ar")),
    })
    run("cargo", "build", "--locked", "--release", "--manifest-path", ENGINE / "Cargo.toml", "--target-dir", ROOT / "target/pif-engine", "--target", target, env=env)
    run(sys.executable, ROOT / "scripts/build-device-faker.py", "--sdk", args.sdk)
    env.update({"DUCK_RUNTIME_DIR": str(ROOT / "components/zygisk-runtime"),
                "DUCK_LANGUAGE_DIR": str(ROOT / "components/language-spoof"),
                "DUCK_FAKER_LIBRARY": str(ROOT / "target/device-faker/libdevicefaker.a"),
                "DUCK_FAKER_JAVA": str(ROOT / "components/device-faker/java")})
    wrapper = ENGINE / "android/gradlew"
    gradle_command = [wrapper.with_suffix(".bat")] if os.name == "nt" else ["sh", wrapper]
    run(*gradle_command, "--no-daemon", "assembleRelease", cwd=wrapper.parent, env=env)
    payload = ROOT / "module/play-integrity"
    if payload.exists():
        shutil.rmtree(payload)
    payload.mkdir(parents=True, exist_ok=True)
    shutil.copytree(ENGINE / "scripts", payload / "scripts", dirs_exist_ok=True)
    shutil.copy2(ROOT / f"target/pif-engine/{target}/release/pif-engine", payload / "pif-engine")
    dex = ENGINE / "android/app/build/intermediates/dex/release/minifyReleaseWithR8/classes.dex"
    shutil.copy2(dex, payload / "classes.dex")
    (payload / "language-spoof.version").write_text("1\n")
    libraries = ENGINE / "android/app/build/intermediates/stripped_native_libs/release/stripReleaseDebugSymbols/out/lib"
    zygisk = ROOT / "module/zygisk"
    if zygisk.exists():
        shutil.rmtree(zygisk)
    zygisk.mkdir(exist_ok=True)
    for abi in ["arm64-v8a", "armeabi-v7a"]:
        shutil.copy2(libraries / abi / "libplayintegrityfix.so", zygisk / f"{abi}.so")
    notices = ROOT / "module/licenses/play-integrity"
    if notices.exists():
        shutil.rmtree(notices)
    notices.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ENGINE / "LICENSE", notices / "GPL-3.0.txt")
    for name in ["README.md", "RULES.md", "NOTICE", "UPSTREAM.json"]:
        shutil.copy2(ENGINE / name, notices / name)
    for name in DEPENDENCIES:
        for path in (VENDOR / name).rglob("*"):
            if path.is_file() and ("LICENSE" in path.name.upper() or path.name.upper()=="NOTICE"):
                destination=notices / name / path.relative_to(VENDOR / name)
                destination.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(path,destination)
    language_notices = ROOT / "module/licenses/language-spoof"
    language_notices.mkdir(parents=True, exist_ok=True)
    for name in ["LICENSE", "NOTICE", "README.md"]:
        shutil.copy2(ROOT / "components/language-spoof" / name, language_notices / name)
    # Maven dependencies' license texts are kept with the source component.
    shutil.copytree(ENGINE / "licenses", notices, dirs_exist_ok=True)
    # Packaging includes the exact committed source and locked dependencies.
    commit = source_revision()
    if commit:
        source_url = f"https://github.com/eltavine/Duck-ToolBox/tree/{commit}/components/play-integrity"
        (notices / "SOURCE.txt").write_text(
            f"Corresponding Play Integrity GPLv3 source (Git commit {commit}):\n"
            f"{source_url}\n"
            "Build and dependency retrieval script:\n"
            f"https://github.com/eltavine/Duck-ToolBox/blob/{commit}/scripts/build-play-integrity.py\n"
            "Included source archive: webroot/licenses/corresponding-source.tar.gz\n"
            "This script retrieves pinned dependency sources and applies the recorded ShadowHook change.\n"
        )
    (notices.parent / "play-integrity-source.tar.gz").unlink(missing_ok=True)
    (ROOT / "module/webroot/licenses/play-integrity-source.tar.gz").unlink(missing_ok=True)
    artifacts = [*(p for p in payload.rglob("*") if p.is_file()), *zygisk.glob("*.so")]
    (payload / "build.json").write_text(json.dumps({
        "upstream_revision": json.loads((ENGINE / "UPSTREAM.json").read_text())["revision"],
        "source_commit": commit,
        "embedded": {"api": 4, "language_contract": 1, "device_faker_revision": (ROOT / "components/device-faker/UPSTREAM_REVISION").read_text().strip()},
        "files": {p.relative_to(ROOT / "module").as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in artifacts},
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
