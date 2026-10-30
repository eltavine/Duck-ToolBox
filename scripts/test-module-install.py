#!/usr/bin/env python3
"""Check installer platform/API guards before any runtime preparation."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallGuards(unittest.TestCase):
    def install(self, architecture, api):
        with tempfile.TemporaryDirectory(prefix="duck-install-guard-") as directory:
            root = Path(directory)
            shutil.copy2(ROOT / "module/customize.sh", root / "customize.sh")
            shutil.copy2(ROOT / "module/device-faker.sh", root / "device-faker.sh")
            (root / "util_functions.sh").write_text('''
DATA_ROOT="$TEST_RUNTIME"
repair_runtime() { mkdir -p "$TEST_RUNTIME"; touch "$TEST_RUNTIME/prepared"; }
normalize_boot_hash() { :; }
''')
            runtime = root / "runtime"
            result = subprocess.run(["sh", "-c", '''
ui_print() { printf '%s\\n' "$*"; }
abort() { printf '%s\\n' "$*"; exit 1; }
. "$MODPATH/customize.sh"
'''], capture_output=True, text=True, timeout=10, env={
                **os.environ, "ARCH": architecture, "API": str(api), "KSU": "true",
                "APATCH": "false", "MODPATH": str(root), "TEST_RUNTIME": str(runtime),
            })
            return result, (runtime / "prepared").exists()

    def test_supported_arm64_prepares_runtime(self):
        result, prepared = self.install("arm64", 26)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(prepared)

    def test_incompatible_architectures_fail_before_runtime_changes(self):
        for architecture in ["x64", "x86", "arm", "unknown"]:
            with self.subTest(architecture=architecture):
                result, prepared = self.install(architecture, 26)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("this package requires arm64", result.stdout)
                self.assertFalse(prepared)

    def test_unsupported_android_fails_before_runtime_changes(self):
        result, prepared = self.install("arm64", 25)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("API 26", result.stdout)
        self.assertFalse(prepared)


if __name__ == "__main__":
    unittest.main()
