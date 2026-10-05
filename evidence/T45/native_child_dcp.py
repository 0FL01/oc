#!/usr/bin/env python3
"""Eligible-child atomic: normal ELFs, synthetic owned HOME/HTTP/SQLite only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading
from native_dcp_controls import Controls, configure, input_text, names, ordinary, t50
from native_dcp_defaults import qualify

ROOT = Path(__file__).resolve().parents[2]


def call(agent, identity, session=None):
    args = dict(agent=agent, description=agent, prompt='OWN_'+agent+'_TASK')
    if session:
        args['sessionID'] = session
    return t50.tool('subagent', args, identity)[:-1]


class Child(Controls):
    def start(self):
        super().start()
        self.owners.append(dict(pid=self.process.pid,pgid=os.getpgid(self.process.pid),kind='pty'))

    def stop(self, *args, **kwargs):
        process = self.process
        super().stop(*args, **kwargs)
        if process is not None:
            identity = next(item for item in self.owners if item['pid']==process.pid)
            identity.update(exit=process.returncode,reaped=not Path(f'/proc/{process.pid}').exists())

    def screen(self):
        return super().screen() if hasattr(self, 'grid_lock') else []

    def __init__(self, binary, case, fragment=None, permission=None):
        self.case, self.rounds, self.lock = case, {}, threading.Lock()
        self.children, self.owners = {}, []
        super().__init__(binary, self.response, permission or {'subagent':'allow', 'read':'allow', 'compress':'allow'})
        self.project.joinpath('seed').write_text('own closed read '*200)
        path = self.home/'config/opencode/opencode.json'
        config = json.loads(path.read_text())
        config['agent'].update({
            'build':{'system':'ROOT_PRIVATE_SYSTEM'},
            'general':{'model':'fixture/general-model'},
            'explore':{'model':'fixture/explore-model'},
            'custom':{'mode':'subagent', 'model':'fixture/custom-model', 'system':'OWN_CUSTOM_SYSTEM'},
            'spectator':{'mode':'subagent', 'model':'fixture/spectator-model'}})
        if case == 'profile-deny':
            config['agent']['explore']['permission'] = {'compress':'deny'}
        if case == 'profile-wildcard':
            config['agent']['explore']['permission'] = {'*':'deny', 'read':'allow'}
        if case == 'parent-plan':
            # Plan's native readonly ceiling is captured before child launch.
            config['default_agent'] = 'plan'
            config['agent']['plan'] = {'permission':{'compress':'deny'}}
        config['provider']['fixture']['models'].update({
            name+'-model':{'limit':{'context':context,'output':4096}}
            for name,context in [('general',200000),('explore',300000),('custom',400000),('spectator',500000)]})
        path.write_text(json.dumps(config))
        if fragment is not None:
            configure(self, fragment)

    def response(self, _, number, request):
        model = request['model']
        with self.lock:
            self.rounds[model] = self.rounds.get(model,0)+1
            round = self.rounds[model]
        if model == 'm':
            if self.case == 'child-false' and round == 3:
                text = next(item['content'][0]['text'] for item in request['input'] if 'DCP context anchors' in json.dumps(item))
                closed = [a for a in json.loads(text[text.index('[{'):]) if a['closed']]
                return t50.tool('compress', {'topic':'root still enabled', 'content':[{'startId':closed[0]['id'], 'endId':closed[-1]['id'], 'summary':'ROOT_KEPT short fact'}]}, 'root-compress')
            if round % 2 == 0:
                return t50.completed('root terminal '+('discard root filler '*200 if self.case == 'child-false' and round == 2 else ''))
            agents = ['general','explore','custom','spectator'] if self.case in ('default','ask') else ['explore']
            if round > 1:
                agents = [agent for agent in agents if agent != 'spectator']
            return sum((call(agent, f'delegate-{agent}-{round}', self.children.get(agent)) for agent in agents), []) + t50.completed('')[-1:]
        agent = model.removesuffix('-model')
        wire = input_text(request)
        assert 'ROOT_PRIVATE' not in wire
        assert 'OWN_'+agent+'_TASK' in wire
        if agent == 'explore':
            assert not set(names(request)).intersection(('shell','bash','edit','write','apply_patch','subagent','question'))
        enabled = self.case in ('default','ask','explore-absent')
        if not enabled:
            ordinary(request)
            if round == 1:
                return t50.tool('compress', {'topic':'stale','content':[{'startId':'alien','endId':'alien','summary':'never store'}]}, 'stale-child')
            pairs = [item for item in request['input'] if item.get('type') == 'function_call_output' and item['call_id'] == 'stale-child']
            assert len(pairs) == 1 and 'excluded by issuing request' in pairs[0]['output']
            return t50.completed('refused child terminal')
        assert 'compress' in names(request)
        if agent == 'spectator':
            return t50.completed('SPECTATOR_UNCHANGED')
        if round == 1:
            return t50.tool('read', {'path':'seed'}, 'read-'+agent)
        if round == 2:
            pairs = [item for item in request['input'] if item.get('type') == 'function_call_output' and item['call_id'] == 'read-'+agent]
            assert len(pairs) == 1
            return t50.completed('OLD_'+agent+'_SENTINEL '+'discard closed work '*200)
        if round == 3:
            text = next(item['content'][0]['text'] for item in request['input'] if 'DCP context anchors' in json.dumps(item))
            closed = [a for a in json.loads(text[text.index('[{'):]) if a['closed']]
            return t50.tool('compress', {'topic':'own '+agent, 'content':[{'startId':closed[0]['id'], 'endId':closed[-1]['id'], 'summary':'OWN_'+agent+'_KEPT short working fact'}]}, 'compress-'+agent)
        assert round in (4,5), 'unexpected replay'
        assert 'OWN_'+agent+'_KEPT' in wire and 'OLD_'+agent+'_SENTINEL' not in wire
        calls = [i for i in request['input'] if i.get('call_id') == 'read-'+agent and i.get('type') == 'function_call']
        results = [i for i in request['input'] if i.get('call_id') == 'read-'+agent and i.get('type') == 'function_call_output']
        assert len(calls) == len(results) <= 1
        if round == 4:
            result = next(i for i in request['input'] if i.get('call_id') == 'compress-'+agent and i.get('type') == 'function_call_output')
            assert 'compressed' in result['output']
        return t50.completed('own final')

    def run_cli(self, prompt):
        process = subprocess.Popen([str(self.binary),'--data-dir',str(self.data),'run','--json','--session','t50-background',prompt],
            cwd=self.project,env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        identity = dict(pid=process.pid, pgid=os.getpgid(process.pid))
        try:
            stdout,stderr = process.communicate(timeout=15)
            assert process.returncode == 0, stderr.decode()
            assert len(stdout)<262144 and len(stderr)<65536
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            process.stdout.close(); process.stderr.close()
            identity.update(exit=process.returncode, reaped=not Path(f'/proc/{process.pid}').exists())
            self.owners.append(identity)

    def raw(self):
        return self.rows('SELECT id,session_id,seq,role,text FROM messages ORDER BY rowid')

    def remember_children(self):
        self.children = {agent:session for session,agent in self.rows('SELECT id,agent FROM sessions WHERE parent_id IS NOT NULL')}

    def receipt(self):
        assert not self.errors, self.errors
        thresholds = []
        contexts = {'general':200000,'explore':300000,'custom':400000,'spectator':500000}
        for agent,result in self.rows('SELECT s.agent,t.result FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.parent_id IS NOT NULL'):
            for span in json.loads(result)['spans']:
                facts = span['request']
                context = contexts[agent]
                assert (facts['context_limit'],facts['dcp_min_context'],facts['dcp_max_context']) == (context,context*40//100,context*55//100)
                thresholds.append((agent,context,facts['dcp_min_context'],facts['dcp_max_context']))
        print(json.dumps(dict(case=self.case, binary=str(self.binary), status='PASS', physical=self.physical_requests,
            main=len(self.requests), auxiliary=self.auxiliary_requests, owners=self.owners,
            models=self.rounds, blocks=self.rows('SELECT session_id,count(*) FROM compression_blocks GROUP BY session_id'),
            operations=self.rows('SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name,state'),
            grants=self.rows('SELECT count(*) FROM permission_grants'), thresholds=thresholds, files_written=0)), flush=True)


def compression(binary, ask=False, absent=False):
    case = 'ask' if ask else ('explore-absent' if absent else 'default')
    permission = {'subagent':'allow','read':'allow'} if absent else None
    with Child(binary,case,{'compress':{'permission':'ask'}} if ask else None,permission) as f:
        if ask:
            f.start(); f.send(b'ROOT_PRIVATE seed\r')
            t50.until(lambda:f.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(5,)],'seed child turns')
        else:
            f.run_cli('ROOT_PRIVATE seed')
        f.remember_children()
        before = f.raw()
        if ask:
            f.send(b'ROOT_PRIVATE compress\r')
            for count in range(3):
                t50.until(lambda:'Permission required' in '\n'.join(f.screen()),'genuine child Ask')
                assert f.rows('SELECT count(*) FROM compression_blocks')[0][0] == count
                f.send(b'\r')
                t50.until(lambda:f.rows('SELECT count(*) FROM compression_blocks')[0][0] == count+1,'approved own commit')
            t50.until(lambda:f.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(9,)],'compressed child turns')
            f.stop()
        else:
            f.run_cli('ROOT_PRIVATE compress')
        assert f.raw()[:len(before)] == before, 'immutable RAW prefix'
        expected = 1 if absent else 3
        assert f.rows('SELECT count(*) FROM compression_blocks') == [(expected,)], (f.rounds, f.errors, f.rows('SELECT name,state,output FROM tool_operations'))
        assert f.rows("SELECT count(*) FROM compression_blocks WHERE session_id='t50-background'") == [(0,)]
        if not absent:
            spectator = f.children['spectator']
            assert [r for r in f.raw() if r[1]==spectator] == [r for r in before if r[1]==spectator]
        effects = f.rows('SELECT name,state,count(*) FROM tool_operations WHERE name!=\'subagent\' GROUP BY name,state ORDER BY name')
        physical = f.physical_requests
        # A real fresh native process reopens the same durable HOT; no replay.
        if ask:
            f.start(); f.send(b'ROOT_PRIVATE reopen\r')
            t50.until(lambda:f.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(13,)],'reopened child turns')
            f.stop()
        else:
            f.run_cli('ROOT_PRIVATE reopen')
        assert f.rows('SELECT name,state,count(*) FROM tool_operations WHERE name!=\'subagent\' GROUP BY name,state ORDER BY name') == effects
        assert f.physical_requests == physical+(3 if absent else 5), (physical, f.physical_requests, f.auxiliary_requests)
        assert len(f.requests) == (11 if absent else 22)
        assert f.rows('SELECT count(*) FROM permission_grants') == [(0,)]
        f.receipt()


def refusal(binary, case, fragment=None, permission=None):
    with Child(binary,case,fragment,permission) as f:
        f.run_cli('ROOT_PRIVATE refusal')
        assert len(f.requests)==4
        root = next(r for r in f.requests if r['model']=='m')
        if case == 'child-false':
            assert 'compress' in names(root)
            # Real root compression remains admitted under child-only opt-out.
            f.run_cli('ROOT_PRIVATE seed root closed')
        assert f.rows('SELECT count(*) FROM compression_blocks') == [(1 if case == 'child-false' else 0,)]
        if case == 'child-false':
            assert f.rows("SELECT count(*) FROM compression_blocks WHERE session_id='t50-background'") == [(1,)]
            assert 'ROOT_KEPT' in input_text(f.requests[-1]) and 'discard root filler' not in input_text(f.requests[-1])
        assert f.rows('SELECT count(*) FROM permission_grants') == [(0,)]
        assert f.rows("SELECT count(*) FROM tool_operations WHERE name='compress' AND state='started'") == [(0,)]
        preview = next(tool['description'] for tool in root['tools'] if tool['name']=='subagent')
        assert 'Own-session compression:' in preview
        f.receipt()


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('binaries',nargs='+',type=Path)
    parser.add_argument('--budgets-only',action='store_true')
    args=parser.parse_args(); binaries=[p.resolve() for p in args.binaries]
    before={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries}
    print(json.dumps(dict(base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        dirty=subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).splitlines(),before=before)),flush=True)
    for binary in binaries:
        with tempfile.TemporaryDirectory(prefix='t45-child-help-') as tmp:
            home = Path(tmp)
            result = subprocess.run([str(binary),'--help'],env={'HOME':str(home),'XDG_CONFIG_HOME':str(home/'config'),'PATH':'/usr/bin:/bin'},cwd=home,capture_output=True,timeout=10)
            assert result.returncode == 0 and b'Usage:' in result.stdout
            print(json.dumps(dict(case='isolated-help',binary=str(binary),exit=0,reaped=True,stdout_bytes=len(result.stdout))),flush=True)
        assert not home.exists()
        if args.budgets_only:
            qualify(binary,'child-atomic-budget-known',{'context':40000,'output':2048})
            qualify(binary,'child-atomic-budget-missing',{})
            continue
        compression(binary)
        compression(binary,ask=True)
        compression(binary,absent=True)
        for case,fragment,permission in [
            ('child-false',{'experimental':{'allowSubAgents':False}},None),
            ('global-off',{'enabled':False},None),('tool-off',{'compress':{'enabled':False}},None),
            ('manual',{'manualMode':{'enabled':True}},None),
            ('central-deny',None,{'subagent':'allow','read':'allow','compress':'deny'}),
            ('no-consumer-ask',{'compress':{'permission':'ask'}},None),
            ('profile-deny',None,None),('profile-wildcard',None,None),('parent-plan',None,None)]:
            refusal(binary,case,fragment,permission)
    after={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries}
    assert before==after
    print(json.dumps(dict(after=after,unchanged=True,cases=(2 if args.budgets_only else 12)*len(binaries))),flush=True)


if __name__=='__main__': main()
