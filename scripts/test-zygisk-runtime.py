#!/usr/bin/env python3
"""Exercise v4 composition, channel deadlines and pthread key cleanup at dlclose."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "components/zygisk-runtime"
HEADER = ROOT / "components/play-integrity/android/app/src/main/cpp"
java_home = os.environ.get("JAVA_HOME")
if not java_home:
    if Path("/usr/libexec/java_home").is_file():
        java_home = subprocess.check_output(["/usr/libexec/java_home"], text=True).strip()
    else:
        import shutil
        javac = Path(shutil.which("javac")).resolve()
        java_home = str(javac.parent.parent)
include = Path(java_home) / "include"
platform = "darwin" if (include / "darwin").is_dir() else "linux"
with tempfile.TemporaryDirectory(prefix="duck-runtime-") as directory:
    directory = Path(directory)
    binary = directory / "runtime-test"
    cxx = [os.environ.get("CXX", "c++"), "-std=c++20", "-Wall", "-Wextra", "-Werror", "-pthread"]
    subprocess.run([*cxx,
                    "-DDUCK_HAS_DEVICE_FAKER=1", "-DDUCK_HAS_LANGUAGE_SPOOF=1", "-I", str(SOURCE), "-I", str(HEADER),
                    "-I", str(include), "-I", str(include / platform),
                    str(SOURCE / "tests/runtime.cpp"), str(SOURCE / "entry.cpp"),
                    str(SOURCE / "engine.cpp"), str(SOURCE / "channel.cpp"), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True, timeout=15)
    source = SOURCE / "tests/pthread_keys.cpp"
    fixture = directory / "keys-fixture.so"
    key_test = directory / "keys-test"
    shared = ["-dynamiclib"] if platform == "darwin" else [
        "-shared", "-fPIC", "-Wl,--wrap=pthread_key_create", "-Wl,--wrap=pthread_key_delete",
    ]
    subprocess.run([*cxx, *shared, "-DDUCK_KEY_FIXTURE", "-I", SOURCE,
                    source, SOURCE / "pthread_keys.cpp", "-o", fixture], check=True)
    subprocess.run([*cxx, source, "-ldl", "-o", key_test], check=True)
    subprocess.run([key_test, fixture], check=True, timeout=15)
print("Zygisk v4 composition, channels, fd ownership, bounded waits and pthread key cleanup passed")
