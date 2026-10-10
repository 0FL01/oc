"""Fresh-only complete proof archive; no source/history or comparator changes.

Usage: pack SCRATCH ARCHIVE RECEIPT; audit ARCHIVE RECEIPT; root CAPTURE_ROOT.
Scratch is transient acquisition, not an alternate durable evidence location.
Every member remains byte-exact; only fresh same-run/same-side PNG storage aliases.
"""
import collections
import hashlib
import importlib.util
import io
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile

from PIL import Image

REPO = Path(__file__).resolve().parents[2]
MAX_MEMBER = 16 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
NAMES = ("ordinary", "temporal", "matched")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def safe(name):
    path = PurePosixPath(name)
    assert not path.is_absolute() and ".." not in path.parts
    assert path.parts[0] in NAMES and str(path) == name
    return path


def inventory(files, links):
    entries = [[p, len(v), sha(v), links.get(p)] for p, v in sorted(files.items())]
    return {"members": len(entries), "logical_bytes": sum(len(v) for v in files.values()),
            "member_inventory_sha256": sha(json.dumps(entries, separators=(",", ":")).encode()),
            "same_side_png_aliases": len(links)}


def unpack(encoded):
    decoder = lzma.LZMADecompressor(memlimit=32 * 1024 * 1024)
    raw = decoder.decompress(encoded, max_length=MAX_TOTAL + 1)
    assert len(raw) <= MAX_TOTAL and decoder.eof and not decoder.unused_data
    files, links = {}, {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as source:
        for item in source:
            path = safe(item.name)
            assert item.name not in files and item.name not in links and not item.isdir()
            if item.islnk():
                target = safe(item.linkname)
                assert path.parts[:2] == target.parts[:2] and path.parts[1] in ("oc", "upstream")
                assert item.name.endswith(".png") and item.linkname.endswith(".png")
                links[item.name] = item.linkname
            else:
                assert item.isfile() and 0 <= item.size <= MAX_MEMBER
                files[item.name] = source.extractfile(item).read(MAX_MEMBER + 1)
                assert len(files[item.name]) == item.size
            assert len(files) + len(links) <= 2000
    for name, target in links.items():
        assert target in files and target not in links
        files[name] = files[target]
    assert sum(len(v) for v in files.values()) <= MAX_TOTAL
    return files, links, len(raw)


def fresh_files(scratch):
    scratch = scratch.resolve()
    assert scratch.is_relative_to(Path("/home/opencode/.cache/opencode-tmp/opencode"))
    files = {}
    for name in NAMES:
        base = scratch / name
        assert base.is_dir() and not base.is_symlink() and base.resolve().parent == scratch
        for path in base.rglob("*"):
            assert not path.is_symlink()
            if path.is_file():
                assert path.stat().st_size <= MAX_MEMBER
                files[str(path.relative_to(scratch))] = path.read_bytes()
    assert len(files) <= 2000 and sum(map(len, files.values())) <= MAX_TOTAL
    return files


def durable_write(path, data):
    with path.open("xb") as target:
        target.write(data)
        target.flush()
        os.fsync(target.fileno())
    descriptor = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def audit_frames(files):
    spec = importlib.util.spec_from_file_location("strict_frames", REPO / "tui-recovery/scripts/compare_frames.py")
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    # Feed the unchanged strict comparator real distinct bounded private files;
    # no validation mocking, alternate equality oracle or report expectations.
    def compare(left, right, report, mode):
        assert safe(left).parts[1] == "upstream" and safe(right).parts[1] == "oc"
        with tempfile.TemporaryDirectory(prefix="root-compare-", dir="/home/opencode/.cache/opencode-tmp/opencode") as folder:
            reference, actual = Path(folder) / Path(left).name, Path(folder) / ("oc-" + Path(right).name)
            reference.write_bytes(files[left])
            actual.write_bytes(files[right])
            observed = (comparator.compare_grid if mode == "grid" else comparator.compare_png)(reference, actual)
        recorded = json.loads(files[report])
        assert observed == recorded, report
        return "EQUAL" if observed["status"] == "PASS" else "DIFFERENT"
    reports = []
    for name, count, statuses in (("ordinary", 56, {"DIFFERENT": 50, "EQUAL": 4, "NATIVE_ONLY_RESOURCE_DETAILS": 4}),
                                  ("temporal", 160, {"DIFFERENT": 160})):
        read = lambda p: json.loads(files[name + "/" + p])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        canonical = subprocess.run(["node", "-e", "const fs=require('fs'),c=require('crypto');const x=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(c.createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(x).sort(([a],[b])=>a.localeCompare(b))))).digest('hex'));"],
                                   input=json.dumps(manifest), text=True, stdout=subprocess.PIPE, check=True).stdout
        assert len(manifest) == 536 and canonical == lock["oc"]["source_manifest_sha256"]
        assert all(file_sha(REPO / p) == d for p, d in manifest.items())
        builds = [c for c in read("commands.json") if c["argv"][0] == "cargo"]
        assert len(builds) == 1 and builds[0]["argv"][:3] == ["cargo", "build", "--locked"] and builds[0]["exit_code"] == 0
        assert len(lock["captures"]) == count and collections.Counter(a["status"] for a in lock["attempts"]) == statuses
        dimensions = collections.Counter()
        for capture in lock["captures"]:
            side, stem = capture["path"].split("/")
            assert side == capture["origin"] and side in ("oc", "upstream")
            assert not any(x in capture["status"] for x in ("FAIL", "UNSTABLE"))
            assert capture["cells_file"] == stem + ".cells.json"
            data = read(side + "/" + capture["cells_file"])
            grid = comparator.validate_grid(data, side)
            for field, suffix in (("cells", ".cells.json"), ("png", ".png"), ("render", ".render.json"), ("text", ".txt")):
                assert sha(files[name + "/" + side + "/" + stem + suffix]) == capture[field + "_sha256"]
            if "vt_file" in capture:
                assert capture["vt_file"] == stem + ".vt"
                assert sha(files[name + "/" + side + "/" + capture["vt_file"]]) == capture["vt_sha256"]
            render = read(side + "/" + stem + ".render.json")
            with Image.open(io.BytesIO(files[name + "/" + side + "/" + stem + ".png"])) as image:
                image = image.convert("RGBA")
                assert image.size == (render["screenshot"]["png_width"], render["screenshot"]["png_height"])
                assert image.getextrema()[3] == (255, 255)
                dimensions[str(image.size)] += 1
                if "clip" in render["screenshot"]:
                    assert image.size == (render["screenshot"]["clip"]["width"], render["screenshot"]["clip"]["height"])
                    assert render["before"] == render["after"] and not render["layout_changed_during_screenshot"]
                else:
                    assert render["actual_renderer_canvas"] and render["temporal_unsettled"]
                    observation = render["observation"]
                    assert type(render["schema_version"]) is int and render["schema_version"] == 2
                    assert "frame" not in observation and observation["frame_file"] == capture["cells_file"]
                    assert observation["frame_sha256"] == capture["cells_sha256"]
                    raster = observation["raster"]
                    x = raster["read_at"]["x"] * (image.width // grid["columns"]) + 1
                    y = raster["read_at"]["y"] * (image.height // grid["rows"]) + 1
                    assert list(image.getpixel((x, y))) == raster["rgba"]
        for side in ("oc", "upstream"):
            checks = read(side + "/tool-preview-checks.json")
            assert file_sha(Path(lock[side]["executable_path"])) == lock[side]["executable_sha256"]
            if name == "ordinary":
                assert checks["status"] == "PASS_BEHAVIOR_ONLY"
                if side == "oc":
                    facts = checks["owner_facts"]
                    assert (facts["operation_count"], facts["presentation_count"], facts["frozen_foreground_shell"]) == (4, 5, True)
            else:
                assert checks["status"] == "DIAGNOSTIC_CURSOR_ONLY" and checks["cursor_temporal"]["no_tool_replay"]
                cursor = read(side + "/cursor-temporal-checks.json")
                assert cursor["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY" and len(cursor["states"]) == 6
                assert sum(s["temporal_full_frames"] for s in cursor["states"]) == 72
                assert all(s["cycles"] == 0 and s["phantom_command_states"] == 0 and s["final_caret_preserved"] for s in cursor["states"])
                for state in cursor["states"]:
                    assert state["trace_file"] == "cursor-" + state["state"] + ".trace.json"
                    trace_bytes = files[name + "/" + side + "/" + state["trace_file"]]
                    assert sha(trace_bytes) == state["trace_sha256"]
                    trace = json.loads(trace_bytes)
                    assert len(trace["samples"]) == state["samples"] and len(trace["commands"]) == state["commands"]
                    owner = trace["input_owner"]
                    assert all(not c["cursor"]["visible"] or c["cursor"]["synchronized"] or
                               (c["cursor"]["x"], c["cursor"]["y"]) == (owner["x"], owner["y"])
                               for c in trace["commands"])
                    phases = [s["raster"]["visible"] for s in trace["samples"]]
                    rises = [i for i in range(1, len(phases)) if phases[i] and not phases[i - 1]]
                    assert max(0, len(rises) - 1) == state["cycles"] == 0
                    assert read(side + "/cursor-" + state["state"] + ".cells.json")["cursor"] == owner
        for attempt in lock["attempts"]:
            if attempt["status"] == "NATIVE_ONLY_RESOURCE_DETAILS":
                continue
            scenario, mode = attempt["scenario"], attempt["mode"]
            suffix = ".cells.json" if mode == "grid" else ".png"
            assert compare(name + "/upstream/" + scenario + suffix, name + "/oc/" + scenario + suffix,
                           name + "/" + scenario + "." + mode + "-diff.json", mode) == attempt["status"]
        reports.append({"episode": name, "captures": count, "source_manifest_sha256": canonical, "dimensions": dict(dimensions), "comparisons": statuses})
    matched = json.loads(files["matched/report.json"])
    assert matched["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY" and len(matched["reports"][0]["matched_actual_raster_phases"]) == 72
    indices = collections.Counter()
    for pair in matched["reports"][0]["matched_actual_raster_phases"]:
        state, visible = pair["state"], pair["actual_raster_visible"]
        index = indices[state, visible]
        indices[state, visible] += 1
        roots = []
        prefix = "/home/opencode/.cache/opencode-tmp/opencode/root-regressions-001/"
        for side in ("upstream", "oc"):
            assert pair[side].startswith(prefix)
            path = pair[side][len(prefix):]
            assert safe(path).parts[:2] == ("temporal", side) and path.endswith(".render.json")
            roots.append(path[:-len(".render.json")])
        for mode in ("grid", "png"):
            suffix = ".cells.json" if mode == "grid" else ".png"
            report = f"matched/temporal-{state}-{'visible' if visible else 'hidden'}-{index}-{mode}.json"
            assert compare(roots[0] + suffix, roots[1] + suffix, report, mode) == pair["comparisons"][mode]
    return reports


def audit_root(root):
    """Verify the separately retained complete Root episode without rewriting it."""
    assert root.resolve() == REPO / "evidence/tui/root-handoff-003"
    spec = importlib.util.spec_from_file_location("strict_root", REPO / "tui-recovery/scripts/compare_frames.py")
    comparator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(comparator)
    spec = importlib.util.spec_from_file_location("root_sidecars", REPO / "scripts/tui_capture/check_cursor_temporal.py")
    sidecars = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(sidecars)
    read = lambda p: sidecars.read(root / p)
    lock, manifest = read("capture.lock.json"), read("source-manifest.json")
    assert len(manifest) == 536 and all(file_sha(REPO / p) == d for p, d in manifest.items())
    canonical = subprocess.run(["node", "-e", "const fs=require('fs'),c=require('crypto');const x=JSON.parse(fs.readFileSync(0,'utf8'));process.stdout.write(c.createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(x).sort(([a],[b])=>a.localeCompare(b))))).digest('hex'));"], input=json.dumps(manifest), text=True, stdout=subprocess.PIPE, check=True).stdout
    assert canonical == lock["oc"]["source_manifest_sha256"]
    builds = [c for c in read("commands.json") if c["argv"][0] == "cargo"]
    assert len(builds) == 1 and builds[0]["argv"][:3] == ["cargo", "build", "--locked"] and builds[0]["exit_code"] == 0
    assert len(lock["captures"]) == 20 and len(lock["attempts"]) == 20
    grids, dimensions = {}, collections.Counter()
    for capture in lock["captures"]:
        side, stem = capture["path"].split("/")
        assert side == capture["origin"] and side in ("oc", "upstream")
        assert capture["status"] == "CAPTURED_TOOL_PREVIEW"
        for field in ("cells", "render", "text", "vt", "png"):
            filename = stem + ".png" if field == "png" else capture[field + "_file"]
            assert Path(filename).name == filename and filename.startswith(stem + ".")
            path = root / side / filename
            assert path.stat().st_size <= MAX_MEMBER and file_sha(path) == capture[field + "_sha256"]
        grid_path = root / side / capture["cells_file"]
        grid = comparator.validate_grid(comparator.decode_grid(comparator.read_bounded(grid_path), grid_path), side)
        grids[side, stem] = grid
        render = read(side + "/" + capture["render_file"])
        assert render["before"] == render["after"] and not render["layout_changed_during_screenshot"]
        with Image.open(root / side / (stem + ".png")) as image:
            assert image.convert("RGBA").getextrema()[3] == (255, 255)
            assert image.size == (render["screenshot"]["clip"]["width"], render["screenshot"]["clip"]["height"])
            dimensions[str(image.size)] += 1
        if "child-hint" in stem or "shell-hint" in stem:
            line = next(row for row in grid["cells"] if "Press ctrl+y to move running work" in "".join(cell["symbol"] for cell in row))
            text = "".join(cell["symbol"] for cell in line)
            start = text.index("Press ")
            assert all(cell["fg"] == "#808080" for cell in line[start:start + 5])
            assert all(cell["fg"] == "#eeeeee" for cell in line[start + 6:start + 12])
    for attempt in lock["attempts"]:
        stem, mode = attempt["scenario"], attempt["mode"]
        suffix = ".cells.json.gz" if mode == "grid" else ".png"
        observed = (comparator.compare_grid if mode == "grid" else comparator.compare_png)(root / "upstream" / (stem + suffix), root / "oc" / (stem + suffix))
        assert observed == read(stem + "." + mode + "-diff.json") and observed["status"] == "FAIL"
        assert attempt["status"] == "DIFFERENT" and attempt["exit_code"] == 1
    requests = {}
    for side in ("oc", "upstream"):
        assert file_sha(Path(lock[side]["executable_path"])) == lock[side]["executable_sha256"]
        checks = read(side + "/tool-preview-checks.json")
        proof = checks["composer"]
        assert checks["status"] == proof["status"] == "PASS_BEHAVIOR_ONLY"
        assert proof["actual_calls"] == 3 and proof["root_prompt_remapped_key"] == "ctrl+y"
        assert proof["preserved_source_and_process"] and proof["nested_child_shell_not_root_target"] and proof["no_effect_replay"]
        requests[side] = proof["actual_requests"]
        grid = grids[side, "tool-preview-root-background-footer-120"]
        footers = [row for row in grid["cells"] if "Build · Fixture Caption Model ·" in "".join(cell["symbol"] for cell in row)]
        assert footers and all(next(cell for cell in row if cell["symbol"] == "B")["fg"] == "#12ab34" for row in footers)
    return {"captures": 20, "seals": 100, "comparisons": {"DIFFERENT": 20}, "dimensions": dict(dimensions), "source_manifest_sha256": canonical, "requests": requests,
            "remaining_accepted_user_border": {side: grids[side, "tool-preview-root-child-hint-80"]["cells"][3][2]["fg"] for side in ("oc", "upstream")}}


def main():
    mode, *args = sys.argv[1:]
    if mode == "pack":
        scratch, archive, receipt = map(Path, args)
        files = fresh_files(scratch)
        compressor = lzma.LZMACompressor(check=lzma.CHECK_SHA256, filters=[{"id": lzma.FILTER_LZMA2, "preset": 6 | lzma.PRESET_EXTREME, "dict_size": 16 * 1024 * 1024, "lc": 3, "lp": 0, "pb": 0}])
        output = io.BytesIO()
        class Sink:
            def write(self, data):
                output.write(compressor.compress(data))
                return len(data)
        links, pngs = {}, {}
        for path in sorted(files):
            if path.endswith(".png"):
                key = (safe(path).parts[:2], sha(files[path]))
                if key in pngs and files[pngs[key]] == files[path]:
                    links[path] = pngs[key]
                else:
                    pngs[key] = path
        def order(path):
            suffixes = (".cells.json", ".png", ".render.json", ".vt", ".txt", ".trace.json")
            category = next((i for i, suffix in enumerate(suffixes) if path.endswith(suffix)), len(suffixes))
            return category, path.rsplit("/", 1)[-1], path
        with tarfile.open(fileobj=Sink(), mode="w|", format=tarfile.USTAR_FORMAT) as target:
            for path in sorted(files, key=order):
                data = files[path]
                item = tarfile.TarInfo(path)
                item.mode = 0o644
                if path in links:
                    assert safe(path).parts[1] in ("oc", "upstream")
                    item.type, item.linkname = tarfile.LNKTYPE, links[path]
                    target.addfile(item)
                else:
                    item.size = len(data)
                    target.addfile(item, io.BytesIO(data))
        output.write(compressor.flush())
        del compressor
        encoded = output.getvalue()
        restored, restored_links, tar_bytes = unpack(encoded)
        assert restored == files and restored_links == links
        result = inventory(restored, links) | {"archive_sha256": sha(encoded), "archive_bytes": len(encoded), "tar_bytes": tar_bytes, "reports": audit_frames(restored)}
        data = (json.dumps(result, indent=2) + "\n").encode()
        # Fixed remaining retention after Root003; never first materialize an
        # over-cap loose evidence tree. Keep room for source audit/receipt notes.
        assert ((len(encoded) + 4095) // 4096) * 4096 <= 2_531_328, len(encoded)
        assert len(data) < 8192
        durable_write(archive, encoded)
        durable_write(receipt, data)
    elif mode == "root":
        result = audit_root(Path(args[0]))
    else:
        assert mode == "audit"
        archive, receipt = map(Path, args)
        recorded = json.loads(receipt.read_bytes())
        assert archive.stat().st_size <= 2_531_328
        encoded = archive.read_bytes()
        assert len(encoded) == recorded["archive_bytes"] and sha(encoded) == recorded["archive_sha256"]
        files, links, tar_bytes = unpack(encoded)
        assert inventory(files, links).items() <= recorded.items() and tar_bytes == recorded["tar_bytes"]
        assert audit_frames(files) == recorded["reports"]
        result = recorded
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
