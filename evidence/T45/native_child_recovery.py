#!/usr/bin/env python3
"""R3(g): real normal native process death/restart, loopback/SQLite only."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import threading

from native_foreground_children import Foreground, native, call, items
from run_background_check import source


class Recovery(Foreground):
    def __init__(self, binary, case):
        self.barrier = threading.Event()
        self.resume = threading.Event()
        self.child_posts = []
        self.parent_barrier = threading.Event()
        self.parent_release = threading.Event()
        self.resumed_barrier = threading.Event()
        self.resumed_release = threading.Event()
        super().__init__(binary, case)

    def response(self, _, number, request):
        if request['model'] == 'm':
            self.parent_requests.append(request)
            if len(self.parent_requests) == 1:
                return call('subagent', dict(agent='maker', description='original delegation',
                    prompt='EXACT_ORIGINAL_TASK', background=True), 'original-launch') + native.completed('')
            self.parent_barrier.set()
            assert self.parent_release.wait(15), 'owned parent provider watchdog'
            return native.completed('parent finished')
        assert request['model'] == 'gpt-child-a'
        self.child_posts.append(request)
        if self.case == 'repeat' and len(self.child_posts) == 2:
            self.resumed_barrier.set()
            assert self.resumed_release.wait(15), 'resumed execution watchdog'
        if self.case == 'read' and len(self.child_posts) == 1:
            return native.tool('read', {'path':str(self.project/'seed')}, 'exact-read')
        held = 2 if self.case == 'read' else 1
        if len(self.child_posts) == held:
            self.barrier.set()
            assert self.resume.wait(15), 'owned child provider watchdog'
        return native.completed('committed recovered result')

    def __exit__(self, *args):
        self.resume.set()
        self.parent_release.set()
        self.resumed_release.set()
        handlers = list(self.server._threads)
        root = self.root
        super().__exit__(*args)
        assert not root.exists()
        assert all(not h.is_alive() for h in handlers)
        assert all(not Path(f"/proc/{o['pid']}").exists() for o in self.owners)
        print(json.dumps(dict(cleanup_path=str(root), temp_removed=True,
            http_handlers_joined=True, pty_readers_joined=True, owners=self.owners)), flush=True)


def proof(binary, case):
    with Recovery(binary, case) as f:
        f.start()
        f.send(b'PARENT_PRIVATE_TRANSCRIPT\r')
        assert f.barrier.wait(10) and f.parent_barrier.wait(10)
        native.until(lambda:f.auxiliary_requests == 1, 'initial auxiliary request')
        original = f.rows('SELECT operation_id,parent_id,child_id,child_turn,delivery_id,identity FROM child_jobs')[0]
        accepted = f.rows('SELECT * FROM turn_acceptances ORDER BY rowid')
        raw = f.rows('SELECT id,prompt FROM turns ORDER BY rowid')
        before = f.physical_requests
        f.stop(crash=True)
        # Only owned synthetic SQLite/config faults; no live configuration.
        if case == 'hot':
            with sqlite3.connect(f.data/'oc.sqlite') as db:
                # A committed current journal fixture, with original admission
                # still immutable. Recovery must use HOT, never turns.prompt.
                value = json.loads(db.execute('SELECT result FROM turns WHERE id=?',(original[3],)).fetchone()[0])
                value['input'][0]['content'][0]['text'] = 'LATEST_SELECTED_HOT'
                db.execute('UPDATE turns SET result=? WHERE id=?',(json.dumps(value),original[3]))
        elif case not in ('safe', 'read','repeat'):
            with sqlite3.connect(f.data/'oc.sqlite') as db:
                if case in ('patch','write','edit','shell','mcp','completed-effect'):
                    name = {'patch':'apply_patch','mcp':'mcp_fixture_effect','completed-effect':'apply_patch'}.get(case,case)
                    state = 'completed' if case == 'completed-effect' else 'started'
                    db.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES('uncertain',?,?,?,?,'{}','captured')",(original[2],original[3],name,state))
                elif case == 'corrupt-hot':
                    db.execute("UPDATE turns SET result=json_set(result,'$.working',json_object('turn_id','foreign')) WHERE id=?",(original[3],))
                elif case == 'attempts':
                    for attempt in range(10):
                        db.execute("INSERT INTO events(session_id,kind,payload) VALUES(?,'subagent_resume_claim',?)",(original[2],json.dumps(dict(operation=original[0],attempt=attempt+1))))
                elif case == 'location':
                    db.execute("UPDATE prefs SET value='/unavailable' WHERE key=?",('tui.session_location.'+original[2],))
                elif case == 'session':
                    db.execute("UPDATE sessions SET model='fixture/mismatch' WHERE id=?",(original[2],))
                elif case == 'fingerprint':
                    db.execute("UPDATE events SET payload=json_set(payload,'$.fence.generation_fingerprint','mismatch') WHERE kind='subagent_recovery_fence'")
                elif case in ('profile','policy','model','credential','generation'):
                    config = Path(f.env['XDG_CONFIG_HOME'])/'opencode/opencode.json'
                    settings = json.loads(config.read_text())
                    if case == 'profile': settings['agent']['maker']['system'] += ' changed'
                    elif case == 'policy': settings['permission']['read'] = 'deny'
                    elif case == 'model': del settings['provider']['fixture']['models']['gpt-child-a']
                    elif case == 'credential': settings['provider']['fixture']['options']['apiKey'] = ''
                    else:
                        db.execute("UPDATE child_jobs SET identity=json_set(identity,'$.generation',999)")
                    config.write_text(json.dumps(settings))
                else: raise AssertionError(case)
        f.resume.set()
        f.parent_release.set()
        f.start()
        if case == 'repeat':
            assert f.resumed_barrier.wait(10)
            assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_resume_claim'") == [(1,)]
            assert f.rows('SELECT state FROM child_jobs') == [('running',)]
            f.stop(crash=True)
            f.resumed_release.set()
            f.start()
        native.until(lambda:f.rows('SELECT count(*) FROM child_jobs WHERE message_id IS NOT NULL') == [(1,)], 'same job terminal notice')
        if case in ('safe','read','repeat','hot'):
            additional = 2 if case == 'repeat' else 1
            assert f.physical_requests == before + additional, (case, f.counts(), f.errors)
            assert f.rows('SELECT state FROM child_jobs') == [('completed',)]
            wire = json.dumps(f.child_posts[-1])
            assert wire.count('EXACT_ORIGINAL_TASK') == (0 if case == 'hot' else 1), wire
            if case == 'hot': assert 'LATEST_SELECTED_HOT' in wire
            assert 'PARENT_PRIVATE_TRANSCRIPT' not in wire
            if case == 'read':
                assert items(f.child_posts[-1])[0][0] == 'exact-read'
                assert 'fixture seed' in items(f.child_posts[-1])[0][1]
                assert f.rows("SELECT count(*) FROM tool_operations WHERE name='read'") == [(1,)]
            assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_resume_claim'") == [(additional,)]
        else:
            assert f.physical_requests == before, (case, f.counts(),f.errors)
            assert f.rows('SELECT state FROM child_jobs') == [('unknown',)]
        assert f.rows('SELECT operation_id,parent_id,child_id,child_turn,delivery_id FROM child_jobs')[0] == original[:5]
        assert f.rows('SELECT * FROM turn_acceptances ORDER BY rowid') == accepted
        assert f.rows('SELECT id,prompt FROM turns ORDER BY rowid') == raw
        assert f.rows("SELECT status FROM turns WHERE session_id='t50-background'") == [('unknown',)]
        assert f.rows("SELECT count(*) FROM events WHERE kind='subagent_notice'") == [(1,)]
        assert not f.project.joinpath('once').exists()
        assert not f.project.joinpath('forbidden-startup').exists()
        f.stop()
        counts = f.counts()
        f.start()
        f.stop()
        assert f.counts() == counts, 'idle reopen executed or redelivered'
        assert not f.errors, f.errors
        return dict(case=case,status='PASS',requests=f.physical_requests,
            dispatches=f.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")[0][0],
            job=original[:5],state=f.rows('SELECT state FROM child_jobs')[0][0],notices=1,
            effects=0,acceptances_unchanged=True,reopen_extra_requests=0)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('binaries', nargs='+', type=Path)
    p.add_argument('--cases', default='safe,read,repeat,hot,patch,write,edit,shell,mcp,completed-effect,corrupt-hot,attempts,location,session,fingerprint,profile,policy,model,credential,generation')
    args = p.parse_args()
    binaries = [path.resolve() for path in args.binaries]
    before = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    association = source()
    print(json.dumps(dict(before=before,source=association)),flush=True)
    for binary in binaries:
        for case in args.cases.split(','):
            print(json.dumps(dict(binary=str(binary),**proof(binary,case))),flush=True)
    after = {str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert before == after and source() == association
    print(json.dumps(dict(after=after,source=association,cases=len(args.cases.split(','))*len(binaries))),flush=True)


if __name__ == '__main__': main()
