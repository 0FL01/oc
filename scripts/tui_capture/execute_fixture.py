"""Scoped tool log: four direct tools, real Code Mode/search/MCP/fetch, failure."""
import json
from pathlib import Path
import threading

code_release = threading.Event()
fetch_release = threading.Event()
lock = threading.Lock()
requests = 0
main_requests = 0
fetches = 0
fetch_paths = []
DIRECT_IDS = ['call_vis46_read', 'call_vis46_grep', 'call_vis46_glob', 'call_vis46_webfetch']
SUCCESS_IDS = DIRECT_IDS + ['call_vis46_success']
WIDE = 'abcdefghijklmnopqrstuvwxyz' * 6
TITLE = 'VIS46 execute fixture'
PROMPT = 'VIS46 execute the supplied program exactly once.'
ERROR_PROMPT = 'VIS46 exercise the supplied failed program exactly once.'


def program(spec):
    url = f"http://localhost:{spec['execute_fixture_port']}/vis46-fetch"
    return ('// Actual program: Ω界\nconst discovered = search({query: "output", namespace: "visexecute", limit: 8});\n'
            'const first = await tools.visexecute.output({index: 2, query: "needle", nested: {value: "first"}});\n'
            '\nconst second = await tools.visexecute.output({index: 3, query: "other", nested: {value: "second"}});\n'
            f'\nconst response = await fetch("{url}");\n'
            'console.log("VIS46 actual log");\n'
            'return {\n  result: "VIS46-RESULT",\n  first: "completed",\n'
            '  second: "completed",\n  status: response.status,\n'
            f'  wide: "{WIDE}"\n' + '};')


ERROR_CODE = ('try {\n  await tools.visexecute.output({index: 4, query: "failure", nested: {value: "error"}});\n'
              '} catch (error) {\n  throw new Error("VIS46 ERROR bounded words demonstrate ordinary prose wrapping without splitting each word across the dialog edge.\\nline b\\nline c\\nline d\\nline e\\nline f");\n}')
OUTPUT = json.dumps({'result':'VIS46-RESULT','first':'completed','second':'completed','status':200,'wide':WIDE}, indent=2) + '\n\nLogs:\nVIS46 actual log'


def configure(spec, home, project, config, cli):
    peer = Path(__file__).parent / 'tool_preview_mcp.py'
    (home / 'vis16-peer-phase.json').write_text(json.dumps({'phase':'healthy'}))
    config['snapshots'] = False
    config['compaction'] = {'auto':False}
    config['tool_output'] = {'max_lines':100, 'max_bytes':8192}
    config['mcp'] = {'servers':{'visexecute':{'type':'local',
        'command':['/usr/bin/python3',str(peer),'--small'],'codemode':True}}}
    action = 'visexecute__output' if spec['origin'] == 'oc' else 'visexecute_output'
    (project / 'execute-note.txt').write_text('VIS46_MARKER\nCaptured exploration fixture.\n')
    if (project / 'execute-absent.txt').exists():
        raise AssertionError('Execute fixture requires a genuinely absent Read target')
    config['permissions'] = [{'action':'*','resource':'*','effect':'deny'},
        {'action':'execute','resource':'*','effect':'allow'},
        {'action':action,'resource':'*','effect':'allow'},
        *[{'action':name,'resource':'*','effect':'allow'} for name in ('read','grep','glob')],
        *[{'action':'webfetch','resource':f"http://localhost:{spec['execute_fixture_port']}/{path}",'effect':'allow'}
          for path in ('vis46-direct','vis46-fetch')]]
    cli['session']['tps'] = False
    if spec['origin'] == 'oc':
        config['animations'] = False
        cli['terminal'] = {'copy':'osc52'}


def control(action):
    if action == 'release-code':
        code_release.set()
    elif action == 'release-fetch':
        fetch_release.set()
    else:
        raise ValueError('Unknown execute fixture control')
    return {'action':action}


def fetch(handler, emit):
    global fetches
    with lock:
        fetches += 1
        valid = (fetches <= 2 and handler.path in ('/vis46-direct', '/vis46-fetch')
                 and handler.path not in fetch_paths)
        fetch_paths.append(handler.path)
    emit({'kind':'execute_fetch','valid':valid,'path':handler.path})
    if not valid:
        handler.send_error(400, 'Unexpected execute fetch')
        return
    if handler.path == '/vis46-fetch' and not fetch_release.wait(30):
        handler.send_error(504, 'Bounded execute fetch hold expired')
        return
    payload = b'VIS46 DIRECT OK\n' if handler.path == '/vis46-direct' else b'VIS46 FETCH OK\n'
    handler.send_response(200)
    handler.send_header('Content-Type','text/plain')
    handler.send_header('Content-Length',str(len(payload)))
    handler.end_headers()
    handler.wfile.write(payload)


