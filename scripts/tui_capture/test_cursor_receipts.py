"""Receipt-format guards, not application or pixel-parity qualification."""
import gzip
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

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
        compressed_render = self.path.with_name(self.path.name + ".gz")
        compressed_render.write_bytes(gzip.compress(json.dumps(render).encode(), mtime=0))
        self.assertEqual(audit.verified_frame(compressed_render, audit.read(compressed_render)),
                         (self.grid, path), "gzip sidecar keeps the same full sibling/seal")
        for changes in ({"frame": self.grid}, {"frame_file": "../" + path.name},
                        {"frame_sha256": "0" * 64}):
            with self.subTest(changes=changes), self.assertRaises(AssertionError):
                audit.verified_frame(self.path, {"schema_version": 2,
                                                 "observation": observation | changes})

    def test_gzip_has_encoded_and_decoded_bounds_and_no_unlisted_render_aliases(self):
        # Small fixture exercises the unchanged production64MiB guard without
        # producing a large artifact solely to test oversized encoded padding.
        oversized = self.path.with_name("oversized.render.json.gz")
        for data in (gzip.compress(b"{}", mtime=0) + b"\0" * 65,
                     gzip.compress(b" " * 128, mtime=0)):
            oversized.write_bytes(data)
            with mock.patch.object(audit, "MAX_FILE_BYTES", 64), self.assertRaises(AssertionError):
                audit.read(oversized)
        root = self.path.parent
        folder = root / "oc"
        folder.mkdir()
        scenario = "cursor-idle-phase-visible-00"
        render = folder / (scenario + ".render.json")
        render.write_bytes(b"{}")
        capture = {"origin": "oc", "scenario": scenario, "path": "oc/" + scenario,
                   "render_sha256": hashlib.sha256(render.read_bytes()).hexdigest()}
        self.assertEqual(audit.temporal_renders(root, folder, "oc", "idle", [capture]), [render])
        alias = render.with_name(render.name + ".gz")
        alias.write_bytes(gzip.compress(render.read_bytes(), mtime=0))
        with self.assertRaises(AssertionError):
            audit.temporal_renders(root, folder, "oc", "idle", [capture])
