#!/usr/bin/env python3
"""Serial, bounded, lossless gzip receipts for the synthetic R6 atomic."""
import gzip
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

CACHE = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')


def main():
    label, *command = sys.argv[1:]
    assert CACHE.is_dir() and label.replace('-', '').isalnum() and command
    path = CACHE / ('t45-profile-' + label + '.log.gz')
    assert not path.exists(), path
    env = os.environ.copy()
    env.update(CARGO_BUILD_JOBS='3', RUST_TEST_THREADS='1', CARGO_NET_OFFLINE='true', TMPDIR=str(CACHE))
    start = time.monotonic()
    size = 0
    with path.open('xb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0) as log:
        log.write((json.dumps({'argv': command, 'cwd': str(Path.cwd()), 'jobs': 3,
                               'threads': 1, 'offline': True, 'TMPDIR': str(CACHE)})+'\n').encode())
        process = subprocess.Popen(command, env=env, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        try:
            while selector.get_map():
                if time.monotonic()-start > 1798:
                    raise TimeoutError('owned operational deadline 1798s')
                for key, _ in selector.select(timeout=0.2):
                    chunk = os.read(key.fd, 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    size += len(chunk)
                    if size > 16*1024*1024:
                        raise RuntimeError('raw log cap')
                    log.write(chunk)
            code = process.wait(timeout=3)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
            selector.close()
            process.stdout.close()
        log.write((json.dumps({'exit': code, 'elapsed': round(time.monotonic()-start, 2),
                              'bytes': size, 'pid': process.pid, 'reaped': True})+'\n').encode())
    total = sum(p.stat().st_size for p in CACHE.glob('t45-profile-*.log.gz'))
    assert total + sum(p.stat().st_size for p in Path('evidence/T45').glob('*profile*') if p.is_file()) <= 1048576
    print(json.dumps({'exit': code, 'log': str(path), 'gzip_total': total,
                      'elapsed': round(time.monotonic()-start, 2)}), flush=True)
    return code


if __name__ == '__main__':
    sys.exit(main())