def respond(handler, body, spec, emit):
    global requests, main_requests
    title = not body.get('tools')
    with lock:
        requests += 1
        if not title:
            main_requests += 1
        number, main = requests, main_requests
    results = [item for item in body.get('input',[]) if item.get('type') == 'function_call_output']
    result_ids = [item.get('call_id') for item in results]
    definitions = {item.get('name') for item in body.get('tools',[])}
    valid = (handler.path == '/v1/responses' and body.get('stream') is True and
             body.get('model') == 'fixture-model-1' and number <= 5 and
             (title or ({'read','grep','glob','webfetch','execute'} <= definitions and main <= 4)))
    if not title and main == 2:
        valid = valid and sorted(result_ids) == sorted(SUCCESS_IDS)
        valid = valid and next((item.get('output') for item in results if item.get('call_id') == 'call_vis46_success'), None) == OUTPUT
    if not title and main == 4:
        valid = valid and sorted(result_ids) == sorted(SUCCESS_IDS + ['call_vis46_error'])
    emit({'kind':'provider','operation':'title' if title else 'execute_preview',
          'valid':valid,'index':main,'registered_tools':sorted(definitions),'result_ids':result_ids})
    if not valid:
        handler.send_error(400,'VIS46 fixture request contract rejected')
        return
    tool = not title and main in (1,3)
    text = TITLE if title else 'VIS46-DONE' if main == 2 else 'VIS46-ERROR-DONE'
    code = program(spec) if main == 1 else ERROR_CODE
    calls = (list(zip(DIRECT_IDS, ('read','grep','glob','webfetch'), (
        {'path':'execute-absent.txt'}, {'pattern':'VIS46_MARKER','path':'.'},
        {'pattern':'execute-note.txt','path':'.'},
        {'url':f"http://localhost:{spec['execute_fixture_port']}/vis46-direct"})))
        + [('call_vis46_success','execute',{'code':code})]) if tool and main == 1 else (
        [('call_vis46_error','execute',{'code':code})] if tool else [])
    output = [{'id':'fc_'+call_id,'type':'function_call','status':'completed',
               'call_id':call_id,'name':name,'arguments':json.dumps(arguments)}
              for call_id, name, arguments in calls]
    if not tool:
        output = [{'id':f'msg_vis46_{number}','type':'message','role':'assistant','status':'completed',
                   'content':[{'type':'output_text','text':text,'annotations':[]}]}]
    if tool:
        for call_id, name, arguments in calls:
            emit({'kind':'fixture_tool_call','index':main,'call_id':call_id,'name':name,'arguments':arguments})
        if main == 1:
            emit({'kind':'execute_expected_output','text':OUTPUT})
    response = {'id':f'resp_vis46_{number}','object':'response','created_at':1700000000,
                'model':body['model'],'status':'in_progress','output':[],'error':None,'incomplete_details':None}
    handler.send_response(200)
    handler.send_header('Content-Type','text/event-stream')
    handler.end_headers()
    sequence = 0
    def event(value):
        nonlocal sequence
        handler.wfile.write(('event: '+value['type']+'\ndata: '+json.dumps({**value,'sequence_number':sequence})+'\n\n').encode())
        handler.wfile.flush()
        sequence += 1
    event({'type':'response.created','response':response})
    for index, item in enumerate(output):
        event({'type':'response.output_item.added','output_index':index,
               'item':{**item,'status':'in_progress',**({'arguments':''} if tool else {'content':[]})}})
        if tool:
            arguments = item['arguments']
            if item['call_id'] == 'call_vis46_success':
                event({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':index,'delta':arguments[:8]})
                emit({'kind':'execute_receiving_held'})
                if not code_release.wait(30):
                    raise AssertionError('Bounded execute receiving hold expired')
                arguments = arguments[8:]
            event({'type':'response.function_call_arguments.delta','item_id':item['id'],'output_index':index,'delta':arguments})
        else:
            event({'type':'response.content_part.added','item_id':item['id'],'output_index':index,'content_index':0,
                   'part':{'type':'output_text','text':'','annotations':[]}})
            event({'type':'response.output_text.delta','item_id':item['id'],'output_index':index,'content_index':0,'delta':text})
            event({'type':'response.output_text.done','item_id':item['id'],'output_index':index,'content_index':0,'text':text})
            event({'type':'response.content_part.done','item_id':item['id'],'output_index':index,'content_index':0,'part':item['content'][0]})
        event({'type':'response.output_item.done','output_index':index,'item':item})
    event({'type':'response.completed','response':{**response,'status':'completed','output':output,
           'usage':{'input_tokens':1234,'output_tokens':64,'total_tokens':1298}}})
    emit({'kind':'provider_completed','operation':'title' if title else 'execute_preview'})
