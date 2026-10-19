"""Examples and provenance checks need no task, journal or documentation tree."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from check_docs import ROOT, jsonc, validate


class DocumentationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "kit"
        self.root.mkdir()
        for name in ("examples", "references"):
            shutil.copytree(ROOT / name, self.root / name)
        (self.root / "planning").mkdir()
        for name in ("baseline.lock.json", "source-manifest.json"):
            shutil.copyfile(ROOT / "planning" / name, self.root / "planning" / name)

    def test_examples_and_baselines_without_execution_package(self):
        counts = validate(self.root)
        self.assertGreater(counts["examples"], 0)
        self.assertEqual(counts["source_baselines"], 3)
        for name in ("progress", "GOAL.md", "prompts", "evidence", "planning/tasks.json"):
            self.assertFalse((self.root / name).exists(), name)

    def test_jsonc_preserves_urls_and_comment_text_in_strings(self):
        value = jsonc('{"url":"https://example.invalid/a//b",/* hi */"text":"/* literal */",}')
        self.assertEqual(value["url"], "https://example.invalid/a//b")
        self.assertEqual(value["text"], "/* literal */")

    def test_malformed_example_rejected(self):
        (self.root / "examples/opencode.jsonc").write_text('{"providers": }', encoding="utf-8")
        with self.assertRaises(ValueError):
            validate(self.root)

    def test_modified_discovery_snapshot_rejected(self):
        (self.root / "references/openproxy-models.user.mjs").write_text("// modified reference", encoding="utf-8")
        with self.assertRaisesRegex(AssertionError, "Discovery snapshot digest mismatch"):
            validate(self.root)

    def test_invalid_dcp_license_rejected(self):
        path = self.root / "planning/baseline.lock.json"
        baseline = json.loads(path.read_text(encoding="utf-8"))
        baseline["dcp"]["license"] = "MIT"
        path.write_text(json.dumps(baseline), encoding="utf-8")
        with self.assertRaisesRegex(AssertionError, "DCP license must remain AGPL"):
            validate(self.root)

    def test_ignored_local_json_is_not_scanned(self):
        (self.root / "opencode.local.json").write_text("not json and may contain user config", encoding="utf-8")
        validate(self.root)


if __name__ == "__main__":
    unittest.main()
