"""Freeze URL-backed assets in a NEW isolated original-TUI fixture cache.

Never changes donor descriptors, packaged built-ins, renderer, binary or user
configuration. The real OpenTUI worker subsequently consumes these ordinary URL
cache entries, including the explicitly recorded Swift same-release freeze.
"""

import hashlib
import json
import pathlib
from urllib.parse import urlparse

ROOT = pathlib.Path(__file__).resolve().parents[2]
CACHE = pathlib.Path("/home/opencode/.cache/opencode-tmp/opencode")
ASSETS = ROOT / "crates/oc-tui/assets/syntax"
LIMIT = 16 * 1024 * 1024


def url_key(url):
    # Exact OpenTUI 0.5.10 DownloadUtils.hashUrl: signed JS int32, UTF-16 units,
    # Math.abs(...).toString(16). Not a substituted cryptographic cache key.
    value = 0
    encoded = url.encode("utf-16-le")
    for index in range(0, len(encoded), 2):
        value = (31 * value + int.from_bytes(encoded[index:index + 2], "little")) & 0xffffffff
    if value >= 0x80000000:
        value -= 0x100000000
    return format(abs(value), "x")


def asset(path, expected):
    with path.open("rb") as stream:
        data = stream.read(LIMIT + 1)
    if not data or len(data) > LIMIT or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError("Missing/changed frozen reference asset: " + path.name)
    return data


def preload(home):
    home = pathlib.Path(home)
    if home.is_symlink() or not home.is_dir() or not home.resolve().is_relative_to(CACHE):
        raise ValueError("Reference cache must belong to the fresh approved fixture home")
    base = home / "data/opentui/tree-sitter"
    if any(path.is_symlink() for path in (home / "data", home / "data/opentui")) or not base.resolve().is_relative_to(home.resolve()):
        raise ValueError("Reference cache parent must remain inside its fresh home")
    if base.exists() or base.is_symlink():
        raise ValueError("Refusing an existing reference syntax cache")
    manifest_bytes = (ASSETS / "manifest.json").read_bytes()
    manifest = json.loads(manifest_bytes)
    reference = json.loads((ASSETS / "fixtures.reference.json").read_bytes())
    digest = hashlib.sha256(manifest_bytes).hexdigest()
    if reference["manifest_sha256"] != digest or len(manifest["grammars"]) != 39:
        raise ValueError("Unqualified reference manifest")
    wasm = {entry["name"]: entry["wasm_sha256"] for entry in reference["reference"]}
    records, targets = [], set()
    # Validate every payload and destination before creating the fresh cache.
    for grammar in manifest["grammars"]:
        parts = [(kind, part) for kind, query in grammar["queries"].items()
                 for part in query["parts"]]
        if not parts:  # Built-ins consume their pinned package assets, not URLs.
            continue
        name = pathlib.PurePosixPath(urlparse(grammar["reference_wasm"]).path).name
        records.append({"file": "languages/" + name, "url": grammar["reference_wasm"],
                        "sha256": wasm[grammar["name"]], "kind": "grammar",
                        "data": asset(CACHE / "syntax-reference-wasm" / (grammar["name"] + ".wasm"), wasm[grammar["name"]])})
        for kind, part in parts:
            if pathlib.Path(part["file"]).name != part["file"]:
                raise ValueError("Query part basename")
            records.append({"file": f"queries/{grammar['name']}-{url_key(part['cache_url'])}.scm",
                            "url": part["cache_url"], "source_url": part["source_url"],
                            "sha256": part["sha256"], "kind": kind,
                            "data": asset(ASSETS / part["file"], part["sha256"])})
    for record in records:
        if record["file"] in targets:
            raise ValueError("Ambiguous reference cache key")
        targets.add(record["file"])
    base.mkdir(parents=True)
    for record in records:
        target = base / record["file"]
        target.parent.mkdir(exist_ok=True)
        with target.open("xb") as stream:
            stream.write(record.pop("data"))
        record["initial_atime_ns"] = target.stat().st_atime_ns
    return {"version": 1, "manifest_sha256": digest, "cache": str(base), "files": records}
