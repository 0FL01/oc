#!/usr/bin/env python3
"""Explicit fixed-authority Go qualification; never prints credential material."""
from pathlib import Path
import os
import shlex
import subprocess
import sys


def main() -> int:
    if sys.argv[1:] != ["--run"]:
        print("Explicit opt-in required: python3 scripts/t53_go_live.py --run", file=sys.stderr)
        return 2
    root = Path(__file__).resolve().parent.parent
    source = root / ".local/live.env"
    if source.is_symlink() or source.stat().st_uid != os.geteuid():
        raise SystemExit("Authorized input file ownership refused")
    key = None
    for line in source.read_text().splitlines():
        line = line.strip().removeprefix("export ")
        if line.startswith("#") or "=" not in line:
            continue
        name, value = line.split("=", 1)
        if name.strip() == "OC_API_KEY":
            parts = shlex.split(value, comments=True)
            if len(parts) != 1 or not parts[0]:
                raise SystemExit("Authorized OC_API_KEY input is malformed")
            key = parts[0]
    if not key:
        raise SystemExit("Authorized OC_API_KEY input is absent")
    env = os.environ.copy()
    for name in ("OC_API_KEY", "LUDKA2_API_URL", "LUDKA2_API_KEY", "OC_TEST_MODEL"):
        env.pop(name, None)
    env.update(OPENCODE_API_KEY=key, OC_GO_LIVE="1",
               OC_GO_LIVE_LEDGER=str(root / "evidence/T53/live-campaign.json"),
               TMPDIR="/home/opencode/.cache/opencode-tmp/opencode",
               CARGO_BUILD_JOBS="3", RUST_TEST_THREADS="1")
    return subprocess.run(["cargo", "test", "--locked", "-p", "oc-adapters", "--lib",
                           "provider::go_live_tests::go06_bounded_native_go_live", "--",
                           "--exact", "--ignored", "--nocapture"], cwd=root, env=env).returncode


if __name__ == "__main__":
    raise SystemExit(main())
