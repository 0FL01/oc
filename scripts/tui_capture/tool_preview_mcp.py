"""VIS16/17 isolated real MCP peer; literal markers are payload, not notices."""
import json
import os
from pathlib import Path
import sys

for line in sys.stdin:
    request = json.loads(line)
    method = request.get('method')
    if 'id' not in request:
        continue
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25', 'capabilities':{'tools':{}},
                  'serverInfo':{'name':'vis16', 'version':'fixture'}}
    elif method == 'tools/list':
        result = {'tools':[{'name':'output', 'description':'Isolated bounded output fixture',
                           'inputSchema':{'type':'object', 'properties':{
                               'index':{'type':'integer'}, 'query':{'type':'string'},
                               'nested':{'type':'object'}}, 'required':['index','query','nested']}}]}
    elif method == 'tools/call':
        args = request['params']['arguments']
        with (Path(os.environ['HOME']) / 'vis16-mcp.jsonl').open('a') as log:
            log.write(json.dumps({'method':method, 'arguments':args}) + '\n')
        number = args['index']
        label = 'FIRST' if number == 1 else 'SECOND'
        text = ('VIS-MCP-ERROR: genuine producer failure' if number == 4 else
                f'VIS-MCP-{label}-BEGIN\n[Part preview truncated]\n'
                '[output preview truncated; full result retained]\n[truncated]\n' +
                ''.join(f'VIS-MCP-{label}-LINE-{n:03} actual output words\n' for n in range(1, 121)) +
                f'VIS-MCP-{label}-DISTANT-END\n')
        result = {'content':[{'type':'text','text':text}], 'isError':number == 4}
    else:
        result = {}
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}), flush=True)
