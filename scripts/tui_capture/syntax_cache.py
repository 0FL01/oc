"""Freeze URL-backed assets in a NEW isolated original-TUI fixture cache.

Never changes donor descriptors, packaged built-ins, renderer, binary or user
configuration. The real OpenTUI worker subsequently consumes these ordinary URL
cache entries, including the explicitly recorded Swift same-release freeze.
"""

import hashlib
import json
import os
import pathlib
import pwd
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


def preload_execute(home, assets, approved_root):
    """Only the official 2.0.31 Execute dialog's URL-backed JSON inputs.

    TypeScript stays bundled in the unmodified binary. This is deliberately not
    the legacy 39-grammar qualification or an asset-root override.
    """
    expected_root = pathlib.Path(pwd.getpwuid(os.getuid()).pw_dir) / '.cache/opencode-tmp/opencode'
    root, home, assets = map(pathlib.Path, (approved_root, home, assets))
    if root != expected_root or root.is_symlink() or root.stat().st_uid != os.getuid():
        raise ValueError('Execute cache root must be the current UID approved cache')
    for directory in (home, assets):
        if (directory.is_symlink() or not directory.is_dir() or
                directory.stat().st_uid != os.getuid() or
                not directory.resolve().is_relative_to(root.resolve())):
            raise ValueError('Execute cache inputs/home must remain in the owned approved cache')
    base = home / 'data/opentui/tree-sitter'
    if (any(part.is_symlink() for part in (home / 'data', home / 'data/opentui', base)) or
            not base.resolve().is_relative_to(home.resolve()) or base.exists()):
        raise ValueError('Refusing an existing or redirected Execute syntax cache')
    wasm = assets / 'tree-sitter-json.wasm'
    if wasm.is_symlink() or wasm.stat().st_uid != os.getuid():
        raise ValueError('Execute JSON WASM must be an owned ordinary payload')
    records = [
        {'file':'languages/tree-sitter-json.wasm', 'kind':'grammar',
         'url':'https://github.com/tree-sitter/tree-sitter-json/releases/download/v0.24.8/tree-sitter-json.wasm',
         'sha256':'d2119fb98d5912719b13f9458574f8608d2d29dfbe45f6be1f860ea1fe2a2405'},
        {'file':'queries/json-256dee0a.scm', 'kind':'highlights',
         'url':'https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/refs/heads/master/queries/json/highlights.scm',
         'source_url':'https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/cf12346a3414fa1b06af75c79faebe7f76df080a/queries/json/highlights.scm',
         'sha256':'6ab09656820f0e8dac860c96dced341221284544c05308ee337727f5db937101'},
    ]
    payloads = [asset(wasm, records[0]['sha256']),
                asset(ASSETS / 'json.highlights.0.scm', records[1]['sha256'])]
    if url_key(records[1]['url']) != '256dee0a':
        raise ValueError('Execute JSON query cache key changed')
    base.mkdir(parents=True)
    for record, data in zip(records, payloads):
        target = base / record['file']
        target.parent.mkdir(exist_ok=True)
        with target.open('xb') as stream:
            stream.write(data)
        record['initial_atime_ns'] = target.stat().st_atime_ns
    return {'version':2, 'profile':'execute-2.0.31-typescript-json-only',
            'cache':str(base), 'files':records,
            'bundled_typescript':{
                'wasm_sha256':'778025db5a8be0e70f8ccc3671e486dfeddd048c25d9e8a70c26de2e1bf6f97d',
                'highlights_sha256':'8e823819058d480c450ca4ef377a05675f6ed6f7c0460a69b972cdbe940c0893',
                'override':False}}
