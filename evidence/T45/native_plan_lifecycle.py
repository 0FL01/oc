#!/usr/bin/env python3
"""Normal ELF Plan proof through existing TUI/headless/storage/tools, fake only."""
import argparse
import codecs
import ctypes
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sqlite3
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT/path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

native = load('native_background', 'evidence/T50/native_background.py')
sys.modules.setdefault('native_background', native)
ui = load('plan_terminal', 'evidence/T50/native_shell_controls.py')

def text(item):
    return ''.join(part.get('text','') for part in item.get('content',[]) if isinstance(part,dict))

class Plan(native.Native):
    consume_pty = ui.Consumer.consume_pty
    screen = ui.Consumer.screen

    def __init__(self, binary, family, constraint='allow', custom=False):
        self.phase, self.step = 'prime', 0
        self.family, self.constraint, self.custom = family, constraint, custom
        self.views, self.summary_posts = [], 0
        self.cli_receipts = []
        self.child_steps = 0
        self.switch_arrived, self.switch_release = threading.Event(), threading.Event()
        super().__init__(binary, self.response, {'edit':'allow','read':'allow','compress':'allow','shell':'deny'})
        config = Path(self.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
        if constraint.startswith('project-'):
            self.home = self.project/'owned-home'
            self.home.mkdir()
            self.env['HOME'] = str(self.home)
        self.plan = self.home/'.opencode/plan'
        if constraint=='no-home':
            self.env.pop('HOME')
            self.plan = self.project/'~/.opencode/plan'
        else:
            self.plan.mkdir(parents=True)
        self.file = self.plan/'requested.md'
        self.model = 'gpt-plan-fixture' if family == 'patch' else 'plan-fixture'
        settings = json.loads(config.read_text())
        settings['model'] = 'fixture/'+self.model
        settings['compaction'] = {'auto':False,'keep':{'tokens':0}}
        settings['provider']['fixture']['models'] = {name:{'name':'Plan Patch' if name.startswith('gpt-') else 'Plan Text','limit':{'context':500000,'output':4096}} for name in ('gpt-plan-fixture','plan-fixture')}
        if custom:
            settings['agent']['plan'] = {'system':'CUSTOM_PLAN_SYSTEM_ONLY'}
        if constraint == 'deny':
            settings['permission']['edit'] = 'deny'
        if constraint == 'ask':
            settings['permission']['edit'] = 'ask'
        if constraint == 'path-deny':
            settings['permission']['edit'] = {'*':'allow',str(self.file):'deny'}
        if constraint.startswith('project-path-'):
            settings['permission']['edit'] = {'*':'allow',str(self.file):constraint.removeprefix('project-path-')}
        if constraint.startswith('project-glob-'):
            settings['permission']['edit'] = {'*':'allow',str(self.project.parent/'*'/self.home.name/'.opencode/plan/*'):constraint.removeprefix('project-glob-')}
        if constraint.startswith('project-external-'):
            settings['permission']['external_directory'] = {'*':'allow',str(self.plan.relative_to(self.project)/'*'):constraint.removeprefix('project-external-')}
        if constraint == 'profile-deny':
            settings['agent']['plan'] = {'permission':{'edit':'deny'}}
        if constraint == 'external-deny':
            settings['permission']['external_directory'] = 'deny'
        if constraint == 'external-ask':
            settings['permission']['external_directory'] = 'ask'
        if constraint in ('parent-deny','parent-ask'):
            settings['permission']['subagent'] = 'allow'
            settings['agent']['plan'] = {'permission':{'edit':'deny' if constraint=='parent-deny' else 'ask'}}
            settings['agent']['helper'] = {'mode':'subagent','model':'fixture/'+self.model,'permission':{'edit':'allow'}}
        if constraint == 'wildcard-deny':
            settings['permission'] = {'*':'deny','read':'allow','compress':'allow'}
        if constraint == 'symlink':
            self.file.symlink_to(self.project/'ordinary')
        if constraint == 'root-symlink':
            parked = self.home/'.opencode-parked'
            (self.home/'.opencode').rename(parked)
            (self.home/'.opencode').symlink_to(parked)
        if constraint == 'data-root':
            self.data = self.plan/'data'
            self.file = self.data/'forbidden'
        settings['agent'].update({'disabled':{'disabled':True},'hidden-primary':{'hidden':True,'mode':'primary'},'all-primary':{'mode':'all'}})
        config.write_text(json.dumps(settings))

    def start(self):
        self.grid_lock = threading.Lock()
        self.grid = [[' ' for _ in range(110)] for _ in range(self.height)]
        self.cursor, self.pending = (0,0), ''
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        super().start()

    def __exit__(self, *args):
        self.switch_release.set()
        super().__exit__(*args)

    def response(self, _, number, request):
        if not request.get('tools'):
            self.summary_posts += 1
            return native.completed('## Objective\nContinue the explicitly requested planning discussion.\n## WorkState\nClosed investigation summarized; preserve the current selection.')
        self.step += 1
        family = 'patch' if request['model'].startswith('gpt-') else 'edit'
        child = any('CHILD_PLAN' in text(item) for item in request['input'] if item.get('role')=='user')
        reminders = [(i,text(item)) for i,item in enumerate(request['input'])
                     if item.get('role')=='system' and text(item).startswith('<system-reminder>\nYou are')]
        if self.phase not in ('prime','build','stale','hidden','all','grant-build') and not child:
            assert reminders and 'You are in Plan mode.' in reminders[-1][1], (self.phase,reminders)
            assert 'unless the user explicitly asks' in reminders[-1][1]
            assert 'or ask a subagent' in reminders[-1][1]
            if self.custom:
                assert 'CUSTOM_PLAN_SYSTEM_ONLY' in json.dumps(request['input'])
        elif self.phase in ('build','stale'):
            assert reminders and 'NO LONGER' in reminders[-1][1], reminders
        tools = {entry['name']:entry for entry in request['tools']}
        if self.constraint in ('allow','symlink','data-root','ask','path-deny','external-deny','external-ask'):
            expected = {'apply_patch'} if family=='patch' else {'write','edit'}
            assert set(tools).intersection({'apply_patch','write','edit'})==expected, (self.phase,list(tools))
        outputs = [item for item in request['input'] if item.get('type')=='function_call_output']
        self.views.append({'phase':self.phase,'step':self.step,'lane':'child' if child else 'main','model':request['model'],
                           'schemas':request['tools'],'reminders':reminders,'outputs':outputs})
        if self.phase == 'mutate':
            actions = [('write',{'path':str(self.file),'content':'first\n'}),
                       ('edit',{'path':str(self.file),'oldString':'first','newString':'second'}),
                       ('write',{'path':'ordinary','content':'FORBIDDEN'})]
            if self.family=='patch':
                actions = [('apply_patch',{'patchText':f'*** Begin Patch\n*** Add File: {self.file}\n+first\n*** End Patch'}),
                           ('apply_patch',{'patchText':f'*** Begin Patch\n*** Update File: {self.file}\n@@\n-first\n+second\n*** End Patch'}),
                           ('apply_patch',{'patchText':'*** Begin Patch\n*** Add File: ordinary\n+FORBIDDEN\n*** End Patch'})]
            if self.step <= len(actions):
                name,args = actions[self.step-1]
                return native.tool(name,args,'plan-'+str(self.step))
            assert self.file.read_text()=='second\n'
            assert not (self.project/'ordinary').exists()
            assert 'denied' in outputs[-1]['output'].lower(), outputs[-1]
        if self.phase == 'negative' and self.step == 1:
            if self.family == 'patch':
                return native.tool('apply_patch',{'patchText':f'*** Begin Patch\n*** Add File: {self.file}\n+FORBIDDEN\n*** End Patch'},'negative')
            return native.tool('write',{'path':str(self.file),'content':'FORBIDDEN'},'negative')
        if self.phase == 'parent':
            if child:
                self.child_steps += 1
                if self.child_steps == 1:
                    if self.family == 'patch':
                        return native.tool('apply_patch',{'patchText':f'*** Begin Patch\n*** Add File: {self.file}\n+FORBIDDEN\n*** End Patch'},'parent-negative')
                    return native.tool('write',{'path':str(self.file),'content':'FORBIDDEN'},'parent-negative')
            elif self.step == 1:
                return native.tool('subagent',{'agent':'helper','description':'Parent permission proof','prompt':'CHILD_PLAN explicitly request plan file mutation'},'delegate')
        if self.phase == 'preimage' and self.step == 1:
            if self.family == 'patch':
                return native.tool('apply_patch',{'patchText':f'*** Begin Patch\n*** Update File: {self.file}\n@@\n-before\n+FORBIDDEN\n*** End Patch'},'preimage')
            return native.tool('edit',{'path':str(self.file),'oldString':'before','newString':'FORBIDDEN'},'preimage')
        if self.phase == 'auto-ordinary' and self.step == 1:
            if family == 'patch':
                return native.tool('apply_patch',{'patchText':'*** Begin Patch\n*** Add File: ordinary\n+FORBIDDEN\n*** End Patch'},'auto-ordinary')
            return native.tool('write',{'path':'ordinary','content':'FORBIDDEN'},'auto-ordinary')
        if self.phase == 'switch' and self.step <= 2:
            if self.step == 1:
                self.switch_arrived.set()
                assert self.switch_release.wait(15),'owned prepared request barrier expired'
            if family == 'patch':
                return native.tool('apply_patch',{'patchText':'*** Begin Patch\n*** Add File: ordinary\n+FORBIDDEN\n*** End Patch'},'switch-'+str(self.step))
            return native.tool('write',{'path':'ordinary','content':'FORBIDDEN'},'switch-'+str(self.step))
        if self.phase in ('grant-build','grant-plan') and self.step == 1:
            if family == 'patch':
                patch = '*** Begin Patch\n*** Add File: ordinary\n+before\n*** End Patch' if self.phase=='grant-build' else '*** Begin Patch\n*** Update File: ordinary\n@@\n-before\n+FORBIDDEN\n*** End Patch'
                return native.tool('apply_patch',{'patchText':patch},self.phase)
            return native.tool('write',{'path':'ordinary','content':'before\n' if self.phase=='grant-build' else 'FORBIDDEN'},self.phase)
        if self.phase == 'dcp' and self.step == 1:
            prefix = 'DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: '
            anchors = next(json.loads(text(item)[len(prefix):]) for item in request['input'] if text(item).startswith(prefix))
            closed = [anchor for anchor in anchors if anchor['closed']]
            assert len(closed)>=1
            return native.tool('compress',{'topic':'closed plan work','content':[{'startId':closed[0]['id'],'endId':closed[-1]['id'],'summary':'## Objective\nDiscuss the requested plan.\n## WorkState\nThe explicitly requested file was updated; ordinary files remain unchanged.'}]},'plan-compress')
        return native.completed(('closed investigation '*300)+'\nDONE_'+self.phase)

    def settled(self, phase, prompt=None):
        self.phase, self.step = phase, 0
        before = self.rows("SELECT count(*) FROM turns WHERE status='completed'")[0][0]
        self.tail.clear()
        self.send(((prompt or 'Discuss the current plan PHASE_'+phase)+'\r').encode())
        native.until(lambda:self.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(before+1,)] or self.errors,
                     ('turn did not settle',phase),seconds=20)
        assert not self.errors,self.errors
        native.until(lambda:self.finished_frame(phase), ('finished turn footer not painted',phase))

    def finished_frame(self, phase):
        screen = '\n'.join(self.screen())
        marker = 'DONE_'+phase
        if marker not in screen: return False
        # Existing durable completed-footer ACK, adapted to this fixture's
        # model labels. Body streaming/DB settlement do not acknowledge keys.
        return re.search(r'(?:Build|Plan)\s*·\s*[^·\n]+\s*·\s*\d',screen.split(marker,1)[1])

    def agent(self, id):
        self.tail.clear()
        self.send(b'/agents\r')
        native.until(lambda: 'Agents' in '\n'.join(self.screen()),'agents picker not painted')
        self.send(id.encode())
        time.sleep(.1)
        self.send(b'\r')
        native.until(lambda:self.rows("SELECT json_extract(value,'$.agent') FROM prefs WHERE key LIKE 'tui.selection.session:%'")==[(id,)],'selection not committed')
        time.sleep(.1)

    def raw(self):
        return self.rows('SELECT id,seq,role,text FROM messages ORDER BY seq')

    def switch(self):
        self.phase,self.step = 'switch',0
        before = self.rows("SELECT count(*) FROM turns WHERE status='completed'")[0][0]
        reminders = [row for row in self.raw() if row[2]=='system']
        self.send(b'Implement immediately in the ordinary file; preserve the current task\r')
        assert self.switch_arrived.wait(10),self.errors
        target = 'plan-fixture' if self.model.startswith('gpt-') else 'gpt-plan-fixture'
        label = 'Plan Text' if target=='plan-fixture' else 'Plan Patch'
        self.send(b'/model\r')
        native.until(lambda:'Select model' in '\n'.join(self.screen()),'busy model picker not painted')
        self.send(label.encode()+b'\r')
        native.until(lambda:'Select model' not in '\n'.join(self.screen()),'model draft not painted')
        self.send(b'\r')
        native.until(lambda:self.rows("SELECT json_extract(value,'$.models.plan.id') FROM prefs WHERE key LIKE 'tui.selection.session:%'")==[(target,)],'busy model selection not committed')
        assert [row for row in self.raw() if row[2]=='system']==reminders,'model selection left Plan'
        self.switch_release.set()
        native.until(lambda:self.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(before+1,)] or self.errors,'live-switch turn did not settle',seconds=20)
        assert not self.errors,self.errors
        assert not (self.project/'ordinary').exists()
        native.until(lambda:'DONE_switch' in '\n'.join(self.screen()),'switch not painted')
        time.sleep(.1)

    def compact(self):
        before = self.rows('SELECT count(*) FROM session_compactions')[0][0]
        self.send(b'/compact\r')
        native.until(lambda:self.rows('SELECT count(*) FROM session_compactions')[0][0]>before,'compact not admitted')
        native.until(lambda:self.rows("SELECT json_extract(snapshot,'$.state') FROM session_compactions ORDER BY rowid DESC LIMIT 1")==[('completed',)] or self.errors,'compact not complete',seconds=20)
        assert not self.errors,self.errors
        time.sleep(.1)

    def run_cli(self, phase, *flags, expected=0):
        self.phase,self.step = phase,0
        command = [str(self.binary),'--data-dir',str(self.data),'run','--session','t50-background','--json',*flags,'Explicitly create/update the requested plan file; discuss implementation']
        process = subprocess.Popen(command,
                                   cwd=self.project,env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        try:
            out,err = process.communicate(timeout=20)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
        assert process.returncode==expected,(phase,out.decode(errors='replace'),err.decode(errors='replace'))
        assert not self.errors,self.errors
        receipt = {'phase':phase,'command':command,'exit':process.returncode,'stdout':out.decode(),'stderr':err.decode()}
        self.cli_receipts.append(receipt)
        return receipt

def lifecycle(binary,family,custom):
    with Plan(binary,family,custom=custom) as fixture:
        fixture.start()
        fixture.settled('prime','Investigate the codebase before implementation')
        fixture.agent('plan')
        entered = fixture.raw()
        assert sum('You are in Plan mode.' in row[3] for row in entered)==1
        assert list(fixture.plan.iterdir())==[]
        fixture.agent('plan')
        assert fixture.raw()==entered,'identical selection appended reminder'
        fixture.settled('mutate','Explicitly create the plan file '+str(fixture.file)+' with first, then edit first to second; do not modify ordinary files')
        originals = fixture.raw()
        original_logs = fixture.rows('SELECT id,result FROM turns ORDER BY rowid')
        fixture.switch()
        fixture.settled('dcp')
        fixture.settled('after-dcp','Implement the changes now')
        fixture.compact()
        fixture.settled('after-compact')
        fixture.stop()
        posts = fixture.physical_requests
        fixture.start()
        assert fixture.physical_requests==posts,'restart generated without prompt'
        fixture.settled('reopened')
        fixture.agent('build')
        fixture.settled('build')
        # Two real conversation-only undo operations hide the leave from the
        # retained view while keeping the committed Build selection and RAW.
        fixture.send(b'/undo\r')
        native.until(lambda:fixture.rows('SELECT upper_seq FROM conversation_state')!=[(None,)],'first undo did not apply')
        time.sleep(.15)
        fixture.send(b'/undo\r')
        time.sleep(.15)
        fixture.settled('stale')
        after = fixture.raw()
        assert after[:len(originals)]==originals,'RAW rows rewritten'
        assert fixture.rows('SELECT id,result FROM turns ORDER BY rowid')[:len(original_logs)]==original_logs,'canonical closed RAW TurnLogs rewritten'
        assert fixture.file.read_text()=='second\n' and not (fixture.project/'ordinary').exists()
        fixture.stop()
        fixture.run_cli('plan-cli','--agent','plan')
        fixture.run_cli('saved-cli')
        fixture.run_cli('auto-ordinary','--auto')
        assert not (fixture.project/'ordinary').exists()
        config = fixture.home/'config/opencode/opencode.json'
        original_config = config.read_text()
        settings = json.loads(original_config)
        settings['agent']['plan'] = {'disabled':True}
        config.write_text(json.dumps(settings))
        before = fixture.physical_requests
        acceptances = fixture.rows('SELECT count(*) FROM turn_acceptances')
        fixture.run_cli('invalid-saved',expected=1)
        assert fixture.physical_requests==before and fixture.rows('SELECT count(*) FROM turn_acceptances')==acceptances
        fixture.run_cli('build','--agent','build')
        config.write_text(original_config)
        for invalid in ('unknown','disabled','general','explore'):
            before = fixture.physical_requests
            acceptances = fixture.rows('SELECT count(*) FROM turn_acceptances')
            fixture.run_cli('invalid','--agent',invalid,expected=1)
            assert fixture.physical_requests==before and fixture.rows('SELECT count(*) FROM turn_acceptances')==acceptances
        fixture.run_cli('hidden','--agent','hidden-primary')
        fixture.run_cli('all','--agent','all-primary')
        dispatches = dict(fixture.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        assert sum(dispatches.values())==fixture.physical_requests
        result = {'family':family,'custom':custom,'physical_posts':fixture.physical_requests,'dispatches':dispatches,
                  'summary_posts':fixture.summary_posts,'file_creates':1,'file_edits':1,'ordinary_effects':0,
                  'raw_prefix_sha256':hashlib.sha256(json.dumps(originals).encode()).hexdigest(),
                  'raw_turnlog_prefix_sha256':hashlib.sha256(json.dumps(original_logs).encode()).hexdigest(),
                  'tool_states':fixture.rows('SELECT name,state,count(*) FROM tool_operations GROUP BY name,state'),
                  'reminder_rows':[(seq,value) for _,seq,role,value in fixture.raw() if role=='system'],
                  'cli_receipts':fixture.cli_receipts,
                  'captured_views':fixture.views}
    return result

def negative(binary,family,constraint):
    with Plan(binary,family,constraint) as fixture:
        fixture.run_cli('negative','--agent','plan',expected=1 if constraint in ('ask','external-ask','project-path-ask','project-glob-ask','project-external-ask') else 0)
        assert not fixture.file.exists() and not (fixture.project/'ordinary').exists()
        if constraint=='no-home': assert not fixture.plan.exists(), 'no captured HOME created a literal tilde Plan directory'
        return {'family':family,'constraint':constraint,'physical_posts':fixture.physical_requests,'effects':0,
                'tool_states':fixture.rows('SELECT name,state,count(*) FROM tool_operations GROUP BY name,state'),
                'captured_views':fixture.views}

def preimage(binary,family):
    with Plan(binary,family,'ask') as fixture:
        fixture.file.write_text('before\n')
        fixture.start()
        fixture.agent('plan')
        fixture.phase,fixture.step = 'preimage',0
        fixture.send(b'Explicitly edit the requested plan file\r')
        native.until(lambda:'Allow once' in '\n'.join(fixture.screen()),'actual Ask preview not painted')
        assert fixture.rows('SELECT count(*) FROM tool_operations')==[(0,)]
        assert fixture.file.read_text()=='before\n'
        fixture.file.write_text('before\nforeign\n')
        fixture.send(b'\r')
        native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(1,)] or fixture.errors,'stale Ask did not settle',seconds=20)
        assert not fixture.errors,fixture.errors
        assert fixture.file.read_text()=='before\nforeign\n'
        states = fixture.rows('SELECT name,state,output FROM tool_operations')
        assert len(states)==1 and states[0][1]=='failed' and ('changed' in states[0][2] or 'preimage' in states[0][2]),states
        return {'family':family,'case':'approved-preimage','effects':0,'physical_posts':fixture.physical_requests,
                'tool_states':states,'captured_views':fixture.views}

