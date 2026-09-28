#!/usr/bin/env python3
"""VIS41 actual CLI/PTy owner. Clean env, bounded loopback Responses, read-only facts."""
import base64
import fcntl
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import sqlite3
import struct
import subprocess
import sys
import termios
import threading
import time

spec = json.loads(Path(sys.argv[1]).read_text())
assert spec['origin'] in ('upstream', 'oc')
assert hashlib.sha256(Path(spec['binary']).read_bytes()).hexdigest() == spec['binary_sha256']
root = Path(spec['root'])
assert root.parent == Path('/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs')
root.mkdir()
home, project = root / 'home', Path(spec.get('project') or root / 'project')
assert project.resolve().is_relative_to(root.parent.resolve()) and project.name == 'project'
assert not project.exists() or not any(project.iterdir()), 'Shared fixture project is no longer empty'
for directory in [project, *[home / p for p in ('config/opencode', 'cache', 'data', 'state')]]:
    directory.mkdir(parents=True, exist_ok=True)
lock = threading.Lock()
release = threading.Event()
requests, completed = [], []


def emit(event):
    with lock:
        print(json.dumps({'at_ns': time.monotonic_ns(), **event}, ensure_ascii=False), flush=True)


class Provider(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *_):
        pass

    def do_POST(self):
        raw = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        body = json.loads(raw)
        system = str(body.get('instructions', '')) + '\n' + '\n'.join(
            str(item.get('content', '')) for item in body.get('input', [])
            if isinstance(item, dict) and item.get('role') in ('system', 'developer'))
        title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
        text = json.dumps(body.get('input', []), ensure_ascii=False)
        prompts = []
        for item in body.get('input', []):
            if not isinstance(item, dict) or item.get('role') != 'user':
                continue
            content = item.get('content', [])
            prompts.append(content if isinstance(content, str) else ''.join(p.get('text', '') for p in content if isinstance(p, dict)))
        latest = prompts[-1] if prompts else ''
        name = next((n for n in ('seed-a', 'seed-b', 'complete', 'cancel') if latest == 'VIS41 ' + n + ': no tools or filesystem effects.'), None)
        if title:
            name = 'seed-b' if 'VIS41 seed-b:' in text else 'seed-a'
        valid = self.path == '/v1/responses' and body.get('model') == 'fixture-model-1' and body.get('stream') is True and name is not None
        with lock:
            index = len(requests)
            operation = 'title' if title else 'main'
            if index >= 6 or any(r['operation'] == operation and r['case'] == name for r in requests):
                valid = False
            record = {'index': index, 'operation': operation, 'case': name, 'valid': valid,
                      'body_sha256': hashlib.sha256(raw).hexdigest(), 'input': body.get('input'),
                      'tool_names': [t.get('name') for t in body.get('tools', [])]}
            requests.append(record)
        emit({'kind': 'provider', **record})
        if not valid:
            payload = b'{"error":{"message":"VIS41 bounded fixture rejected request"}}'
            self.send_response(400)
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        answer = ('VIS41 fixture B' if name == 'seed-b' else 'VIS41 fixture A') if title else 'VIS41-DONE ' + name
        response = {'id': 'resp_vis41_' + str(index), 'object': 'response', 'status': 'in_progress', 'model': 'fixture-model-1', 'output': []}
        item = {'id': 'msg_vis41_' + str(index), 'type': 'message', 'role': 'assistant', 'status': 'completed',
                'content': [{'type': 'output_text', 'text': answer, 'annotations': []}]}
        events = [
            {'type': 'response.created', 'response': response},
            {'type': 'response.output_item.added', 'output_index': 0, 'item': {**item, 'status': 'in_progress', 'content': []}},
            {'type': 'response.content_part.added', 'output_index': 0, 'item_id': item['id'], 'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
            {'type': 'response.output_text.delta', 'output_index': 0, 'item_id': item['id'], 'content_index': 0, 'delta': answer},
            {'type': 'response.output_text.done', 'output_index': 0, 'item_id': item['id'], 'content_index': 0, 'text': answer},
            {'type': 'response.content_part.done', 'output_index': 0, 'item_id': item['id'], 'content_index': 0, 'part': item['content'][0]},
            {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
            {'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [item],
                'usage': {'input_tokens': 512, 'output_tokens': 16, 'total_tokens': 528, 'input_tokens_details': {'cached_tokens': 0}, 'output_tokens_details': {'reasoning_tokens': 0}}}},
        ]
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Connection', 'close')
        self.end_headers()
        try:
            for sequence, event in enumerate(events):
                self.wfile.write(('event: ' + event['type'] + '\ndata: ' + json.dumps({**event, 'sequence_number': sequence}, ensure_ascii=False) + '\n\n').encode())
                self.wfile.flush()
                if sequence == 0 and name in ('complete', 'cancel') and not title:
                    release.clear()
                    emit({'kind': 'provider_held', 'case': name, 'index': index, 'timeout_seconds': 60})
                    end = time.monotonic() + 60
                    while not release.is_set() and time.monotonic() < end:
                        if select.select([self.connection], [], [], .05)[0]:
                            if not self.connection.recv(1, socket.MSG_PEEK):
                                emit({'kind': 'provider_disconnected', 'case': name, 'index': index})
                                return
                    if not release.is_set():
                        emit({'kind': 'barrier_timeout', 'case': name, 'index': index})
                        return
            completed.append(index)
            emit({'kind': 'provider_completed', 'case': name, 'index': index, 'operation': operation})
        except (BrokenPipeError, ConnectionResetError):
            emit({'kind': 'provider_disconnected', 'case': name, 'index': index})
        self.close_connection = True


server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
settings = {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'fixture-not-secret'}
models = {'fixture-model-1': {'name': 'VIS41 Fixture', 'limit': {'context': 65536, 'output': 2048}}}
cli = {'theme': {'name': 'opencode', 'mode': 'dark'}, 'animations': spec['animations'],
       'session': {'sidebar': 'hide', 'tps': False}, 'tabs': {'layout': spec['tabs'], 'indicators': spec['indicators']},
       'debug': {'devtools': False}, 'attention': {'notifications': False, 'sound': False},
       'cursor': {'style': 'block', 'blinking': False}}
if spec['origin'] == 'upstream':
    config = {'model': 'fixture/fixture-model-1', 'plugins': ['-opencode.models.dev'], 'share': 'disabled', 'update': 'disable',
              'snapshots': False, 'compaction': {'auto': False},
              'providers': {'fixture': {'name': 'Fixture', 'package': '@opencode/ai/providers/openai/responses', 'settings': settings, 'models': models}},
              'permissions': [{'action': '*', 'resource': '*', 'effect': 'deny'}]}
    argv = [spec['binary'], '--standalone']
else:
    config = {'model': 'fixture/fixture-model-1', 'animations': spec['animations'], 'snapshots': False,
              'compaction': {'auto': False}, 'permissions': {'*': 'deny'},
              'provider': {'fixture': {'name': 'Fixture', 'npm': '@ai-sdk/openai', 'options': settings, 'models': models}}}
    argv = [spec['binary'], 'tui']
for file, content in [('opencode.json', config), ('cli.json', cli)]:
    (home / 'config/opencode' / file).write_text(json.dumps(content))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_CACHE_HOME': str(home / 'cache'),
       'XDG_DATA_HOME': str(home / 'data'), 'XDG_STATE_HOME': str(home / 'state'), 'PATH': '/usr/bin:/bin',
       'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'SHELL': '/bin/sh', 'TZ': 'UTC',
       'OC_TEST_ALLOW_LOOPBACK': '1', 'OPENCODE_TEST_HOME': str(home), 'OPENCODE_DISABLE_AUTOUPDATE': '1',
       'OPENCODE_DISABLE_MODELS_FETCH': 'true', 'OPENCODE_DISABLE_FILEWATCHER': 'true',
       'OPENCODE_CONFIG_PROJECT_DISABLE': 'true', 'OPENCODE_CONFIG_CONTENT': json.dumps(config)}
