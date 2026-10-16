#!/usr/bin/env python3
"""Sequential bounded offline receipts; no product timeout changes."""
import gzip
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')

def main():
    name, *command = sys.argv[1:]
    assert name.replace('-', '').isalnum() and command
    env = dict(os.environ, CARGO_BUILD_JOBS='3', RUST_TEST_THREADS='1',
               CARGO_NET_OFFLINE='true', TMPDIR=str(BENCH), PYTHONDONTWRITEBYTECODE='1')
    path = BENCH / ('t45-controls-' + name + '.log.gz')
    started = time.monotonic()
    with gzip.open(path, 'wb') as log:
        process = subprocess.Popen(['timeout', '--kill-after=2s', '1798s', *command],
                                   cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        size = 0
        for chunk in iter(lambda: process.stdout.read(8192), b''):
            size += len(chunk)
            if size > 16 * 1024 * 1024:
                process.terminate()
                process.wait()
                raise RuntimeError('receipt exceeds raw quota')
            log.write(chunk)
        code = process.wait()
        receipt = dict(command=command, exit=code, raw_bytes=size,
                       seconds=round(time.monotonic()-started, 3))
        log.write(('\nRECEIPT ' + json.dumps(receipt) + '\n').encode())
    print(json.dumps(dict(receipt, log=str(path), retained_bytes=path.stat().st_size)))
    if code:
        with gzip.open(path, 'rt') as log:
            print(log.read())
    return code

if __name__ == '__main__':
    sys.exit(main())
