#!/usr/bin/env python3
"""Freeze donor syntax inputs for native compilation, never production downloads.

Import only public generated C grammars, query inputs and their licenses. Sources
live in the approved development cache; output archives preserve complete source
bytes, relative scanner includes and attribution without vendoring Git metadata.
"""

import argparse
import gzip
import hashlib
import io
import json
import pathlib
import re
import subprocess
import tarfile
from urllib.parse import urlparse

ROOT = pathlib.Path(__file__).resolve().parents[1]
CACHE = pathlib.Path("/home/opencode/.cache/opencode-tmp/opencode")
OPENTUI = CACHE / "t44-opentui-0.5.10"
NVIM = CACHE / "t44-nvim-queries"
OUT = ROOT / "crates/oc-tui/assets/syntax"
QUERY_REPOS = {}


def command(*args):
    return subprocess.check_output(args, text=True, timeout=120).strip()


def head(path):
    return command("git", "-C", str(path), "rev-parse", "HEAD")


def checkout(repository, ref):
    path = CACHE / ("syntax-" + repository.replace("/", "-") + "-" + ref.replace("/", "-"))
    if not path.exists():
        url = "https://github.com/" + repository + ".git"
        if re.fullmatch(r"[0-9a-f]{40}", ref):
            subprocess.run(["git", "clone", "--no-checkout", "--depth", "1", url, str(path)], check=True, timeout=120)
            subprocess.run(["git", "-C", str(path), "fetch", "--depth", "1", "origin", ref], check=True, timeout=120)
            subprocess.run(["git", "-C", str(path), "checkout", "--detach", ref], check=True, timeout=120)
        else:
            subprocess.run(["git", "clone", "--depth", "1", "--branch", ref, url, str(path)], check=True, timeout=120)
    if command("git", "-C", str(path), "remote", "get-url", "origin") != "https://github.com/" + repository + ".git":
        raise ValueError("Source cache origin mismatch")
    return path


def descriptors(path):
    script = f'import config from {json.dumps(str(path))}; console.log(JSON.stringify(config.parsers));'
    return json.loads(command("bun", "-e", script))


def query_source(url):
    if url.startswith("./assets/"):
        path = OPENTUI / "packages/core/src/lib/tree-sitter" / url[2:]
        QUERY_REPOS["anomalyco/opentui"] = OPENTUI
        return path.read_bytes(), {"repository": "anomalyco/opentui", "revision": head(OPENTUI), "path": path.relative_to(OPENTUI).as_posix()}
    parts = urlparse(url).path.strip("/").split("/")
    repository = "/".join(parts[:2])
    rest = parts[2:]
    if rest[0] == "raw":
        rest = rest[1:]
    if rest[:2] == ["refs", "heads"]:
        rest = rest[2:]
    ref, relative = rest[0], "/".join(rest[1:])
    source = NVIM if repository == "nvim-treesitter/nvim-treesitter" else checkout(repository, ref)
    QUERY_REPOS[repository] = source
    path = source / relative
    return path.read_bytes(), {"repository": repository, "revision": head(source), "path": relative}


def highlights(url):
    data, provenance = query_source(url)
    # OpenTUI fetchHighlightQueries concatenates ONLY configured sources. Neovim's
    # `; inherits:` remains a comment there; expanding it changes the oracle and
    # can introduce directives its Web Tree-sitter binding cannot even compile.
    provenance["sha256"] = hashlib.sha256(data).hexdigest()
    return data, [provenance]


