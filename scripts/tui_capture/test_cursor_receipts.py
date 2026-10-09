"""Receipt-format guards, not application or pixel-parity qualification."""
import gzip
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import check_cursor_temporal as audit


class FrameReferenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "cursor-idle-phase-visible-00.render.json"
        self.grid = {"columns": 2, "rows": 2, "cells": [["all fields retained"]]}

    def test_historical_inline_cannot_redirect_comparison_outside_its_sibling(self):
        render = {"schema_version": 1, "observation": {"frame": self.grid}}
        grid, path = audit.verified_frame(self.path, render)
        self.assertEqual(grid, self.grid)
        self.assertEqual(path.name, "cursor-idle-phase-visible-00.cells.json")
        for reference in ("../escaped.cells.json.gz", "/escaped.cells.json.gz"):
            render["observation"]["frame_file"] = reference
            with self.assertRaises(AssertionError):
                audit.verified_frame(self.path, render)

    def test_reference_requires_schema_basename_seal_and_no_inline_bypass(self):
        path = self.path.with_suffix("").with_suffix(".cells.json.gz")
        path.write_bytes(gzip.compress(json.dumps(self.grid).encode(), mtime=0))
        observation = {"frame_file": path.name,
                       "frame_sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        render = {"schema_version": 2, "observation": observation}
        self.assertEqual(audit.verified_frame(self.path, render), (self.grid, path))
        for changes in ({"frame": self.grid}, {"frame_file": "../" + path.name},
                        {"frame_sha256": "0" * 64}):
            with self.subTest(changes=changes), self.assertRaises(AssertionError):
                audit.verified_frame(self.path, {"schema_version": 2,
                                                "observation": observation | changes})
