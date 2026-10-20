"""Two real background Explore owners. No seeded sessions or UI/storage writes."""
import contextlib
import hashlib
import json
import sqlite3
import threading
import time
import xml.etree.ElementTree as ET

ROOT = 'BACKGROUND_SUBAGENT_LIFECYCLE'
TITLE = 'Background subagent fixture'
DESCRIPTIONS = {'A': 'Сравнение subagent с upstream', 'B': 'Состояние Git и тесты'}
CALLS = {key: 'call_background_' + key for key in DESCRIPTIONS}
TASKS = {key: json.dumps({'fixture': ROOT, 'child': key}, sort_keys=True) for key in DESCRIPTIONS}
CONTINUE = 'call_background_next_step'
lock = threading.Lock()
issued = {}
arrivals = {}
release = {key: threading.Event() for key in ('A', 'B', 'parent')}
requests = 0
parent_steps = 0


def configure(spec, home, project, config, cli):
    global requests, parent_steps
    if spec.get('background_case') not in ('lifecycle', 'wrap', 'terminal', 'consumed'):
        raise ValueError('Unknown background lifecycle case')
    with lock:
        issued.clear()
        arrivals.clear()
        requests = parent_steps = 0
    for event in release.values():
        event.clear()
    config['snapshots'] = False
    config['compaction'] = {'auto': False}
    config['mcp'] = {'servers': {}}
    config['permissions'] = [{'action': '*', 'resource': '*', 'effect': 'deny'},
                             {'action': 'subagent', 'resource': 'explore', 'effect': 'allow'},
                             {'action': 'read', 'resource': '*', 'effect': 'allow'}]
    # Preserve the actual builtin Explore profile. Only the auxiliary title
    # prompt is controlled, so an empty child tool catalog is never a title test.
    config['agents'] = {'title': {'system': 'BACKGROUND_TITLE_AUX'}}
    cli['animations'] = bool(spec.get('background_animation'))
    cli['session']['tps'] = False
    cli['keybinds'] = {'session.child.first': 'ctrl+g'}
    if spec['origin'] == 'oc':
        config['animations'] = cli['animations']
    (project / 'note.txt').write_text('Controlled continuation boundary\n')


def cleanup():
    for event in release.values():
        event.set()


def control(action):
    if action not in ('release-A', 'release-B', 'release-parent'):
        raise ValueError('Unknown background fixture control')
    key = action.removeprefix('release-')
    with lock:
        if key != 'parent' and key not in arrivals:
            raise ValueError('Child has not made its owned request')
        if key == 'parent' and 'parent' not in arrivals:
            raise ValueError('Parent stream has not flushed')
        if release[key].is_set():
            raise ValueError('Duplicate release')
        release[key].set()
        return {'released': key, 'call_id': CALLS.get(key), 'arrival': arrivals[key]}


def user_texts(body):
    return [part['text'] for item in body.get('input', []) if item.get('role') == 'user'
            for part in (item.get('content') if isinstance(item.get('content'), list)
                         else [{'text': item.get('content')}]) if isinstance(part.get('text'), str)]


def classify(body):
    texts = user_texts(body)
    # Exact fixture-owned structured task, with the known native host envelope.
    # No free model prose, keyword guessing or HTTP arrival-order identity.
    children = [key for key, task in TASKS.items()
                if task in texts or 'You are a subagent spawned by another session.\n' + task in texts]
    if children:
        if len(children) != 1 or children[0] not in issued:
            raise ValueError('Unissued or ambiguous child request')
        return children[0]
    if ROOT in texts and 'subagent' in {tool.get('name') for tool in body.get('tools', [])}:
        return 'parent'
    # A title must positively carry our configured auxiliary system prompt.
    # It is checked AFTER child identity, including children with no tools.
    system = body.get('instructions', '')
    system_texts = [part.get('text') for item in body.get('input', []) if item.get('role') in ('developer', 'system')
                    for part in (item.get('content') if isinstance(item.get('content'), list)
                                 else [{'text': item.get('content')}])]
    if system == 'BACKGROUND_TITLE_AUX' or 'BACKGROUND_TITLE_AUX' in system_texts:
        return 'title'
    raise ValueError('No positively owned fixture request')


