"""Test that package validation rejects stale/broken planning inputs."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from check_docs import ROOT, jsonc, validate


class DocumentationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name) / "kit"
        shutil.copytree(ROOT, self.root, ignore=shutil.ignore_patterns("__pycache__", ".lock"))

    def tearDown(self):
        self.temp.cleanup()

    def test_valid_package(self):
        counts = validate(self.root)
        self.assertEqual(counts["tasks"], 31)
        self.assertEqual(counts["acceptance_specifications"], 82)

    def test_jsonc_preserves_urls_and_comment_text_in_strings(self):
        value = jsonc('{"url":"https://example.invalid/a//b",/* hi */"text":"/* literal */",}')
        self.assertEqual(value["url"], "https://example.invalid/a//b")
        self.assertEqual(value["text"], "/* literal */")

    def test_cycle_rejected(self):
        path = self.root / "planning/tasks.json"
        data = json.loads(path.read_text())
        data["tasks"][0]["depends_on"] = ["T01"]
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(AssertionError, "Cyclic"):
            validate(self.root)

    def test_unassigned_scenario_rejected(self):
        acceptance_path = self.root / "planning/acceptance.json"
        acceptance = json.loads(acceptance_path.read_text())
        acceptance["tests"][-1]["id"] = "EXTRA01"
        acceptance_path.write_text(json.dumps(acceptance))
        tasks_path = self.root / "planning/tasks.json"
        tasks = json.loads(tasks_path.read_text())
        t00 = next(task for task in tasks["tasks"] if task["id"] == "T00")
        t00["tests"].remove("OPS05")
        tasks_path.write_text(json.dumps(tasks))
        with self.assertRaisesRegex(AssertionError, "Unassigned"):
            validate(self.root)

    def test_generic_objective_has_no_codex_size_cap(self):
        path = self.root / "prompts/AGENT_GOAL.txt"
        path.write_text("x" * 4001)
        self.assertEqual(validate(self.root)["goal_characters"], 4001)

    def test_runner_specific_active_surface_rejected(self):
        path = self.root / "GOAL.md"
        path.write_text(path.read_text() + "\nCodex CLI must run this repository.\n")
        with self.assertRaisesRegex(AssertionError, "Runner-specific"):
            validate(self.root)

    def test_missing_a13_goal_gate_rejected(self):
        path = self.root / "GOAL.md"
        path.write_text(path.read_text().replace("**A13 CONFIGURED WORKSPACE:**", "**CONFIGURED WORKSPACE:**"))
        with self.assertRaisesRegex(AssertionError, "exact A01-A13"):
            validate(self.root)

    def test_missing_a13_final_section_rejected(self):
        path = self.root / "evidence/FINAL.template.md"
        path.write_text(path.read_text().replace("## A13", "## Configured workspace"))
        with self.assertRaisesRegex(AssertionError, "FINAL template"):
            validate(self.root)

    def test_test_plan_registry_drift_rejected(self):
        path = self.root / "docs/TEST_PLAN.md"
        path.write_text(path.read_text().replace("**CFG05 — Config roots.**", "**Config roots.**"))
        with self.assertRaisesRegex(AssertionError, "TEST_PLAN"):
            validate(self.root)


if __name__ == "__main__":
    unittest.main()
