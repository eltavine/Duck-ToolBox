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
        self.archive = archive(self.root)
        with patch.object(sbom, "scan", return_value=inventory(self.root)):
            self.record = sbom.generate(self.root, self.archive, "unused", COMMIT)
        self.provenance = self.root / "provenance.json"
        self.bundle = self.root / "sbom.json"
        self.provenance.write_text("{}")
        self.bundle.write_text("{}")

    def test_changed_files_and_stale_source_identity_fail_before_attestation(self):
        for filename in [self.record["archive"], self.record["sbom"]]:
            path = self.root / filename
            original = path.read_bytes()
            path.write_bytes(original + b" ")
            with (
                self.subTest(filename=filename),
                self.assertRaisesRegex(ValueError, "checksum"),
            ):
                release.read_payload(self.root, COMMIT)
            path.write_bytes(original)
        with self.assertRaisesRegex(ValueError, "identity"):
            release.read_payload(self.root, "b" * 40)

    def test_unsafe_and_extra_release_file_names_are_rejected(self):
        self.record["archive"] = "../escape.zip"
        (self.root / "release.json").write_text(json.dumps(self.record))
        with self.assertRaisesRegex(ValueError, "Unexpected"):
            release.read_payload(self.root, COMMIT)

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
                    self.root / self.record["sbom"],
                )
        args = command.call_args.args
        self.assertIn("--deny-self-hosted-runners", args)
        self.assertIn(COMMIT, args)
        self.assertIn("owner/repo/.github/workflows/ci.yml", args)
        self.assertIn("https://spdx.dev/Document/v2.3", args)

    def test_signature_failure_never_creates_a_release(self):
        with patch.object(
            release, "gh", side_effect=subprocess.CalledProcessError(1, "gh")
        ) as command:
            with self.assertRaises(subprocess.CalledProcessError):
                release.publish(
                    self.root,
                    self.root,
                    self.provenance,
                    self.bundle,
                    "stable",
                    "owner/repo",
                    COMMIT,
                )
        self.assertTrue(
            all(call.args[0] == "attestation" for call in command.call_args_list)
        )

    def test_all_assets_are_uploaded_as_draft_and_zip_is_selected_by_name(self):
        document = json.loads((self.root / self.record["sbom"]).read_text())
        calls = []
        url = f"https://github.com/owner/repo/releases/download/v1.2.3/{self.archive.name}"

        def command(*args):
            calls.append(args)
            if args[0] == "attestation":
                return json.dumps(
                    [{"verificationResult": {"statement": {"predicate": document}}}]
                )
            if args[:2] == ("release", "view"):
                assets = [
                    self.root / "SHA256SUMS",
                    self.archive,
                    self.root / self.record["sbom"],
                    self.root / f"{self.record['name']}.provenance.sigstore.json",
                    self.root / f"{self.record['name']}.sbom.sigstore.json",
                ]
                return json.dumps(
                    {
                        "assets": [
                            {
                                "name": a.name,
                                "size": a.stat().st_size,
                                "url": url if a == self.archive else "wrong",
                            }
                            for a in assets
                        ]
                    }
                )
            if args[:2] == ("release", "create"):
                for suffix in [
                    ".zip",
                    ".spdx.json",
                    ".provenance.sigstore.json",
                    ".sbom.sigstore.json",
                ]:
                    self.assertTrue(any(str(a).endswith(suffix) for a in args))
                self.assertIn("--draft", args)
                self.assertTrue(
                    Path(args[args.index("--notes-file") + 1])
                    .read_text()
                    .startswith("Release changes")
                )
            return ""

        with (
            patch.object(release, "gh", side_effect=command),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            self.assertEqual(
                release.publish(
                    self.root,
                    self.root,
                    self.provenance,
                    self.bundle,
                    "stable",
                    "owner/repo",
                    COMMIT,
                ),
                url,
            )
        creation = next(
            i for i, a in enumerate(calls) if a[:2] == ("release", "create")
        )
        publication = next(
            i for i, a in enumerate(calls) if a[:2] == ("release", "edit")
        )
        self.assertLess(creation, publication)
        self.assertIn("--draft=false", calls[publication])
        self.assertEqual(len((self.root / "SHA256SUMS").read_text().splitlines()), 4)

    def test_partial_upload_recovers_only_a_draft_for_the_same_commit(self):
        document = json.loads((self.root / self.record["sbom"]).read_text())
        calls = []

        def command(*args):
            calls.append(args)
            if args[0] == "attestation":
                return json.dumps(
                    [{"verificationResult": {"statement": {"predicate": document}}}]
                )
            if args[:2] == ("release", "create"):
                raise subprocess.CalledProcessError(1, "gh")
            if args[:2] == ("release", "view"):
                assets = [
                    self.archive,
                    self.root / self.record["sbom"],
                    self.root / "SHA256SUMS",
                    self.root / f"{self.record['name']}.provenance.sigstore.json",
                    self.root / f"{self.record['name']}.sbom.sigstore.json",
                ]
                return json.dumps(
                    {
                        "isDraft": True,
                        "targetCommitish": COMMIT,
                        "assets": [
                            {
                                "name": a.name,
                                "size": a.stat().st_size,
                                "url": "https://example.com/" + a.name,
                            }
                            for a in assets
                        ],
                    }
                )
            return ""

        with (
            patch.object(release, "gh", side_effect=command),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            release.publish(
                self.root,
                self.root,
                self.provenance,
                self.bundle,
                "stable",
                "owner/repo",
                COMMIT,
            )
        self.assertTrue(
            any(a[:2] == ("release", "upload") and "--clobber" in a for a in calls)
        )

    def test_public_release_is_never_replaced_on_retry(self):
        with (
            patch.object(release, "verify_bundle"),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            responses = [
                subprocess.CalledProcessError(1, "gh"),
                json.dumps({"isDraft": False, "targetCommitish": COMMIT, "assets": []}),
            ]
            with patch.object(release, "gh", side_effect=responses) as command:
                with self.assertRaisesRegex(ValueError, "Refusing"):
                    release.publish(
                        self.root,
                        self.root,
                        self.provenance,
                        self.bundle,
                        "stable",
                        "owner/repo",
                        COMMIT,
                    )
                self.assertFalse(
                    any(
                        a.args[:2] == ("release", "upload")
                        for a in command.call_args_list
                    )
                )

    def test_stable_metadata_cannot_be_published_as_canary(self):
        with (
            patch.object(release, "verify_bundle"),
            self.assertRaisesRegex(ValueError, "Nightly"),
        ):
            release.publish(
                self.root,
                self.root,
                self.provenance,
                self.bundle,
                "nightly",
                "owner/repo",
                COMMIT,
            )

    def test_canary_retry_uses_the_packaged_timestamp_and_preserves_prerelease_flags(
        self,
    ):
        canary = archive(self.root, "1.2.3 (7-canary)")
        self.archive = self.root / "duck-toolbox-1.2.3-7-canary.zip"
        canary.rename(self.archive)
        with patch.object(sbom, "scan", return_value=inventory(self.root)):
            self.record = sbom.generate(self.root, self.archive, "unused", COMMIT)
        calls = []

        def command(*args):
            calls.append(args)
            if args[:2] == ("release", "view"):
                paths = [
                    self.archive,
                    self.root / self.record["sbom"],
                    self.root / "SHA256SUMS",
                    self.root / f"{self.record['name']}.provenance.sigstore.json",
                    self.root / f"{self.record['name']}.sbom.sigstore.json",
                ]
                return json.dumps(
                    {
                        "assets": [
                            {
                                "name": p.name,
                                "size": p.stat().st_size,
                                "url": "https://example.com/" + p.name,
                            }
                            for p in paths
                        ]
                    }
                )
            return ""

        with (
            patch.object(release, "gh", side_effect=command),
            patch.object(release, "verify_bundle"),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            for _ in range(2):
                release.publish(
                    self.root,
                    self.root,
                    self.provenance,
                    self.bundle,
                    "nightly",
                    "owner/repo",
                    COMMIT,
                )
        creates = [a for a in calls if a[:2] == ("release", "create")]
        self.assertEqual(creates[0][2], creates[1][2])
        self.assertEqual(
            creates[0][2], f"nightly-{COMMIT[:8]}-{self.record['release_timestamp']}"
        )
        self.assertIn("--prerelease", creates[0])
        self.assertIn("--latest=false", creates[0])

    def test_incomplete_upload_is_never_published(self):
        with (
            patch.object(release, "verify_bundle"),
            patch.dict("os.environ", {"GITHUB_RUN_ID": "42"}),
        ):
            with patch.object(
                release, "gh", side_effect=["", '{"assets": []}']
            ) as command:
                with self.assertRaisesRegex(ValueError, "incomplete"):
                    release.publish(
                        self.root,
                        self.root,
                        self.provenance,
                        self.bundle,
                        "stable",
                        "owner/repo",
                        COMMIT,
                    )
        self.assertFalse(any("--draft=false" in c.args for c in command.call_args_list))
