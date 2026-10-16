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
        # Copy documentation inputs, never build trees or local credential/config
        # directories. Production validation reads exactly these repo surfaces.
        self.root.mkdir()
        for name in ("planning", "progress", "examples", "prompts", "docs", "roadmap", "audit", "references", "evidence"):
            shutil.copytree(ROOT / name, self.root / name,
                            ignore=shutil.ignore_patterns("__pycache__", ".lock", "tui"))
        for name in ("GOAL.md", "README.md", "AGENTS.md", "OPENCODE_RUST_MASTER_PLAN.md"):
            shutil.copyfile(ROOT / name, self.root / name)

    def tearDown(self):
        self.temp.cleanup()

    def test_valid_package(self):
        counts = validate(self.root)
        self.assertGreater(counts["tasks"], 0)
        self.assertGreater(counts["acceptance_specifications"], 0)

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
        acceptance["tests"].append({"id": "EXTRA01", "title": "Unassigned", "expected": "Must have an owner"})
        acceptance_path.write_text(json.dumps(acceptance))
        tasks_path = self.root / "planning/tasks.json"
        tasks = json.loads(tasks_path.read_text())
        t00 = next(task for task in tasks["tasks"] if task["id"] == "T00")
        t00["tests"].remove("OPS05")
        tasks_path.write_text(json.dumps(tasks))
        with self.assertRaisesRegex(AssertionError, "Unassigned"):
            validate(self.root)

    def test_duplicate_scenario_owner_rejected(self):
        path = self.root / "planning/tasks.json"
        data = json.loads(path.read_text())
        data["tasks"][1]["tests"].append("ENV01")
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(AssertionError, "one owner"):
            validate(self.root)

    def test_shared_goal_gate_references_are_not_detailed_owners(self):
        path = self.root / "planning/tasks.json"
        data = json.loads(path.read_text())
        for task in data["tasks"][:2]:
            task["tests"].append("A13")
        path.write_text(json.dumps(data))
        validate(self.root)

    def test_unknown_goal_gate_is_rejected(self):
        path = self.root / "planning/tasks.json"
        data = json.loads(path.read_text())
        data["tasks"][0]["tests"].append("A14")
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(AssertionError, "Unknown test"):
            validate(self.root)

    def test_goal_gate_cannot_shadow_a_detailed_test(self):
        path = self.root / "planning/acceptance.json"
        data = json.loads(path.read_text())
        data["tests"].append({"id": "A02", "title": "Shadow", "expected": "Invalid namespace"})
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(AssertionError, "shadow GOAL"):
            validate(self.root)

    def test_duplicate_reference_is_rejected(self):
        path = self.root / "planning/tasks.json"
        data = json.loads(path.read_text())
        data["tasks"][0]["tests"].append(data["tasks"][0]["tests"][0])
        path.write_text(json.dumps(data))
        with self.assertRaisesRegex(AssertionError, "Duplicate test reference"):
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

    def test_ignored_local_json_is_not_scanned(self):
        (self.root / "opencode.local.json").write_text("not json and may contain user config")
        validate(self.root)


if __name__ == "__main__":
    unittest.main()
