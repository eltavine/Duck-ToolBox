#!/usr/bin/env python3
"""Corresponding-source links must identify the exact published Git checkout."""
import importlib.util
import io
import tarfile
import copy
import os
import zipfile
from unittest.mock import patch
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_module", Path(__file__).with_name("package-module.py"))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
build_spec = importlib.util.spec_from_file_location("build_device_faker", Path(__file__).with_name("build-device-faker.py"))
build_faker = importlib.util.module_from_spec(build_spec)
build_spec.loader.exec_module(build_faker)


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


class DistributionIntegrityTest(unittest.TestCase):
    def test_rejects_source_changes_even_with_a_matching_commit_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git(root, "init", "-q")
            (root / "source.rs").write_text("committed source")
            git(root, "add", ".")
            git(root, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid", "commit", "-qm", "Source")
            commit = git(root, "rev-parse", "HEAD")
            raw = subprocess.check_output(["git", "archive", commit], cwd=root)
            archive = root / "source.tar.gz"
            for corrupt in [False, True]:
                with tarfile.open(archive, "w:gz") as output, tarfile.open(fileobj=io.BytesIO(raw)) as tracked:
                    for member in tracked:
                        entry = copy.copy(member)
                        entry.name = "Duck-ToolBox-source/" + member.name
                        data = b"altered source" if corrupt else tracked.extractfile(member).read()
                        entry.size = len(data)
                        output.addfile(entry, io.BytesIO(data))
                    marker = tarfile.TarInfo("Duck-ToolBox-source/SOURCE_COMMIT")
                    marker.size = len(commit)
                    output.addfile(marker, io.BytesIO(commit.encode()))
                if corrupt:
                    with self.assertRaisesRegex(ValueError, "file differs"):
                        package.validate_source_archive(archive, commit, root)
                else:
                    package.validate_source_archive(archive, commit, root)

    def test_failed_packaging_preserves_the_previous_complete_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module = root / "module"
            module.mkdir()
            source = module / "module.prop"
            source.write_text("version=1")
            os.utime(source, (1, 1))
            archive = root / "module.zip"
            package.write_module_archive(module, archive)
            before = archive.read_bytes()
            with zipfile.ZipFile(archive) as output:
                self.assertEqual(output.getinfo("module.prop").date_time[0], 1980)
            source.write_text("version=2")
            with patch.object(zipfile.ZipFile, "write", side_effect=OSError("disk full")):
                with self.assertRaisesRegex(OSError, "disk full"):
                    package.write_module_archive(module, archive)
            self.assertEqual(archive.read_bytes(), before)
            self.assertEqual(list(root.glob("*.zip")), [archive])


class DependencyLicenseTest(unittest.TestCase):
    def test_root_license_does_not_supply_dependency_permission(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "LICENSE").write_text("Root project MIT license")
            crate = root / "dependency"
            crate.mkdir()
            dependency = {"name": "unlicensed", "version": "1.0", "license": None,
                          "license_file": None, "manifest_path": str(crate / "Cargo.toml")}
            missing = build_faker.unlicensed_dependencies({"packages": [dependency]})
            self.assertEqual(missing, ["unlicensed@1.0"])
            with self.assertRaisesRegex(ValueError, "unlicensed@1.0"):
                package.validate_dependency_licenses({"unlicensed_dependencies": missing})

    def test_declared_license_file_must_exist_and_contain_text(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            crate = root / "crate"
            crate.mkdir()
            dependency = {"name": "custom-license", "version": "1.0", "license": None,
                          "license_file": "../COPYING", "manifest_path": str(crate / "Cargo.toml")}
            metadata = {"packages": [dependency]}
            for content in [None, " \n", "Redistribution permission and original copyright"]:
                if content is not None:
                    (root / "COPYING").write_text(content)
                missing = build_faker.unlicensed_dependencies(metadata)
                self.assertEqual(missing, [] if content and content.strip() else ["custom-license@1.0"])
            dependency["license_file"] = None
            dependency["license"] = "MIT OR Apache-2.0"
            self.assertEqual(build_faker.unlicensed_dependencies(metadata), [])
            package.validate_dependency_licenses({"unlicensed_dependencies": []})

    def test_older_or_invalid_build_metadata_cannot_skip_the_check(self):
        for value in [{}, {"unlicensed_dependencies": None},
                      {"unlicensed_dependencies": ""}, {"unlicensed_dependencies": [False]}]:
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "rebuild"):
                package.validate_dependency_licenses(value)


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
