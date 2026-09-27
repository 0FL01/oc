#!/usr/bin/env python3
"""Real released native PTY against an already committed, held owner session."""
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

spec = json.loads(Path(sys.argv[1]).read_text())
source = json.loads(Path(spec['native_spec']).read_text())
assert hashlib.sha256(Path(source['binary']).read_bytes()).hexdigest() == source['binary_sha256']
assert source['argv'] == [source['binary'], 'tui', '--session', source['session']]
assert source['env']['HOME'].startswith('/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/')
env = {**source['env'], 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'SHELL': '/bin/sh'}


def emit(value):
    print(json.dumps(value, ensure_ascii=False), flush=True)


master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))


def session():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen(source['argv'], cwd=source['cwd'], env=env,
                         stdin=slave, stdout=slave, stderr=slave, preexec_fn=session)
os.close(slave)
emit({'kind': 'launch', 'argv': source['argv'], 'cwd': source['cwd'],
      'binary_sha256': source['binary_sha256'], 'native_spec': source,
      'inherited_env': False, 'env_names': sorted(env), 'history_ingress': False})
pending, forced = b'', False
try:
    while child.poll() is None:
        ready, _, _ = select.select([master, sys.stdin], [], [], .1)
        if master in ready:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if data:
                emit({'kind': 'output', 'at_ns': time.monotonic_ns(), 'data': base64.b64encode(data).decode()})
        if sys.stdin in ready:
            data = os.read(sys.stdin.fileno(), 65536)
            if not data:
                forced = True
                break
            pending += data
            while b'\n' in pending:
                line, pending = pending.split(b'\n', 1)
                value = json.loads(line)
                if value['kind'] == 'input':
                    os.write(master, base64.b64decode(value['data']))
                elif value['kind'] == 'stop':
                    forced = True
            if forced:
                break
finally:
    if child.poll() is None and not forced:
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            pass
    if child.poll() is None:
        forced = True
        os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()
    os.close(master)
    emit({'kind': 'exit', 'code': child.returncode, 'termination': 'forced_stop' if forced else 'natural'})
