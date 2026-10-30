#!/usr/bin/env python3
"""Exercise module boot hooks and the real engine with isolated Android properties."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
ENGINE = Path(os.environ.get("PIF_ENGINE_TEST_BINARY", ROOT / "target/pif-engine/debug/pif-engine"))
BOOT_HASH = "ab" * 32
INITIAL = {
    "ro.boot.verifiedbootstate": "orange",
    "ro.boot.flash.locked": "0",
    "ro.debuggable": "1",
    "ro.boot.bootmode": "recovery",
    "vendor.boot.bootmode": "recovery",
}
AVB_DEFAULTS = {
    "ro.boot.vbmeta.device_state": "locked",
    "ro.boot.vbmeta.invalidate_on_error": "yes",
    "ro.boot.vbmeta.avb_version": "1.0",
    "ro.boot.vbmeta.hash_alg": "sha256",
    "ro.boot.vbmeta.size": "4096",
}


class PropHandler(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="duck-prop-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.module = self.root / "module"
        self.component = self.module / "play-integrity"
        self.data = self.root / "data"
        self.adb = self.root / "sys/data/adb"
        self.bin = self.adb / "ksu/bin"
        self.bin.mkdir(parents=True)
        self.data.mkdir()
        self.storage = self.root / "sys/storage/emulated/0"
        (self.storage / "Android").mkdir(parents=True)
        shutil.copytree(ROOT / "components/play-integrity/scripts", self.component / "scripts")
        (self.component / "pif-engine").symlink_to(ENGINE.resolve())
        for source in (ROOT / "module").glob("*.sh"):
            shutil.copy2(source, self.module / source.name)
        # Relocate absolute Android paths in temporary script copies only.
        for script in self.module.rglob("*.sh"):
            script.write_text(script.read_text().replace("/data/adb/", str(self.adb) + "/")
                              .replace("/storage/emulated/0/", str(self.storage) + "/"))
        (self.module / "bin").mkdir()
        self.executable(self.module / "bin/duckctl.sh", "#!/bin/sh\nexit 0\n")
        self.state = self.root / "properties.json"
        self.reset()
        # Match the shell features of the root manager's BusyBox on Linux hosts too.
        self.executable(self.bin / "busybox", '#!/bin/sh\nshift\nexec bash "$@"\n')
        self.executable(self.bin / "getprop", "#!/bin/sh\nprintf 1\n")
        self.executable(self.bin / "resetprop", """#!/usr/bin/env python3
import json, os, pathlib, sys
path = pathlib.Path(os.environ['PROP_TEST_STATE'])
values = json.loads(path.read_text())
args = sys.argv[1:]
if not args:
    print('\\n'.join('[' + key + ']: [' + value + ']' for key, value in values.items()))
elif len(args) == 1 and not args[0].startswith('-'):
    print(values.get(args[0], ''))
elif len(args) == 3 and args[0] == '-n':
    values[args[1]] = args[2]
    path.write_text(json.dumps(values))
elif len(args) == 2 and args[0] == '--delete':
    values.pop(args[1], None)
    path.write_text(json.dumps(values))
