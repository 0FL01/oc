#!/usr/bin/env python3
"""Reap-check and remove only exact roots created by this slice's Cargo tests."""
import ctypes
import gzip
import json
from pathlib import Path
import shutil
import struct
import sys

BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')
LIBC = ctypes.CDLL(None,use_errno=True)

def birth(path):
    data = ctypes.create_string_buffer(256)
    assert LIBC.statx(-100,str(path).encode(),0x100,0x800,ctypes.byref(data))==0
    assert struct.unpack_from('<I',data.raw)[0]&0x800
    seconds,nanos = struct.unpack_from('<qI',data.raw,80)
    return seconds+nanos/1e9

def main():
    windows = []
    names = sys.argv[1:] or ['adapters1','workspace-final','workspace-final2']
    assert set(names).issubset({'adapters1','workspace-final','workspace-final2','workspace-current','workspace-qualified','workspace-verified'})
    for name in names:
        path = BENCH/('t45-plan-'+name+'.log.gz')
        receipt = json.loads(gzip.decompress(path.read_bytes()).decode().split('\nRECEIPT ')[-1])
        end = path.stat().st_mtime
        windows.append((name,end-receipt['seconds']-1,end+1))
    owners = []
    # Metadata only until ownership is proven. Inherited roots are excluded by
    # filesystem birth time, not mtime or PID-number guesses.
    for primary in BENCH.glob('oc-tui-workspace-primary-*'):
        if primary.is_symlink(): continue
        created = birth(primary)
        matches = [name for name,start,end in windows if start<=created<=end]
        if not matches: continue
        assert len(matches)==1
        pid = int(primary.name.rsplit('-',1)[1])
        assert not Path('/proc/'+str(pid)).exists(), 'owned Cargo child still alive'
        roots = [BENCH/f'oc-tui-workspace-{kind}-{pid}' for kind in ('primary','stale','vanished')]
        window = next((start,end) for name,start,end in windows if name==matches[0])
        assert all(path.is_dir() and not path.is_symlink() and window[0]<=birth(path)<=window[1] for path in roots)
        owners.append((matches[0],pid,roots))
    assert len(owners)==len(windows) and {name for name,_,_ in owners}=={name for name,_,_ in windows},owners
    for name,pid,roots in owners:
        for path in roots:
            shutil.rmtree(path)
            assert not path.exists()
        print(json.dumps({'receipt':name,'reaped_rust_test_pid':pid,'exact_roots_removed':[str(path) for path in roots]}),flush=True)

if __name__=='__main__': main()
