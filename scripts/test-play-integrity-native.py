#!/usr/bin/env python3
"""Run native protocol and host JVM text tests without Android or the toolbox runtime."""
from pathlib import Path
import os
import platform
import re
import subprocess
import tempfile

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent
ENGINE = ROOT / "components/play-integrity"
SOURCE = ENGINE / "android/app/src/main/cpp"
VENDOR = ROOT / "third-party/native" if (ROOT / "third-party/native").is_dir() else ROOT / "target/play-integrity/vendor"
TARGET = ROOT / "target/play-integrity/native-tests"
TARGET.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="pif-native-fixture-") as fixture:
    root=Path(fixture)
    subprocess.run([
        os.environ.get("CXX", "c++"), "-std=c++23", "-pthread",
        "-I", str(SOURCE), "-I", str(VENDOR),
        f'-DPIF_TEST_ROOT="{root}"',
        f'-DPIF_DATA_DIR="{root / "data"}"',
        f'-DPIF_MODULE_DIR="{root / "module/play-integrity"}"',
        str(ENGINE / "tests/native.cpp"), str(SOURCE / "config.cpp"),
        str(SOURCE / "companion.cpp"), "-o", str(TARGET),
    ], check=True)
    subprocess.run([str(TARGET)], check=True, timeout=20)
    # JNI uses Modified UTF-8 for NewStringUTF. Test real standard UTF-8 conversion,
    # including supplementary code points, against the JVM used for Java checks.
    properties=subprocess.run(["java","-XshowSettings:properties","-version"],capture_output=True,text=True,check=True)
    java_home=Path(re.search(r"java.home = (.+)",properties.stderr)[1].strip())
    host="darwin" if platform.system()=="Darwin" else "linux"
    library=java_home / "lib/server"
    stub=root / "android/log.h"
    stub.parent.mkdir()
    stub.write_text('#pragma once\n#define ANDROID_LOG_DEBUG 3\n#define ANDROID_LOG_INFO 4\nint __android_log_print(int,const char*,const char*,...);\n')
    prop_stub=root / "sys/system_properties.h"
    prop_stub.parent.mkdir()
    prop_stub.write_text('#pragma once\n#include <cstdint>\nstruct prop_info;\nconst prop_info *__system_property_find(const char *);\nvoid __system_property_read_callback(const prop_info *,void (*)(void *,const char *,const char *,uint32_t),void *);\n')
    subprocess.run(["javac","-d",str(root),str(ENGINE / "tests/android/os/Build.java")],check=True)
    jni_target=root / "jni-tests"
    subprocess.run([
        os.environ.get("CXX","c++"),"-std=c++23",
        "-I",str(SOURCE),"-I",str(VENDOR),"-I",str(root),
        "-I",str(java_home / "include"),"-I",str(java_home / "include" / host),
        str(ENGINE / "tests/jni.cpp"),str(SOURCE / "java_injection.cpp"),str(SOURCE / "observation.cpp"),
        "-L",str(library),"-Wl,-rpath,"+str(library),"-ljvm","-o",str(jni_target),
    ],check=True)
    subprocess.run([str(jni_target),str(root)],check=True,timeout=10)
