#!/usr/bin/env python3
"""Normal native ELFs, owned loopback barriers/effects/recovery; synthetic HOME.

PTy is an input/ownership driver here, not a visual acceptance claim. Existing
fixture handlers are non-daemon and server_close joins them before TempDir exit.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import threading

from native_foreground_children import Foreground, call, items, native, owner
from run_background_check import source

ROOT = Path(__file__).resolve().parents[2]


class Background(Foreground):
    def __init__(self, binary, case):
        self.parent_held = threading.Event()
        self.parent_release = threading.Event()
        self.retry_unknown = False
        self.reader_fails = False
        super().__init__(binary, case)

    def response(self, _, number, request):
        pairs = items(request)
        if request['model'] == 'm':
            with self.lock:
                step = len(self.parent_requests)
                self.parent_requests.append(request)
            if step == 0:
                # Observe the real parent running-result COMMIT boundary. This
                # fixture trigger cannot accept a callback/JSON-only child: its
                # actual native turn, accepted user input and owner generation
                # must already be durable. It neither waits for model output nor
                # manufactures the stronger peer-POST-arrival ordering claim.
                with sqlite3.connect(self.data/'oc.sqlite') as db:
                    db.executescript("""
                    CREATE TABLE fixture_owned_start_order(
                        operation TEXT PRIMARY KEY, child_turn TEXT, user_message TEXT,
                        launch_seq INTEGER, child_accept_seq INTEGER);
                    CREATE TRIGGER fixture_running_requires_owned_start
                    AFTER UPDATE OF state ON tool_operations
                    WHEN NEW.name='subagent' AND NEW.state='running'
                    BEGIN
                      SELECT CASE WHEN NOT EXISTS(
                        SELECT 1 FROM child_jobs j JOIN turns t ON t.id=j.child_turn
                        JOIN turn_acceptances a ON a.turn_id=t.id AND a.session_id=j.child_id
                        JOIN messages m ON m.id=a.user_message AND m.session_id=j.child_id
                        WHERE j.operation_id=NEW.id AND j.parent_id=NEW.session_id
                        AND j.child_id=t.session_id AND j.state='running' AND t.status='started'
                        AND json_extract(NEW.output,'$.status')='running'
                        AND json_extract(NEW.output,'$.sessionID')=j.child_id
                        AND json_extract(NEW.output,'$.jobGeneration')=j.operation_id
                        AND json_extract(NEW.output,'$.deliveryID')=j.delivery_id
                      ) THEN RAISE(ABORT,'running before real owned child acceptance') END;
                      INSERT INTO fixture_owned_start_order
                        SELECT j.operation_id,j.child_turn,a.user_message,
                          (SELECT seq FROM events WHERE kind='subagent_launch' AND json_extract(payload,'$.operation')=j.operation_id),
                          (SELECT seq FROM events WHERE kind='turn_started' AND session_id=j.child_id AND payload=j.child_turn)
                        FROM child_jobs j JOIN turn_acceptances a ON a.turn_id=j.child_turn
                        WHERE j.operation_id=NEW.id;
                    END;
                    """)
                calls = call('subagent', dict(agent='maker', description='Owned A',
                    prompt='CHILD_A_TASK', background=True), 'original-call-A')
                if self.case == 'reverse':
                    calls += call('subagent', dict(agent='reader', description='Owned B',
                        prompt='CHILD_B_TASK', background=True, model='fixture/child-b'), 'original-call-B')
                return calls + native.completed('')
            if self.retry_unknown and step == 2:
                child = self.rows('SELECT child_id FROM child_jobs')[0][0]
                return call('subagent', dict(agent='maker', description='Explicit unsafe continuation',
                    prompt='MUST_NOT_ADMIT', background=True, sessionID=child), 'unsafe-continuation') + native.completed('')
            if self.retry_unknown and step == 3:
                assert 'explicit recovery required' in pairs[-1][1]
                return native.completed('unsafe continuation refused')
            if step == 1:
                assert [identity for identity, _ in pairs] == (['original-call-A', 'original-call-B'] if self.case == 'reverse' else ['original-call-A'])
                for _, output in pairs:
                    assert json.loads(output)['status'] == 'running'
                self.parent_held.set()
                assert self.parent_release.wait(10), 'parent provider barrier watchdog'
                if self.case == 'reverse':
                    return native.tool('shell', {'command':'printf p >> parent-effect'}, 'independent-parent-effect')
                return native.completed('parent settled')
            assert self.case == 'reverse' and step >= 2
            wire = json.dumps(request)
            assert wire.count('Automatic background subagent result') == 2, wire
            if step == 2:
                assert [identity for identity, _ in pairs] == ['original-call-A','original-call-B','independent-parent-effect']
            return native.completed('parent settled with two notices')
        index = 0 if request['model'] == 'gpt-child-a' else 1
        assert request['model'] in ('gpt-child-a','child-b')
        if not pairs:
            self.child_requests[index] = request
            self.arrived[index].set()
            assert self.release[index].wait(10), 'child provider barrier watchdog'
            if index == 1:
                if self.reader_fails:
                    return [{'type':'response.failed','response':{'status':'failed',
                        'error':{'code':'invalid_request_error','message':'synthetic native child failure'}}}]
                return native.completed('error: successful child prose')
            if self.case in ('cancel','unknown'):
                return native.tool('shell', {'command':'printf "%s" $$ > owned-leader; printf x >> unknown-once; exec sleep 5'}, 'owned-child-leaf')
            return native.tool('apply_patch', {'patchText':'*** Begin Patch\n*** Add File: once\n+x\n*** End Patch'}, 'exact-one-effect')
        assert index == 0 and self.case not in ('cancel','unknown')
        assert [identity for identity, _ in pairs] == ['exact-one-effect']
        return native.completed('error: successful maker prose')

    def ready(self):
        assert self.arrived[0].wait(10) and self.parent_held.wait(10)
        if self.case == 'reverse':
            assert self.arrived[1].wait(10)
        native.until(lambda:self.auxiliary_requests == 1, 'actual initial auxiliary request')
        assert self.rows("SELECT count(*) FROM turns WHERE status='started' AND session_id='t50-background'") == [(1,)]
        for index, request in enumerate(self.child_requests):
            if request is None:
                continue
            wire = json.dumps(request)
            assert 'PARENT_PRIVATE_TRANSCRIPT' not in wire and 'PARENT_SYSTEM_PRIVATE' not in wire
            assert ('OWN_MAKER_SYSTEM' if index == 0 else 'OWN_READER_SYSTEM') in wire
            assert 'OWN_WORKSPACE_FOREGROUND_GUIDANCE' in wire
            assert len([item for item in request['input'] if item.get('role') == 'user']) == 1
        assert self.rows("SELECT state,count(*) FROM child_jobs GROUP BY state") == [('running',2 if self.case == 'reverse' else 1)]
        assert self.rows("SELECT state,count(*) FROM tool_operations WHERE name='subagent' GROUP BY state") == [('running',2 if self.case == 'reverse' else 1)]
        ordering = self.rows('SELECT operation,child_turn,user_message,launch_seq,child_accept_seq FROM fixture_owned_start_order ORDER BY rowid')
        assert len(ordering) == (2 if self.case == 'reverse' else 1)
        assert all(launch_seq < accept_seq for _,_,_,launch_seq,accept_seq in ordering)
        assert self.rows("SELECT count(*) FROM schema_migrations WHERE version=10 AND applied_at='t45-child-jobs'") == [(1,)]

    def counts(self):
        result = super().counts()
        result.update(jobs=self.rows('SELECT operation_id,parent_id,child_id,state,delivery_id,message_id,result FROM child_jobs ORDER BY rowid'),
            notices=self.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'")[0][0],
            effect_operations=self.rows("SELECT name,state,count(*) FROM tool_operations WHERE name!='subagent' GROUP BY name,state ORDER BY name,state"),
            owned_start_order=self.rows('SELECT operation,child_turn,user_message,launch_seq,child_accept_seq FROM fixture_owned_start_order ORDER BY rowid'))
        return result

    def __exit__(self, *args):
        self.parent_release.set()
        root = self.root
        handlers = list(self.server._threads)
        super().__exit__(*args)
        assert not root.exists(), 'exact owned TempDir survived cleanup'
        assert all(not handler.is_alive() for handler in handlers)
        assert all(not Path(f"/proc/{receipt['pid']}").exists() for receipt in self.owners)
        print(json.dumps(dict(cleanup_path=str(root), temp_removed=True,
            http_handlers_joined=True, http_worker_joined=not self.thread.is_alive(),
            pty_readers_joined=True, native_owners_reaped=True)), flush=True)


def reopen(fixture, notices):
    before = fixture.counts()
    fixture.start()
    native.until(lambda:fixture.rows('SELECT count(*) FROM child_jobs WHERE message_id IS NOT NULL') == [(notices,)], 'committed terminal delivery after native reopen')
    fixture.stop()
    after = fixture.counts()
    assert after['physical'] == before['physical'], 'idle reopen generated/polled'
    assert len(after['operations']) == len(before['operations']), 'native reopen created a tool operation'
    for previous,current in zip(before['operations'],after['operations']):
        if previous[2] == 'started':
            assert previous[:2] == current[:2] and previous[3] == current[3] and current[2] == 'unknown'
        else:
            assert previous == current, 'native reopen rewrote an immutable tool fact'
    assert after['notices'] == notices
    fixture.start()
    fixture.stop()
    assert fixture.counts() == after, 'second reopen redelivered/replayed'
    assert not fixture.errors, fixture.errors
    return after


def reverse(binary, pty):
    with Background(binary,'reverse') as fixture:
        process = None
        if pty:
            fixture.start()
            fixture.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        else:
            process = subprocess.Popen([str(binary),'--data-dir',str(fixture.data),'run','--json','--session','t50-background','PARENT_PRIVATE_TRANSCRIPT'],
                cwd=fixture.project,env=fixture.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        receipt = owner(fixture.process if pty else process)
        try:
            fixture.ready()
            assert fixture.physical_requests == 5
            fixture.release[1].set()
            native.until(lambda:fixture.rows("SELECT count(*) FROM child_jobs WHERE state='completed' AND message_id IS NOT NULL") == [(1,)], 'B completes and owner delivers during active parent')
            assert fixture.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='started'") == [(1,)]
            assert not fixture.project.joinpath('parent-effect').exists()
            fixture.release[0].set()
            native.until(lambda:fixture.rows("SELECT count(*) FROM child_jobs WHERE state='completed' AND message_id IS NOT NULL") == [(2,)], 'A effects/completes and delivers during active parent')
            assert fixture.project.joinpath('once').read_bytes() == b'x\n'
            assert not fixture.project.joinpath('parent-effect').exists()
            assert fixture.rows("SELECT j.operation_id FROM child_jobs j JOIN messages m ON m.id=j.message_id ORDER BY m.seq") == [(fixture.rows("SELECT operation_id FROM child_jobs ORDER BY rowid DESC")[0][0],),(fixture.rows("SELECT operation_id FROM child_jobs ORDER BY rowid")[0][0],)]
            fixture.parent_release.set()
            if pty:
                native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'normal parent safe continuation settled')
                fixture.stop()
            else:
                stdout,stderr = process.communicate(timeout=10)
                assert process.returncode == 0, stderr.decode()
                assert len(stdout)<262144 and len(stderr)<65536
            assert fixture.physical_requests == 7 and len(fixture.requests) == 6
            assert fixture.project.joinpath('parent-effect').read_bytes() == b'p'
            assert fixture.rows("SELECT state,count(*) FROM tool_operations WHERE name='subagent' GROUP BY state") == [('running',2)]
            if pty:
                fixture.height = 70
                fixture.start()
                fixture.send(b'FORK_BOUNDARY\r')
                native.until(lambda:len(fixture.parent_requests)==4 and fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'actual fork boundary turn')
                raw = fixture.rows("SELECT id,seq,role,text FROM messages WHERE session_id='t50-background' ORDER BY seq")
                fixture.message_action('FORK_BOUNDARY',3)
                fork = native.until(lambda:fixture.rows("SELECT id FROM sessions WHERE id LIKE 'fork-%'"),'actual owner fork created')[0][0]
                native.until(lambda:b'(fork)' in fixture.tail,'actual fork selection adopted')
                assert fixture.rows('SELECT count(*) FROM child_jobs WHERE parent_id=?',[fork]) == [(0,)]
                assert fixture.physical_requests == 8
                fixture.send(b'\x7f' * len(b'FORK_BOUNDARY') + b'FORK_CONTINUE\r')
                native.until(lambda:len(fixture.parent_requests)==5 and fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'fork uses captured child notices once')
                assert fixture.rows("SELECT count(*) FROM turns WHERE session_id=? AND prompt='FORK_CONTINUE'",[fork]) == [(1,)],fixture.rows('SELECT session_id,prompt,status FROM turns')
                fixture.message_action('FORK_CONTINUE',1)
                native.until(lambda:fixture.rows('SELECT upper_seq FROM conversation_state WHERE session_id=?',[fork]),'real fork Revert applied')
                fixture.send(b'\x7f' * len(b'FORK_CONTINUE') + b'REPLACEMENT_AFTER_REVERT\r')
                native.until(lambda:len(fixture.parent_requests)==6 and fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'revert retains visible prefix notices once without replay')
                fixture.stop()
                assert fixture.rows("SELECT id,seq,role,text FROM messages WHERE session_id='t50-background' ORDER BY seq") == raw
                assert fixture.rows('SELECT count(*) FROM child_jobs') == [(2,)]
                assert fixture.project.joinpath('once').read_bytes() == b'x\n'
                assert fixture.project.joinpath('parent-effect').read_bytes() == b'p'
                assert fixture.physical_requests == 10
            before = fixture.counts()
            after = reopen(fixture,2)
            assert before == after
            receipt.update(exit=0,reaped=not Path(f"/proc/{receipt['pid']}").exists())
            return dict(case='reverse-pty' if pty else 'reverse-headless',status='PASS',counts=after,owners=[receipt,*fixture.owners],
                child_effects=1,parent_effects=1,child_notices=2,busy_notice_batch=1,reopenNoReplay=True)
        finally:
            for event in fixture.release:
                event.set()
            fixture.parent_release.set()
            if process is not None:
                if process.poll() is None:
                    process.kill()
                process.wait()
                for stream in (process.stdout,process.stderr):
                    stream.close()


def leaf_case(binary, crash):
    with Background(binary,'unknown' if crash else 'cancel') as fixture:
        fixture.start()
        fixture.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        fixture.ready()
        fixture.release[0].set()
        native.until(lambda:fixture.project.joinpath('owned-leader').exists(), 'actual owned shell launched')
        pid = int(fixture.project.joinpath('owned-leader').read_text())
        leaf = dict(pid=pid,pgid=os.getpgid(pid),startticks=Path(f'/proc/{pid}/stat').read_text().split(') ')[1].split()[19])
        native.until(lambda:fixture.project.joinpath('unknown-once').read_bytes() == b'x', 'actual effect before interruption')
        assert fixture.rows("SELECT state FROM tool_operations WHERE name='shell'") == [('started',)]
        if crash:
            fixture.stop(crash=True)
            # Python is a temporary subreaper ONLY for its explicitly owned crash
            # leaf; no broad wait/prune and no foreign process is signalled.
            os.killpg(leaf['pgid'],signal.SIGKILL)
            adopted,status = os.waitpid(pid,0)
            assert adopted == pid
            leaf.update(exit=status)
        else:
            fixture.send(b'\x03')
            native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'parent cancel joins child provider/tool')
            native.until(lambda:not Path(f'/proc/{pid}').exists(), 'actual owned shell leaf reaped')
        leaf['reaped'] = not Path(f'/proc/{pid}').exists()
        assert leaf['reaped']
        fixture.parent_release.set()
        if not crash:
            fixture.stop()
        after = reopen(fixture,1)
        assert after['physical'] == 4 and after['notices'] == 1
        assert after['jobs'][0][3] == ('unknown' if crash else 'cancelled')
        assert fixture.project.joinpath('unknown-once').read_bytes() == b'x'
        if crash:
            fixture.retry_unknown = True
            fixture.start()
            fixture.send(b'EXPLICIT_UNSAFE_CONTINUATION\r')
            native.until(lambda:len(fixture.parent_requests) == 4 and fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'unsafe same-child continuation is refused before prompt/effect')
            fixture.stop()
            after = fixture.counts()
            assert after['physical'] == 6
            assert fixture.rows("SELECT count(*) FROM messages WHERE session_id=(SELECT child_id FROM child_jobs) AND text LIKE '%MUST_NOT_ADMIT%'") == [(0,)]
            assert fixture.project.joinpath('unknown-once').read_bytes() == b'x'
        assert not fixture.errors,fixture.errors
        return dict(case='unknown-effect-crash' if crash else 'parent-cancel-owned-leaf',status='PASS',counts=after,leaf=leaf,owners=fixture.owners,effects=1,child_notices=1,reopenNoReplay=True)


def exact_failure(binary):
    with Background(binary, 'reverse') as fixture:
        fixture.reader_fails = True
        fixture.start()
        fixture.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        fixture.ready()
        fixture.release[1].set()
        native.until(lambda:fixture.rows("SELECT count(*) FROM child_jobs WHERE state='error' AND message_id IS NOT NULL") == [(1,)], 'real child response.failed terminal and notice')
        assert fixture.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='started'") == [(1,)]
        assert fixture.rows("SELECT count(*) FROM child_jobs WHERE state='running'") == [(1,)]
        fixture.release[0].set()
        native.until(lambda:fixture.rows("SELECT count(*) FROM child_jobs WHERE state='completed' AND message_id IS NOT NULL") == [(1,)], 'independent sibling success despite error-prefixed prose')
        assert fixture.rows("SELECT result FROM child_jobs WHERE state='completed'") == [('error: successful maker prose',)]
        assert fixture.project.joinpath('once').read_bytes() == b'x\n'
        fixture.parent_release.set()
        native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='started'") == [(0,)], 'parent continues once with both exact terminal notices')
        fixture.stop()
        after = reopen(fixture, 2)
        assert after['physical'] == 7 and after['notices'] == 2
        assert fixture.project.joinpath('parent-effect').read_bytes() == b'p'
        assert sorted(row[3] for row in after['jobs']) == ['completed', 'error']
        return dict(case='typed-failure-versus-success-prose',status='PASS',counts=after,
            owners=fixture.owners,child_effects=1,parent_effects=1,child_notices=2,reopenNoReplay=True)


def fault_case(binary, terminal):
    with Background(binary,'terminal-fault' if terminal else 'delivery-fault') as fixture:
        fixture.start()
        fixture.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        fixture.ready()
        connection = sqlite3.connect(fixture.data/'oc.sqlite')
        boundary = 'state' if terminal else 'message_id'
        connection.executescript("CREATE TABLE child_commit_fault(value TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED); "
            f"CREATE TRIGGER fail_child_commit AFTER UPDATE OF {boundary} ON child_jobs WHEN NEW.state='completed' BEGIN INSERT INTO child_commit_fault VALUES('missing-session'); END;")
        connection.close()
        fixture.release[0].set()
        native.until(lambda:fixture.rows("SELECT count(*) FROM turns WHERE status='completed' AND session_id=(SELECT child_id FROM child_jobs)") == [(1,)], 'actual child committed before receipt fault')
        fixture.parent_release.set()
        # Fatal worker joins its work; PTy then exits through the public shutdown.
        native.until(lambda:fixture.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'") == [(5,)], 'exact native dispatch count before fault')
        fixture.stop(expected_exit=1)
        assert fixture.rows('SELECT state,message_id FROM child_jobs') == [('running' if terminal else 'completed',None)]
        assert fixture.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'") == [(0,)]
        connection = sqlite3.connect(fixture.data/'oc.sqlite')
        connection.execute('DROP TRIGGER fail_child_commit')
        connection.commit()
        connection.close()
        after = reopen(fixture,1)
        assert after['physical'] == 5 and after['jobs'][0][3] == 'completed'
        assert after['jobs'][0][-1] == 'error: successful maker prose'
        assert fixture.project.joinpath('once').read_bytes() == b'x\n'
        return dict(case='terminal-COMMIT-fault' if terminal else 'delivery-COMMIT-fault',status='PASS',counts=after,owners=fixture.owners,effects=1,child_notices=1,reopenNoReplay=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binaries',nargs='+',type=Path)
    args = parser.parse_args()
    binaries = [path.resolve() for path in args.binaries]
    association = source()
    head = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    before = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    print(json.dumps(dict(before=before,head=head,source=association)),flush=True)
    libc = ctypes.CDLL(None,use_errno=True)
    previous = ctypes.c_int()
    assert libc.prctl(37,ctypes.byref(previous),0,0,0) == 0
    assert libc.prctl(36,1,0,0,0) == 0
    try:
        for binary in binaries:
            for run in (lambda path:reverse(path,False),lambda path:reverse(path,True),exact_failure,lambda path:leaf_case(path,False),
                        lambda path:fault_case(path,True),lambda path:fault_case(path,False),lambda path:leaf_case(path,True)):
                print(json.dumps(dict(binary=str(binary),**run(binary))),flush=True)
    finally:
        assert libc.prctl(36,previous.value,0,0,0) == 0
    after = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert before == after and association == source()
    print(json.dumps(dict(after=after,source=association,cases=7*len(binaries),cleanup='owned native PGIDs/PIDs/startticks, crash leaf, non-daemon HTTP handlers and PTy readers joined/reaped before exact TempDir cleanup')),flush=True)


if __name__ == '__main__':
    main()