if spec['origin'] == 'oc':
    env['OC_TUI_TEST_METRICS'] = str(Path(spec['evidence']) / 'native-ui-metrics.json')
version = subprocess.run([spec['binary'], '--version'], env=env, cwd=project, capture_output=True, timeout=20)
emit({'kind': 'launch', 'argv': argv, 'cwd': str(project), 'env_names': sorted(env), 'inherited_environment': False,
      'version': version.stdout.decode().strip(), 'version_exit': version.returncode, 'binary_sha256': spec['binary_sha256'],
      'config': config, 'cli_config': cli, 'history_ingress': False, 'authenticated_api_calls': 0,
      'reference_project_reused': bool(spec.get('reference_project'))})
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))


def session():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen(argv, env=env, cwd=project, stdin=slave, stdout=slave, stderr=slave, preexec_fn=session)
os.close(slave)
emit({'kind': 'process_started', 'pid': child.pid})


def process_sample(request_id):
    # CPU is per actual owned product process, never the Python/browser harness.
    stat = Path(f'/proc/{child.pid}/stat').read_text().rsplit(') ', 1)[1].split()
    threads = []
    for directory in Path(f'/proc/{child.pid}/task').iterdir():
        try:
            sample_start = time.monotonic_ns()
            status = dict(line.split(':', 1) for line in (directory / 'status').read_text().splitlines() if ':' in line)
            scheduler = [int(value) for value in (directory / 'schedstat').read_text().split()]
            threads.append({'tid': int(directory.name), 'voluntary': int(status['voluntary_ctxt_switches']),
                            'involuntary': int(status['nonvoluntary_ctxt_switches']),
                            'scheduler_runtime_ns': scheduler[0], 'scheduler_wait_ns': scheduler[1],
                            'scheduler_timeslices': scheduler[2], 'state': status['State'].strip(),
                            'sample_start_ns': sample_start, 'sample_end_ns': time.monotonic_ns()})
        except FileNotFoundError:
            pass
    emit({'kind': 'process_sample', 'request_id': request_id, 'pid': child.pid, 'cpu_ticks': int(stat[11]) + int(stat[12]),
          'clock_ticks_per_second': os.sysconf('SC_CLK_TCK'), 'threads': threads,
          'voluntary_context_switches': sum(t['voluntary'] for t in threads),
          'involuntary_context_switches': sum(t['involuntary'] for t in threads)})


