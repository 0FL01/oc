#!/usr/bin/env python3
"""Direct normal ELF, isolated home, owned HTTP/stdio MCP and bounded native TUI."""
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import traceback

ROOT = Path(__file__).resolve().parents[2]
BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')
SCHEMA = {'type': 'object', 'properties': {'value': {'type': 'string'}},
          'required': ['value'], 'additionalProperties': False}
MCP_NAMES = ('search','rotate_widget')
WIRE_NAMES = tuple(server+'__'+tool for server in ('http','stdio') for tool in MCP_NAMES)
TOOLS = [{'name': name, 'description': 'search fixture' if name=='search' else 'arbitrary mutating-name fixture', 'inputSchema': SCHEMA} for name in MCP_NAMES]
NATIVE = ('read', 'glob', 'grep', 'edit', 'write', 'apply_patch', 'bash', 'subagent',
          'question', 'opencode_session_rename', 'opencode_session_move')

def mcp_reply(request, effect):
    method = request['method']
    if method == 'initialize':
        return {'protocolVersion': request['params']['protocolVersion'], 'capabilities': {'tools': {}},
                'serverInfo': {'name': 'owned-builtin-fixture', 'version': '1'}, 'instructions': 'OWNED_MCP_GUIDANCE'}
    if method == 'tools/list':
        return {'tools': TOOLS}
    if method == 'tools/call':
        assert request['params']['name'] in MCP_NAMES
        effect(dict(request['params']['arguments'],tool=request['params']['name']))
        return {'content': [{'type': 'text', 'text': 'OWNED_MCP_RESULT'}]}
    raise AssertionError(method)

def stdio(effect_path, gate=None):
    Path(effect_path+'.pid').write_text(str(os.getpid()))
    def effect(args):
        with open(effect_path, 'a') as out:
            out.write(json.dumps(args)+'\n')
    for line in sys.stdin:
        request = json.loads(line)
        with open(effect_path+'.methods','a') as out:
            out.write(request['method']+'\n')
        if 'id' not in request:
            continue
        if gate and request['method']=='initialize':
            wait(lambda: Path(gate).exists(),10)
        result = mcp_reply(request, effect)
        print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)

def event(value):
    return ('data: '+json.dumps(value)+'\n\n').encode()

def done(text):
    return event({'type':'response.output_text.delta','delta':text}) + event({
        'type':'response.completed','response':{'output':[{
            'type':'message','role':'assistant','content':[{'type':'output_text','text':text}]}]}})

def call(index, name, arguments):
    item = {'type':'function_call','id':'fc_'+str(index),'call_id':'c'+str(index),
            'name':name,'arguments':json.dumps(arguments),'status':'completed'}
    return event({'type':'response.output_item.done','output_index':0,'item':item}) + event({
        'type':'response.completed','response':{'output':[item]}})

def wait(predicate, seconds=15):
    until = time.monotonic()+seconds
    while time.monotonic() < until:
        if predicate():
            return
        time.sleep(.02)
    raise AssertionError('bounded fixture condition not reached')

