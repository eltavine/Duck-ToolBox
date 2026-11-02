import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from fixtures import COMMIT, archive, inventory, source
from inputs import (
    LOCKFILES,
    expected_packages,
    validate_inventory,
    enrich_cargo_sources,
)
import sbom


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = source(Path(self.temporary.name))

    def test_pnpm_stream_and_source_evidence_are_complete(self):
        packages = expected_packages(self.root)
        self.assertEqual(
            packages[LOCKFILES[4]], {("pnpm", "12.16.1"), ("@scope/app", "1.0.0")}
        )
        validate_inventory(inventory(self.root), self.root)

    def test_missing_dependency_is_rejected_for_each_lockfile(self):
        for index, relative in enumerate(LOCKFILES):
            document = inventory(self.root)
            document["relationships"] = [
                r
                for r in document["relationships"]
                if r["relatedSpdxElement"] != f"SPDXRef-Input-{index}"
            ]
            with (
                self.subTest(relative=relative),
                self.assertRaisesRegex(ValueError, "Incomplete"),
            ):
                validate_inventory(document, self.root)

    def test_missing_or_changed_evidence_cannot_pass_as_a_complete_inventory(self):
        document = inventory(self.root)
        document["files"][0]["checksums"][1]["checksumValue"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "evidence"):
            validate_inventory(document, self.root)

    def test_unknown_lockfile_schema_is_rejected(self):
        path = self.root / LOCKFILES[4]
        path.write_text(path.read_text().replace("'9.0'", "'10.0'"))
        with self.assertRaisesRegex(ValueError, "schema"):
            expected_packages(self.root)

    def test_maven_group_is_part_of_the_dependency_identity(self):
        document = inventory(self.root)
        maven = next(p for p in document["packages"] if p["name"] == "java-lib")
        maven["externalRefs"][0]["referenceLocator"] = "pkg:maven/other/java-lib@2.0.0"
        with self.assertRaisesRegex(ValueError, "Incomplete"):
            validate_inventory(document, self.root)

    def test_cargo_git_revisions_and_registry_hashes_are_preserved(self):
        path = self.root / LOCKFILES[0]
        path.write_text(
            path.read_text()
            + 'source="git+https://github.com/example/lib?rev=main#'
            + "d" * 40
            + '"\n'
        )
        registry = self.root / LOCKFILES[1]
        registry.write_text(
            registry.read_text()
            + 'source="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="'
            + "e" * 64
            + '"\n'
        )
        document = inventory(self.root)
        enrich_cargo_sources(document, self.root)
        git = next(p for p in document["packages"] if p["name"] == "dep-0")
        self.assertEqual(
            git["downloadLocation"], "git+https://github.com/example/lib@" + "d" * 40
        )
        self.assertIn("#" + "d" * 40, git["sourceInfo"])
        registry_package = next(p for p in document["packages"] if p["name"] == "dep-1")
        self.assertEqual(
            registry_package["checksums"],
            [{"algorithm": "SHA256", "checksumValue": "e" * 64}],
        )

    def test_complete_sbom_validates_and_binds_the_delivered_zip(self):
        zipped = archive(self.root)
        original = zipped.read_bytes()
        with patch.object(sbom, "scan", return_value=inventory(self.root)):
            record = sbom.generate(self.root, zipped, "unused", COMMIT)
        document = json.loads((self.root / record["sbom"]).read_text())
        sbom.validate(self.root / record["sbom"])
        self.assertEqual(zipped.read_bytes(), original)
        self.assertIn("not an assertion", document["documentComment"])
        delivered = [
            f for f in document["files"] if f["fileName"].startswith("./module/")
        ]
        with zipfile.ZipFile(zipped) as contents:
            self.assertEqual(len(delivered), len(contents.namelist()))
        described = [
            r for r in document["relationships"] if r["relationshipType"] == "DESCRIBES"
        ]
        self.assertEqual([r["relatedSpdxElement"] for r in described], [sbom.MODULE_ID])
        self.assertTrue(
            any(
                p["name"] == "json" and p["primaryPackagePurpose"] == "SOURCE"
                for p in document["packages"]
            )
        )

    def test_tini_corruption_and_mismatched_component_sources_stop_generation(self):
        for name, content, message in [
            ("bin/tini-arm64", b"corrupt", "Tini"),
            (
                "play-integrity/build.json",
                json.dumps({"source_commit": "b" * 40}),
                "source",
            ),
        ]:
            zipped = archive(self.root)
            changed = self.root / "changed.zip"
            with (
                zipfile.ZipFile(zipped) as before,
                zipfile.ZipFile(changed, "w") as after,
            ):
                for entry in before.infolist():
                    after.writestr(
                        entry, content if entry.filename == name else before.read(entry)
                    )
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, message):
                sbom.assemble(
                    copy.deepcopy(inventory(self.root)), self.root, changed, COMMIT
                )

    def test_unsafe_or_duplicate_payload_entries_are_rejected(self):
        for name in ["../escape", "/absolute", "a\\b", "module.prop"]:
            zipped = archive(self.root)
            with zipfile.ZipFile(zipped, "a") as contents:
                import warnings

                with warnings.catch_warnings():
                    warnings.simplefilter("ignore", UserWarning)
                    contents.writestr(name, b"bad")
            with (
                self.subTest(name=name),
                self.assertRaisesRegex(ValueError, "Unsafe or duplicate"),
            ):
                sbom.archive_files(zipped)

    def test_schema_validator_rejects_dangling_relationships(self):
        zipped = archive(self.root)
        document = inventory(self.root)
        sbom.assemble(document, self.root, zipped, COMMIT)
        sbom.relate(document, sbom.MODULE_ID, "CONTAINS", "SPDXRef-Missing")
        path = self.root / "invalid.spdx.json"
        path.write_text(json.dumps(document))
        with self.assertRaisesRegex(ValueError, "Invalid SPDX"):
            sbom.validate(path)
