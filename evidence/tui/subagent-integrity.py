"""Read-only current Subagent receipts audit; integrity is not pixel parity."""

import collections
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

from PIL import Image


REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "evidence/tui"
ATTEMPTS = {
    "subagent-cards-005": (30, {"DIFFERENT": 30}),
    "subagent-tool-regression-001": (
        56, {"DIFFERENT": 50, "EQUAL": 4, "NATIVE_ONLY_RESOURCE_DETAILS": 4}),
    "subagent-temporal-001": (160, {"DIFFERENT": 160}),
}


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, REPO / path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


comparator = module("strict_frames", "tui-recovery/scripts/compare_frames.py")
temporal = module("cursor_receipts", "scripts/tui_capture/check_cursor_temporal.py")


def read(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


reports = []
cells = 0
for name, (expected, comparison_counts) in ATTEMPTS.items():
    root = ROOT / name
    lock = read(root / "capture.lock.json")
    manifest = read(root / "source-manifest.json")
    canonical = subprocess.run([
        "node", "-e",
        "const fs=require('fs'),crypto=require('crypto');"
        "const v=JSON.parse(fs.readFileSync(0,'utf8'));"
        "const c=JSON.stringify(Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))));"
        "process.stdout.write(crypto.createHash('sha256').update(c).digest('hex'));",
    ], input=json.dumps(manifest), text=True, stdout=subprocess.PIPE, check=True).stdout
    assert len(manifest) == 532 and canonical == lock["oc"]["source_manifest_sha256"], name
    for path, recorded in manifest.items():
        assert digest(REPO / path) == recorded, (name, path)
    builds = [c for c in read(root / "commands.json") if c["argv"][0] == "cargo"]
    assert len(builds) == 1 and builds[0]["argv"][:3] == ["cargo", "build", "--locked"], name
    assert builds[0]["exit_code"] == 0, name
    for side in ("oc", "upstream"):
        assert digest(Path(lock[side]["executable_path"])) == lock[side]["executable_sha256"], name
        checks = read(root / side / "tool-preview-checks.json")
        if name == "subagent-temporal-001":
            assert checks["status"] == "DIAGNOSTIC_CURSOR_ONLY", (name, side)
            assert checks["cursor_temporal"]["no_tool_replay"], (name, side)
            cursor = read(root / side / "cursor-temporal-checks.json")
            assert cursor["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY", (name, side)
            assert len(cursor["states"]) == 6, (name, side)
            assert all(s["cycles"] == 0 and s["phantom_command_states"] == 0
                       and s["final_caret_preserved"] for s in cursor["states"]), (name, side)
        else:
            assert checks["status"] == "PASS_BEHAVIOR_ONLY", (name, side)
    captures = lock["captures"]
    assert len(captures) == expected, (name, len(captures))
    dimensions = collections.Counter()
    for capture in captures:
        stem = root / capture["path"]
        assert not any(word in capture["status"] for word in ("FAIL", "UNSTABLE")), capture
        assert capture["cells_file"] == stem.name + ".cells.json.gz", stem
        encoded = stem.parent / capture["cells_file"]
        assert digest(encoded) == capture["cells_sha256"], stem
        for suffix, key in ((".png", "png_sha256"), (".render.json", "render_sha256")):
            assert digest(Path(str(stem) + suffix)) == capture[key], (name, stem, key)
        assert stem.parent in (root / "oc", root / "upstream"), stem
        grid = comparator.validate_grid(
            comparator.decode_grid(comparator.read_bounded(encoded), encoded), stem.parent.name)
        cells += grid["columns"] * grid["rows"]
        render_path = Path(str(stem) + ".render.json")
        render = read(render_path)
        with Image.open(Path(str(stem) + ".png")) as source:
            image = source.convert("RGBA")
            shot = render["screenshot"]
            assert image.size == (shot["png_width"], shot["png_height"]), stem
            assert image.getextrema()[3] == (255, 255), stem
            if "clip" in shot:
                assert image.size == (shot["clip"]["width"], shot["clip"]["height"]), stem
                assert render["before"] == render["after"] and not render["layout_changed_during_screenshot"], stem
                assert (grid["columns"], grid["rows"]) == (render["before"]["columns"], render["before"]["rows"]), stem
            else:
                assert render["actual_renderer_canvas"] and render["temporal_unsettled"], stem
                frame, reference = temporal.verified_frame(render_path, render)
                assert reference == encoded and frame == grid, stem
            dimensions[f"{image.width}x{image.height}"] += 1
    comparisons = collections.Counter(a["status"] for a in lock["attempts"])
    assert comparisons == comparison_counts, (name, comparisons)
    if name == "subagent-tool-regression-001":
        facts = read(root / "oc/tool-preview-checks.json")["owner_facts"]
        assert facts["operation_count"] == 4 and facts["presentation_count"] == 5 and facts["frozen_foreground_shell"], facts
        assert dimensions["1011x1280"] == 14 and dimensions["1011x1920"] == 2, dimensions
    reports.append({"attempt": name, "source_entries": len(manifest), "captures": len(captures),
                    "dimensions": dict(dimensions), "comparisons": dict(comparisons)})

print(json.dumps({"status": "QUALIFIED_INTEGRITY_ONLY", "pixel_parity": "NOT_PASS",
                  "captures": sum(r["captures"] for r in reports), "validated_cells": cells,
                  "seals": 3 * sum(r["captures"] for r in reports), "reports": reports}, indent=2))
