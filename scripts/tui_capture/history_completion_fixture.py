"""Actual bounded primary/child completion; canonical payload exceeds old 64 KiB."""
import hashlib
import json
import sqlite3
import threading

ROOT_TASK = 'T70_INSPECT_ROOT'
DELEGATE_TASK = 'T70_INSPECT_CHILD'
lock = threading.Lock()
released = {name: threading.Event() for name in ('root', 'child')}
ready = {name: False for name in released}
requests = 0
delegated = False
THOUGHTS = ['T70-THOUGHT-%d ' % n + ('canonical public reasoning ' * 640) + '\n\nT70-THOUGHT-END-%d' % n
            for n in range(5)]
assert sum(len(text.encode()) for text in THOUGHTS) > 65536
CALLS = [('read', {'path': 'history-note.txt'}),
         ('grep', {'pattern': 'T70_MARKER', 'path': '.'}),
         ('glob', {'pattern': 'history-*.txt', 'path': '.'}),
         ('read', {'path': 'history-absent.txt'})]


def configure(spec, home, project, config, cli):
    config['mcp'] = {'servers': {}}
    rules = [{'action': '*', 'resource': '*', 'effect': 'deny'},
             *[{'action': name, 'resource': '*', 'effect': 'allow'}
               for name in ('read', 'grep', 'glob')],
             {'action': 'subagent', 'resource': 'helper', 'effect': 'allow'}]
    config['permissions'] = rules
    config['agents'] = {'helper': {'mode': 'subagent', 'description': 'History inspector',
                                   'system': 'Execute only the fixture calls.', 'permissions': rules}}
    config['providers']['fixture']['models']['fixture-model-1']['limit'] = {
        'context': 200000, 'output': 32768}
    cli['keybinds'] = {'session.child.first': 'ctrl+g'}
    (project / 'history-note.txt').write_text('T70_MARKER\nCanonical history fixture.\n')


def control(action):
    source = action.removeprefix('release-')
    if source not in released or action != 'release-' + source:
        raise ValueError('Unknown completion fixture control')
    released[source].set()
    return {'released': source}


def snapshot(home):
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db', '.sqlite', '.sqlite3'):
            continue
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as conn:
            conn.row_factory = sqlite3.Row
            tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            data = {}
            for name in ('sessions', 'session_v2', 'child_jobs', 'tool_operations'):
                if name in tables:
                    data[name] = [dict(row) for row in conn.execute(f'SELECT * FROM "{name}" ORDER BY rowid LIMIT 16')]
            if 'turns' in tables:
                data['turns'] = []
                for row in conn.execute('SELECT id,session_id,status,result FROM turns ORDER BY rowid LIMIT 8'):
                    raw = row[3] or '{}'
                    reasoning = [part['reasoning'] for part in json.loads(raw).get('display_parts', [])
                                 if isinstance(part.get('reasoning'), str)]
                    data['turns'].append({
                        'id': row[0], 'session': row[1], 'status': row[2],
                        'bytes': len(raw.encode()), 'sha256': hashlib.sha256(raw.encode()).hexdigest(),
                        'public_reasoning_bytes': sum(len(text.encode()) for text in reasoning),
                        'public_reasoning_sha256': [hashlib.sha256(text.encode()).hexdigest() for text in reasoning]})
            observations.append({'data': data})
    with lock:
        return {'observations': observations, 'ready': dict(ready), 'requests': requests,
                'canonical_reasoning_bytes_per_source': sum(len(s.encode()) for s in THOUGHTS),
                'expected_public_reasoning_sha256': [hashlib.sha256(text.encode()).hexdigest() for text in THOUGHTS],
                'read_only_sqlite': True}


