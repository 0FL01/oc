#!/usr/bin/env python3
"""VIS38 reference-only: exact oracle text -> original public session import -> PTY.

No database writes, provider generation, plugin execution, or authoring environment.
The imported user message intentionally exercises U34; not a native compression.
"""
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
import time


def emit(value):
    with output_lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)


output_lock = threading.Lock()
spec = json.loads(Path(sys.argv[1]).read_text())
root = Path(spec['isolated_root'])
root.mkdir()  # newly owned direct child; refuse reuse
home, project = root / 'home', root / 'project'
for directory in (project, home / 'config/opencode', home / 'cache', home / 'data', home / 'state'):
    directory.mkdir(parents=True)
oracle = json.loads(Path(spec['goldens']).read_text())
case = next(c for c in oracle['notifications'] if c['id'] == spec['case'])


class RefuseProvider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        # A display-only imported session must never issue a generation request.
        emit({'kind': 'unexpected_provider_request', 'path': self.path})
        self.send_error(503, 'VIS38 display-only fixture rejects generation')

    def do_GET(self):
        emit({'kind': 'unexpected_provider_request', 'path': self.path})
        self.send_error(503, 'VIS38 display-only fixture rejects discovery')


server = ThreadingHTTPServer(('127.0.0.1', 0), RefuseProvider)
threading.Thread(target=server.serve_forever, daemon=True).start()
config = {'model': 'fixture/fixture-model-1', 'share': 'disabled', 'update': 'disable',
          'plugins': ['-opencode.models.dev'], 'snapshots': False,
          'providers': {'fixture': {'name': 'VIS38 Display Fixture',
              'package': '@opencode/ai/providers/openai/responses',
              'settings': {'baseURL': f'http://127.0.0.1:{server.server_port}/v1'},
              'models': {'fixture-model-1': {'name': 'VIS38 Display Model', 'limit': {'context': 32000, 'output': 2048}}}}}}
cli = {'theme': {'name': 'opencode', 'mode': 'dark'}, 'animations': False,
       'session': {'sidebar': 'hide', 'tps': False}, 'tabs': {'layout': 'horizontal'},
       'attention': {'notifications': False, 'sound': False}, 'cursor': {'style': 'block', 'blinking': False}}
for name, value in [('opencode.json', config), ('cli.json', cli)]:
    (home / 'config/opencode' / name).write_text(json.dumps(value))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
       'XDG_CACHE_HOME': str(home / 'cache'), 'XDG_DATA_HOME': str(home / 'data'),
       'XDG_STATE_HOME': str(home / 'state'), 'PATH': '/usr/bin:/bin', 'SHELL': '/bin/sh',
       'TERM': 'xterm-256color', 'COLORTERM': 'truecolor', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8',
       'TZ': 'UTC', 'OPENCODE_TEST_HOME': str(home), 'OPENCODE_DISABLE_AUTOUPDATE': '1',
       'OPENCODE_DISABLE_MODELS_FETCH': 'true', 'OPENCODE_DISABLE_FILEWATCHER': 'true',
       'OPENCODE_CONFIG_PROJECT_DISABLE': 'true', 'OPENCODE_CONFIG_CONTENT': json.dumps(config)}
stamp = 1700000000000
session_id = 'ses_vis38_' + spec['case'].replace('-', '_')
info = {'id': session_id, 'projectID': 'global', 'cost': 0,
        'tokens': {'input': 0, 'output': 0, 'reasoning': 0, 'cache': {'read': 0, 'write': 0}},
        'time': {'created': stamp, 'updated': stamp + 3},
        'title': 'VIS38 ' + spec['case'], 'location': {'directory': str(project)}}


def assistant(suffix, text, order):
    return {'id': 'msg_vis38_' + suffix, 'type': 'assistant', 'agent': 'build',
            'model': {'providerID': 'fixture', 'id': 'fixture-model-1'},
            'content': [{'type': 'text', 'text': text}], 'finish': 'stop',
            'time': {'created': stamp + order, 'completed': stamp + order}}


