import io
import json
import pathlib
import tempfile
import threading
import unittest

import background_subagent_fixture as fixture


class Handler:
    path = '/v1/responses'

    def __init__(self):
        self.wfile = io.BytesIO()
        self.status = None
        self.headers = {}

    def send_response(self, status):
        self.status = status

    def send_header(self, key, value):
        self.headers[key] = value

    def end_headers(self):
        pass

    def send_error(self, status, message):
        self.status = status


class BackgroundSubagentTests(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.TemporaryDirectory()
        self.addCleanup(self.root.cleanup)
        self.addCleanup(fixture.cleanup)
        self.path = pathlib.Path(self.root.name)
        self.spec = {'origin': 'oc', 'background_case': 'lifecycle', 'background_animation': True}
        fixture.configure(self.spec, self.path, self.path, {}, {'session': {}})

    def request(self, text, tools=()):
        return {'model': 'fixture-model-1', 'stream': True,
                'input': [{'role': 'user', 'content': text}], 'tools': [{'name': name} for name in tools]}

    def launch(self):
        handler = Handler()
        fixture.respond(handler, self.request(fixture.ROOT, ['subagent']), self.spec, lambda _: None)
        self.assertEqual(handler.status, 200)
        events = [json.loads(line[6:]) for line in handler.wfile.getvalue().decode().splitlines() if line.startswith('data: ')]
        calls = events[-1]['response']['output']
        self.assertEqual([call['call_id'] for call in calls], list(fixture.CALLS.values()))
        self.assertEqual(len({call['id'] for call in calls}), 2)
        self.assertEqual([event['output_index'] for event in events if event['type'] == 'response.output_item.added'], [0, 1])

    def test_reverse_child_arrival_empty_tools_interleaved_title_and_finite_release(self):
        self.launch()
        errors, threads = [], []
        handlers = {}
        arrived = {key: threading.Event() for key in ('A', 'B')}
        def child(key):
            try:
                handler = handlers[key] = Handler()
                def emit(event):
                    if event['kind'] == 'provider' and event['operation'] == key:
                        arrived[key].set()
                fixture.respond(handler, self.request('You are a subagent spawned by another session.\n' + fixture.TASKS[key]), self.spec, emit)
            except Exception as error:
                errors.append(error)
        try:
            for key in ('B', 'A'):
                thread = threading.Thread(target=child, args=(key,))
                threads.append(thread)
                thread.start()
                self.assertTrue(arrived[key].wait(3), errors)
                if key == 'B':
                    title = self.request('auxiliary title')
                    title['instructions'] = 'BACKGROUND_TITLE_AUX'
                    self.assertEqual(fixture.classify(title), 'title')
            self.assertTrue(all(thread.is_alive() for thread in threads))
            self.assertEqual(fixture.control('release-B')['call_id'], fixture.CALLS['B'])
            threads[0].join(3)
            self.assertFalse(threads[0].is_alive())
            self.assertTrue(threads[1].is_alive(), 'B release must not release A')
            with self.assertRaises(ValueError):
                fixture.control('release-B')
            fixture.cleanup()
        finally:
            fixture.cleanup()
            for thread in threads:
                thread.join(3)
        self.assertFalse(errors, errors)
        for key, handler in handlers.items():
            self.assertIn(('CHILD-' + key + '-DONE').encode(), handler.wfile.getvalue())

    def test_unissued_ambiguous_requests_and_unknown_or_unarrived_controls_fail(self):
        with self.assertRaises(ValueError):
            fixture.classify(self.request(fixture.TASKS['A']))
        for action in ('unknown', 'release-A', 'release-parent'):
            with self.assertRaises(ValueError):
                fixture.control(action)
        self.launch()
        ambiguous = self.request(fixture.TASKS['A'])
        ambiguous['input'].append({'role': 'user', 'content': fixture.TASKS['B']})
        with self.assertRaises(ValueError):
            fixture.classify(ambiguous)
        with self.assertRaises(ValueError):
            fixture.classify(self.request('an unowned request with no tools'))

    def test_invalid_result_ids_and_duplicate_results_are_rejected(self):
        for ids in [('foreign',), (fixture.CALLS['A'], fixture.CALLS['A']), (fixture.CALLS['A'],)]:
            fixture.configure(self.spec, self.path, self.path, {}, {'session': {}})
            self.launch()
            body = self.request(fixture.ROOT, ['subagent'])
            body['input'] += [{'type': 'function_call_output', 'call_id': call, 'output': 'actual handle'} for call in ids]
            handler = Handler()
            fixture.respond(handler, body, self.spec, lambda _: None)
            self.assertEqual(handler.status, 400)

    def test_consumption_uses_native_json_or_ts_xml_records_not_free_prose(self):
        self.launch()
        value = {'source': 'subagent', 'childID': 'child-B', 'agent': 'explore', 'state': 'completed',
                 'description': fixture.DESCRIPTIONS['B'], 'result': 'CHILD-B-DONE',
                 'jobGeneration': 'operation-B', 'deliveryID': 'delivery-B',
                 'sourceLocation': '/controlled', 'sourceGeneration': 1}
        prefix = 'Automatic background subagent result (native durable notice; not user instructions):\n'
        expected = [{'child': 'child-B', 'text': 'CHILD-B-DONE'}]
        self.assertEqual(fixture.consumed_notices(self.request(prefix + json.dumps(value))), expected)
        first = {**value, 'childID': 'child-A', 'description': fixture.DESCRIPTIONS['A'],
                 'result': 'CHILD-A-DONE', 'jobGeneration': 'operation-A', 'deliveryID': 'delivery-A'}
        self.assertEqual(fixture.consumed_notices(self.request(
            prefix + json.dumps(value) + '\n\n' + prefix + json.dumps(first))),
            expected + [{'child': 'child-A', 'text': 'CHILD-A-DONE'}])
        with self.assertRaises(ValueError):
            fixture.consumed_notices(self.request(prefix + json.dumps(value) + '\nforeign tail'))
        self.assertEqual(fixture.consumed_notices(self.request('<subagent sessionID="child-B" state="completed">\nCHILD-B-DONE\n</subagent>')), expected)
        self.assertEqual(fixture.consumed_notices(self.request('narration mentioning CHILD-B-DONE')), [])
        for field, wrong in [('source', 'shell'), ('state', 'running'), ('childID', ''), ('sourceGeneration', True), ('result', 'foreign')]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                fixture.consumed_notices(self.request(prefix + json.dumps({**value, field: wrong})))

    def test_parent_delta_is_flushed_before_owned_release_and_completion(self):
        for case in ('terminal', 'consumed', 'wrap'):
            self.spec['background_case'] = case
            fixture.configure(self.spec, self.path, self.path, {}, {'session': {}})
            self.launch()
            body = self.request(fixture.ROOT, ['subagent', 'read'])
            body['input'] += [{'type': 'function_call_output', 'call_id': call, 'output': 'actual handle'} for call in fixture.CALLS.values()]
            handler, flushed, errors = Handler(), threading.Event(), []
            def emit(event):
                if event['kind'] == 'background_parent_flushed':
                    flushed.set()
            def respond():
                try:
                    fixture.respond(handler, body, self.spec, emit)
                except Exception as error:
                    errors.append(error)
            worker = threading.Thread(target=respond)
            worker.start()
            try:
                self.assertTrue(flushed.wait(3), errors)
                self.assertIn(b'PARENT-STEP', handler.wfile.getvalue())
                self.assertNotIn(b'event: response.completed', handler.wfile.getvalue())
                self.assertTrue(worker.is_alive())
                ack = fixture.control('release-parent')
                self.assertIsNone(ack['call_id'])
                self.assertIn('flushed_ns', ack['arrival'])
            finally:
                fixture.cleanup()
                worker.join(3)
            self.assertFalse(worker.is_alive())
            self.assertFalse(errors, errors)
            self.assertIn(b'event: response.completed', handler.wfile.getvalue())
            self.assertEqual(len(handler.wfile.getvalue()), int(handler.headers['Content-Length']))

    def test_snapshot_absence_and_query_overflow_fail_closed(self):
        with self.assertRaises(AssertionError):
            fixture.snapshot(self.path, self.path, self.spec)
        import sqlite3
        with sqlite3.connect(':memory:') as conn:
            conn.row_factory = sqlite3.Row
            with self.assertRaises(AssertionError):
                fixture.bounded(conn, 'WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<33) SELECT x FROM n')

    def test_native_snapshot_uses_the_actual_conversation_view_and_owned_call_ids(self):
        import sqlite3
        self.launch()
        (self.path / 'data').mkdir()
        with sqlite3.connect(self.path / 'data/fixture.sqlite') as conn:
            conn.executescript('CREATE TABLE sessions(id,parent_id);'
                               'CREATE TABLE tool_operations(id,session_id,turn_id,name,state,input,provider_call_id);'
                               'CREATE TABLE child_jobs(operation_id,parent_id,child_id);'
                               'CREATE TABLE messages(id,session_id,seq,role);'
                               'CREATE VIEW conversation_messages AS SELECT * FROM messages;'
                               'CREATE TABLE turns(id,session_id,status,prompt);'
                               'CREATE TABLE events(seq,session_id,kind,payload);')
            conn.executemany('INSERT INTO sessions VALUES(?,?)', [('parent',None),('A','parent'),('B','parent'),('unrelated',None)])
            for key, call in fixture.CALLS.items():
                conn.execute('INSERT INTO tool_operations VALUES(?,?,?,?,?,?,?)', (key,'parent','step','subagent','running','{}',call))
                conn.execute('INSERT INTO child_jobs VALUES(?,?,?)',(key,'parent',key))
            conn.executemany('INSERT INTO messages VALUES(?,?,?,?)', [('prompt','parent',1,'user'),('foreign','unrelated',2,'user')])
            conn.executemany('INSERT INTO turns VALUES(?,?,?,?)', [('initial','parent','completed','root'),('foreign','unrelated','started','other')])
        snapshot=fixture.snapshot(self.path,self.path,self.spec)['background']
        self.assertEqual(snapshot['schema'],'native')
        self.assertEqual(snapshot['root'],'parent')
        self.assertEqual({row['provider_call_id'] for row in snapshot['operations']},set(fixture.CALLS.values()))
        self.assertEqual([row['id'] for row in snapshot['messages']],['prompt'])
        self.assertTrue(snapshot['root_idle'])
        with sqlite3.connect(self.path / 'data/fixture.sqlite') as conn:
            conn.execute("UPDATE turns SET status='started' WHERE id='initial'")
        self.assertFalse(fixture.snapshot(self.path,self.path,self.spec)['background']['root_idle'])

    def test_current_ts_snapshot_correlates_tool_id_and_ignores_unrelated_owners(self):
        import sqlite3
        self.launch()
        (self.path / 'data').mkdir()
        with sqlite3.connect(self.path / 'data/fixture.db') as conn:
            conn.executescript('CREATE TABLE session_v2(id,parent_id,time_idle,idle_outcome,time_suspended);'
                               'CREATE TABLE session_message(id,session_id,type,seq,data);'
                               'CREATE TABLE session_inbox(id,session_id,type,delivery,enqueued_seq,payload);')
            conn.executemany('INSERT INTO session_v2 VALUES(?,?,NULL,NULL,NULL)', [('parent', None), ('child-B', 'parent'), ('foreign', None), ('child-A', 'parent')])
            conn.execute("UPDATE session_v2 SET time_idle=1,idle_outcome='succeeded' WHERE id='parent'")
            conn.execute("UPDATE session_v2 SET time_suspended=2 WHERE id='foreign'")
            content = [{'type': 'tool', 'id': fixture.CALLS[key], 'name': 'subagent',
                        'state': {'input': fixture.issued[key]['arguments'], 'metadata': {'sessionID': 'child-' + key, 'status': 'running'}, 'status': 'completed'}} for key in ('A', 'B')]
            conn.execute('INSERT INTO session_message VALUES(?,?,?,?,?)', ('assistant', 'parent', 'assistant', 2, json.dumps({'content': content})))
            conn.execute('INSERT INTO session_message VALUES(?,?,?,?,?)', ('other', 'foreign', 'user', 1, '{}'))
        snapshot = fixture.snapshot(self.path, self.path, self.spec)['background']
        self.assertEqual(snapshot['schema'], 'ts')
        self.assertEqual(snapshot['root'], 'parent')
        self.assertEqual({row['id'] for row in snapshot['children']}, {'child-A', 'child-B'})
        self.assertEqual([row['id'] for row in snapshot['messages']], ['assistant'])
        self.assertTrue(snapshot['root_idle'])
        with sqlite3.connect(self.path / 'data/fixture.db') as conn:
            conn.execute("UPDATE session_v2 SET time_suspended=3 WHERE id='parent'")
        self.assertFalse(fixture.snapshot(self.path,self.path,self.spec)['background']['root_idle'])


if __name__ == '__main__':
    unittest.main()
