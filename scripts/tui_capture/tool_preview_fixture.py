"""VIS16/17 ordinary provider calls; binaries own execution and output preparation."""
import hashlib
import contextlib
import json
from pathlib import Path
import sqlite3
import threading
import time
import prompt_caret_fixture
import prompt_history_fixture
import user_shell_fixture
import composer_fixture
import syntax_fixture
import question_fixture
import execute_fixture

lock = threading.Lock()
requests = 0
small_submitted_ns = None
small_submitted_wall_ns = None
SMALL_ELAPSED_MS = 5500
SMALL_PRINTF = "printf 'VIS\\nb\\nc\\nd\\ne\\nf\\ng\\nh\\ni\\nj\\n'"
SMALL_EFFECT = 'echo VIS-SHELL-EFFECT>>"$HOME/effect"'
SMALL_COMMAND = SMALL_PRINTF + ';' + SMALL_EFFECT
COMMAND = "printf 'VIS-SHELL-LINE-%03d\\n' " + ' '.join(str(n) for n in range(1, 81)) + "; printf '[Part preview truncated]\\n[output preview truncated; full result retained]\\n[truncated]\\n'"
PROMPT = 'VIS16 preview: execute the supplied MCP and Shell calls exactly once.'

def command(spec):
    if spec.get('tool_preview_small'):
        return SMALL_COMMAND
    return COMMAND + "; printf 'VIS-SHELL-EFFECT\\n' >> tool-preview-" + spec['origin'] + '.effects'

def mark_small_submission(at_ns):
    global small_submitted_ns, small_submitted_wall_ns
    with lock:
        if small_submitted_ns is not None:
            raise AssertionError('Small preview allows exactly one submission')
        small_submitted_ns = at_ns
        small_submitted_wall_ns = time.time_ns()

def wait_small_completion(spec, emit):
    with lock:
        submitted = small_submitted_ns
        submitted_wall = small_submitted_wall_ns
    if submitted is None:
        raise AssertionError('Missing actual PTY submission clock')
    admission_ns = 0
    basis = 'pty_submission'
    if spec['origin'] == 'upstream':
        # AssistantFooter uses the public user-message creation time, not PTY
        # input time. Read the actual projection; never rewrite its timestamps.
        created = []
        home = Path(spec['isolated_root']) / 'upstream' / 'home'
        for database in (home / 'data').rglob('*'):
            if database.suffix not in ('.db','.sqlite','.sqlite3'):
                continue
            with contextlib.closing(sqlite3.connect(database.as_uri()+'?mode=ro', uri=True)) as connection:
                if connection.execute("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='session_message')").fetchone()[0]:
                    created += [row[0] for row in connection.execute(
                        "SELECT time_created FROM session_message WHERE type='user' ORDER BY seq LIMIT 2")]
        if len(created) != 1:
            raise AssertionError('Expected one actual original user-message clock')
        admission_ns = created[0] * 1_000_000 - submitted_wall
        if abs(admission_ns) > 10_000_000_000:
            raise AssertionError('Original user admission exceeds bounded timing fixture')
        basis = 'original_user_time_created'
    deadline = submitted + admission_ns + SMALL_ELAPSED_MS * 1_000_000
    remaining = deadline - time.monotonic_ns()
    if remaining > 0:
        time.sleep(remaining / 1_000_000_000)
    emit({'kind':'fixture_real_elapsed_barrier', 'submitted_ns':submitted,
          'response_ns':time.monotonic_ns(), 'target_ms':SMALL_ELAPSED_MS,
          'deadline_ns':deadline, 'clock_basis':basis,
          'admission_after_submit_ms':admission_ns / 1_000_000})

