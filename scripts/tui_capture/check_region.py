#!/usr/bin/env python3
"""Diagnostic for a captured rectangular grid; full-frame comparator remains authoritative."""
import argparse
from collections import Counter
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tui-recovery/scripts"))
from compare_frames import validate_grid  # noqa: E402


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("reference", type=Path)
    cli.add_argument("actual", type=Path)
    cli.add_argument("--rect", type=int, nargs=4, required=True, metavar=("X", "Y", "WIDTH", "HEIGHT"))
    args = cli.parse_args()
    reference = validate_grid(json.loads(args.reference.read_text()), "upstream")
    actual = validate_grid(json.loads(args.actual.read_text()), "oc")
    for key in ("scenario", "fixture_sha256", "environment_id", "columns", "rows"):
        if reference[key] != actual[key]:
            raise ValueError(f"Incomparable frames: {key}")
    x, y, width, height = args.rect
    if not (0 <= x < reference["columns"] and 0 <= y < reference["rows"]
            and width > 0 and height > 0 and x + width <= reference["columns"]
            and y + height <= reference["rows"]):
        raise ValueError("Invalid rectangle")
    symbols = styles = 0
    by_field = {}
    color_pairs = Counter()
    examples = []
    modifier_examples = []
    for row in range(y, y + height):
        for col in range(x, x + width):
            left = reference["cells"][row][col]
            right = actual["cells"][row][col]
            symbols += left["symbol"] != right["symbol"] or left["width"] != right["width"]
            styles += any(left[key] != right[key] for key in ("fg", "bg", "modifiers"))
            for key in left:
                if left[key] != right[key]:
                    by_field[key] = by_field.get(key, 0) + 1
            if left["fg"] != right["fg"]:
                color_pairs[(left["fg"], right["fg"])] += 1
                if len(examples) < 12:
                    examples.append({"x": col, "y": row, "symbol": left["symbol"],
                                     "reference_fg": left["fg"], "oc_fg": right["fg"]})
            if left["modifiers"] != right["modifiers"] and len(modifier_examples) < 12:
                modifier_examples.append({"x": col, "y": row, "symbol": left["symbol"],
                                          "reference": left["modifiers"], "oc": right["modifiers"]})
    print(json.dumps({"scope": "diagnostic region of already captured paired grids; no parity attestation",
                      "rect": args.rect, "cells": width * height,
                      "symbol_or_width_differences": symbols, "styled_differences": styles,
                      "different_by_field": by_field,
                      "foreground_examples": examples,
                      "modifier_examples": modifier_examples,
                      "top_foreground_pairs": [{"reference": ref, "oc": oc, "cells": count}
                                               for (ref, oc), count in color_pairs.most_common(8)]}, sort_keys=True))


if __name__ == "__main__":
    main()
