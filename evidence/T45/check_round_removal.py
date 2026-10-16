#!/usr/bin/env python3
"""Single offline check owner; lossless compressed logs and outer watchdog only."""
import gzip
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
LOGS = ROOT / 'evidence/T45/round-removal-logs'
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')


def main():
    label, *command = sys.argv[1:]
    assert label and '/' not in label and command
    LOGS.mkdir(exist_ok=True)
    path = LOGS / (label + '.log.gz')
    assert not path.exists(), path
    env = dict(os.environ, CARGO_BUILD_JOBS='3', RUST_TEST_THREADS='1',
               CARGO_NET_OFFLINE='true', TMPDIR=str(TMP), PYTHONDONTWRITEBYTECODE='1')
    before = set(TMP.iterdir())
    start = time.monotonic()
    with gzip.open(path, 'wb', compresslevel=9) as log:
        log.write((json.dumps({'command': command, 'jobs': 3, 'threads': 1,
                               'offline': True, 'outer_watchdog_seconds': 1798}) + '\n').encode())
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        watchdog = threading.Timer(1798, lambda: os.killpg(process.pid, signal.SIGKILL))
        watchdog.start()
        try:
            while block := process.stdout.read1(65536):
                log.write(block)
                log.flush()
            code = process.wait()
        finally:
            watchdog.cancel()
            watchdog.join()
            process.stdout.close()
        log.write((json.dumps({'exit': code, 'seconds': round(time.monotonic()-start, 3)}) + '\n').encode())
    total = sum(p.stat().st_size for p in LOGS.iterdir())
    assert total <= 1024*1024, total
    print(json.dumps({'label': label, 'command': command, 'exit': code,
                      'seconds': round(time.monotonic()-start, 3), 'lossless_log': str(path.relative_to(ROOT)),
                      'total_compressed_bytes': total,
                      'new_tmp_paths': [str(p) for p in set(TMP.iterdir())-before]}))
    return code


if __name__ == '__main__':
    sys.exit(main())
