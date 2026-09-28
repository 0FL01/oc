#!/usr/bin/env python3
"""Warning-only physical size inventory of Git-owned handwritten code.

No working-tree writes, baseline updates or checksums. Data and vendor trees
are not code; fixtures containing executable Rust remain part of the inventory.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath

LIMIT = 5000
SCRIPT_EXTENSIONS = {".py", ".mjs", ".js", ".ts", ".sh"}
EXCLUDED_COMPONENTS = {
    "vendor", "generated", "assets", "target", "evidence", "references",
    "donor", "opencode", ".local", ".opencode", "node_modules", "__pycache__",
}


def git(root, *args):
    result = subprocess.run(
        ["git", "-C", str(root), *args], stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=False,
    )
    if result.returncode:
        raise ValueError(f"git {' '.join(args)}: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def owned(path):
    parts = PurePosixPath(path).parts
    if any(part in EXCLUDED_COMPONENTS for part in parts):
        return False
    if parts and parts[0] == "scripts":
        return PurePosixPath(path).suffix in SCRIPT_EXTENSIONS
    return (
        len(parts) >= 4 and parts[0] == "crates"
        and parts[2] in {"src", "tests"} and path.endswith(".rs")
    )


def measure(data):
    text = data.decode("utf-8")  # Invalid code encoding is an error, not a skip.
    return {"lines": text.count("\n") + int(bool(text) and not text.endswith("\n")),
            "bytes": len(data)}


def role(path, data):
    parts = PurePosixPath(path).parts
    stem = PurePosixPath(path).stem
    if parts[0] == "scripts":
        return "dev-tool"
    if parts[2] == "tests" or "tests" in parts[3:] or stem == "tests" or stem.endswith("_tests"):
        return "test"
    # This detects possible mixing, NOT precise production/test SLOC.
    if re.search(rb"#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|test|tokio::test)", data):
        return "mixed/unknown"
    return "source"


def inventory(root, base=None):
    root = Path(root).resolve()
    working_paths = {
        raw.decode("utf-8") for raw in
        git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "crates", "scripts").split(b"\0")
        if raw
    }
    base_blobs = {}
    if base:
        commit = git(root, "rev-parse", "--verify", f"{base}^{{commit}}").decode().strip()
        for entry in git(root, "ls-tree", "-rz", commit, "--", "crates", "scripts").split(b"\0"):
            if not entry:
                continue
            meta, raw_path = entry.split(b"\t", 1)
            mode, kind, oid = meta.split()
            path = raw_path.decode("utf-8")
            if owned(path) and kind == b"blob" and mode != b"120000":
                base_blobs[path] = oid.decode()
    records = []
    for path in sorted(working_paths | base_blobs.keys()):
        if not owned(path):
            continue
        current_path = root / path
        # Check before reading: neither internal nor external links are followed.
        if any(candidate.is_symlink() for candidate in (current_path, *current_path.parents)
               if candidate != root and candidate.is_relative_to(root)):
            records.append({"path": path, "status": "excluded-symlink"})
            continue
        try:
            current = current_path.read_bytes()
        except FileNotFoundError:
            current = None
        previous = git(root, "cat-file", "blob", base_blobs[path]) if path in base_blobs else None
        if current is None and previous is None:
            # A tracked deletion is still visible without a requested base.
            records.append({"path": path, "status": "deleted", "role": "unknown", "current": None, "base": None})
            continue
        now = measure(current) if current is not None else None
        before = measure(previous) if previous is not None else None
        status = ("deleted" if now is None else "new" if before is None and base
                  else "modified" if base and current != previous else "unchanged")
        item = {"path": path, "role": role(path, current if current is not None else previous),
                "status": status, "current": now, "base": before,
                "warning": bool(now and now["lines"] > LIMIT)}
        if base:
            item["delta"] = {key: (now or {}).get(key, 0) - (before or {}).get(key, 0)
                             for key in ("lines", "bytes")}
        records.append(item)
    # Rename is represented as old deleted + new path, without invented base size.
    return records


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", help="Git commit against which to report physical-size deltas")
    parser.add_argument("--changed", action="store_true", help="Only changed/new/deleted paths (requires --base)")
    parser.add_argument("--json", action="store_true", help="Detailed inventory rather than compact advisory summary")
    args = parser.parse_args(argv)
    if args.changed and not args.base:
        parser.error("--changed requires --base")
    try:
        root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
        records = inventory(root, args.base)
        if args.changed:
            records = [item for item in records if item["status"] != "unchanged"]
        records.sort(key=lambda item: (-((item.get("current") or {}).get("lines", 0)), item["path"]))
        if args.json:
            print(json.dumps(records, ensure_ascii=False, indent=2))
        else:
            counted = [item for item in records if item.get("current")]
            print(f"{len(counted)} files; {sum(item['current']['lines'] for item in counted)} physical lines; "
                  f"{sum(item['current']['bytes'] for item in counted)} UTF-8 bytes")
            for item in records:
                if item.get("warning") or args.changed or item["status"] == "excluded-symlink":
                    size = item.get("current")
                    delta = item.get("delta")
                    print(f"{'WARN >5000 ' if item.get('warning') else ''}{item['path']}: "
                          f"{size if size else item['status']} ({item.get('role', 'excluded')})"
                          f"{f' delta={delta}' if delta else ''}")
            print("Advisory only; renames appear as old deleted + new path. Details: --json.")
        return 0
    except (OSError, UnicodeError, ValueError) as error:
        print(f"code_size: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