def snapshot(request_id):
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db', '.sqlite', '.sqlite3'):
            continue
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            for table in ('session', 'session_v2', 'sessions', 'session_message', 'message', 'messages', 'turns', 'tool_ops', 'tool_executions', 'prefs'):
                if table not in tables:
                    continue
                count = connection.execute(f'SELECT COUNT(*) FROM {table}').fetchone()[0]
                rows = [dict(r) for r in connection.execute(f'SELECT * FROM {table} LIMIT 100')]
                for row in rows:
                    for key, value in row.items():
                        if isinstance(value, bytes):
                            row[key] = {'bytes_sha256': hashlib.sha256(value).hexdigest(), 'length': len(value)}
                observations.append({'database': str(database), 'table': table, 'count': count, 'rows': rows})
    # Read only the fixture's real client persistence, never authoring-agent state.
    client = []
    for file in home.rglob('*'):
        if file.is_file() and file.suffix == '.json' and file.stat().st_size < 262144 and ('tui' in str(file) or file.name == 'kv.json'):
            try:
                client.append({'path': str(file.relative_to(home)), 'data': json.loads(file.read_text())})
            except (ValueError, UnicodeDecodeError):
                pass
    emit({'kind': 'owner_snapshot', 'request_id': request_id, 'observations': observations, 'client_persistence': client,
          'project_files': {str(file.relative_to(project)): hashlib.sha256(file.read_bytes()).hexdigest() for file in project.rglob('*') if file.is_file()},
          'requests': len(requests), 'completed': len(completed), 'invalid': sum(not r['valid'] for r in requests), 'readonly_sqlite': True})


pending, forced = b'', False
try:
    end = time.monotonic() + 240
    while child.poll() is None and time.monotonic() < end:
        ready, _, _ = select.select([master, sys.stdin], [], [], .1)
        if master in ready:
            try:
                data = os.read(master, 65536)
            except OSError:
                break
            if data:
                emit({'kind': 'output', 'data': base64.b64encode(data).decode()})
        if sys.stdin in ready:
            data = os.read(sys.stdin.fileno(), 65536)
            if not data:
                forced = True
                break
            pending += data
            while b'\n' in pending:
                line, pending = pending.split(b'\n', 1)
                command = json.loads(line)
                kind = command['kind']
                if kind == 'input':
                    os.write(master, base64.b64decode(command['data']))
                    emit({'kind': 'input_written', 'label': command.get('label'), 'base64': command['data']})
                elif kind == 'process_sample':
                    process_sample(command['request_id'])
                elif kind == 'owner_snapshot':
                    snapshot(command['request_id'])
                elif kind == 'release':
                    release.set()
                    emit({'kind': 'barrier_release', 'request_id': command['request_id']})
                elif kind == 'resize':
                    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', command['rows'], command['columns'], 0, 0))
                    os.killpg(child.pid, signal.SIGWINCH)
                    emit({'kind': 'resize', 'columns': command['columns'], 'rows': command['rows']})
                elif kind == 'stop':
                    forced = True
                else:
                    raise ValueError('Unadmitted control ' + kind)
            if forced:
                break
    else:
        if child.poll() is None:
            emit({'kind': 'runner_timeout'})
            forced = True
finally:
    release.set()
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
    while select.select([master], [], [], 0)[0]:
        try:
            data = os.read(master, 65536)
        except OSError:
            break
        if not data:
            break
        emit({'kind': 'output', 'data': base64.b64encode(data).decode()})
    os.close(master)
    server.shutdown()
    emit({'kind': 'exit', 'code': child.returncode, 'termination': 'forced' if forced else 'natural',
          'provider_requests': len(requests), 'provider_completed': len(completed)})
