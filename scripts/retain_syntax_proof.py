"""Self-contained byte-exact TAR/XZ retention for the complete fresh T44 proof.

No baseline, pixel predictor, correction library or rewritten producer metadata.
All captured bytes survive; only fresh same-run/same-side identical PNGs alias.
"""
import argparse
import collections
import hashlib
import io
import json
import lzma
import math
import os
from pathlib import Path, PurePosixPath
import resource
import stat
import subprocess
import tarfile

import archive_failed_capture as storage

REPO, TEMP = storage.REPO, storage.TEMP
ROOTS = {"inventory": TEMP / "syntax-current-010/inventory",
         **{name: TEMP / "syntax-regressions-001" / name
            for name in ("ordinary", "temporal", "matched")}}
MAX_FILE, MAX_TOTAL = 16 * 1024**2, 128 * 1024**2
TEMPORAL_PROFILES = {
    "blink-baseline": ("blink-current-001/temporal", None, "blink", "collapsed", "supported", 80, 24),
    "blink-fallback": ("blink-fallback-001/temporal", "blink-fallback-001/matched", "blink", "expanded", "unsupported", 80, 24),
    "blink-supported": ("blink-controls-001/supported", "blink-controls-001/supported-matched", "blink", "expanded", "supported", 120, 40),
    "blink-steady": ("blink-controls-001/steady-fallback", "blink-controls-001/steady-fallback-matched", "steady", "expanded", "unsupported", 160, 48),
}


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def path_ok(name):
    path = PurePosixPath(name)
    require(not path.is_absolute() and path.as_posix() == name and "\0" not in name and
            all(part not in ("", ".", "..") for part in name.split("/")) and
            len(path.parts) >= 2 and path.parts[0] in ROOTS, "member path")
    return path


def source_files():
    files, total = {}, 0
    for name, root in ROOTS.items():
        require(root.is_dir() and not root.is_symlink() and root.resolve().is_relative_to(TEMP),
                "fresh capture root")
        for path in sorted(root.rglob("*")):
            meta = path.lstat()
            require(not path.is_symlink() and meta.st_uid == os.getuid(), "foreign/symlink capture")
            require(stat.S_ISDIR(meta.st_mode) or stat.S_ISREG(meta.st_mode), "nonregular capture")
            if path.is_dir():
                continue
            require(meta.st_size <= MAX_FILE, "member byte bound")
            data = path.read_bytes()
            after = path.stat()
            require((meta.st_size, meta.st_mtime_ns, meta.st_ino) ==
                    (after.st_size, after.st_mtime_ns, after.st_ino), "capture changed")
            total += len(data)
            require(total <= MAX_TOTAL and len(files) < 2000, "complete campaign bound")
            member = name + "/" + path.relative_to(root).as_posix()
            path_ok(member)
            files[member] = data
    return files


def png_aliases(files):
    first, aliases = {}, {}
    for name, data in sorted(files.items()):
        if not name.endswith(".png"):
            continue
        parts = path_ok(name).parts
        require(len(parts) >= 3 and parts[1] in ("oc", "upstream"), "PNG source identity")
        key = (parts[:2], digest(data))
        if key in first:
            require(files[first[key]] == data, "encoded PNG identity")
            aliases[name] = first[key]
        else:
            first[key] = name
    return aliases


def inventory(files, aliases):
    rows = [[name, len(data), digest(data), aliases.get(name)] for name, data in sorted(files.items())]
    return {"members": len(rows), "logical_bytes": sum(len(data) for data in files.values()),
            "member_inventory_sha256": digest(json.dumps(rows, separators=(",", ":")).encode()),
            "same_side_png_aliases": len(aliases)}