def grammar(descriptor):
    name = descriptor["filetype"]
    parts = urlparse(descriptor["wasm"]).path.strip("/").split("/")
    repository = "/".join(parts[:2])
    ref = parts[4] if parts[2:4] == ["releases", "download"] else None
    if name == "nix":
        # The donor uses an ast-grep WASM, not a tagged native-source URL.
        # Keep its independent reference recorded; fixture parity must verify
        # this exact native source, never silently call the versions identical.
        repository, ref = "nix-community/tree-sitter-nix", "master"
    if ref is None:
        raise ValueError("Unrecognized grammar release reference")
    source = checkout(repository, ref)
    subdirectory = {
        "php": "php", "ocaml": "grammars/ocaml", "fsharp": "fsharp",
        "xml": "xml", "markdown": "tree-sitter-markdown",
        "markdown_inline": "tree-sitter-markdown-inline", "typescript": "typescript",
    }.get(name, "")
    parser = source / subdirectory / "src/parser.c"
    generated = None
    if not parser.is_file():
        candidates = list(source.glob("**/src/parser.c"))
        if not candidates and (source / subdirectory / "grammar.js").is_file():
            cli = CACHE / "syntax-cli/bin/tree-sitter"
            if command(str(cli), "--version") != "tree-sitter 0.23.2":
                raise ValueError("Wrong pinned native grammar generator")
            subprocess.run([str(cli), "generate", "--abi", "14", "--no-bindings"],
                           cwd=source / subdirectory, check=True, timeout=240)
            candidates = list(source.glob("**/src/parser.c"))
            generated = {"version": "tree-sitter-cli 0.23.2", "abi": 14}
        if len(candidates) != 1:
            raise ValueError(f"Ambiguous native parser source for {name}")
        parser = candidates[0]
    parser_dir = parser.parent
    if subprocess.run(["git", "-C", str(source), "ls-files", "--error-unmatch",
                       parser.relative_to(source).as_posix()], stdout=subprocess.DEVNULL,
                      stderr=subprocess.DEVNULL, check=False).returncode:
        if name != "swift":
            raise ValueError("Unrecorded generated native grammar source")
        # Repeated imports must not lose the already generated source's origin.
        generated = {"version": "tree-sitter-cli 0.23.2", "abi": 14}
    scanner = [p for p in parser_dir.glob("scanner.*") if p.suffix in {".c", ".cc", ".cpp"}]
    symbols = re.findall(rb"(?:TS_PUBLIC\s+)?const TSLanguage\s*\*\s*(tree_sitter_[a-z_]+)\s*\(void\)", parser.read_bytes())
    if len(symbols) != 1:
        raise ValueError(f"Missing unique generated native language entry for {name}")
    payload = io.BytesIO()
    paths = []
    # Keep C sources and all relative headers: scanner shared directories differ
    # among grammars. Do not pack examples, tests, binaries, or build output.
    for p in source.rglob("*"):
        if not p.is_file() or p.is_symlink() or ".git" in p.parts:
            continue
        relative = p.relative_to(source)
        if p.suffix in {".h", ".hpp"} or p.parent == parser_dir and p.suffix in {".c", ".cc", ".cpp", ".json"} or p.name.lower().startswith(("license", "copying", "notice")):
            paths.append((relative.as_posix(), p))
    licenses = [name for name, _ in paths if pathlib.PurePosixPath(name).name.lower().startswith(("license", "copying"))]
    if not licenses:
        raise ValueError(f"No license found for {name}")
    with tarfile.open(fileobj=payload, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for relative, p in sorted(paths):
            data = p.read_bytes()
            info = tarfile.TarInfo(relative)
            info.mode, info.size = 0o644, len(data)
            tar.addfile(info, io.BytesIO(data))
    encoded = gzip.compress(payload.getvalue(), mtime=0)
    filename = name + ".tar.gz"
    (OUT / filename).write_bytes(encoded)
    return {
        "name": name, "aliases": descriptor.get("aliases", []), "repository": repository,
        "revision": head(source), "release": ref, "reference_wasm": descriptor["wasm"],
        "archive": filename, "archive_sha256": hashlib.sha256(encoded).hexdigest(),
        "parser": parser.relative_to(source).as_posix(),
        "scanners": [p.relative_to(source).as_posix() for p in scanner],
        "symbol": symbols[0].decode(), "licenses": licenses,
        "generator": generated, "injection_mapping": descriptor.get("injectionMapping", {}),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--languages", help="Comma-separated development import subset; default full donor inventory")
    args = parser.parse_args()
    if head(OPENTUI) != "f6673a04ccb671b9207da358c57152bfd27c781f":
        raise ValueError("Wrong pinned OpenTUI source")
    parsers = descriptors(OPENTUI / "packages/core/src/lib/tree-sitter/parsers-config.ts")
    parsers += descriptors(ROOT / "opencode/packages/tui/src/parsers-config.ts")
    if args.languages:
        selected = set(args.languages.split(","))
        parsers = [p for p in parsers if p["filetype"] in selected]
        if {p["filetype"] for p in parsers} != selected:
            raise ValueError("Unknown import language")
    OUT.mkdir(exist_ok=True)
    records = []
    for descriptor in parsers:
        entry = grammar(descriptor)
        entry["queries"] = {}
        for kind, urls in descriptor["queries"].items():
            combined, sources, parts = [], [], []
            # The shipped built-in file already concatenates its configured
            # sources. Import it once, not once per original remote URL.
            bundled = OPENTUI / "packages/core/src/lib/tree-sitter/assets" / entry["name"] / (kind + ".scm")
            actual_urls = ["./assets/" + entry["name"] + "/" + kind + ".scm"] if bundled.is_file() else urls
            if entry["name"] == "swift" and kind == "highlights":
                # The donor's floating `main` is now 0.7.4 and cannot compile
                # against its actual release 0.7.1 WASM (unsafe_expression is
                # absent). Freeze the official SAME-release query for the native
                # and reference fixture, recording the original URL, not a
                # hand-authored replacement or a missing-grammar waiver.
                entry["reference_query_freeze"] = {
                    "original_urls": urls,
                    "reason": "Floating main query is incompatible with the configured release WASM",
                    "reference_revision": entry["revision"],
                }
                actual_urls = ["https://raw.githubusercontent.com/alex-pinkus/tree-sitter-swift/0.7.1/queries/highlights.scm"]
            for number, actual in enumerate(actual_urls):
                if kind == "highlights":
                    data, provenance = highlights(actual)
                    sources.extend(provenance)
                else:
                    data, provenance = query_source(actual)
                    provenance["sha256"] = hashlib.sha256(data).hexdigest()
                    sources.append(provenance)
                combined.append(data)
                # The real reference worker caches URL parts before joining.
                # Swift keeps its original URL key with the explicitly frozen
                # same-release bytes, never a synthetic/modified query.
                if actual.startswith("https://"):
                    part_file = f"{entry['name']}.{kind}.{number}.scm"
                    (OUT / part_file).write_bytes(data)
                    parts.append({"file": part_file, "sha256": hashlib.sha256(data).hexdigest(),
                                  "source_url": actual, "cache_url": urls[number]})
            filename = entry["name"] + "." + kind + ".scm"
            query = b"\n".join(combined)
            (OUT / filename).write_bytes(query)
            entry["queries"][kind] = {"file": filename, "sha256": hashlib.sha256(query).hexdigest(), "sources": sources, "parts": parts}
        records.append(entry)
        print("Imported", entry["name"], entry["revision"], entry["archive_sha256"])
    query_licenses = []
    for repository, source in sorted(QUERY_REPOS.items()):
        paths = [p for p in source.iterdir() if p.is_file() and p.name.lower().startswith(("license", "copying"))]
        if not paths:
            raise ValueError("Missing query-source license: " + repository)
        for path in sorted(paths):
            filename = "license-" + repository.replace("/", "-") + "-" + path.name
            data = path.read_bytes()
            (OUT / filename).write_bytes(data)
            query_licenses.append({"repository": repository, "revision": head(source),
                                   "source_path": path.name, "file": filename,
                                   "sha256": hashlib.sha256(data).hexdigest()})
    filetypes_path = ROOT / "opencode/packages/tui/src/util/filetype.ts"
    filetypes = json.loads(command("bun", "-e", f'import {{ LANGUAGE_EXTENSIONS }} from {json.dumps(str(filetypes_path))}; console.log(JSON.stringify(LANGUAGE_EXTENSIONS));'))
    resolver_path = OPENTUI / "packages/core/src/lib/tree-sitter/resolve-ft.ts"
    resolver = json.loads(command("bun", "-e", f'import {{ extensionToFiletype, basenameToFiletype }} from {json.dumps(str(resolver_path))}; console.log(JSON.stringify({{extensions:Object.fromEntries(extensionToFiletype),basenames:Object.fromEntries(basenameToFiletype)}}));'))
    resolver["source_sha256"] = hashlib.sha256(resolver_path.read_bytes()).hexdigest()
    manifest = {"version": 1, "donor": "2670273ff17da96f85c5826ced57aa1b368754fa", "opentui": head(OPENTUI), "query_revision": head(NVIM), "query_licenses": query_licenses, "filetypes": filetypes, "filetypes_sha256": hashlib.sha256(filetypes_path.read_bytes()).hexdigest(), "info_resolver": resolver, "grammars": records}
    (OUT / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
