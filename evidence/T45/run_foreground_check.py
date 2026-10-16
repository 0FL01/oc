#!/usr/bin/env python3
"""Single-Cargo offline coordinator; immutable lossless bounded gzip logs."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')

def source():
    names = subprocess.check_output(['git','ls-files','crates'],cwd=ROOT,text=True).splitlines()
    names += ['crates/oc-adapters/src/runtime/turn/foreground.rs',
              'crates/oc-adapters/tests/fixtures/foreground_children.rs']
    digest = hashlib.sha256()
    for name in sorted(set(names)):
        path = ROOT/name
        if path.is_file():
            digest.update(name.encode()+b'\0'+path.read_bytes()+b'\0')
    return digest.hexdigest()

def main():
    name, *command = sys.argv[1:]
    assert name.replace('-','').isalnum() and command
    path = BENCH/('t45-foreground-'+name+'.log.gz')
    env = dict(os.environ,CARGO_BUILD_JOBS='3',RUST_TEST_THREADS='1',
               CARGO_NET_OFFLINE='true',TMPDIR=str(BENCH),PYTHONDONTWRITEBYTECODE='1')
    started = time.monotonic()
    association = source()
    with path.open('xb') as raw, gzip.GzipFile(fileobj=raw,mode='wb',mtime=0) as log:
        process = subprocess.Popen(['timeout','--kill-after=2s','1798s',*command],
            cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True)
        pgid = os.getpgid(process.pid)
        ticks = Path(f'/proc/{process.pid}/stat').read_text().split(') ')[1].split()[19]
        output = bytearray()
        for chunk in iter(lambda:process.stdout.read(8192),b''):
            log.write(chunk)
            output.extend(chunk)
            assert len(output)<=16*1024*1024, 'raw log cap'
        code = process.wait()
        process.stdout.close()
        text = output.decode(errors='replace')
        receipt = dict(command=command,head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            source=association,source_after=source(),exit=code,raw_bytes=len(output),
            counts=re.findall(r'test result: .*',text),seconds=round(time.monotonic()-started,3),
            owner=dict(pid=process.pid,pgid=pgid,startticks=ticks,reaped=True))
        log.write(('\nRECEIPT '+json.dumps(receipt)+'\n').encode())
    path.chmod(0o444)
    print(json.dumps(dict(receipt,log=str(path),retained_bytes=path.stat().st_size)))
    if code: print(text[-6000:])
    return code

if __name__=='__main__': sys.exit(main())
