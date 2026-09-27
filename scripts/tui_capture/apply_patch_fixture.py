"""VIS35 provider emits calls only; binaries own all file mutations/results."""
import hashlib
import json
import os
import sqlite3
import time
import compaction_fixture

SEED = ''.join(f'line-{n:02}\n' for n in range(1, 25))
LONG = 'const longValue = "' + 'fixture word ' * 14 + '";'
PATCHES = {
    'create': '*** Add File: created.ts\n+const answer = 42;\n+' + LONG,
    'empty': '*** Add File: empty.txt',
    'multihunk': '*** Update File: update.txt\n@@\n-line-08\n+LINE-EIGHT\n@@\n-line-20\n+LINE-TWENTY',
    'delete': '*** Delete File: delete.txt',
    'move': '*** Update File: move.txt\n*** Move to: moved.txt\n@@\n-old move\n+new move',
    'replace': '*** Update File: replace.txt\n@@\n-old alpha\n-old beta\n+replacement',
    'multifile': '*** Add File: multi-a.txt\n+alpha\n*** Add File: multi-b.txt\n+beta',
    'partial': '*** Add File: prefix.txt\n+confirmed prefix\n*** Update File: stale.txt\n@@\n-not present\n+never applied',
    'stale': '*** Update File: stale.txt\n@@\n-not present\n+never applied',
    'denied': '*** Update File: denied.txt\n@@\n-denied sentinel\n+must not appear',
}
FILES = ['created.ts','empty.txt','update.txt','delete.txt','move.txt','moved.txt','replace.txt','multi-a.txt','multi-b.txt','prefix.txt','stale.txt','denied.txt']
index = 0

def configure(spec, home, project, config, cli):
    # Both sides share cwd geometry but have independent fresh filesystem bytes.
    from pathlib import Path
    root = Path(spec['isolated_root']).resolve()
    if project.resolve().parent != root or not home.resolve().is_relative_to(root) or root.stat().st_uid != os.getuid() or project.is_symlink():
        raise RuntimeError('VIS35 requires owned isolated fixture directory')
    project.mkdir(parents=True, exist_ok=True)
    for name in FILES:
        if (project/name).is_file():
            (project/name).unlink()
    for name, text in {'update.txt':SEED,'delete.txt':'gone one\ngone two\n','move.txt':'old move\n','replace.txt':'old alpha\nold beta\n','stale.txt':'actual stale\n','denied.txt':'denied sentinel\n'}.items():
        (project/name).write_text(text)
        (project/name).chmod(0o751 if name=='move.txt' else 0o640)
    config['snapshots'] = False
    config['compaction'] = {'auto':False}
    cli['session']['tps'] = False
    if spec['origin']=='upstream':
        plugin = home/'config/opencode/vis35-admission'
        plugin.mkdir()
        source = (__import__('pathlib').Path(__file__).parent/'apply_patch_admission.mjs').read_text()
        (plugin/'index.js').write_text(source)
        (plugin/'package.json').write_text(json.dumps({'type':'module','main':'index.js'}))
        config['plugins'].append(str(plugin))
        config['permissions'] = [{'action':'*','resource':'*','effect':'deny'}, *[{'action':'edit','resource':name,'effect':'allow'} for name in FILES if name!='denied.txt']]
    else:
        config['permissions'] = {'*':'deny','apply_patch':{'*':'deny',**{name:'allow' for name in FILES if name!='denied.txt'}}}
        config['animations'] = False
    if spec.get('patch_view'):
        cli['diffs'] = {'view':spec['patch_view'],'wrap':spec.get('patch_wrap','word')}

