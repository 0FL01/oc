"""Bounded VIS37 real question calls and read-only native/donor source records."""
import json
import threading

import prompt_caret_fixture

PROMPT = 'VIS37 paired question consumer'
CANCEL = 'VIS37 dismiss the next question'
TITLE = 'VIS question fixture'
DONE = 'VIS37-DONE: owned answers accepted.'
CUSTOM = 'custom Ω界'
QUESTIONS = {'questions': [
    {'header': 'Languages and native runtimes', 'question': 'Pick languages?', 'multiple': True,
     'options': [{'label': 'Rust', 'description': 'Native owner and a bounded ordered answer.'},
                 {'label': 'Python', 'description': 'Fixture peer, not a production runtime dependency.'}]},
    {'header': 'Platform and execution targets', 'multiple': False,
     'question': 'Where Ω界? Choose an explicit host for the current workspace and retain this long question in metadata order.',
     'options': [{'label': 'Linux', 'description': 'Current GNU/Linux host, with no external service or paid effect.'},
                 {'label': 'Other', 'description': 'An explicit custom selection remains owned by this question.'}]},
]}
ANSWERS = [['Rust', 'Python'], [CUSTOM]]
NATIVE_RESULT = {**QUESTIONS, 'answers': ANSWERS}
DONOR_RESULT = 'User has answered your questions: ' + ', '.join(
    '"' + q['question'] + '"="' + ', '.join(a) + '"'
    for q, a in zip(QUESTIONS['questions'], ANSWERS)
) + ". You can now continue with the user's answers in mind."
state_lock = threading.Lock()
main_requests = 0
title_requests = 0


def configure(spec, config):
    config['mcp'] = {'servers': {}}
    config['permissions'] = [{'action': '*', 'resource': '*', 'effect': 'deny'},
                             {'action': 'question', 'resource': '*', 'effect': 'allow'}]
    # The fixture's typed result is small and complete; do not replace it with a
    # cold-output envelope merely because the ordinary VIS16 fixture uses 1KiB.
    config['tool_output'] = {'max_lines': 100, 'max_bytes': 8192}


def valid_result(result, origin):
    if result.get('call_id') != 'call_vis37_answer':
        return False
    output = result.get('output')
    if origin == 'upstream':
        return output == DONOR_RESULT
    if not isinstance(output, str) or len(output.encode()) > 8192:
        return False
    try:
        return json.loads(output) == NATIVE_RESULT
    except (ValueError, TypeError):
        return False


