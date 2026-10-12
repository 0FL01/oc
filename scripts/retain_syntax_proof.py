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
        driver = name.endswith("-driver")
        require((root.is_file() if driver else root.is_dir()) and not root.is_symlink() and root.resolve().is_relative_to(TEMP),
                "fresh capture root")
        # Fixed campaign parent logs are additional members; their episode
        # subtrees are already included separately, without duplication.
        paths = [root] if driver else root.glob("*") if name.endswith("-logs") else root.rglob("*")
        for path in sorted(paths):
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
            member = name + "/" + (path.name if driver else path.relative_to(root).as_posix())
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
    raw_sha = hashlib.sha256()

    class Decoded:
        size = 0
        last_nonzero_end = 0
        started = False

        def read(self, size):
            if decoder.eof:
                return b""
            data = decoder.decompress(encoded if not self.started else b"", max_length=min(size, 64 * 1024))
            self.started = True
            require(data or decoder.eof, "complete XZ input")
            tail = data.rstrip(b"\0")
            if tail:
                self.last_nonzero_end = self.size + len(tail)
            self.size += len(data)
            require(self.size <= MAX_TOTAL, "single complete bounded XZ")
            raw_sha.update(data)
            return data

    # Source bytes and inverse members coexist for byte equality. Streaming
    # avoids a third complete TAR allocation under the unchanged 384 MiB AS cap.
    stream = Decoded()
    files, aliases = {}, {}
    with tarfile.open(fileobj=stream, mode="r|") as archive:
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
        while stream.read(64 * 1024):
            pass
        require(decoder.eof and not decoder.unused_data, "single complete bounded XZ")
        require(stream.size % tarfile.RECORDSIZE == 0 and stream.size - archive.offset >= 1024 and
                stream.last_nonzero_end <= archive.offset, "complete TAR tail")
    for name, target in aliases.items():
        require(target in files and target not in aliases, "direct canonical alias only")
        files[name] = files[target]
    require(sum(map(len, files.values())) <= MAX_TOTAL, "logical bound")
    return files, aliases, raw_sha.hexdigest(), stream.size


def check_source_manifest(lock, manifest, source_count):
    require(isinstance(manifest, dict) and len(manifest) == source_count and
            all(isinstance(name, str) and isinstance(value, str) and len(value) == 64 and
                all(char in "0123456789abcdef" for char in value)
                for name, value in manifest.items()), "complete producer source membership")
    for name in manifest:
        path = PurePosixPath(name)
        require(not path.is_absolute() and path.as_posix() == name and "\0" not in name and
                all(part not in ("", ".", "..") for part in name.split("/")) and
                (name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                          "tui-recovery/scripts/compare_frames.py", "tui-recovery/scripts/test_compare_frames.py") or
                 name.startswith(("crates/", "scripts/tui_capture/"))), "repository-relative source path")
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


def check_current_source(manifest):
    # Exactly the acquisition's source enumeration, not an arbitrary same-size list.
    sources = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
                              "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "crates", "scripts/tui_capture",
                              "tui-recovery/scripts/compare_frames.py", "tui-recovery/scripts/test_compare_frames.py"],
                             cwd=REPO, capture_output=True, timeout=10, check=False)
    require(sources.returncode == 0, "current source enumeration")
    expected = {name for name in sources.stdout.decode().split("\0") if name and (REPO / name).exists()}
    require(set(manifest) == expected, "exact current producer source set")
    for name, expected_sha in manifest.items():
        path = REPO / name
        require(path.resolve().is_relative_to(REPO) and path.is_file(), "source remains within repository")
        with path.open("rb") as stream:
            require(hashlib.file_digest(stream, "sha256").hexdigest() == expected_sha, "current producer source")


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


def check_middle_facts(checks, profile, side, historical=False, persistent=False, aligned=False, controls=False):
    require(not controls or aligned, "current aligned functional controls scope")
    require(not aligned or persistent, "current persistent compact alignment scope")
    require(not persistent or (profile == "compact" and not historical), "current compact persistence scope")
    require(checks.get("origin") == side and checks.get("middle_case") == profile,
            "actual middle profile/owner")
    failed = historical and profile == "compact"
    require(checks.get("status") == ("FAILED" if failed else "PASS_BEHAVIOR_ONLY"),
            "middle outcome scope")
    if failed:
        require(any(item.get("stage") == "failure" and isinstance(item.get("reason"), str)
                    for item in checks["checks"]), "historical compact failure retained")
        return
    proofs = [item for item in checks["checks"] if item.get("stage") == "proof"]
    require(len(proofs) == 1, "complete Middle Down/restart/reopen proof")
    facts = proofs[0]
    names = ["Alpha", "Beta", "Gamma"] + (["Delta"] if profile == "clipped" else [])
    ids = facts.get("root_ids", {})
    require(isinstance(ids, dict) and set(ids) == set(names) and
            all(isinstance(value, str) and value for value in ids.values()) and
            len(set(ids.values())) == len(names) and
            type(facts.get("requests")) is int and facts["requests"] == 2 * len(names) and
            type(facts.get("tool_calls")) is int and facts["tool_calls"] == 0 and
            all(facts.get(key) is True for key in
                ("history_unchanged", "closed_deck_not_resurrected")) and
            facts.get("reopened_same_id") == ids["Alpha"], "immutable middle identities/effect facts")
    if profile == "compact":
        geometry = [item for item in checks["checks"] if item.get("stage") == "compact-geometry"]
        require(len(geometry) == 1 and geometry[0].get("actual_drag") is True and
                type(geometry[0].get("width")) is int and geometry[0]["width"] == 5 and
                geometry[0].get("restart_persistence_claim") is persistent,
                "actual compact drag and explicit persistence scope")
    if persistent:
        restored = [item for item in checks["checks"] if item.get("stage") == "durable-rail"]
        require(len(restored) == 1 and type(restored[0].get("width")) is int and
                restored[0]["width"] == 5 and restored[0].get("restored_on_history_reopen") is True and
                restored[0].get("preference_scope") == ("Location" if side == "oc" else "global") and
                restored[0].get("native_cas_observed") is (side == "oc") and
                isinstance(restored[0].get("actual_ordinal"), dict) and
                type(restored[0]["actual_ordinal"].get("x")) is int and
                0 <= restored[0]["actual_ordinal"]["x"] < 5,
                "actual restored compact rail")
    if aligned:
        layouts = [item for item in checks["checks"] if item.get("stage") == "compact-layout"]
        expected = [{"x": 2, "y": y} for y in (3, 5, 7)]
        require(len(layouts) == 1 and type(layouts[0].get("width")) is int and
                layouts[0]["width"] == 5 and layouts[0].get("actual_ordinals") == expected and
                all(type(point.get(key)) is int for point in layouts[0]["actual_ordinals"] for key in ("x", "y")) and
                restored[0]["actual_ordinal"] == {"x": 2, "y": 3} and
                type(restored[0]["actual_ordinal"].get("y")) is int,
                "actual compact paint/hit top allocation and restored geometry")
    if controls:
        records = [item for item in checks["checks"] if item.get("stage") == "rail-controls"]
        require(len(records) == 1, "one actual search/cancel/Add proof")
        observed = records[0]
        require(all(observed.get(key) is True for key in
                    ("matched_left_click", "dialog_opened", "cancelled", "same_root_answer",
                     "draft_preserved", "caret_preserved", "history_deck_unchanged", "no_extra_requests")) and
                observed.get("actual_search") == {"x": 2, "y": 1} and
                all(type(observed["actual_search"].get(key)) is int for key in ("x", "y")) and
                isinstance(observed.get("actual_add"), dict) and
                type(observed["actual_add"].get("x")) is int and observed["actual_add"]["x"] == 2 and
                type(observed["actual_add"].get("y")) is int and 0 <= observed["actual_add"]["y"] < 24,
                "actual painted controls and unchanged captured owners")
        require(all(isinstance(observed.get(key), list) and len(observed[key]) == 5 and
                    all(isinstance(cell, dict) and cell.get("bg") == "#1e1e1e" for cell in observed[key])
                    for key in ("search_hover_row", "add_hover_row")) and
                 observed["search_hover_row"][2].get("symbol") == "⌕" and
                 observed["add_hover_row"][2].get("symbol") == "+", "actual current-profile hover surfaces")
        require(isinstance(observed.get("add_idle_row"), list) and len(observed["add_idle_row"]) == 5 and
                all(isinstance(cell, dict) and cell.get("bg") == "#141414" for cell in observed["add_idle_row"]) and
                observed["add_idle_row"][2].get("symbol") == "+", "actual Add idle-to-hover transition")


