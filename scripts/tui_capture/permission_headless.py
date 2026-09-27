#!/usr/bin/env python3
"""Offline actual binary checks: headless ignores TUI auto config; CLI auto is Once."""
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import threading
import apply_patch_fixture

output = Path(sys.argv[1]).resolve()
output.mkdir(exist_ok=False)
binary = Path(sys.argv[2]).resolve()
root = Path('/home/opencode/.cache/opencode-tmp/opencode') / output.name
root.mkdir(exist_ok=False)
report = {'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'live_requests':0,'cases':[]}
for case, auto, deny in [('configauto-no-consumer',False,False),('cli-auto-once',True,False),('cli-auto-deny',True,True)]:
    home = root/case/'home'
    project = root/case/'project'
    data = root/case/'data'
    config = home/'config/opencode'
    config.mkdir(parents=True)
    project.mkdir()
    events = []
    apply_patch_fixture.index = 0
    spec = {'origin':'oc','permission':True,'permission_patches':{'shell':''},'permission_calls':{'shell':{'name':'bash','arguments':{'argv':['/usr/bin/touch','headless-marker']}}}}
    class Provider(BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            apply_patch_fixture.respond(self,body,spec,events.append)
    server = ThreadingHTTPServer(('127.0.0.1',0),Provider)
    thread = threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    (config/'opencode.json').write_text(json.dumps({'model':'fixture/fixture-model-1','provider':{'fixture':{'npm':'@ai-sdk/openai','options':{'baseURL':f'http://127.0.0.1:{server.server_port}/v1','apiKey':'fixture-not-secret'},'models':{'fixture-model-1':{'limit':{'context':65536,'output':4096}}}}},'permission':{'bash':'deny' if deny else 'ask'},'agent':{'title':{'disable':True}}}))
    (config/'cli.json').write_text('{"session":{"permissions":"autoaccept"}}')
    argv = [str(binary),'--data-dir',str(data),'run','VIS36 shell: invoke ordinary shell function','--json',*(['--auto'] if auto else [])]
    env = {'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),'PATH':'/usr/bin:/bin','OC_TEST_ALLOW_LOOPBACK':'1','LANG':'C.UTF-8'}
    try:
        completed = subprocess.run(argv,cwd=project,env=env,capture_output=True,timeout=20)
        exit_code = completed.returncode
        stdout,stderr = completed.stdout,completed.stderr
    except subprocess.TimeoutExpired as error:
        exit_code = 'TIMEOUT'
        stdout,stderr = error.stdout or b'',error.stderr or b''
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    (output/(case+'.stdout')).write_bytes(stdout)
    (output/(case+'.stderr')).write_bytes(stderr)
    (output/(case+'.protocol.json')).write_text(json.dumps(events,indent=2)+'\n')
    with sqlite3.connect((data/'oc.sqlite').as_uri()+'?mode=ro',uri=True) as connection:
        connection.row_factory = sqlite3.Row
        grants = [dict(row) for row in connection.execute('SELECT * FROM permission_grants')]
        operations = [dict(row) for row in connection.execute('SELECT * FROM tool_operations')]
    effect = (project/'headless-marker').exists()
    counts = {'provider_requests':sum(e['kind']=='provider' for e in events),'provider_completed':sum(e['kind']=='provider_completed' for e in events),'tool_calls':sum(e['kind']=='fixture_tool_call' for e in events),'invalid_requests':sum(e['kind']=='provider' and not e['valid'] for e in events)}
    passed = effect == (auto and not deny) and not grants and (auto or b'approval' in stderr) and exit_code != 'TIMEOUT'
    report['cases'].append({'case':case,'argv':argv,'exit_code':exit_code,'effect':effect,'grants':grants,'operations':operations,'counts':counts,'pass':passed})
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
sys.exit(0 if all(case['pass'] for case in report['cases']) else 1)
