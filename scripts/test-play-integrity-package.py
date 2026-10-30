#!/usr/bin/env python3
"""Corresponding-source links must identify the exact published Git checkout."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_module", Path(__file__).with_name("package-module.py"))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


class SourceCheckoutTest(unittest.TestCase):
    def test_pins_clean_source_commit_and_rejects_local_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git(root, "init", "-q")
            engine = root / "components/play-integrity"
            scripts = root / "scripts"
            engine.mkdir(parents=True)
            scripts.mkdir()
            faker = root / "components/device-faker/manager"
            faker.mkdir(parents=True)
            (faker / "main.rs").write_text("faker source")
            source = engine / "main.rs"
            source.write_text("original")
            (scripts / "build-play-integrity.py").write_text("build")
            git(root, "add", ".")
            git(root, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid",
                "commit", "-qm", "Initial source")
            commit = git(root, "rev-parse", "HEAD")
            package.check_source_checkout(commit, root)
            with self.assertRaisesRegex(ValueError, "commit differs"):
                package.check_source_checkout("0" * 40, root)
            source.write_text("changed after source commit")
            with self.assertRaisesRegex(ValueError, "unpublished"):
                package.check_source_checkout(commit, root)
            source.write_text("original")
            (scripts / "new-build-step.py").write_text("untracked")
            with self.assertRaisesRegex(ValueError, "unpublished"):
                package.check_source_checkout(commit, root)
            (scripts / "new-build-step.py").unlink()
            package.check_source_checkout(commit, root)
            (faker / "main.rs").write_text("unpublished faker source")
            with self.assertRaisesRegex(ValueError, "unpublished"):
                package.check_source_checkout(commit, root)

    def test_rejects_invalid_commit_identifiers(self):
        for value in (None, "", "main", "a" * 39, "z" * 40):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "invalid"):
                package.check_source_checkout(value)


class ShellEntrypointTest(unittest.TestCase):
    def fixture(self, root):
        scripts = root / "scripts"
        scripts.mkdir()
        (root / "module/bin").mkdir(parents=True)
        (root / "module/webroot").mkdir()
        (root / "module/module.prop").write_text("version=1.2.3\n")
        (root / "module/webroot/index.html").write_text("ui")
        for name in ("README.md", "CHANGELOG.md", "THIRD_PARTY_NOTICES.md"):
            (root / name).write_text(name)
        wrapper = scripts / "package-module.sh"
        shutil.copy2(Path(__file__).with_name(wrapper.name), wrapper)
        return wrapper

    def test_forwards_absolute_paths_with_spaces_to_shared_packager(self):
        with tempfile.TemporaryDirectory(prefix="module package ") as directory:
            root = Path(directory)
            wrapper = self.fixture(root)
            (root / "scripts/package-module.py").write_text("import json, sys; print(json.dumps(sys.argv[1:]))")
            binary = str(root / "backend binary")
            output = str(root / "release output")
            result = subprocess.check_output(["bash", str(wrapper), binary, output], text=True)
            self.assertEqual(json.loads(result), ["--binary", binary, "--output", f"{output}/duck-toolbox-1.2.3.zip"])

    def test_rejects_missing_engine_instead_of_emitting_an_incomplete_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wrapper = self.fixture(root)
            shutil.copy2(Path(__file__).with_name("package-module.py"), root / "scripts/package-module.py")
            binary = root / "backend"
            binary.write_bytes(b"not an Android ELF")
            result = subprocess.run(["bash", str(wrapper), str(binary)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("play-integrity/build.json", result.stderr)
            self.assertEqual(list(root.glob("dist/*.zip")), [])


if __name__ == "__main__":
    unittest.main()
