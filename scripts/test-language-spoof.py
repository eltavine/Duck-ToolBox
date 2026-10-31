#!/usr/bin/env python3
"""Exercise language rule isolation, companion framing and Java callback behavior."""
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
COMPONENT = ROOT / "components/language-spoof"
properties = subprocess.run(["java", "-XshowSettings:properties", "-version"], capture_output=True, text=True, check=True)
java_home = Path(re.search(r"java.home = (.+)", properties.stderr)[1].strip())
platform = "darwin" if (java_home / "include/darwin").is_dir() else "linux"
vendor = ROOT / "target/play-integrity/vendor"

with tempfile.TemporaryDirectory(prefix="duck-language-") as directory:
    root = Path(directory)
    binary = root / "native"
    subprocess.run([
        os.environ.get("CXX", "c++"), "-std=c++23", "-Wall", "-Wextra", "-Werror", "-pthread",
        "-I", str(COMPONENT / "native"), "-I", str(vendor),
        "-I", str(java_home / "include"), "-I", str(java_home / "include" / platform),
        f'-DDUCK_LANGUAGE_DATA="{root / "data"}"', f'-DDUCK_LANGUAGE_MODULE="{root / "module"}"',
        str(COMPONENT / "tests/native.cpp"), str(COMPONENT / "native/config.cpp"),
        str(COMPONENT / "native/companion.cpp"), "-o", str(binary),
    ], check=True)
    subprocess.run([str(binary), str(root)], check=True, timeout=10)
    includes = root / "native-stubs"
    (includes / "sys").mkdir(parents=True)
    (includes / "android").mkdir()
    (includes / "sys/system_properties.h").write_text('''#pragma once
#include <cstdint>
#define PROP_VALUE_MAX 92
#define PROP_NAME_MAX 32
struct prop_info { const char *name; const char *value; };
''')
    (includes / "android/log.h").write_text('''#pragma once
#define ANDROID_LOG_WARN 5
int __android_log_print(int, const char *, const char *, ...);
''')
    (includes / "shadowhook.h").write_text('''#pragma once
extern "C" void *shadowhook_hook_sym_addr(void *, void *, void **);
''')
    property_test = root / "properties"
    subprocess.run([
        os.environ.get("CXX", "c++"), "-std=c++23", "-Wall", "-Wextra", "-Werror",
        "-Ddlsym=duck_test_dlsym", "-I", str(includes),
        str(COMPONENT / "tests/properties.cpp"), str(COMPONENT / "native/properties.cpp"),
        "-o", str(property_test),
    ], check=True)
    subprocess.run([str(property_test)], check=True, timeout=10)
    # These fixtures cover callback policy only. ART hook installation requires
    # a real Android runtime; a host JVM cannot establish device compatibility.
    stubs = {
        "android/os/LocaleList.java": '''package android.os;
import java.util.Locale;
public class LocaleList {
    private final Locale[] values;
    public LocaleList(Locale... locales) { values = locales; }
    public Locale get(int index) { return values[index]; }
    public int size() { return values.length; }
    public boolean isEmpty() { return values.length == 0; }
    public static void setDefault(LocaleList locales) {}
}''',
        "android/os/Build.java": '''package android.os;
public class Build { public static class VERSION { public static int SDK_INT = 26; } }''',
        "android/content/res/Configuration.java": '''package android.content.res;
import android.os.LocaleList;
import java.util.Locale;
public class Configuration {
    public int densityDpi = 420;
    private LocaleList locales = new LocaleList(Locale.forLanguageTag("fr-CA"));
    public Configuration() {}
    public Configuration(Configuration source) { densityDpi = source.densityDpi; locales = source.locales; }
    public void setLocales(LocaleList value) { locales = value; }
    public void setLayoutDirection(Locale value) {}
    public LocaleList getLocales() { return locales; }
}''',
        "android/content/res/Resources.java": '''package android.content.res;
public class Resources {
    public static Resources getSystem() { return new Resources(); }
    public Configuration getConfiguration() { return new Configuration(); }
    public Object getDisplayMetrics() { return null; }
    public void updateConfiguration(Configuration value, Object metrics) {}
}''',
        "android/util/Log.java": '''package android.util;
public class Log { public static int w(String tag, String message) { return 0; } }''',
        "org/lsposed/hiddenapibypass/HiddenApiBypass.java": '''package org.lsposed.hiddenapibypass;
public class HiddenApiBypass { public static boolean addHiddenApiExemptions(String... values) { return true; } }''',
    }
    sources = []
    for name, text in stubs.items():
        path = root / "stubs" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        sources.append(path)
    sources += list((COMPONENT / "java").rglob("*.java"))
    sources.append(COMPONENT / "tests/PolicyTest.java")
    subprocess.run(["javac", "-d", str(root / "classes"), *map(str, sources)], check=True)
    subprocess.run(["java", "-ea", "-cp", str(root / "classes"), "dev.duck.toolbox.language.PolicyTest"], check=True, timeout=10)
print("Language target isolation, bounded transport and Java callback policy passed")
