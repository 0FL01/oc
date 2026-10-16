"""Opt-in VIS34 Responses fixture. Only actual requests and read-only DB observations."""
import hashlib
import json
import threading
import time
import sqlite3

PROMPTS = [f'VIS34 seed {i}: preserve requirement R{i}; no filesystem changes.' for i in range(1, 4)]
NEXT = 'VIS34 next: continue from checkpoint; no tools.'
RESTART = 'VIS34 restart: continue from persisted checkpoint; no tools.'
HOLD = 'VIS34 held tool: wait at safe boundary; no filesystem changes.'
SUMMARY = '## Objective\n- VIS34-CHECKPOINT: preserve R1, R2, R3.\n\n## Work State\n- Three seeded exchanges completed; filesystem unchanged.\n\n## Next Move\n1. Continue the user request without replaying tools.\n'
release = threading.Event()
behavior = 'complete'
index = 0
summary_index = 0
held_issued = False
overflow_issued = False

def texts(items):
    return '\n'.join(p.get('text', '') for x in items if isinstance(x, dict)
                     for p in x.get('content', []) if isinstance(p, dict))

def respond(handler, body, spec, emit):
    global index, summary_index, held_issued, overflow_issued
    index += 1
    items = body.get('input', [])
    instructions = body.get('instructions', '')
    system = instructions + texts([x for x in items if x.get('role') in ('system', 'developer')])
    user = texts([x for x in items if x.get('role') == 'user'])
    last_user = texts([x for x in items if x.get('role') == 'user'][-1:])
    title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
    # OC2 appends buildPrompt as a real user message; Rust uses a Developer
    # summarizer instruction and a User JSON transcript. Never classify tools
    # or a history quote containing "summary" as a summarizer request.
    summary = not title and (
        (not body.get('tools') and 'Summarize only what the user' in system) or
        ('Summarize only what the user' in last_user and '## Objective' in last_user))
    operation = 'title' if title else 'compaction' if summary else 'transcript'
    valid = handler.path == '/v1/responses' and body.get('stream') is True and body.get('model') == 'fixture-model-1' and index <= 24
    results = [x for x in items if x.get('type') == 'function_call_output']
    known = next((p for p in [RESTART, NEXT, HOLD, *reversed(PROMPTS)] if p in last_user), None)
    valid = valid and (title or summary or known is not None or bool(results))
    if summary:
        summary_index += 1
        valid = valid and 'VIS34' in user
    emit({'kind':'provider', 'operation':operation, 'valid':valid, 'index':index,
          'summary_index':summary_index if summary else None, 'request':body,
          'request_sha256':hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest(),
          'path':handler.path, 'roles':[x.get('role', x.get('type')) for x in items],
          'registered_tools':[x.get('name') for x in body.get('tools', [])]})
    if not valid:
        handler.send_error(400, 'VIS34 request contract'); return
    if spec.get('compaction_trigger') == 'overflow' and operation == 'transcript' and known == NEXT and not overflow_issued:
        overflow_issued = True
        handler.send_response(400); handler.send_header('Content-Type','application/json'); handler.end_headers()
        handler.wfile.write(json.dumps({'error':{'type':'invalid_request_error','code':'context_length_exceeded','message':'VIS34 context window exceeded'}}).encode())
        emit({'kind':'provider_overflow','operation':operation,'index':index}); return
    if summary and behavior == 'fail':
        handler.send_response(400); handler.send_header('Content-Type','application/json'); handler.end_headers()
        handler.wfile.write(json.dumps({'error':{'type':'invalid_request_error','code':'fixture_summary_failure','message':'VIS34 bounded summary failure'}}).encode())
        emit({'kind':'provider_failed','operation':operation,'summary_index':summary_index}); return
    text = 'VIS34 compaction fixture' if title else SUMMARY if summary else (
        'VIS34-RESTART-DONE' if known == RESTART else 'VIS34-NEXT-DONE' if known == NEXT else
        'VIS34-HELD-DONE' if known == HOLD or results else
        f'VIS34-ANSWER-{PROMPTS.index(known)+1}:\n' + ''.join(f'ARCHIVE-{PROMPTS.index(known)+1}-{n:03} requirement detail retained in raw history.\n' for n in range(120)))
    tool = not summary and not title and known == HOLD and not held_issued
    if tool:
        held_issued = True
        definitions = {x['name']:x for x in body.get('tools', [])}
        name = 'bash' if 'bash' in definitions else 'shell'
        schema = definitions[name].get('parameters', {})
        args = {'argv':['sleep','25']} if 'argv' in schema.get('properties', {}) else {'command':'sleep 25','description':'VIS34 bounded held tool'}
        item = {'id':f'fc_vis34_{index}','type':'function_call','status':'completed','call_id':'call_vis34_hold','name':name,'arguments':json.dumps(args)}
        emit({'kind':'fixture_tool_call','name':name,'arguments':args,'call_id':'call_vis34_hold'})
    else:
        item = {'id':f'msg_vis34_{index}','type':'message','role':'assistant','status':'completed','content':[{'type':'output_text','text':text,'annotations':[]}]}
    response = {'id':f'resp_vis34_{index}','object':'response','created_at':1700000000,'model':body['model'],'status':'in_progress','output':[],'error':None,'incomplete_details':None}
    handler.send_response(200); handler.send_header('Content-Type','text/event-stream'); handler.end_headers()
    sequence = 0
    def event(value):
        nonlocal sequence
        handler.wfile.write(('event: '+value['type']+'\ndata: '+json.dumps({**value,'sequence_number':sequence})+'\n\n').encode()); handler.wfile.flush(); sequence += 1
    try:
        event({'type':'response.created','response':response})
        event({'type':'response.output_item.added','output_index':0,'item':{**item,'status':'in_progress', **({'arguments':''} if tool else {'content':[]})}})
        if tool:
            event({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':0,'delta':item['arguments']})
        else:
            event({'type':'response.content_part.added','item_id':item['id'],'output_index':0,'content_index':0,'part':{'type':'output_text','text':'','annotations':[]}})
            chunks = [text] if not summary else [text[:80], text[80:]]
            for n, chunk in enumerate(chunks):
                event({'type':'response.output_text.delta','item_id':item['id'],'output_index':0,'content_index':0,'delta':chunk})
                if summary and n == 0:
                    release.clear(); emit({'kind':'compaction_stream_held','summary_index':summary_index,'behavior':behavior,'timeout_seconds':30})
                    release.wait(30)
            event({'type':'response.output_text.done','item_id':item['id'],'output_index':0,'content_index':0,'text':text})
            event({'type':'response.content_part.done','item_id':item['id'],'output_index':0,'content_index':0,'part':item['content'][0]})
        event({'type':'response.output_item.done','output_index':0,'item':item})
        input_tokens = 23000 if spec.get('compaction_trigger') == 'threshold' and known == PROMPTS[2] and not summary else 1234 if summary else 6000
        usage = {'input_tokens':input_tokens,'output_tokens':321 if summary else 1800,'total_tokens':input_tokens+(321 if summary else 1800),'input_tokens_details':{'cached_tokens':234 if summary else 0},'output_tokens_details':{'reasoning_tokens':21 if summary else 0}}
        event({'type':'response.completed','response':{**response,'status':'completed','output':[item],'usage':usage}})
        emit({'kind':'provider_completed','operation':operation,'index':index,'summary_index':summary_index if summary else None,'usage':usage})
    except (BrokenPipeError, ConnectionResetError):
        emit({'kind':'provider_disconnected','operation':operation})

def snapshot(home, project):
    observations = []
    for db in (home / 'data').rglob('*'):
        if db.suffix not in ('.db','.sqlite','.sqlite3'): continue
        with sqlite3.connect(db.as_uri()+'?mode=ro', uri=True) as conn:
            conn.row_factory = sqlite3.Row
            tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")]
            wanted = ('messages','turns','tool_operations','session_checkpoint','session_compactions','session_message','session_provider_context','session_inbox','session_compaction','session_context')
            data = {t:[dict(r) for r in conn.execute(f'SELECT * FROM "{t}" LIMIT 100')] for t in wanted if t in tables}
            observations.append({'database':str(db),'tables':tables,'data':data})
    return {'observations':observations,'project_files':{str(p.relative_to(project)):hashlib.sha256(p.read_bytes()).hexdigest() for p in project.rglob('*') if p.is_file()}}
