#!/usr/bin/env python3
"""Actual normal-ELF foreground barriers, exact IDs, durable counters and owners.

Synthetic loopback/config/HOME only. No goal-success text matching, live API,
background-subagent support or family navigation claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'evidence/T50'))
import native_background as native
from run_foreground_check import source

def items(request):
    return [(item['call_id'],item['output']) for item in request['input']
            if item.get('type')=='function_call_output']

def call(name,args,identity):
    return native.tool(name,args,identity)[:-1]

def owner(process):
    return dict(pid=process.pid,pgid=os.getpgid(process.pid),
                startticks=Path(f'/proc/{process.pid}/stat').read_text().split(') ')[1].split()[19])

class Foreground(native.Native):
    def __init__(self,binary,case):
        self.case=case
        self.arrived=[threading.Event(),threading.Event()]
        self.release=[threading.Event(),threading.Event()]
        self.followup=threading.Event()
        self.child_requests=[None,None]
        self.parent_requests=[]
        self.lock=threading.Lock()
        self.owners=[]
        self.leaves=[]
        self.cli=[]
        super().__init__(binary,self.response,{'subagent':'allow','edit':'allow','shell':'allow','read':'allow'})
        self.project.joinpath('AGENTS.md').write_text('OWN_WORKSPACE_FOREGROUND_GUIDANCE')
        config=Path(self.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
        settings=json.loads(config.read_text())
        settings['agent'].update({
            'title':{'disabled':True},
            'build':{'system':'PARENT_SYSTEM_PRIVATE'},
            'maker':{'mode':'subagent','model':'fixture/gpt-child-a',
                     'system':'OWN_MAKER_SYSTEM','permission':{'edit':'allow','shell':'allow'}},
            'reader':{'mode':'subagent','model':'fixture/gpt-decoy',
                      'system':'OWN_READER_SYSTEM','permission':{'edit':'deny','shell':'deny'}}})
        settings['provider']['fixture']['models'].update({name:{'limit':{'context':500000,'output':4096}}
            for name in ('gpt-child-a','child-b','gpt-decoy')})
        config.write_text(json.dumps(settings))

    def response(self,_,number,request):
        model=request['model']
        pairs=items(request)
        if model=='m':
            with self.lock: self.parent_requests.append(request)
            if not pairs:
                return (call('subagent',{'agent':'maker','description':'A','prompt':'CHILD_A_TASK'},'original-call-A')
                        +call('subagent',{'agent':'reader','description':'B','prompt':'CHILD_B_TASK','model':'fixture/child-b'},'original-call-B')
                        +[{'type':'response.completed','response':{'status':'completed'}}])
            assert self.case=='reverse' and self.release[0].is_set() and self.release[1].is_set()
            self.followup.set()
            return native.completed('synthetic parent output')
        index=0 if model=='gpt-child-a' else 1
        assert model in ('gpt-child-a','child-b')
        if not pairs:
            self.child_requests[index]=request
            self.arrived[index].set()
            assert self.release[index].wait(10),'owned provider barrier watchdog'
            if index==1: return native.completed('synthetic reader output')
            if self.case=='reverse':
                return native.tool('apply_patch',{'patchText':'*** Begin Patch\n*** Add File: once\n+x\n*** End Patch'},'exact-one-effect')
            return native.tool('shell',{'command':'printf "%s" $$ > owned-leader; exec sleep 5'},'cancel-leaf')
        assert index==0 and self.case=='reverse'
        assert [identity for identity,_ in pairs]==['exact-one-effect']
        return native.completed('synthetic maker output')

    def wait_children(self):
        assert all(event.wait(10) for event in self.arrived),'both actual peers must dispatch before release'
        assert not any(event.is_set() for event in self.release)
        native.until(lambda:self.auxiliary_requests==1,'owned initial auxiliary request')
        assert self.physical_requests==4 and len(self.requests)==3 and not self.followup.is_set(),(self.physical_requests,len(self.requests),self.auxiliary_requests)
        for index,request in enumerate(self.child_requests):
            wire=json.dumps(request)
            assert ('OWN_MAKER_SYSTEM' if index==0 else 'OWN_READER_SYSTEM') in wire
            assert 'OWN_WORKSPACE_FOREGROUND_GUIDANCE' in wire
            assert 'PARENT_SYSTEM_PRIVATE' not in wire and 'PARENT_PRIVATE_TRANSCRIPT' not in wire
            prompts=[item for item in request['input'] if item.get('role')=='user']
            assert len(prompts)==1 and ('CHILD_A_TASK' if index==0 else 'CHILD_B_TASK') in json.dumps(prompts[0])
            names={tool['name'] for tool in request['tools']}
            if index==0: assert 'apply_patch' in names and 'edit' not in names and 'write' not in names
            else: assert not names.intersection(('edit','write','apply_patch','bash','shell'))
        assert self.child_requests[0]['model']=='gpt-child-a'
        assert self.child_requests[1]['model']=='child-b','explicit override precedes profile model'
        rows=self.rows('SELECT id,parent_id,agent,model FROM sessions WHERE parent_id IS NOT NULL ORDER BY rowid')
        assert len(rows)==2 and rows[0][2:] == ('maker','fixture/gpt-child-a') and rows[1][2:] == ('reader','fixture/child-b')
        self.children=rows
        assert self.rows('SELECT count(*) FROM turns')[0][0]==3
        assert self.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")[0][0]==4

    def counts(self):
        return dict(physical=self.physical_requests,main=len(self.requests),auxiliary=self.auxiliary_requests,
                    dispatches=self.rows("SELECT json_extract(payload,'$.lane'),json_extract(payload,'$.owner'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1,2 ORDER BY 1,2"),
                    turns=self.rows('SELECT session_id,status,count(*) FROM turns GROUP BY session_id,status ORDER BY session_id,status'),
                    children=self.rows('SELECT id,parent_id,agent,model FROM sessions WHERE parent_id IS NOT NULL ORDER BY rowid'),
                    operations=self.rows('SELECT session_id,name,state,input,output FROM tool_operations ORDER BY rowid'))

    def start(self):
        super().start()
        self.owners.append(owner(self.process))

    def stop(self,*args,**kwargs):
        process=self.process
        super().stop(*args,**kwargs)
        if process is not None:
            self.owners[-1].update(exit=process.returncode,reaped=not Path(f'/proc/{process.pid}').exists())
        for leaf in self.leaves:
            assert not Path(f"/proc/{leaf['pid']}").exists(),'owned native leaf survived shutdown'
            leaf['reaped']=True

    def __exit__(self,*args):
        for event in self.release: event.set()
        super().__exit__(*args)

def reverse(binary):
    with Foreground(binary,'reverse') as fixture:
        process=subprocess.Popen([str(binary),'--data-dir',str(fixture.data),'run','--json','--session','t50-background','PARENT_PRIVATE_TRANSCRIPT'],
            cwd=fixture.project,env=fixture.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        identity=owner(process)
        try:
            fixture.wait_children()
            fixture.release[1].set()
            native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='completed' AND session_id=?",[fixture.children[1][0]])==[(1,)],'reader completion')
            assert fixture.physical_requests==4 and len(fixture.parent_requests)==1 and not fixture.followup.is_set()
            fixture.release[0].set()
            stdout,stderr=process.communicate(timeout=10)
            assert process.returncode==0,stderr.decode()
            assert len(stdout)<262144 and len(stderr)<65536
            assert fixture.followup.is_set() and fixture.physical_requests==6
            pairs=items(fixture.parent_requests[-1])
            assert [identity for identity,_ in pairs]==['original-call-A','original-call-B']
            for pair,row in zip(pairs,fixture.children):
                assert pair[1].startswith(f'<subagent sessionID="{row[0]}" state="completed">')
            assert fixture.project.joinpath('once').read_bytes()==b'x\n'
            assert fixture.rows("SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name")==[('apply_patch','completed',1),('subagent','completed',2)]
            assert fixture.rows("SELECT status,count(*) FROM turns GROUP BY status")==[('completed',3)]
            counts=fixture.counts()
            assert sum(row[2] for row in counts['dispatches'])==fixture.physical_requests
            frames=[json.loads(line) for line in stdout.splitlines()]
            metadata=dict(stdout_bytes=len(stdout),stderr_bytes=len(stderr),events=len(frames),
                types={str(kind):sum(frame.get('type')==kind for frame in frames) for kind in sorted({frame.get('type') for frame in frames},key=str)},
                stdout_sha256=hashlib.sha256(stdout).hexdigest())
            fixture.start()
            native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='started'")==[(0,)],'reopen settled')
            fixture.stop()
            assert fixture.counts()==counts,'reopen must not replay child requests/operations'
            assert not fixture.errors
            identity.update(exit=process.returncode,reaped=not Path(f'/proc/{process.pid}').exists())
            return dict(case='reverse',status='PASS',counts=counts,callIds=[pair[0] for pair in pairs],effect_bytes=2,
                        request_views=fixture.child_requests,owners=[identity,*fixture.owners],stdout=metadata,reopenNoReplay=True)
        finally:
            for event in fixture.release:event.set()
            if process.poll() is None: process.kill()
            process.wait()
            for stream in (process.stdout,process.stderr):stream.close()

def cancel(binary):
    with Foreground(binary,'cancel') as fixture:
        fixture.start()
        fixture.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        fixture.wait_children()
        fixture.release[0].set()
        native.until(lambda:fixture.project.joinpath('owned-leader').exists(),'actual admitted child leaf')
        pid=int(fixture.project.joinpath('owned-leader').read_text())
        leaf=dict(pid=pid,pgid=os.getpgid(pid),startticks=Path(f'/proc/{pid}/stat').read_text().split(') ')[1].split()[19])
        fixture.leaves.append(leaf)
        assert fixture.rows("SELECT count(*) FROM tool_operations WHERE name='shell' AND state='started'")==[(1,)]
        assert fixture.physical_requests==4 and len(fixture.parent_requests)==1
        fixture.send(b'\x03')
        native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='started'")==[(0,)],'all admitted work settled after cancel')
        native.until(lambda:not Path(f'/proc/{pid}').exists(),'owned actual leaf reaped')
        leaf['reaped']=True
        assert fixture.rows("SELECT status,count(*) FROM turns GROUP BY status")==[('cancelled',3)]
        assert fixture.rows("SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name")==[('shell','cancelled',1),('subagent','cancelled',2)]
        fixture.release[1].set()
        fixture.stop()
        counts=fixture.counts()
        assert fixture.physical_requests==4 and sum(row[2] for row in counts['dispatches'])==4
        fixture.start()
        fixture.stop()
        assert fixture.counts()==counts,'cancelled child replayed after reopen'
        assert not fixture.errors
        return dict(case='cancel',status='PASS',counts=counts,leaf=leaf,owners=fixture.owners,
                    pty_tail_bytes=len(fixture.tail),reopenNoReplay=True,laterEffects=0)

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('binaries',nargs='+',type=Path)
    args=parser.parse_args()
    association=source()
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    binaries=[path.resolve() for path in args.binaries]
    before={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    print(json.dumps(dict(before=before,head=head,source=association)),flush=True)
    for binary in binaries:
        for run in (reverse,cancel):
            print(json.dumps(dict(binary=str(binary),**run(binary))),flush=True)
    after={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert before==after and source()==association
    print(json.dumps(dict(after=after,source=association,cases=2*len(binaries),cleanup='owned native PGIDs/startticks, HTTP handlers and PTY readers joined/reaped before exact TempDir cleanup')),flush=True)

if __name__=='__main__':main()