def respond(handler, body, emit):
    global requests, delegated
    definitions = {entry.get('name') for entry in body.get('tools', [])}
    items = body.get('input', [])
    texts = [part.get('text') for item in items if item.get('role') == 'user'
             for part in (item.get('content') if isinstance(item.get('content'), list)
                          else [{'text': item.get('content')}])]
    results = {item.get('call_id') for item in items if item.get('type') == 'function_call_output'}
    with lock:
        requests += 1
        number = requests
        child = delegated and not any(text in (ROOT_TASK, DELEGATE_TASK) for text in texts)
    title = not definitions
    source = 'child' if child else 'root'
    delegation = not title and not child and texts[-1:] == [DELEGATE_TASK]
    tool_calls = []
    if delegation and 'call_t70_delegate' not in results:
        tool_calls = [('call_t70_delegate', 'subagent', {
            'agent': 'helper', 'description': 'Inspect child history', 'prompt': 'Inspect canonical child history.'})]
        with lock:
            delegated = True
    elif not title and not delegation and f'call_t70_{source}_0' not in results:
        tool_calls = [(f'call_t70_{source}_{index}', name, args)
                      for index, (name, args) in enumerate(CALLS)]
    valid = handler.path == '/v1/responses' and body.get('stream') is True and number <= 12
    valid = valid and body.get('model') == 'fixture-model-1'
    valid = valid and all(name in definitions for _, name, _ in tool_calls)
    emit({'kind': 'provider', 'operation': 'title' if title else source,
          'valid': valid, 'call_ids': sorted(results), 'actual_tool_calls': len(tool_calls)})
    if not valid:
        handler.send_error(400, 'completion fixture contract rejected')
        return
    output = []
    if tool_calls and not delegation:
        output += [{'type': 'reasoning', 'id': f'rs_t70_{source}_{index}',
                    'summary': [{'type': 'summary_text', 'text': text}]}
                   for index, text in enumerate(THOUGHTS)]
    for call_id, name, args in tool_calls:
        output.append({'id': 'fc_' + call_id, 'type': 'function_call', 'status': 'completed',
                       'call_id': call_id, 'name': name, 'arguments': json.dumps(args)})
        emit({'kind': 'fixture_tool_call', 'name': name, 'call_id': call_id, 'arguments': args})
    text = 'T70 history inspection' if title else 'T70-PARENT-DONE' if delegation else f'T70-{source.upper()}-DONE'
    if not output:
        output.append({'type': 'message', 'id': f'msg_t70_{number}', 'status': 'completed',
                       'role': 'assistant', 'content': [{'type': 'output_text', 'text': text, 'annotations': []}]})
    response = {'id': f'resp_t70_{number}', 'object': 'response', 'created_at': 1700000000,
                'model': body['model'], 'status': 'in_progress', 'output': [],
                'error': None, 'incomplete_details': None}
    handler.send_response(200)
    handler.send_header('Content-Type', 'text/event-stream')
    handler.send_header('Connection', 'close')
    handler.end_headers()
    sequence = 0

    def event(value):
        nonlocal sequence
        value = {**value, 'sequence_number': sequence}
        sequence += 1
        handler.wfile.write(('event: ' + value['type'] + '\ndata: ' + json.dumps(value) + '\n\n').encode())
        handler.wfile.flush()

    event({'type': 'response.created', 'response': response})
    for index, item in enumerate(output):
        event({'type': 'response.output_item.added', 'output_index': index,
               'item': {**item, 'status': 'in_progress',
                        **({'arguments': ''} if item['type'] == 'function_call' else
                           {'summary': []} if item['type'] == 'reasoning' else {'content': []})}})
        if item['type'] == 'function_call':
            event({'type': 'response.function_call_arguments.delta', 'item_id': item['id'],
                   'output_index': index, 'delta': item['arguments']})
        elif item['type'] == 'reasoning':
            event({'type': 'response.reasoning_summary_part.added', 'item_id': item['id'],
                   'output_index': index, 'summary_index': 0,
                   'part': {'type': 'summary_text', 'text': ''}})
            event({'type': 'response.reasoning_summary_text.delta', 'item_id': item['id'],
                   'output_index': index, 'summary_index': 0,
                   'delta': item['summary'][0]['text']})
            event({'type': 'response.reasoning_summary_text.done', 'item_id': item['id'],
                   'output_index': index, 'summary_index': 0,
                   'text': item['summary'][0]['text']})
            event({'type': 'response.reasoning_summary_part.done', 'item_id': item['id'],
                   'output_index': index, 'summary_index': 0, 'part': item['summary'][0]})
        elif item['type'] == 'message':
            event({'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': index,
                   'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}})
            event({'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': index,
                   'content_index': 0, 'delta': text})
        event({'type': 'response.output_item.done', 'output_index': index, 'item': item})
    if not title and not delegation and not tool_calls:
        with lock:
            ready[source] = True
        emit({'kind': 'completion_ready', 'source': source})
        if not released[source].wait(30):
            raise AssertionError('Actual response completion was not released')
    event({'type': 'response.completed', 'response': {
        **response, 'status': 'completed', 'output': output,
        'usage': {'input_tokens': 1234, 'output_tokens': 64, 'total_tokens': 1298}}})
    emit({'kind': 'provider_completed', 'operation': 'title' if title else source})