def unpack(encoded):
    require(0 < len(encoded) <= MAX_FILE, "encoded archive bound")
    decoder = lzma.LZMADecompressor(memlimit=32 * 1024**2)
    raw = decoder.decompress(encoded, max_length=MAX_TOTAL + 1)
    require(decoder.eof and not decoder.unused_data and len(raw) <= MAX_TOTAL, "single complete bounded XZ")
    raw_sha, raw_bytes = digest(raw), len(raw)
    buffer = io.BytesIO(raw)
    del raw, decoder
    files, aliases = {}, {}
    with tarfile.open(fileobj=buffer, mode="r:") as archive:
        for item in archive:
            path = path_ok(item.name)
            require(item.name not in files and item.name not in aliases and
                    len(files) + len(aliases) < 2000, "member membership")
            if item.islnk():
                target = path_ok(item.linkname)
                require(path.parts[:2] == target.parts[:2] and path.parts[1] in ("oc", "upstream") and
                        item.name.endswith(".png") and item.linkname.endswith(".png"), "alias scope")
                aliases[item.name] = item.linkname
            else:
                require(item.isfile() and 0 <= item.size <= MAX_FILE, "regular bounded member")
                data = archive.extractfile(item).read(MAX_FILE + 1)
                require(len(data) == item.size, "exact member size")
                files[item.name] = data
        # tarfile stops at the first end marker. The single USTAR stream must
        # contain both end blocks and only its canonical zero record padding.
        require(raw_bytes % tarfile.RECORDSIZE == 0 and raw_bytes - archive.offset >= 1024 and
                not any(buffer.getbuffer()[archive.offset:]), "complete TAR tail")
    for name, target in aliases.items():
        require(target in files and target not in aliases, "direct canonical alias only")
        files[name] = files[target]
    require(sum(map(len, files.values())) <= MAX_TOTAL, "logical bound")
    return files, aliases, raw_sha, raw_bytes


def check_source_manifest(lock, manifest, source_count):
    require(isinstance(manifest, dict) and len(manifest) == source_count and
            all(isinstance(name, str) and isinstance(value, str) and len(value) == 64 and
                all(char in "0123456789abcdef" for char in value)
                for name, value in manifest.items()), "complete producer source membership")
    # Acquisition uses JavaScript localeCompare, not Python/codepoint key order.
    # Reuse that exact dev-tool collation; Node runs jitless with a small heap and
    # inherits the unchanged 384 MiB address-space bound. TAR/XZ stays stdlib-only.
    canonical = subprocess.run([
        "node", "--jitless", "--max-old-space-size=16", "--max-semi-space-size=1", "-e",
        'const m=JSON.parse(require("node:fs").readFileSync(0,"utf8"));'
        'const c=JSON.stringify(Object.fromEntries(Object.entries(m).sort(([a],[b])=>a.localeCompare(b))));'
        'process.stdout.write(require("node:crypto").createHash("sha256").update(c).digest("hex"));'
    ], input=json.dumps(manifest, ensure_ascii=False).encode(), capture_output=True, timeout=10, check=False)
    require(canonical.returncode == 0 and
            canonical.stdout.decode("ascii") == lock["oc"]["source_manifest_sha256"],
            "producer source manifest seal")


def check_caret_readiness(cursor):
    facts = cursor.get("draft_input")
    require(isinstance(facts, dict) and type(facts.get("left_moves")) is int and
            facts["left_moves"] == 2, "actual two Left inputs")
    for origin_name, cursor_name, columns in (("origin", "paste_cursor", 19),
                                              ("origin", "inside_cursor", 16),
                                              ("resized_origin", "resized_cursor", 16)):
        origin, position = facts.get(origin_name), facts.get(cursor_name)
        require(isinstance(origin, dict) and isinstance(position, dict) and
                all(type(item.get(key)) is int and item[key] >= 0
                    for item in (origin, position) for key in ("x", "y")) and
                position.get("visible") is True and position["y"] == origin["y"] and
                position["x"] == origin["x"] + columns,
                "actual painted paste/readiness and logical caret")


