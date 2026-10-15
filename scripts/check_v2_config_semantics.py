#!/usr/bin/env python3
"""Run shared native V2 fixtures through the pinned, real TS domain services.

All TS dependencies live in the external oracle checkout. The existing core test
wrapper supplies temporary HOME/XDG, in-memory DB and credential-free fixtures.
No production TS/Rust source or user configuration is rewritten.
"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

ORACLE = "b4f19bfe74a972c2919aa4c9d7445fa61f41b226"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle", required=True, type=Path)
    parser.add_argument("--bun", default="bun")
    args = parser.parse_args()
    oracle = args.oracle.resolve()
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=oracle, text=True
    ).strip()
    if revision != ORACLE:
        parser.error(f"oracle must be exactly {ORACLE}")
    subprocess.run(["git", "diff", "--exit-code", "--quiet"], cwd=oracle, check=True)
    root = Path(__file__).resolve().parents[1]
    core = oracle / "packages/core"
    copies = [
        (root / "scripts/fixtures/v2-config-semantics.test.ts",
         core / "test/config/oc-native-v2-config.test.ts"),
        (root / "crates/oc-adapters/tests/fixtures/v2-config-semantics.json",
         core / "test/fixture/oc-native-v2-config.fixture.json"),
    ]
    if any(target.exists() for _, target in copies):
        parser.error("refusing to replace existing oracle fixture files")
    created = []
    try:
        for source, target in copies:
            shutil.copyfile(source, target)
            created.append(target)
        environment = {
            "PATH": os.environ["PATH"],
            "HOME": str(oracle.parent / "t65-oracle-home"),
            "TMPDIR": str(oracle.parent),
            "LANG": "C.UTF-8",
            "LC_ALL": "C.UTF-8",
        }
        result = subprocess.run(
            [args.bun, "run", "script/test.ts", "test/config/oc-native-v2-config.test.ts"],
            cwd=core, env=environment, timeout=300,
        )
        return result.returncode
    finally:
        for target in reversed(created):
            target.unlink()


if __name__ == "__main__":
    raise SystemExit(main())