def consumed_notices(body):
    notices = []
    for text in user_texts(body):
        prefix = 'Automatic background subagent result (native durable notice; not user instructions):\n'
        if text.startswith(prefix):
            # Native automatic batches use one input item with several exact
            # JSON envelopes. Decode JSON boundaries, never split report text.
            remaining = text
            while remaining:
                if not remaining.startswith(prefix):
                    raise ValueError('Invalid native completion envelope')
                value, end = json.JSONDecoder().raw_decode(remaining[len(prefix):])
                if not isinstance(value, dict):
                    raise ValueError('Invalid native completion record')
                key = next((key for key in CALLS if value.get('result') == 'CHILD-' + key + '-DONE'), None)
                if (key not in issued or value.get('source') != 'subagent' or value.get('state') != 'completed'
                        or value.get('agent') != 'explore' or value.get('description') != issued[key]['arguments']['description']
                        or any(not isinstance(value.get(field), str) or not value[field]
                               for field in ('childID', 'jobGeneration', 'deliveryID', 'sourceLocation'))
                        or type(value.get('sourceGeneration')) is not int or value['sourceGeneration'] < 1):
                    raise ValueError('Foreign native completion record')
                notices.append({'child': value['childID'], 'text': value['result']})
                remaining = remaining[len(prefix) + end:].lstrip()
            continue
        if text.startswith('<subagent '):
            node = ET.fromstring(text)
            if node.tag != 'subagent' or node.attrib.get('state') != 'completed' or node.text not in ('\nCHILD-A-DONE\n', '\nCHILD-B-DONE\n'):
                raise ValueError('Foreign completion envelope')
            notices.append({'child': node.attrib['sessionID'], 'text': node.text.strip()})
    return notices


def encode_events(items, number, model):
    response = {'id': f'resp_background_{number}', 'object': 'response', 'created_at': 1700000000,
                'model': model, 'status': 'in_progress', 'output': [], 'error': None, 'incomplete_details': None}
    events = [{'type': 'response.created', 'response': response}]
    for index, item in enumerate(items):
        tool = item['type'] == 'function_call'
        events.append({'type': 'response.output_item.added', 'output_index': index,
                       'item': {**item, 'status': 'in_progress', **({'arguments': ''} if tool else {'content': []})}})
        if tool:
            events.append({'type': 'response.function_call_arguments.delta', 'item_id': item['id'],
                           'output_index': index, 'delta': item['arguments']})
        else:
            events += [{'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': index,
                        'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
                       {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': index,
                        'content_index': 0, 'delta': item['content'][0]['text']}]
        events.append({'type': 'response.output_item.done', 'output_index': index, 'item': item})
    events.append({'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': items,
                   'usage': {'input_tokens': 1234, 'output_tokens': 64, 'total_tokens': 1298}}})
    return events