def configure(spec, home, project, config, cli):
    if spec.get('execute_preview'):
        return execute_fixture.configure(spec, home, project, config, cli)
    (home / 'vis16-peer-phase.json').write_text(json.dumps({'phase':'healthy'}))
    config['snapshots'] = False
    config['compaction'] = {'auto':False}
    config['tool_output'] = {'max_lines':12, 'max_bytes':1024}
    cli['session']['tps'] = False
    if spec.get('cursor_temporal') and spec['origin'] == 'upstream':
        cli['cursor'] = {'style':'default'} if spec['cursor_temporal'] == 'default' else {
            'style':'block', 'blinking':spec['cursor_temporal'] == 'blink'}
    peer = Path(__file__).parent / 'tool_preview_mcp.py'
    if spec['origin'] == 'upstream':
        config['permissions'] = [{'action':'*','resource':'*','effect':'deny'},
                                 {'action':'vis16_output','resource':'*','effect':'allow'},
                                 {'action':'shell','resource':'*','effect':'allow'}]
        config['mcp'] = {'servers':{'vis16':{'type':'local','command':['/usr/bin/python3',str(peer)],'codemode':False}}}
    else:
        config['animations'] = False
        config['permissions'] = [{'action':'*','resource':'*','effect':'deny'},
                                 {'action':'vis16__output','resource':'*','effect':'allow'},
                                 {'action':'shell','resource':command(spec),'effect':'allow'}]
        config['mcp'] = {'servers':{'vis16':{'type':'local','command':['/usr/bin/python3',str(peer)],'codemode':False}}}
    if spec.get('tool_preview_small'):
        server = config['mcp']['servers'].pop('vis16')
        server['command'].append('--small')
        config['mcp']['servers']['vispreview'] = server
        config['permissions'][1]['action'] = 'vispreview__output' if spec['origin'] == 'oc' else 'vispreview_output'
        if spec['origin'] == 'upstream':
            # Pinned ShellParse authorizes each simple/redirected AST statement,
            # not the compound command string used by the native shell owner.
            config['permissions'][2:] = [
                {'action':'shell','resource':resource,'effect':'allow'}
                for resource in [SMALL_PRINTF, SMALL_EFFECT]]
    if spec.get('mcp_status'):
        servers = config['mcp']['servers']
        # A safe display key distinct from handshake/argv and inherited scalar
        # values; the ordinary tool-effect fixture keeps its existing key.
        servers['vishealthy'] = servers.pop('vis16')
        servers['visdisabled'] = {'type':'local','command':['/usr/bin/python3',str(peer)],
                                  'disabled':True}
        servers['visfailed'] = {'type':'local','command':['/usr/bin/python3',str(peer),'--failed']}
        if spec.get('mcp_footer'):
            for name, role in (('vishealthy','healthy-peer'),('visdisabled','disabled-peer'),('visfailed','failed-peer')):
                servers[name]['command'] += ['--probe-role', role]
        if spec.get('mcp_footer') == 'remap':
            cli['keybinds'] = {'leader':'ctrl+g','dialog.select.prev':'f2','dialog.select.next':'f3',
                'dialog.select.page_up':'alt+u','dialog.select.page_down':'alt+d',
                'dialog.select.home':'alt+h','dialog.select.end':'alt+e',
                'dialog.select.submit':'f4','dialog.mcp.toggle':'f6,<leader>t'}
    if spec.get('prompt_caret') or spec.get('prompt_history') or spec.get('user_shell'):
        config['mcp'] = {'servers':{}} if spec['origin'] == 'upstream' else {}
    if spec.get('user_shell'):
        user_shell_fixture.configure(spec, home, project, config)
    if spec.get('combined_composer'):
        composer_fixture.configure(spec, home, project, config, cli)
    if spec.get('syntax_inventory'):
        syntax_fixture.configure(spec, home, project, config)
    if spec.get('prompt_history'):
        (project / 'note.txt').write_text('HISTORY_FILE_CURRENT_CANARY')
        if spec['prompt_history'] == 'remap':
            cli['keybinds'] = {'leader':'ctrl+g','prompt.history.previous':'f2,<leader>p',
                                'prompt.history.next':'f3'}
    if spec.get('question_forms'):
        question_fixture.configure(spec, config)

def control(home, action, project=None, spec=None):
    if spec and spec.get('execute_preview'):
        return execute_fixture.control(action)
    if spec and spec.get('syntax_inventory'):
        return syntax_fixture.control(action)
    if spec and spec.get('combined_composer'):
        return composer_fixture.control(project, action, spec)
    if action not in ('fail', 'recover'):
        raise ValueError('Unknown owned Home MCP fixture action')
    phase = 'failed' if action == 'fail' else 'healthy'
    temporary = home / 'vis16-peer-phase.next'
    temporary.write_text(json.dumps({'phase':phase}))
    temporary.replace(home / 'vis16-peer-phase.json')
    return {'phase':phase}

