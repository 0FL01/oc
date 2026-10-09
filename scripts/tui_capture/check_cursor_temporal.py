"""Audit immutable real-terminal cursor receipts; no pixel-parity relaxation."""

import collections
import argparse
import hashlib
import gzip
import json
from pathlib import Path
import sys
import subprocess

from PIL import Image


def read(path):
    if path.suffix == ".gz":
        with gzip.open(path, "rb") as source:
            data = source.read(64 * 1024 * 1024 + 1)
        assert 0 < len(data) <= 64 * 1024 * 1024, path
        return json.loads(data)
    return json.loads(path.read_text())


def verified_frame(render_path, render):
    observation = render["observation"]
    cells_path = render_path.with_suffix("").with_suffix(".cells.json")
    version = render["schema_version"]
    assert type(version) is int and version in (1, 2), render_path
    if version == 1:
        assert "frame_file" not in observation and "frame_sha256" not in observation, render_path
        return observation["frame"], cells_path
    assert "frame" not in observation, render_path
    assert observation["frame_file"] in (cells_path.name, cells_path.name + ".gz"), render_path
    cells_path = render_path.parent / observation["frame_file"]
    assert cells_path.resolve().parent == render_path.parent.resolve(), render_path
    assert observation["frame_sha256"] == hashlib.sha256(cells_path.read_bytes()).hexdigest(), render_path
    return read(cells_path), cells_path


def main():
    reports = []
    rust_inputs = set()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compare-output", type=Path)
    parser.add_argument("roots", nargs="+")
    options = parser.parse_args()
    if options.compare_output:
        options.compare_output.mkdir(exist_ok=False)
    for argument in options.roots:
        root = Path(argument)
        lock = read(root / "capture.lock.json")
        manifest = read(root / "source-manifest.json")
        rust = {p: v for p, v in manifest.items() if p.startswith(("crates/", "Cargo")) or p == "rust-toolchain.toml"}
        rust_inputs.add(hashlib.sha256(json.dumps(rust, sort_keys=True).encode()).hexdigest())
        builds = [c for c in read(root / "commands.json") if c["argv"][0] == "cargo"]
        assert len(builds) == 1 and builds[0]["exit_code"] == 0, root
        report = {"attempt": root.name, "profile": lock["profile"], "sides": {},
                  "comparisons": dict(collections.Counter(a["status"] for a in lock["attempts"]))}
        phases = {}
        for side in ("upstream", "oc"):
            folder = root / side
            checks = read(folder / "cursor-temporal-checks.json")
            tool = read(folder / "tool-preview-checks.json")
            assert checks["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY", (root, side)
            assert tool["cursor_temporal"]["no_tool_replay"] is True, (root, side)
            pixels = 0
            frames = 0
            for state in checks["states"]:
                assert state["phantom_command_states"] == 0 and state["final_caret_preserved"], state
                assert state["cadence_preserved"] and state["adequate_raster_sampling"], state
                assert state["temporal_full_frames"] >= 10, state
                for render_path in sorted(folder.glob("cursor-" + state["state"] + "-phase-*.render.json")):
                    render = read(render_path)
                    assert render["actual_renderer_canvas"] and render["temporal_unsettled"], render_path
                    observation = render["observation"]
                    raster = observation["raster"]
                    phases.setdefault((state["state"], raster["visible"]), {}).setdefault(side, []).append(render_path)
                    grid, _ = verified_frame(render_path, render)
                    with Image.open(render_path.with_suffix("").with_suffix(".png")) as source:
                        image = source.convert("RGBA")
                        assert image.width % grid["columns"] == 0 and image.height % grid["rows"] == 0
                        x = raster["read_at"]["x"] * (image.width // grid["columns"]) + 1
                        y = raster["read_at"]["y"] * (image.height // grid["rows"]) + 1
                        assert list(image.getpixel((x, y))) == raster["rgba"], (render_path, raster)
                        assert image.getextrema()[3] == (255, 255), render_path
                        pixels += image.width * image.height
                    frames += 1
            report["sides"][side] = {"status": checks["status"], "cycles": [s["cycles"] for s in checks["states"]],
                                     "commands": [s["commands"] for s in checks["states"]],
                                     "full_temporal_pngs": frames, "full_raster_pixels_checked": pixels}
        report["matched_actual_raster_phases"] = []
        for (state, visible), sides in sorted(phases.items()):
            assert len(sides) == 2 and min(map(len, sides.values())) >= 3, (root, state, visible)
            for index, (left, right) in enumerate(zip(sides["upstream"], sides["oc"])):
                pair = {"state": state, "actual_raster_visible": visible, "upstream": str(left), "oc": str(right)}
                if options.compare_output:
                    pair["comparisons"] = {}
                    for mode in ("grid", "png"):
                        stem = f"{root.name}-{state}-{'visible' if visible else 'hidden'}-{index}-{mode}"
                        output = options.compare_output / (stem + ".json")
                        inputs = [verified_frame(p, read(p))[1] if mode == "grid" else p.with_suffix("").with_suffix(".png") for p in (left, right)]
                        result = subprocess.run([sys.executable, "tui-recovery/scripts/compare_frames.py", mode,
                                                 *map(str, inputs), "--report", str(output)],
                                                stdout=subprocess.PIPE, text=True, check=False)
                        assert result.returncode in (0, 1), (output, result.stdout)
                        pair["comparisons"][mode] = "EQUAL" if result.returncode == 0 else "DIFFERENT"
                report["matched_actual_raster_phases"].append(pair)
        reports.append(report)
    assert len(rust_inputs) == 1 and reports, "Profiles must use the same actual Rust inputs"
    result = {"status": "QUALIFIED_CURSOR_BEHAVIOR_ONLY", "pixel_parity": "NOT_PASS", "reports": reports}
    if options.compare_output:
        (options.compare_output / "report.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "pixel_parity": result["pixel_parity"], "reports": [{k: v for k, v in r.items() if k != "matched_actual_raster_phases"} | {"matched_pairs": len(r["matched_actual_raster_phases"])} for r in reports]}, indent=2))


if __name__ == "__main__":
    main()
