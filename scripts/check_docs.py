#!/usr/bin/env python3
"""Check tracked examples and source baselines; never write execution state."""
from __future__ import annotations

import json
import hashlib
from pathlib import Path
import re
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def jsonc(text: str):
    """Remove comments outside strings; then remove trailing commas outside strings."""
    out, i, quoted = [], 0, False
    while i < len(text):
        c = text[i]
        if quoted:
            out.append(c)
            if c == "\\":
                i += 1
                if i < len(text):
                    out.append(text[i])
            elif c == '"':
                quoted = False
        elif c == '"':
            quoted = True
            out.append(c)
        elif text.startswith("//", i):
            end = text.find("\n", i + 2)
            i = len(text) if end == -1 else end
            out.append("\n")
            continue
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            if end == -1:
                raise ValueError("Unclosed JSONC comment")
            out.append(" ")
            i = end + 2
            continue
        else:
            out.append(c)
        i += 1
    text, out, i, quoted = "".join(out), [], 0, False
    while i < len(text):
        c = text[i]
        if quoted:
            out.append(c)
            if c == "\\":
                i += 1
                if i < len(text):
                    out.append(text[i])
            elif c == '"':
                quoted = False
        elif c == '"':
            quoted = True
            out.append(c)
        elif c == ",":
            j = i + 1
            while j < len(text) and text[j].isspace():
                j += 1
            if j == len(text) or text[j] not in "}]":
                out.append(c)
        else:
            out.append(c)
        i += 1
    return json.loads("".join(out))


def validate(root: Path = ROOT) -> dict[str, int]:
    examples = []
    for extension, parse in (("json", json.loads), ("jsonc", jsonc), ("toml", tomllib.loads)):
        for path in sorted((root / "examples").glob(f"*.{extension}")):
            parse(path.read_text(encoding="utf-8"))
            examples.append(path)
    assert examples, "No configuration examples"
    baseline = json.loads((root / "planning/baseline.lock.json").read_text(encoding="utf-8"))
    json.loads((root / "planning/source-manifest.json").read_text(encoding="utf-8"))
    for project in ("opencode", "openproxy", "dcp"):
        assert re.fullmatch(r"[0-9a-f]{40}", baseline[project]["commit"]), f"Invalid {project} source pin"
    assert baseline["dcp"]["license"] == "AGPL-3.0-or-later", "DCP license must remain AGPL-3.0-or-later"
    snapshot = root / baseline["discovery"]["snapshot"]
    assert snapshot.is_file()
    assert hashlib.sha256(snapshot.read_bytes()).hexdigest() == baseline["discovery"]["sha256"], "Discovery snapshot digest mismatch"
    reference = snapshot.read_text(encoding="utf-8")
    assert "DISCOVERY_ATTEMPT_TIMEOUT_MS = 15000" in reference
    assert "DISCOVERY_TOTAL_TIMEOUT_MS = 30000" in reference
    return {"examples": len(examples), "source_baselines": 3}


if __name__ == "__main__":
    try:
        counts = validate()
        print("OK: configuration example syntax and source baselines only (native admission is tested in Rust)")
        print(json.dumps(counts, ensure_ascii=False))
    except (AssertionError, ValueError, OSError, KeyError, TypeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