def check_middle_build(lock, manifest, commands, persistent=False, controls=False):
    invocation = commands[0]
    argv = invocation["argv"]
    require(invocation["role"] == "capture runner invocation" and len(argv) >= 2 and
            argv[1] == str(REPO / "scripts/tui_capture/capture.mjs") and len(argv[2:]) % 2 == 0,
            "actual repository capture runner")
    pairs = list(zip(argv[2::2], argv[3::2], strict=True))
    options = dict(pairs)
    if persistent:
        require(options.get("--rail-persistence") == "true" and options.get("--middle-case") == "compact",
                "actual durable rail acquisition")
    if controls:
        require(persistent and options.get("--rail-controls") == "true", "actual functional rail acquisition")
    require(len(options) == len(pairs) and options.get("--build-oc") == "true" and
            options.get("--tab-middle-close") == "true" and
            options.get("--oc") == lock["oc"]["executable_path"] == str(REPO / "target/debug/oc") and
            options.get("--reference") == lock["upstream"]["executable_path"] ==
            str(TEMP / "t44-reference/package/bin/opencode"), "fixed built/pinned executable paths")
    # This sealed runner's execute() explicitly runs Cargo with cwd=repo. Bind
    # that owner, invocation, exact build and fixed output rather than accepting
    # a successful arbitrary Cargo command and an unrelated executable path.
    require(lock["runner_hashes"]["capture.mjs"] == manifest["scripts/tui_capture/capture.mjs"] and
            lock["oc"]["build_command"] == ["cargo", "build", "--locked"] and
            lock["sources"]["upstream_commit"] == "2670273ff17da96f85c5826ced57aa1b368754fa",
            "sealed build owner and pinned donor")
    builds = [item for item in commands if item["argv"][0] == "cargo"]
    require(len(builds) == 1 and builds[0]["argv"] == lock["oc"]["build_command"] and
            builds[0]["exit_code"] == 0, "one exact recorded repository build")


