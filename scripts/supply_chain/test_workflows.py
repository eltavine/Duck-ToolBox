from fnmatch import fnmatchcase
import json
from pathlib import Path
import unittest

import yaml

from payload import ARTIFACT_PREFIX, FULL_ID, LITE_ID

ROOT = Path(__file__).resolve().parents[2]


class WorkflowPermissionsTests(unittest.TestCase):
    def test_default_update_sources_select_only_full(self):
        catalog = yaml.safe_load((ROOT / "module.yaml").read_text())
        update = json.loads((ROOT / "update.json").read_text())
        pattern = catalog["release"]["assetPattern"]
        for module_id in (FULL_ID, LITE_ID):
            name = f"{ARTIFACT_PREFIX[module_id]}-{update['version']}.zip"
            self.assertEqual(fnmatchcase(name, pattern), module_id == FULL_ID)
            self.assertEqual(update["zipUrl"].endswith("/" + name), module_id == FULL_ID)

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

    def test_each_edition_has_both_attestations_with_distinct_subjects(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
        steps = workflow["jobs"]["publish"]["steps"]
        attest = [
            step for step in steps if step.get("uses", "").startswith("actions/attest@")
        ]
        self.assertEqual(len(attest), 4)
        for edition, zip_var, prefix in (("full", "MODULE_PATH", ""), ("lite", "LITE_MODULE_PATH", "lite-")):
            steps = {step["id"]: step["with"] for step in attest}
            provenance = steps[prefix + "provenance"]
            sbom = steps[prefix + "sbom"]
            zip_path = f"${{{{ env.{zip_var} }}}}"
            sbom_path = f"dist/{edition}/${{{{ needs.package.outputs.{prefix}sbom-file }}}}"
            self.assertEqual(provenance["subject-path"].splitlines(), [zip_path, sbom_path])
            self.assertNotIn("sbom-path", provenance)
            self.assertEqual(sbom["subject-path"], zip_path)
            self.assertEqual(sbom["sbom-path"], sbom_path)
