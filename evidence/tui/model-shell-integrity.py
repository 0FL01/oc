"""Read-only audit of this slice's immutable full-frame receipts, not parity PASS."""

import collections
import hashlib
import json
from pathlib import Path
import subprocess

from PIL import Image


REPO = Path(__file__).resolve().parents[2]
ROOT = REPO / "evidence/tui"
ATTEMPTS = {
    "model-shell-023": 16,
    "model-shell-024": 16,
    "model-shell-025": 16,
    "model-shell-child-regression-009": 40,
    "model-shell-child-regression-010": 40,
    "model-shell-child-regression-011": 40,
    "model-shell-tool-regression-006": 56,
    "model-shell-temporal-002": 160,
}


def read(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


reports = []
for name, expected in ATTEMPTS.items():
    root = ROOT / name
    lock = read(root / "capture.lock.json")
    manifest = read(root / "source-manifest.json")
    # The runner canonicalizes with Node's localeCompare, not Python/ASCII order.
    canonical_digest = subprocess.run([
        "node", "-e",
        "const fs=require('fs'),crypto=require('crypto');"
        "const v=JSON.parse(fs.readFileSync(0,'utf8'));"
        "const c=JSON.stringify(Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))));"
        "process.stdout.write(crypto.createHash('sha256').update(c).digest('hex'));"
    ], input=json.dumps(manifest), text=True, stdout=subprocess.PIPE, check=True).stdout
    assert canonical_digest == lock["oc"]["source_manifest_sha256"], name
    for path, recorded in manifest.items():
        assert digest(REPO / path) == recorded, (name, path)
    builds = [c for c in read(root / "commands.json") if c["argv"][0] == "cargo"]
    assert len(builds) == 1 and builds[0]["argv"][:3] == ["cargo", "build", "--locked"], name
    assert builds[0]["exit_code"] == 0, name
    for side in ("oc", "upstream"):
        assert digest(Path(lock[side]["executable_path"])) == lock[side]["executable_sha256"], name
        checks = read(root / side / "tool-preview-checks.json")
        if name == "model-shell-temporal-002":
            assert checks["status"] == "DIAGNOSTIC_CURSOR_ONLY", (name, side)
            assert checks["cursor_temporal"]["no_tool_replay"] is True, (name, side)
            assert read(root / side / "cursor-temporal-checks.json")["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY", (name, side)
        else:
            assert checks["status"] == "PASS_BEHAVIOR_ONLY", (name, side)
    captures = lock["captures"]
    assert len(captures) == expected, (name, len(captures))
    dimensions = collections.Counter()
    for capture in captures:
        stem = root / capture["path"]
        assert "FAIL" not in capture["status"] and "UNSTABLE" not in capture["status"], capture
        for suffix, key in ((".cells.json", "cells_sha256"), (".png", "png_sha256"), (".render.json", "render_sha256")):
            assert digest(Path(str(stem) + suffix)) == capture[key], (name, stem, key)
        grid = read(Path(str(stem) + ".cells.json"))
        render = read(Path(str(stem) + ".render.json"))
        assert len(grid["cells"]) == grid["rows"], stem
        assert all(len(row) == grid["columns"] for row in grid["cells"]), stem
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
                assert render["observation"]["frame"]["cells"] == grid["cells"], stem
            dimensions[f"{image.width}x{image.height}"] += 1
    comparisons = collections.Counter(a["status"] for a in lock["attempts"])
    assert set(comparisons) <= {"EQUAL", "DIFFERENT", "NATIVE_ONLY_RESOURCE_DETAILS"}, name
    if name == "model-shell-tool-regression-006":
        assert comparisons == {"EQUAL": 4, "DIFFERENT": 50, "NATIVE_ONLY_RESOURCE_DETAILS": 4}, name
        facts = read(root / "oc/tool-preview-checks.json")["owner_facts"]
        assert facts["operation_count"] == 4 and facts["presentation_count"] == 5 and facts["frozen_foreground_shell"], facts
        assert dimensions["1011x1280"] == 14 and dimensions["1011x1920"] == 2, dimensions
    else:
        assert comparisons == {"DIFFERENT": expected}, (name, comparisons)
    reports.append({"attempt": name, "source_entries": len(manifest), "captures": len(captures),
                    "dimensions": dict(dimensions), "comparisons": dict(comparisons)})

print(json.dumps({"status": "QUALIFIED_INTEGRITY_ONLY", "pixel_parity": "NOT_PASS",
                  "captures": sum(r["captures"] for r in reports), "reports": reports}, indent=2))
