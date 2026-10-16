#!/usr/bin/env python3
"""R7 synthetic normal-ELF root/child/restart/Location capture; no live APIs."""
import argparse
import datetime
import hashlib
import http.server
import json
import os
from pathlib import Path
import signal
import sqlite3
import struct
import subprocess
import tempfile
import threading
from native_dcp_controls import Controls, t50

OPEN, CLOSE = '<oc-execution-environment>', '</oc-execution-environment>'
BASE = 'You are OpenCode, a native coding assistant.'


def text(item):
    return ''.join(p.get('text', '') for p in item.get('content', []) if isinstance(p, dict))


def output(message=None, call=None):
    item = ({'type':'function_call','id':call[0], 'call_id':call[0], 'name':call[1],
             'arguments':json.dumps(call[2]), 'status':'completed'} if call else
            {'type':'message','role':'assistant','content':[{'type':'output_text','text':message}]})
    return ('data: '+json.dumps({'type':'response.completed','response':{
        'status':'completed','output':[item]}})+'\n\n').encode()


def qualify(binary):
    assert os.geteuid() != 0
    with tempfile.TemporaryDirectory(prefix='t45-host-owned-') as temporary:
        root = Path(temporary)
        home, a, b, data = (root / n for n in ('home','a','b<delimiter>','data'))
        config = home / 'config/opencode'
        config.mkdir(parents=True)
        a.mkdir(); b.mkdir()
        a.joinpath('seed').write_text('ROOT_EFFECT_FACT')
        # Real synthetic Git, independently initialized before native collection.
        subprocess.run(['git','init','--quiet',str(a)],check=True,env={'PATH':'/usr/bin:/bin','HOME':str(home)})
        a.joinpath('AGENTS.md').write_text('A_INSTRUCTION')
        b.joinpath('AGENTS.md').write_text('B_INSTRUCTION')
        requests, failures, owners, rounds = [], [], [], {}
        state = {'project':a,'custom':False,'process':None}

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup(); self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get('content-length',0))
                    assert 0 < length <= 1048576
                    request = json.loads(self.rfile.read(length))
                    requests.append(request)
                    layers = [text(i) for i in request['input'] if OPEN in text(i)]
                    assert len(layers) == 1, 'missing/duplicate R7 environment layer'
                    assert layers[0].count(OPEN) == layers[0].count(CLOSE) == 1
                    layer = layers[0]
                    facts = json.loads(layer.split(OPEN+'\n',1)[1].split('\n'+CLOSE,1)[0])
                    uname = os.uname()
                    assert facts['hostname'] == uname.nodename
                    assert facts['os'] == uname.sysname
                    assert facts['kernelRelease'] == uname.release
                    assert facts['machineArchitecture'] == uname.machine
                    with binary.open('rb') as elf:
                        header = elf.read(20)
                    assert header[:4] == b'\x7fELF'
                    machine = int.from_bytes(header[18:20], 'little' if header[5]==1 else 'big')
                    assert facts['processArchitecture'] == {62:'x86_64',183:'aarch64',3:'x86',40:'arm',243:'riscv64'}[machine]
                    assert facts['processPointerBits'] == {1:32,2:64}[header[4]]
                    assert facts['effectiveUid'] == os.geteuid() and facts['effectiveGid'] == os.getegid()
                    pid = state['process'].pid
                    status = dict(line.split(':',1) for line in Path(f'/proc/{pid}/status').read_text().splitlines() if ':' in line)
                    assert int(status['CapEff'].strip(),16) == 0
                    assert int(status['Uid'].split()[1]) == facts['effectiveUid']
                    assert int(status['Gid'].split()[1]) == facts['effectiveGid']
                    # Available execution CPUs may be narrowed by cgroup quota; never online host count.
                    assert 1 <= facts['availableParallelism'] <= len(os.sched_getaffinity(pid))
                    quota = Path('/sys/fs/cgroup/cpu.max')
                    if quota.exists() and quota.read_text().split()[0] == 'max':
                        assert facts['availableParallelism'] == len(os.sched_getaffinity(pid))
                    project = state['project']
                    assert facts['workingDirectory'] == facts['workspaceRoot'] == str(project)
                    assert facts['gitRepository'] == (project == a)
                    assert facts['shellExecutable'] == '/usr/bin/bash', 'fish SHELL must select PATH bash'
                    assert facts['temporaryDirectory'] == str(root/'approved-tmp')
                    assert facts['dateUtc'] == datetime.datetime.now(datetime.timezone.utc).date().isoformat()
                    distro = {}
                    for line in Path('/etc/os-release').read_text().splitlines():
                        if '=' in line:
                            key, value = line.split('=',1)
                            distro[key]=value.strip('"\'')
                    assert facts['distributionId'] == distro['ID']
                    assert facts['distributionVersion'] == distro['VERSION_ID']
                    wire = json.dumps(request)
                    assert 'ENV_SECRET_TRAP' not in wire and '/unselected/fish' not in wire
                    if not request.get('tools'):
                        assert request.get('max_output_tokens') == 256
                        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
                        self.wfile.write(output('synthetic title'))
                        return
                    assert 'A_INSTRUCTION' in wire if project == a else 'B_INSTRUCTION' in wire and 'A_INSTRUCTION' not in wire
                    profiles = [text(i) for i in request['input'] if i.get('role')=='developer']
                    model = request['model']
                    custom = state['custom'] if model == 'm' else model == 'own-custom'
                    assert sum('CUSTOM_OWN_SYSTEM' in p for p in profiles) == int(custom)
                    assert sum(BASE in p for p in profiles) == int(not custom)
                    if model != 'm':
                        assert 'ROOT_ONLY_TRANSCRIPT' not in wire
                        count = rounds.get(model,0)+1; rounds[model]=count
                        body = output(call=(model+'-read','read',{'path':'seed'})) if count==1 else output('child terminal')
                    elif state.get('first'):
                        count = rounds.get('root',0)+1; rounds['root']=count
                        if count == 1:
                            body = output(call=('root-read','read',{'path':'seed'}))
                        elif count in (2,3):
                            body = output(call=('child-'+str(count),'subagent',{
                                'agent':'plain' if count==2 else 'customchild','description':'own child','prompt':'OWN_CHILD_TASK'}))
                        else:
                            assert count == 4
                            body = output('root terminal')
                    else:
                        body = output('restart/location terminal')
                    self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
                    self.wfile.write(body)
                except Exception as error:
                    failures.append(repr(error))
                    self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
                    self.wfile.write(output('fixture assertion recorded'))

        server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        config.joinpath('opencode.json').write_text(json.dumps({
            'model':'fixture/m','compaction':{'auto':False}, 'permission':{'read':'allow','subagent':'allow'},
            'agent':{'title':{'disable':True},'customroot':{'system':'CUSTOM_OWN_SYSTEM'},
                     'plain':{'mode':'subagent','model':'fixture/own-plain'},
                     'customchild':{'mode':'subagent','model':'fixture/own-custom','system':'CUSTOM_OWN_SYSTEM'}},
            'provider':{'fixture':{'npm':'@ai-sdk/openai','options':{
                'baseURL':f'http://127.0.0.1:{server.server_port}/v1','apiKey':'synthetic'},
                'models':{n:{'limit':{'context':100000,'output':2048}} for n in ('m','own-plain','own-custom')}}}}))
        env = {'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),'XDG_DATA_HOME':str(home/'data'),
               'XDG_CACHE_HOME':str(home/'cache'),'PATH':'/usr/bin:/bin','SHELL':'/unselected/fish',
               'TMPDIR':str(root/'approved-tmp'),'OC_TEST_ALLOW_LOOPBACK':'1','UNSELECTED':'ENV_SECRET_TRAP'}
        root.joinpath('approved-tmp').mkdir()
        try:
            for number, (project, session, custom) in enumerate(((a,'host-a',False),(a,'host-a',True),(b,'host-b',True),(a,'host-a',False))):
                state.update(project=project,custom=custom,first=number==0)
                command = [str(binary),'--data-dir',str(data),'run','--json','--session',session,
                           '--agent','customroot' if custom else 'build','ROOT_ONLY_TRANSCRIPT']
                process = subprocess.Popen(command,cwd=project,env=env,stdin=subprocess.DEVNULL,
                                           stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
                state['process']=process
                try:
                    stdout, stderr = process.communicate(timeout=30)
                finally:
                    if process.poll() is None:
                        os.killpg(process.pid,signal.SIGKILL); process.communicate(timeout=3)
                    owners.append({'pid':process.pid,'exit':process.returncode,'reaped':not Path(f'/proc/{process.pid}').exists()})
                assert not failures, failures
                assert process.returncode == 0, stderr.decode()
            with sqlite3.connect(data/'oc.sqlite') as database:
                assert database.execute("SELECT count(*) FROM messages WHERE text LIKE '%oc-execution-environment%'").fetchone() == (0,)
                assert database.execute("SELECT count(*) FROM tool_operations WHERE state!='completed'").fetchone() == (0,)
            print(json.dumps({'status':'PASS','requests':requests,'owners':owners,'cases':[
                'root-default-continuation','own-model-default-child','own-model-custom-child',
                'root-custom-restart','Location-B-custom','Location-A-default-reopen']}),flush=True)
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)
            assert not thread.is_alive() and all(o['reaped'] for o in owners)
    assert not root.exists()
    print(json.dumps({'cleanup':'all native PGIDs/stdio/HTTP joined; exact owned TempDir removed'}),flush=True)


