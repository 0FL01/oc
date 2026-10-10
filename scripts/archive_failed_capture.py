#!/usr/bin/env python3
"""Preserve one owned untracked failed capture, replacing only loose copies.

This is not a qualified-proof/baseline rewriter. The complete original bytes and
metadata are retained, and the original directory receives a visible failure
pointer. A private quarantine survives any interrupted transaction.
"""
import argparse
from decimal import Decimal
import hashlib
import io
import json
import lzma
import os
from pathlib import Path
import resource
import shutil
import stat
import subprocess
import tarfile
import tempfile

REPO = Path(__file__).resolve().parents[1]
EVIDENCE = REPO / "evidence/tui"
TEMP = Path("/home/opencode/.cache/opencode-tmp/opencode")
CAP = 1024**3
MEMBER = 16 * 1024**2
TOTAL = 128 * 1024**2
ENCODED = 16 * 1024**2  # Separate bound for this explicitly selected small diagnostic.
PREFIXES = ("model-shell", "subagent", "root-handoff", "syntax-")


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(root):
    files, rows, directories = {}, [], []
    total = 0
    for path in sorted([root, *root.rglob("*")]):
        meta = path.lstat()
        require(meta.st_uid == os.getuid() and not path.is_symlink(), "foreign/symlink member")
        name = path.relative_to(root.parent).as_posix()
        require(stat.S_ISDIR(meta.st_mode) or stat.S_ISREG(meta.st_mode), "nonregular member")
        require(len(files) + len(directories) < 2000, "campaign member bound")
        if path.is_dir():
            directories.append([name, stat.S_IMODE(meta.st_mode), meta.st_uid,
                                meta.st_gid, meta.st_mtime_ns])
            continue
        require(meta.st_size <= MEMBER, "oversized member")
        data = path.read_bytes()
        after = path.stat()
        require((meta.st_size, meta.st_mtime_ns, meta.st_ino) ==
                (after.st_size, after.st_mtime_ns, after.st_ino), "member changed during acquisition")
        total += len(data)
        require(total <= TOTAL and len(files) < 2000, "campaign bound")
        files[name] = data
        rows.append([name, len(data), sha(data), stat.S_IMODE(meta.st_mode),
                     meta.st_uid, meta.st_gid, meta.st_mtime_ns])
    require(files, "empty campaign")
    return files, rows, directories


def allocation(root):
    seen, total = set(), 0
    for path in [root, *root.rglob("*")]:
        meta = path.lstat()
        key = (meta.st_dev, meta.st_ino)
        if key not in seen:
            seen.add(key)
            total += meta.st_blocks * 512
    return total


def owned_allocation():
    seen, total = set(), 0
    for root in EVIDENCE.iterdir():
        if not root.name.startswith(PREFIXES):
            continue
        for path in [root, *root.rglob("*")] if root.is_dir() else [root]:
            meta = path.lstat()
            require(not path.is_symlink(), "owned quota symlink")
            key = (meta.st_dev, meta.st_ino)
            if key not in seen:
                seen.add(key)
                total += meta.st_blocks * 512
    return total


def archive_bytes(files, rows, directories):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT) as archive:
        for name, mode, uid, gid, mtime in directories:
            item = tarfile.TarInfo(name)
            item.type = tarfile.DIRTYPE
            item.mode, item.uid, item.gid, item.mtime = mode, uid, gid, mtime // 1_000_000_000
            item.pax_headers = {"mtime": f"{mtime // 1_000_000_000}.{mtime % 1_000_000_000:09d}"}
            archive.addfile(item)
        for name, length, _, mode, uid, gid, mtime in rows:
            item = tarfile.TarInfo(name)
            item.size, item.mode, item.uid, item.gid = length, mode, uid, gid
            item.mtime = mtime // 1_000_000_000
            item.pax_headers = {"mtime": f"{mtime // 1_000_000_000}.{mtime % 1_000_000_000:09d}"}
            archive.addfile(item, io.BytesIO(files[name]))
    encoded = lzma.compress(buffer.getvalue(), check=lzma.CHECK_SHA256,
                            filters=[{"id": lzma.FILTER_LZMA2, "preset": 6 | lzma.PRESET_EXTREME,
                                      "dict_size": 8 * 1024**2}])
    require(len(encoded) <= ENCODED, "selected diagnostic exceeds independent encoded-archive bound")
    return encoded


def audit(encoded, expected, rows, directories):
    require(0 < len(encoded) <= ENCODED, "archive encoded bound")
    decoder = lzma.LZMADecompressor(memlimit=32 * 1024**2)
    raw = decoder.decompress(encoded, max_length=TOTAL + 1)
    require(decoder.eof and not decoder.unused_data and len(raw) <= TOTAL, "archive framing/bound")
    found = {}
    actual_meta = {}
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for item in archive:
            require((item.isfile() or item.isdir()) and item.name not in actual_meta and
                    not item.name.startswith("/") and ".." not in Path(item.name).parts,
                    "invalid member")
            require(item.size <= MEMBER and len(actual_meta) < 2000, "member bound")
            mtime = int(Decimal(item.pax_headers.get("mtime", str(item.mtime))) * 1_000_000_000)
            actual_meta[item.name] = [item.mode, item.uid, item.gid, mtime, item.isdir()]
            if item.isdir():
                continue
            data = archive.extractfile(item).read(MEMBER + 1)
            require(len(data) == item.size, "member length")
            found[item.name] = data
    require(found.keys() == expected.keys(), "complete member membership")
    require(all(found[name] == data for name, data in expected.items()), "whole-member byte mismatch")
    expected_meta = {row[0]: [*row[3:], False] for row in rows}
    expected_meta.update({row[0]: [*row[1:], True] for row in directories})
    require(actual_meta == expected_meta, "complete original directory/file metadata")