""")
        self.env = {
            **os.environ,
            "PIF_ENGINE_MODULE": str(self.root / "stale-module"),
            "PIF_ENGINE_DATA": str(self.data),
            "PIF_ENGINE_SYSROOT": str(self.root / "sys"),
            "PROP_TEST_STATE": str(self.state),
            "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
        }

    def executable(self, path, content):
        path.write_text(content)
        path.chmod(0o755)

    def reset(self):
        self.state.write_text(json.dumps(INITIAL))
        (self.adb / "boot_hash").write_text(BOOT_HASH)

    def options(self, **options):
        (self.data / "options.json").write_text(json.dumps(options))

    def run_hook(self, name, *args, success=True):
        result = subprocess.run(["sh", str(self.module / name), *args], env=self.env,
                                text=True, capture_output=True, timeout=20)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def properties(self):
        return json.loads(self.state.read_text())

    def assert_applied(self):
        values = self.properties()
        for key, expected in {
            **AVB_DEFAULTS,
            "ro.boot.verifiedbootstate": "green",
            "ro.boot.flash.locked": "1",
            "ro.debuggable": "0",
            "ro.boot.bootmode": "unknown",
            "vendor.boot.bootmode": "unknown",
            "ro.boot.vbmeta.digest": BOOT_HASH,
        }.items():
            self.assertEqual(values.get(key), expected, key)

    def test_unconfigured_engine_leaves_properties_to_the_original_handler(self):
        self.run_hook("post-fs-data.sh")
        self.assert_early()
        self.run_hook("boot-completed.sh")
        self.assert_applied()

    def test_active_engine_preserves_late_oem_ordering_and_fills_avb_defaults(self):
        for mode in ["full", "scripts-only"]:
            with self.subTest(mode=mode):
                self.reset()
                self.options(mode=mode)
                (self.data / "custom.pif.json").write_text('{"MODEL":"Fixture"}')
                (self.data / "custom.app_replace_list.txt").write_text("")
                self.run_hook("late-load.sh")
                early = self.properties()
                self.assertEqual(early["ro.debuggable"], "0")
                self.assertEqual(early["ro.boot.verifiedbootstate"], "orange")
                self.assertNotIn("ro.boot.vbmeta.device_state", early)
                self.run_hook("boot-completed.sh")
                self.assert_applied()

    def test_existing_avb_metadata_is_preserved(self):
        self.options(mode="scripts-only")
        self.state.write_text(json.dumps({**INITIAL, "ro.boot.vbmeta.size": "8192"}))
        self.run_hook("boot-completed.sh")
        self.assertEqual(self.properties()["ro.boot.vbmeta.size"], "8192")

    def test_sha512_boot_hash_gets_matching_missing_algorithm(self):
        for enabled in [True, False]:
            with self.subTest(enabled=enabled):
                self.reset()
                self.options(mode="scripts-only", enabled=enabled)
                (self.adb / "boot_hash").write_text("CD" * 64)
                self.run_hook("boot-completed.sh")
                self.assertEqual(self.properties()["ro.boot.vbmeta.hash_alg"], "sha512")
                self.assertEqual(self.properties()["ro.boot.vbmeta.digest"], "cd" * 64)

    def test_disabled_conflicting_or_invalid_engine_releases_the_handler(self):
        for scenario in ["disabled", "conflict", "invalid-config", "invalid-options"]:
            with self.subTest(scenario=scenario):
                shutil.rmtree(self.data)
                self.data.mkdir()
                self.reset()
                self.options(enabled=scenario != "disabled")
                (self.data / "custom.pif.json").write_text(
                    "{broken" if scenario == "invalid-config" else '{"MODEL":"Fixture"}')
                if scenario == "invalid-options":
                    (self.data / "options.json").write_text("{broken")
                conflict = self.adb / "modules/playintegrityfix"
                if conflict.exists():
                    shutil.rmtree(conflict)
                if scenario == "conflict":
                    conflict.mkdir(parents=True)
                    (conflict / "module.prop").write_text("id=playintegrityfix\n")
                self.run_hook("post-fs-data.sh")
                self.assert_early()
                self.run_hook("boot-completed.sh")
                self.assert_applied()

    def test_failed_engine_stage_falls_back_in_every_boot_hook(self):
        self.options(mode="scripts-only")
        (self.component / "scripts/stage.sh").write_text("exit 1\n")
        for hook in ["post-fs-data.sh", "late-load.sh", "boot-completed.sh"]:
            with self.subTest(hook=hook):
                self.reset()
                self.run_hook(hook)
                if hook == "boot-completed.sh":
                    self.assert_applied()
                else:
                    self.assert_early()

    def assert_early(self):
        values = self.properties()
        self.assertEqual(values["ro.debuggable"], "0")
        for key in ["ro.boot.verifiedbootstate", "ro.boot.flash.locked",
                    "ro.boot.bootmode", "vendor.boot.bootmode"]:
            self.assertEqual(values[key], INITIAL[key], key)
        self.assertNotIn("ro.boot.vbmeta.digest", values)
        self.assertNotIn("ro.boot.vbmeta.device_state", values)

    def test_inactive_engine_always_defers_oem_properties(self):
        for hook in ["post-fs-data.sh", "late-load.sh"]:
            for scenario in ["disabled", "unconfigured", "conflict", "missing-engine"]:
                with self.subTest(hook=hook, scenario=scenario):
                    self.reset()
                    self.options(enabled=scenario != "disabled")
                    module = self.adb / "modules/playintegrityfix"
                    shutil.rmtree(module, ignore_errors=True)
                    if scenario == "conflict":
                        module.mkdir(parents=True)
                        (module / "module.prop").write_text("id=playintegrityfix\n")
                    if scenario == "missing-engine":
                        (self.component / "pif-engine").unlink(missing_ok=True)
                    self.run_hook(hook)
                    self.assert_early()
                    self.run_hook("boot-completed.sh")
                    self.assert_applied()
                    if scenario == "missing-engine":
                        (self.component / "pif-engine").symlink_to(ENGINE.resolve())

    def test_fallback_still_honors_the_switch_and_applies_the_hash(self):
        self.options(mode="scripts-only")
        (self.adb / "disable_prop_handler").touch()
        for failed in [False, True]:
            with self.subTest(failed=failed):
                if failed:
                    (self.component / "scripts/stage.sh").write_text("exit 1\n")
                self.reset()
                self.run_hook("post-fs-data.sh")
                self.run_hook("boot-completed.sh")
                values = self.properties()
                self.assertEqual({key: values[key] for key in INITIAL}, INITIAL)
                self.assertEqual(values["ro.boot.vbmeta.digest"], BOOT_HASH)
                self.assertNotIn("ro.boot.vbmeta.device_state", values)

    def test_boot_hash_validation_and_comments(self):
        self.options(enabled=False)
        file = self.adb / "boot_hash"
        for invalid in ["abc", "z" * 64]:
            with self.subTest(hash=invalid):
                self.reset()
                file.write_text(invalid)
                result = self.run_hook("prop.sh")
                self.assertIn("ignoring invalid boot hash", result.stderr)
                self.assertNotIn("ro.boot.vbmeta.digest", self.properties())
                self.assertEqual(file.read_text(), invalid)
        file.write_text("  # comment\n" + "AB " * 32 + "\n")
        self.run_hook("prop.sh")
        self.assertEqual(self.properties()["ro.boot.vbmeta.digest"], BOOT_HASH)
        file.write_text("CD" * 64)
        self.run_hook("prop.sh")
        self.assertEqual(self.properties()["ro.boot.vbmeta.digest"], "cd" * 64)
        file.write_text(" # comment only\n")
        self.run_hook("prop.sh")
        self.assertFalse(file.exists())

    def test_earlier_property_failure_is_not_hidden_by_later_success(self):
        script = self.bin / "resetprop"
        original = script.read_text()
        # Reject a write in the middle of the pass, then permit subsequent AVB writes.
        script.write_text(original.replace("args = sys.argv[1:]", "args = sys.argv[1:]\nif args == ['-n', 'ro.debuggable', '0']: sys.exit(7)"))
        result = self.run_hook("prop.sh", "--fallback", success=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.properties()["ro.debuggable"], "1")
        self.assertEqual(self.properties()["ro.boot.vbmeta.size"], "4096")

    def test_missing_resetprop_is_reported(self):
        (self.bin / "resetprop").unlink()
        result = self.run_hook("prop.sh", success=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("resetprop is unavailable", result.stderr)


if __name__ == "__main__":
    if not ENGINE.is_file():
        raise SystemExit("Build pif-engine in target/pif-engine before running these tests")
    unittest.main()
