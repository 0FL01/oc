#!/usr/bin/env python3
"""Offline direct-ELF retry/span/lane qualification; no Cargo or user config."""
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

ROOT = Path(__file__).resolve().parents[2]
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')


def event(value):
    return ('data: ' + json.dumps(value) + '\n\n').encode()


def completed(text):
    return event({'type': 'response.output_text.delta', 'delta': text}) + event({
        'type': 'response.completed', 'response': {'output': [
            {'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': text}]}]}})


class Fixture:
    def __init__(self, binary, mode):
        self.binary, self.mode = binary, mode
        self.temp = tempfile.TemporaryDirectory(prefix='t54-retry-', dir=TMP)
        self.root = Path(self.temp.name)
        self.home, self.project = self.root/'home', self.root/'project'
        self.project.mkdir()
        config = self.home/'config'/'opencode'
        config.mkdir(parents=True)
        self.lanes = {lane: [] for lane in ('main', 'child', 'compaction', 'title')}
        self.lock = threading.Lock()
        self.received = threading.Event()
        fixture = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                length = int(self.headers['Content-Length'])
                assert 0 < length < 1024*1024
                request = json.loads(self.rfile.read(length))
                assert self.path == '/v1/responses' and request['model'] == 'fixture-model'
                assert self.headers.get('Authorization') == 'Bearer auth-canary'
                assert self.headers.get('X-T54-Binding') == 'pinned-fixture'
                data = json.dumps(request['input'])
                lane = ('title' if 'Generate a short session title' in data else
                        'compaction' if 'Summarize only what the user and assistant said and did' in data else
                        'child' if 'CHILD_LANE' in data else 'main')
                with fixture.lock:
                    fixture.lanes[lane].append(request)
                    number = len(fixture.lanes[lane])
                status, headers, body = fixture.response(lane, number)
                self.send_response(status)
                self.send_header('Content-Type', 'text/event-stream' if status == 200 else 'application/json')
                self.send_header('Content-Length', str(len(body)))
                for key, value in headers.items():
                    self.send_header(key, value)
                self.end_headers()
                try:
                    self.wfile.write(body)
                    self.wfile.flush()
                except BrokenPipeError:
                    pass
                if lane == 'main':
                    fixture.received.set()

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        settings = {'model': 'fixture/fixture-model', 'permission': {'bash': 'allow', 'subagent': 'allow'},
                    'agent': {'helper': {'mode': 'subagent', 'prompt': 'CHILD_LANE'}},
                    'compaction': {'auto': mode == 'compaction', 'buffer': 500000},
                    'provider': {'fixture': {'npm': '@ai-sdk/openai', 'options': {
                        'baseURL': f'http://127.0.0.1:{self.server.server_port}/v1', 'apiKey': 'auth-canary',
                        'headers': {'X-T54-Binding': 'pinned-fixture'}},
                        'models': {'fixture-model': {'limit': {'context': 500000, 'output': 4096}}}}}}
        (config/'opencode.json').write_text(json.dumps(settings))
        self.env = {'HOME': str(self.home), 'XDG_CONFIG_HOME': str(self.home/'config'),
                    'XDG_DATA_HOME': str(self.home/'data'), 'OC_TEST_ALLOW_LOOPBACK': '1',
                    'TERM': 'xterm-256color', 'PATH': '/usr/bin:/bin'}

    def response(self, lane, number):
        if lane == 'title':
            if self.mode == 'title_failure':
                return 503, {'x-should-retry': 'true'}, b'{"error":{"code":"server_error"},"raw":"body-canary"}'
            return 200, {}, completed('Fixture title')
        if lane == 'compaction':
            if number in (1, 3):
                return 503, {}, b'{"error":{"code":"server_error"},"raw":"body-canary"}'
            return 200, {}, completed('invalid candidate' if number == 2 else '## Objective\nRetain completed work.')
        if self.mode == 'child':
            if lane == 'child':
                return (503, {}, b'{"error":{"code":"server_error"}}') if number == 1 else (200, {}, completed('childdone'))
            if number == 1:
                call = {'type': 'function_call', 'id': 'fc_child', 'call_id': 'child', 'name': 'subagent',
                        'arguments': json.dumps({'agent': 'helper', 'prompt': 'child fixture', 'description': 'child probe'}),
                        'status': 'completed'}
                return 200, {}, event({'type': 'response.output_item.done', 'output_index': 0, 'item': call}) + event({'type': 'response.completed', 'response': {'output': [call]}})
            return 200, {}, completed('rootdone')
        if self.mode == 'compaction':
            return 200, {}, completed('rootdone')
        if self.mode == 'effect':
            if number == 1:
                call = {'type': 'function_call', 'id': 'fc_once', 'call_id': 'once', 'name': 'bash',
                        'arguments': json.dumps({'argv': ['sh', '-c', 'printf effect >> retry-effects']}),
                        'status': 'completed'}
                return 200, {}, event({'type': 'response.output_item.done', 'output_index': 0, 'item': call}) + event({'type': 'response.completed', 'response': {'output': [call]}})
            if number == 2:
                return 200, {}, event({'type': 'response.output_text.delta', 'delta': 'partial'})
            return 200, {}, completed('effect preserved')
        if self.mode == 'title_failure':
            return 200, {}, completed('rootdone')
        if self.mode in ('mixed', 'park', 'restart'):
            if number == 1:
                return 429, {'retry-after-ms': '5000' if self.mode in ('park', 'restart') else '0'}, b'{"error":{"code":"rate_limit_exceeded"},"raw":"body-canary env-canary https://private.invalid"}'
            if number == 2:
                return 200, {'x-should-retry': 'false', 'retry-after-ms': '900000'}, event({'type': 'response.output_text.delta', 'delta': 'partial'})
            return 200, {}, completed('continued')
        if self.mode == 'override':
            return (401, {'x-should-retry': 'true'}, b'{"error":{"code":"authentication_error"}}') if number == 1 else (200, {}, completed('override done'))
        if self.mode == 'quota':
            return 429, {}, b'{"error":{"code":"insufficient_quota"}}'
        raise AssertionError('unknown fixture mode')

    def run(self, session=None):
        command = [str(self.binary), 'run', '--json']
        if session:
            command += ['--session', session]
        command += ['T54 offline retry fixture']
        result = subprocess.run(command, cwd=self.project, env=self.env, capture_output=True, timeout=20)
        text = (result.stdout + result.stderr).decode()
        self.safe(text)
        return result.returncode, text

    @staticmethod
    def safe(text):
        assert all(value not in text for value in ('auth-canary', 'body-canary', 'env-canary', 'private.invalid'))

    def db(self):
        databases = list((self.home/'data').rglob('oc.sqlite'))
        assert len(databases) == 1
        return sqlite3.connect(databases[0])

    def facts(self):
        with self.db() as db:
            spans = [json.loads(row[0])['spans'] for row in db.execute('SELECT result FROM turns WHERE result IS NOT NULL ORDER BY rowid')]
            counted = dict(db.execute("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
            intents = db.execute('SELECT count(*) FROM tool_operations').fetchone()[0]
            retry_events = [json.loads(row[0]) for row in db.execute("SELECT payload FROM events WHERE kind='retry_scheduled'")]
        sockets = {lane: len(requests) for lane, requests in self.lanes.items()}
        for lane, count in counted.items():
            assert count == sockets[lane], ('dispatch/socket mismatch', lane, count, sockets[lane])
        assert sum(counted.values()) == sum(sockets.values())
        known_spans = {span['id'] for turn in spans for span in turn}
        assert all(item['span'] in known_spans for item in retry_events)
        self.safe(json.dumps(spans) + json.dumps(retry_events))
        return {'sockets': sockets, 'dispatches': counted, 'spans': spans, 'tool_intents': intents}

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        self.temp.cleanup()


def wait_for(predicate, timeout=10):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if predicate():
            return
        time.sleep(.02)
    raise AssertionError('bounded fixture condition not reached')


def pty_case(fixture):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
    process = subprocess.Popen([str(fixture.binary)], cwd=fixture.project, env=fixture.env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    os.close(slave)
    capture = bytearray()

    def drain():
        # Drain only bytes ready now: a 16 ms animation otherwise keeps a
        # 20 ms blocking read loop alive past the retry phase being asserted.
        while select.select([master], [], [], 0)[0]:
            try:
                block = os.read(master, 65536)
            except OSError:
                break
            if not block:
                break
            capture.extend(block)
            assert len(capture) < 4*1024*1024
        return capture.decode(errors='replace')

    try:
        wait_for(lambda: 'opencode' in drain().lower() or len(capture) > 100)
        os.write(master, b'T54 offline parked retry\r')
        wait_for(lambda: 'Retry scheduled' in drain())
        with fixture.db() as db:
            raw = db.execute('SELECT result FROM turns').fetchone()[0]
            assert json.loads(raw)['spans'][-1]['retry']['attempt'] == 2
        if fixture.mode == 'restart':
            process.kill()
            process.wait(timeout=5)
            before = {lane: len(rows) for lane, rows in fixture.lanes.items()}
            second_master, second_slave = pty.openpty()
            fcntl.ioctl(second_slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
            second = subprocess.Popen([str(fixture.binary)], cwd=fixture.project, env=fixture.env, stdin=second_slave, stdout=second_slave, stderr=second_slave, start_new_session=True)
            os.close(second_slave)
            try:
                time.sleep(.5)
                assert second.poll() is None
                with fixture.db() as db:
                    status, raw = db.execute('SELECT status,result FROM turns').fetchone()
                    assert status == 'unknown'
                    span = json.loads(raw)['spans'][-1]
                    assert span['completed'] is not None
                    due = span['retry']['at']
                wait_for(lambda: time.time()*1000 > due, timeout=10)
                time.sleep(.05)
                assert before == {lane: len(rows) for lane, rows in fixture.lanes.items()}, 'restart dispatched cached retry after due'
            finally:
                second.terminate()
                second.wait(timeout=5)
                os.close(second_master)
            return {'case': 'pty-restart-no-resume', 'cached_retry_dispatched': False, **fixture.facts()}
        os.write(master, b'\x18n')
        time.sleep(.2)
        drain()
        wait_for(lambda: len(fixture.lanes['main']) == 3, 12)
        time.sleep(.2)
        drain()
        # Re-select the original first tab using its real SGR mouse input route.
        os.write(master, b'\x1b[<0;5;1M\x1b[<0;5;1m')
        wait_for(lambda: 'continued' in drain())
        fixture.safe(capture.decode(errors='replace'))
        facts = fixture.facts()
        assert facts['spans'][0][0]['status'] == 'failed'
        assert facts['spans'][0][0]['retry']['attempt'] == 3
        return {'case': 'pty-retry-park-reopen', 'retry_visible': True, 'parked_completion_visible': True, **facts}
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        os.close(master)


def main():
    binary = (ROOT/(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/oc')).resolve()
    executable = binary.read_bytes()
    assert executable[:4] == b'\x7fELF'
    results = []
    for mode in ('mixed', 'override', 'quota', 'effect', 'child', 'compaction', 'title_failure', 'park', 'restart'):
        fixture = Fixture(binary, mode)
        try:
            if mode in ('park', 'restart'):
                results.append(pty_case(fixture))
                continue
            code, text = fixture.run()
            assert code == (1 if mode == 'quota' else 0), (mode, 'exit', code)
            retry_events = []
            for line in text.splitlines():
                try:
                    value = json.loads(line)
                except ValueError:
                    continue
                if isinstance(value, dict) and value.get('type') == 'retry':
                    retry_events.append(value)
            if mode == 'compaction':
                with fixture.db() as db:
                    session = db.execute('SELECT id FROM sessions WHERE parent_id IS NULL').fetchone()[0]
                code, text = fixture.run(session)
                assert code == 0
            facts = fixture.facts()
            if mode == 'mixed':
                assert facts['sockets']['main'] == 3
                assert fixture.lanes['main'][0] == fixture.lanes['main'][1]
                assert 'partial' in json.dumps(fixture.lanes['main'][2]['input'])
                assert facts['spans'][0][0]['retry']['attempt'] == 3
                assert facts['spans'][0][0]['finish'] == 'error'
                assert facts['spans'][0][0]['error'] is not None
                assert facts['spans'][0][-1]['retry'] is None
                assert facts['spans'][0][-1]['finish'] == 'stop'
                assert facts['spans'][0][-1]['error'] is None
                assert [item['attempt'] for item in retry_events] == [2, 3]
            elif mode == 'override':
                assert facts['sockets']['main'] == 2
                assert [item['attempt'] for item in retry_events] == [2]
            elif mode == 'quota':
                assert facts['sockets']['main'] == 1 and not retry_events
            elif mode == 'child':
                assert facts['sockets']['main'] == 2 and facts['sockets']['child'] == 2
                assert facts['tool_intents'] == 1
            elif mode == 'effect':
                assert facts['sockets']['main'] == 3 and facts['tool_intents'] == 1
                assert [item['attempt'] for item in retry_events] == [2]
                assert (fixture.project/'retry-effects').read_text() == 'effect'
                continuation = fixture.lanes['main'][2]['input']
                assert any(item.get('type') == 'function_call' and item.get('call_id') == 'once' for item in continuation)
                assert any(item.get('type') == 'function_call_output' and item.get('call_id') == 'once' for item in continuation)
                facts['committed_effect_count'] = 1
            elif mode == 'title_failure':
                assert facts['sockets']['main'] == 1 and facts['sockets']['title'] == 1
            elif mode == 'compaction':
                assert facts['sockets']['compaction'] == 4
                assert 'invalid candidate' not in json.dumps(fixture.lanes['compaction'][2]['input'])
                assert fixture.lanes['compaction'][2] == fixture.lanes['compaction'][3]
            results.append({'case': mode, 'exit': code, **facts})
        finally:
            fixture.close()
    print(json.dumps({'binary': str(binary.relative_to(ROOT)), 'sha256': hashlib.sha256(executable).hexdigest(),
                      'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                      'source_dirty': True, 'cases': results, 'scope': 'T54 backend functional proof; VIS43 external paired visuals pending'}, indent=2))


if __name__ == '__main__':
    main()
