#!/usr/bin/env python3
"""VIS09/VIS29 real binaries; native public actions, reference-only public import.

Only fixture HOME/config is written. Native history/prefs are observed read-only.
The original rejects every generation request. A native wire probe is explicit.
"""
import base64
from decimal import Decimal
import fcntl
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
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
import threading
import time

spec = json.loads(Path(sys.argv[1]).read_text())
root = Path(spec['root'])
assert str(root).startswith('/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/variants-')
root.mkdir()
home, project = root / 'home', Path(spec['project'])
project.mkdir(parents=True, exist_ok=True)
for directory in (home / 'config/opencode', home / 'cache', home / 'data', home / 'state'):
    directory.mkdir(parents=True)
assert hashlib.sha256(Path(spec['binary']).read_bytes()).hexdigest() == spec['binary_sha256']
fixture = json.loads(Path(spec['fixture']).read_text())
output_lock = threading.Lock()
requests = []
wire_allowed = False


def emit(value):
    with output_lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        emit({'kind': 'unexpected_provider_request', 'path': self.path})
        self.send_error(503, 'Display fixture forbids discovery')

    def do_POST(self):
        length = int(self.headers.get('Content-Length', '0'))
        assert 0 <= length <= 1048576
        body = json.loads(self.rfile.read(length))
        requests.append(body)
        valid = (spec['origin'] == 'oc' and wire_allowed and len(requests) == 1
                 and self.path == '/v1/responses' and body.get('stream') is True
                 and body.get('model') == fixture['chosen_model']
                 and body.get('reasoning', {}).get('effort') == 'low')
        emit({'kind': 'provider', 'path': self.path, 'request': body, 'valid': valid})
        if not valid:
            self.send_error(503, 'Display fixture rejects generation')
            return
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Connection', 'close')
        self.end_headers()
        response = {'id': 'resp_vis09_wire', 'object': 'response', 'model': body['model'],
                    'status': 'in_progress', 'output': [], 'error': None, 'incomplete_details': None}
        item = {'id': 'msg_vis09_wire', 'type': 'message', 'role': 'assistant', 'status': 'in_progress', 'content': []}

        def send(event):
            self.wfile.write(('event: ' + event['type'] + '\ndata: ' + json.dumps(event) + '\n\n').encode())
            self.wfile.flush()

        send({'type': 'response.created', 'response': response})
        send({'type': 'response.output_item.added', 'output_index': 0, 'item': item})
        send({'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': 0, 'content_index': 0,
              'part': {'type': 'output_text', 'text': '', 'annotations': []}})
        text = 'VIS09-WIRE: exact selected model and alias effort accepted.'
        send({'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'delta': text})
        item.update(status='completed', content=[{'type': 'output_text', 'text': text, 'annotations': []}])
        send({'type': 'response.output_item.done', 'output_index': 0, 'item': item})
        send({'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [item],
              'usage': {'input_tokens': 100, 'output_tokens': 12, 'total_tokens': 112}}})
        emit({'kind': 'provider_completed', 'model': body['model']})
        self.close_connection = True


server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
settings = {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'offline-fixture-not-a-secret'}
models = fixture['models']
cli = fixture['cli']
if spec['origin'] == 'upstream':
    models = {identity: {**entry, 'variants': [{'id': name, 'settings': value} for name, value in entry['variants'].items()]}
              for identity, entry in models.items()}
    config = {'model': 'fixture/' + fixture['initial_model'], 'plugins': ['-opencode.models.dev'],
              'share': 'disabled', 'update': 'disable', 'snapshots': False, 'compaction': {'auto': False},
              'providers': {'fixture': {'name': fixture['provider_name'],
                  'package': '@opencode/ai/providers/openai/responses', 'settings': settings, 'models': models}},
              'permissions': [{'action': '*', 'resource': '*', 'effect': 'deny'}]}
else:
    config = {'model': 'fixture/' + fixture['initial_model'], 'animations': False, 'snapshots': False,
              'compaction': {'auto': False}, 'permissions': {'*': 'deny'},
              'provider': {'fixture': {'name': fixture['provider_name'], 'npm': '@ai-sdk/openai',
                  'options': settings, 'models': models}}}
for name, data in [('opencode.json', config), ('cli.json', cli)]:
    (home / 'config/opencode' / name).write_text(json.dumps(data))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_CACHE_HOME': str(home / 'cache'),
       'XDG_DATA_HOME': str(home / 'data'), 'XDG_STATE_HOME': str(home / 'state'), 'PATH': '/usr/bin:/bin',
       'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor',
       'SHELL': '/bin/sh', 'TZ': 'UTC', 'OC_TEST_ALLOW_LOOPBACK': '1', 'OPENCODE_TEST_HOME': str(home),
       'OPENCODE_DISABLE_AUTOUPDATE': '1', 'OPENCODE_DISABLE_MODELS_FETCH': 'true',
       'OPENCODE_DISABLE_FILEWATCHER': 'true', 'OPENCODE_CONFIG_PROJECT_DISABLE': 'true',
       'OPENCODE_CONFIG_CONTENT': json.dumps(config)}
