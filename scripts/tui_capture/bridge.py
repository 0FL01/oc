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
import shlex
import sqlite3
import time
import urllib.request

scanner_release = threading.Event()


def emit(value):
    with output_lock:
        print(json.dumps(value, ensure_ascii=False), flush=True)


output_lock = threading.Lock()
spec = json.loads(Path(sys.argv[1]).read_text())
live = {}
if spec.get('sessions_interaction'):
    # Secrets stay inside this bridge; neither config nor launch evidence contains them.
    for line in (Path(__file__).resolve().parents[2] / '.local/live.env').read_text().splitlines():
        tokens = shlex.split(line, comments=True)
        if tokens and '=' in tokens[-1]:
            key, value = tokens[-1].split('=', 1)
            live[key] = value
    if not all(live.get(k) for k in ('LUDKA2_API_URL', 'LUDKA2_API_KEY', 'OC_TEST_MODEL')):
        raise RuntimeError('Missing bounded live prerequisites')
live_requests = 0
prior_live_requests = 0
if spec.get('sessions_interaction'):
    for protocol in Path(sys.argv[1]).resolve().parents[2].glob('*/**/protocol.json'):
        try:
            prior_spec = json.loads((protocol.parent / 'bridge-spec.json').read_text())
            if prior_spec.get('sessions_campaign', 'legacy') != spec.get('sessions_campaign', 'legacy'):
                continue
            events = json.loads(protocol.read_text())
            prior_live_requests += sum(e.get('kind') == 'provider' and e.get('valid') is True and
                                       'live_request_index' in e for e in events)
        except (OSError, ValueError):
            pass
root = Path(spec['isolated_root'])
fixture = Path(spec['fixture'])
prompt = (fixture / 'input.txt').read_text().strip()
second_prompt = ('Second same-session model check?' if spec.get('models_interaction')
                 else 'Second same-session spacing check?')
profile_id = 'Reader' if spec.get('agent_profile') else None
profile_prompt = 'You are the isolated T44 paired reader.' if profile_id else None
bounded = spec.get('bounded_mode')
if bounded:
    profile_id = 'fixture-shell' if bounded == 'shell' else 'fixture-reader'
    profile_prompt = 'You are the isolated T44 bounded fixture profile.'
shell_commands = ["printf 'SHELL-SHORT\\n'", "printf 'SHELL-LINE-%02d\\n' " + ' '.join(str(i) for i in range(1, 41))]
bounded_requests = 0
answer = (fixture / 'transcript.md').read_text().strip()
if spec.get('sample') == 'short':
    answer = 'GEOMETRY-SHORT: one short answer.'
elif spec.get('sample') == 'reasoning':
    answer = 'GEOMETRY-SHORT: public reasoning completed.'
elif spec.get('sample') == 'reasoning-steps':
    answer = 'GEOMETRY-SHORT: two public reasoning steps completed.'
elif spec.get('sample') == 'tools':
    answer = 'GEOMETRY-SHORT: tool read completed.'
elif spec.get('sample') == 'rows':
    answer = '```text\n' + '\n'.join(f'ROW-{i:03}' for i in range(90)) + '\n```'
elif spec.get('sample') == 'rows-reflow':
    answer = '```text\n' + '\n'.join(
        f'ROW-{i:03}' + (' ' + 'x' * 90 if i <= 40 else '') for i in range(90)) + '\n```'
catalog = json.loads((fixture / 'model-catalog.json').read_text())
selected_model = 'fixture-scroll-11' if spec.get('models_interaction') else 'fixture-model-1'
if spec.get('models_interaction'):
    # Identical test-only configured catalog on both sides; the normal eight
    # fixture entries and every other mode retain their original input.
    template = catalog['models'][0]
    catalog['models'].extend({**template, 'id': f'fixture-scroll-{i:02}',
                              'name': f'ZZ Scroll {i:02}'} for i in range(12))
