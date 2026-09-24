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
profile_id = 'Reader' if spec.get('agent_profile') else None
profile_prompt = 'You are the isolated T44 paired reader.' if profile_id else None
answer = (fixture / 'transcript.md').read_text().strip()
if spec.get('sample') == 'short':
    answer = 'GEOMETRY-SHORT: one short answer.'
elif spec.get('sample') == 'reasoning':
    answer = 'GEOMETRY-SHORT: public reasoning completed.'
elif spec.get('sample') == 'tools':
    answer = 'GEOMETRY-SHORT: tool read completed.'
elif spec.get('sample') == 'rows':
    answer = '```text\n' + '\n'.join(f'ROW-{i:03}' for i in range(90)) + '\n```'
catalog = json.loads((fixture / 'model-catalog.json').read_text())
title = json.loads((fixture / 'scenarios.json').read_text())['base']['title']
transcript_round = 0
title_round = 0
round_lock = threading.Lock()


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        global transcript_round, title_round
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        serialized = json.dumps(body, ensure_ascii=False)
        valid = self.path == '/v1/responses' and body.get('model') == 'fixture-model-1' and body.get('stream') is True
        # Title requests are real upstream auxiliary operations, not another transcript.
        system = str(body.get('instructions', '')) + json.dumps([x for x in body.get('input', []) if x.get('role') in ('system', 'developer')], ensure_ascii=False)
        is_title = ('You are a title generator. You output ONLY a thread title.' in system or
                    (not body.get('tools') and body.get('max_output_tokens') == 256 and
                      'title' in system.lower()))
        with round_lock:
            tool_results = [x for x in body.get('input', []) if x.get('type') == 'function_call_output']
            round_number = 1 if tool_results else 0
            turn_number = transcript_round - 1 if tool_results else transcript_round
            if is_title:
                title_round += 1
                turn_number = title_round - 1
            elif not tool_results:
                transcript_round += 1
        second = spec.get('tab_restart') and turn_number > 0
        text = (('Second fixture session' if second else title) if is_title else
                ('GEOMETRY-SECOND: tool read completed.' if second else answer))
        valid = valid and (is_title or prompt in serialized)
        profile_prompt_present = profile_prompt is not None and profile_prompt in system
        if profile_id and not is_title:
            valid = valid and profile_prompt_present
        if spec.get('sample') == 'tools' and not is_title:
            valid = valid and 'read' in [x.get('name') for x in body.get('tools', [])]
            if round_number:
                calls = [x for x in tool_results if x.get('call_id') == 'call_fixture_read']
                valid = valid and len(calls) == 1 and 'fixture-content' in str(calls[0].get('output', ''))
        record = {'kind': 'provider', 'path': self.path, 'model': body.get('model'),
                   'stream': body.get('stream'), 'prompt_present': prompt in serialized,
                   'profile_prompt_present': profile_prompt_present,
                   'operation': 'title' if is_title else 'transcript', 'valid': valid,
                  'request_sha256': hashlib.sha256(serialized.encode()).hexdigest(),
                   'registered_tools': [x.get('name') for x in body.get('tools', [])],
                   'tool_result_count': len(tool_results),
                   'fixture_content_returned': any('fixture-content' in str(x.get('output', '')) for x in tool_results),
                  'request_top_level_keys': sorted(body)}
        emit(record)
        if not valid:
            self.send_error(400, 'fixture contract rejected')
            return
        if spec.get('sample') == 'tools' and not is_title and round_number == 0:
            item = {'id': 'fc_fixture', 'type': 'function_call', 'status': 'completed',
                    'call_id': 'call_fixture_read', 'name': 'read',
                    'arguments': json.dumps({'path': 'fixture-note.txt'})}
            text = ''
        else:
            item = {'id': 'msg_fixture', 'type': 'message', 'role': 'assistant', 'status': 'completed',
                 'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
        response = {'id': 'resp_fixture', 'object': 'response', 'created_at': 1700000000,
                    'model': 'fixture-model-1', 'status': 'in_progress', 'output': [],
                    'error': None, 'incomplete_details': None}
        events = [
            {'type': 'response.created', 'response': response},
        ]
        if spec.get('sample') == 'reasoning' and not is_title:
            public = '**Inspecting**\n\nPublic summary only.'
            events += [
                {'type': 'response.output_item.added', 'output_index': 0,
                 'item': {'type': 'reasoning', 'id': 'rs_fixture', 'status': 'in_progress', 'summary': []}},
                {'type': 'response.reasoning_summary_part.added', 'item_id': 'rs_fixture',
                 'output_index': 0, 'summary_index': 0, 'part': {'type': 'summary_text', 'text': ''}},
                {'type': 'response.reasoning_summary_text.delta', 'item_id': 'rs_fixture',
                 'output_index': 0, 'summary_index': 0, 'delta': public},
                {'type': 'response.output_item.done', 'output_index': 0,
                 'item': {'type': 'reasoning', 'id': 'rs_fixture', 'summary': [{'type': 'summary_text', 'text': public}],
                          'encrypted_content': 'opaque-fixture-must-not-display'}},
            ]
        if item['type'] == 'function_call':
            events += [
                {'type': 'response.output_item.added', 'output_index': 0,
                 'item': {**item, 'status': 'in_progress', 'arguments': ''}},
                {'type': 'response.function_call_arguments.delta', 'item_id': item['id'],
                 'output_index': 0, 'delta': item['arguments']},
                {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
                {'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [item]}},
            ]
        else:
          events += [
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
if spec.get('sample') == 'tools':
    (project / 'fixture-note.txt').write_text('fixture-content\n')
settings = {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'fixture-not-a-secret'}
models = {m['id']: {k: v for k, v in m.items() if k not in ('id', 'variants')} for m in catalog['models']}
if spec.get('variants'):
    variants = json.loads((fixture / 'variant-dialog.json').read_text())
    # Equivalent native input shapes; original v2 uses id/settings arrays while
    # native config retains the documented named object form.
    models[catalog['models'][0]['id']]['variants'] = (
        [{'id': name, 'settings': settings} for name, settings in variants.items()]
        if spec['origin'] == 'upstream' else variants)
if spec['origin'] == 'upstream':
    config = {'model': 'fixture/fixture-model-1', 'share': 'disabled', 'update': 'disable',
              'plugins': ['-opencode.models.dev'],
              'providers': {'fixture': {'name': catalog['provider']['name'],
               'package': '@opencode/ai/providers/openai/responses', 'settings': settings, 'models': models}}}
    if profile_id:
        # v2.0.12 schema/config.ts and schema/config/agent.ts: actual custom
        # primary profile, not a painted Build label. The color is the pinned
        # dark opencode categorical[0] hue.blue.200.
        config['default_agent'] = profile_id
        config['agents'] = {profile_id: {'mode': 'primary', 'system': profile_prompt,
                                         'color': '#5c9cf5'}}
    cli_config = {'theme': {'name': 'opencode', 'mode': 'dark'}, 'animations': False,
                   'session': {'sidebar': spec.get('sidebar', 'auto'), 'tps': False},
                   'tabs': {'layout': spec.get('tabs', 'horizontal')},
                  'attention': {'notifications': False, 'sound': False},
                  'cursor': {'style': 'block', 'blinking': False}}
    argv = [spec['binary'], '--standalone']
else:
    config = {'model': 'fixture/fixture-model-1', 'provider': {'fixture': {
        'name': catalog['provider']['name'], 'npm': '@ai-sdk/openai', 'options': settings, 'models': models}},
        'permissions': {'read': 'allow'}}
    if profile_id:
        # Native inline definitions use singular `agent` and `prompt`.
        config['default_agent'] = profile_id
        config['agent'] = {profile_id: {'mode': 'primary', 'prompt': profile_prompt}}
    argv = [spec['binary'], 'tui']
    cli_config = {
        'session': {'sidebar': spec.get('sidebar', 'auto')},
        'tabs': {'layout': spec.get('tabs', 'horizontal')}}
if spec.get('devtools') is not None:
    cli_config['debug'] = {'devtools': spec['devtools']}
(home / 'config/opencode/cli.json').write_text(json.dumps(cli_config))
(home / 'config/opencode/opencode.json').write_text(json.dumps(config))
if spec.get('startup_error'):
    (home / 'config/opencode/opencode.json').write_text('{"model":"DO-NOT-LEAK-KEY", INVALID}')
if spec.get('seed_root'):
    if spec['origin'] == 'oc':
        home = Path(spec['seed_root']) / 'home'
    project = Path(spec['seed_root']) / 'project'
    argv += ['--session', spec['session']]
    (home / 'config/opencode/cli.json').write_text(json.dumps(cli_config))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_CACHE_HOME': str(home / 'cache'),
       'XDG_DATA_HOME': str(home / 'data'), 'XDG_STATE_HOME': str(home / 'state'),
       'PATH': '/usr/bin:/bin', 'SHELL': '/bin/sh', 'TERM': 'xterm-256color', 'COLORTERM': 'truecolor',
       'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TZ': 'UTC', 'OC_TEST_ALLOW_LOOPBACK': '1',
       'OPENCODE_TEST_HOME': str(home), 'OPENCODE_DISABLE_AUTOUPDATE': '1',
       'OPENCODE_DISABLE_MODELS_FETCH': 'true', 'OPENCODE_DISABLE_FILEWATCHER': 'true',
       'OPENCODE_CONFIG_CONTENT': json.dumps(config), 'OPENCODE_CONFIG_PROJECT_DISABLE': 'true'}
version = subprocess.run([spec['binary'], '--version'], env=env, cwd=project, capture_output=True, timeout=20)
if spec.get('seed_root') and spec['origin'] == 'upstream':
    # Supported import CLI with real parent metadata; no source-rendered frame,
    # renderer replacement or direct upstream database mutation.
    for session_id, parent in [('ses_v03_parent', None), ('ses_v03_child', 'ses_v03_parent')]:
        info = {'id': session_id, 'projectID': 'global', 'cost': 0,
                'tokens': {'input': 0, 'output': 0, 'reasoning': 0, 'cache': {'read': 0, 'write': 0}},
                'time': {'created': 1700000000000, 'updated': 1700000000000},
                'title': 'Geometry child' if parent else 'Geometry parent',
                'location': {'directory': str(project)}}
        if parent: info['parentID'] = parent
        payload = {'info': info, 'messages': []}
        source = home / (session_id + '.json')
        source.write_text(json.dumps(payload))
        imported = subprocess.run([spec['binary'], 'session', 'import', '--standalone', str(source)],
                                  env=env, cwd=project, capture_output=True, timeout=30)
        emit({'kind': 'session_import', 'session': session_id, 'payload': payload,
              'exit_code': imported.returncode, 'stdout': imported.stdout.decode(), 'stderr': imported.stderr.decode()})
        if imported.returncode: raise RuntimeError('supported session import failed')
emit({'kind': 'launch', 'argv': argv, 'cwd': str(project), 'env': env,
      'version': version.stdout.decode().strip(), 'version_exit': version.returncode})
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))


def session():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


pending = b''
generation = 0
try:
    while True:
        if generation:
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', spec['rows'], spec['columns'], 0, 0))
        child = subprocess.Popen(argv, stdin=slave, stdout=slave,
                                 stderr=slave, cwd=project, env=env, preexec_fn=session)
        os.close(slave)
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
                        emit({'kind': 'output', 'generation': generation, 'data': base64.b64encode(data).decode()})
                if sys.stdin in ready:
                    data = os.read(sys.stdin.fileno(), 65536)
                    if not data:
                        forced = True
                        break
                    pending += data
                    while b'\n' in pending:
                        line, pending = pending.split(b'\n', 1)
                        command = json.loads(line)
                        if command['kind'] == 'input':
                            os.write(master, base64.b64decode(command['data']))
                        elif command['kind'] == 'resize':
                            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', command['rows'], command['columns'], 0, 0))
                            os.killpg(child.pid, signal.SIGWINCH)
                            emit({'kind': 'resize', 'columns': command['columns'], 'rows': command['rows']})
                        elif command['kind'] == 'stop':
                            forced = True
                    if forced:
                        break
        finally:
            # The PTY master can report EIO just before waitpid observes a
            # normal app.exit. Give that exit a bounded chance to complete;
            # otherwise a graceful quit is misclassified as forced teardown.
            if not forced and child.poll() is None:
                try:
                    child.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
            if child.poll() is None:
                forced = True
                os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                forced = True
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            # Drain the exited PTY before telling the frontend the generation ended.
            while select.select([master], [], [], 0)[0]:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    break
                if not data:
                    break
                emit({'kind': 'output', 'generation': generation, 'data': base64.b64encode(data).decode()})
            os.close(master)
            emit({'kind': 'exit', 'generation': generation, 'code': child.returncode,
                  'termination': 'forced_stop' if forced else 'natural'})
        if forced or not spec.get('tab_restart'):
            break
        # The same bridge/server/config/project and XDG roots survive the first exit.
        command = json.loads(sys.stdin.readline())
        if command.get('kind') != 'relaunch' or generation:
            break
        generation += 1
        emit({'kind': 'relaunch', 'generation': generation, 'argv': argv, 'cwd': str(project)})
finally:
    server.shutdown()