native_context = case.get('native_context')
if native_context:
    # Actual committed public history, schema-mapped through original public
    # import. This context is never an ingress to the native binary.
    messages = []
    for order, row in enumerate(native_context['messages'][:-1], 1):
        assert row['role'] in ('user', 'assistant')
        for previous in native_context.get('prior_notifications', []):
            if previous['before_message_id'] == row['id'] and previous['payload'] is not None:
                messages.append({'id': 'msg_vis38_prior_' + str(order), 'type': 'user',
                                 'text': previous['payload'], 'time': {'created': stamp + len(messages) + 1}})
        if row['role'] == 'assistant':
            mapped = assistant('context_' + str(order), row['text'], order)
        else:
            mapped = {'id': 'msg_vis38_context_' + str(order), 'type': 'user',
                      'text': row['text'], 'time': {'created': stamp + order}}
        mapped['time']['created'] = stamp + len(messages) + 1
        if mapped['type'] == 'assistant':
            mapped['time']['completed'] = mapped['time']['created']
        messages.append(mapped)
else:
    messages = [assistant('before', 'VIS38-BEFORE: agent step completed.', 1)]
if native_context and native_context.get('reference_delivery'):
    # On genuine restart/control reopen the existing card remains at the old
    # operation boundary. Do not append a fabricated fourth commit at the tail.
    expected = case['prompts'][0]['body']['parts'][0]['text'] if case['prompts'] else None
    assert native_context['prior_notifications'][-1]['payload'] == expected
elif case['prompts']:
    text = case['prompts'][0]['body']['parts'][0]['text']
    if text != case['payload']:
        raise RuntimeError('Oracle interception payload differs')
    messages.append({'id': 'msg_vis38_report', 'type': 'user', 'text': text,
                      'time': {'created': stamp + len(messages) + 1}})
messages.append(assistant('after', native_context['capture_after'] if native_context else 'VIS38-AFTER: continuation display.', len(messages) + 1))
info['time']['updated'] = stamp + len(messages)
payload = {'info': info, 'messages': messages}
source = home / 'session.json'
source.write_text(json.dumps(payload, ensure_ascii=False))
emit({'kind': 'fixture', 'case': case['id'], 'goldens_sha256': hashlib.sha256(Path(spec['goldens']).read_bytes()).hexdigest(),
      'notification_text_sha256': hashlib.sha256(case['payload'].encode()).hexdigest() if case['payload'] else None,
       'transfer': payload, 'native_context': native_context,
       'delivery_mapping': 'D05 intercepted ignored noReply text -> OC2 imported user text; display reference only'})


def command(argv):
    result = subprocess.run(argv, env=env, cwd=project, capture_output=True, timeout=45)
    emit({'kind': 'command', 'argv': argv, 'exit_code': result.returncode,
          'stdout': result.stdout.decode(), 'stderr': result.stderr.decode()})
    if result.returncode:
        raise RuntimeError('Original public session command failed')
    return result.stdout.decode()


try:
    command([spec['binary'], '--version'])
    imported = command([spec['binary'], 'session', 'import', '--standalone', str(source)])
    if 'Imported session: ' + session_id not in imported:
        raise RuntimeError('Import did not confirm session identity')
    exported = json.loads(command([spec['binary'], 'session', 'export', '--standalone', session_id]))
    if exported['messages'] != messages:
        raise RuntimeError('Public export differs from exact imported messages')
    emit({'kind': 'exact_import_export_verified', 'session_id': session_id, 'message_count': len(messages)})
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))

    def session():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    argv = [spec['binary'], '--standalone', '--session', session_id]
    child = subprocess.Popen(argv, cwd=project, env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=session)
    os.close(slave)
    emit({'kind': 'launch', 'argv': argv, 'cwd': str(project), 'config': config, 'cli': cli,
          'env_names': sorted(env), 'inherited_env': False})
    pending = b''
    forced = False
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
        if not forced and child.poll() is None:
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
finally:
    server.shutdown()
    server.server_close()