def snapshot(home, project):
    value = compaction_fixture.snapshot(home, project)
    for observation in value['observations']:
        with sqlite3.connect(__import__('pathlib').Path(observation['database']).as_uri()+'?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            for table in ('patch_effects','conversation_tools','conversation_turns'):
                if table in observation['tables']:
                    observation['data'][table] = [dict(row) for row in connection.execute(f'SELECT * FROM "{table}" LIMIT 100')]
    value['files'] = {name:({'present':True,'bytes_hex':(project/name).read_bytes().hex(),'mode':oct((project/name).stat().st_mode & 0o777),'sha256':hashlib.sha256((project/name).read_bytes()).hexdigest()} if (project/name).is_file() else {'present':False}) for name in FILES}
    hook = home/'vis35-hook.jsonl'
    value['executor_hook'] = [json.loads(line) for line in hook.read_text().splitlines()] if hook.exists() else []
    return value

def respond(handler, body, spec, emit):
    global index
    index += 1
    items = body.get('input',[])
    system = body.get('instructions','') + compaction_fixture.texts([x for x in items if x.get('role') in ('system','developer')])
    title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
    user = compaction_fixture.texts([x for x in items if x.get('role')=='user'][-1:])
    case = next((name for name in PATCHES if f'VIS35 {name}:' in user),None)
    definitions = {x.get('name'):x for x in body.get('tools',[])}
    name = 'apply_patch' if spec['origin']=='oc' else 'patch'
    call_id = 'call_vis35_'+str(case)
    results = [x for x in items if x.get('type')=='function_call_output' and x.get('call_id')==call_id]
    tool = not title and case is not None and not results
    valid = handler.path=='/v1/responses' and body.get('stream') is True and body.get('model')=='fixture-model-1' and index<=32 and (title or case is not None)
    if tool:
        valid = valid and name in definitions and definitions[name].get('type')=='function' and definitions[name].get('parameters',{}).get('properties',{}).get('patchText',{}).get('type')=='string'
    emit({'kind':'provider','operation':'title' if title else 'patch','case':case,'index':index,'valid':valid,'request':body,'actual_results':results})
    if not valid:
        handler.send_error(400,'VIS35 contract rejected');return
    text = 'VIS35 patch fixture' if title else f'VIS35-DONE-{case}'
    arguments = {'patchText':'*** Begin Patch\n'+PATCHES[case]+'\n*** End Patch'} if tool else None
    item = {'id':f'fc_vis35_{index}','type':'function_call','status':'completed','call_id':call_id,'name':name,'arguments':json.dumps(arguments)} if tool else {'id':f'msg_vis35_{index}','type':'message','role':'assistant','status':'completed','content':[{'type':'output_text','text':text,'annotations':[]}]}
    response = {'id':f'resp_vis35_{index}','object':'response','created_at':1700000000,'model':body['model'],'status':'in_progress','output':[],'error':None,'incomplete_details':None}
    handler.send_response(200);handler.send_header('Content-Type','text/event-stream');handler.end_headers()
    sequence=0
    def event(value):
        nonlocal sequence
        handler.wfile.write(('event: '+value['type']+'\ndata: '+json.dumps({**value,'sequence_number':sequence})+'\n\n').encode());handler.wfile.flush();sequence+=1
    try:
        event({'type':'response.created','response':response})
        event({'type':'response.output_item.added','output_index':0,'item':{**item,'status':'in_progress',**({'arguments':''} if tool else {'content':[]})}})
        if tool:
            emit({'kind':'fixture_tool_call','case':case,'name':name,'arguments':arguments,'call_id':call_id})
            # Hold the actual argument stream, not a fabricated execution result.
            event({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':0,'delta':item['arguments'][:16]})
            emit({'kind':'fixture_argument_delta','case':case,'call_id':call_id,'item_id':item['id'],'delta':item['arguments'][:16],'phase':'pending','monotonic_ns':time.monotonic_ns()})
            time.sleep(8)
            event({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':0,'delta':item['arguments'][16:]})
            emit({'kind':'fixture_argument_delta','case':case,'call_id':call_id,'item_id':item['id'],'delta':item['arguments'][16:],'phase':'finish','monotonic_ns':time.monotonic_ns()})
        else:
            event({'type':'response.content_part.added','item_id':item['id'],'output_index':0,'content_index':0,'part':{'type':'output_text','text':'','annotations':[]}})
            event({'type':'response.output_text.delta','item_id':item['id'],'output_index':0,'content_index':0,'delta':text})
            event({'type':'response.output_text.done','item_id':item['id'],'output_index':0,'content_index':0,'text':text})
            event({'type':'response.content_part.done','item_id':item['id'],'output_index':0,'content_index':0,'part':item['content'][0]})
        event({'type':'response.output_item.done','output_index':0,'item':item})
        event({'type':'response.completed','response':{**response,'status':'completed','output':[item],'usage':{'input_tokens':1234,'output_tokens':64,'total_tokens':1298}}})
        emit({'kind':'provider_completed','operation':'title' if title else 'patch','case':case,'index':index})
    except (BrokenPipeError,ConnectionResetError):
        emit({'kind':'provider_disconnected','index':index})
