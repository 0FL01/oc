import hashlib
import io
import json
import os
from pathlib import Path
import pwd
import sqlite3
import tempfile
import unittest

import execute_fixture as fixture
import tool_preview_fixture


class Handler:
    def __init__(self, path='/v1/responses'):
        self.path, self.wfile, self.status = path, io.BytesIO(), None

    def send_response(self, status):
        self.status = status

    def send_error(self, status, _message):
        self.status = status

    def send_header(self, *_args):
        pass

    def end_headers(self):
        pass


class ExecuteFixture(unittest.TestCase):
    def setUp(self):
        fixture.requests = fixture.main_requests = fixture.fetches = 0
        fixture.fetch_paths.clear()
        fixture.code_release.set()
        fixture.fetch_release.set()
        self.spec = {'origin':'oc', 'execute_preview':True, 'execute_fixture_port':45678}
        self.body = {'stream':True, 'model':'fixture-model-1', 'input':[],
                     'tools':[{'name':name} for name in ('read','grep','glob','webfetch','execute')]}

    def respond(self, body):
        handler, logs = Handler(), []
        fixture.respond(handler, body, self.spec, logs.append)
        events = [json.loads(line[6:]) for line in handler.wfile.getvalue().decode().splitlines()
                  if line.startswith('data: ')]
        return handler, logs, events

    def test_compound_sse_owns_distinct_ids_indices_arguments_and_completed_items(self):
        handler, logs, events = self.respond(self.body)
        self.assertEqual(handler.status, 200)
        completed = events[-1]['response']['output']
        self.assertEqual([item['call_id'] for item in completed], fixture.SUCCESS_IDS)
        self.assertEqual(len({item['id'] for item in completed}), 5)
        for index, item in enumerate(completed):
            added = next(event for event in events if event['type'] == 'response.output_item.added'
                         and event['output_index'] == index)
            self.assertEqual(added['item']['arguments'], '')
            deltas = [event['delta'] for event in events if event['type'] == 'response.function_call_arguments.delta'
                      and event['output_index'] == index and event['item_id'] == item['id']]
            self.assertEqual(''.join(deltas), item['arguments'])
        self.assertEqual([event['sequence_number'] for event in events], list(range(len(events))))
        self.assertEqual(len([event for event in logs if event['kind'] == 'fixture_tool_call']), 5)
        self.assertIn('search({query: "output", namespace: "visexecute", limit: 8})', fixture.program(self.spec))

    def test_continuations_require_exact_ids_and_original_execute_output(self):
        results = [{'type':'function_call_output', 'call_id':name,
                    'output':fixture.OUTPUT if name == 'call_vis46_success' else 'actual direct result'}
                   for name in fixture.SUCCESS_IDS]
        for main, ids in [(2, results), (4, results + [{'type':'function_call_output', 'call_id':'call_vis46_error','output':'error'}])]:
            for corrupt in (False, True):
                with self.subTest(main=main, corrupt=corrupt):
                    fixture.requests, fixture.main_requests = main, main - 1
                    supplied = [dict(item) for item in ids]
                    if corrupt:
                        supplied[0]['call_id'] = supplied[1]['call_id']
                    handler, logs, _ = self.respond({**self.body, 'input':supplied})
                    self.assertEqual(handler.status, 400 if corrupt else 200)
                    self.assertEqual(logs[0]['valid'], not corrupt)
        fixture.requests, fixture.main_requests = 2, 1
        results[-1]['output'] = 'not the actual captured output'
        self.assertEqual(self.respond({**self.body, 'input':results})[0].status, 400)

    def test_only_one_direct_and_one_nested_get_are_admitted(self):
        logs = []
        for route in ('/vis46-direct', '/vis46-fetch'):
            handler = Handler(route)
            fixture.fetch(handler, logs.append)
            self.assertEqual(handler.status, 200)
            self.assertIn(b'VIS46 ', handler.wfile.getvalue())
        duplicate = Handler('/vis46-direct')
        fixture.fetch(duplicate, logs.append)
        self.assertEqual(duplicate.status, 400)
        self.assertEqual([event['valid'] for event in logs], [True, True, False])

    def test_snapshot_hashes_settled_raw_without_copying_live_payload(self):
        root = Path(pwd.getpwuid(os.getuid()).pw_dir)/'.cache/opencode-tmp/opencode'
        with tempfile.TemporaryDirectory(dir=root, prefix='execute-fixture-test-') as directory:
            home = Path(directory)
            (home/'data').mkdir()
            with sqlite3.connect(home/'data/fixture.db') as connection:
                connection.execute('CREATE TABLE turns(id TEXT,session_id TEXT,status TEXT,result TEXT)')
                connection.executemany('INSERT INTO turns VALUES(?,?,?,?)', [
                    ('finished','session','completed','{"display_parts":[]}'),
                    ('active','session','running','unsettled')])
            snapshot = tool_preview_fixture.snapshot(home, home, self.spec)
            raw = snapshot['observations'][0]['data']['settled_raw']
            self.assertEqual(len(raw), 1)
            self.assertEqual(raw[0]['sha256'], hashlib.sha256(b'{"display_parts":[]}').hexdigest())
            self.assertNotIn('result', raw[0])
            self.assertIn('http_fetches', snapshot)


if __name__ == '__main__':
    unittest.main()