def respond(handler, body, spec, emit, title, number):
    global main_requests, title_requests
    with state_lock:
        if title:
            title_requests += 1
            index = title_requests
        else:
            main_requests += 1
            index = main_requests
    users = []
    for item in body.get('input', []):
        if item.get('role') == 'user':
            content = item.get('content', [])
            users.append(content if isinstance(content, str) else ''.join(
                p.get('text', '') for p in content if p.get('type') in ('input_text', 'text')))
    results = [item for item in body.get('input', []) if item.get('type') == 'function_call_output']
    definitions = [item.get('name') for item in body.get('tools', [])]
    valid = handler.path == '/v1/responses' and body.get('stream') is True
    valid = valid and body.get('model') == 'fixture-model-1' and number <= 4
    if title:
        valid = valid and index == 1
    else:
        valid = valid and 'question' in definitions and index in (1, 2, 3)
        valid = valid and users == ([PROMPT, CANCEL] if index == 3 else [PROMPT])
        valid = valid and (not results if index == 1 else len(results) == 1 and valid_result(results[0], spec['origin']))
    emit({'kind': 'provider', 'operation': 'question_title' if title else 'question_main',
          'valid': valid, 'index': index, 'user_texts': users, 'actual_results': results,
          'registered_tools': definitions})
    if not valid:
        handler.send_error(400, 'VIS37 structured question contract rejected')
        return
    if title or index == 2:
        prompt_caret_fixture.emit_message(handler, body, TITLE if title else DONE, number)
    else:
        call_id = 'call_vis37_answer' if index == 1 else 'call_vis37_cancel'
        item = {'id': 'fc_vis37_' + str(number), 'type': 'function_call', 'status': 'completed',
                'call_id': call_id, 'name': 'question', 'arguments': json.dumps(QUESTIONS)}
        emit({'kind': 'fixture_tool_call', 'name': 'question', 'call_id': call_id, 'arguments': QUESTIONS})
        response = {'id': 'resp_vis37_' + str(number), 'object': 'response', 'created_at': 1700000000,
                    'model': body['model'], 'status': 'in_progress', 'output': [], 'error': None,
                    'incomplete_details': None}
        events = [{'type': 'response.created', 'response': response},
                  {'type': 'response.output_item.added', 'output_index': 0,
                   'item': {**item, 'status': 'in_progress', 'arguments': ''}},
                  {'type': 'response.function_call_arguments.delta', 'item_id': item['id'],
                   'output_index': 0, 'delta': item['arguments']},
                  {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
                  {'type': 'response.completed', 'response': {**response, 'status': 'completed',
                   'output': [item], 'usage': {'input_tokens': 1234, 'output_tokens': 64, 'total_tokens': 1298}}}]
        payload = ''.join('event: ' + event['type'] + '\ndata: ' +
                          json.dumps({**event, 'sequence_number': i}) + '\n\n'
                          for i, event in enumerate(events)).encode()
        handler.send_response(200)
        handler.send_header('Content-Type', 'text/event-stream')
        handler.send_header('Content-Length', str(len(payload)))
        handler.end_headers()
        handler.wfile.write(payload)
        handler.wfile.flush()
    emit({'kind': 'provider_completed', 'operation': 'question_title' if title else 'question_main'})


def snapshot(connection, tables, origin):
    def rows(sql):
        return [dict(row) for row in connection.execute(sql)]

    if origin == 'oc' and {'messages', 'tool_operations'} <= tables:
        operations = rows("SELECT id,session_id,turn_id,provider_call_id,name,state,"
                          "CASE WHEN length(CAST(input AS BLOB))<=16384 THEN input END AS input,"
                          "CASE WHEN length(CAST(output AS BLOB))<=16384 THEN output END AS output "
                          "FROM tool_operations WHERE name='question' ORDER BY rowid LIMIT 4")
        messages = rows("SELECT id,session_id,seq,role,"
                        "CASE WHEN length(CAST(text AS BLOB))<=8192 THEN text END AS text "
                        "FROM messages ORDER BY seq LIMIT 8")
        return {'operations': operations, 'messages': messages, 'readonly': True}
    if origin == 'upstream' and 'session_message' in tables:
        # The running pinned binary stores V2 content parts inside the source
        # message. V1 message/part tables are not its actual projection.
        if connection.execute('SELECT 1 FROM session_message WHERE length(CAST(data AS BLOB))>16384 LIMIT 1').fetchone():
            raise AssertionError('Question source message exceeds bounded observation')
        records = rows("SELECT sm.id AS message_id,sm.session_id,sm.seq,j.key AS part_index,j.value AS data "
                       "FROM session_message sm,json_each(sm.data,'$.content') j "
                       "WHERE sm.type='assistant' AND json_extract(j.value,'$.type')='tool' "
                       "AND json_extract(j.value,'$.name')='question' ORDER BY sm.seq,j.key LIMIT 4")
        for row in records:
            if row['data'] is None:
                raise AssertionError('Question part exceeds bounded observation')
            row['data'] = json.loads(row['data'])
        messages = rows("SELECT id,session_id,seq,type,data FROM session_message ORDER BY seq LIMIT 8")
        return {'operations': records, 'messages': messages, 'readonly': True}
    return {'operations': [], 'messages': [], 'readonly': True}