title = json.loads((fixture / 'scenarios.json').read_text())['base']['title']
transcript_round = 0
title_round = 0
round_lock = threading.Lock()


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        global transcript_round, title_round, bounded_requests, live_requests
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        if spec.get('sessions_interaction'):
            system = str(body.get('instructions', '')) + json.dumps([x for x in body.get('input', []) if x.get('role') in ('system', 'developer')])
            is_title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
            with round_lock:
                live_requests += 1
                index = live_requests
            emit({'kind':'provider', 'operation':'title' if is_title else 'transcript',
                  'model':body.get('model'), 'valid':self.path == '/v1/responses' and index <= 8 and prior_live_requests + index <= 24,
                  'request_sha256':hashlib.sha256(json.dumps(body,sort_keys=True).encode()).hexdigest(),
                  'live_request_index':index})
            if index > 8 or prior_live_requests + index > 24 or self.path != '/v1/responses':
                self.send_error(400, 'bounded live limit'); return
            body['model'] = live['OC_TEST_MODEL']
            body['max_output_tokens'] = 2048 if is_title else 1024
            body['reasoning'] = {'effort':'low'}
            url = live['LUDKA2_API_URL'].rstrip('/')
            if not url.endswith('/responses'): url += '/responses'
            request = urllib.request.Request(url, data=json.dumps(body).encode(), headers={
                'Authorization':'Bearer ' + live['LUDKA2_API_KEY'], 'Content-Type':'application/json'})
            try:
                with urllib.request.urlopen(request, timeout=90) as response:
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
                    title_text = ''
                    completed = False
                    for line in response:
                        self.wfile.write(line); self.wfile.flush()
                        if line.startswith(b'data: '):
                            try: event = json.loads(line[6:])
                            except ValueError: continue
                            if is_title and event.get('type') == 'response.output_text.delta':
                                title_text += event.get('delta', '')
                            completed |= event.get('type') == 'response.completed'
                    emit({'kind':'provider_completed' if completed else 'provider_incomplete',
                          'operation':'title' if is_title else 'transcript',
                          'live_request_index':index, **({'generated_title':title_text} if is_title else {})})
            except Exception as error:
                # Do not emit URLs, headers, bodies or arbitrary remote exceptions.
                emit({'kind':'provider_error', 'error_type':type(error).__name__, 'live_request_index':index,
                      **({'http_status':error.code} if isinstance(error, urllib.error.HTTPError) else {})})
            return
        serialized = json.dumps(body, ensure_ascii=False)
        valid = self.path == '/v1/responses' and body.get('stream') is True
        # Title requests are real upstream auxiliary operations, not another transcript.
        system = str(body.get('instructions', '')) + json.dumps([x for x in body.get('input', []) if x.get('role') in ('system', 'developer')], ensure_ascii=False)
        is_title = ('You are a title generator. You output ONLY a thread title.' in system or
                     (not body.get('tools') and body.get('max_output_tokens') == 256 and
                       'title' in system.lower()))
        valid = valid and body.get('model') == (selected_model if spec.get('models_interaction') and
                                                second_prompt in serialized and not is_title
                                                else 'fixture-model-1')
        with round_lock:
            tool_results = [x for x in body.get('input', []) if x.get('type') == 'function_call_output']
            if (spec.get('two_turn') or spec.get('models_interaction')) and second_prompt in serialized and not is_title:
                turn_number = 1
                round_number = int(any(x.get('call_id') == 'call_fixture_read_2' for x in tool_results))
            else:
                round_number = 1 if tool_results else 0
                turn_number = transcript_round - 1 if tool_results else transcript_round
            if is_title:
                title_round += 1
                turn_number = title_round - 1
            elif not tool_results:
                transcript_round += 1
        second = (spec.get('tab_restart') or spec.get('two_turn') or spec.get('models_interaction')) and turn_number > 0
        text = (('Regenerated fixture title' if spec.get('regenerate_title') and title_round == 2 else
                 'Second fixture session' if second else title) if is_title else
                  ('GEOMETRY-TURN-TWO: tool read completed.' if (spec.get('two_turn') or spec.get('models_interaction')) and second else
                  'GEOMETRY-SECOND: tool read completed.' if second else answer))
        expected_prompt = second_prompt if (spec.get('two_turn') or spec.get('models_interaction')) and second and not is_title else prompt
        valid = valid and (is_title or expected_prompt in serialized)
        profile_prompt_present = profile_prompt is not None and profile_prompt in system
        if profile_id and not is_title:
            valid = valid and profile_prompt_present
        if spec.get('sample') == 'tools' and not is_title:
            valid = valid and 'read' in [x.get('name') for x in body.get('tools', [])]
            if round_number:
                calls = [x for x in tool_results if x.get('call_id') == ('call_fixture_read_2' if (spec.get('two_turn') or spec.get('models_interaction')) and second else 'call_fixture_read')]
                valid = valid and len(calls) == 1 and 'fixture-content' in str(calls[0].get('output', ''))
        if spec.get('sample') == 'reasoning-steps' and not is_title:
            valid = valid and not tool_results and turn_number == 0 and round_number == 0
        record = {'kind': 'provider', 'path': self.path, 'model': body.get('model'),
                   'stream': body.get('stream'), 'prompt_present': expected_prompt in serialized,
                   'profile_prompt_present': profile_prompt_present,
                   'operation': 'title' if is_title else 'transcript', 'valid': valid,
                   'turn_number': turn_number, 'round_number': round_number,
                  'request_sha256': hashlib.sha256(serialized.encode()).hexdigest(),
                   'registered_tools': [x.get('name') for x in body.get('tools', [])],
                   'tool_result_count': len(tool_results),
                   'fixture_content_returned': any('fixture-content' in str(x.get('output', '')) for x in tool_results),
                   'request_top_level_keys': sorted(body)}
        if bounded:
            tools = body.get('tools', [])
            record['tool_schemas'] = {x.get('name'): x.get('parameters') for x in tools}
            record['reasoning_effort'] = body.get('reasoning', {}).get('effort')
            if is_title:
                valid = valid and title_round == 1
            else:
                index = bounded_requests
                bounded_requests += 1
                if bounded == 'variants':
                    expected = [None, 'high', 'low', None]
                    valid = valid and index < 4 and not tool_results and record['reasoning_effort'] == expected[index]
                    text = f'BOUNDED-VARIANT-{index}: completed.'
                else:
                    shell_name = 'bash' if 'bash' in record['registered_tools'] else 'shell'
                    valid = valid and index < 3 and shell_name in record['registered_tools']
                    schema = record['tool_schemas'].get(shell_name) or {}
                    valid = valid and ('command' in schema.get('properties', {}) or 'argv' in schema.get('properties', {}))
                    expected_count = index
                    valid = valid and len(tool_results) == expected_count
                    outputs = {x.get('call_id'): str(x.get('output', '')) for x in tool_results}
                    for i in range(index):
                        output = outputs.get(f'call_fixture_shell_{i}', '')
                        required = ['SHELL-SHORT'] if i == 0 else [f'SHELL-LINE-{n:02}' for n in range(1, 41)]
                        valid = valid and all(line in output for line in required) and 'permission denied' not in output.lower()
                    record['fixture_outputs_verified'] = valid
                    record['fixture_outputs'] = outputs
                    text = 'BOUNDED-SHELL: both real printf tools completed.'
                record['bounded_request_index'] = index
            record['valid'] = valid
        emit(record)
        if not valid:
            self.send_error(400, 'fixture contract rejected')
            return
        if spec.get('sample') == 'tools' and not is_title and round_number == 0:
            item = {'id': 'fc_fixture_2' if (spec.get('two_turn') or spec.get('models_interaction')) and second else 'fc_fixture',
                    'type': 'function_call', 'status': 'completed',
                    'call_id': 'call_fixture_read_2' if (spec.get('two_turn') or spec.get('models_interaction')) and second else 'call_fixture_read', 'name': 'read',
                    'arguments': json.dumps({'path': 'fixture-note.txt'})}
            text = ''
        else:
            item = {'id': 'msg_fixture_2' if (spec.get('two_turn') or spec.get('models_interaction')) and second else 'msg_fixture',
                    'type': 'message', 'role': 'assistant', 'status': 'completed',
                  'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
        if bounded == 'shell' and not is_title and index < 2:
            arguments = {'command': shell_commands[index], 'description': 'Fixture short output' if index == 0 else 'Fixture long output'}
            if 'argv' in schema.get('properties', {}):
                arguments = {'argv': ['printf', 'SHELL-SHORT\\n'] if index == 0 else
                             ['printf', 'SHELL-LINE-%02d\\n', *[str(i) for i in range(1, 41)]]}
            record_call = {'kind': 'fixture_tool_call', 'name': shell_name, 'arguments': arguments,
                           'call_id': f'call_fixture_shell_{index}'}
            emit(record_call)
            item = {'id': f'fc_fixture_shell_{index}', 'type': 'function_call', 'status': 'completed',
                    'call_id': record_call['call_id'], 'name': shell_name, 'arguments': json.dumps(arguments)}
            text = ''
        response = {'id': 'resp_fixture_2' if (spec.get('two_turn') or spec.get('models_interaction')) and second else 'resp_fixture', 'object': 'response', 'created_at': 1700000000,
                    'model': body.get('model'), 'status': 'in_progress', 'output': [],
                    'error': None, 'incomplete_details': None}
        events = [
            {'type': 'response.created', 'response': response},
        ]
        reasoning_items = []
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
        if spec.get('sample') == 'reasoning-steps' and not is_title:
            for index, (reasoning_id, public) in enumerate([
                    ('rs_fixture_step_1', '**Inspecting**\n\nFirst public step marker.'),
                    ('rs_fixture_step_2', '**Verifying**\n\nSecond public step marker.')]):
                part = {'type': 'summary_text', 'text': public}
                reasoning = {'type': 'reasoning', 'id': reasoning_id, 'status': 'completed',
                             'summary': [part], 'encrypted_content': 'opaque-fixture-must-not-display'}
                reasoning_items.append(reasoning)
                events += [
                    {'type': 'response.output_item.added', 'output_index': index,
                     'item': {'type': 'reasoning', 'id': reasoning_id, 'status': 'in_progress', 'summary': []}},
                    {'type': 'response.reasoning_summary_part.added', 'item_id': reasoning_id,
                     'output_index': index, 'summary_index': 0, 'part': {'type': 'summary_text', 'text': ''}},
                    {'type': 'response.reasoning_summary_text.delta', 'item_id': reasoning_id,
                     'output_index': index, 'summary_index': 0, 'delta': public},
                ]
                events.append({'type': 'response.reasoning_summary_part.done', 'item_id': reasoning_id,
                               'output_index': index, 'summary_index': 0, 'part': part})
                events.append({'type': 'response.output_item.done', 'output_index': index, 'item': reasoning})
        message_index = len(reasoning_items)
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
            {'type': 'response.output_item.added', 'output_index': message_index,
             'item': {**item, 'status': 'in_progress', 'content': []}},
            {'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': message_index,
             'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
            {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': message_index,
             'content_index': 0, 'delta': text},
            {'type': 'response.output_text.done', 'item_id': item['id'], 'output_index': message_index,
             'content_index': 0, 'text': text},
            {'type': 'response.content_part.done', 'item_id': item['id'], 'output_index': message_index,
             'content_index': 0, 'part': item['content'][0]},
            {'type': 'response.output_item.done', 'output_index': message_index, 'item': item},
            {'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [*reasoning_items, item],
             'usage': {'input_tokens': 6000, 'output_tokens': 763, 'total_tokens': 6763,
                       'input_tokens_details': {'cached_tokens': 0},
                       'output_tokens_details': {'reasoning_tokens': 0}}}},
          ]
        payload = ''.join('event: ' + e['type'] + '\ndata: ' + json.dumps({**e, 'sequence_number': i}, ensure_ascii=False) + '\n\n'
                           for i, e in enumerate(events)).encode()
        scanner_hold = spec.get('scanner') and not is_title and round_number == 0 and turn_number == 0
        prefix = ('event: response.created\ndata: ' + json.dumps({**events[0], 'sequence_number': 0}) + '\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        try:
            if scanner_hold:
                self.wfile.write(prefix)
                self.wfile.flush()
                emit({'kind': 'scanner_held', 'timeout_seconds': 60})
                released = scanner_release.wait(60)
                emit({'kind': 'scanner_resumed', 'released': released})
                self.wfile.write(payload[len(prefix):])
            else:
                self.wfile.write(payload)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            emit({'kind': 'provider_disconnected', 'operation': record['operation']})
            return
        emit({'kind': 'provider_completed', 'operation': record['operation'],
              'response_text_sha256': hashlib.sha256(text.encode()).hexdigest()})


server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
home = root / spec['origin'] / 'home'
project = root / 'project'
if spec.get('sessions_interaction'):
    project = root / 'other-project'
for path in [home, project, *[home / x for x in ('config/opencode', 'cache', 'data', 'state')]]:
    path.mkdir(parents=True, exist_ok=True)
if spec.get('sample') == 'tools':
    (project / 'fixture-note.txt').write_text('fixture-content\n')
settings = {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'fixture-not-a-secret'}
models = {m['id']: {k: v for k, v in m.items() if k not in ('id', 'variants')} for m in catalog['models']}
if spec.get('sessions_interaction'):
    models = {'fixture-model-1': {**models['fixture-model-1'], 'name':live['OC_TEST_MODEL'], 'limit':{'context':32000,'output':1024}}}
if spec.get('variants') or bounded == 'variants':
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
        config['default_agent'] = 'build' if bounded else profile_id
        config['agents'] = {profile_id: {'mode': 'primary', 'system': profile_prompt,
                                          'color': '#5c9cf5'}}
        if bounded:
            config['agents'][profile_id]['permissions'] = [
                {'action': '*', 'resource': '*', 'effect': 'deny'},
                *[{'action': 'shell', 'resource': command, 'effect': 'allow'} for command in shell_commands]] if bounded == 'shell' else [
                    {'action': '*', 'resource': '*', 'effect': 'deny'},
                    {'action': 'read', 'resource': '*', 'effect': 'allow'}]
    cli_config = {'theme': {'name': 'opencode', 'mode': 'dark'}, 'animations': spec.get('animations', False),
                   'session': {'sidebar': spec.get('sidebar', 'auto'), 'tps': False},
                   'tabs': {'layout': spec.get('tabs', 'horizontal')},
                  'attention': {'notifications': False, 'sound': False},
                  'cursor': {'style': 'block', 'blinking': False}}
    argv = [spec['binary'], '--standalone']
else:
    config = {'model': 'fixture/fixture-model-1', 'provider': {'fixture': {
        'name': catalog['provider']['name'], 'npm': '@ai-sdk/openai', 'options': settings, 'models': models}},
        'permissions': {'read': 'allow'}}
    if spec.get('scanner'):
        config['animations'] = spec['animations']
    if profile_id:
        # Native inline definitions use singular `agent` and `prompt`.
        config['default_agent'] = 'build' if bounded else profile_id
        config['agent'] = {profile_id: {'mode': 'primary', 'prompt': profile_prompt}}
        if bounded:
            config['agent']['build'] = {'mode': 'primary', 'prompt': 'Initial isolated fixture profile.'}
        if bounded == 'shell':
            # Native bash is argv-only. Authorize its exact boundary-preserving
            # resource strings, not a general shell or printf wildcard.
            resources = shell_commands
            config['permissions'] = {'*': 'deny', 'bash': {'*': 'deny', **{command: 'allow' for command in resources}}}
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
       'version': version.stdout.decode().strip(), 'version_exit': version.returncode,
       **({'sessions_resume':spec.get('sessions_resume',False)} if spec.get('sessions_interaction') else {})})
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
        scanner_paused = False
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
                        if command['kind'] == 'sessions_snapshot' and spec.get('sessions_interaction'):
                            observations = []
                            for database in (home / 'data').rglob('*'):
                                if database.suffix not in ('.db', '.sqlite', '.sqlite3'): continue
                                try:
                                    with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
                                        connection.row_factory = sqlite3.Row
                                        tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
                                        for table in ('session', 'session_v2', 'sessions'):
                                            if table not in tables: continue
                                            columns = {r[1] for r in connection.execute(f'PRAGMA table_info({table})')}
                                            wanted = [c for c in ('id','title','parent_id','created_at','updated_at','time_created','time_updated','directory','location','project_id','subpath') if c in columns]
                                            rows = [dict(r) for r in connection.execute(f"SELECT {','.join(wanted)} FROM {table} ORDER BY id")]
                                            if table == 'sessions' and 'prefs' in tables:
                                                locations = dict(connection.execute("SELECT substr(key,22), value FROM prefs WHERE key LIKE 'tui.session_location.%'"))
                                                for row in rows: row['directory'] = locations.get(row['id'])
                                            observations.append({'database':str(database), 'table':table, 'columns':sorted(columns), 'rows':rows})
                                except sqlite3.Error as error:
                                    observations.append({'database':str(database),'error_type':type(error).__name__})
                            emit({'kind':'sessions_snapshot','request_id':command['request_id'],
                                  'observed_at_ms':int(time.time()*1000), 'observations':observations})
                        elif command['kind'] == 'input':
                            os.write(master, base64.b64decode(command['data']))
                        elif command['kind'] == 'resize':
                            fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', command['rows'], command['columns'], 0, 0))
                            os.killpg(child.pid, signal.SIGWINCH)
                            emit({'kind': 'resize', 'columns': command['columns'], 'rows': command['rows']})
                        elif command['kind'] == 'stop':
                            forced = True
                        elif command['kind'] == 'release_scanner' and spec.get('scanner'):
                            scanner_release.set()
                            emit({'kind': 'scanner_release_requested'})
                        elif command['kind'] in ('pause_scanner', 'resume_scanner') and spec.get('scanner'):
                            pause = command['kind'] == 'pause_scanner'
                            try:
                                if pause and child.poll() is None and not scanner_paused:
                                    os.killpg(child.pid, signal.SIGSTOP)
                                    scanner_paused = True
                                elif not pause and scanner_paused:
                                    os.killpg(child.pid, signal.SIGCONT)
                                    scanner_paused = False
                                if pause and scanner_paused:
                                    # Flush all PTY bytes written before SIGSTOP before ACK;
                                    # the frontend must apply them before its paused resample.
                                    while select.select([master], [], [], 0)[0]:
                                        try:
                                            drained = os.read(master, 65536)
                                        except OSError:
                                            break
                                        if not drained:
                                            break
                                        emit({'kind': 'output', 'generation': generation,
                                              'data': base64.b64encode(drained).decode()})
                                emit({'kind': 'scanner_pause_ack' if pause else 'scanner_resume_ack',
                                      'request_id': command['request_id'], 'paused': scanner_paused,
                                      'generation': generation})
                            except ProcessLookupError:
                                scanner_paused = False
                                emit({'kind': 'scanner_pause_ack' if pause else 'scanner_resume_ack',
                                      'request_id': command['request_id'], 'paused': False,
                                      'generation': generation, 'error': 'child exited'})
                    if forced:
                        break
        finally:
            # SIGSTOP also stops graceful termination; never wait/kill a paused
            # process group. This runs on stop, stdin EOF, exception and PTY EIO.
            if scanner_paused:
                try:
                    os.killpg(child.pid, signal.SIGCONT)
                except ProcessLookupError:
                    pass
                scanner_paused = False
                emit({'kind': 'scanner_cleanup_resumed', 'generation': generation})
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
        if forced or not (spec.get('tab_restart') or spec.get('sessions_interaction')):
            break
        # The same bridge/server/config/project and XDG roots survive the first exit.
        command = json.loads(sys.stdin.readline())
        if command.get('kind') != 'relaunch' or generation:
            break
        generation += 1
        if spec.get('sessions_interaction'):
            project = root / 'project'
            project.mkdir(parents=True, exist_ok=True)
        emit({'kind': 'relaunch', 'generation': generation, 'argv': argv, 'cwd': str(project)})
finally:
    scanner_release.set()
    server.shutdown()
    server.server_close()