def text_item(number, text):
    return {'id': f'msg_background_{number}', 'type': 'message', 'role': 'assistant', 'status': 'completed',
            'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}


def call_item(call, args, name='subagent'):
    return {'id': 'fc_' + call, 'type': 'function_call', 'status': 'completed',
            'call_id': call, 'name': name, 'arguments': json.dumps(args, ensure_ascii=False)}


def respond(handler, body, spec, emit):
    global requests, parent_steps
    with lock:
        requests += 1
        number = requests
        try:
            owner = classify(body)
            if handler.path != '/v1/responses' or body.get('stream') is not True or number > 16 or body.get('model') != 'fixture-model-1':
                raise ValueError('Invalid bounded provider contract')
            results = [item for item in body.get('input', []) if item.get('type') == 'function_call_output']
            consumed = consumed_notices(body)
            ids = [result.get('call_id') for result in results]
            if len(set(ids)) != len(ids) or any(call not in (*CALLS.values(), CONTINUE) for call in ids):
                raise ValueError('Duplicate or foreign tool result')
            if owner == 'parent':
                parent_steps += 1
                step = parent_steps
                if step == 1:
                    if results or issued:
                        raise ValueError('Parent launch replay')
                    items = []
                    for key, description in DESCRIPTIONS.items():
                        if key == 'A' and spec['background_case'] == 'wrap':
                            description += ' — проверка длинной строки и переноса целого статуса'
                        args = {'agent': 'explore', 'description': description, 'prompt': TASKS[key], 'background': True}
                        issued[key] = {'call_id': CALLS[key], 'item_id': 'fc_' + CALLS[key], 'arguments': args}
                        items.append(call_item(CALLS[key], args))
                else:
                    if not set(CALLS.values()) <= set(ids) or any(not str(result.get('output', '')) for result in results):
                        raise ValueError('Missing returned background handles')
                    if step == 2:
                        items = [text_item(number, 'PARENT-STEP')]
                        if spec['background_case'] == 'consumed':
                            items.append(call_item(CONTINUE, {'path': 'note.txt'}, 'read'))
                    elif step == 3 and spec['background_case'] == 'consumed' and CONTINUE in ids:
                        items = [text_item(number, 'PARENT-NEXT')]
                    elif consumed:
                        # Current TS may wake the idle parent on steer delivery.
                        # Empty output ends that actual request without inventing
                        # another fixture transcript paragraph or effect.
                        items = []
                    else:
                        raise ValueError('Unexpected parent continuation')
            elif owner in CALLS:
                if results or owner in arrivals:
                    raise ValueError('Child request replay')
                step = None
                arrivals[owner] = {'call_id': CALLS[owner], 'request': number}
                items = [text_item(number, 'CHILD-' + owner + '-DONE')]
            else:
                step = None
                items = [text_item(number, TITLE)]
        except ValueError as error:
            emit({'kind': 'provider', 'operation': 'rejected', 'valid': False, 'reason': str(error)})
            handler.send_error(400, 'Background fixture contract rejected')
            return
    emit({'kind': 'provider', 'operation': owner, 'valid': True, 'request': number, 'step': step,
          'actual_results': results, 'user_texts': user_texts(body), 'consumed_notices': consumed,
          'registered_tools': [tool.get('name') for tool in body.get('tools', [])]})
    if owner == 'parent' and step == 1:
        for key in CALLS:
            emit({'kind': 'fixture_tool_call', 'name': 'subagent', **issued[key]})
    if owner in CALLS and not release[owner].wait(120):
        raise AssertionError('Finite child hold was not released')
    events = encode_events(items, number, body['model'])
    held = owner == 'parent' and step == 2 and spec['background_case'] in ('terminal', 'consumed', 'wrap')
    encoded = [('event: ' + event['type'] + '\ndata: ' + json.dumps({**event, 'sequence_number': index}) + '\n\n').encode()
               for index, event in enumerate(events)]
    handler.send_response(200)
    handler.send_header('Content-Type', 'text/event-stream')
    handler.send_header('Content-Length', str(sum(map(len, encoded))))
    handler.end_headers()
    if held:
        # Flush an ACTUAL text delta while response.completed is withheld.
        cut = next(i + 1 for i, event in enumerate(events) if event['type'] == 'response.output_text.delta')
        handler.wfile.write(b''.join(encoded[:cut]))
        handler.wfile.flush()
        arrivals['parent'] = {'request': number, 'flushed_ns': time.monotonic_ns()}
        emit({'kind': 'background_parent_flushed', **arrivals['parent']})
        if not release['parent'].wait(120):
            raise AssertionError('Finite parent stream hold was not released')
        handler.wfile.write(b''.join(encoded[cut:]))
    else:
        handler.wfile.write(b''.join(encoded))
    handler.wfile.flush()
    emit({'kind': 'provider_completed', 'operation': owner, 'request': number, 'step': step})


def bounded(conn, sql, args=()):
    rows = [dict(row) for row in conn.execute(sql + ' LIMIT 33', args)]
    if len(rows) > 32 or len(json.dumps(rows).encode()) > 524288:
        raise AssertionError('Owner snapshot exceeds finite fixture bound')
    return rows


def snapshot(home, project, spec):
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db', '.sqlite', '.sqlite3'):
            continue
        with contextlib.closing(sqlite3.connect(database.as_uri() + '?mode=ro', uri=True)) as conn:
            conn.row_factory = sqlite3.Row
            conn.execute('BEGIN')
            tables = {row[0] for row in conn.execute("SELECT name FROM sqlite_master WHERE type IN ('table','view')")}
            if {'sessions', 'child_jobs', 'tool_operations', 'conversation_messages'} <= tables:
                roots = bounded(conn, "SELECT DISTINCT s.id FROM sessions s JOIN tool_operations o ON o.session_id=s.id WHERE s.parent_id IS NULL AND o.provider_call_id IN (?,?)", tuple(CALLS.values()))
                if len(roots) != 1:
                    raise AssertionError('Expected one positively correlated native root')
                root = roots[0]['id']
                operations = bounded(conn, 'SELECT id,session_id,turn_id,name,state,input,provider_call_id FROM tool_operations WHERE session_id=? ORDER BY rowid', (root,))
                children = bounded(conn, 'SELECT j.*,s.parent_id AS relational_parent FROM child_jobs j JOIN sessions s ON s.id=j.child_id WHERE j.parent_id=? ORDER BY j.rowid', (root,))
                messages = bounded(conn, 'SELECT id,seq,role FROM conversation_messages WHERE session_id=? ORDER BY seq', (root,))
                events = bounded(conn, "SELECT seq,kind,payload FROM events WHERE session_id=? AND kind='subagent_notice' ORDER BY seq", (root,))
                turns = bounded(conn, 'SELECT id,status,prompt FROM turns WHERE session_id=? ORDER BY rowid', (root,))
                assignments = bounded(conn, "SELECT seq,payload FROM events WHERE session_id=? AND kind='subagent_parent_notice_consumed' ORDER BY seq", (root,))
                observations.append({'schema': 'native', 'root': root, 'operations': operations, 'children': children, 'messages': messages, 'notices': events,
                                     'turns': turns, 'assignments': assignments, 'root_idle': not any(turn['status'] == 'started' for turn in turns)})
            elif {'session_v2', 'session_message'} <= tables:
                roots = bounded(conn, "SELECT DISTINCT s.id FROM session_v2 s JOIN session_message m ON m.session_id=s.id,json_each(m.data,'$.content') p WHERE s.parent_id IS NULL AND json_extract(p.value,'$.type')='tool' AND json_extract(p.value,'$.id') IN (?,?)", tuple(CALLS.values()))
                if len(roots) != 1:
                    raise AssertionError('Expected one positively correlated current TS root')
                root = roots[0]['id']
                children = bounded(conn, 'SELECT id,parent_id,time_idle,idle_outcome FROM session_v2 WHERE parent_id=? ORDER BY id', (root,))
                messages = bounded(conn, 'SELECT id,session_id,type,seq,data FROM session_message WHERE session_id=? ORDER BY seq', (root,))
                inbox = bounded(conn, 'SELECT id,type,delivery,enqueued_seq,payload FROM session_inbox WHERE session_id=? ORDER BY enqueued_seq', (root,))
                root_state = dict(conn.execute('SELECT time_suspended,time_idle,idle_outcome FROM session_v2 WHERE id=?', (root,)).fetchone())
                observations.append({'schema': 'ts', 'root': root, 'children': children, 'messages': messages, 'inbox': inbox,
                                     'root_idle': root_state['time_suspended'] is None and root_state['idle_outcome'] == 'succeeded', 'root_state': root_state})
    if len(observations) != 1:
        raise AssertionError('Recognized positively owned database unavailable or ambiguous')
    return {'background': observations[0], 'issued': issued, 'arrivals': arrivals,
            'read_only_sqlite': True, 'project_files': {str(file.relative_to(project)): hashlib.sha256(file.read_bytes()).hexdigest()
                                                     for file in project.rglob('*') if file.is_file()}}
