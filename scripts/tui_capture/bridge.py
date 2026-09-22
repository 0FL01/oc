#!/usr/bin/env python3
"""Test-only local Responses fixture and env-cleared real PTY bridge (JSONL IPC)."""
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
import struct
import subprocess
import sys
import termios
import threading


def emit(value):
    with output_lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)


output_lock = threading.Lock()
spec = json.loads(Path(sys.argv[1]).read_text())
root = Path(spec['isolated_root'])
fixture = Path(spec['fixture'])
prompt = (fixture / 'input.txt').read_text().strip()
answer = (fixture / 'transcript.md').read_text().strip()
catalog = json.loads((fixture / 'model-catalog.json').read_text())
title = json.loads((fixture / 'scenarios.json').read_text())['base']['title']


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        serialized = json.dumps(body, ensure_ascii=False)
        valid = self.path == '/v1/responses' and body.get('model') == 'fixture-model-1' and body.get('stream') is True
        # Title requests are real upstream auxiliary operations, not another transcript.
        system = str(body.get('instructions', '')) + json.dumps([x for x in body.get('input', []) if x.get('role') in ('system', 'developer')], ensure_ascii=False)
        is_title = 'You are a title generator. You output ONLY a thread title.' in system
        text = title if is_title else answer
        valid = valid and (is_title or prompt in serialized)
        record = {'kind': 'provider', 'path': self.path, 'model': body.get('model'),
                  'stream': body.get('stream'), 'prompt_present': prompt in serialized,
                  'operation': 'title' if is_title else 'transcript', 'valid': valid,
                  'request_sha256': hashlib.sha256(serialized.encode()).hexdigest(),
                  'registered_tools': [x.get('name') for x in body.get('tools', [])],
                  'request_top_level_keys': sorted(body)}
        emit(record)
        if not valid:
            self.send_error(400, 'fixture contract rejected')
            return
        item = {'id': 'msg_fixture', 'type': 'message', 'role': 'assistant', 'status': 'completed',
                'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
        response = {'id': 'resp_fixture', 'object': 'response', 'created_at': 1700000000,
                    'model': 'fixture-model-1', 'status': 'in_progress', 'output': [],
                    'error': None, 'incomplete_details': None}
        events = [
            {'type': 'response.created', 'response': response},
            {'type': 'response.output_item.added', 'output_index': 0,
             'item': {**item, 'status': 'in_progress', 'content': []}},
            {'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': 0,
             'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
            {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0,
             'content_index': 0, 'delta': text},
            {'type': 'response.output_text.done', 'item_id': item['id'], 'output_index': 0,
             'content_index': 0, 'text': text},
            {'type': 'response.content_part.done', 'item_id': item['id'], 'output_index': 0,
             'content_index': 0, 'part': item['content'][0]},
            {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
            {'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [item],
             'usage': {'input_tokens': 6000, 'output_tokens': 763, 'total_tokens': 6763,
                       'input_tokens_details': {'cached_tokens': 0},
                       'output_tokens_details': {'reasoning_tokens': 0}}}},
        ]
        payload = ''.join('event: ' + e['type'] + '\ndata: ' + json.dumps({**e, 'sequence_number': i}, ensure_ascii=False) + '\n\n'
                          for i, e in enumerate(events)).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)
        self.wfile.flush()
        emit({'kind': 'provider_completed', 'operation': record['operation'],
              'response_text_sha256': hashlib.sha256(text.encode()).hexdigest()})


server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
home = root / spec['origin'] / 'home'
project = root / 'project'
for path in [home, project, *[home / x for x in ('config/opencode', 'cache', 'data', 'state')]]:
    path.mkdir(parents=True, exist_ok=True)
settings = {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'fixture-not-a-secret'}
models = {m['id']: {k: v for k, v in m.items() if k not in ('id', 'variants')} for m in catalog['models']}
if spec['origin'] == 'upstream':
    config = {'model': 'fixture/fixture-model-1', 'share': 'disabled', 'update': 'disable',
              'plugins': ['-opencode.models.dev'],
              'providers': {'fixture': {'name': catalog['provider']['name'],
              'package': '@opencode/ai/providers/openai/responses', 'settings': settings, 'models': models}}}
    cli_config = {'theme': {'name': 'opencode', 'mode': 'dark'}, 'animations': False,
                  'session': {'sidebar': 'auto', 'tps': False}, 'debug': {'devtools': False},
                  'attention': {'notifications': False, 'sound': False},
                  'cursor': {'style': 'block', 'blinking': False}}
    (home / 'config/opencode/cli.json').write_text(json.dumps(cli_config))
    argv = [spec['binary'], '--standalone']
else:
    config = {'model': 'fixture/fixture-model-1', 'provider': {'fixture': {
        'name': catalog['provider']['name'], 'npm': '@ai-sdk/openai', 'options': settings, 'models': models}}}
    argv = [spec['binary'], 'tui']
(home / 'config/opencode/opencode.json').write_text(json.dumps(config))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_CACHE_HOME': str(home / 'cache'),
       'XDG_DATA_HOME': str(home / 'data'), 'XDG_STATE_HOME': str(home / 'state'),
       'PATH': '/usr/bin:/bin', 'SHELL': '/bin/sh', 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor',
       'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TZ': 'UTC', 'OC_TEST_ALLOW_LOOPBACK': '1',
       'OPENCODE_TEST_HOME': str(home), 'OPENCODE_DISABLE_AUTOUPDATE': '1',
       'OPENCODE_DISABLE_MODELS_FETCH': 'true', 'OPENCODE_DISABLE_FILEWATCHER': 'true',
       'OPENCODE_CONFIG_CONTENT': json.dumps(config), 'OPENCODE_CONFIG_PROJECT_DISABLE': 'true'}
version = subprocess.run([spec['binary'], '--version'], env=env, cwd=project, capture_output=True, timeout=20)
emit({'kind': 'launch', 'argv': argv, 'cwd': str(project), 'env': env,
      'version': version.stdout.decode().strip(), 'version_exit': version.returncode})
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))


def session():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, cwd=project, env=env, preexec_fn=session)
os.close(slave)
pending = b''
try:
    while child.poll() is None:
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
                break
            pending += data
            stop = False
            while b'\n' in pending:
                line, pending = pending.split(b'\n', 1)
                command = json.loads(line)
                if command['kind'] == 'input':
                    os.write(master, base64.b64decode(command['data']))
                elif command['kind'] == 'stop':
                    stop = True
            if stop:
                break
finally:
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()
    os.close(master)
    server.shutdown()
    emit({'kind': 'exit', 'code': child.returncode})