def check_temporal_profile(lock, cursor, side, name):
    _, _, mode, case, sync, columns, rows = TEMPORAL_PROFILES[name]
    profile = lock["profile"]
    require(all(profile.get(key) == value for key, value in
                (("cursor_temporal", mode), ("cursor_case", case), ("cursor_sync", sync),
                 ("cursor_renderer", "webgl"), ("columns", columns), ("rows", rows))) and
            profile.get("cursor_blink") is (mode == "blink"), "actual declared temporal profile")
    reference_failure = name == "blink-baseline" and side == "upstream"
    require(cursor["status"] == ("FAILED_CURSOR_QUALIFICATION" if reference_failure else
                                  "QUALIFIED_CURSOR_BEHAVIOR_ONLY") and
            cursor["mode"] == mode and cursor["card_case"] == case and
            cursor["geometry"] == {"columns": columns, "rows": rows}, "temporal outcome scope")
    states = cursor["states"]
    require([item["state"] for item in states] ==
            ["idle", "hover", "restored", "search-idle", "search-hover", "composer-restored"],
            "all six temporal owners")
    check_caret_readiness(cursor)
    for state in states:
        failed_hover = reference_failure and state["state"] == "hover"
        require(all(type(state[key]) is int and state[key] >= 0 for key in
                    ("temporal_full_frames", "samples", "commands", "cycles", "moves", "phantom_command_states")) and
                state["temporal_full_frames"] == 12 and state["samples"] > 0 and
                state["phantom_command_states"] == 0 and state["final_caret_preserved"] is True and
                state["adequate_raster_sampling"] is True, "complete actual temporal observations")
        periods = state["cycle_periods_ms"]
        require(isinstance(periods, list) and len(periods) == state["cycles"] and
                all(type(value) in (int, float) and math.isfinite(value) and value > 0 for value in periods),
                "actual cycle timestamps")
        gap = state["max_sample_gap_ms"]
        idle = states[3] if state["state"].startswith("search-") else states[0]
        require(type(gap) in (int, float) and math.isfinite(gap) and gap > 0 and
                (mode != "blink" or gap <= min(idle["cycle_periods_ms"]) / 4),
                "adequate actual raster sampling")
        require(state["cadence_preserved"] is (not failed_hover) and
                state["raster_blink_observed"] is (mode == "blink" and not failed_hover) and
                ((0 < state["cycles"] < 3) if failed_hover else
                 (state["cycles"] >= 3 if mode == "blink" else state["cycles"] == 0)),
                "blink cycles/cadence or explicit preserved reference failure")
        require((state["moves"] > 0 and state["commands"] > 0) if "hover" in state["state"] else
                state["moves"] == 0, "actual continuous hover/control")


