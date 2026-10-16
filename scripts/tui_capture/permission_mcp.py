"""Isolated JSONL MCP: actual tools/call counts; no network or filesystem tool effects."""
import json
import os
from pathlib import Path
import sys
log = Path(os.environ['HOME']) / 'vis36-mcp.jsonl'
for line in sys.stdin:
    request = json.loads(line)
    with log.open('a') as output:
        output.write(json.dumps(request) + '\n')
    if 'id' not in request: continue
    method = request.get('method')
    if method == 'initialize':
        result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'vis36','version':'fixture'}}
    elif method == 'tools/list':
        result = {'tools':[{'name':'echo','description':'Isolated permission echo','inputSchema':{'type':'object','properties':{'message':{'type':'string'}},'required':['message']}}]}
    elif method == 'tools/call':
        result = {'content':[{'type':'text','text':'VIS36-MCP-EFFECT ' + request['params']['arguments']['message']}]}
    else: result = {}
    print(json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}), flush=True)
