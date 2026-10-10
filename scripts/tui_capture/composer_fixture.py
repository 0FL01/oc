"""VIS39 real delegation/process/PTY fixture; no seeded sessions or UI state."""
import hashlib
import json
from pathlib import Path
import sqlite3
import threading

ROOT_TASK = 'VIS39_ROOT_TASK'
HANDOFF_TASK = 'VIS39_ROOT_HANDOFF_TASK'
CHILD_TASK = 'VIS39_CHILD_TASK'
MODEL_TASK = 'VIS39_ROOT_MODEL_SHELL_TASK'
CONTINUE_TASK = 'VIS39_CONTINUE_ROOT_TASK'
CONTINUE_CHILD_TASK = 'VIS39_CONTINUE_CHILD_TASK'
ERROR_TASK = 'VIS39_MISSING_AGENT_TASK'
COMMANDS = ['python3 composer-probe.py first', 'python3 composer-probe.py second']
lock = threading.Lock()
requests = 0
parent_call_issued = False
continuation_child = None
continuation_call_issued = False
continuation_release = threading.Event()


def configure(spec, home, project, config, cli):
    config['mcp'] = {'servers': {}} if spec['origin'] == 'upstream' else {}
    config['tool_output'] = {'max_lines': 1000, 'max_bytes': 65536}
    cli['keybinds'] = {'session.child.first': 'ctrl+g'}
    if spec.get('subagent_cards') or spec.get('root_handoff'):
        provider = config['providers' if spec['origin'] == 'upstream' else 'provider']['fixture']
        provider['models']['fixture-model-1']['name'] = 'Fixture Caption Model'
    if spec['origin'] == 'upstream':
        rules = [{'action': '*', 'resource': '*', 'effect': 'deny'},
                 {'action': 'subagent', 'resource': 'helper', 'effect': 'allow'},
                 *[{'action': 'shell', 'resource': c, 'effect': 'allow'} for c in COMMANDS]]
        config['permissions'] = rules
        config['agents'] = {'helper': {'mode': 'subagent', 'description': 'VIS39 fixture worker',
                                      'system': 'Execute only the fixture-owned tool calls.',
                                      'permissions': rules}}
    else:
        rules = {'*': 'deny', 'subagent': {'*': 'deny', 'helper': 'allow'},
                 'bash': {'*': 'deny', **{c: 'allow' for c in COMMANDS}}}
        config['permissions'] = rules
        config['agent'] = {'helper': {'mode': 'subagent', 'description': 'VIS39 fixture worker',
                                      'prompt': 'Execute only the fixture-owned tool calls.',
                                      'permissions': rules}}
    if spec.get('root_handoff'):
        cli['keybinds']['session.background'] = 'ctrl+y'
        config['agents' if spec['origin'] == 'upstream' else 'agent']['build'] = {'color':'#12ab34'}
    program = '''import os, sys, time
from pathlib import Path
phase = sys.argv[1]
assert phase in ('first', 'second')
root = Path(__file__).resolve().parent
origin = ORIGIN
with (root / ('composer-' + origin + '-started.effects')).open('a') as f:
    f.write(phase + '\\n')
print('VIS39-LIVE-' + phase, flush=True)
for n in ((79,) if ROOT_HANDOFF or ROOT_MODEL and phase == 'second' else range(80)):
    print('VIS39-ROW-%03d' % n, flush=True)
if ROOT_MODEL or ROOT_HANDOFF:
    # Separate real pipe publications for the bounded visible-tail assertion;
    # simultaneous stdout/stderr drains do not promise OS-wide ordering.
    time.sleep(.2)
print('\\x1b[31mVIS39-STDERR-' + phase + '\\x1b[0m', file=sys.stderr, flush=True)
(root / ('composer-' + origin + '-ready-' + phase)).write_text(str(os.getpid()))
deadline = time.monotonic() + 180
while not (root / ('composer-' + origin + '-release-' + phase)).exists():
    if time.monotonic() >= deadline:
        raise SystemExit('fixture release deadline')
    time.sleep(.025)
print('VIS39-FINAL-FLUSH-' + phase, flush=True)
with (root / ('composer-' + origin + '-completed.effects')).open('a') as f:
    f.write(phase + '\\n')
'''
    (project / 'composer-probe.py').write_text(program.replace('ORIGIN', repr(spec['origin']))
                                              .replace('ROOT_MODEL', repr(bool(spec.get('model_shell'))))
                                              .replace('ROOT_HANDOFF', repr(bool(spec.get('root_handoff')))))


def control(project, action, spec):
    global continuation_child
    if spec.get('subagent_cards') and action == 'capture-continuation':
        # Capture an actual relational child ID, never original RAW/tool prose.
        data = snapshot(project.parent / spec['origin'] / 'home', project, spec)
        rows = [row for observation in data['observations']
                for row in observation['data'].get('sessions', [])
                if row.get('parent_id') is not None]
        if len(rows) != 1:
            raise AssertionError('Expected one actual admitted continuation child')
        continuation_child = rows[0]['id']
        return {'captured_child': continuation_child}
    if spec.get('subagent_cards') and action == 'release-continuation':
        continuation_release.set()
        return {'released': 'continuation'}
    if action not in ('release-first', 'release-second'):
        raise ValueError('Unknown finite composer fixture control')
    phase = action.removeprefix('release-')
    (project / ('composer-' + spec['origin'] + '-release-' + phase)).write_text('release')
    return {'released': phase}