def producer_checks(files, source_count=704, inventory_stages=None, require_hover=False, require_tail=False, temporal_only=False, temporal_profile=None, require_mcp_label=False, require_shell_wrap=False, require_shell_style=False):
    inventory_stages = inventory_stages or ["syntax-live-partial", "syntax-complete-inventory",
                                          "syntax-current-profile-draft", "syntax-replayed-inventory"]
    reports = []
    episodes = (("temporal", 160),) if temporal_only else (("inventory", 2 * len(inventory_stages)), ("ordinary", 56), ("temporal", 160))
    for episode, count in episodes:
        cursor_statuses = {}
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, source_count)
        for name, expected in manifest.items():
            with (REPO / name).open("rb") as stream:
                require(hashlib.file_digest(stream, "sha256").hexdigest() == expected, "current producer source")
        builds = [item for item in read("commands.json") if item["argv"][0] == "cargo"]
        require(len(builds) == 1 and builds[0]["argv"][:3] == ["cargo", "build", "--locked"] and
                builds[0]["exit_code"] == 0, "actual current-source build")
        require(len(lock["captures"]) == count, "complete planned capture membership")
        for side in ("oc", "upstream"):
            with Path(lock[side]["executable_path"]).open("rb") as stream:
                require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                        "actual executable identity")
            checks = read(side + "/tool-preview-checks.json")
            if episode == "inventory":
                require(checks["status"] == "PASS_BEHAVIOR_ONLY" and checks["syntax"]["no_replay"],
                        "completed actual syntax/reopen proof")
                require([stage["stage"] for stage in checks["stages"]] == inventory_stages,
                        "all actual planned states")
                if require_hover:
                    require(checks["syntax"].get("user_hover_after_resize") is True and
                            checks["syntax"].get("replay_user_unhovered") is True,
                            "actual pointer hover and fresh replay mount")
                if require_tail:
                    require(checks["syntax"].get("shrink_retained_tail") is True,
                            "actual alternate-screen shrink tail")
                if len(inventory_stages) == 7:
                    facts = checks["syntax"]["root_add"]
                    require(all(facts.get(name) is True for name in
                                ("actual_busy_mouse_add", "busy_return_draft_preserved",
                                 "completed_while_parked", "home_closed_without_root_effect",
                                 "no_provider_replay")) and checks["syntax"]["requests"] == 4,
                            "actual parked-root episode")
                    if side == "oc":
                        require(facts.get("cancelled_exact_retained_root") is True,
                                "actual retained-root cancellation")
            elif episode == "ordinary":
                require(checks["status"] == "PASS_BEHAVIOR_ONLY", "ordinary actual effects")
                if require_shell_wrap:
                    facts = checks.get("shell_command_wrap", {})
                    require(facts.get("columns") == 120 and facts.get("continuation_separator") is True,
                             "actual collapsed Shell character-wrap separator")
                    if require_shell_style:
                        states = facts.get("style_states", [])
                        require([item.get("stage") for item in states] == ["completed", "reopened", "restarted"] and
                                all(item.get("gutter_foreground") == "#ffffff" and
                                    item.get("background") == ("#141414" if item["stage"] == "completed" else "#1e1e1e")
                                    for item in states), "actual live/cold Shell command/gutter styles")
            else:
                require(checks["status"] == "DIAGNOSTIC_CURSOR_ONLY", "temporal outer scope")
                cursor = read(side + "/cursor-temporal-checks.json")
                if temporal_profile:
                    check_temporal_profile(lock, cursor, side, temporal_profile)
                    require(checks["cursor_temporal"]["no_tool_replay"] is True, "no tool replay")
                    cursor_statuses[side] = cursor["status"]
                else:
                    require(cursor["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY" and
                             sum(state["temporal_full_frames"] for state in cursor["states"]) == 72,
                             "complete default temporal proof")
                if temporal_only:
                    check_caret_readiness(cursor)
            if require_mcp_label and side == "oc" and episode != "inventory":
                require(checks["owner_facts"]["mcp_labels"] == {
                    "wire_name": "vis16__output", "display_name": "vis16_output",
                    "intent_records": 3, "result_records": 3}, "captured MCP intent/result label facts")
        report = {"episode": episode, "captures": count,
                  "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))}
        if temporal_profile:
            report.update(profile=temporal_profile, cursor_statuses=cursor_statuses)
        reports.append(report)
    return reports


def diagnostic_checks(files):
    # Keep temporary before/after diagnostics and the two failed older-source
    # probes complete. Retain their provenance, never call them current.
    reports = []
    for episode, count in (("diagnostic-before", 8), ("diagnostic-after", 8),
                           ("probe-first", 6), ("probe-second", 14)):
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 707)
        builds = [item for item in read("commands.json") if item["argv"][0] == "cargo"]
        require(len(manifest) == 707 and len(lock["captures"]) == count and len(builds) == 1 and
                builds[0]["argv"][:3] == ["cargo", "build", "--locked"] and builds[0]["exit_code"] == 0,
                "recorded diagnostic acquisition scope")
        for side in ("oc", "upstream"):
            checks = read(side + "/tool-preview-checks.json")
            if episode == "probe-first":
                require(checks["status"] == "FAILED" and len(checks["stages"]) == 2,
                        "failed Root viewport-probe visibility")
            elif episode == "probe-second":
                require(checks["status"] == ("FAILED" if side == "oc" else "PASS_BEHAVIOR_ONLY") and
                        len(checks["stages"]) == (6 if side == "oc" else 7),
                        "failed native cancelled-prefix visibility")
            else:
                require(checks["status"] == "PASS_BEHAVIOR_ONLY" and checks["syntax"]["no_replay"],
                        "actual diagnostic syntax episode")
        if episode in ("diagnostic-before", "diagnostic-after"):
            require(episode + "/oc/scheduler.preview-loss.jsonl" in files,
                    "original loss diagnostic retained")
        reports.append({"episode": episode, "captures": count, "current_source": False,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"])),
                        "native_executable_sha256": lock["oc"]["executable_sha256"]})
    return reports


def shell_diagnostic_checks(files):
    reports = []
    for episode, count in (("inventory", 14), ("ordinary", 32)):
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 708)
        builds = [item for item in read("commands.json") if item["argv"][0] == "cargo"]
        require(len(lock["captures"]) == count and len(builds) == 1 and
                builds[0]["argv"][:3] == ["cargo", "build", "--locked"] and builds[0]["exit_code"] == 0,
                "recorded Shell diagnostic acquisition scope")
        for side in ("oc", "upstream"):
            checks = read(side + "/tool-preview-checks.json")
            require(checks["status"] == ("PASS_BEHAVIOR_ONLY" if episode == "inventory" else "FAILED"),
                    "Shell diagnostic outcome preserved")
            if episode == "ordinary":
                require(checks.get("reason") == "Actual Shell command/gutter style differs at completed",
                        "failed overbroad foreground assertion visibility")
        reports.append({"episode": episode, "captures": count, "current_source": False,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"])),
                        "native_executable_sha256": lock["oc"]["executable_sha256"]})
    return reports


