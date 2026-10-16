#!/usr/bin/env python3
"""Frozen R3 normal ELF proof: owned synthetic HOME, loopback, SQLite and PTY."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import threading

from native_foreground_children import Foreground, native, owner
from run_background_check import source


class Commands(Foreground):
    def __init__(self, binary, case):
        self.posts = []
        self.parent_held = threading.Event()
        super().__init__(binary, case)
        path = Path(self.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
        settings = json.loads(path.read_text())
        settings['agent']['build']['permission'] = {'edit':'deny','shell':'allow'}
        settings['agent']['maker']['model'] = 'fixture/gpt-child-a#slow'
        settings['agent']['maker']['variant'] = 'slow'
        settings['agent']['other'] = dict(mode='primary', system='OWN_OTHER_SYSTEM', model='fixture/gpt-child-a#slow')
        settings['provider']['fixture']['models']['gpt-child-a']['variants'] = {'slow':{'reasoningEffort':'low'}}
        settings['provider']['fixture']['models']['family/override'] = dict(limit=dict(context=500000,output=4096),variants={'fast':{'reasoningEffort':'high'}})
        route = dict(template='EXPANDED $1',agent='maker')
        if case in ('explicit','primary-recovery'):
            route.update(agent='other',subagent=True,model=dict(providerID='fixture',modelID='family/override#slow',variant='fast'))
        elif case == 'fallback': route.update(agent='build',subagent=True)
        elif case == 'legacy':
            directory = path.parent/'commands'
            directory.mkdir()
            directory.joinpath('route.md').write_text('---\ndescription: legacy command\nsubtask: true\n---\nEXPANDED $1')
            route = None
        elif case in ('false','alias-false'):
            route.update(model='fixture/family/override#fast')
            route.update(dict(subagent=False,subtask=True) if case == 'false' else dict(subtask=False))
        elif case == 'inline': route.update(agent='other',model='fixture/family/override#fast')
        elif case == 'invalid-model': route['model']='fixture/absent'
        elif case == 'invalid-variant': route['model']='fixture/gpt-child-a#absent'
        elif case == 'unknown': route['agent']='absent'
        elif case == 'disabled': settings['agent']['maker']['disable']=True
        elif case in ('deny','ask'): settings['permission']['subagent']=case
        elif case == 'depth': settings['experimental']={'subagent_depth':0}
        elif case == 'budget': settings['provider']['fixture']['models']['gpt-child-a']['limit']={'context':1,'output':1}
        settings['command'] = {} if route is None else {'route':route}
        path.write_text(json.dumps(settings))
        self.invocation='/route literal-'+case
        self.expanded='EXPANDED literal-'+case

    def response(self, _, number, request):
        self.posts.append(request)
        if 'PARENT_CONTINUE' in json.dumps(request['input']):
            self.parent_held.set()
            assert self.release[1].wait(15), 'owned parent watchdog'
            return native.completed('parent fixture response')
        if len(self.posts)==1:
            self.arrived[0].set()
            assert self.release[0].wait(15), 'owned command child watchdog'
            if self.case=='cancel':
                return native.tool('shell',{'command':'printf "%s" $$ > owned-leader; exec sleep 5'},'fixture-owned-command-leaf')
        return native.completed('error: typed completed fixture prose')

    def selection(self):
        return self.rows("SELECT key,value FROM prefs WHERE key LIKE 'tui.selection.session:%' ORDER BY key")

    def counterfacts(self):
        return dict(physical=self.physical_requests,main=len(self.requests),auxiliary=self.auxiliary_requests,
            dispatches=self.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1 ORDER BY 1"),
            turns=self.rows('SELECT status,count(*) FROM turns GROUP BY status ORDER BY status'),
            jobs=self.rows('SELECT operation_id,child_id,child_turn,state,delivery_id,message_id FROM child_jobs'),
            notices=self.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'")[0][0],
            operations=self.rows('SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name,state'))

    def __exit__(self,*args):
        root=self.root
        handlers=list(self.server._threads) if isinstance(self.server._threads,list) else []
        super().__exit__(*args)
        assert not root.exists() and all(not h.is_alive() for h in handlers)
        assert all(not Path(f"/proc/{o['pid']}").exists() for o in self.owners)
        print(json.dumps(dict(cleanup=str(root),temp_removed=True,http_joined=True,pty_joined=True,owners=self.owners)),flush=True)


def valid(binary,case):
    with Commands(binary,case) as f:
        f.start()
        before=f.selection()
        f.send(f.invocation.encode()+b'\r')
        assert f.arrived[0].wait(10), (case,f.errors,bytes(f.tail[-600:]))
        background=case not in ('false','alias-false','inline')
        native.until(lambda:f.auxiliary_requests==1,'owned title request')
        expected='family/override' if case in ('explicit','primary-recovery','false','alias-false','inline') else ('m' if case in ('legacy','fallback') else 'gpt-child-a')
        request=f.posts[0]
        assert request['model']==expected
        if expected!='m': assert request['reasoning']['effort']==('high' if expected=='family/override' else 'low')
        wire=json.dumps(request)
        assert f.expanded in wire and f.invocation not in wire
        assert ('OWN_OTHER_SYSTEM' if case in ('explicit','primary-recovery','inline') else 'PARENT_SYSTEM_PRIVATE' if case in ('legacy','fallback') else 'OWN_MAKER_SYSTEM') in wire
        if case not in ('legacy','fallback'): assert 'PARENT_SYSTEM_PRIVATE' not in wire
        assert 'OWN_WORKSPACE_FOREGROUND_GUIDANCE' in wire
        assert f.physical_requests==2 and len(f.requests)==1
        assert f.rows("SELECT text FROM messages WHERE session_id='t50-background' AND role='user'")==[(f.invocation,)]
        assert f.rows("SELECT prompt FROM turns WHERE session_id='t50-background'")==[(f.expanded,)]
        if background:
            native.until(lambda:f.rows("SELECT status FROM turns WHERE session_id='t50-background'")==[('completed',)],'parent command completes before child')
            assert f.rows('SELECT state FROM child_jobs')==[('running',)]
            assert f.selection()==before
            assert f.rows("SELECT state FROM tool_operations WHERE name='subagent'")==[('running',)],f.counterfacts()
            saved=f.rows('SELECT operation_id,child_id,child_turn,delivery_id FROM child_jobs')
            assert f.rows('SELECT count(*) FROM turns')==[(2,)]
            parent_log=json.loads(f.rows("SELECT result FROM turns WHERE session_id='t50-background'")[0][0])
            child_log=json.loads(f.rows("SELECT result FROM turns WHERE session_id!='t50-background'")[0][0])
            assert parent_log['display']['location']==child_log['display']['location']
            assert parent_log['display']['config_generation']==child_log['display']['config_generation']
            assert parent_log['display']['command']['source']=='native_command'
            calls=[item for item in parent_log['input'] if item.get('type')=='function_call']
            outputs=[item for item in parent_log['input'] if item.get('type')=='function_call_output']
            assert len(calls)==len(outputs)==1,(calls,outputs,parent_log['input'])
            assert calls[0]['call_id']==outputs[0]['call_id']
            assert json.loads(outputs[0]['output'])['status']=='running'
        else:
            assert f.rows('SELECT count(*) FROM child_jobs')==[(0,)]
            assert f.selection()!=before
        accepted=f.rows('SELECT * FROM turn_acceptances ORDER BY rowid')
        raw=f.rows('SELECT id,prompt FROM turns ORDER BY rowid')
        if case in ('recovery','primary-recovery','changed-generation'):
            f.stop(crash=True)
            if case=='changed-generation':
                path=Path(f.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
                settings=json.loads(path.read_text())
                settings['command']['route']['template']='CHANGED GENERATION $1'
                path.write_text(json.dumps(settings))
            f.release[0].set()
            f.start()
        elif case=='cancel':
            f.release[0].set()
            native.until(lambda:f.project.joinpath('owned-leader').exists(),'owned command shell leaf')
            pid=int(f.project.joinpath('owned-leader').read_text())
            import os
            f.leaves.append(dict(pid=pid,pgid=os.getpgid(pid),startticks=Path(f'/proc/{pid}/stat').read_text().split(') ')[1].split()[19]))
            f.send(b'PARENT_CONTINUE\r')
            assert f.parent_held.wait(10)
            f.send(b'\x03')
            native.until(lambda:f.rows('SELECT state FROM child_jobs')==[('cancelled',)],'owned command child cancelled')
            native.until(lambda:not Path(f'/proc/{pid}').exists(),'owned command leaf reaped')
            f.release[1].set()
        else: f.release[0].set()
        native.until(lambda:f.rows("SELECT count(*) FROM turns WHERE status='started'")==[(0,)],'command work settled')
        if background:
            native.until(lambda:f.rows('SELECT count(*) FROM child_jobs WHERE message_id IS NOT NULL')==[(1,)],'one command child notice')
            assert f.rows('SELECT operation_id,child_id,child_turn,delivery_id FROM child_jobs')==saved
            expected_state='cancelled' if case=='cancel' else 'unknown' if case=='changed-generation' else 'completed'
            assert f.rows('SELECT state FROM child_jobs')==[(expected_state,)]
        if case in ('recovery','primary-recovery'):
            assert f.physical_requests==3
            assert f.rows('SELECT * FROM turn_acceptances ORDER BY rowid')==accepted
            assert f.rows('SELECT id,prompt FROM turns ORDER BY rowid')==raw
            assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_resume_claim'")==[(1,)]
        elif case=='changed-generation':
            assert f.physical_requests==2
            assert f.rows('SELECT * FROM turn_acceptances ORDER BY rowid')==accepted
            assert f.rows('SELECT id,prompt FROM turns ORDER BY rowid')==raw
            assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_resume_claim'")==[(0,)]
        f.stop()
        counts=f.counterfacts()
        assert sum(row[1] for row in counts['dispatches'])==counts['physical']
        selection=f.selection()
        f.start(); f.stop()
        assert f.counterfacts()==counts and f.selection()==selection
        assert not f.errors,f.errors
        return dict(case=case,status='PASS',counts=counts,wire_model=expected,background=background,reopen_extra_requests=0,raw_invocation=f.invocation,expanded=f.expanded)


def invalid(binary,case):
    with Commands(binary,case) as f:
        process=subprocess.Popen([str(binary),'--data-dir',str(f.data),'run','--json','--session','t50-background',f.invocation],cwd=f.project,env=f.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        identity=owner(process)
        try:
            stdout,stderr=process.communicate(timeout=10)
            assert process.returncode!=0,(case,stdout,stderr)
            assert f.physical_requests==0 and not f.requests and not f.errors
            assert f.rows('SELECT count(*) FROM turns')==[(0,)]
            assert f.rows('SELECT count(*) FROM child_jobs')==[(0,)]
            assert f.rows('SELECT count(*) FROM tool_operations')==[(0,)]
            assert f.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")==[(0,)]
            assert not f.selection()
            identity.update(exit=process.returncode,reaped=not Path(f'/proc/{process.pid}').exists())
            return dict(case=case,status='PASS',physical=0,accepted=0,children=0,intents=0,selection_writes=0,owner=identity,stdout_bytes=len(stdout),stderr_bytes=len(stderr))
        finally:
            if process.poll() is None: process.kill()
            process.wait()
            for stream in (process.stdout,process.stderr):stream.close()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('binaries',nargs='+',type=Path)
    parser.add_argument('--cases',default='inferred,explicit,legacy,false,alias-false,inline,fallback,recovery,primary-recovery,changed-generation,cancel,invalid-model,invalid-variant,unknown,disabled,deny,ask,depth,budget')
    args=parser.parse_args()
    binaries=[path.resolve() for path in args.binaries]
    association=source()
    before={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    print(json.dumps(dict(before=before,source=association)),flush=True)
    for binary in binaries:
        for case in args.cases.split(','):
            run=invalid if case in ('invalid-model','invalid-variant','unknown','disabled','deny','ask','depth','budget') else valid
            print(json.dumps(dict(binary=str(binary),**run(binary,case))),flush=True)
    after={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert before==after and source()==association
    print(json.dumps(dict(after=after,source=association,cases=len(args.cases.split(','))*len(binaries))),flush=True)


if __name__=='__main__':main()
