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
import sqlite3
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


def observe(label):
    lifecycle = source['durable_lifecycle']
    database = Path(lifecycle['database']).resolve()
    assert database.is_relative_to(Path(source['env']['XDG_DATA_HOME']).resolve())
    with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
        connection.row_factory = sqlite3.Row
        tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type IN ('table','view')")}
        wanted = ('messages', 'conversation_messages', 'tool_operations', 'conversation_tools',
                  'compression_blocks', 'compression_members', 'dcp_run_views', 'dcp_accounting',
                  'dcp_coverage', 'dcp_run_identity', 'conversation_state', 'conversation_redo',
                  'conversation_points', 'conversation_exclusions', 'conversation_contexts',
                  'conversation_revision', 'conversation_objects', 'conversation_versions', 'sessions')
        data = {name: [dict(r) for r in connection.execute(f'SELECT * FROM "{name}" LIMIT 101')]
                for name in wanted if name in tables}
        assert all(len(value) <= 100 for value in data.values()), 'Bounded lifecycle observation cap exceeded'
    emit({'kind': 'owner_snapshot', 'label': label, 'at_ns': time.monotonic_ns(),
          'database': str(database), 'read_only': True, 'limit': 100, 'data': data})


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
                elif value['kind'] == 'observe':
                    observe(value['label'])
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