class Fixture:
    def __init__(self, binary, primary, family, constraint):
        self.binary, self.primary, self.family, self.constraint = binary, primary, family, constraint
        self.temp = tempfile.TemporaryDirectory(prefix='t45-builtin-', dir=BENCH)
        self.root = Path(self.temp.name)
        self.project = self.root/'project'
        self.project.mkdir()
        (self.project/'warmup.txt').write_text('OWNED_WARMUP')
        self.home = self.root/'home'
        config = self.home/'config/opencode'
        config.mkdir(parents=True)
        self.requests = {lane: [] for lane in ('main','general','explore','title')}
        self.mcp_methods, self.effects, self.errors = [], [], []
        self.complete = threading.Event()
        self.connect_gate = threading.Event()
        self.lock = threading.Lock()
        fixture = self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                self.send_response(405)
                self.send_header('Content-Length','0')
                self.end_headers()
            def do_DELETE(self):
                self.send_response(200)
                self.send_header('Content-Length','0')
                self.end_headers()
            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                try:
                    if self.path == '/mcp':
                        if fixture.constraint == 'pending' and request['method']=='initialize':
                            assert fixture.connect_gate.wait(10)
                        with fixture.lock:
                            fixture.mcp_methods.append(request['method'])
                        if 'id' not in request:
                            self.send_response(202)
                            self.send_header('Content-Length','0')
                            self.end_headers()
                            return
                        result = mcp_reply(request, lambda args: fixture.effects.append(args))
                        body = json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}).encode()
                        content = 'application/json'
                    elif self.path == '/v1/responses':
                        body = fixture.respond(request)
                        content = 'text/event-stream'
                    else:
                        raise AssertionError(self.path)
                except Exception:
                    fixture.errors.append(traceback.format_exc())
                    body, content = done('FIXTURE_ASSERTION_FAILED'), 'text/event-stream'
                    fixture.complete.set()
                self.send_response(200)
                self.send_header('Content-Type',content)
                self.send_header('Content-Length',str(len(body)))
                self.end_headers()
                try:
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        model = 'fixture-model' if family == 'edit' else 'gpt-5.1-codex'
        self.model = model
        settings = {'model':'fixture/'+model,'default_agent':primary,
                    'permission':{tool:'allow' for tool in NATIVE},
                    'compaction':{'auto':False},
                    'provider':{'fixture':{'npm':'@ai-sdk/openai','options':{
                        'baseURL':f'http://127.0.0.1:{self.server.server_port}/v1','apiKey':'owned-fixture'},
                        'models':{model:{'limit':{'context':500000,'output':4096}}}}},
                    'mcp':{'http':{'type':'remote','url':f'http://127.0.0.1:{self.server.server_port}/mcp'},
                           'stdio':{'type':'local','command':[sys.executable,str(Path(__file__).resolve()),
                                      '--stdio',str(self.root/'stdio-effects')]},
                           'disabled':{'type':'remote','url':f'http://127.0.0.1:{self.server.server_port}/disabled','enabled':False},
                           'failed':{'type':'remote','url':'http://127.0.0.1:1/mcp','timeout':100}}}
        policy = settings['permission']
        if constraint == 'pending':
            settings['mcp']['stdio']['command'].append(str(self.root/'connect-gate'))
        if constraint == 'wire':
            policy.update({name:'deny' for name in WIRE_NAMES})
        elif constraint == 'legacy':
            policy.update({name.replace('__','_'):'deny' for name in WIRE_NAMES})
        elif constraint == 'wildcard':
            settings['permission'] = {'*':'deny',**policy,'http_*':'deny','stdio_*':'deny'}
        elif constraint == 'ask':
            policy.update({'http_*':'ask','stdio_*':'ask'})
        elif constraint == 'resource':
            policy.update({'http_*':{'*':'deny','approved':'allow'},'stdio_*':{'*':'deny','approved':'allow'}})
        elif constraint == 'tools':
            settings['tools'] = {name.replace('__','_'):False for name in WIRE_NAMES}
        elif constraint == 'parent':
            settings['agent'] = {primary:{'permission':{'http_*':'deny','stdio_*':'deny'}}}
        elif constraint == 'profile':
            settings['agent'] = {'explore':{'permission':{'http_*':'deny','stdio_*':'deny'}}}
        elif constraint == 'profile-ask':
            settings['agent'] = {agent:{'permission':{'http_*':'ask','stdio_*':'ask'}} for agent in ('general','explore')}
        elif constraint == 'own-model':
            settings['provider']['fixture']['models']['fixture-model'] = {'limit':{'context':500000,'output':4096}}
            settings['agents'] = {'general':{'model':'fixture/fixture-model'},
                                  'explore':{'model':{'providerID':'fixture','modelID':'fixture-model'}}}
        assert not any(key.startswith(('http','stdio')) for key in policy) if constraint in ('allow','parent','profile','tools') else True
        (config/'opencode.json').write_text(json.dumps(settings))
        self.env = {'HOME':str(self.home),'XDG_CONFIG_HOME':str(self.home/'config'),
                    'XDG_DATA_HOME':str(self.home/'data'),'OC_TEST_ALLOW_LOOPBACK':'1',
                    'TERM':'xterm-256color','PATH':'/usr/bin:/bin'}

    def denied_mcp(self, lane):
        return (self.constraint not in ('allow','profile','pending','profile-ask','own-model') or
                (self.constraint == 'profile' and lane == 'explore') or
                (self.constraint == 'profile-ask' and lane != 'main'))

    def lane_family(self, lane):
        return 'edit' if self.constraint == 'own-model' and lane != 'main' else self.family

    def actions(self, lane):
        actions = [(name,{'value':lane}) for name in WIRE_NAMES]
        if self.constraint == 'ask' and lane == 'main':
            return [('read',{'path':'warmup.txt'}),actions[0]]
        if lane == 'main':
            if self.constraint in ('pending','profile-ask'):
                actions.insert(0,('read',{'path':'warmup.txt'}))
            actions += [('subagent',{'agent':agent,'prompt':'NATIVE_CHILD_'+agent,'description':'builtin '+agent}) for agent in ('general','explore')]
        path = 'mutation-'+lane
        if self.lane_family(lane) == 'edit':
            actions.append(('write',{'path':path,'content':'OWNED_NATIVE_WRITE'}))
        else:
            actions.append(('apply_patch',{'patchText':'*** Begin Patch\n*** Add File: '+path+'\n+OWNED_NATIVE_WRITE\n*** End Patch'}))
        if lane != 'main':
            actions += [('question',{'questions':[{'question':'blocked question','header':'fixture','options':[{'label':'ok','description':'ok'}]}]}),
                        ('subagent',{'agent':'general','prompt':'nested','description':'blocked nested'}),
                        ('opencode_session_rename',{'title':'FORBIDDEN_CHILD_TITLE'}),
                        ('opencode_session_move',{'directory':str(self.project)})]
        if lane == 'explore' or self.primary == 'plan':
            actions.append(('bash',{'argv':['sh','-c','printf effect >> shell-effects']}))
        return actions

    def respond(self, request):
        data = json.dumps(request['input'])
        if 'Generate a short session title' in data:
            assert request['model'] == self.model
            self.requests['title'].append(request)
            return done('Owned fixture')
        users = json.dumps([item for item in request['input'] if item.get('role') == 'user'])
        lane = next((agent for agent in ('general','explore') if 'NATIVE_CHILD_'+agent in users),'main')
        assert request['model'] == ('fixture-model' if self.lane_family(lane) == 'edit' else 'gpt-5.1-codex')
        self.requests[lane].append(request)
        number = len(self.requests[lane])
        initial_headless = self.constraint in ('ask','pending','profile-ask') and lane == 'main' and number == 1
        if initial_headless:
            # Headless startup intentionally has no connect barrier. The first
            # admitted native read gives configured peers time to publish; the
            # next captured request tests actual registered Ask, no consumer.
            if self.constraint == 'pending':
                self.connect_gate.set()
                (self.root/'connect-gate').write_text('release configured startup')
            time.sleep(.15)
        tools = {tool['name']:tool for tool in request['tools']}
        assert not any(name.startswith(('disabled__','failed__')) for name in tools)
        denied = self.denied_mcp(lane)
        visible = not denied or self.constraint in ('ask','resource','profile-ask')
        for name in WIRE_NAMES:
            if not initial_headless:
                assert (name in tools) == visible, (lane,number,self.constraint,name,list(tools))
            elif self.constraint == 'pending':
                assert name not in tools, (name,'pending must have no capability')
            if name in tools:
                assert tools[name]['parameters'] == SCHEMA
        guidance = [item for item in request['input'] if item.get('role') == 'developer' and 'OWNED_MCP_GUIDANCE' in json.dumps(item)]
        expected_guidance = (sum(any(server+'__'+name in tools for name in MCP_NAMES) for server in ('http','stdio')) if initial_headless and not denied else
                             0 if denied else 2)
        assert len(guidance) == expected_guidance, (lane,number,guidance)
        if lane == 'explore':
            assert 'search specialist' in data and 'very thorough' in data
            assert all(name not in tools for name in ('bash','write','edit','apply_patch','question','subagent','opencode_session_rename','opencode_session_move'))
        if lane == 'general':
            assert 'You are OpenCode, a coding assistant' in data
            assert 'search specialist' not in data
            assert all(name not in tools for name in ('question','subagent','opencode_session_rename','opencode_session_move'))
        if lane != 'main':
            assert 'Owned bounded builtin profile fixture' not in users and 'Owned headless Ask fixture' not in users
        if self.primary == 'plan':
            assert all(name not in tools for name in ('bash','write','edit','apply_patch'))
        elif lane != 'explore':
            assert (('write' in tools and 'edit' in tools and 'apply_patch' not in tools) if self.lane_family(lane) == 'edit' else
                    ('apply_patch' in tools and 'write' not in tools and 'edit' not in tools))
        if lane == 'main':
            preview = tools['subagent']['description']
            assert 'Available subagents:' in preview and '- general:' in preview and '- explore:' in preview
            for agent in ('general','explore'):
                section = preview.split('- '+agent+':',1)[1].split('\n- ',1)[0]
                if not initial_headless:
                    assert ('http__rotate_widget' in section) == (not self.denied_mcp(agent) or self.constraint in ('ask','resource','profile-ask'))
                names = section.split('Effective permission preview: ',1)[1].split('. Resource/',1)[0].split(', ')
                if self.primary == 'plan' or agent == 'explore':
                    assert all(name not in names for name in ('write','edit','apply_patch','bash'))
                elif self.lane_family(agent) == 'edit':
                    assert 'write' in names and 'edit' in names and 'apply_patch' not in names
                else:
                    assert 'apply_patch' in names and 'write' not in names and 'edit' not in names
        actions = self.actions(lane)
        outputs = [item for item in request['input'] if item.get('type') == 'function_call_output']
        assert len(outputs) == number-1, (lane,number,outputs)
        if number > 1:
            previous = actions[number-2][0]
            output = outputs[-1]['output']
            if previous.startswith(('http__','stdio__')):
                assert ('OWNED_MCP_RESULT' in output) == (not denied), (lane,previous,output)
            elif previous == 'subagent' and lane == 'main':
                if self.constraint == 'profile-ask':
                    assert 'approval' in output.lower() and 'BUILTIN_CHILD_DONE' not in output, output
                else:
                    assert 'BUILTIN_CHILD_DONE' in output, output
            elif previous in ('write','apply_patch'):
                exists = (self.project/('mutation-'+lane)).exists()
                assert exists == (self.primary == 'build' and lane != 'explore'), (lane,output)
        if number <= len(actions):
            name,args = actions[number-1]
            return call(number,name,args)
        if lane == 'main':
            self.complete.set()
        return done('BUILTIN_CHILD_DONE' if lane != 'main' else 'BUILTIN_ROOT_DONE')

    def run(self):
        headless = self.constraint in ('ask','pending','profile-ask')
        if headless:
            process = subprocess.Popen([str(self.binary),'run','--json','Owned headless Ask fixture'],
                                       cwd=self.project,env=self.env,stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE,stderr=subprocess.STDOUT,start_new_session=True)
            master = process.stdout.fileno()
        else:
            master,slave = pty.openpty()
            fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,140,0,0))
            process = subprocess.Popen([str(self.binary)],cwd=self.project,env=self.env,
                                       stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
            os.close(slave)
        captured = bytearray()
        stop = threading.Event()
        def reader():
            while not stop.is_set():
                if select.select([master],[],[],.05)[0]:
                    try:
                        chunk = os.read(master,65536)
                    except OSError:
                        return
                    if not chunk:
                        return
                    captured.extend(chunk)
                    if len(captured)>1024*1024:
                        self.errors.append('PTY capture bound')
                        return
        thread = threading.Thread(target=reader)
        thread.start()
        try:
            if not headless:
                wait(lambda: self.mcp_methods.count('tools/list') == 1 and len(captured)>100)
                time.sleep(.15)  # owner publication, no deadline/cap change
                os.write(master,b'Owned bounded builtin profile fixture\r')
            try:
                wait(lambda: process.poll() is not None if self.constraint == 'ask' else self.complete.is_set(),20)
            except AssertionError as error:
                raise AssertionError({'process':process.poll(),'requests':{lane:len(rows) for lane,rows in self.requests.items()},
                                      'errors':self.errors,'native_output':captured.decode(errors='replace')[-4000:]}) from error
            if self.constraint != 'ask':
                wait(lambda: b'BUILTIN_ROOT_DONE' in captured or self.errors,10)
            assert not self.errors, self.errors
            if not headless:
                os.write(master,b'\x03')
            process.wait(timeout=10)
            if self.constraint == 'ask':
                assert process.returncode == 1 and b'approval_required' in captured, captured.decode(errors='replace')
                assert len(self.requests['main'])==2 and not self.requests['general'] and not self.requests['explore']
            else:
                assert process.returncode == 0, process.returncode
            stdio_pid = int((self.root/'stdio-effects.pid').read_text())
            try:
                os.kill(stdio_pid,0)
            except ProcessLookupError:
                pass
            else:
                raise AssertionError('native MCP child not reaped')
            expected = {lane:0 if self.denied_mcp(lane) else 2 for lane in ('main','general','explore')}
            actual_http = {lane:sum(args['value']==lane for args in self.effects) for lane in expected}
            stdio_path = self.root/'stdio-effects'
            stdio_effects = [json.loads(line) for line in stdio_path.read_text().splitlines()] if stdio_path.exists() else []
            actual_stdio = {lane:sum(args['value']==lane for args in stdio_effects) for lane in expected}
            assert actual_http == expected and actual_stdio == expected, (expected,actual_http,actual_stdio)
            assert self.mcp_methods.count('initialize')==1 and self.mcp_methods.count('tools/list')==1
            methods = (self.root/'stdio-effects.methods').read_text().splitlines()
            assert methods.count('initialize')==1 and methods.count('tools/list')==1
            assert methods.count('tools/call')==sum(expected.values())
            assert not (self.project/'shell-effects').exists()
            databases = list((self.home/'data').rglob('oc.sqlite'))
            assert len(databases)==1
            with sqlite3.connect(databases[0]) as db:
                dispatched = dict(db.execute("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
                intents = db.execute('SELECT count(*) FROM tool_operations').fetchone()[0]
                turns = db.execute('SELECT count(*) FROM turns').fetchone()[0]
                states = {name+':'+state:count for name,state,count in db.execute('SELECT name,state,count(*) FROM tool_operations GROUP BY name,state')}
                assert db.execute("SELECT count(*) FROM sessions WHERE title='FORBIDDEN_CHILD_TITLE'").fetchone()[0]==0
            for tool in WIRE_NAMES:
                assert states.get(tool+':completed',0)==sum(expected.values())//2, states
            for tool in ('question','bash','opencode_session_rename','opencode_session_move'):
                assert states.get(tool+':completed',0)==0, states
            counts = {lane:len(rows) for lane,rows in self.requests.items()}
            assert dispatched.get('main',0)==counts['main'] and dispatched.get('child',0)==counts['general']+counts['explore']
            assert dispatched.get('title',0)==counts['title'] and sum(dispatched.values())==sum(counts.values())
            return {'primary':self.primary,'family':self.family,'constraint':self.constraint,
                    'requests':counts,'dispatches':dispatched,'turns':turns,'tool_intents':intents,'tool_states':states,
                    'http_effects':actual_http,'stdio_effects':actual_stdio,
                    'native_file_effects':sum((self.project/('mutation-'+lane)).exists() for lane in expected),
                    'denied_shell_effects':0,'captured_views_checked':sum(counts.values())-counts['title']}
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGINT)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGTERM)
                    process.wait(timeout=10)
            stop.set()
            thread.join(timeout=5)
            assert not thread.is_alive()
            if headless:
                process.stdout.close()
            else:
                os.close(master)
            self.server.shutdown()
            self.server.server_close()  # joins non-daemon HTTP request handlers
            self.thread.join(timeout=5)
            assert not self.thread.is_alive()
            self.temp.cleanup()

def main():
    if sys.argv[1]=='--stdio':
        stdio(sys.argv[2],sys.argv[3] if len(sys.argv)>3 else None)
        return
    only = next((arg.split('=',1)[1] for arg in sys.argv[1:] if arg.startswith('--only=')),None)
    binaries = [Path(arg).resolve() for arg in sys.argv[1:] if not arg.startswith('--only=')]
    hashes = {str(binary):hashlib.sha256(binary.read_bytes()).hexdigest() for binary in binaries}
    print(json.dumps({'before':hashes}),flush=True)
    for binary in binaries:
        cases = [('build','edit','allow'),('build','patch','allow'),('plan','edit','allow'),('plan','patch','allow')]
        cases += [('build','edit',constraint) for constraint in ('wire','legacy','wildcard','ask','resource','tools','parent','profile','profile-ask')]
        cases.append(('build','edit','pending'))
        cases.append(('build','patch','own-model'))
        for primary,family,constraint in cases:
            if only and constraint != only:
                continue
            fixture = Fixture(binary,primary,family,constraint)
            print(json.dumps({'binary':str(binary),**fixture.run()}),flush=True)
    after = {str(binary):hashlib.sha256(binary.read_bytes()).hexdigest() for binary in binaries}
    assert after == hashes
    print(json.dumps({'after':after,'cleanup':'all native processes/readers/HTTP handlers reaped; exact TempDirs removed'}),flush=True)

if __name__=='__main__':
    main()
