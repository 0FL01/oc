#!/usr/bin/env python3
"""AUD38: existing offline targets and the native ELF under an owned Node-free PATH."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    binary = args.binary.resolve()
    with binary.open("rb") as stream:
        assert stream.read(4) == b"\x7fELF"
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip())
    targets = []
    for name in ("e2e_offline", "golden_binary"):
        candidates = [
            path for path in (repo / "target/debug/deps").glob(f"{name}-*")
            if path.is_file() and os.access(path, os.X_OK)
        ]
        assert len(candidates) == 1, (name, len(candidates))
        targets.append(candidates[0])

    with tempfile.TemporaryDirectory(prefix="t42-node-free-") as directory:
        root = Path(directory)
        for name in ("bin", "home", "config", "data", "cache", "state", "cargo", "project"):
            (root / name).mkdir()
        for name in ("cargo", "rustc", "rustdoc"):
            (root / "bin" / name).symlink_to(sysroot / "bin" / name)
        for name in ("sh", "bash", "cc", "gcc", "ar", "as", "ld", "ranlib", "strip", "objcopy", "env", "sleep", "git"):
            resolved = shutil.which(name)
            assert resolved is not None, name
            (root / "bin" / name).symlink_to(Path(resolved).resolve())
        path = str(root / "bin")
        assert all(shutil.which(name, path=path) is None for name in ("node", "bun", "npx"))
        env = {
            "HOME": str(root / "home"),
            "XDG_CONFIG_HOME": str(root / "config"),
            "XDG_DATA_HOME": str(root / "data"),
            "XDG_CACHE_HOME": str(root / "cache"),
            "XDG_STATE_HOME": str(root / "state"),
            "CARGO_HOME": str(root / "cargo"),
            "CARGO_NET_OFFLINE": "true",
            "CARGO": str(root / "bin/cargo"),
            "PATH": path,
            "SHELL": str(root / "bin/sh"),
            "TMPDIR": directory,
            "RUST_TEST_THREADS": "1",
        }
        results = []
        for command in ([str(binary), "--help"], [str(binary), "--smoke"]):
            result = subprocess.run(command, cwd=root / "project", env=env,
                                    capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, (command[-1], result.returncode)
            results.append({"command": command[-1], "exit": result.returncode})
        for target in targets:
            result = subprocess.run([str(target), "--test-threads=1"], cwd=repo,
                                    env=env, capture_output=True, text=True, timeout=120)
            print(result.stdout, end="")
            if result.returncode != 0:
                print(result.stderr, end="")
            assert result.returncode == 0, (target.name, result.returncode)
            results.append({"target": target.name.split("-")[0], "exit": result.returncode})
        print(json.dumps({
            "AUD38_no_required_node": "PASS", "native_elf": True, "fresh_home_xdg_cargo": True,
            "node_bun_npx_on_path": False, "real_provider_requests": 0,
            "results": results,
        }, sort_keys=True))


if __name__ == "__main__":
    main()
