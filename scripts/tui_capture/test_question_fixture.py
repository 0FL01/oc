import copy
import io
import json
import sqlite3
import unittest

import question_fixture as fixture


class QuestionFixtureTests(unittest.TestCase):
    def test_provider_accepts_only_exact_owned_results_and_never_cancel_continuation(self):
        class Handler:
            path = '/v1/responses'

            def __init__(self):
                self.wfile = io.BytesIO()
                self.error = None

            def send_response(self, status):
                self.status = status

            def send_header(self, *_):
                pass

            def end_headers(self):
                pass

            def send_error(self, status, message):
                self.error = (status, message)

        for origin in ('oc', 'upstream'):
            fixture.main_requests = fixture.title_requests = 0
            events = []
            body = {'model': 'fixture-model-1', 'stream': True,
                    'tools': [{'name': 'question'}],
                    'input': [{'role': 'user', 'content': fixture.PROMPT}]}
            first = Handler()
            fixture.respond(first, body, {'origin': origin}, events.append, False, 1)
            self.assertIsNone(first.error)
            self.assertEqual(events[0]['valid'], True)
            self.assertEqual(events[1]['call_id'], 'call_vis37_answer')
            output = json.dumps(fixture.NATIVE_RESULT) if origin == 'oc' else fixture.DONOR_RESULT
            result = {'type': 'function_call_output', 'call_id': 'call_vis37_answer', 'output': output}
            self.assertTrue(fixture.valid_result(result, origin))
            invalid = copy.deepcopy(result)
            invalid['call_id'] = 'call_vis37_cancel'
            self.assertFalse(fixture.valid_result(invalid, origin))
            invalid = copy.deepcopy(result)
            invalid['output'] = 'unstructured guessed answer'
            self.assertFalse(fixture.valid_result(invalid, origin))
            body['input'].append(result)
            second = Handler()
            fixture.respond(second, body, {'origin': origin}, events.append, False, 2)
            self.assertIsNone(second.error)
            body['input'].append({'role': 'user', 'content': fixture.CANCEL})
            third = Handler()
            fixture.respond(third, body, {'origin': origin}, events.append, False, 3)
            self.assertIsNone(third.error)
            self.assertEqual([e['call_id'] for e in events if e['kind'] == 'fixture_tool_call'],
                             ['call_vis37_answer', 'call_vis37_cancel'])
            fourth = Handler()
            fixture.respond(fourth, body, {'origin': origin}, events.append, False, 4)
            self.assertEqual(fourth.error[0], 400)
            self.assertFalse(events[-1]['valid'])

    def test_readonly_projection_keeps_donor_source_ids_metadata_and_native_raw(self):
        with sqlite3.connect(':memory:') as connection:
            connection.row_factory = sqlite3.Row
            connection.executescript('CREATE TABLE session_message(id,session_id,type,seq,data);')
            part = {'type': 'tool', 'name': 'question', 'id': 'call_vis37_answer',
                    'state': {'status': 'completed', 'input': fixture.QUESTIONS,
                              'content': [{'type': 'text', 'text': fixture.DONOR_RESULT}],
                              'metadata': {'answers': fixture.ANSWERS}}}
            connection.execute('INSERT INTO session_message VALUES(?,?,?,?,?)',
                               ('m1', 's1', 'assistant', 7, json.dumps({'content': [part]})))
            connection.execute('PRAGMA query_only=1')
            observed = fixture.snapshot(connection, {'session_message'}, 'upstream')
            self.assertEqual(observed['operations'][0]['part_index'], 0)
            self.assertEqual(observed['operations'][0]['message_id'], 'm1')
            self.assertEqual(observed['operations'][0]['data'], part)
            self.assertTrue(observed['readonly'])
        with sqlite3.connect(':memory:') as connection:
            connection.row_factory = sqlite3.Row
            connection.executescript('CREATE TABLE tool_operations(id,session_id,turn_id,provider_call_id,name,state,input,output);'
                                     'CREATE TABLE messages(id,session_id,seq,role,text);')
            connection.execute('INSERT INTO tool_operations VALUES(?,?,?,?,?,?,?,?)',
                               ('op1', 's1', 't1', 'call_vis37_answer', 'question', 'completed', json.dumps(fixture.QUESTIONS),
                                json.dumps(fixture.NATIVE_RESULT)))
            connection.execute('INSERT INTO messages VALUES(?,?,?,?,?)', ('u1', 's1', 1, 'user', fixture.PROMPT))
            connection.execute('PRAGMA query_only=1')
            observed = fixture.snapshot(connection, {'messages', 'tool_operations'}, 'oc')
            self.assertEqual(observed['messages'][0]['text'], fixture.PROMPT)
            self.assertEqual(json.loads(observed['operations'][0]['output']), fixture.NATIVE_RESULT)
            self.assertTrue(observed['readonly'])


if __name__ == '__main__':
    unittest.main()
