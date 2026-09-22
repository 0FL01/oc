"""Tests for comparison utility, NOT tests of OpenCode or its Rust port."""
from __future__ import annotations

from contextlib import redirect_stdout
from copy import deepcopy
from io import StringIO
import json
from pathlib import Path
import tempfile
import unittest

import compare_frames as cf

try:
    from PIL import Image
except ImportError:
    Image = None


def example(origin: str) -> dict:
    cell = {"symbol": " ", "fg": "#eeeeee", "bg": "#0a0a0a", "modifiers": [], "width": 1}
    return {"schema_version": 1, "origin": origin, "scenario": "utility-test-only",
            "fixture_sha256": "a" * 64, "environment_id": "utility-test-profile",
            "producer_commit": ("b" if origin == "upstream" else "c") * 40,
            "columns": 2, "rows": 2,
            "cursor": {"visible": True, "x": 0, "y": 1, "shape": "block"},
            "cells": [[deepcopy(cell) for _ in range(2)] for _ in range(2)]}


class GridTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.ref, self.actual = self.root / "ref.json", self.root / "actual.json"
        self.rd, self.ad = example("upstream"), example("oc")

    def write(self):
        self.ref.write_text(json.dumps(self.rd))
        self.actual.write_text(json.dumps(self.ad))

    def compare(self):
        self.write()
        return cf.compare_grid(self.ref, self.actual)

    def test_equal(self):
        result = self.compare()
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(result["cells_checked"], 4)

    def test_symbol_difference(self):
        self.ad["cells"][0][1]["symbol"] = "я"
        result = self.compare()
        self.assertEqual(result["status"], "FAIL")
        self.assertEqual(result["difference_bbox_inclusive"], [1, 0, 1, 0])

    def test_foreground_difference(self):
        self.ad["cells"][0][0]["fg"] = "#ffffff"
        self.assertEqual(self.compare()["different_cells"], 1)

    def test_background_on_blank_is_checked(self):
        self.ad["cells"][1][0]["bg"] = "#eeeeee"
        self.assertEqual(self.compare()["status"], "FAIL")

    def test_attributes_difference(self):
        self.ad["cells"][0][0]["modifiers"] = ["bold"]
        self.assertEqual(self.compare()["status"], "FAIL")

    def test_cursor_difference(self):
        self.ad["cursor"]["x"] = 1
        result = self.compare()
        self.assertEqual(result["different_cells"], 0)
        self.assertEqual(result["status"], "FAIL")
        self.assertTrue(result["cursor_differs"])

    def test_missing_style_invalid(self):
        del self.ad["cells"][0][0]["fg"]
        with self.assertRaises(cf.Invalid): self.compare()

    def test_environment_mismatch_invalid(self):
        self.ad["environment_id"] = "other-font"
        with self.assertRaises(cf.Invalid): self.compare()

    def test_fixture_mismatch_invalid(self):
        self.ad["fixture_sha256"] = "f" * 64
        with self.assertRaises(cf.Invalid): self.compare()

    def test_wrong_origin_invalid(self):
        self.ad["origin"] = "upstream"
        with self.assertRaises(cf.Invalid): self.compare()

    def test_geometry_mismatch_invalid(self):
        self.ad["rows"] = 1
        self.ad["cells"] = self.ad["cells"][:1]
        self.ad["cursor"]["y"] = 0
        with self.assertRaises(cf.Invalid): self.compare()

    def test_incomplete_grid_invalid(self):
        self.ad["cells"][0].pop()
        with self.assertRaises(cf.Invalid): self.compare()

    def test_valid_wide_pair(self):
        for value in (self.rd, self.ad):
            value["cells"][0][0].update(symbol="界", width=2)
            value["cells"][0][1].update(symbol="", width=0)
        self.assertEqual(self.compare()["status"], "PASS")

    def test_invalid_wide_pair(self):
        self.ad["cells"][0][1].update(symbol="界", width=2)
        with self.assertRaises(cf.Invalid): self.compare()

    def test_same_input_rejected(self):
        self.write()
        with self.assertRaises(cf.Invalid): cf.compare_grid(self.ref, self.ref)

    def test_report_cannot_overwrite_input(self):
        self.write()
        old = self.ref.read_bytes()
        with redirect_stdout(StringIO()):
            code = cf.main(["grid", str(self.ref), str(self.actual), "--report", str(self.ref)])
        self.assertEqual(code, 2)
        self.assertEqual(self.ref.read_bytes(), old)

    def test_cli_writes_failure_report_and_exit1(self):
        self.ad["cells"][1][0]["symbol"] = "x"
        self.write()
        report = self.root / "evidence/diff.json"
        with redirect_stdout(StringIO()):
            code = cf.main(["grid", str(self.ref), str(self.actual), "--report", str(report)])
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(report.read_text())["status"], "FAIL")

    def test_missing_input_is_not_pass(self):
        with redirect_stdout(StringIO()):
            code = cf.main(["grid", str(self.ref), str(self.actual)])
        self.assertEqual(code, 2)

    def test_bad_json_is_not_pass(self):
        self.write()
        self.actual.write_text("not json")
        with redirect_stdout(StringIO()):
            code = cf.main(["grid", str(self.ref), str(self.actual)])
        self.assertEqual(code, 2)


@unittest.skipIf(Image is None, "Pillow unavailable; PNG utility cases NOT RUN")
class PngTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.ref, self.actual = self.root / "ref.png", self.root / "actual.png"
        Image.new("RGB", (3, 2), (10, 10, 10)).save(self.ref)
        Image.new("RGB", (3, 2), (10, 10, 10)).save(self.actual)

    def test_equal_pixels(self):
        self.assertEqual(cf.compare_png(self.ref, self.actual)["status"], "PASS")

    def test_single_pixel_difference(self):
        image = Image.new("RGB", (3, 2), (10, 10, 10))
        image.putpixel((1, 1), (11, 10, 10))
        image.save(self.actual)
        result = cf.compare_png(self.ref, self.actual)
        self.assertEqual(result["status"], "FAIL")
        self.assertEqual(result["different_pixels"], 1)
        self.assertEqual(result["difference_bbox_inclusive"], [1, 1, 1, 1])

    def test_dimension_mismatch_is_invalid(self):
        Image.new("RGB", (4, 2)).save(self.actual)
        with self.assertRaises(cf.Invalid): cf.compare_png(self.ref, self.actual)

    def test_non_png_rejected(self):
        wrong = self.root / "actual.jpeg"
        Image.new("RGB", (3, 2)).save(wrong)
        with self.assertRaises(cf.Invalid): cf.compare_png(self.ref, wrong)


if __name__ == "__main__":
    unittest.main()