def parent(binary,family,constraint):
    with Plan(binary,family,constraint) as fixture:
        fixture.run_cli('parent','--agent','plan')
        assert fixture.child_steps>=1,'no actual functional child consumer'
        assert not fixture.file.exists() and not (fixture.project/'ordinary').exists()
        return {'family':family,'case':constraint,'effects':0,'physical_posts':fixture.physical_requests,
                'child_requests':fixture.child_steps,
                'dispatches':fixture.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"),
                'tool_states':fixture.rows('SELECT name,state,output FROM tool_operations'),
                'captured_views':fixture.views}

def saved_grant(binary,family):
    with Plan(binary,family,'ask') as fixture:
        fixture.start()
        fixture.phase,fixture.step = 'grant-build',0
        fixture.send(b'Explicitly create the ordinary Build file\r')
        native.until(lambda:'Always allow' in '\n'.join(fixture.screen()),'saved-grant Ask consumer not painted')
        fixture.send(b'\x1b[C\r')
        native.until(lambda:fixture.rows('SELECT count(*) FROM permission_grants')==[(1,)],'actual Always grant not persisted')
        native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(1,)] or fixture.errors,'Build grant turn did not settle')
        native.until(lambda:fixture.finished_frame('grant-build'),'Build grant durable completed footer not painted')
        assert not fixture.errors,fixture.errors
        assert (fixture.project/'ordinary').read_text()=='before\n'
        fixture.agent('plan')
        fixture.settled('grant-plan','Implement the ordinary file immediately with the saved grant')
        assert (fixture.project/'ordinary').read_text()=='before\n'
        assert not fixture.file.exists()
        states = fixture.rows('SELECT name,state,output FROM tool_operations ORDER BY rowid')
        assert len(states)==2 and states[0][1]=='completed' and states[1][1]=='failed' and 'denied' in states[1][2],states
        return {'family':family,'case':'saved-Build-grant-cannot-widen-Plan','Build_effects':1,'Plan_effects':0,
                'physical_posts':fixture.physical_requests,'grants':fixture.rows('SELECT action,pattern FROM permission_grants'),
                'tool_states':states,'captured_views':fixture.views}