session_id = 'vis09-presentation'


def command(argv):
    result = subprocess.run(argv, cwd=project, env=env, capture_output=True, timeout=45)
    emit({'kind': 'command', 'argv': argv, 'exit_code': result.returncode,
          'stdout': result.stdout.decode(), 'stderr': result.stderr.decode()})
    assert result.returncode == 0, result.stderr.decode()
    return result.stdout.decode()


if spec['origin'] == 'upstream':
    source = json.loads(Path(spec['native_transfer']).read_text())
    assert source['native_history_ingress'] is False and source['native_history'] == []
    assert source['fixture_sha256'] == hashlib.sha256(Path(spec['fixture']).read_bytes()).hexdigest()
    payload = source['transfer']
    session_id = payload['info']['id']
    transfer_file = home / 'session.json'
    transfer_file.write_text(json.dumps(payload))
    command([spec['binary'], 'session', 'import', '--standalone', str(transfer_file)])
    exported = json.loads(command([spec['binary'], 'session', 'export', '--standalone', session_id]))
    assert exported['messages'] == payload['messages']
    for field in ('title', 'agent', 'model', 'tokens', 'cost', 'location'):
        assert exported['info'][field] == payload['info'][field], field
    emit({'kind': 'exact_import_export_verified', 'payload': payload, 'exported': exported})
    argv = [spec['binary'], '--standalone', '--session', session_id]
else:
    argv = [spec['binary'], 'tui', '--session', session_id]
command([spec['binary'], '--version'])
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))
terminal_before = termios.tcgetattr(slave)


def session():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen(argv, cwd=project, env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=session)
emit({'kind': 'launch', 'argv': argv, 'cwd': str(project), 'config': config, 'cli': cli,
      'binary_sha256': spec['binary_sha256'], 'env_names': sorted(env), 'inherited_environment': False,
      'history_ingress': spec['origin'] == 'upstream', 'session': session_id})


def snapshot(request_id):
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db', '.sqlite', '.sqlite3'):
            continue
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            for table in ('session_v2', 'session', 'sessions', 'session_message', 'messages', 'conversation_messages', 'turns', 'prefs', 'tool_ops', 'turn_acceptances'):
                if table in tables:
                    observations.append({'table': table, 'rows': [dict(r) for r in connection.execute(f'SELECT * FROM {table} LIMIT 100')]})
    client = []
    for file in home.rglob('*.json'):
        if file.stat().st_size < 262144 and ('tui' in str(file) or file.name in ('kv.json', 'model.json')):
            client.append({'path': str(file.relative_to(home)), 'data': json.loads(file.read_text())})
    value = {'kind': 'owner_snapshot', 'request_id': request_id, 'observations': observations,
             'client_persistence': client, 'requests': len(requests), 'readonly_sqlite': True,
             'project_files': [str(file.relative_to(project)) for file in project.rglob('*') if file.is_file()]}
    if request_id == 'native-transfer':
        assert spec['origin'] == 'oc'
        named = next(row for o in observations if o['table'] == 'sessions' for row in o['rows'] if row['id'] == session_id)
        history = [row for o in observations if o['table'] == 'conversation_messages' for row in o['rows']]
        assert named['title'] == fixture['title'] and history == [] and requests == []
        # Native session timestamps are stored seconds, no fabricated message time.
        transfer = {'info': {'id': 'ses_vis09_presentation', 'projectID': 'global', 'agent': 'build',
                    'model': {'providerID': 'fixture', 'id': fixture['initial_model'], 'variant': 'default'},
                    'title': named['title'], 'location': {'directory': str(project)},
                    'time': {'created': int(Decimal(named['created_at']) * 1000), 'updated': int(Decimal(named['updated_at']) * 1000)},
                    'cost': 0, 'tokens': {'input': 0, 'output': 0, 'reasoning': 0, 'cache': {'read': 0, 'write': 0}}}, 'messages': []}
        proof = {'native_history_ingress': False, 'native_history': history, 'native_session_row': named,
                 'fixture_sha256': hashlib.sha256(Path(spec['fixture']).read_bytes()).hexdigest(), 'transfer': transfer,
                 'mapping': 'Real public rename of empty owner session; zero usage/cost scaffolding; no message timestamps or history injection.'}
        Path(spec['native_transfer']).write_text(json.dumps(proof, indent=2) + '\n')
        value['transfer'] = proof
    emit(value)


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
                elif value['kind'] == 'owner_snapshot':
                    snapshot(value['request_id'])
                elif value['kind'] == 'allow_wire':
                    assert spec['origin'] == 'oc' and not wire_allowed
                    wire_allowed = True
                    emit({'kind': 'wire_admitted'})
                elif value['kind'] == 'stop':
                    forced = True
                else:
                    raise ValueError('Unknown control')
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
    emit({'kind': 'exit', 'code': child.returncode, 'termination': 'forced_stop' if forced else 'natural',
          'terminal_restored': termios.tcgetattr(slave) == terminal_before, 'provider_requests': len(requests)})
    os.close(master)
    os.close(slave)
    server.shutdown()
    server.server_close()
