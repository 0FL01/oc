"""VIS16/17 ordinary provider calls; binaries own execution and output preparation."""
import hashlib
import json
from pathlib import Path
import sqlite3
import threading

lock = threading.Lock()
requests = 0
COMMAND = "printf 'VIS-SHELL-LINE-%03d\\n' " + ' '.join(str(n) for n in range(1, 81)) + "; printf '[Part preview truncated]\\n[output preview truncated; full result retained]\\n[truncated]\\n'"
PROMPT = 'VIS16 preview: execute the supplied MCP and Shell calls exactly once.'

def command(spec):
    return COMMAND + "; printf 'VIS-SHELL-EFFECT\\n' >> tool-preview-" + spec['origin'] + '.effects'

def configure(spec, home, project, config, cli):
    config['snapshots'] = False
    config['compaction'] = {'auto':False}
    config['tool_output'] = {'max_lines':12, 'max_bytes':1024}
    cli['session']['tps'] = False
    peer = Path(__file__).parent / 'tool_preview_mcp.py'
    if spec['origin'] == 'upstream':
        config['permissions'] = [{'action':'*','resource':'*','effect':'deny'},
                                 {'action':'vis16_output','resource':'*','effect':'allow'},
                                 {'action':'shell','resource':'*','effect':'allow'}]
        config['mcp'] = {'servers':{'vis16':{'type':'local','command':['/usr/bin/python3',str(peer)],'codemode':False}}}
    else:
        config['animations'] = False
        config['permissions'] = {'*':'deny', 'vis16__output':'allow', 'bash':{'*':'deny',command(spec):'allow'}}
        config['mcp'] = {'vis16':{'type':'local','command':['/usr/bin/python3',str(peer)],'enabled':True}}

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
            observations.append({'tables':sorted(tables), 'data':data})
    peer = home / 'vis16-mcp.jsonl'
    artifacts = {str(p.relative_to(home)): {'bytes':p.stat().st_size, 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
                 for p in (home / 'data').rglob('tool-output/*') if p.is_file()}
    effect = project / ('tool-preview-' + spec['origin'] + '.effects')
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
            'mcp_calls':[json.loads(line) for line in peer.read_text().splitlines()] if peer.exists() else []}

def respond(handler, body, spec, emit):
    global requests
    with lock:
        requests += 1
        number = requests
    items = body.get('input', [])
    system = str(body.get('instructions','')) + json.dumps([x for x in items if x.get('role') in ('system','developer')])
    title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
    results = [x for x in items if x.get('type') == 'function_call_output']
    definitions = {x.get('name'):x for x in body.get('tools', [])}
    mcp = 'vis16__output' if spec['origin'] == 'oc' else 'vis16_output'
    shell = 'shell' if 'shell' in definitions else 'bash'
    index = len(results)
    valid = handler.path == '/v1/responses' and body.get('stream') is True and body.get('model') == 'fixture-model-1' and number <= 6
    valid = valid and (title or (index <= 4 and mcp in definitions and shell in definitions))
    for i, result in enumerate(results):
        valid = valid and result.get('call_id') == f'call_vis16_{i}'
        output = str(result.get('output',''))
        valid = valid and ('VIS-MCP-' + ['FIRST','SECOND'][i] in output if i < 2 else 'VIS-SHELL-LINE-080' in output if i == 2 else ('mcp tool reported failure' if spec['origin']=='oc' else 'VIS-MCP-ERROR') in output)
    emit({'kind':'provider', 'operation':'title' if title else 'tool_preview', 'valid':valid,
          'index':index, 'actual_results':results, 'registered_tools':list(definitions)})
    if not valid:
        handler.send_error(400,'VIS16 fixture contract rejected')
        return
    tool = not title and index < 4
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
