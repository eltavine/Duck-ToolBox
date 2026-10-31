#!/usr/bin/env python3
"""Corresponding-source links must identify the exact published Git checkout."""
import importlib.util
import os
import zipfile
from unittest.mock import patch
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import warnings

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

    def test_rejects_an_embedded_component_replaced_by_a_git_submodule(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            git(root, "init", "-q")
            (root / "source.rs").write_text("component source")
            git(root, "add", ".")
            git(root, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid", "commit", "-qm", "Source")
            commit = git(root, "rev-parse", "HEAD")
            git(root, "update-index", "--add", "--cacheinfo", f"160000,{commit},components/play-integrity")
            git(root, "-c", "user.name=CI", "-c", "user.email=ci@example.invalid", "commit", "-qm", "Gitlink fixture")
            # Materialize the empty submodule path so status is clean.
            (root / "components/play-integrity").mkdir(parents=True)
            with self.assertRaisesRegex(ValueError, "not Git submodules"):
                package.check_source_checkout(git(root, "rev-parse", "HEAD"), root)


class DistributionIntegrityTest(unittest.TestCase):
    def test_language_contract_requires_both_metadata_and_the_version_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            module = Path(directory)
            payload = module / "play-integrity"
            payload.mkdir()
            manifest = {"embedded": {"language_contract": 1}}
            marker = payload / "language-spoof.version"
            for value in (None, b"", b"2\n", b"1", b"1\nextra"):
                if value is not None:
                    marker.write_bytes(value)
                with self.subTest(value=value), self.assertRaisesRegex(ValueError, "language contract"):
                    package.validate_language_contract(manifest, module)
            marker.write_bytes(b"1\n")
            package.validate_language_contract(manifest, module)
            for invalid in ({}, {"embedded": {"language_contract": 2}}):
                with self.assertRaisesRegex(ValueError, "language contract"):
                    package.validate_language_contract(invalid, module)

    def test_repackaging_removes_legacy_source_archives_but_preserves_notices(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module = root / "module"
            notices = module / "licenses/play-integrity/SOURCE.txt"
            notices.parent.mkdir(parents=True)
            notices.write_text("https://github.com/eltavine/Duck-ToolBox/tree/" + "a" * 40)
            legacy = ["webroot/licenses/corresponding-source.tar.gz",
                      "webroot/licenses/play-integrity-source.tar.gz",
                      "licenses/play-integrity-source.tar.gz"]
            for relative in legacy:
                for suffix in ["", ".sha256"]:
                    path = module / (relative + suffix)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"obsolete archive")
            package.remove_source_archives(module)
            package.remove_source_archives(module)
            archive = root / "module.zip"
            package.write_module_archive(module, archive)
            with zipfile.ZipFile(archive) as output:
                self.assertEqual(output.namelist(), ["licenses/play-integrity/SOURCE.txt"])
                self.assertEqual(output.read(output.namelist()[0]), notices.read_bytes())

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
            with self.assertWarnsRegex(RuntimeWarning, "unlicensed@1.0"):
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
            with warnings.catch_warnings(record=True) as recorded:
                warnings.simplefilter("always")
                package.validate_dependency_licenses({"unlicensed_dependencies": []})
            self.assertEqual(recorded, [])

    def test_older_or_invalid_build_metadata_cannot_skip_the_check(self):
        for value in [{}, {"unlicensed_dependencies": None},
                      {"unlicensed_dependencies": ""}, {"unlicensed_dependencies": [False]}]:
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "rebuild"):
                package.validate_dependency_licenses(value)


class NoticePreservationTest(unittest.TestCase):
    def test_exact_copies_pass_and_missing_or_truncated_notices_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            module = Path(directory)
            source = package.ROOT
            def copy(original, delivered):
                target = module / delivered
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source / original, target)
            for name in ["LICENSE", "THIRD_PARTY_NOTICES.md"]:
                copy(name, name)
            for name in ["LICENSE", "NOTICE", "README.md", "RULES.md", "UPSTREAM.json"]:
                copy("components/play-integrity/" + name,
                     "licenses/play-integrity/" + ("GPL-3.0.txt" if name == "LICENSE" else name))
            shutil.copytree(source / "components/play-integrity/licenses", module / "licenses/play-integrity", dirs_exist_ok=True)
            for original, name in [("engine/LICENSE", "GPL-3.0.txt"), ("engine/NOTICES", "NOTICE"), ("UPSTREAM_REVISION", "UPSTREAM_REVISION")]:
                copy("components/device-faker/" + original, "licenses/device-faker/" + name)
            for name in ["LICENSE", "NOTICE", "README.md"]:
                copy("components/language-spoof/" + name, "licenses/language-spoof/" + name)
            package.validate_component_notices(module=module)
            for relative in ["LICENSE", "licenses/play-integrity/NOTICE", "licenses/play-integrity/zygisk-ISC.txt", "licenses/device-faker/NOTICE", "licenses/language-spoof/NOTICE", "licenses/language-spoof/LICENSE"]:
                path = module / relative
                original = path.read_bytes()
                for changed in [b"", b"truncated notice"]:
                    path.write_bytes(changed)
                    with self.assertRaisesRegex(ValueError, "altered component notice"):
                        package.validate_component_notices(module=module)
                path.unlink()
                with self.assertRaisesRegex(ValueError, "Missing or altered"):
                    package.validate_component_notices(module=module)
                path.write_bytes(original)


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
