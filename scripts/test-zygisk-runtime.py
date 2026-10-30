#!/usr/bin/env python3
"""Exercise real v4 ABI composition, ownership, unload and channel deadlines."""
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
    binary = Path(directory) / "runtime-test"
    subprocess.run([os.environ.get("CXX", "c++"), "-std=c++20", "-Wall", "-Wextra", "-Werror", "-pthread",
                    "-DDUCK_HAS_DEVICE_FAKER=1", "-I", str(SOURCE), "-I", str(HEADER),
                    "-I", str(include), "-I", str(include / platform),
                    str(SOURCE / "tests/runtime.cpp"), str(SOURCE / "entry.cpp"),
                    str(SOURCE / "engine.cpp"), str(SOURCE / "channel.cpp"), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True, timeout=15)
print("Zygisk v4 composition, channels, fd ownership and bounded waits passed")