def discovery(binary,family,constraint='allow'):
    with Plan(binary,family,constraint) as fixture:
        config_trap = fixture.plan/'opencode.json'
        instruction_trap = fixture.plan/'AGENTS.md'
        config_trap.write_text('{ invalid plan config must not be discovered')
        instruction_trap.write_text('PLAN_DIRECTORY_IS_NOT_AN_INSTRUCTION_ROOT')
        libc = ctypes.CDLL(None,use_errno=True)
        watch = libc.inotify_init1(0x800 | 0x80000)
        assert watch>=0
        try:
            for path in (config_trap,instruction_trap):
                assert libc.inotify_add_watch(watch,str(path).encode(),0x20)>=0
            fixture.run_cli('mutate','--agent','plan','--auto')
            assert fixture.file.read_text()=='second\n'
            assert 'PLAN_DIRECTORY_IS_NOT_AN_INSTRUCTION_ROOT' not in json.dumps(fixture.views)
            import os
            try:
                events = os.read(watch,65536)
            except BlockingIOError:
                events = b''
            assert not events,'Plan invocation root caused config/instruction body opens'
        finally:
            import os
            os.close(watch)
        return {'family':family,'case':'invocation-root-not-discovery','constraint':constraint,'config_instruction_opens':0,
                'physical_posts':fixture.physical_requests,'file_creates':1,'file_edits':1,'ordinary_effects':0,
                'captured_views':fixture.views}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binaries',nargs='+',type=Path)
    parser.add_argument('--only',choices=['lifecycle','negative','project-negative','no-home'])
    args = parser.parse_args()
    binaries = [path.resolve() for path in args.binaries]
    hashes = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    print(json.dumps({'before':hashes}),flush=True)
    for binary in binaries:
        if args.only not in ('negative','project-negative','no-home'):
            for family,custom in [('edit',False),('patch',False),('edit',True)]:
                print(json.dumps({'binary':str(binary),'case':'lifecycle',**lifecycle(binary,family,custom)}),flush=True)
        if args.only != 'lifecycle':
            for family in ('edit','patch'):
                project_constraints = ('project-path-deny','project-path-ask','project-glob-deny','project-glob-ask','project-external-deny','project-external-ask')
                constraints = ('no-home',) if args.only=='no-home' else project_constraints if args.only=='project-negative' else ('deny','path-deny','profile-deny','external-deny','wildcard-deny','ask','external-ask','symlink','root-symlink','data-root',*project_constraints,'no-home')
                for constraint in constraints:
                    print(json.dumps({'binary':str(binary),'case':'negative',**negative(binary,family,constraint)}),flush=True)
                if args.only in ('project-negative','no-home'): continue
                print(json.dumps({'binary':str(binary),**preimage(binary,family)}),flush=True)
                for constraint in ('parent-deny','parent-ask'):
                    print(json.dumps({'binary':str(binary),**parent(binary,family,constraint)}),flush=True)
                print(json.dumps({'binary':str(binary),**saved_grant(binary,family)}),flush=True)
                print(json.dumps({'binary':str(binary),**discovery(binary,family)}),flush=True)
                print(json.dumps({'binary':str(binary),**discovery(binary,family,'project-path-allow')}),flush=True)
    after = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert after==hashes
    print(json.dumps({'after':after,'cleanup':'all owned process/readers/HTTP handlers joined; exact TempDirs removed'}),flush=True)

if __name__=='__main__':
    main()
