import json
import io
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import threading
import unittest

import tool_preview_fixture as fixture
import composer_fixture as composer


class ToolPreviewFixtureTests(unittest.TestCase):
    def test_pending_subagent_flushes_a_real_incomplete_contract_until_owned_release(self):
        class Handler:
            path = '/v1/responses'
            def __init__(self):
                self.wfile = io.BytesIO()
                self.headers = {}
            def send_response(self, status):
                self.status = status
            def send_header(self, key, value):
                self.headers[key] = value
            def end_headers(self):
                pass
            def send_error(self, status, message):
                raise AssertionError((status, message))

        for origin in ('oc', 'upstream'):
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                spec = {'origin': origin, 'combined_composer': True, 'subagent_cards': True,
                        'subagent_pending': True, 'pending_animation': True}
                config = {'providers': {'fixture': {'models': {'fixture-model-1': {}}}}}
                cli = {'session': {}}
                fixture.configure(spec, root, root, config, cli)
                self.assertIs(cli['animations'], True)
                if origin == 'oc':
                    self.assertIs(config['animations'], True)
                handler, flushed, observations, errors = Handler(), threading.Event(), [], []
                def emit(event):
                    observations.append(event)
                    if event['kind'] == 'pending_contract_flushed':
                        flushed.set()
                def respond():
                    try:
                        fixture.respond(handler, {'model': 'fixture-model-1', 'stream': True,
                            'tools': [{'name': 'subagent'}],
                            'input': [{'role': 'user', 'content': composer.ROOT_TASK}]}, spec, emit)
                    except Exception as error:
                        errors.append(error)
                worker = threading.Thread(target=respond)
                worker.start()
                try:
                    self.assertTrue(flushed.wait(3), errors)
                    before = handler.wfile.getvalue()
                    partial = [json.loads(line[6:]) for line in before.decode().splitlines() if line.startswith('data: ')]
                    self.assertEqual(len(partial), 3)
                    prefix = partial[-1]['delta']
                    self.assertGreaterEqual(len(prefix.encode()), 256)
                    self.assertIn('Inspect captured child shell', prefix)
                    with self.assertRaises(json.JSONDecodeError):
                        json.loads(prefix)
                    self.assertTrue(worker.is_alive())
                    with self.assertRaises(ValueError):
                        composer.control(root, 'release-pending', {'origin': origin})
                    ack = composer.control(root, 'release-pending', spec)
                    self.assertEqual(ack['call_id'], 'call_vis39_parent')
                finally:
                    composer.pending_release.set()
                    worker.join(3)
                self.assertFalse(worker.is_alive())
                self.assertFalse(errors, errors)
                payload = handler.wfile.getvalue()
                self.assertEqual(len(payload), int(handler.headers['Content-Length']))
                events = [json.loads(line[6:]) for line in payload.decode().splitlines() if line.startswith('data: ')]
                self.assertEqual([event['sequence_number'] for event in events], list(range(len(events))))
                args = json.loads(''.join(event['delta'] for event in events if event['type'] == 'response.function_call_arguments.delta'))
                self.assertEqual(args['description'], 'Inspect captured child shell')
                self.assertEqual(args, json.loads(events[-2]['item']['arguments']))
                self.assertEqual(events[-1]['type'], 'response.completed')
                self.assertEqual(len([event for event in observations if event['kind'] == 'fixture_tool_call']), 1)

    def test_pending_snapshot_requires_a_recognized_positive_ts_owner(self):
        with tempfile.TemporaryDirectory() as directory:
            home = pathlib.Path(directory)
            spec = {'origin': 'upstream', 'subagent_cards': True, 'subagent_pending': True}
            with self.assertRaises(AssertionError):
                composer.snapshot(home, home, spec)
            (home / 'data').mkdir()
            with sqlite3.connect(home / 'data' / 'owner.db') as conn:
                conn.executescript('CREATE TABLE session_v2(id,parent_id,time_created,fork_session_id);'
                                   'CREATE TABLE session_message(id,session_id,type,seq,data);'
                                   "INSERT INTO session_v2 VALUES('root',NULL,1,NULL);")
                part = {'type': 'tool', 'id': 'call_vis39_parent', 'name': 'subagent',
                        'state': {'status': 'streaming', 'input': ''}, 'time': {'created': 1}}
                conn.execute('INSERT INTO session_message VALUES(?,?,?,?,?)',
                             ('assistant', 'root', 'assistant', 1, json.dumps({'content': [part]})))
            data = composer.snapshot(home, home, spec)['observations'][0]['data']
            self.assertEqual(data['pending_schema'], 'ts')
            self.assertEqual(len(data['sessions']), 1)
            self.assertEqual(data['tool_parts'][0]['state']['status'], 'streaming')
            self.assertNotIn('ran', data['tool_parts'][0]['time'])

    def test_execute_profile_uses_real_codemode_with_only_exact_nested_authorities(self):
        for origin, tool in [('oc', 'visexecute__output'), ('upstream', 'visexecute_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                spec = {'origin':origin, 'execute_preview':True, 'execute_fixture_port':45678}
                fixture.configure(spec, pathlib.Path(directory), pathlib.Path(directory),
                                  config, {'session':{}})
                server = config['mcp']['servers']['visexecute']
                self.assertIs(server['codemode'], True)
                self.assertEqual(server['command'][-1], '--small')
                self.assertEqual(config['permissions'], [
                    {'action':'*', 'resource':'*', 'effect':'deny'},
                     {'action':'execute', 'resource':'*', 'effect':'allow'},
                     {'action':tool, 'resource':'*', 'effect':'allow'},
                     *[{'action':name, 'resource':'*', 'effect':'allow'} for name in ('read','grep','glob')],
                     *[{'action':'webfetch', 'resource':'http://localhost:45678/'+name, 'effect':'allow'}
                       for name in ('vis46-direct','vis46-fetch')]])
                self.assertNotIn('setCacheKey', config)

    def test_generic_preview_uses_explicit_direct_mcp_on_both_binaries(self):
        for origin, tool in [('oc', 'vis16__output'), ('upstream', 'vis16_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                fixture.configure({'origin': origin}, pathlib.Path(directory),
                                  pathlib.Path(directory), config, {'session': {}})
                self.assertIs(config['mcp']['servers']['vis16']['codemode'], False)
                self.assertEqual(config['permissions'][0]['effect'], 'deny')
                self.assertEqual([rule['action'] for rule in config['permissions']
                                  if rule['effect'] == 'allow'], [tool, 'shell'])

    def test_small_profile_keeps_limits_and_identical_exact_shell_permission(self):
        for origin, tool in [('oc', 'vispreview__output'), ('upstream', 'vispreview_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                spec = {'origin':origin, 'tool_preview_small':True}
                fixture.configure(spec, pathlib.Path(directory), pathlib.Path(directory),
                                  config, {'session':{}})
                server = config['mcp']['servers']['vispreview']
                self.assertIs(server['codemode'], False)
                self.assertEqual(server['command'][-1], '--small')
                self.assertEqual(config['tool_output'], {'max_lines':12, 'max_bytes':1024})
                self.assertEqual(config['permissions'], [
                    {'action':'*', 'resource':'*', 'effect':'deny'},
                    {'action':tool, 'resource':'*', 'effect':'allow'},
                    *[{'action':'shell', 'resource':resource, 'effect':'allow'} for resource in
                      ([fixture.SMALL_COMMAND] if origin == 'oc' else
                       [fixture.SMALL_PRINTF, fixture.SMALL_EFFECT])]])
                self.assertEqual(fixture.command(spec), fixture.SMALL_COMMAND)

    def test_small_peer_returns_real_complete_literal_marker_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            requests = [{'jsonrpc':'2.0', 'id':index, 'method':'tools/call',
                         'params':{'name':'output', 'arguments':{'index':index}}}
                        for index in [1,2]]
            peer = pathlib.Path(fixture.__file__).with_name('tool_preview_mcp.py')
            process = subprocess.run([sys.executable, str(peer), '--small'],
                                     input=''.join(json.dumps(request)+'\n' for request in requests),
                                     capture_output=True, text=True, check=True, timeout=10,
                                     env={'HOME':directory})
            results = [json.loads(line)['result'] for line in process.stdout.splitlines()]
            self.assertEqual(len(results), 2)
            for result in results:
                self.assertIs(result['isError'], False)
                body = result['content'][0]['text']
                self.assertLess(len(body.encode()), 1024)
                self.assertLess(len(body.splitlines()), 12)
                for marker in ['[Part preview truncated]',
                               '[output preview truncated; full result retained]', '[truncated]']:
                    self.assertIn(marker, body)


if __name__ == '__main__':
    unittest.main()
