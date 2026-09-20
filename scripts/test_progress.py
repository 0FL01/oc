"""Offline regression tests for the documentation journal, not for Rust oc."""
from pathlib import Path
import json
import tempfile
import unittest

from progress import Invalid, Journal, NOTE_LIMIT

ROOT = Path(__file__).resolve().parents[1]
NOTE = "## Result\nTest checkpoint.\n## Checks\nSynthetic check.\n## Risks\nNot product evidence.\n## Next\nContinue test.\n"


class JournalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "planning").mkdir()
        tasks = [{"id": f"T{i:02}", "phase": "M0", "title": f"Task {i}",
                  "depends_on": [f"T{i-1:02}"] if i else [], "work": "Synthetic task.",
                  "spec": "roadmap/M0.md", "evidence": f"evidence/T{i:02}/report.md"}
                 for i in range(3)]
        (self.root / "planning/tasks.json").write_text(json.dumps({"tasks": tasks}))
        (self.root / "note.md").write_text(NOTE)
        self.journal = Journal(self.root)
        self.journal.initialize()

    def tearDown(self):
        self.temp.cleanup()

    def state(self):
        return self.journal.load()

    def start(self, task="T00"):
        self.journal.start(self.state(), task)

    def finish(self, task="T00"):
        report = self.root / f"evidence/{task}/report.md"
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text("Synthetic utility test, NOT Rust acceptance.\n")
        self.journal.checkpoint(self.state(), "note.md", "finish", f"evidence/{task}/report.md")

    def test_initial_views_and_ready(self):
        state = self.state()
        self.journal.check_views(state)
        self.assertEqual(self.journal.ready(state), ["T00"])
        self.assertIsNone(state["current"])
        self.assertFalse((self.root / "progress/M0/T00/INDEX.md").read_text().endswith("\n\n"))

    def test_dependency_and_single_active(self):
        with self.assertRaises(Invalid):
            self.start("T01")
        self.start()
        with self.assertRaises(Invalid):
            self.start("T01")
        with self.assertRaises(Invalid):
            self.start("T00")

    def test_checkpoint_does_not_complete(self):
        self.start()
        self.journal.checkpoint(self.state(), "note.md")
        info = self.state()["tasks"]["T00"]
        self.assertEqual(info["status"], "active")
        self.assertEqual(info["last_seq"], 1)
        self.assertEqual((self.root / info["latest"]).read_text(), NOTE)

    def test_finish_requires_report_and_does_not_write_on_failure(self):
        self.start()
        before = self.journal.state_path.read_bytes()
        with self.assertRaises(Invalid):
            self.journal.checkpoint(self.state(), "note.md", "finish", "evidence/T00/report.md")
        self.assertEqual(before, self.journal.state_path.read_bytes())
        self.assertFalse((self.root / "progress/M0/T00/0001.md").exists())
        self.finish()
        self.assertEqual(self.journal.ready(self.state()), ["T01"])
        self.assertIn("Последний срез: T00 [done]", (self.root / "progress/NOW.md").read_text())
        self.journal.check_views(self.state())

    def test_note_byte_limit_and_required_sections(self):
        self.start()
        before = self.journal.state_path.read_bytes()
        (self.root / "bad.md").write_text(NOTE + "Я" * NOTE_LIMIT)
        with self.assertRaises(Invalid):
            self.journal.checkpoint(self.state(), "bad.md")
        (self.root / "bad.md").write_text("Missing the four sections")
        with self.assertRaises(Invalid):
            self.journal.checkpoint(self.state(), "bad.md")
        self.assertEqual(before, self.journal.state_path.read_bytes())

    def test_bounded_index_preserves_all_leaves(self):
        self.start()
        for _ in range(40):
            self.journal.checkpoint(self.state(), "note.md")
        directory = self.root / "progress/M0/T00"
        index = (directory / "INDEX.md").read_text()
        self.assertEqual(len(list(directory.glob("[0-9]*.md"))), 40)
        self.assertEqual(index.count("- ["), 12)
        self.assertNotIn("0001.md", index)
        self.assertIn("0040.md", index)
        for path in (self.root / "progress").rglob("INDEX.md"):
            self.assertLessEqual(path.stat().st_size, 8192)
        self.assertLessEqual((self.root / "progress/NOW.md").stat().st_size, 8192)

    def test_block_resume_retains_checkpoint(self):
        self.start()
        work = self.root / "work.txt"
        work.write_text("uncommitted user work\n")
        self.journal.checkpoint(self.state(), "note.md", "block")
        self.assertIsNone(self.state()["current"])
        resumed = Journal(self.root)
        resumed.start(resumed.load(), "T00")
        self.assertEqual(resumed.load()["tasks"]["T00"]["last_seq"], 1)
        self.assertEqual(work.read_text(), "uncommitted user work\n")

    def test_orphan_is_detected_not_deleted(self):
        self.start()
        path = self.root / "progress/M0/T00/0001.md"
        path.write_text(NOTE)
        with self.assertRaisesRegex(Invalid, "Orphan"):
            self.state()
        self.assertTrue(path.exists())

    def test_stale_views_rebuild_without_state_change(self):
        before = self.journal.state_path.read_bytes()
        (self.root / "progress/NOW.md").write_text("stale")
        with self.assertRaisesRegex(Invalid, "Stale"):
            self.journal.check_views(self.state())
        self.journal.render(self.state())
        self.journal.check_views(self.state())
        self.assertEqual(before, self.journal.state_path.read_bytes())

    def test_reopen_blocks_invalidating_completed_dependents(self):
        self.start()
        self.finish()
        self.start("T01")
        self.finish("T01")
        with self.assertRaisesRegex(Invalid, "Completed dependents"):
            self.journal.reopen(self.state(), "T00", "Correction")
        self.journal.reopen(self.state(), "T01", "Correction")
        self.assertEqual(self.state()["tasks"]["T01"]["status"], "todo")

    def test_path_escape_and_symlink_rejected(self):
        self.start()
        with self.assertRaises(Invalid):
            self.journal.checkpoint(self.state(), "../note.md")
        (self.root / "link.md").symlink_to(self.root / "note.md")
        with self.assertRaises(Invalid):
            self.journal.checkpoint(self.state(), "link.md")

    def test_state_corruption_rejected(self):
        state = self.state()
        state["current"] = "T00"
        self.journal.save(state)
        with self.assertRaisesRegex(Invalid, "One-active"):
            self.state()

    def test_missing_report_on_done_detected(self):
        self.start()
        self.finish()
        (self.root / "evidence/T00/report.md").unlink()
        with self.assertRaises(Invalid):
            self.state()

    def test_no_double_initialization(self):
        before = self.journal.state_path.read_bytes()
        with self.assertRaises(Invalid):
            self.journal.initialize()
        self.assertEqual(before, self.journal.state_path.read_bytes())

    def test_long_valid_note_still_fits_resume_budget(self):
        self.start()
        fill = "x" * (NOTE_LIMIT - len(NOTE.encode()) - 1)
        (self.root / "long.md").write_text(NOTE + fill)
        self.journal.checkpoint(self.state(), "long.md")
        self.journal.check_views(self.state())
        self.assertLessEqual((self.root / "progress/NOW.md").stat().st_size, 8192)


if __name__ == "__main__":
    unittest.main()
