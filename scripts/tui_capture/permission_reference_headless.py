#!/usr/bin/env python3
"""Bounded, env-cleared pinned-original headless counterpart; no real provider."""
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import apply_patch_fixture
import permission_fixture

output = Path(sys.argv[1]).resolve()
output.mkdir(exist_ok=False)
binary = Path(sys.argv[2]).resolve()
root = Path('/home/opencode/.cache/opencode-tmp/opencode') / output.name
root.mkdir(exist_ok=False)
report = {'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
          'live_requests':0,'scope':'Actual pinned original CLI; paired native is supplied separately.', 'cases':[]}
for case, auto, deny in [('configauto-no-consumer',False,False),('cli-auto-once',True,False),('cli-auto-deny',True,True)]:
    home, project = root/case/'home', root/case/'project'
    config = home/'config/opencode'
    config.mkdir(parents=True)
    project.mkdir()
    events = []
    apply_patch_fixture.index = 0
    spec = {'origin':'upstream','permission':True,'permission_patches':{'shell':''},
            'permission_calls':{'shell':{'name':'shell','arguments':{'command':'/usr/bin/touch headless-marker'}}}}
    class Provider(BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            apply_patch_fixture.respond(self,body,spec,events.append)
    server = ThreadingHTTPServer(('127.0.0.1',0),Provider)
    thread = threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    settings = {'model':'fixture/fixture-model-1','share':'disabled','update':'disable',
        'plugins':['-opencode.models.dev'], 'providers':{'fixture':{
            'package':'@opencode/ai/providers/openai/responses',
            'settings':{'baseURL':f'http://127.0.0.1:{server.server_port}/v1','apiKey':'fixture-not-secret'},
            'models':{'fixture-model-1':{'limit':{'context':65536,'output':4096}}}}},
        'permissions':[{'action':'*','resource':'*','effect':'ask'},
                       {'action':'shell','resource':'/usr/bin/touch headless-marker' if deny else '*','effect':'deny' if deny else 'ask'}]}
    (config/'opencode.json').write_text(json.dumps(settings))
    (config/'cli.json').write_text('{"session":{"permissions":"autoaccept"}}')
    argv = [str(binary),'run','--standalone','--format','json',*(['--auto'] if auto else []),
            'VIS36 shell: invoke ordinary shell function']
    env = {'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),
        'XDG_DATA_HOME':str(home/'data'),'XDG_CACHE_HOME':str(home/'cache'),'XDG_STATE_HOME':str(home/'state'),
        'PATH':'/usr/bin:/bin','LANG':'C.UTF-8','OPENCODE_TEST_HOME':str(home),
        'OPENCODE_DISABLE_AUTOUPDATE':'1','OPENCODE_DISABLE_MODELS_FETCH':'true',
        'OPENCODE_CONFIG_PROJECT_DISABLE':'true','OPENCODE_CONFIG_CONTENT':json.dumps(settings)}
    child = subprocess.Popen(argv,cwd=project,env=env,stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = child.communicate(timeout=25)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(child.pid,signal.SIGTERM)
        try:
            stdout, stderr = child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid,signal.SIGKILL)
            stdout, stderr = child.communicate()
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    (output/(case+'.stdout')).write_bytes(stdout)
    (output/(case+'.stderr')).write_bytes(stderr)
    (output/(case+'.protocol.json')).write_text(json.dumps(events,indent=2)+'\n')
    snapshot = permission_fixture.snapshot(home,project)
    (output/(case+'.snapshot.json')).write_text(json.dumps(snapshot,indent=2)+'\n')
    grants = [row for o in snapshot['observations'] for table,rows in o['data'].items()
              if 'permission' in table for row in rows]
    effect = (project/'headless-marker').exists()
    counts = {'provider_requests':sum(e['kind']=='provider' for e in events),
        'provider_completed':sum(e['kind']=='provider_completed' for e in events),
        'tool_calls':sum(e['kind']=='fixture_tool_call' for e in events),
        'invalid_requests':sum(e['kind']=='provider' and not e['valid'] for e in events)}
    results = [r for e in events if e['kind']=='provider' for r in e.get('actual_results',[])]
    report['cases'].append({'case':case,'argv':argv,'exit_code':child.returncode,
        'timed_out':timed_out,'effect':effect,'grants':grants,'counts':counts,'actual_results':results,
        'targeted_effect_pass':effect == (auto and not deny) and not grants and not timed_out and not counts['invalid_requests'] and counts['tool_calls']==1})
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
sys.exit(0 if all(c['targeted_effect_pass'] for c in report['cases']) else 1)
