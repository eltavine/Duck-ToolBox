import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from fixtures import COMMIT, archive, inventory, source
import release
import sbom


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = source(Path(self.temporary.name))
        self.directory = self.root / "dist"
        self.bundles = {}
        for variant in ("full", "lite"):
            provenance = self.root / f"{variant}-provenance.json"
            sbom_bundle = self.root / f"{variant}-sbom.json"
            provenance.write_text(f"{variant} provenance")
            sbom_bundle.write_text(f"{variant} SBOM")
            self.bundles[variant] = (provenance, sbom_bundle)
        self.packages()

    def packages(self, version="1.2.3", version_code="123", suffix=None):
        self.records = {}
        for variant in ("full", "lite"):
            folder = self.directory / variant
            module = archive(folder, version, variant, version_code)
            if suffix:
                canonical = module.with_name(f"{module.stem.split(' (')[0]}-{suffix}.zip")
                module.rename(canonical)
                module = canonical
            with patch.object(sbom, "scan", return_value=inventory(self.root)):
                self.records[variant] = sbom.generate(self.root, module, "unused", COMMIT, variant)
        self.full = self.directory / "full"
        self.record = self.records["full"]
        self.archive = self.full / self.record["archive"]
        self.bundle = self.bundles["full"][1]

    def publish(self, channel="stable"):
        return release.publish(self.root, self.directory, self.bundles, channel, "owner/repo", COMMIT)

    def uploaded(self):
        files = [self.directory / "SHA256SUMS"]
        for variant, record in self.records.items():
            folder = self.directory / variant
            files.extend(folder / name for name in (
                record["archive"], record["sbom"],
                f"{record['name']}.provenance.sigstore.json",
                f"{record['name']}.sbom.sigstore.json",
            ))
        return [
            {"name": path.name, "size": path.stat().st_size,
             "digest": f"sha256:{release.digest(path)}",
             "url": f"https://example.com/{path.name}"}
            for path in reversed(files)
        ]

    def verified(self, path):
        subject = Path(path)
        variant = subject.parent.name
        document = json.loads((subject.parent / self.records[variant]["sbom"]).read_text())
        return json.dumps([{"verificationResult": {"statement": {"predicate": document}}}])

    def test_changed_files_and_stale_source_identity_fail_before_attestation(self):
        for filename in [self.record["archive"], self.record["sbom"]]:
            path = self.full / filename
            original = path.read_bytes()
            path.write_bytes(original + b" ")
            with (
                self.subTest(filename=filename),
                self.assertRaisesRegex(ValueError, "checksum"),
            ):
                release.read_payload(self.full, COMMIT)
            path.write_bytes(original)
        with self.assertRaisesRegex(ValueError, "identity"):
            release.read_payload(self.full, "b" * 40)

    def test_unsafe_and_extra_release_file_names_are_rejected(self):
        self.record["archive"] = "../escape.zip"
        (self.full / "release.json").write_text(json.dumps(self.record))
        with self.assertRaisesRegex(ValueError, "Unexpected"):
            release.read_payload(self.full, COMMIT)

    def test_verified_predicate_must_match_the_downloadable_sbom(self):
        result = [
            {"verificationResult": {"statement": {"predicate": {"different": True}}}}
        ]
        with patch.object(release, "gh", return_value=json.dumps(result)) as command:
            with self.assertRaisesRegex(ValueError, "differs"):
                release.verify_bundle(
                    self.archive,
                    self.bundle,
                    "owner/repo",
                    COMMIT,
                    self.full / self.record["sbom"],
                )
        args = command.call_args.args
        self.assertIn("--deny-self-hosted-runners", args)
        self.assertIn(COMMIT, args)
        self.assertIn("owner/repo/.github/workflows/ci.yml", args)
        self.assertIn("https://spdx.dev/Document/v2.3", args)

    def test_mismatched_editions_fail_before_signing(self):
        lite = self.directory / "lite"
        other = archive(lite, "1.2.4", "lite", "124")
        with patch.object(sbom, "scan", return_value=inventory(self.root)):
            sbom.generate(self.root, other, "unused", COMMIT, "lite")
        with patch.object(release, "verify_bundle") as verify:
            with self.assertRaisesRegex(ValueError, "versions must match"):
                self.publish()
            verify.assert_not_called()

    def test_lite_signature_failure_never_creates_a_release(self):
        def command(*args):
            self.assertEqual(args[0], "attestation")
            if Path(args[2]).parent.name == "lite":
                raise subprocess.CalledProcessError(1, "gh")
            return self.verified(args[2])

        with patch.object(release, "gh", side_effect=command) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                self.publish()
        self.assertEqual(run.call_count, 4)

    def test_all_assets_are_uploaded_as_draft_and_full_zip_is_selected_by_name(self):
        def command(*args):
            if args[0] == "attestation":
                return self.verified(args[2])
            if args[:2] == ("release", "view"):
                return json.dumps({"assets": self.uploaded()})
            if args[:2] == ("release", "create"):
                for suffix in (".zip", ".spdx.json", ".provenance.sigstore.json", ".sbom.sigstore.json"):
                    self.assertEqual(sum(str(a).endswith(suffix) for a in args), 2)
                self.assertIn("--draft", args)
                notes = Path(args[args.index("--notes-file") + 1]).read_text()
                self.assertTrue(notes.startswith("Release changes"))
                for record in self.records.values():
                    self.assertIn(record["archive"], notes)
            return ""

        with (
            patch.object(release, "gh", side_effect=command) as run,
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            self.assertEqual(self.publish(), f"https://example.com/{self.archive.name}")
        calls = [call.args for call in run.call_args_list]
        self.assertTrue(all(args[0] == "attestation" for args in calls[:6]))
        self.assertEqual(calls[6][:2], ("release", "create"))
        self.assertIn("--draft=false", calls[-1])
        self.assertEqual(len((self.directory / "SHA256SUMS").read_text().splitlines()), 8)

    def test_partial_upload_recovers_only_a_draft_for_the_same_commit(self):
        def command(*args):
            if args[:2] == ("release", "create"):
                raise subprocess.CalledProcessError(1, "gh")
            if args[:2] == ("release", "view"):
                return json.dumps({
                    "isDraft": True, "targetCommitish": COMMIT, "assets": self.uploaded()
                })
            return ""

        with (
            patch.object(release, "gh", side_effect=command) as run,
            patch.object(release, "verify_bundle"),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            self.publish()
        self.assertTrue(any(
            call.args[:2] == ("release", "upload") and "--clobber" in call.args
            for call in run.call_args_list
        ))

    def test_public_release_is_never_replaced_on_retry(self):
        responses = [
            subprocess.CalledProcessError(1, "gh"),
            json.dumps({"isDraft": False, "targetCommitish": COMMIT, "assets": []}),
        ]
        with (
            patch.object(release, "verify_bundle"),
            patch.object(release, "gh", side_effect=responses) as run,
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            with self.assertRaisesRegex(ValueError, "Refusing"):
                self.publish()
        self.assertEqual(run.call_count, 2)

    def test_stable_metadata_cannot_be_published_as_canary(self):
        with self.assertRaisesRegex(ValueError, "Nightly"):
            self.publish("nightly")

    def test_canary_retry_uses_the_packaged_timestamp_and_preserves_prerelease_flags(self):
        self.packages("1.2.3 (7-canary)", "7", "7-canary")

        def command(*args):
            if args[:2] == ("release", "view"):
                return json.dumps({"assets": self.uploaded()})
            return ""

        with (
            patch.object(release, "gh", side_effect=command) as run,
            patch.object(release, "verify_bundle"),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            for _ in range(2):
                self.publish("nightly")
        creates = [call.args for call in run.call_args_list if call.args[:2] == ("release", "create")]
        self.assertEqual(creates[0][2], creates[1][2])
        self.assertEqual(creates[0][2], f"nightly-{COMMIT[:8]}-{self.record['release_timestamp']}")
        self.assertIn("--prerelease", creates[0])
        self.assertIn("--latest=false", creates[0])

    def test_incomplete_or_modified_upload_is_never_published(self):
        for corruption in ("missing", "digest"):
            def command(*args):
                if args[:2] == ("release", "view"):
                    assets = self.uploaded()
                    if corruption == "missing":
                        assets.pop()
                    else:
                        assets[0]["digest"] = "sha256:" + "0" * 64
                    return json.dumps({"assets": assets})
                return ""

            with (
                patch.object(release, "verify_bundle"),
                patch.object(release, "gh", side_effect=command) as run,
                patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
                self.subTest(corruption=corruption),
            ):
                with self.assertRaisesRegex(ValueError, "incomplete or their digests differ"):
                    self.publish()
                self.assertFalse(any("--draft=false" in call.args for call in run.call_args_list))
