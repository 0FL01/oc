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
import os
from pathlib import Path, PurePosixPath
import resource
import stat
import tarfile

import archive_failed_capture as storage

REPO, TEMP = storage.REPO, storage.TEMP
ROOTS = {"inventory": TEMP / "syntax-current-010/inventory",
         **{name: TEMP / "syntax-regressions-001" / name
            for name in ("ordinary", "temporal", "matched")}}
MAX_FILE, MAX_TOTAL = 16 * 1024**2, 128 * 1024**2


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


def producer_checks(files):
    reports = []
    for episode, count in (("inventory", 8), ("ordinary", 56), ("temporal", 160)):
        read = lambda relative: json.loads(files[episode + "/" + relative])
        lock, manifest = read("capture.lock.json"), read("source-manifest.json")
        require(len(manifest) == 704, "complete current producer source membership")
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
                require([stage["stage"] for stage in checks["stages"]] ==
                        ["syntax-live-partial", "syntax-complete-inventory", "syntax-current-profile-draft",
                         "syntax-replayed-inventory"], "four actual planned states")
            elif episode == "ordinary":
                require(checks["status"] == "PASS_BEHAVIOR_ONLY", "ordinary actual effects")
            else:
                require(checks["status"] == "DIAGNOSTIC_CURSOR_ONLY", "temporal outer scope")
                cursor = read(side + "/cursor-temporal-checks.json")
                require(cursor["status"] == "QUALIFIED_CURSOR_BEHAVIOR_ONLY" and
                        sum(state["temporal_full_frames"] for state in cursor["states"]) == 72,
                        "complete default temporal proof")
        reports.append({"episode": episode, "captures": count,
                        "comparisons": dict(collections.Counter(item["status"] for item in lock["attempts"]))})
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("measure", "pack", "audit"))
    args = parser.parse_args()
    resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
    require(os.getuid() != 0, "non-root evidence owner")
    archive = REPO / "evidence/tui/syntax-proof.tar.xz"
    receipt = REPO / "evidence/tui/syntax-proof.inventory.json"
    if args.mode == "audit":
        record = json.loads(receipt.read_bytes())
        encoded = archive.read_bytes()
        require(digest(encoded) == record["archive_sha256"], "independent encoded seal")
        restored, aliases, raw_sha, raw_bytes = unpack(encoded)
        require(inventory(restored, aliases) == record["inventory"] and
                (raw_sha, raw_bytes) == (record["tar_sha256"], record["tar_bytes"]), "whole independent inventory")
        reports = producer_checks(restored)
        require(reports == record["producer_reports"], "recorded producer audit scope")
    else:
        require(not archive.exists() and not receipt.exists(), "retained proof overwrite refused")
        files = source_files()
        reports = producer_checks(files)
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