def live_location(binary):
    """One live owner: move applies only after old request/tools/turn close."""
    def response(owner, number, request):
        layers = [text(i) for i in request['input'] if OPEN in text(i)]
        assert len(layers) == 1
        facts = json.loads(layers[0].split(OPEN+'\n',1)[1].split('\n'+CLOSE,1)[0])
        expected = owner.project if number <= 3 or number >= 7 else owner.destination
        assert facts['workingDirectory'] == facts['workspaceRoot'] == str(expected)
        assert facts['shellExecutable'] == '/usr/bin/bash'
        assert facts['temporaryDirectory'] == str(owner.root/'approved-tmp')
        wire = json.dumps(request['input'])
        assert ('LIVE_A_RULE' in wire) == (expected == owner.project)
        assert ('LIVE_B_RULE' in wire) == (expected == owner.destination)
        assert sum(BASE in text(i) for i in request['input'] if i.get('role')=='developer') == int(expected == owner.project)
        assert sum('LIVE_B_CUSTOM' in text(i) for i in request['input'] if i.get('role')=='developer') == int(expected == owner.destination)
        if number in (1,4):
            destination = owner.destination if number == 1 else owner.project
            return t50.tool('opencode_session_move',{'directory':str(destination)},'move-'+str(number))
        if number in (2,5):
            result = next(i for i in request['input'] if i.get('call_id')=='move-'+str(number-1) and i.get('type')=='function_call_output')
            assert json.loads(result['output'])['status'] == 'pending'
            return t50.tool('shell',{'command':'printf \'%s\\n\' "$0" "$TMPDIR"; pwd'},'cwd-'+str(number))
        if number in (3,6):
            result = next(i for i in request['input'] if i.get('call_id')=='cwd-'+str(number-1) and i.get('type')=='function_call_output')
            assert str(expected) in result['output'], result
            assert '/usr/bin/bash' in result['output'] and str(owner.root/'approved-tmp') in result['output']
        assert number <= 7
        return t50.completed('live boundary terminal')

    with Controls(binary,response,{'*':'allow','external_directory':'allow'}) as native:
        native.destination = native.root/'destination'
        native.destination.mkdir()
        (native.project/'AGENTS.md').write_text('LIVE_A_RULE')
        (native.destination/'AGENTS.md').write_text('LIVE_B_RULE')
        (native.destination/'opencode.json').write_text(json.dumps({'agent':{'build':{'system':'LIVE_B_CUSTOM'}}}))
        (native.root/'approved-tmp').mkdir()
        native.env['SHELL']='/not-selected/fish'
        native.env['TMPDIR']=str(native.root/'approved-tmp')
        native.start(); first_pid=native.process.pid
        native.send(b'move source to destination\r')
        t50.until(lambda: native.rows('SELECT phase FROM session_moves')==[('applied',)], 'first placement not terminal/applied')
        t50.until(lambda: 'destination' in '\n'.join(native.screen()[-4:]), 'destination frontend not adopted')
        native.send(b'move destination to source\r')
        t50.until(lambda: native.rows('SELECT phase FROM session_moves ORDER BY rowid')==[('applied',),('applied',)], 'second placement not terminal/applied')
        t50.until(lambda: 'project' in '\n'.join(native.screen()[-4:]), 'source frontend not adopted')
        native.send(b'continue source\r')
        t50.until(lambda:len(native.requests)==7 and native.settled(3), 'live placement continuation not settled')
        assert native.process.pid == first_pid and not native.errors, native.errors
        assert native.rows("SELECT count(*) FROM messages WHERE text LIKE '%oc-execution-environment%'") == [(0,)]
        print(json.dumps({'status':'PASS','case':'live-Location-A-B-A-prepared-source-pin','requests':native.requests,
                          'pid':first_pid,'normal_exit_expected':0,'same_process':True}),flush=True)


if __name__ == '__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('--binary',type=Path,required=True)
    args=parser.parse_args(); binary=args.binary.resolve()
    with binary.open('rb') as stream:
        digest = hashlib.file_digest(stream,'sha256').hexdigest()
    with binary.open('rb') as stream:
        header = stream.read(20)
    print(json.dumps({'binary':str(binary),'sha256':digest,'elf_magic':header[:4].hex(),
                      'elf_class':header[4],'elf_machine':int.from_bytes(header[18:20],'little')}),flush=True)
    qualify(binary)
    live_location(binary)