def encode(files, aliases):
    compressor = lzma.LZMACompressor(check=lzma.CHECK_SHA256,
                                    filters=[{"id": lzma.FILTER_LZMA2, "preset": 6 | lzma.PRESET_EXTREME,
                                              "dict_size": 16 * 1024**2}])
    output, raw_sha = io.BytesIO(), hashlib.sha256()

    class Sink:
        size = 0

        def write(self, data):
            self.size += len(data)
            require(self.size <= MAX_TOTAL, "TAR byte bound")
            raw_sha.update(data)
            output.write(compressor.compress(data))
            return len(data)

    sink = Sink()
    suffixes = (".cells.json", ".png", ".render.json", ".vt", ".txt", ".trace.json")
    ordering = lambda name: (next((index for index, suffix in enumerate(suffixes)
                                   if name.endswith(suffix)), len(suffixes)), name.rsplit("/", 1)[-1], name)
    with tarfile.open(fileobj=sink, mode="w|", format=tarfile.USTAR_FORMAT) as archive:
        for name in sorted(files, key=ordering):
            item = tarfile.TarInfo(name)
            item.mode = 0o644
            if name in aliases:
                item.type, item.linkname = tarfile.LNKTYPE, aliases[name]
                archive.addfile(item)
            else:
                item.size = len(files[name])
                archive.addfile(item, io.BytesIO(files[name]))
    output.write(compressor.flush())
    encoded = output.getvalue()
    # The local Sink class has a type/MRO cycle; explicitly release its large
    # captured native encoder before allocating the bounded inverse buffer.
    del compressor
    output.close()
    return encoded, raw_sha.hexdigest(), sink.size