def snapshot(home, project, spec):
    effects = {}
    for name in ('started', 'completed', 'terminal'):
        file = project / ('composer-' + spec['origin'] + '-' + name + '.effects')
        data = file.read_bytes() if file.exists() else b''
        if len(data) > 1024:
            raise AssertionError('Composer fixture effect counter exceeded bound')
        effects[name] = {'lines': data.decode().splitlines(), 'bytes': len(data),
                         'sha256': hashlib.sha256(data).hexdigest()}
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db', '.sqlite', '.sqlite3'):
            continue
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as conn:
            conn.row_factory = sqlite3.Row
            tables = {r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            data = {}
            for table in ('sessions', 'child_jobs', 'shell_jobs', 'tool_operations', 'terminals', 'terminal_selection'):
                if table in tables:
                    query = f'SELECT * FROM "{table}" ORDER BY rowid LIMIT 12'
                    if table == 'shell_jobs':
                        query = """SELECT j.*,
                          EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.session_id
                            AND e.kind='shell_foreground' AND e.payload=j.operation_id) AS foreground_admitted,
                          EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.session_id
                            AND e.kind='shell_background' AND e.payload=j.operation_id) AS converted_background
                           FROM shell_jobs j ORDER BY j.rowid LIMIT 12"""
                    elif table == 'child_jobs' and spec.get('root_handoff'):
                        query = """SELECT j.*, EXISTS(SELECT 1 FROM events e
                          WHERE e.session_id=j.parent_id AND e.kind='subagent_background'
                          AND e.payload=j.operation_id) AS converted_background
                          FROM child_jobs j ORDER BY j.rowid LIMIT 12"""
                    data[table] = [dict(r) for r in conn.execute(query)]
            if (spec.get('subagent_cards') or spec.get('root_handoff')) and 'session_v2' in tables:
                data['sessions'] = [dict(r) for r in conn.execute(
                    'SELECT id,parent_id,time_created FROM session_v2 '
                    'WHERE fork_session_id IS NULL ORDER BY time_created,id LIMIT 12')]
            observations.append({'tables': sorted(tables), 'data': data})
    ready = {phase: (project / ('composer-' + spec['origin'] + '-ready-' + phase)).exists()
             for phase in ('first', 'second')}
    return {'effects': effects, 'ready': ready, 'observations': observations,
            'read_only_sqlite': True}


def respond(handler, body, spec, emit):
    global requests, parent_call_issued, continuation_call_issued
    with lock:
        requests += 1
        number = requests
    items = body.get('input', [])
    results = [i for i in items if i.get('type') == 'function_call_output']
    definitions = {i.get('name'): i for i in body.get('tools', [])}
    # These are controlled user/tool arguments, not free assistant prose.
    texts = [part.get('text') for i in items if i.get('role') == 'user'
             for part in (i.get('content') if isinstance(i.get('content'), list)
                          else [{'text': i.get('content')}])]
    child = any(i.get('call_id', '').startswith('call_vis39_shell_') for i in results)
    if not results and definitions:
        # The only next tool-bearing request after our explicit delegation is
        # its child. Native may wrap the controlled task in its context pack.
        # Correlate the issued structured call, not a substring of that pack.
        with lock:
            child = parent_call_issued and ROOT_TASK not in texts
    title = not definitions
    root_model = bool(spec.get('model_shell'))
    handoff = bool(spec.get('root_handoff'))
    cards = bool(spec.get('subagent_cards'))
    current_marker = next((text for text in reversed(texts)
                           if text in (ROOT_TASK, CONTINUE_TASK, ERROR_TASK)), None)
    continuation_parent = cards and current_marker == CONTINUE_TASK
    # The issued structured continuation also identifies its native context-pack
    # request; do not parse that task envelope or free assistant text.
    continuation_worker = cards and continuation_call_issued and child and not continuation_parent
    missing_agent = cards and current_marker == ERROR_TASK
    if continuation_parent or missing_agent:
        child = False
    elif continuation_worker:
        child = True
    if root_model:
        # This separate root scenario has no delegation/context-pack inference.
        child = False
    if handoff:
        child = parent_call_issued and HANDOFF_TASK not in texts
    index = sum(i.get('call_id', '').startswith('call_vis39_shell_') for i in results) if child else len(results)
    valid = handler.path == '/v1/responses' and body.get('stream') is True and number <= (18 if cards else 12)
    valid = valid and body.get('model') == 'fixture-model-1'
    valid = valid and (title or (HANDOFF_TASK in texts or child if handoff else MODEL_TASK in texts if root_model else child or ROOT_TASK in texts or continuation_parent or missing_agent))
    valid = valid and all(i.get('call_id') in ('call_vis39_parent', 'call_vis39_shell_0', 'call_vis39_shell_1', *(['call_vis39_continue','call_vis39_missing'] if cards else [])) for i in results)
    name = 'shell' if child or root_model else 'subagent'
    tool = not title and (index < 2 if child or root_model else index == 0)
    if handoff:
        name = 'shell' if child or index == 1 else 'subagent'
        tool = not title and (index == 0 if child else index < 2)
    if continuation_worker:
        tool = False
    elif continuation_parent or missing_agent:
        expected = 'call_vis39_continue' if continuation_parent else 'call_vis39_missing'
        tool = not title and not any(i.get('call_id') == expected for i in results)
    valid = valid and (not tool or name in definitions)
    emit({'kind': 'provider', 'operation': 'title' if title else 'child' if child else 'parent',
           'valid': valid, 'index': index, 'actual_results': results,
           'user_texts': texts, 'registered_tools': list(definitions),
           **({'controlled_phase':'missing-agent' if missing_agent else 'continuation-parent'
               if continuation_parent else 'continuation-child' if continuation_worker
               else 'original-child' if child else 'original-parent'} if cards else {})})
    if not valid:
        handler.send_error(400, 'VIS39 fixture contract rejected')
        return
    text = 'VIS39 parent' if title else 'VIS39-ROOT-SHELL-DONE' if root_model else 'VIS39-CHILD-DONE' if child else 'VIS39-PARENT-DONE'
    if handoff and not title and not child:
        text = 'VIS39-ROOT-HANDOFF-DONE'
    if cards and not title:
        if continuation_worker:
            # Hold a real ordinary child request, not the UI or its metadata.
            if not continuation_release.wait(60):
                raise AssertionError('Finite continuation response was not released')
            text = 'VIS39-CONTINUED-CHILD-DONE'
        elif continuation_parent:
            text = 'VIS39-CONTINUED-PARENT-DONE'
        elif missing_agent:
            text = 'VIS39-MISSING-AGENT-DONE'
    if tool:
        args = {'command': COMMANDS[index]} if name == 'shell' else {
            'agent': 'helper', 'description': 'Inspect child shell', 'prompt': CHILD_TASK}
        if cards and not child:
            if continuation_parent:
                if continuation_child is None:
                    raise AssertionError('No actual child was captured for continuation')
                args = {'agent':'helper','description':'Continue captured child',
                        'prompt':CONTINUE_CHILD_TASK,'sessionID':continuation_child}
            elif missing_agent:
                args = {'agent':'vis39_missing_agent','description':'Reject missing child profile',
                        'prompt':'Never launch a missing child'}
            else:
                args['model'] = 'fixture/fixture-model-1'
                args['description'] = 'Inspect captured child shell'
        item = {'id': f'fc_vis39_{number}', 'type': 'function_call', 'status': 'completed',
                'call_id': f'call_vis39_shell_{index}' if name == 'shell' else 'call_vis39_parent',
                 'name': name, 'arguments': json.dumps(args)}
        if continuation_parent or missing_agent:
            item['call_id'] = 'call_vis39_continue' if continuation_parent else 'call_vis39_missing'
        if continuation_parent:
            continuation_call_issued = True
        emit({'kind': 'fixture_tool_call', 'name': name, 'arguments': args})
        if not child and not root_model:
            with lock:
                parent_call_issued = True
    else:
        item = {'id': f'msg_vis39_{number}', 'type': 'message', 'role': 'assistant',
                'status': 'completed', 'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
    response = {'id': f'resp_vis39_{number}', 'object': 'response', 'created_at': 1700000000,
                'model': body['model'], 'status': 'in_progress', 'output': [],
                'error': None, 'incomplete_details': None}
    events = [{'type': 'response.created', 'response': response},
              {'type': 'response.output_item.added', 'output_index': 0,
               'item': {**item, 'status': 'in_progress', **({'arguments': ''} if tool else {'content': []})}}]
    if tool:
        events.append({'type': 'response.function_call_arguments.delta', 'item_id': item['id'],
                       'output_index': 0, 'delta': item['arguments']})
    else:
        events += [{'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': 0,
                    'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
                   {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0,
                    'content_index': 0, 'delta': text}]
    events += [{'type': 'response.output_item.done', 'output_index': 0, 'item': item},
               {'type': 'response.completed', 'response': {**response, 'status': 'completed',
                'output': [item], 'usage': {'input_tokens': 1234, 'output_tokens': 64, 'total_tokens': 1298}}}]
    payload = ''.join('event: ' + e['type'] + '\ndata: ' + json.dumps({**e, 'sequence_number': n}) + '\n\n'
                      for n, e in enumerate(events)).encode()
    handler.send_response(200)
    handler.send_header('Content-Type', 'text/event-stream')
    handler.send_header('Content-Length', str(len(payload)))
    handler.end_headers()
    handler.wfile.write(payload)
    handler.wfile.flush()
    emit({'kind': 'provider_completed', 'operation': 'title' if title else 'child' if child else 'parent'})