def middle_checks(files, persistent=False, aligned=False, controls=False):
    reports = []
    profiles = {"horizontal": (120, 40, "horizontal", 22, 18),
                "clipped": (80, 24, "horizontal", 26, 20),
                "vertical": (160, 48, "vertical", 22, 18),
                "compact": (160, 48, "vertical", 26, 13)}
    for episode in ROOTS:
        if episode in ("current-logs", "historical-logs"):
            continue
        period, case = episode.split("-", 1)
        current = period == "current"
        columns, rows, tabs, count, old_count = profiles[case]
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 709)
        if current:
            check_current_source(manifest)
        check_middle_build(lock, manifest, read("commands.json"), persistent, controls)
        require(len(lock["captures"]) == (count + (10 if controls else 0) if current else old_count), "whole middle capture scope")
        # Historical compact stopped at its actual shrunk viewport before relaunch.
        if not current and case == "compact":
            columns, rows = 80, 24
        require(lock["profile"]["columns"] == columns and lock["profile"]["rows"] == rows and
                lock["profile"]["settings"]["tabs"] == tabs,
                "real declared middle geometry")
        statuses = {}
        for side in ("oc", "upstream"):
            if current:
                with Path(lock[side]["executable_path"]).open("rb") as stream:
                    require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                            "actual middle executable identity")
            checks = read(side + "/middle-tabs-checks.json")
            require(checks == lock["tab_middle_close"][side], "actual middle side facts")
            check_middle_facts(checks, case, side, not current, persistent, aligned, controls)
            statuses[side] = checks["status"]
        reports.append({"episode": episode, "captures": len(lock["captures"]), "current_source": current,
                        "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


def compact_controls_diagnostic_checks(files):
    reports = []
    for episode, count, reason in (("historical-first", 24, "Error: Add hover baseline was not idle\n"),
                                   ("historical-second", 25, "Error: Timed out waiting for rail-add-idle\n")):
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 709)
        check_middle_build(lock, manifest, read("commands.json"), persistent=True, controls=True)
        require(len(lock["captures"]) == count, "complete failed controls episode retained")
        statuses = {}
        for side in ("oc", "upstream"):
            facts = read(side + "/middle-tabs-checks.json")
            require(facts == lock["tab_middle_close"][side], "historical controls facts")
            if side == "oc":
                check_middle_facts(facts, "compact", side, persistent=True, aligned=True)
            else:
                require(facts.get("origin") == side and facts.get("middle_case") == "compact" and
                        facts.get("status") == "FAILED" and
                        any(item.get("stage") == "failure" and isinstance(item.get("reason"), str) and
                            item["reason"].startswith(reason)
                            for item in facts["checks"]), "original failed idle predicate remains visible")
            statuses[side] = facts["status"]
        reports.append({"episode": episode, "captures": len(lock["captures"]), "current_source": False,
                        "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


def check_vis12_build(lock, manifest, commands, episode, *, wrap_attempt=None):
    require(wrap_attempt is None or (episode == "caret" and wrap_attempt in ("001", "002", "003")),
            "fixed VIS12 wrap attempt")
    invocation = commands[0]
    argv = invocation["argv"]
    require(invocation.get("role") == "capture runner invocation" and len(argv) >= 2 and
            argv[1] == str(REPO / "scripts/tui_capture/capture.mjs") and len(argv[2:]) % 2 == 0,
            "actual VIS12 capture runner")
    pairs = list(zip(argv[2::2], argv[3::2], strict=True))
    options = dict(pairs)
    expected = {"--reference": str(TEMP / "t44-reference/package/bin/opencode"),
                "--oc": str(REPO / "target/debug/oc"), "--build-oc": "true", "--geometry": "true",
                "--sample": "short", "--sidebar": "hide", "--tool-preview": "true",
                "--columns": "80" if episode == "history" else "120", "--rows": "24" if episode == "history" else "40",
                "--output": str(TEMP / ("vis12-current-001" if wrap_attempt is None else
                                       "vis12-wrap-current-" + wrap_attempt) / episode)}
    expected["--prompt-caret" if episode == "caret" else "--prompt-history"] = (
        "true" if episode == "caret" else "default" if episode == "history" else "remap")
    require(len(options) == len(pairs) and options == expected and
            lock["oc"]["executable_path"] == options["--oc"] and
            lock["upstream"]["executable_path"] == options["--reference"], "fixed VIS12 acquisition and paths")
    probe = "prompt_caret" if episode == "caret" else "prompt_history"
    require(all(lock["runner_hashes"][name] == manifest["scripts/tui_capture/" + name]
                for name in ("capture.mjs", probe + ".mjs", probe + "_fixture.py")) and
            lock["oc"]["build_command"] == ["cargo", "build", "--locked"] and
            lock["sources"]["upstream_commit"] == "2670273ff17da96f85c5826ced57aa1b368754fa",
            "sealed VIS12 build/fixture owners and pinned donor")
    builds = [item for item in commands if item["argv"][0] == "cargo"]
    require(len(builds) == 1 and builds[0]["argv"] == lock["oc"]["build_command"] and
            builds[0]["exit_code"] == 0, "one exact recorded VIS12 repository build")


def check_vis12_facts(checks, episode, side):
    caret = episode == "caret"
    stages = ("caret-home caret-home-word-clicked caret-home-word-edited caret-wide-second-clicked "
              "caret-wide-second-edited caret-combining-clicked caret-combining-edited caret-zwj-start-clicked "
              "caret-zwj-start-edited caret-trailing-clicked caret-trailing-edited caret-wrap-gap-clicked "
              "caret-wrap-gap-edited caret-blank-line caret-resized-word-clicked caret-resized-word-edited "
              "caret-chip-neighbor caret-chip-expanded caret-overlay caret-overlay-restored caret-before-submit "
              "caret-submitted caret-session-edited caret-reopened caret-restarted").split() if caret else (
              "history-home history-input-1 history-accepted-1 history-input-2 history-accepted-2 "
              "history-input-3 history-accepted-3 history-unfinished history-unfinished-edge history-session-latest "
              "history-session-older history-next-visual history-next-edge history-next-latest history-forward-boundary "
              "history-edit-base history-edited-copy history-edited-refuses-older history-new-home history-home-shared "
              "history-overlay history-overlay-owned history-recalled-accepted history-restarted-shared "
              "history-slash-accepted").split()
    if episode == "remap":
        stages.insert(stages.index("history-unfinished-edge"), "history-remapped-old-up")
        stages.insert(stages.index("history-recalled-accepted"), "history-remapped-old-down")
        stages.insert(stages.index("history-slash-accepted"), "history-restarted-leader")
    require(checks.get("origin") == side and checks.get("status") == "PASS_BEHAVIOR_ONLY" and
            [item.get("stage") for item in checks["stages"]] == stages, "complete actual VIS12 stages and scope")
    count = 2 if caret else 6
    facts = checks["prompt_caret" if caret else "prompt_history"]
    require(facts.get("status") == ("OBSERVED_PROMPT_CARET" if caret else "OBSERVED_PROMPT_HISTORY") and
            all(type(facts.get(key)) is int and facts[key] == value for key, value in
                (("provider_requests", count), ("explicit_user_submissions", 1 if caret else 4), ("tool_effects", 0))),
            "exact VIS12 explicit submissions and effects")
    requests = [item.get("requests") for item in checks["stages"]]
    require(all(item.get("status") == "CAPTURED_TOOL_PREVIEW" and type(item.get("calls")) is int and
                item["calls"] == 0 for item in checks["stages"]) and
            all(type(value) is int and 0 <= value <= count for value in requests) and
            requests == sorted(requests) and requests[0] == 0 and requests[-1] == count,
            "actual VIS12 request progression without tool calls")
    if caret:
        require(all(value == 0 for value in requests[:stages.index("caret-submitted")]) and
                facts.get("user_text") == "Проведи RECXON, жду план" and
                facts.get("native_durable_user_verified") is (side == "oc") and
                facts.get("reopen_restart_no_replay") is True, "click/edit before one exact Enter; no replay")
        snapshots = [facts[key] for key in ("before", "after", "restarted")]
    else:
        require(all(value == 4 for value in requests[stages.index("history-accepted-3"):
                                                    stages.index("history-recalled-accepted")]) and
                facts.get("mode") == ("remap" if episode == "remap" else "default") and
                facts.get("shared_home_session_restart") is True and
                facts.get("effective_leader_verified") is (episode == "remap") and
                facts.get("reference_forward_boundary") == "EMPTY_PROMPT" and
                facts.get("reference_nonempty_draft_refused") is (side == "upstream") and
                all(facts.get(key) is (side == "oc") for key in
                    ("native_unfinished_draft_restored", "native_consecutive_dedup_verified", "native_raw_users_verified")),
                "real history boundaries, configured keys and declared native/reference differences")
        snapshots = [facts[key] for key in ("accepted", "before_restart", "after_restart", "slash")]
    require(all(snapshot.get("kind") == "tool_preview_snapshot" and snapshot.get("artifacts") == {} and
                snapshot.get("shell_effect") is None and snapshot.get("mcp_calls") == [] and
                snapshot.get("mcp_lifecycle") == [] for snapshot in snapshots), "VIS12 snapshots without effects")
    require(snapshots[1]["observations"] == snapshots[2]["observations"] and
            (not caret or snapshots[0]["observations"] == snapshots[1]["observations"]),
            "accepted data unchanged on reopen/restart")
    if side == "oc":
        data = [next(item["data"] for item in snapshot["observations"] if "prompt_user_messages" in item["data"])
                for snapshot in snapshots]
        accepted = ["Проведи RECXON, жду план"] if caret else [
            "HISTORY-FIRST Ω界", "HISTORY-SECOND\nsecond line Ω界", "HISTORY-THIRD @note.txt done"]
        require([row["text"] for row in data[0]["prompt_user_messages"]] == accepted and
                all(item.get(key) == [] for item in data for key in
                    ("tool_operations", "tool_output_resources", "presentation")), "native exact RAW and no tool rows")
        if not caret:
            require(all(item["prompt_input_history"] == accepted for item in data[:3]) and
                    data[3]["prompt_input_history"] == accepted + ["/mcps"] and
                    [row["text"] for row in data[1]["prompt_user_messages"]] == accepted + [accepted[-1]] and
                    data[1]["prompt_user_messages"] == data[3]["prompt_user_messages"],
                    "shared durable dedup, fresh-session acceptance and slash-before-dispatch")


def vis12_checks(files):
    reports = []
    for episode, columns, rows, count in (("history", 80, 24, 50), ("remap", 120, 40, 56), ("caret", 120, 40, 50)):
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 709)
        check_current_source(manifest)
        check_vis12_build(lock, manifest, read("commands.json"), episode)
        require(len(lock["captures"]) == count and len(lock["attempts"]) == count and
                lock["profile"]["columns"] == columns and lock["profile"]["rows"] == rows and
                lock["profile"]["prompt_caret"] is (episode == "caret") and
                lock["profile"]["prompt_history"] == ("remap" if episode == "remap" else "default" if episode == "history" else None),
                "complete actual VIS12 membership and geometry")
        statuses = {}
        for side in ("oc", "upstream"):
            with Path(lock[side]["executable_path"]).open("rb") as stream:
                require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                        "actual VIS12 executable identity")
            checks = read(side + "/tool-preview-checks.json")
            require(checks == lock["tool_preview"][side], "actual VIS12 side facts")
            check_vis12_facts(checks, episode, side)
            statuses[side] = checks["status"]
        reports.append({"episode": episode, "captures": count, "current_source": True, "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


def check_vis12_wrap_launches(protocol, attempt, side):
    output = TEMP / ("vis12-wrap-current-" + attempt) / "caret"
    name = "caret" if attempt == "001" else "caret-" + digest(str(output).encode())[:12]
    root = TEMP / "t44-reference/runs" / name
    launches = [item for item in protocol if item.get("kind") in ("launch", "relaunch")]
    require(len(launches) == (1 if attempt == "001" else 2) and
            all(item.get("cwd") == str(root / "project") for item in launches),
            "actual VIS12 wrap fixture root on launch/relaunch")
    home = root / side / "home"
    require(launches[0]["env"].get("HOME") == str(home) and
            all(launches[0]["env"].get("XDG_" + key + "_HOME") == str(home / suffix)
                for key, suffix in (("CONFIG", "config"), ("CACHE", "cache"),
                                    ("DATA", "data"), ("STATE", "state"))),
            "actual isolated VIS12 wrap HOME/XDG lineage")


def check_vis12_wrap_geometry(native, original):
    # An assertion about the acquired owner slice, not a normalized frame or a
    # full equality claim. Complete comparisons and all blank/footer cells stay.
    require(all(frame.get("columns") == 120 and frame.get("rows") == 40 and
                len(frame["cells"]) == 40 and all(len(row) == 120 for row in frame["cells"])
                for frame in (native, original)) and
            native.get("cursor") == original.get("cursor") ==
            {"x": 86, "y": 27, "visible": True, "shape": "block"},
            "actual expanded long-word full geometry and complete caret")
    for y in range(15, 28):
        count = 60 if y == 27 else 70
        for frame in (native, original):
            require("".join(cell["symbol"] for cell in frame["cells"][y][26:96]) ==
                    "z" * count + " " * (70 - count), "actual visible long-word partial tail")
        require(native["cells"][y][26:26 + count] == original["cells"][y][26:26 + count],
                "all 900 visible long-word styled glyph cells")


def vis12_wrap_checks(files):
    reports = []
    for prefix, attempt, count, current, scope in (
            ("failed-caret", "001", 38, False, "FAILED_INCOMPLETE"),
            ("current-caret", "002", 50, True, "BEHAVIOR_GEOMETRY_ONLY"),
            ("rejected-caret", "003", 50, False, "REJECTED_FOREGROUND_EXPERIMENT")):
        read = lambda relative: json.loads(files[prefix + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 709)
        check_vis12_build(lock, manifest, read("commands.json"), "caret", wrap_attempt=attempt)
        require(len(lock["captures"]) == count and lock["profile"]["columns"] == 120 and
                lock["profile"]["rows"] == 40 and lock["profile"]["prompt_caret"] is True and
                lock["profile"]["prompt_history"] is None, "complete VIS12 wrap capture scope")
        if current:
            check_current_source(manifest)
            for side in ("oc", "upstream"):
                with Path(lock[side]["executable_path"]).open("rb") as stream:
                    require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                            "actual current VIS12 wrap executable")
            check_vis12_wrap_geometry(read("oc/tool-preview-caret-chip-expanded.cells.json"),
                                      read("upstream/tool-preview-caret-chip-expanded.cells.json"))
        if attempt == "001":
            require([item["status"] for item in lock["attempts"]] == ["FAILED", "BLOCKED"],
                    "interrupted acquisition remains failed/blocked, not compared or replayed")
        else:
            require(len(lock["attempts"]) == 50 and all(item["status"] == "DIFFERENT" for item in lock["attempts"]),
                    "complete strict VIS12 wrap comparisons remain DIFFERENT")
        statuses = {}
        for side in ("oc", "upstream"):
            checks, protocol = read(side + "/tool-preview-checks.json"), read(side + "/protocol.json")
            check_vis12_wrap_launches(protocol, attempt, side)
            providers = [item for item in protocol if item.get("kind") == "provider"]
            require(len(providers) == (0 if attempt == "001" and side == "oc" else 2) and
                    all(item.get("valid") is True for item in providers), "actual wrap provider count/validity")
            require(all(item.get("artifacts") == {} and item.get("shell_effect") is None and
                        item.get("mcp_calls") == [] and item.get("mcp_lifecycle") == []
                        for item in protocol if item.get("kind") == "tool_preview_snapshot"),
                    "actual wrap protocol snapshots without additional effects")
            if attempt == "001":
                require(checks.get("origin") == side and
                        checks.get("status") == ("IN_PROGRESS" if side == "oc" else "FAILED") and
                        len(checks["stages"]) == (14 if side == "oc" else 23) and
                        (side == "oc" or checks.get("reason") == "Saved caret session row not unique"),
                        "failed duplicate-title and incomplete native facts retained")
            else:
                require(checks == lock["tool_preview"][side], "actual complete wrap side facts")
                check_vis12_facts(checks, "caret", side)
            statuses[side] = checks["status"]
        reports.append({"episode": prefix, "captures": count, "current_source": current, "scope": scope,
                        "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


def check_vis37_build(lock, manifest, commands, profile, attempt, form_campaign=False):
    allowed = (("wide", "001"), ("small", "002"), ("wide", "002"), ("tall", "002"))
    if form_campaign:
        allowed += tuple((profile, attempt) for attempt in ("003", "004")
                         for profile in ("small", "wide", "tall"))
    require((profile, attempt) in allowed,
            "fixed VIS37 acquisition")
    columns, rows = {"small": (80, 24), "wide": (120, 40), "tall": (160, 48)}[profile]
    invocation = commands[0]
    argv = invocation["argv"]
    require(invocation.get("role") == "capture runner invocation" and len(argv) >= 2 and
            argv[1] == str(REPO / "scripts/tui_capture/capture.mjs") and len(argv[2:]) % 2 == 0,
            "actual VIS37 capture runner")
    pairs = list(zip(argv[2::2], argv[3::2], strict=True))
    options = dict(pairs)
    expected = {"--reference": str(TEMP / "t44-reference/package/bin/opencode"),
                "--oc": str(REPO / "target/debug/oc"), "--build-oc": "true", "--geometry": "true",
                "--sample": "short", "--sidebar": "hide", "--tool-preview": "true", "--question-forms": "true",
                "--columns": str(columns), "--rows": str(rows),
                "--output": str(TEMP / (("vis37-form-current-" if form_campaign else "vis37-current-") + attempt) / profile)}
    require(len(options) == len(pairs) and options == expected and
            lock["oc"]["executable_path"] == options["--oc"] and
            lock["upstream"]["executable_path"] == options["--reference"] and
            lock["profile"]["columns"] == columns and lock["profile"]["rows"] == rows and
            lock["profile"].get("question_forms") is True, "fixed VIS37 options, paths and geometry")
    require(all(lock["runner_hashes"][name] == manifest["scripts/tui_capture/" + name]
                for name in ("capture.mjs", "tool_preview.mjs", "tool_preview_fixture.py",
                             "question_forms.mjs", "question_fixture.py")) and
            lock["oc"]["build_command"] == ["cargo", "build", "--locked"] and
            lock["sources"]["upstream_commit"] == "2670273ff17da96f85c5826ced57aa1b368754fa",
            "sealed VIS37 producer/fixture and pinned donor")
    builds = [item for item in commands if item["argv"][0] == "cargo"]
    require(len(builds) == 1 and builds[0]["argv"] == lock["oc"]["build_command"] and
            builds[0]["exit_code"] == 0, "one exact captured VIS37 repository build")


def check_vis37_facts(checks, protocol, side):
    names = ("question-home question-pending question-one-selected question-multiple-selected question-long-field "
             "question-custom-empty question-custom-edited question-review question-completed question-answered-card "
             "question-switched question-reopened question-restarted question-cancel-pending question-dismissed "
             "question-cancel-switched question-cancel-reopened").split()
    require(checks.get("origin") == side and checks.get("status") == "PASS_BEHAVIOR_ONLY" and
            [item.get("stage") for item in checks["stages"]] == names, "all actual owned VIS37 stages")
    for index, stage in enumerate(checks["stages"]):
        requests, calls = (0, 0) if index == 0 else (2, 1) if index < 8 else (3, 1) if index < 13 else (4, 2)
        require(stage.get("status") == "CAPTURED_TOOL_PREVIEW" and
                type(stage.get("requests")) is int and stage["requests"] == requests and
                type(stage.get("calls")) is int and stage["calls"] == calls,
                "no editing/review/reopen/restart/dismissal generation")
    facts = checks["question_forms"]
    answers = [["Rust", "Python"], ["custom Ω界"]]
    require(facts.get("status") == "OBSERVED_OWNED_QUESTION" and facts.get("answers") == answers and
            all(type(facts.get(key)) is int and facts[key] == value for key, value in
                (("provider_requests", 4), ("main_requests", 3), ("explicit_user_submissions", 2),
                 ("explicit_answer_submissions", 1), ("question_calls", 2))) and
            all(facts.get(key) is True for key in ("no_edit_submission", "no_restart_replay",
                                                 "cancellation_not_answered", "readonly_native_and_donor_source_records")),
            "exact explicit VIS37 inputs and ownership scope")
    providers = [item for item in protocol if item.get("kind") == "provider"]
    mains = [item for item in providers if item.get("operation") == "question_main"]
    titles = [item for item in providers if item.get("operation") == "question_title"]
    calls = [item for item in protocol if item.get("kind") == "fixture_tool_call"]
    require(len(providers) == 4 and len(mains) == 3 and len(titles) == 1 and
            all(item.get("valid") is True and type(item.get("index")) is int for item in providers) and titles[0].get("index") == 1 and
            [item.get("index") for item in mains] == [1, 2, 3] and
            [item.get("call_id") for item in calls] == ["call_vis37_answer", "call_vis37_cancel"] and
            all(item.get("name") == "question" for item in calls), "real question wire sequence and no continuation on cancel")
    argument = calls[0]["arguments"]
    require(argument == calls[1]["arguments"] and len(argument["questions"]) == 2 and
            [question.get("header") for question in argument["questions"]] ==
            ["Languages and native runtimes", "Platform and execution targets"] and
            argument["questions"][0].get("multiple") is True and argument["questions"][1].get("multiple") is False and
            all(item.get("registered_tools") == ["question"] for item in mains) and
            [item.get("user_texts") for item in mains] == [["VIS37 paired question consumer"]] * 2 +
            [["VIS37 paired question consumer", "VIS37 dismiss the next question"]] and
            mains[0].get("actual_results") == [], "actual same typed questions and exact user submissions")
    native_result = {**argument, "answers": answers}
    donor_result = 'User has answered your questions: ' + ', '.join(
        '"' + question["question"] + '"="' + ', '.join(answer) + '"'
        for question, answer in zip(argument["questions"], answers, strict=True)
    ) + ". You can now continue with the user's answers in mind."
    for request in mains[1:]:
        results = request["actual_results"]
        require(len(results) == 1 and results[0].get("type") == "function_call_output" and
                results[0].get("call_id") == "call_vis37_answer" and
                (json.loads(results[0]["output"]) == native_result if side == "oc" else
                 results[0].get("output") == donor_result), "actual owned native typed/donor canonical answer result")
    snapshots = [facts[key] for key in ("completed", "reopened", "restarted", "dismissed", "final")]
    observed = [item for item in protocol if item.get("kind") == "tool_preview_snapshot"]
    require(observed == snapshots and all(item.get("artifacts") == {} and item.get("shell_effect") is None and
                                         item.get("mcp_calls") == [] and item.get("mcp_lifecycle") == []
                                         for item in snapshots), "actual read-only snapshots without extra effects")
    require(snapshots[0]["observations"] == snapshots[1]["observations"] == snapshots[2]["observations"] and
            snapshots[3]["observations"] == snapshots[4]["observations"], "durable source IDs and RAW unchanged on viewing/restart")
    for index, snapshot in enumerate(snapshots):
        data = [row["data"] for row in snapshot["observations"] if "question_records" in row["data"]]
        require(len(data) == 1 and data[0]["question_records"].get("readonly") is True,
                "bounded actual native/donor source records")
        records = data[0]["question_records"]["operations"]
        require(len(records) == (1 if index < 3 else 2), "only completed answer and owned dismissed question")
        if side == "oc":
            require(records[0].get("provider_call_id") == "call_vis37_answer" and
                    records[0].get("name") == "question" and records[0].get("state") == "completed" and
                    json.loads(records[0]["input"]) == argument and json.loads(records[0]["output"]) == native_result and
                    len(data[0]["tool_operations"]) == len(records) and data[0]["tool_output_resources"] == [] and
                    all(any(all(row.get(key) == value for key, value in record.items())
                            for row in data[0]["tool_operations"]) for record in records),
                    "actual completed native typed metadata/input/output")
            users = [row["text"] for row in data[0]["question_records"]["messages"] if row["role"] == "user"]
            if index >= 3:
                require(records[1].get("provider_call_id") == "call_vis37_cancel" and records[1].get("name") == "question" and
                        records[1].get("state") == "cancelled" and records[1].get("output") == "error: question dismissed" and
                        json.loads(records[1]["input"]) == argument, "native cancellation is not a successful answer")
        else:
            part = records[0]["data"]
            require(part.get("type") == "tool" and part.get("name") == "question" and part.get("id") == "call_vis37_answer" and
                    part["state"].get("status") == "completed" and part["state"].get("input") == argument and
                    part["state"]["metadata"].get("answers") == answers and
                    part["state"]["content"] == [{"type": "text", "text": donor_result}],
                    "actual donor V2 source part and typed answers")
            users = [json.loads(row["data"])["text"] for row in data[0]["question_records"]["messages"] if row["type"] == "user"]
            if index >= 3:
                cancelled = records[1]["data"]
                require(cancelled.get("id") == "call_vis37_cancel" and cancelled.get("name") == "question" and
                        cancelled["state"].get("status") == "error" and cancelled["state"].get("input") == argument and
                        "answers" not in (cancelled["state"].get("metadata") or {}), "donor cancellation is not an answered source")
        require(users == ["VIS37 paired question consumer"] + (["VIS37 dismiss the next question"] if index >= 3 else []),
                "actual RAW user submissions stay exact")
    exits = [item for item in protocol if item.get("kind") == "exit"]
    require(len(exits) == 2 and exits[0].get("generation") == 0 and exits[0].get("code") == 0 and
            exits[1].get("generation") == 1 and exits[1].get("termination") == "forced_stop",
            "natural first shutdown and honest final harness teardown")


def check_vis37_answered_card(native, original):
    # Owner-glyph assertion only. Every blank cell, clock and whole-frame report
    # remains unmodified and participates in the strict full comparisons.
    require((native.get("columns"), native.get("rows")) in ((80, 24), (120, 40), (160, 48)) and
            all((frame.get("columns"), frame.get("rows")) == (native["columns"], native["rows"]) and
                len(frame["cells"]) == frame["rows"] and all(len(row) == frame["columns"] for row in frame["cells"])
                for frame in (native, original)) and native.get("cursor") == original.get("cursor"),
            "complete answered frame geometry/caret")
    ranges = []
    for frame in (native, original):
        titles = [y for y, row in enumerate(frame["cells"]) if '# Questions' in ''.join(cell["symbol"] for cell in row)]
        require(len(titles) == 1, "actual unique answered card heading")
        start = end = titles[0]
        borders = [x for x, cell in enumerate(frame["cells"][start]) if cell["symbol"] == "┃"]
        require(len(borders) == 1, "actual answered block border")
        border = borders[0]
        while start > 0 and frame["cells"][start - 1][border]["symbol"] == "┃":
            start -= 1
        while end + 1 < frame["rows"] and frame["cells"][end + 1][border]["symbol"] == "┃":
            end += 1
        ranges.append((start, end, border))
    require(ranges[0] == ranges[1], "actual answered block order/spacing/wrap geometry")
    for y in range(ranges[0][0], ranges[0][1] + 1):
        for left, right in zip(native["cells"][y], original["cells"][y], strict=True):
            if left["symbol"].strip() or right["symbol"].strip():
                require(left == right, "all actual answered nonblank glyph styles/text")


def vis37_checks(files):
    reports = []
    for prefix, attempt, profile, count in (("failed-wide", "001", "wide", 25),
                                            ("current-small", "002", "small", 34),
                                            ("current-wide", "002", "wide", 34),
                                            ("current-tall", "002", "tall", 34)):
        read = lambda relative: json.loads(files[prefix + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        check_source_manifest(lock, manifest, 713)
        check_vis37_build(lock, manifest, read("commands.json"), profile, attempt)
        current = attempt == "002"
        require(len(lock["captures"]) == count and len(lock["attempts"]) == (34 if current else 30),
                "all complete and failed VIS37 frames/comparison attempts")
        require(collections.Counter(item["status"] for item in lock["attempts"]) ==
                ({"DIFFERENT": 34} if current else {"DIFFERENT": 20, "BLOCKED": 10}),
                "whole strict comparisons remain different; failed missing counterparts stay blocked")
        if current:
            check_current_source(manifest)
            for side in ("oc", "upstream"):
                with Path(lock[side]["executable_path"]).open("rb") as stream:
                    require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                            "actual current VIS37 executable identity")
            check_vis37_answered_card(read("oc/tool-preview-question-answered-card.cells.json"),
                                      read("upstream/tool-preview-question-answered-card.cells.json"))
        statuses = {}
        output = TEMP / ("vis37-current-" + attempt) / profile
        root = TEMP / "t44-reference/runs" / (profile + "-" + digest(str(output).encode())[:12])
        for side in ("oc", "upstream"):
            checks, protocol = read(side + "/tool-preview-checks.json"), read(side + "/protocol.json")
            require(checks == lock["tool_preview"][side], "captured VIS37 side facts remain exact")
            launches = [item for item in protocol if item.get("kind") in ("launch", "relaunch")]
            home = root / side / "home"
            require(len(launches) == (2 if current or side == "oc" else 1) and
                    all(item.get("cwd") == str(root / "project") for item in launches) and
                    launches[0]["env"].get("HOME") == str(home) and
                    all(launches[0]["env"].get("XDG_" + key + "_HOME") == str(home / key.lower())
                        for key in ("CONFIG", "CACHE", "DATA", "STATE")), "actual sealed fresh fixture launch/relaunch lineage")
            if current:
                check_vis37_facts(checks, protocol, side)
            else:
                require(checks.get("origin") == side and checks.get("status") == "FAILED" and
                        len(checks["stages"]) == (14 if side == "oc" else 9) and
                        checks.get("reason") == ("Timed out waiting for question-dismissed" if side == "oc" else
                                                 "Extra/missing question ownership or effects"),
                        "known observer failures remain visible, not promoted/replayed")
                providers = [item for item in protocol if item.get("kind") == "provider"]
                require(len(providers) == (4 if side == "oc" else 3) and
                        all(item.get("valid") is True for item in providers), "actual failed partial provider effects")
            statuses[side] = checks["status"]
        reports.append({"episode": prefix, "captures": count, "current_source": current,
                        "scope": "ANSWER_CONSUMER_CARD_GLYPHS_ONLY" if current else "FAILED_OBSERVER_PROJECTION",
                        "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


def vis37_form_roots(current_only=False):
    if current_only:
        return {**{"current-" + profile: TEMP / "vis37-form-current-004" / profile
                   for profile in ("small", "wide", "tall")},
                "current-logs": TEMP / "vis37-form-current-004",
                "current-driver": TEMP / "vis37-form-capture-r4.py"}
    roots = {"failed-wide": TEMP / "vis37-form-current-001/wide"}
    roots.update({period + "-" + profile: TEMP / ("vis37-form-current-" + attempt) / profile
                  for period, attempt in (("historical", "002"), ("current", "003"))
                  for profile in ("small", "wide", "tall")})
    for period, attempt, helper in (("failed", "001", "vis37-form-capture.py"),
                                    ("historical", "002", "vis37-form-capture-r2.py"),
                                    ("current", "003", "vis37-form-capture-r3.py")):
        roots[period + "-logs"] = TEMP / ("vis37-form-current-" + attempt)
        roots[period + "-driver"] = TEMP / helper
    return roots


def check_vis37_form_keyboard(checks, inputs):
    require(all(checks["question_forms"].get(key) is True for key in
                ("tab_and_backtab_committed_without_submit", "return_after_tab_reopens_custom", "optional_empty_custom_tab_did_not_submit")),
            "actual custom Tab/Shift+Tab/reedit without reply")
    expected = [("question_empty_custom_tab", "CQ=="), ("question_empty_custom_backtab", "G1ta"),
                ("question_custom_tab_commit", "CQ=="), ("question_custom_backtab", "G1ta"),
                ("question_custom_reopen_edit", "DQ=="), ("question_commit_custom", "DQ=="),
                ("question_explicit_reply", "DQ==")]
    selected = [(item.get("kind"), item.get("base64")) for item in inputs if item.get("kind") in dict(expected)]
    require(selected == expected, "exact real PTY navigation, commit and single explicit reply bytes")


def vis37_form_checks(files, current_only=False):
    reports = []
    if current_only:
        episodes = [("current-" + profile, "004", profile) for profile in ("small", "wide", "tall")]
    else:
        episodes = [("failed-wide", "001", "wide")]
        episodes += [(period + "-" + profile, attempt, profile)
                     for period, attempt in (("historical", "002"), ("current", "003"))
                     for profile in ("small", "wide", "tall")]
    for prefix, attempt, profile in episodes:
        read = lambda relative: json.loads(files[prefix + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        current, failed = attempt == ("004" if current_only else "003"), attempt == "001"
        check_source_manifest(lock, manifest, 713)
        check_vis37_build(lock, manifest, read("commands.json"), profile, attempt, form_campaign=True)
        require(len(lock["captures"]) == (22 if failed else 34) and
                len(lock["attempts"]) == (36 if failed else 34), "complete form frames and failed attempts retained")
        if failed or not current:
            require(collections.Counter(item["status"] for item in lock["attempts"]) ==
                    ({"DIFFERENT": 8, "BLOCKED": 28} if failed else {"DIFFERENT": 34}),
                    "honest failed and intermediate form comparison outcomes")
        if current:
            check_current_source(manifest)
            for side in ("oc", "upstream"):
                with Path(lock[side]["executable_path"]).open("rb") as stream:
                    require(hashlib.file_digest(stream, "sha256").hexdigest() == lock[side]["executable_sha256"],
                            "actual current form executables")
        statuses = {}
        output = TEMP / ("vis37-form-current-" + attempt) / profile
        root = TEMP / "t44-reference/runs" / (profile + "-" + digest(str(output).encode())[:12])
        for side in ("oc", "upstream"):
            checks, protocol = read(side + "/tool-preview-checks.json"), read(side + "/protocol.json")
            require(checks == lock["tool_preview"][side], "form side producer facts remain exact")
            launches = [item for item in protocol if item.get("kind") in ("launch", "relaunch")]
            home = root / side / "home"
            require(len(launches) == (1 if failed and side == "oc" else 2) and
                    all(item.get("cwd") == str(root / "project") for item in launches) and
                    launches[0]["env"].get("HOME") == str(home) and
                    all(launches[0]["env"].get("XDG_" + key + "_HOME") == str(home / key.lower())
                        for key in ("CONFIG", "CACHE", "DATA", "STATE")), "sealed form fixture ownership and lineage")
            if failed and side == "oc":
                providers = [item for item in protocol if item.get("kind") == "provider"]
                require(checks.get("status") == "FAILED" and len(checks["stages"]) == 4 and
                        checks.get("reason") == "Timed out waiting for question-long-field" and
                        len(providers) == 2 and all(item.get("valid") is True for item in providers),
                        "lost same-owner gesture failure and partial effects remain visible")
            else:
                check_vis37_facts(checks, protocol, side)
                if current:
                    check_vis37_form_keyboard(checks, read(side + "/inputs.json"))
            statuses[side] = checks["status"]
        reports.append({"episode": prefix, "captures": len(lock["captures"]), "current_source": current,
                        "scope": "PENDING_FORM_BEHAVIOR_ONLY" if current else
                                 "FAILED_SAME_OWNER_GESTURE" if failed else "HISTORICAL_INTERMEDIATE_FORM",
                        "statuses": statuses,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
    return reports


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
    parser.add_argument("--campaign", choices=("syntax", "write", "write-r2", "write-r3", "wrap", "root", "root-diagnostics", "hover", "raster", "caret", "mcp-label", "shell-wrap", "shell-wrap-r2", "shell-wrap-r3", "shell-wrap-diagnostics", "middle-tabs", "rail-width", "compact-layout", "compact-controls-r3", "compact-controls-diagnostics", "vis12", "vis12-wrap", "vis37", "vis37-form", "vis37-form-r4", *TEMPORAL_PROFILES), default="syntax")
    args = parser.parse_args()
    source_count = 704
    inventory_stages = None
    temporal_profile = args.campaign if args.campaign in TEMPORAL_PROFILES else None
    temporal_only = args.campaign == "caret" or temporal_profile is not None
    if args.campaign in ("vis37-form", "vis37-form-r4"):
        ROOTS = vis37_form_roots(args.campaign == "vis37-form-r4")
    elif args.campaign == "vis37":
        ROOTS = {"failed-wide": TEMP / "vis37-current-001/wide",
                 **{"current-" + profile: TEMP / "vis37-current-002" / profile for profile in ("small", "wide", "tall")},
                 "failed-logs": TEMP / "vis37-current-001", "current-logs": TEMP / "vis37-current-002",
                 "failed-driver": TEMP / "vis37-capture.py", "current-driver": TEMP / "vis37-capture-r2.py"}
    elif args.campaign == "vis12-wrap":
        ROOTS = {period + "-" + kind: TEMP / ("vis12-wrap-current-" + attempt) / ("caret" if kind == "caret" else "")
                 for period, attempt in (("failed", "001"), ("current", "002"), ("rejected", "003"))
                 for kind in ("caret", "logs")}
    elif args.campaign == "vis12":
        ROOTS = {name: TEMP / "vis12-current-001" / name for name in ("history", "remap", "caret")}
        ROOTS["current-logs"] = TEMP / "vis12-current-001"
    elif args.campaign == "compact-controls-diagnostics":
        ROOTS = {"historical-first": TEMP / "compact-controls-current-001/compact",
                 "historical-second": TEMP / "compact-controls-current-002/compact",
                 "historical-logs": TEMP / "compact-controls-current-001",
                 "current-logs": TEMP / "compact-controls-current-002"}
    elif args.campaign == "compact-controls-r3":
        ROOTS = {"current-compact": TEMP / "compact-controls-current-003/compact",
                 "current-logs": TEMP / "compact-controls-current-003"}
    elif args.campaign in ("rail-width", "compact-layout"):
        ROOTS = {"current-compact": TEMP / (args.campaign + "-current-001/compact"),
                 "current-logs": TEMP / (args.campaign + "-current-001")}
    elif args.campaign == "middle-tabs":
        ROOTS = {period + "-" + case: TEMP / ("middle-tabs-current-" + attempt) / case
                 for period, attempt in (("current", "002"), ("historical", "001"))
                 for case in ("horizontal", "clipped", "vertical", "compact")}
        ROOTS.update({"current-logs": TEMP / "middle-tabs-current-002",
                      "historical-logs": TEMP / "middle-tabs-current-001"})
    elif temporal_profile:
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
        if args.campaign in ("vis37-form", "vis37-form-r4"):
            return vis37_form_checks(files, args.campaign == "vis37-form-r4")
        if args.campaign == "vis37":
            return vis37_checks(files)
        if args.campaign == "vis12-wrap":
            return vis12_wrap_checks(files)
        if args.campaign == "vis12":
            return vis12_checks(files)
        if args.campaign == "compact-controls-diagnostics":
            return compact_controls_diagnostic_checks(files)
        if args.campaign in ("middle-tabs", "rail-width", "compact-layout", "compact-controls-r3"):
            return middle_checks(files, args.campaign in ("rail-width", "compact-layout", "compact-controls-r3"),
                                 args.campaign in ("compact-layout", "compact-controls-r3"),
                                 args.campaign == "compact-controls-r3")
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
        require(allocated + extra <= storage.CAP,
                f"unchanged retained-evidence cap: allocated={allocated}, extra={extra}, "
                f"archive={len(encoded)}, receipt={len(metadata)}, cap={storage.CAP}")
        if args.mode == "pack":
            storage.durable(archive, encoded)
            storage.durable(receipt, metadata)
            require(digest(archive.read_bytes()) == record["archive_sha256"], "durable encoded identity")
    print(json.dumps({"status": "FULL_BYTE_EXACT", "archive_bytes": len(encoded),
                      "inventory": record["inventory"], "reports": reports,
                      "retained": args.mode != "measure"}))


if __name__ == "__main__":
    main()