def snapshot(home, project, spec):
    observations = []
    for database in (home / 'data').rglob('*'):
        if database.suffix not in ('.db','.sqlite','.sqlite3'):
            continue
        with sqlite3.connect(database.as_uri()+'?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            data = {}
            for table in ('tool_operations','tool_output_resources'):
                if table in tables:
                    data[table] = [dict(r) for r in connection.execute(f'SELECT * FROM "{table}" ORDER BY rowid LIMIT 10')]
            if 'events' in tables:
                data['presentation'] = [json.loads(r[0]) for r in connection.execute("SELECT payload FROM events WHERE kind='tool_output_presentation' ORDER BY seq LIMIT 10")]
            if spec.get('prompt_caret') and spec['origin'] == 'oc' and 'messages' in tables:
                data['prompt_user_messages'] = [dict(r) for r in connection.execute(
                    "SELECT id,session_id,seq,role,CASE WHEN length(CAST(text AS BLOB))<=2048 THEN text END AS text FROM messages WHERE role='user' ORDER BY seq LIMIT 2")]
            if (spec.get('prompt_history') or spec.get('user_shell')) and spec['origin'] == 'oc' and 'messages' in tables:
                data['prompt_user_messages'] = [dict(r) for r in connection.execute(
                    "SELECT id,session_id,seq,role,CASE WHEN length(CAST(text AS BLOB))<=2048 THEN text END AS text FROM messages WHERE role='user' ORDER BY rowid LIMIT 8")]
                row = connection.execute("SELECT CASE WHEN length(CAST(value AS BLOB))<=16384 THEN value END FROM prefs WHERE key='tui.prompt_history.v1'").fetchone()
                data['prompt_input_history'] = json.loads(row[0]) if row and row[0] else []
            if spec.get('user_shell') and spec['origin'] == 'oc' and 'turns' in tables:
                data['model_turns'] = connection.execute('SELECT COUNT(*) FROM turns').fetchone()[0]
            if spec.get('syntax_inventory') and spec['origin'] == 'oc' and 'turns' in tables:
                data['syntax_turns'] = [dict(row) for row in connection.execute(
                    'SELECT id,session_id,status FROM turns ORDER BY rowid LIMIT 4')]
            if spec.get('question_forms'):
                data['question_records'] = question_fixture.snapshot(connection, tables, spec['origin'])
            observations.append({'tables':sorted(tables), 'data':data})
    peer = home / 'vis16-mcp.jsonl'
    lifecycle = home / 'vis16-mcp-lifecycle.jsonl'
    artifacts = {}
    for file in (home / 'data').rglob('tool-output/*'):
        # Active capture parts are atomically renamed by their real owner. Their
        # live descriptor remains in the SQL snapshot; only frozen artifacts
        # belong in the immutable no-replay hash comparison.
        if file.suffix == '.part' or not file.is_file():
            continue
        data = file.read_bytes()
        artifacts[str(file.relative_to(home))] = {
            'bytes':len(data), 'sha256':hashlib.sha256(data).hexdigest()}
    effect = home / 'effect' if spec.get('tool_preview_small') else project / ('tool-preview-' + spec['origin'] + '.effects')
    effect_bytes = None
    if effect.exists():
        with effect.open('rb') as counter:
            effect_bytes = counter.read(1025)
    if effect_bytes is not None and len(effect_bytes) > 1024:
        raise AssertionError('Shell effect counter exceeded bounded fixture size')
    shell_effect = None if effect_bytes is None else {
        'bytes':len(effect_bytes), 'lines':len(effect_bytes.splitlines()),
        'sha256':hashlib.sha256(effect_bytes).hexdigest()}
    return {'observations':observations, 'artifacts':artifacts, 'shell_effect':shell_effect,
            **({'syntax':syntax_fixture.snapshot(home, project)} if spec.get('syntax_inventory') else {}),
            **({'composer':composer_fixture.snapshot(home, project, spec)} if spec.get('combined_composer') else {}),
            **({'user_shell_boundary':user_shell_fixture.boundary(project, spec)} if spec.get('user_shell') else {}),
            'mcp_lifecycle':[json.loads(line) for line in lifecycle.read_text().splitlines()] if lifecycle.exists() else [],
            'mcp_calls':[json.loads(line) for line in peer.read_text().splitlines()] if peer.exists() else []}

def respond(handler, body, spec, emit):
    if spec.get('execute_preview'):
        return execute_fixture.respond(handler, body, spec, emit)
    if spec.get('syntax_inventory'):
        return syntax_fixture.respond(handler, body, spec, emit)
    if spec.get('combined_composer'):
        return composer_fixture.respond(handler, body, spec, emit)
    global requests
    with lock:
        requests += 1
        number = requests
    items = body.get('input', [])
    small = spec.get('tool_preview_small', False)
    if small:
        # This fixture's title request has no tools; every main request must
        # retain both admitted tools. No classification of free LLM prose.
        title = not body.get('tools')
    else:
        system = str(body.get('instructions','')) + json.dumps([x for x in items if x.get('role') in ('system','developer')])
        title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
    if spec.get('question_forms'):
        return question_fixture.respond(handler, body, spec, emit, title, number)
    if spec.get('prompt_caret'):
        return prompt_caret_fixture.respond(handler, body, spec, emit, title, number)
    if spec.get('prompt_history'):
        return prompt_history_fixture.respond(handler, body, spec, emit, title, number)
    results = [x for x in items if x.get('type') == 'function_call_output']
    definitions = {x.get('name'):x for x in body.get('tools', [])}
    prefix = 'vispreview' if small else 'vis16'
    mcp = prefix + ('__output' if spec['origin'] == 'oc' else '_output')
    shell = 'shell' if 'shell' in definitions else 'bash'
    index = len(results)
    count = 3 if small else 4
    valid = handler.path == '/v1/responses' and body.get('stream') is True and body.get('model') == 'fixture-model-1' and number <= count + 2
    valid = valid and (title or (index <= count and mcp in definitions and shell in definitions))
    for i, result in enumerate(results):
        valid = valid and result.get('call_id') == f'call_vis16_{i}'
        output = str(result.get('output',''))
        valid = valid and ('VIS-MCP-' + ['FIRST','SECOND'][i] in output if i < 2 else ('VIS' if small else 'VIS-SHELL-LINE-080') in output if i == 2 else ('mcp tool reported failure' if spec['origin']=='oc' else 'VIS-MCP-ERROR') in output)
    emit({'kind':'provider', 'operation':'title' if title else 'tool_preview', 'valid':valid,
          'index':index, 'actual_results':results, 'registered_tools':list(definitions)})
    if not valid:
        handler.send_error(400,'VIS16 fixture contract rejected')
        return
    tool = not title and index < count
    if small and not title and not tool:
        wait_small_completion(spec, emit)
    text = 'VIS16 tool preview fixture' if title else 'VIS16-DONE: bounded results verified.'
    if tool:
        name = shell if index == 2 else mcp
        args = ({'command':command(spec)} if shell == 'shell' else {'argv':['/bin/sh','-c',command(spec)]}) if index == 2 else {
            'index':index+1, 'query':'native needle', 'nested':{'value':'available parameter'}}
        item = {'id':f'fc_vis16_{number}','type':'function_call','status':'completed',
                'call_id':f'call_vis16_{index}','name':name,'arguments':json.dumps(args)}
        emit({'kind':'fixture_tool_call','index':index,'name':name,'arguments':args})
    else:
        item = {'id':f'msg_vis16_{number}','type':'message','role':'assistant','status':'completed',
                'content':[{'type':'output_text','text':text,'annotations':[]}]}
    response = {'id':f'resp_vis16_{number}','object':'response','created_at':1700000000,
                'model':body['model'],'status':'in_progress','output':[],'error':None,'incomplete_details':None}
    events = [{'type':'response.created','response':response},
              {'type':'response.output_item.added','output_index':0,'item':{**item,'status':'in_progress',**({'arguments':''} if tool else {'content':[]})}}]
    if tool:
        events.append({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':0,'delta':item['arguments']})
    else:
        events.extend([{'type':'response.content_part.added','item_id':item['id'],'output_index':0,'content_index':0,'part':{'type':'output_text','text':'','annotations':[]}},
                       {'type':'response.output_text.delta','item_id':item['id'],'output_index':0,'content_index':0,'delta':text},
                       {'type':'response.output_text.done','item_id':item['id'],'output_index':0,'content_index':0,'text':text},
                       {'type':'response.content_part.done','item_id':item['id'],'output_index':0,'content_index':0,'part':item['content'][0]}])
    events.extend([{'type':'response.output_item.done','output_index':0,'item':item},
                   {'type':'response.completed','response':{**response,'status':'completed','output':[item],
                    'usage':{'input_tokens':1234,'output_tokens':64,'total_tokens':1298}}}])
    payload = ''.join('event: '+event['type']+'\ndata: '+json.dumps({**event,'sequence_number':i})+'\n\n' for i,event in enumerate(events)).encode()
    handler.send_response(200)
    handler.send_header('Content-Type','text/event-stream')
    handler.send_header('Content-Length',str(len(payload)))
    handler.end_headers()
    handler.wfile.write(payload)
    handler.wfile.flush()
    emit({'kind':'provider_completed','operation':'title' if title else 'tool_preview'})
