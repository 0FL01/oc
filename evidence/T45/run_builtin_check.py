#!/usr/bin/env python3
"""Lossless immutable receipts for this sole-owner offline atomic."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import re
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')

def main():
    name, *command = sys.argv[1:]
    assert name.replace('-', '').isalnum() and command
    path = BENCH / ('t45-builtin-' + name + '.log.gz')
    env = dict(os.environ, CARGO_BUILD_JOBS='3', RUST_TEST_THREADS='1',
               CARGO_NET_OFFLINE='true', TMPDIR=str(BENCH), PYTHONDONTWRITEBYTECODE='1')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    diff = subprocess.check_output(['git', 'diff', 'HEAD', '--', 'crates'], cwd=ROOT)
    for file in ['crates/oc-adapters/src/defs/builtin_tests.rs', 'crates/oc-adapters/src/runtime/builtin_tests.rs']:
        diff += (ROOT/file).read_bytes()
    source_diff = hashlib.sha256(diff).hexdigest()
    started = time.monotonic()
    with path.open('xb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as log:
        process = subprocess.Popen(['timeout', '--kill-after=2s', '1798s', *command],
                                   cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        size = 0
        output = bytearray()
        for chunk in iter(lambda: process.stdout.read(8192), b''):
            size += len(chunk)
            log.write(chunk)
            output.extend(chunk)
        code = process.wait()
        counts = re.findall(r'test result: .*', output.decode(errors='replace'))
        receipt = dict(command=command, source=head, code_diff=source_diff, exit=code, raw_bytes=size,
                       counts=counts,
                       seconds=round(time.monotonic()-started, 3))
        log.write(('\nRECEIPT ' + json.dumps(receipt) + '\n').encode())
    print(json.dumps(dict(receipt, log=str(path), retained_bytes=path.stat().st_size)))
    if code:
        with gzip.open(path, 'rt') as log:
            text = log.read()
            print(text[-12000:])
    return code

if __name__ == '__main__':
    sys.exit(main())
