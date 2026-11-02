from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]


class WorkflowPermissionsTests(unittest.TestCase):
    def test_external_release_workflow_actions_are_immutable(self):
        for name in ["ci.yml", "stable.yml"]:
            workflow = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
            for job in workflow["jobs"].values():
                for step in job.get("steps", []):
                    if "uses" in step and not step["uses"].startswith("./"):
                        self.assertRegex(step["uses"], r"@[0-9a-f]{40}$")

    def test_pr_builds_cannot_enter_the_signing_or_release_job(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        publish = workflow["jobs"]["publish"]
        self.assertIn("refs/heads/main", publish["if"])
        self.assertIn("github.event_name == 'push'", publish["if"])
        self.assertIn("github.event_name == 'workflow_dispatch'", publish["if"])
        self.assertNotIn("pull_request", publish["if"])
        for name, job in workflow["jobs"].items():
            if name != "publish":
                self.assertNotIn("write", job.get("permissions", {}).values())
        self.assertNotIn("environment", workflow["jobs"]["package"])

    def test_stable_caller_passes_all_signing_permissions(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/stable.yml").read_text())
        self.assertEqual(
            workflow["jobs"]["release"]["permissions"],
            {
                "contents": "write",
                "id-token": "write",
                "attestations": "write",
            },
        )

    def test_both_attestations_bind_the_zip_with_pinned_actions(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["publish"]["steps"]
        attest = [
            step for step in steps if step.get("uses", "").startswith("actions/attest@")
        ]
        self.assertEqual(len(attest), 2)
        for step in attest:
            self.assertRegex(step["uses"], r"@[0-9a-f]{40}$")
            self.assertIn(
                "${{ env.MODULE_PATH }}", step["with"]["subject-path"].splitlines()
            )
        self.assertIn("sbom-path", attest[1]["with"])
        self.assertNotIn("sbom-path", attest[0]["with"])
        self.assertIn(
            "dist/${{ needs.package.outputs.sbom-file }}",
            attest[0]["with"]["subject-path"].splitlines(),
        )