def main():
    global ROOTS
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("measure", "pack", "audit"))
    parser.add_argument("--campaign", choices=("syntax", "write", "write-r2", "write-r3", "wrap", "root", "root-diagnostics", "hover", "raster", "caret", "mcp-label", "shell-wrap", "shell-wrap-r2", "shell-wrap-r3", "shell-wrap-diagnostics", *TEMPORAL_PROFILES), default="syntax")
    args = parser.parse_args()
    source_count = 704
    inventory_stages = None
    temporal_profile = args.campaign if args.campaign in TEMPORAL_PROFILES else None
    temporal_only = args.campaign == "caret" or temporal_profile is not None
    if temporal_profile:
        current, matched, *_ = TEMPORAL_PROFILES[temporal_profile]
        ROOTS = {"temporal": TEMP / current}
        if matched:
            ROOTS["matched"] = TEMP / matched
        source_count = 707
    elif args.campaign == "caret":
        ROOTS = {name: TEMP / "caret-current-001" / name for name in ("temporal", "matched")}
        source_count = 707
    elif args.campaign == "root-diagnostics":
        ROOTS = {"diagnostic-before": TEMP / "root-loss-diagnostic-001",
                 "diagnostic-after": TEMP / "root-loss-diagnostic-002",
                 "probe-first": TEMP / "root-current-001/inventory",
                  "probe-second": TEMP / "root-current-002/inventory"}
    elif args.campaign == "shell-wrap-diagnostics":
        ROOTS = {"inventory": TEMP / "shell-wrap-current-002/inventory",
                 "ordinary": TEMP / "shell-wrap-regressions-002/ordinary"}
    elif args.campaign in ("root", "hover", "raster", "mcp-label", "shell-wrap", "shell-wrap-r2", "shell-wrap-r3"):
        current, regressions = {"root": ("root-current-003", "root-regressions-001"),
                                "hover": ("hover-current-001", "hover-regressions-001"),
                                "raster": ("raster-current-001", "raster-regressions-001"),
                                 "mcp-label": ("mcp-label-current-001", "mcp-label-regressions-001"),
                                 "shell-wrap": ("shell-wrap-current-001", "shell-wrap-regressions-001"),
                                 "shell-wrap-r2": ("shell-wrap-current-002", "shell-wrap-regressions-002"),
                                 "shell-wrap-r3": ("shell-wrap-current-003", "shell-wrap-regressions-003")}[args.campaign]
        ROOTS = {"inventory": TEMP / current / "inventory",
                 **{name: TEMP / regressions / name
                    for name in ("ordinary", "temporal", "matched")}}
        source_count = 708 if args.campaign in ("mcp-label", "shell-wrap", "shell-wrap-r2", "shell-wrap-r3") else 707
        inventory_stages = ["syntax-live-partial", "syntax-root-parked-home", "syntax-complete-inventory",
                            "syntax-current-profile-draft", "syntax-replayed-inventory",
                            "syntax-root-cancel-pending", "syntax-root-cancelled"]
    elif args.campaign == "wrap":
        ROOTS = {"inventory": TEMP / "wrap-current-001/inventory",
                 **{name: TEMP / "wrap-regressions-001" / name
                    for name in ("ordinary", "temporal", "matched")}}
        source_count = 706
    elif args.campaign != "syntax":
        attempt = {"write": "001", "write-r2": "002", "write-r3": "003"}[args.campaign]
        ROOTS = {"inventory": TEMP / f"write-current-{attempt}/inventory",
                 **{name: TEMP / f"write-regressions-{attempt}" / name
                    for name in ("ordinary", "temporal", "matched")}}
        source_count = 705
    resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
    require(os.getuid() != 0, "non-root evidence owner")
    archive = REPO / f"evidence/tui/{args.campaign}-proof.tar.xz"
    receipt = REPO / f"evidence/tui/{args.campaign}-proof.inventory.json"
    shell_campaign = args.campaign in ("shell-wrap", "shell-wrap-r2", "shell-wrap-r3")
    label_campaign = args.campaign == "mcp-label" or shell_campaign
    tail_campaign = args.campaign == "raster" or label_campaign
    hover_campaign = args.campaign == "hover" or tail_campaign
    def checks(files):
        if args.campaign == "root-diagnostics":
            return diagnostic_checks(files)
        if args.campaign == "shell-wrap-diagnostics":
            return shell_diagnostic_checks(files)
        return producer_checks(files, source_count, inventory_stages, hover_campaign, tail_campaign,
                               temporal_only, temporal_profile, label_campaign, shell_campaign,
                               args.campaign in ("shell-wrap-r2", "shell-wrap-r3"))
    if args.mode == "audit":
        record = json.loads(receipt.read_bytes())
        encoded = archive.read_bytes()
        require(digest(encoded) == record["archive_sha256"], "independent encoded seal")
        restored, aliases, raw_sha, raw_bytes = unpack(encoded)
        require(inventory(restored, aliases) == record["inventory"] and
                (raw_sha, raw_bytes) == (record["tar_sha256"], record["tar_bytes"]), "whole independent inventory")
        reports = checks(restored)
        require(reports == record["producer_reports"], "recorded producer audit scope")
    else:
        require(not archive.exists() and not receipt.exists(), "retained proof overwrite refused")
        files = source_files()
        reports = checks(files)
        aliases = png_aliases(files)
        expected = inventory(files, aliases)  # Seal complete actual targets BEFORE encoding.
        encoded, raw_sha, raw_bytes = encode(files, aliases)
        restored, links, decoded_sha, decoded_bytes = unpack(encoded)
        require(restored.keys() == files.keys() and all(restored[name] == data for name, data in files.items()),
                "every original captured whole byte")
        require(inventory(restored, links) == expected and (decoded_sha, decoded_bytes) == (raw_sha, raw_bytes),
                "complete TAR/XZ roundtrip")
        record = {"version": 1, "codec": "stdlib USTAR + single XZ; original encoded PNGs unchanged",
                  "archive_sha256": digest(encoded), "archive_bytes": len(encoded),
                  "tar_sha256": raw_sha, "tar_bytes": raw_bytes, "inventory": expected,
                  "source_roots": {name: str(root) for name, root in ROOTS.items()},
                  "producer_reports": reports, "retainer_source_sha256": digest(Path(__file__).read_bytes())}
        metadata = (json.dumps(record, indent=2) + "\n").encode()
        allocated = storage.owned_allocation()
        extra = sum((length + 4095) // 4096 * 4096 for length in (len(encoded), len(metadata)))
        require(allocated + extra <= storage.CAP, "unchanged retained-evidence cap")
        if args.mode == "pack":
            storage.durable(archive, encoded)
            storage.durable(receipt, metadata)
            require(digest(archive.read_bytes()) == record["archive_sha256"], "durable encoded identity")
    print(json.dumps({"status": "FULL_BYTE_EXACT", "archive_bytes": len(encoded),
                      "inventory": record["inventory"], "reports": reports,
                      "retained": args.mode != "measure"}))


if __name__ == "__main__":
    main()
