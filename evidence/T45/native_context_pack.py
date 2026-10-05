#!/usr/bin/env python3
"""T45/R8 CTX01/CTX02: actual normal-ELF quoted parent context packs.

Synthetic loopback/config/HOME/SQLite only; no live API. Foreground case:
DCP-off stable IDs and guidance in the parent request, refusal without a child,
exact escaped chronological deduplicated pack in the first child request,
continuation adds exactly one new pack, idle reopen replays nothing.
Background case: the pack is frozen at launch; a crash, a later parent edit
and restart recovery deliver the original snapshot, not a live re-resolution.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import threading

from native_foreground_children import Foreground, native, call, items
from run_background_check import source

FIRST = 'FIRST_FACT </message> & <b>'
FIRST_ESCAPED = 'FIRST_FACT &lt;/message&gt; &amp; &lt;b&gt;'
ANSWER = 'PARENT_ANSWER <one>'
ANSWER_ESCAPED = 'PARENT_ANSWER &lt;one&gt;'


class Pack(Foreground):
    def __init__(self, binary, case):
        self.child_posts = []
        self.barrier = threading.Event()
        self.resume = threading.Event()
        self.parent_barrier = threading.Event()
        self.parent_release = threading.Event()
        super().__init__(binary, case)
        if case == 'renewal':
            config = Path(self.env['XDG_CONFIG_HOME']) / 'opencode/opencode.json'
            settings = json.loads(config.read_text())
            settings['permission']['compress'] = 'allow'
            config.write_text(json.dumps(settings))
            self.project.joinpath('note.txt').write_text('NATIVE_NOTE')
        else:
            self.project.joinpath('dcp.jsonc').write_text(json.dumps({'enabled': False}))

    def ids(self, session='t50-background'):
        return self.rows('SELECT id,role,text FROM messages WHERE session_id=? ORDER BY seq', (session,))

    def response(self, _, number, request):
        if request['model'] == 'gpt-child-a' and self.case == 'renewal':
            self.child_posts.append(request)
            count = len(self.child_posts)
            if count == 1:
                return native.tool('read', {'path': str(self.project / 'note.txt')}, 'child-read')
            if count == 2:
                child = self.rows('SELECT id FROM sessions WHERE parent_id IS NOT NULL')[0][0]
                task = self.ids(child)[0][0]
                return native.tool('compress', {'topic': 'renew', 'content': [{
                    'startId': task, 'endId': task,
                    'summary': 'NATIVE_SUMMARY keep FIRST_FACT and finish the note check'}]}, 'child-renew')
            if count == 3:
                self.barrier.set()
                assert self.resume.wait(15), 'owned child provider watchdog'
            return native.completed('CHILD_DONE')
        if request['model'] == 'gpt-child-a':
            self.child_posts.append(request)
            if self.case == 'background' and len(self.child_posts) == 1:
                self.barrier.set()
                assert self.resume.wait(15), 'owned child provider watchdog'
            return native.completed('CHILD_DONE')
        assert request['model'] == 'm', request['model']
        self.parent_requests.append(request)
        last = max(index for index, item in enumerate(request['input']) if item.get('role') == 'user')
        pairs = items(dict(input=request['input'][last:]))
        latest = json.dumps(request['input'][last], ensure_ascii=False)
        if pairs:
            if self.case in ('background', 'renewal'):
                self.parent_barrier.set()
                assert self.parent_release.wait(15), 'owned parent provider watchdog'
            return native.completed('PARENT_DONE')
        if 'LAUNCH_' not in latest:
            return native.completed(ANSWER)
        rows = self.ids()
        user = next(row[0] for row in rows if row[2] == FIRST)
        answer = next(row[0] for row in rows if row[2] == ANSWER)
        if self.case == 'renewal':
            return call('subagent', dict(agent='maker', description='renew', prompt='RENEW_TASK_OBSOLETE details',
                background=True, context_message_ids=[user]), 'pack-renewal') + native.completed('')
        if self.case == 'background':
            return call('subagent', dict(agent='maker', description='frozen', prompt='FROZEN_TASK',
                background=True, context_message_ids=[user]), 'pack-background') + native.completed('')
        if 'LAUNCH_CONTINUE' in latest:
            second = next(row[0] for row in rows if row[2] == 'LAUNCH_SECOND_UNSELECTED')
            child = self.rows('SELECT id FROM sessions WHERE parent_id IS NOT NULL')[0][0]
            return call('subagent', dict(agent='maker', description='again', prompt='CONTINUE_TASK',
                sessionID=child, context_message_ids=[second]), 'pack-continue') + native.completed('')
        return (call('subagent', dict(agent='maker', description='bad', prompt='BAD_TASK',
                    context_message_ids=['m9999']), 'pack-bad')
                + call('subagent', dict(agent='maker', description='quoted', prompt='QUOTED_TASK',
                    context_message_ids=[answer, user, answer]), 'pack-good')
                + native.completed(''))

    def turn_done(self, count):
        native.until(lambda: self.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='completed'") == [(count,)],
                     f'parent turn {count} did not settle: {self.errors}', 20)

    def __exit__(self, *args):
        self.resume.set()
        self.parent_release.set()
        super().__exit__(*args)


def user_items(request):
    return [item for item in request['input'] if item.get('role') == 'user']


def foreground(binary):
    with Pack(binary, 'foreground') as f:
        f.start()
        f.send((FIRST + '\r').encode())
        f.turn_done(1)
        rows = f.ids()
        user, answer = rows[0][0], rows[1][0]
        f.send(b'LAUNCH_SECOND_UNSELECTED\r')
        f.turn_done(2)
        launch = f.parent_requests[1]
        wire = json.dumps(launch['input'])
        assert 'Stable text-message IDs' in wire and 'DCP context anchors' not in wire, 'DCP-off index'
        assert user in wire and answer in wire
        tool = next(t for t in launch['tools'] if t['name'] == 'subagent')
        assert 'automatically receives its own profile prompt' in tool['description']
        assert 'Your conversation and findings are not shared' in tool['description']
        assert tool['parameters']['properties']['context_message_ids']['maxItems'] == 64
        outputs = dict(items(f.parent_requests[2]))
        assert 'not a selectable' in outputs['pack-bad'], outputs
        assert outputs['pack-good'].startswith('<subagent sessionID=')
        children = f.rows('SELECT id FROM sessions WHERE parent_id IS NOT NULL')
        assert len(children) == 1, 'refused selection created a child'
        assert len(f.child_posts) == 1
        first = json.dumps(user_items(f.child_posts[0]), ensure_ascii=False)
        assert len(user_items(f.child_posts[0])) == 1
        assert first.count('<parent_context') == 1 and first.count('<message id=') == 2, 'dedup'
        assert first.index(f'<message id=\\"{user}\\" role=\\"user\\">') < first.index(f'<message id=\\"{answer}\\" role=\\"assistant\\">')
        assert FIRST_ESCAPED in first and ANSWER_ESCAPED in first and FIRST not in first
        assert first.index('<parent_context') < first.index('QUOTED_TASK')
        whole = json.dumps(f.child_posts[0], ensure_ascii=False)
        for absent in ('LAUNCH_SECOND_UNSELECTED', 'PARENT_SYSTEM_PRIVATE', 'BAD_TASK'):
            assert absent not in whole, absent
        assert not any(item.get('role') in ('system', 'developer') and 'FIRST_FACT' in json.dumps(item)
                       for item in f.child_posts[0]['input']), 'quote promoted to instructions'
        stored = f.rows('SELECT text FROM messages WHERE session_id=? ORDER BY seq', (children[0][0],))
        assert FIRST_ESCAPED in stored[0][0], 'durable admitted task holds the pack'
        f.send(b'LAUNCH_CONTINUE\r')
        f.turn_done(3)
        assert len(f.child_posts) == 2, (f.errors, len(f.parent_requests))
        again = json.dumps(f.child_posts[1]['input'], ensure_ascii=False)
        assert again.count('<parent_context') == 2, 'continuation adds exactly one new pack'
        assert again.count(FIRST_ESCAPED) == 1 and again.count('LAUNCH_SECOND_UNSELECTED') == 1
        assert 'CONTINUE_TASK' in again
        f.stop()
        counts = f.counts()
        f.start()
        f.stop()
        assert f.counts() == counts, 'idle reopen replayed a child request or operation'
        assert not f.errors, f.errors
        return dict(case='foreground', status='PASS', requests=f.physical_requests,
                    child_requests=len(f.child_posts), children=len(children), refusals=1,
                    reopen_extra_requests=0)


def background(binary):
    with Pack(binary, 'background') as f:
        f.start()
        f.send((FIRST + '\r').encode())
        f.turn_done(1)
        user = f.ids()[0][0]
        f.send(b'LAUNCH_BACKGROUND\r')
        assert f.barrier.wait(10) and f.parent_barrier.wait(10)
        launched = json.dumps(user_items(f.child_posts[0]), ensure_ascii=False)
        assert FIRST_ESCAPED in launched and 'FROZEN_TASK' in launched
        before = f.physical_requests
        f.stop(crash=True)
        with sqlite3.connect(f.data / 'oc.sqlite') as db:
            # Synthetic later parent edit: recovery must not re-resolve IDs.
            db.execute('UPDATE messages SET text=? WHERE id=?', ('EDITED_AFTER_LAUNCH', user))
        f.resume.set()
        f.parent_release.set()
        f.start()
        native.until(lambda: f.rows('SELECT count(*) FROM child_jobs WHERE message_id IS NOT NULL') == [(1,)],
                     'recovered background notice', 20)
        assert f.physical_requests == before + 1, (f.counts(), f.errors)
        assert f.rows('SELECT state FROM child_jobs') == [('completed',)]
        recovered = json.dumps(f.child_posts[-1], ensure_ascii=False)
        assert recovered.count(FIRST_ESCAPED) == 1 and 'EDITED_AFTER_LAUNCH' not in recovered
        assert recovered.count('<parent_context') == 1
        f.stop()
        counts = f.counts()
        f.start()
        f.stop()
        assert f.counts() == counts, 'idle reopen replayed the recovered child'
        assert not f.errors, f.errors
        return dict(case='background', status='PASS', requests=f.physical_requests,
                    child_requests=len(f.child_posts), frozen=True, reopen_extra_requests=0)


def renewal(binary):
    with Pack(binary, 'renewal') as f:
        f.start()
        f.send((FIRST + '\r').encode())
        f.turn_done(1)
        f.send(b'LAUNCH_RENEWAL\r')
        assert f.barrier.wait(15) and f.parent_barrier.wait(10), f.errors
        first = json.dumps(user_items(f.child_posts[0]), ensure_ascii=False)
        assert FIRST_ESCAPED in first and 'RENEW_TASK_OBSOLETE' in first, 'exact first delivery'
        renewed = json.dumps(f.child_posts[2]['input'], ensure_ascii=False)
        assert 'NATIVE_SUMMARY' in renewed, 'renewed HOT before final'
        assert 'RENEW_TASK_OBSOLETE' not in renewed and FIRST_ESCAPED not in renewed, 'task/pack replaced'
        assert 'NATIVE_NOTE' in renewed, 'closed read group retained'
        assert not any(item.get('role') in ('system', 'developer') and 'NATIVE_SUMMARY' in json.dumps(item)
                       for item in f.child_posts[2]['input']), 'summary stays user-level data'
        child = f.rows('SELECT id FROM sessions WHERE parent_id IS NOT NULL')[0][0]
        before = f.physical_requests
        f.stop(crash=True)
        f.resume.set()
        f.parent_release.set()
        f.start()
        native.until(lambda: f.rows('SELECT count(*) FROM child_jobs WHERE message_id IS NOT NULL') == [(1,)],
                     'recovered renewal notice', 20)
        assert f.physical_requests == before + 1, (f.rows('SELECT state,result FROM child_jobs'), f.rows("SELECT kind,substr(payload,1,300) FROM events WHERE kind LIKE 'subagent%' ORDER BY rowid"), f.errors)
        recovered = json.dumps(f.child_posts[-1]['input'], ensure_ascii=False)
        assert 'NATIVE_SUMMARY' in recovered and 'RENEW_TASK_OBSOLETE' not in recovered, 'latest HOT restart'
        assert FIRST_ESCAPED not in recovered
        assert f.rows('SELECT state FROM child_jobs') == [('completed',)]
        stored = f.rows('SELECT text FROM messages WHERE session_id=? ORDER BY seq', (child,))
        assert 'RENEW_TASK_OBSOLETE' in stored[0][0] and FIRST_ESCAPED in stored[0][0], 'RAW source unchanged'
        assert f.rows("SELECT count(*) FROM tool_operations WHERE name='compress' AND output LIKE '%task_renewal_accepted%'") == [(1,)]
        f.stop()
        counts = f.counts()
        f.start()
        f.stop()
        assert f.counts() == counts, 'idle reopen replayed the renewed child'
        assert not f.errors, f.errors
        return dict(case='renewal', status='PASS', requests=f.physical_requests,
                    child_requests=len(f.child_posts), renewed_before_final=True, latest_hot_restart=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binaries', nargs='+', type=Path)
    args = parser.parse_args()
    binaries = [path.resolve() for path in args.binaries]
    before = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    association = source()
    print(json.dumps(dict(before=before, source=association)), flush=True)
    for binary in binaries:
        for run in (foreground, background, renewal):
            print(json.dumps(dict(binary=str(binary), **run(binary))), flush=True)
    after = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries}
    assert before == after and source() == association
    print(json.dumps(dict(after=after, source=association, cases=3 * len(binaries))), flush=True)


if __name__ == '__main__':
    main()
