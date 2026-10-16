#!/usr/bin/env python3
"""Direct normal-ELF T55 R3 proof. Main uses synthetic loopback fixtures only.

Relay accepts an explicit campaign and RAM manifest; it never creates/resets one.
Only Fixture's offline case creates a disposable fake campaign. No env/key loading.
"""
import ctypes
import fcntl
import hashlib
import http.client
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
sys.path.insert(0, str(ROOT/'scripts'))
import bounded_live as bounded

TITLE = "Generate a short session title from the user's request. Output only the title, in at most 100 characters."
EXPECTED = b'hello\n'


def event(value):
    return ('data: '+json.dumps(value)+'\n\n').encode()


def terminal(output=None):
    value = {'type': 'response.completed', 'response': {'status': 'completed', 'usage': {'input_tokens': 100, 'output_tokens': 40}}}
    if output is not None:
        value['response']['output'] = output
    return event(value)


def message(text):
    return {'type': 'message', 'role': 'assistant', 'status': 'completed', 'content': [{'type': 'output_text', 'text': text}]}


def wait_for(predicate, timeout=10):
    until = time.monotonic()+timeout
    while time.monotonic() < until:
        if predicate():
            return
        time.sleep(.02)
    raise AssertionError('bounded_fixture_condition')


def safe(text):
    assert all(value not in text for value in ('synthetic-canary', 'BODY-CANARY', 'PRIVATE-CANARY', 'private.invalid', 'DONE-REPLAY-CANARY', 'TERMINAL-REPLAY-CANARY')), 'unsafe_diagnostic'


def reap(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
    process.wait(timeout=5)
    assert process.poll() is not None


def group_gone(pid):
    try:
        os.killpg(pid, 0)
    except ProcessLookupError:
        return
    raise AssertionError('owned_group_still_live')


class Relay:
    """Explicit trusted helper lease, reusable by a separately authorized caller."""
    def __init__(self, campaign, manifest, offline=True):
        self.campaign = campaign
        prior = bounded.Ledger(campaign).snapshot()
        args = [sys.executable, '-B', str(ROOT/'scripts/bounded_live.py'), 'serve', '--campaign', campaign]
        args += ['--offline'] if offline else ['--live-opt-in', 'bounded-v1']
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        env={'PATH': '/usr/bin:/bin'}, start_new_session=True)
        try:
            self.process.stdin.write((json.dumps(manifest)+'\n').encode())
            self.process.stdin.flush()
            assert select.select([self.process.stdout], [], [], 5)[0], 'relay_ready_timeout'
            raw = self.process.stdout.readline(65536)
            self.ready = json.loads(raw)
            assert self.ready['id'] == prior['id'] and self.ready['provider_base'].startswith('http://127.0.0.1:')
        except BaseException:
            self.close()
            raise

    def snapshot(self):
        return bounded.Ledger(self.campaign).snapshot()

    def close(self):
        if self.process.stdin and not self.process.stdin.closed:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait(timeout=5)
        assert self.process.returncode == 0, 'relay_shutdown'
        group_gone(self.process.pid)
        self.process.stdout.close()
        self.process.stderr.close()


