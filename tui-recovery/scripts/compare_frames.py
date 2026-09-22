#!/usr/bin/env python3
"""Compare already captured terminal frames; never capture or attest a program run.

Exit 0: equal; 1: comparable but different; 2: invalid/incomparable inputs.
Grid mode is standard-library only. PNG mode requires Pillow. No masks/resizing.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile
from typing import Any

MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_CELLS = 250_000
MAX_PIXELS = 20_000_000
MODIFIERS = {"bold", "dim", "italic", "underlined", "slow_blink", "rapid_blink",
             "reversed", "hidden", "crossed_out"}


class Invalid(ValueError):
    """Inputs cannot support a valid comparison."""


def checked_paths(reference: Path, actual: Path, report: Path | None = None) -> None:
    if reference.resolve() == actual.resolve():
        raise Invalid("Reference and actual must be different files")
    if reference.exists() and actual.exists() and os.path.samefile(reference, actual):
        raise Invalid("Reference and actual refer to the same inode")
    if report is not None:
        if report.resolve() in {reference.resolve(), actual.resolve()}:
            raise Invalid("Report would overwrite an input")
        if report.exists() and any(p.exists() and os.path.samefile(report, p)
                                   for p in (reference, actual)):
            raise Invalid("Report is an input alias")


def read_bounded(path: Path) -> bytes:
    if not path.is_file() or not 0 < path.stat().st_size <= MAX_FILE_BYTES:
        raise Invalid(f"Missing, empty or oversized input: {path.name}")
    with path.open("rb") as source:
        data = source.read(MAX_FILE_BYTES + 1)
    if not data or len(data) > MAX_FILE_BYTES:
        raise Invalid(f"Empty or oversized input: {path.name}")
    return data


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def positive_int(value: Any) -> bool:
    return type(value) is int and value > 0


def validate_grid(data: Any, origin: str) -> dict[str, Any]:
    if (not isinstance(data, dict) or type(data.get("schema_version")) is not int
            or data.get("schema_version") != 1):
        raise Invalid("Expected grid schema_version=1")
    if data.get("origin") != origin:
        raise Invalid(f"Expected origin={origin}")
    for key in ("scenario", "environment_id"):
        if not isinstance(data.get(key), str) or not data[key].strip():
            raise Invalid(f"Missing {key}")
    for key, length in (("fixture_sha256", 64), ("producer_commit", 40)):
        if not isinstance(data.get(key), str) or not re.fullmatch(rf"[0-9a-f]{{{length}}}", data[key]):
            raise Invalid(f"Invalid {key}")
    cols, rows = data.get("columns"), data.get("rows")
    if not positive_int(cols) or not positive_int(rows) or cols * rows > MAX_CELLS:
        raise Invalid("Invalid or oversized grid dimensions")
    cells = data.get("cells")
    if not isinstance(cells, list) or len(cells) != rows:
        raise Invalid("Grid row count mismatch")
    for row in cells:
        if not isinstance(row, list) or len(row) != cols:
            raise Invalid("Grid column count mismatch")
        for x, cell in enumerate(row):
            if not isinstance(cell, dict) or set(cell) != {"symbol", "fg", "bg", "modifiers", "width"}:
                raise Invalid("Each cell needs exact symbol/fg/bg/modifiers/width fields")
            if not isinstance(cell["symbol"], str) or len(cell["symbol"].encode("utf-8")) > 512:
                raise Invalid("Invalid cell symbol")
            for key in ("fg", "bg"):
                if not isinstance(cell[key], str) or not re.fullmatch(r"#[0-9a-f]{6}", cell[key]):
                    raise Invalid("Cell colors must be resolved lowercase #rrggbb")
            mods = cell["modifiers"]
            if not isinstance(mods, list) or any(not isinstance(m, str) for m in mods):
                raise Invalid("Invalid cell modifiers")
            if mods != sorted(set(mods)) or not set(mods) <= MODIFIERS:
                raise Invalid("Modifiers must be sorted, unique and supported")
            if type(cell["width"]) is not int or cell["width"] not in (0, 1, 2):
                raise Invalid("Cell width must be 0, 1 or 2")
            if cell["width"] == 0:
                if x == 0 or row[x - 1].get("width") != 2 or cell["symbol"] != "":
                    raise Invalid("Invalid wide-cell continuation")
            elif not cell["symbol"]:
                raise Invalid("Non-continuation cells need a symbol, including blanks")
            if cell["width"] == 2 and (x + 1 >= cols or not isinstance(row[x + 1], dict)
                                       or row[x + 1].get("width") != 0):
                raise Invalid("Wide cell must be followed by a continuation")
    cursor = data.get("cursor")
    if not isinstance(cursor, dict) or set(cursor) != {"visible", "x", "y", "shape"}:
        raise Invalid("Cursor needs exact visible/x/y/shape fields")
    if type(cursor["visible"]) is not bool or cursor["shape"] not in ("block", "bar", "underline"):
        raise Invalid("Invalid cursor visibility/shape")
    for key, bound in (("x", cols), ("y", rows)):
        if type(cursor[key]) is not int or not 0 <= cursor[key] < bound:
            raise Invalid("Cursor outside grid")
    return data


def compare_grid(reference: Path, actual: Path) -> dict[str, Any]:
    checked_paths(reference, actual)
    raw_ref, raw_actual = read_bounded(reference), read_bounded(actual)
    ref = validate_grid(json.loads(raw_ref), "upstream")
    got = validate_grid(json.loads(raw_actual), "oc")
    for key in ("scenario", "fixture_sha256", "environment_id", "columns", "rows"):
        if ref[key] != got[key]:
            raise Invalid(f"Incomparable frames: different {key}")
    count = 0
    samples = []
    left, top, right, bottom = ref["columns"], ref["rows"], -1, -1
    for y, (rr, ar) in enumerate(zip(ref["cells"], got["cells"])):
        for x, (rc, ac) in enumerate(zip(rr, ar)):
            if rc != ac:
                count += 1
                left, top, right, bottom = min(left, x), min(top, y), max(right, x), max(bottom, y)
                if len(samples) < 20:
                    samples.append({"x": x, "y": y,
                                    "different_fields": [k for k in rc if rc[k] != ac[k]]})
    cursor_differs = ref["cursor"] != got["cursor"]
    return {"kind": "grid", "status": "PASS" if count == 0 and not cursor_differs else "FAIL",
            "scope": "frame equality only; capture provenance is not attested",
            "scenario": ref["scenario"], "fixture_sha256": ref["fixture_sha256"],
            "environment_id": ref["environment_id"],
            "reference_sha256": sha(raw_ref), "actual_sha256": sha(raw_actual),
            "reference_commit": ref["producer_commit"], "actual_commit": got["producer_commit"],
            "cells_checked": ref["columns"] * ref["rows"], "different_cells": count,
            "cursor_differs": cursor_differs,
            "difference_bbox_inclusive": [left, top, right, bottom] if count else None,
            "samples": samples}


def compare_png(reference: Path, actual: Path) -> dict[str, Any]:
    checked_paths(reference, actual)
    raw_ref, raw_actual = read_bounded(reference), read_bounded(actual)
    try:
        from PIL import Image
    except ImportError as exc:
        raise Invalid("PNG comparison requires Pillow in the test environment") from exc
    from io import BytesIO
    try:
        ref_image, actual_image = Image.open(BytesIO(raw_ref)), Image.open(BytesIO(raw_actual))
    except Image.DecompressionBombError as exc:
        raise Invalid("Image dimensions exceed safe decoder limits") from exc
    with ref_image as r, actual_image as a:
        if r.format != "PNG" or a.format != "PNG":
            raise Invalid("PNG mode requires two PNG images")
        if r.size != a.size:
            raise Invalid("Incomparable images: different pixel dimensions (no resizing)")
        if r.width * r.height > MAX_PIXELS or getattr(r, "n_frames", 1) != 1 or getattr(a, "n_frames", 1) != 1:
            raise Invalid("Image exceeds comparison bounds or is animated")
        width, height = r.size
        rb, ab = r.convert("RGBA").tobytes(), a.convert("RGBA").tobytes()
    count, left, top, right, bottom = 0, width, height, -1, -1
    for offset in range(0, len(rb), 4):
        if rb[offset:offset + 4] != ab[offset:offset + 4]:
            count += 1
            index = offset // 4
            x, y = index % width, index // width
            left, top, right, bottom = min(left, x), min(top, y), max(right, x), max(bottom, y)
    return {"kind": "png", "status": "PASS" if count == 0 else "FAIL",
            "scope": "decoded RGBA equality only; use capture lock for provenance/profile",
            "reference_sha256": sha(raw_ref), "actual_sha256": sha(raw_actual),
            "width": width, "height": height, "pixels_checked": width * height,
            "different_pixels": count, "changed_fraction_diagnostic_only": count / (width * height),
            "difference_bbox_inclusive": [left, top, right, bottom] if count else None}


def save_report(path: Path, result: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".frame-report-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as out:
            json.dump(result, out, ensure_ascii=False, indent=2)
            out.write("\n")
            out.flush()
            os.fsync(out.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("grid", "png"))
    parser.add_argument("reference", type=Path)
    parser.add_argument("actual", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args(argv)
    safe_report = False
    try:
        checked_paths(args.reference, args.actual, args.report)
        safe_report = True
        result = (compare_grid if args.kind == "grid" else compare_png)(args.reference, args.actual)
        code = 0 if result["status"] == "PASS" else 1
    except (Invalid, ValueError, OSError, TypeError, KeyError, RecursionError) as exc:
        result = {"kind": args.kind, "status": "INVALID", "reason": str(exc),
                  "scope": "no valid frame comparison; not a product PASS"}
        code = 2
    if args.report and safe_report:
        try:
            save_report(args.report, result)
        except OSError as exc:
            print(json.dumps({"status": "INVALID", "reason": f"Cannot write report: {exc}"}))
            return 2
    print(json.dumps(result, ensure_ascii=False))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
