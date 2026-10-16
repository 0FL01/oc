#!/usr/bin/env python3
"""Actual normal ELF, live PTY, loopback protocol barriers and durable counters."""
import codecs
import hashlib
import json
import os
from pathlib import Path
import sys
import threading
import time

from native_foreground_children import Foreground, native, call, items
from native_shell_controls import Consumer


class Controls(Foreground):
    consume_pty = Consumer.consume_pty
    screen = Consumer.screen

    def __init__(self, binary, case):
        super().__init__(binary, case)
        self.convert = case != 'cancel'
        self.shell_route = case == 'shell-route'
        self.child_followup = threading.Event()
        self.grid_lock = threading.Lock()
        self.grid = [[" " for _ in range(110)] for _ in range(self.height)]
        self.cursor = (0, 0)
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        config = Path(self.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
        settings = json.loads(config.read_text())
        settings['permission'].update({'opencode_session_move':'allow','external_directory':'allow'})
        config.write_text(json.dumps(settings))
        self.destination = self.project/'destination'
        self.destination.mkdir()
        self.destination.joinpath('AGENTS.md').write_text('OWN_DESTINATION_GUIDANCE')

    def painted(self):
        return '\n'.join(self.screen())

    def response(self, _, number, request):
        pairs = items(request)
        if request['model'] == 'm':
            with self.lock: self.parent_requests.append(request)
            if self.convert and len(self.parent_requests) >= 3:
                return native.completed('synthetic source safe terminal output')
            if not pairs:
                return (call('subagent',{'agent':'maker','description':'A','prompt':'CHILD_A_TASK'},'original-call-A')
                        +call('subagent',{'agent':'reader','description':'B','prompt':'CHILD_B_TASK','model':'fixture/child-b'},'original-call-B')
                        +[{'type':'response.completed','response':{'status':'completed'}}])
            assert self.release[1].is_set(), 'parent crossed still-foreground barrier'
            self.followup.set()
            if self.convert and len(pairs) == 2:
                assert not self.release[0].is_set(), 'background work restarted or finished early'
                assert json.loads(pairs[0][1])['status'] == 'running'
                return native.tool('opencode_session_move',{'sessionID':'t50-background','directory':str(self.destination)},'actual-parent-move')
            assert pairs[1][1].endswith('</subagent>')
            return native.completed('synthetic parent output')
        index = 0 if request['model'] == 'gpt-child-a' else 1
        assert request['model'] in ('gpt-child-a','child-b')
        if not pairs:
            self.child_requests[index] = request
            self.arrived[index].set()
            if index == 1:
                assert self.release[1].wait(10), 'owned provider barrier watchdog'
                return native.completed('synthetic reader output')
            return ([{'type':'response.reasoning_summary_text.delta','delta':'OWN_LIVE_REASONING'}]
                    +native.tool('shell',{'command':'printf "%s" $$ > owned-leader; pwd > child-cwd; printf "OWN_SHELL_LIVE\\n"; while [ ! -f release-a ]; do sleep 0.02; done; printf a >> once'},'owned-child-leaf'))
        assert index == 0 and self.convert
        assert pairs[0][0] == 'owned-child-leaf'
        if self.shell_route:
            assert json.loads(pairs[0][1])['status'] == 'running'
            self.child_followup.set()
            assert self.release[0].wait(10), 'owned issued child continuation deadline'
        else:
            assert self.project.joinpath('once').read_bytes() == b'a'
        assert 'OWN_WORKSPACE_FOREGROUND_GUIDANCE' in json.dumps(request)
        assert 'OWN_DESTINATION_GUIDANCE' not in json.dumps(request)
        return native.completed('error: typed successful child output')

    def wait_live(self):
        assert all(e.wait(10) for e in self.arrived)
        native.until(lambda: self.project.joinpath('owned-leader').exists(), 'actual shell leaf absent')
        native.until(lambda: self.auxiliary_requests == 1, 'actual auxiliary dispatch')
        assert self.physical_requests == 4 and len(self.requests) == 3
        self.children = self.rows('SELECT id,parent_id,agent,model FROM sessions WHERE parent_id IS NOT NULL ORDER BY rowid')
        assert len(self.children) == 2
        assert self.rows("SELECT state,count(*) FROM child_jobs GROUP BY state") == [('running',2)]
        assert self.rows('SELECT count(*) FROM turns') == [(3,)]
        assert self.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'") == [(4,)]
        for i,req in enumerate(self.child_requests):
            text = json.dumps(req)
            assert ('OWN_MAKER_SYSTEM' if i == 0 else 'OWN_READER_SYSTEM') in text
            assert 'PARENT_PRIVATE_TRANSCRIPT' not in text and 'PARENT_SYSTEM_PRIVATE' not in text
        pid = int(self.project.joinpath('owned-leader').read_text())
        stat = Path(f'/proc/{pid}/stat').read_text().split(') ')[1].split()
        self.leaves.append(dict(pid=pid,pgid=os.getpgid(pid),startticks=stat[19]))
        return pid

    def menu(self):
        if 'Subagents' not in self.painted(): self.send(b'\x07')
        native.until(lambda: 'Subagents' in self.painted(), 'normal binary missing child consumer')

    def select_a(self):
        for _ in range(2):
            if any('› ' in row and ' A · maker' in row for row in self.screen()): return
            self.send(b'\x1b[B')
            time.sleep(.05)
        raise AssertionError('owner-backed selected A absent')

    def stop(self,*args,**kwargs):
        if self.process is not None and self.process.poll() is None:
            for _ in range(6):
                if self.process.poll() is not None: break
                self.send(b'\x1b')
                time.sleep(.05)
        super().stop(*args,**kwargs)

    def __exit__(self,*args):
        self.project.joinpath('release-a').touch()
        super().__exit__(*args)


def controls(binary, case):
    with Controls(binary,case) as f:
        f.start()
        f.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        pid = f.wait_live()
        operations = f.rows('SELECT operation_id,child_id,child_turn,identity FROM child_jobs ORDER BY rowid')
        f.send(b'ROOT_DRAFT_RETAINED')
        f.menu()
        unmatched=b'no-such-selected-row'
        f.send(unmatched)
        native.until(lambda:'No child jobs' in f.painted(),'passive filter did not hide rows')
        f.send(b'\x02\x03')
        assert f.physical_requests==4 and f.rows('SELECT state FROM child_jobs ORDER BY rowid')==[('running',),('running',)]
        f.send(b'\x7f'*len(unmatched))
        native.until(lambda:' A · maker' in f.painted(),'owner rows not restored after passive filter')
        f.select_a()
        row=next(i+1 for i,line in enumerate(f.screen()) if '› ' in line and ' A · maker' in line)
        f.send(f'\x1b[<0;20;{row}M\x1b[<0;20;{row}m'.encode())
        native.until(lambda:'Linked child' in f.painted() and 'CHILD_A_TASK' in f.painted(),'actual linked child did not open')
        assert not f.followup.is_set() and Path(f'/proc/{pid}').exists()
        selection=f.rows('SELECT id,parent_id,agent,model FROM sessions ORDER BY rowid')
        f.send(b'ARBITRARY_CHILD_TURN\r')
        native.until(lambda:'read-only' in f.painted(),'linked new turn was not visibly refused')
        assert f.physical_requests==4 and f.rows('SELECT count(*) FROM turns')==[(3,)]
        assert f.rows('SELECT id,parent_id,agent,model FROM sessions ORDER BY rowid')==selection
        f.send(b'\x10'); f.send(b'Expand thinking')
        native.until(lambda:'Expand thinking' in f.painted(),'existing thinking command absent')
        f.send(b'\r')
        native.until(lambda:'OWN_LIVE_REASONING' in f.painted(),'actual linked reasoning absent')
        # Real shell consumer has priority over child Ctrl+B.
        f.send(b'\x13')
        native.until(lambda:'Shell ·' in f.painted(),'linked actual shell consumer absent')
        f.send(b'\r')
        native.until(lambda:'OWN_SHELL_LIVE' in f.painted(),'actual leaf read absent')
        if f.shell_route:
            f.send(b'\x02\x02')
            native.until(lambda:f.child_followup.is_set(),'real selected Shell Ctrl+B did not release leaf waiter')
            assert f.rows("SELECT count(*) FROM events WHERE kind='shell_background'")==[(1,)]
            assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_background'")==[(0,)]
            assert f.rows('SELECT state FROM child_jobs ORDER BY rowid')==[('running',),('running',)]
            assert Path(f'/proc/{pid}').exists() and f.physical_requests==5
        f.send(b'\x1b')
        time.sleep(.05)
        f.send(b'\x1b')
        time.sleep(.05)
        # Return preserves the real root draft; opening/hiding did not dispatch.
        f.send(b'\x1b')
        native.until(lambda:'ROOT_DRAFT_RETAINED' in f.painted(),'root draft lost')
        before_control=5 if f.shell_route else 4
        assert f.physical_requests == before_control and not f.followup.is_set()
        assert f.rows('SELECT id,parent_id,agent,model FROM sessions ORDER BY rowid')==selection
        f.menu(); f.select_a()
        f.send(b'\x02' if f.convert else b'\x03')
        if f.convert:
            native.until(lambda:f.rows("SELECT count(*) FROM events WHERE kind='subagent_background'")==[(1,)],'same-child Ctrl+B failed')
            f.send(b'\x02')
            assert f.rows('SELECT operation_id,child_id,child_turn,identity FROM child_jobs ORDER BY rowid') == operations
            assert Path(f'/proc/{pid}').exists() and f.physical_requests == before_control
        else:
            native.until(lambda:f.rows("SELECT state FROM child_jobs ORDER BY rowid")[0]==('cancelled',),'selected child did not settle cancelled')
            assert not Path(f'/proc/{pid}').exists(), 'owned leaf not reaped'
            f.send(b'\x03')
        assert not f.followup.is_set()
        assert f.rows("SELECT state FROM child_jobs ORDER BY rowid")[1] == ('running',)
        f.release[1].set()
        try:
            native.until(lambda: f.followup.is_set() and not f.rows("SELECT status FROM turns WHERE session_id='t50-background' AND status='started'"), 'parent did not continue after only FG peer completed')
        except AssertionError:
            print(json.dumps(dict(counts=f.counts(),errors=f.errors)),flush=True)
            raise
        if f.convert:
            assert f.rows('SELECT phase FROM session_moves') == [('pending',)]
            assert f.rows('SELECT value FROM prefs WHERE key=?',['tui.session_location.t50-background']) == [(str(f.project),)]
            assert Path(f'/proc/{pid}').exists(), 'pending move killed owned child'
            f.menu(); f.select_a(); f.send(b'\r')
            native.until(lambda:'Linked child' in f.painted(),'captured-source viewer before actual move absent')
            f.release[0].set()
            if not f.shell_route: f.project.joinpath('release-a').touch()
            try:
                native.until(lambda:f.rows('SELECT value FROM prefs WHERE key=?',['tui.session_location.t50-background']) == [(str(f.destination),)], 'actual deferred parent move not applied')
            except AssertionError:
                print(json.dumps(dict(counts=f.counts(),moves=f.rows('SELECT * FROM session_moves'),errors=f.errors,screen=f.painted())),flush=True)
                raise
            assert f.rows('SELECT value FROM prefs WHERE key=?',['tui.session_location.t50-background']) == [(str(f.destination),)]
            assert f.rows('SELECT value FROM prefs WHERE key=?',['tui.session_location.'+f.children[0][0]]) == [(str(f.project),)]
            assert 'Linked child' in f.painted(), 'parent relocation replaced pinned child viewer'
            if f.shell_route:
                assert Path(f'/proc/{pid}').exists(), 'actual selected Shell was retargeted during parent move'
                f.project.joinpath('release-a').touch()
            native.until(lambda:f.rows("SELECT state,message_id FROM child_jobs ORDER BY rowid")[0][0]=='completed' and f.rows("SELECT state,message_id FROM child_jobs ORDER BY rowid")[0][1] is not None,'one durable background settlement absent')
            native.until(lambda:'typed successful child output' in f.painted(),'actual child text/terminal was not reconciled into linked view')
            assert f.painted().count('typed successful child output')==1
            assert f.rows("SELECT count(*) FROM messages WHERE session_id=? AND role='assistant' AND text=?",[f.children[0][0],'error: typed successful child output'])==[(1,)]
            if f.shell_route:
                native.until(lambda:f.rows("SELECT count(*) FROM events WHERE kind='shell_notice'")==[(1,)] and not Path(f'/proc/{pid}').exists(),'owned source Shell late completion/reap absent')
            assert f.project.joinpath('once').read_bytes() == b'a'
            assert not f.destination.joinpath('once').exists()
        else:
            f.release[0].set()
            assert not f.project.joinpath('once').exists()
        assert f.project.joinpath('child-cwd').read_text().strip() == str(f.project)
        notice_count = f.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'")[0][0]
        assert notice_count == (1 if f.convert else 0)
        if f.convert:
            f.menu(); f.select_a(); f.send(b'\x02')
        if f.convert:
            assert f.rows("SELECT (SELECT max(rowid) FROM events WHERE kind='session_moved') < (SELECT min(rowid) FROM events WHERE kind='subagent_notice')") == [(1,)], 'late notice did not retain source after actual move'
        expected = 7 if f.convert else 5
        assert f.physical_requests == expected
        assert f.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'") == [(expected,)]
        tool_counts=f.rows("SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name,state")
        assert tool_counts == ([('opencode_session_move','completed',1),('shell','completed',1),('subagent','completed',1),('subagent','running',1)] if f.convert else [('shell','cancelled',1),('subagent','cancelled',1),('subagent','completed',1)]), tool_counts
        f.send(b'\x1b'); time.sleep(.05)
        f.stop()
        before = f.physical_requests
        # Reopen original Location even after actual root move using linked read.
        if f.convert: f.project = f.destination
        f.start(); f.menu(); f.select_a()
        native.until(lambda:'COMPLETED' in f.painted(),'restart current-state absent')
        f.send(b'\r')
        native.until(lambda:'Linked child' in f.painted(),'restart linked read absent')
        if f.convert:
            for _ in range(32):
                if 'typed successful child output' in f.painted(): break
                f.send(b'\x1b[<64;20;10M'); time.sleep(.05)
            native.until(lambda:'typed successful child output' in f.painted(),'restart terminal text absent: '+f.painted())
            assert f.painted().count('typed successful child output')==1
        assert f.physical_requests == before
        f.stop()
        assert not f.errors, f.errors
        return dict(case=case,status='PASS',physical=f.physical_requests,auxiliary=f.auxiliary_requests,
                    dispatches=f.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")[0][0],
                    notices=notice_count,conversion_events=f.rows("SELECT count(*) FROM events WHERE kind='subagent_background'")[0][0],
                     child_operations=2,shell_operations=1,leaf=f.leaves,owners=f.owners,identity_unchanged=True,reopen_rpc=0)


class TerminalRace(Controls):
    def response(self, owner, number, request):
        if request['model'] == 'm' and items(request):
            with self.lock: self.parent_requests.append(request)
            assert self.release[1].is_set(), 'terminal race crossed foreground sibling barrier'
            self.followup.set()
            return native.completed('synthetic terminal race parent')
        return super().response(owner, number, request)


def terminal_race(binary):
    with TerminalRace(binary, 'terminal-race') as f:
        f.start(); f.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        pid=f.wait_live()
        identity=f.rows('SELECT operation_id,child_id,child_turn,identity FROM child_jobs ORDER BY rowid')
        f.menu(); f.select_a()
        barrier=threading.Barrier(2)
        errors=[]
        def release_leaf():
            try:
                barrier.wait(timeout=1)
                f.project.joinpath('release-a').touch()
            except BaseException as error: errors.append(repr(error))
        release=threading.Thread(target=release_leaf)
        release.start()
        try:
            barrier.wait(timeout=1)
            f.send(b'\x02')
        finally:
            release.join(timeout=1)
            assert not release.is_alive() and not errors, errors
        native.until(lambda:f.rows('SELECT state FROM child_jobs ORDER BY rowid')[0]==('completed',),'actual terminal/control race did not settle')
        assert not f.followup.is_set()
        assert f.rows('SELECT state FROM child_jobs ORDER BY rowid')[1]==('running',)
        converted=f.rows("SELECT count(*) FROM events WHERE kind='subagent_background'")[0][0]
        assert converted in (0,1)
        assert f.rows('SELECT operation_id,child_id,child_turn,identity FROM child_jobs ORDER BY rowid')==identity
        assert not Path(f'/proc/{pid}').exists()
        assert f.project.joinpath('once').read_bytes()==b'a'
        # The selected pre-terminal generation is now stale for active controls.
        # Both controls must reject it, not broaden cancellation to the parent/B.
        f.send(b'\x02\x03\x02\x03')
        f.release[1].set()
        native.until(lambda:f.followup.is_set() and not f.rows("SELECT status FROM turns WHERE status='started'"),'stale selected control cancelled unrelated work')
        native.until(lambda:f.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'")==[(converted,)],'terminal-race notice count')
        assert f.rows('SELECT state FROM child_jobs ORDER BY rowid')==[('completed',),('completed',)]
        assert f.rows("SELECT name,state,count(*) FROM tool_operations GROUP BY name,state ORDER BY name,state")==([('shell','completed',1),('subagent','completed',1),('subagent','running',1)] if converted else [('shell','completed',1),('subagent','completed',2)])
        assert f.physical_requests==6 and f.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")==[(6,)]
        f.stop(); before=f.physical_requests
        f.start(); f.menu(); f.send(b'\r')
        native.until(lambda:'Linked child' in f.painted(),'terminal-race restart read absent')
        assert f.physical_requests==before and f.project.joinpath('once').read_bytes()==b'a'
        f.stop(); assert not f.errors, f.errors
        return dict(case='terminal-race-stale-controls',status='PASS',physical=6,auxiliary=1,
                    dispatches=6,conversion_events=converted,notices=converted,effects=1,
                    child_operations=2,shell_operations=1,stale_controls=4,reopen_rpc=0,
                    identity_unchanged=True,owners=f.owners,leaf=f.leaves)


if __name__ == '__main__':
    for raw in sys.argv[1:]:
        binary=Path(raw).resolve()
        before=hashlib.sha256(binary.read_bytes()).hexdigest()
        for case in ('convert','cancel','shell-route'): print(json.dumps(controls(binary,case)),flush=True)
        print(json.dumps(terminal_race(binary)),flush=True)
        after=hashlib.sha256(binary.read_bytes()).hexdigest()
        assert before == after
        print(json.dumps(dict(binary=str(binary),before=before,after=after,status='PASS')),flush=True)