class Fixture:
    def __init__(self, binary, mode, reasoning_replay=False):
        self.binary, self.mode = binary, mode
        self.reasoning_replay = reasoning_replay
        self.temp = tempfile.TemporaryDirectory(prefix='t55-native-', dir=TMP)
        self.root = Path(self.temp.name)
        self.home, self.project = self.root/'home', self.root/'project'
        self.project.mkdir()
        (self.home/'config/opencode').mkdir(parents=True)
        self.requests, self.titles, self.errors, self.tool_names = [], [], [], set()
        self.lock, self.stop = threading.Lock(), threading.Event()
        self.relay = None
        fixture = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers['Content-Length'])
                    assert 0 < length <= 1024*1024
                    request = json.loads(self.rfile.read(length))
                    assert self.path == '/v1/responses' and request['model'] == 'gpt-fixture-model'
                    assert self.headers['Authorization'] == 'Bearer synthetic-canary'
                    title = any(item.get('role') == 'developer' and any(part.get('text') == TITLE for part in item.get('content', []) if isinstance(part, dict)) for item in request['input'])
                    with fixture.lock:
                        lane = fixture.titles if title else fixture.requests
                        lane.append(request)
                        number = len(lane)
                        fixture.tool_names.update(tool['name'] for tool in request.get('tools', []))
                        if fixture.relay:
                            state = fixture.relay.snapshot()
                            assert state['counts']['generation'] >= len(fixture.requests)+len(fixture.titles), 'reserve_before_upstream'
                    status, body, held = (200, terminal([message('Fixture title')]), False) if title else fixture.response(number, request)
                    self.send_response(status)
                    self.send_header('content-type', 'text/event-stream' if status == 200 else 'application/json')
                    self.send_header('content-length', str(len(body)+(1 if held else 0)))
                    self.send_header('x-should-retry', 'true' if status == 503 or fixture.mode == 'conflict' else 'false')
                    self.end_headers()
                    self.wfile.write(body)
                    self.wfile.flush()
                    if held:
                        fixture.stop.wait(20)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except Exception:
                    fixture.errors.append('fixture_request_rejected')
                    self.close_connection = True

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.daemon_threads = False  # server_close joins owned HTTP workers.
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.env = {'HOME': str(self.home), 'XDG_CONFIG_HOME': str(self.home/'config'),
                    'XDG_DATA_HOME': str(self.home/'data'), 'XDG_STATE_HOME': str(self.home/'state'),
                    'OC_TEST_ALLOW_LOOPBACK': '1', 'PATH': '/usr/bin:/bin', 'TERM': 'xterm-256color'}
        self.configure()

    def configure(self):
        base = self.relay.ready['provider_base'] if self.relay else f'http://127.0.0.1:{self.server.server_port}/v1'
        key = bounded.PLACEHOLDER if self.relay else 'synthetic-canary'
        value = {'model': 'fixture/gpt-fixture-model', 'compaction': {'auto': False},
                 'permission': {'read': 'allow', 'edit': 'allow', 'apply_patch': 'allow', 'shell': 'allow', 'bash': 'allow'},
                 'provider': {'fixture': {'npm': '@ai-sdk/openai', 'options': {'baseURL': base, 'apiKey': key},
                              'models': {'gpt-fixture-model': {'limit': {'context': 65536, 'output': 2048}}}}}}
        (self.home/'config/opencode/opencode.json').write_text(json.dumps(value))

    def response(self, number, request):
        if self.mode == 'relay' and number == 1:
            return 503, b'{"error":{"code":"server_error"},"raw":"BODY-CANARY"}', False
        step = number-(1 if self.mode == 'relay' else 0)
        if self.mode == 'unknown':
            if number == 1:
                call = {'type': 'function_call', 'id': 'unknown-item', 'call_id': 'unknown-call', 'name': 'bash',
                        'arguments': json.dumps({'argv': ['/bin/sh', '-c', 'printf effect >> unknown-effects; printf "%s" "$$" > unknown.pid; sleep 30']}), 'status': 'completed'}
                return 200, event({'type': 'response.output_item.done', 'output_index': 0, 'item': call})+terminal([]), False
            assert number == 2
            return 200, terminal([message('After recovery')]), False
        if step == 1:
            events = json.loads((ROOT/'evidence/T55/sparse-completed.fixture.json').read_text())['events']
            call = events[-2]['item']
            body = b''.join(event(value) for value in events[:-1])
            if self.mode == 'unclosed':
                return 200, body, True
            if self.mode == 'eof':
                return 200, body, False
            if self.mode == 'conflict':
                changed = dict(call, arguments=json.dumps({'patchText': 'PRIVATE-CANARY'}))
                return 200, body+terminal([changed]), False
            return 200, body+terminal([call] if self.mode == 'pty' else []), False
        if step == 2:
            self.assert_result(request, 'call_probe')
            call = {'type': 'function_call', 'id': 'fc_read', 'call_id': 'call_read', 'name': 'read', 'arguments': json.dumps({'path': 'probe.txt'}), 'status': 'completed'}
            opaque = {'type': 'reasoning', 'id': 'reasoning', 'status': 'completed', 'encrypted_content': 'OPAQUE-FIXTURE'}
            if self.reasoning_replay:
                opaque.update(summary=[], encrypted_content='DONE-REPLAY-CANARY')
            body = event({'type': 'response.output_item.done', 'output_index': 0, 'item': opaque})
            if self.reasoning_replay or self.mode == 'pty':
                body += event({'type': 'response.output_item.done', 'output_index': 1, 'item': call})
            if self.reasoning_replay:
                return 200, body+terminal([dict(opaque, encrypted_content='TERMINAL-REPLAY-CANARY'), call]), False
            return 200, body+terminal(None if self.mode == 'pty' else [call]), False
        assert step in (3, 4), 'unexpected_main_post'
        self.assert_result(request, 'call_probe')
        self.assert_result(request, 'call_read')
        if self.reasoning_replay:
            items = [item for item in request['input'] if item.get('type') == 'reasoning' and item.get('id') == 'reasoning']
            assert items == [{'type': 'reasoning', 'id': 'reasoning', 'status': 'completed', 'summary': [], 'encrypted_content': 'DONE-REPLAY-CANARY'}], 'actual_done_reasoning_replay'
            assert 'TERMINAL-REPLAY-CANARY' not in json.dumps(request), 'terminal_ciphertext_replayed'
        return 200, terminal([message('Final fixture response' if step == 3 else 'Reopened fixture response')]), False

    @staticmethod
    def assert_result(request, call_id):
        calls = [item for item in request['input'] if item.get('type') == 'function_call' and item.get('call_id') == call_id]
        results = [item for item in request['input'] if item.get('type') == 'function_call_output' and item.get('call_id') == call_id]
        assert len(calls) == len(results) == 1, 'canonical_pair_count'

    def db(self):
        matches = list((self.home/'data').rglob('oc.sqlite'))
        assert len(matches) == 1
        return sqlite3.connect(matches[0])

    def facts(self):
        with self.db() as db:
            operations = list(db.execute('SELECT name,state FROM tool_operations ORDER BY rowid'))
            turns = list(db.execute('SELECT status FROM turns ORDER BY rowid'))
            retries = db.execute("SELECT count(*) FROM events WHERE kind='retry_scheduled'").fetchone()[0]
            dispatched = db.execute("SELECT count(*) FROM events WHERE kind='generation_dispatched'").fetchone()[0]
            diagnostics = list(db.execute("SELECT result FROM turns WHERE result IS NOT NULL"))
        # Inspect only safe span errors, not private canonical opaque input.
        for (raw,) in diagnostics:
            for span in json.loads(raw).get('spans', []):
                safe(json.dumps({'error': span.get('error'), 'retry': span.get('retry')}))
        assert not self.errors, 'fixture_request_rejected'
        return {'main_posts': len(self.requests), 'title_posts': len(self.titles), 'dispatches': dispatched,
                'operations': operations, 'turn_statuses': [row[0] for row in turns], 'retry_events': retries}

    def session(self):
        with self.db() as db:
            return db.execute('SELECT id FROM sessions WHERE parent_id IS NULL').fetchone()[0]

    def headless(self, session=None):
        args = [str(self.binary), 'run', '--json']
        if session:
            args += ['--session', session]
        process = subprocess.Popen(args+['T55 synthetic tool qualification'], cwd=self.project, env=self.env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            stdout, stderr = process.communicate(timeout=20)
            safe((stdout+stderr).decode())
            return process.returncode
        finally:
            reap(process)
            group_gone(process.pid)

    def positive(self):
        assert self.headless() == 0
        self.assert_success(1, 3)
        session = self.session()
        if self.relay:
            prior = self.relay.snapshot()
            self.relay.close()
            self.relay = Relay(self.campaign, self.manifest)
            assert self.relay.snapshot() == prior and self.relay.ready['id'] == prior['id'], 'same_identity_no_reset'
            self.configure()
        assert self.headless(session) == 0
        self.assert_success(2, 4)
        return self.facts()

    def assert_success(self, turns, posts):
        facts = self.facts()
        assert facts['operations'] == [('apply_patch', 'completed'), ('read', 'completed')]
        assert facts['turn_statuses'] == ['completed']*turns
        assert facts['main_posts'] == posts+(1 if self.mode == 'relay' else 0)
        assert facts['retry_events'] == (1 if self.mode == 'relay' else 0)
        assert (self.project/'probe.txt').read_bytes() == EXPECTED, 'independent_file_bytes'
        assert facts['dispatches'] == facts['main_posts']+facts['title_posts']
        if self.reasoning_replay:
            with self.db() as db:
                logs = [json.loads(raw) for (raw,) in db.execute('SELECT result FROM turns WHERE result IS NOT NULL')]
            opaque = [item for log in logs for item in log.get('input', []) if item.get('type') == 'reasoning' and item.get('id') == 'reasoning']
            assert len(opaque) == 1 and opaque[0]['encrypted_content'] == 'DONE-REPLAY-CANARY', 'durable_done_reasoning'
            assert 'TERMINAL-REPLAY-CANARY' not in json.dumps(logs), 'terminal_ciphertext_stored'
            for log in logs:
                safe(json.dumps(log.get('display_parts', [])))

    def pty(self):
        master, slave = pty.openpty()
        initial = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
        process = subprocess.Popen([str(self.binary)], cwd=self.project, env=self.env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
        captured = bytearray()

        def drain():
            while select.select([master], [], [], 0)[0]:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    break
                if not chunk:
                    break
                captured.extend(chunk)
                assert len(captured) <= 4*1024*1024
            return bool(captured)

        try:
            wait_for(drain)
            os.write(master, b'T55 synthetic PTY tool qualification\r')
            def completed():
                drain()
                try:
                    with self.db() as db:
                        return db.execute("SELECT count(*) FROM turns WHERE status='completed'").fetchone()[0] == 1
                except (AssertionError, sqlite3.OperationalError):
                    return False
            wait_for(completed)
            self.assert_success(1, 3)
            # Actual clean shutdown restores the original slave termios.
            os.write(master, b'\x04')  # Native DeleteOrQuit on the idle composer.
            process.wait(timeout=5)
            drain()
            assert process.returncode == 0
            assert termios.tcgetattr(slave) == initial, 'tty_not_restored'
            safe(captured.decode(errors='replace'))
        finally:
            reap(process)
            group_gone(process.pid)
            os.close(master)
            os.close(slave)
        assert self.headless(self.session()) == 0
        self.assert_success(2, 4)
        return {'tty_restored': True, **self.facts()}

    def negative(self):
        if self.mode == 'conflict':
            assert self.headless() == 1
        else:
            process = subprocess.Popen([str(self.binary), 'run', '--json', 'T55 unclosed fixture'], cwd=self.project,
                                        env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                wait_for(lambda: len(self.requests) == 1)
                time.sleep(.1)
                with self.db() as db:
                    assert db.execute('SELECT count(*) FROM tool_operations').fetchone()[0] == 0
                if self.mode == 'eof':
                    wait_for(lambda: self.facts()['retry_events'] == 1, timeout=5)
                process.send_signal(signal.SIGINT)
                stdout, stderr = process.communicate(timeout=5)
                safe((stdout+stderr).decode())
                assert process.returncode == 130
                if self.mode == 'eof':
                    retries = []
                    for line in stderr.splitlines():
                        try:
                            value = json.loads(line)
                        except ValueError:
                            continue
                        if isinstance(value, dict) and value.get('type') == 'retry':
                            retries.append(value)
                    assert [value['attempt'] for value in retries] == [2], 'eof_retry_notice'
            finally:
                reap(process)
                group_gone(process.pid)
        facts = self.facts()
        assert not facts['operations'] and not (self.project/'probe.txt').exists()
        assert facts['main_posts'] == 1 and facts['retry_events'] == (1 if self.mode == 'eof' else 0)
        return facts

    def unknown(self):
        # Adopt orphaned owned leaves so a hard-crash fixture can actually reap
        # them. No global process inspection or foreign PID/group signalling.
        assert ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) == 0  # PR_SET_CHILD_SUBREAPER
        process = subprocess.Popen([str(self.binary), 'run', '--json', 'T55 unknown effect'], cwd=self.project,
                                    env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        leaf = None
        try:
            wait_for(lambda: (self.project/'unknown.pid').exists())
            leaf = int((self.project/'unknown.pid').read_text())
            group = os.getpgid(leaf)
            assert group == leaf and leaf != process.pid, 'owned_leaf_group'
            with self.db() as db:
                assert db.execute("SELECT count(*) FROM tool_operations WHERE state='started'").fetchone()[0] == 1
            assert (self.project/'unknown-effects').read_bytes() == b'effect'
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
            os.killpg(group, signal.SIGKILL)
            # Linux subreaper owns the orphaned shell/sleep; reap every adopted
            # member of that exact group, never a foreign or unscoped wait.
            until = time.monotonic()+5
            while time.monotonic() < until:
                try:
                    pid, _ = os.waitpid(-group, os.WNOHANG)
                except ChildProcessError:
                    break
                if pid == 0:
                    time.sleep(.02)
            else:
                raise AssertionError('owned_leaf_join')
            group_gone(group)
            leaf = None
            assert self.headless(self.session()) == 0
            facts = self.facts()
            assert facts['operations'] == [('bash', 'unknown')]
            assert facts['turn_statuses'] == ['unknown', 'completed']
            assert facts['main_posts'] == 2 and facts['retry_events'] == 0
            assert (self.project/'unknown-effects').read_bytes() == b'effect', 'unknown_effect_replayed'
            return {'unknown_effect_count': 1, 'owned_leaf_joined': True, **facts}
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
            reap(process)
            group_gone(process.pid)
            if leaf is not None:
                try:
                    os.killpg(leaf, signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def relay_case(self):
        # Explicitly fresh offline-only root, never the parent's real campaign.
        self.campaign = bounded.Ledger.create(str(self.root/'offline-envelope'))
        upstream = f'http://127.0.0.1:{self.server.server_port}'
        self.manifest = {'provider': {'generation_url': upstream+'/v1/responses', 'discovery_url': upstream+'/v1/models', 'headers': {'authorization': 'Bearer synthetic-canary'}},
                         'mcp': {'fixture_search': {'url': upstream+'/mcp', 'headers': {}}}}
        self.relay = Relay(self.campaign, self.manifest)
        self.configure()
        before = self.relay.snapshot()
        address = self.relay.ready['provider_base'].removeprefix('http://').split('/')[0]
        for malformed in [{'stream': True, 'max_output_tokens': True}, {'stream': 'true', 'max_output_tokens': 2048}, {'stream': True, 'max_output_tokens': 2048, 'tools': {}}]:
            connection = http.client.HTTPConnection(address, timeout=3)
            try:
                connection.request('POST', '/provider/responses', json.dumps(malformed), {'content-type': 'application/json'})
                response = connection.getresponse()
                assert response.status == 403
                value = json.loads(response.read())
                assert value['error'] == 'bounded_envelope'
            finally:
                connection.close()
        assert self.relay.snapshot() == before and not self.requests and not self.titles, 'pre_effect_rejection'
        result = self.positive()
        wait_for(lambda: all(attempt['outcome'] != 'reserved' for attempt in self.relay.snapshot()['attempts']))
        state = self.relay.snapshot()
        assert state['counts']['generation'] == result['main_posts']+result['title_posts']
        assert state['counts']['control'] == state['counts']['mcp'] == 0
        assert sum(attempt.get('http_status') == 503 for attempt in state['attempts']) == 1
        return {'wrong_type_upstream_requests': 0, 'same_campaign_restart': True, 'reserve_visible_before_upstream': True,
                'relay_counts': state['counts'], 'relay_receipts': [{key: attempt[key] for key in ('kind', 'outcome', 'http_status', 'failure', 'failure_stage') if key in attempt} for attempt in state['attempts']],
                'providerToolNames': sorted(self.tool_names), **result}

    def close(self):
        if self.relay:
            self.relay.close()
            self.relay = None
        self.stop.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        assert not self.thread.is_alive(), 'owned_server_join'
        self.temp.cleanup()


def main():
    assert os.getuid() != 0
    assert not sys.argv[2:] or sys.argv[2:] == ['--reasoning-replay']
    reasoning_replay = '--reasoning-replay' in sys.argv[2:]
    binary = (ROOT/(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/oc')).resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    cases = []
    for mode in ('headless', 'pty', 'conflict', 'unclosed', 'eof', 'unknown', 'relay'):
        fixture = Fixture(binary, mode, reasoning_replay=reasoning_replay)
        try:
            result = fixture.positive() if mode == 'headless' else getattr(fixture, 'relay_case' if mode == 'relay' else ('negative' if mode in ('conflict', 'unclosed', 'eof') else mode))()
            cases.append({'case': mode, **result})
        finally:
            fixture.close()
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == digest
    print(json.dumps({'binary': str(binary.relative_to(ROOT)), 'sha256': digest, 'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                       'source_dirty': True, 'network': 'synthetic loopback only', 'owned_processes_and_servers_joined': True,
                       'reasoning_replay': reasoning_replay,
                      'cases': cases, 'scope': 'T55 R3 offline; R4 live remains parent-only'}, indent=2))


if __name__ == '__main__':
    main()
