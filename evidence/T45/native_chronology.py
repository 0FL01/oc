#!/usr/bin/env python3
"""PRM01 consumer: real normal ELF, isolated typed history, three local wires.

Seed only new operator messages/selection facts between clean process runs.
GO03 owns selection production/lowering; this qualifies root/child restoration
and dispatch, not a second selection or protocol implementation.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import threading


UPDATE = 'CHRONO_UPDATE </system-update> & <b>'
ESCAPED = '<system-update>\nCHRONO_UPDATE &lt;/system-update&gt; &amp; &lt;b&gt;\n</system-update>'


def text(item):
    value = item.get('content') or ''
    return value if isinstance(value, str) else ''.join(part.get('text', '') for part in value)


def response(protocol, identity, call=None):
    if protocol == 'responses':
        item = ({'type': 'function_call', 'id': identity, 'call_id': identity,
                 'name': call[0], 'arguments': json.dumps(call[1]), 'status': 'completed'}
                if call else {'type': 'message', 'id': identity, 'role': 'assistant',
                              'content': [{'type': 'output_text', 'text': identity}]})
        return [{'type': 'response.completed', 'response': {'status': 'completed', 'output': [item]}}]
    if protocol == 'chat':
        delta = ({'tool_calls': [{'index': 0, 'id': identity, 'type': 'function',
                                 'function': {'name': call[0], 'arguments': json.dumps(call[1])}}]}
                 if call else {'content': identity})
        return [{'choices': [{'index': 0, 'delta': delta, 'finish_reason': None}]},
                {'choices': [{'index': 0, 'delta': {}, 'finish_reason': 'tool_calls' if call else 'stop'}]}]
    block = ({'type': 'tool_use', 'id': identity, 'name': call[0], 'input': {}}
             if call else {'type': 'text', 'text': ''})
    delta = ({'type': 'input_json_delta', 'partial_json': json.dumps(call[1])}
             if call else {'type': 'text_delta', 'text': identity})
    return [{'type': 'message_start', 'message': {'id': identity, 'role': 'assistant', 'usage': {}}},
            {'type': 'content_block_start', 'index': 0, 'content_block': block},
            {'type': 'content_block_delta', 'index': 0, 'delta': delta},
            {'type': 'content_block_stop', 'index': 0},
            {'type': 'message_delta', 'delta': {'stop_reason': 'tool_use' if call else 'end_turn'}},
            {'type': 'message_stop'}]


def qualify(binary, protocol, supported, mismatch=False):
    with tempfile.TemporaryDirectory(prefix='t45-chronology-') as temporary:
        root = Path(temporary)
        home, project, data = root / 'home', root / 'project', root / 'data'
        config = home / 'config/opencode'
        config.mkdir(parents=True)
        project.mkdir()
        (project / 'seed.txt').write_text('SETTLED_CHILD_READ\n')
        requests, auxiliary, errors, rounds = [], [], [], {}
        stage, child = 0, None
        environment = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
                       'PATH': '/usr/bin:/bin', 'OC_TEST_ALLOW_LOOPBACK': '1'}

        def check(request, lane):
            items = request['input' if protocol == 'responses' else 'messages']
            assert request.get('max_output_tokens', request.get('max_completion_tokens', request.get('max_tokens'))) <= 2048
            assert 'effort_update' not in json.dumps(request), 'durable DTO leaked into wire'
            if stage == 0:
                return
            before = f'CHRONO_BEFORE_{lane.upper()}_{stage}'
            indexes = [i for i, item in enumerate(items) if before in text(item)]
            assert len(indexes) == 1, 'operator context absent/duplicated'
            updates = [(i, item) for i, item in enumerate(items) if UPDATE in text(item) or ESCAPED in text(item)]
            assert len(updates) == stage, 'chronological system update absent/duplicated'
            index, update = updates[-1]
            assert index >= indexes[0], 'update moved before its operator context'
            if protocol == 'responses' or (protocol == 'messages' and supported):
                assert update['role'] == ('developer' if protocol == 'responses' else 'system')
                assert text(update) == UPDATE
                assert index > indexes[0]
            else:
                assert update['role'] == 'user' and ESCAPED in text(update)
                assert text(update).index(ESCAPED) >= text(update).find(before), 'fallback moved within merged user message'
            top = (request.get('reasoning', {}).get('effort') if protocol == 'responses' else
                   request.get('reasoning_effort') if protocol == 'chat' else request.get('output_config', {}).get('effort'))
            current = 'high' if mismatch else 'low' if stage == 1 else None
            kept = supported and protocol != 'chat' and not mismatch
            assert top == (None if kept else current), ('captured effort/baseline mismatch', lane, stage, top, current, kept)
            lowered = [(i, item['reasoning']['effort']) for i, item in enumerate(items)
                       if item.get('type') == 'configuration_update'] if protocol == 'responses' else [
                (i, item['output_config']['effort']) for i, item in enumerate(items) if item.get('output_config')]
            assert [value for _, value in lowered] == (['low'] + (['medium'] if stage == 2 else []) if kept else [])
            if kept:
                assert lowered[-1][0] > index, 'effort update moved before system update'
            if protocol == 'messages':
                assert ('mid-conversation-output-config-2026-07-01' in request['_beta']) == kept
            initial = json.dumps(request.get('system', [])) if protocol == 'messages' else json.dumps(items[:indexes[0]])
            assert ('ROOT_PROFILE' if lane == 'root' else 'CHILD_PROFILE') in initial

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def send(self, events):
                self.send_response(200)
                self.send_header('Content-Type', 'text/event-stream')
                self.end_headers()
                for event in events:
                    prefix = f"event: {event['type']}\n" if protocol == 'messages' else ''
                    self.wfile.write((prefix + 'data: ' + json.dumps(event) + '\n\n').encode())
                if protocol == 'chat':
                    self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()

            def do_POST(self):
                try:
                    length = int(self.headers.get('Content-Length', '0'))
                    assert 0 < length <= 1048576
                    request = json.loads(self.rfile.read(length))
                    assert self.path == '/v1/' + {'responses': 'responses', 'chat': 'chat/completions', 'messages': 'messages'}[protocol]
                    if not request.get('tools'):
                        assert request.get('max_output_tokens', request.get('max_completion_tokens', request.get('max_tokens'))) == 256
                        auxiliary.append(request['model'])
                        assert len(auxiliary) <= 3
                        self.send(response(protocol, 'synthetic title'))
                        return
                    lane = request['model']
                    assert lane in ('root', 'child')
                    request['_beta'] = self.headers.get('anthropic-beta', '')
                    requests.append(request)
                    check(request, lane)
                    key = stage, lane
                    rounds[key] = rounds.get(key, 0) + 1
                    number = rounds[key]
                    call = None
                    if lane == 'root' and number == 1:
                        arguments = {'agent': 'helper', 'description': 'Chronological child', 'prompt': f'CHILD_TASK_{stage}',
                                     'model': 'fixture/child' + ('#high' if mismatch and stage else '#low' if stage == 1 else '')}
                        if child:
                            arguments['sessionID'] = child
                        call = 'subagent', arguments
                    elif lane == 'child' and number == 1 and stage == 0:
                        call = 'read', {'path': 'seed.txt'}
                    else:
                        assert number == (2 if lane == 'root' or stage == 0 else 1), 'unexpected replay'
                    self.send(response(protocol, f'{lane}-{stage}-{number}', call))
                except Exception as error:
                    errors.append(str(error))
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Peer)
        server.daemon_threads = False
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        configuration = {'model': 'fixture/root', 'compaction': {'auto': False},
                         'agent': {'title': {'disable': True}, 'build': {'system': 'ROOT_PROFILE'},
                                   'helper': {'mode': 'subagent', 'system': 'CHILD_PROFILE', 'model': 'fixture/child',
                                              'permission': {'read': 'allow'}}},
                         'permission': {'read': 'allow', 'subagent': 'allow'},
                         'provider': {'fixture': {'npm': {'responses': '@ai-sdk/openai', 'chat': '@ai-sdk/openai-compatible',
                                                        'messages': '@ai-sdk/anthropic'}[protocol],
                                                  'options': {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'synthetic'},
                                                  'models': {lane: {'limit': {'context': 65536, 'output': 2048},
                                                                   'compatibility': {'supportsEffortUpdates': supported, 'supportsNativeSystemUpdates': supported},
                                                                   'variants': {'low': {'reasoningEffort': 'low'}, 'high': {'reasoningEffort': 'high'}}}
                                                             for lane in ('root', 'child')}}}}

        def invoke():
            (config / 'opencode.json').write_text(json.dumps(configuration))
            process = subprocess.Popen([str(binary), '--data-dir', str(data), 'run', '--json', '--session', 'chronology', f'ROOT_TASK_{stage}'],
                                       cwd=project, env=environment, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                try:
                    stdout, stderr = process.communicate(timeout=20)
                except subprocess.TimeoutExpired:
                    raise AssertionError({'protocol': protocol, 'support': supported, 'mismatch': mismatch,
                                          'stage': stage, 'rounds': list(rounds.items()), 'peer_errors': errors}) from None
                assert not errors, errors
                assert process.returncode == 0, (process.returncode, stderr[-1024:].decode())
                assert len(stdout) < 1048576 and len(stderr) < 65536
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
                process.stdout.close()
                process.stderr.close()
            assert not Path(f'/proc/{process.pid}').exists()

        def seed(session, lane, effort, previous):
            # These new fixtures use the public durable message/event shape;
            # existing messages, turns, outcomes and raw journals are not edited.
            with sqlite3.connect(data / 'oc.sqlite') as db:
                seq = db.execute('SELECT COALESCE(MAX(seq),0) FROM messages').fetchone()[0]
                # Messages' native update slot also requires a user/system/
                # assistant boundary; declaration alone cannot admit every slot.
                for offset, role, body in [(1, 'user', f'CHRONO_BEFORE_{lane.upper()}_{stage}'),
                                          (2, 'system', UPDATE), (3, 'assistant', f'CHRONO_BOUNDARY_{lane.upper()}_{stage}')]:
                    identity = f'chronology-{lane}-{stage}-{role}'
                    db.execute('INSERT INTO messages(id,session_id,seq,role,text) VALUES(?,?,?,?,?)', (identity, session, seq + offset, role, body))
                    db.execute("INSERT INTO events(session_id,kind,payload) VALUES(?,'message',?)", (session, identity))
                db.execute("INSERT INTO events(session_id,kind,payload) VALUES(?,'session_model_selected',?)",
                           (session, json.dumps({'effort_update': {'effort': effort, 'previous': previous}})))

        try:
            invoke()
            with sqlite3.connect(data / 'oc.sqlite') as db:
                child, = db.execute("SELECT id FROM sessions WHERE parent_id='chronology'").fetchone()
                original = db.execute('SELECT id,result FROM turns ORDER BY rowid').fetchall()
            for stage in (1, 2):
                seed('chronology', 'root', 'low' if stage == 1 else None, None if stage == 1 else 'low')
                seed(child, 'child', 'low' if stage == 1 else None, None if stage == 1 else 'low')
                configuration['model'] = 'fixture/root' + ('#high' if mismatch else '#low' if stage == 1 else '')
                invoke()
            with sqlite3.connect(data / 'oc.sqlite') as db:
                assert db.execute("SELECT COUNT(*) FROM sessions WHERE parent_id='chronology'").fetchone()[0] == 1
                assert db.execute("SELECT state FROM tool_operations WHERE name='read'").fetchall() == [('completed',)]
                assert db.execute("SELECT status FROM turns").fetchall() == [('completed',)] * 6
                for identity, raw in original:
                    assert db.execute('SELECT result FROM turns WHERE id=?', (identity,)).fetchone()[0] == raw
                for session in ('chronology', child):
                    markers = [item for (raw,) in db.execute('SELECT result FROM turns WHERE session_id=? ORDER BY rowid', (session,))
                               for item in json.loads(raw)['input'] if item.get('type') == 'effort_update']
                    assert [(item['effort'], item['previous']) for item in markers] == [('low', None), (None, 'low')]
            assert len(requests) == 10, 'unexpected physical requests'
            return {'protocol': protocol, 'declared_support': supported, 'mismatch': mismatch, 'requests': len(requests),
                    'title_requests': len(auxiliary),
                    'root_child_reopen': True, 'settled_read_replayed': False, 'old_turns_immutable': True}
        finally:
            server.shutdown()
            worker.join(timeout=5)
            server.server_close()
            assert not worker.is_alive()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    cases = [('responses', True, False), ('responses', False, False), ('responses', True, True),
             ('chat', True, False), ('messages', False, False), ('messages', True, False)]
    receipts = [qualify(binary, *case) for case in cases]
    print(json.dumps({'status': 'PASS', 'cases': receipts, 'real_provider_requests': 0, 'owned_cleanup': 'joined'}))


if __name__ == '__main__':
    main()