def durable(path, data):
    with path.open("xb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    descriptor = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("campaign", choices=["subagent-cards-001"])
    parser.add_argument("--archive", action="store_true", help="perform verified archival after preflight")
    args = parser.parse_args()
    resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
    require(os.getuid() != 0, "non-root only")
    source = EVIDENCE / args.campaign
    require(source.is_dir() and not source.is_symlink(), "original campaign unavailable")
    require(not subprocess.check_output(["git", "ls-files", "--", str(source.relative_to(REPO))],
                                        cwd=REPO).strip(), "committed/staged campaign refused")
    statuses = {side: json.loads((source / side / "tool-preview-checks.json").read_bytes())["status"]
                for side in ("oc", "upstream")}
    require("FAILED" in statuses.values(), "only an actual failed diagnostic")
    output = EVIDENCE / (args.campaign + ".tar.xz")
    receipt_path = EVIDENCE / (args.campaign + ".archived.json")
    require(not output.exists() and not receipt_path.exists(), "archive overwrite refused")
    files, rows, directories = snapshot(source)
    encoded = archive_bytes(files, rows, directories)
    audit(encoded, files, rows, directories)
    record = {"version": 1, "original_root": str(source.relative_to(REPO)),
              "statuses_unchanged": statuses, "archive": str(output.relative_to(REPO)),
              "archive_bytes": len(encoded), "archive_sha256": sha(encoded),
              "members": len(rows), "logical_bytes": sum(row[1] for row in rows),
              "member_inventory_sha256": sha(json.dumps([rows, directories], separators=(",", ":")).encode()),
              "original_directories": directories, "original_files": rows}
    receipt = (json.dumps(record, indent=2) + "\n").encode()
    note = (f"# Archived owned failed diagnostic — {args.campaign}\n\n"
            f"This attempt remains **FAILED / NOT_QUALIFIED**: {json.dumps(statuses)}.\n"
            "Only redundant loose copies were replaced after durable archival and full byte comparison.\n"
            f"All {len(rows)} original files, their paths, SHA-256 and metadata are preserved.\n\n"
            f"- Archive: `../{output.name}`\n- Inventory/receipt: `../{receipt_path.name}`\n"
            f"- Archive SHA-256: `{sha(encoded)}`\n\n"
            "The TAR stores original campaign-relative paths and complete bytes; restore into a fresh\n"
            "private directory for inspection. No committed proof, baseline, old report or checkpoint\n"
            "was modified, and no failure or raster defect was reclassified as PASS.\n").encode()
    round_blocks = lambda length: (length + 4095) // 4096 * 4096
    before = owned_allocation()
    after = before - allocation(source) + round_blocks(len(encoded)) + round_blocks(len(receipt)) + 8192
    require(before <= CAP and after <= CAP, "unchanged owned-retention cap")
    summary = {key: record[key] for key in ("statuses_unchanged", "archive_bytes", "archive_sha256",
                                           "members", "logical_bytes", "member_inventory_sha256")}
    summary.update({"before_allocated": before, "predicted_after_allocated": after,
                    "state": "VERIFIED_PREFLIGHT"})
    print(json.dumps(summary), flush=True)
    if not args.archive:
        return
    require(source.stat().st_dev == TEMP.stat().st_dev, "atomic same-filesystem quarantine required")
    private = Path(tempfile.mkdtemp(prefix="archive-owned-failed-", dir=TEMP))
    durable(private / "candidate.tar.xz", encoded)
    durable(private / "transaction.json", receipt)
    require(snapshot(source)[1:] == (rows, directories), "source changed before transaction")
    quarantine = private / args.campaign
    source.rename(quarantine)
    # Never automatically discard this private quarantine after an interrupted operation.
    source.mkdir(mode=directories[0][1])
    durable(output, encoded)
    durable(receipt_path, receipt)
    durable(source / "ARCHIVED.md", note)
    saved = output.read_bytes()
    require(sha(saved) == record["archive_sha256"], "durable encoded archive identity")
    audit(saved, files, rows, directories)
    require(snapshot(quarantine)[1:] == (rows, directories), "quarantine changed")
    require(owned_allocation() <= CAP, "final owned-retention cap")
    shutil.rmtree(private)  # Only redundant bytes from this fully verified private transaction.
    summary.update({"state": "DURABLY_ARCHIVED_FAILED", "after_allocated": owned_allocation()})
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
