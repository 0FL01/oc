"""One explicit VIS12 submit after real PTY click/edit; no tool outcomes."""
import json

EDITED = 'Проведи RECXON, жду план'
TITLE = 'VIS prompt caret fixture'

def respond(handler, body, spec, emit, title, number):
    users = []
    for item in body.get('input', []):
        if item.get('role') != 'user':
            continue
        content = item.get('content', [])
        users.append(content if isinstance(content, str) else ''.join(
            p.get('text', '') for p in content if p.get('type') in ('input_text', 'text')))
    valid = handler.path == '/v1/responses' and body.get('stream') is True and number <= 2
    valid = valid and body.get('model') == 'fixture-model-1' and (title or users == [EDITED])
    emit({'kind':'provider', 'operation':'prompt_caret_title' if title else 'prompt_caret_submit',
          'valid':valid, 'user_texts':users})
    if not valid:
        handler.send_error(400, 'VIS12 exact edited prompt rejected')
        return
    text = TITLE if title else 'VIS-CARET-DONE: exact edited prompt accepted.'
    emit_message(handler, body, text, number)
    emit({'kind':'provider_completed', 'operation':'prompt_caret_title' if title else 'prompt_caret_submit'})

def emit_message(handler, body, text, number):
    item = {'id':f'msg_caret_{number}', 'type':'message', 'role':'assistant', 'status':'completed',
            'content':[{'type':'output_text', 'text':text, 'annotations':[]}]}
    response = {'id':f'resp_caret_{number}', 'object':'response', 'created_at':1700000000,
                'model':body['model'], 'status':'in_progress', 'output':[], 'error':None, 'incomplete_details':None}
    events = [
        {'type':'response.created', 'response':response},
        {'type':'response.output_item.added', 'output_index':0, 'item':{**item,'status':'in_progress','content':[]}},
        {'type':'response.content_part.added', 'item_id':item['id'], 'output_index':0, 'content_index':0,
         'part':{'type':'output_text','text':'','annotations':[]}},
        {'type':'response.output_text.delta', 'item_id':item['id'], 'output_index':0, 'content_index':0, 'delta':text},
        {'type':'response.output_text.done', 'item_id':item['id'], 'output_index':0, 'content_index':0, 'text':text},
        {'type':'response.content_part.done', 'item_id':item['id'], 'output_index':0, 'content_index':0, 'part':item['content'][0]},
        {'type':'response.output_item.done', 'output_index':0, 'item':item},
        {'type':'response.completed', 'response':{**response,'status':'completed','output':[item],
         'usage':{'input_tokens':1234,'output_tokens':64,'total_tokens':1298}}},
    ]
    payload = ''.join('event: '+event['type']+'\ndata: '+json.dumps({**event,'sequence_number':i})+'\n\n'
                      for i,event in enumerate(events)).encode()
    handler.send_response(200)
    handler.send_header('Content-Type','text/event-stream')
    handler.send_header('Content-Length', str(len(payload)))
    handler.end_headers()
    handler.wfile.write(payload)
    handler.wfile.flush()
