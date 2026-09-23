"""Isolated real-binary discovery and startup PTY; never calls a paid provider."""
import errno
import fcntl
import json
import os
import pty
import re
import select
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

binary = str(Path(sys.argv[1]).resolve())
base = '/home/opencode/.cache/opencode-tmp/opencode'
assert Path(base).is_dir()
key = 'DUMMY-DISCOVERY-CREDENTIAL'
body_marker = 'DUMMY-DISCOVERY-PRIVATE-BODY'
chosen = 'fixture/new'


def drain(fd, seconds=.65):
    end = time.monotonic() + seconds
    chunks = []
    while time.monotonic() < end:
        if select.select([fd], [], [], .04)[0]:
            try:
                chunks.append(os.read(fd, 65536))
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
    return b''.join(chunks).decode('utf-8', 'replace')


with tempfile.TemporaryDirectory(prefix='oc-discovery-', dir=base) as tmp:
    root = Path(tmp)
    home = root / 'home'
    project = root / 'project'
    config = home / 'config' / 'opencode'
    project.mkdir()
    config.mkdir(parents=True)
    (home / 'data').mkdir()
    requests = []
    response = [401, body_marker.encode(), False]

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append((self.command, self.path,
                             self.headers.get('Authorization') == 'Bearer ' + key))
            status, body, slow = response
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            if slow:
                time.sleep(1.5)
            try:
                self.wfile.write(body)
            except OSError:
                # A client that rejects the status need not read this body.
                pass

        def do_POST(self):
            requests.append(('POST', '', False))
            self.send_error(500)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        fixture = {
            'model': 'ludka2/' + chosen,
            'provider': {'ludka2': {
                'npm': '@ai-sdk/openai', 'name': 'Fixture',
                'options': {'baseURL': 'http://127.0.0.1:' + str(server.server_port) + '/v1',
                            'apiKey': '{env:FIXTURE_DISCOVERY_KEY}'},
            }},
        }
        (config / 'opencode.json').write_text(json.dumps(fixture))
        env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
               'XDG_DATA_HOME': str(home / 'data'), 'TERM': 'xterm-256color',
               'LANG': 'C.UTF-8', 'PATH': '/usr/bin:/bin',
               'FIXTURE_DISCOVERY_KEY': key, 'OC_TEST_ALLOW_LOOPBACK': '1'}
        catalog = lambda ident: json.dumps({'object': 'list', 'data': [
            {'id': ident, 'context_length': 10000, 'max_completion_tokens': 1000}
        ]}).encode()
        cases = [
            ('unauthorized', 401, body_marker.encode(), False, 'Model discovery authentication rejected (401)',
              'HTTP 401'),
            ('forbidden', 403, body_marker.encode(), False, 'Model discovery access forbidden (403)',
              'HTTP 403'),
            ('unauthorized-oversized', 401, body_marker.encode() * (9 * 1024 * 1024 // len(body_marker) + 1),
             False, 'Model discovery authentication rejected (401)', 'HTTP 401'),
            ('forbidden-oversized', 403, body_marker.encode() * (9 * 1024 * 1024 // len(body_marker) + 1),
             False, 'Model discovery access forbidden (403)', 'HTTP 403'),
            ('unauthorized-slow', 401, body_marker.encode(), True,
             'Model discovery authentication rejected (401)', 'HTTP 401'),
            ('forbidden-slow', 403, body_marker.encode(), True,
             'Model discovery access forbidden (403)', 'HTTP 403'),
            ('absent', 200, catalog('fixture/other'), False, 'Selected model absent from catalog',
             'unknown model'),
            ('present', 200, catalog(chosen), False, '█▀▀█', None),
        ]
        for label, status, payload, slow, expected, detailed in cases:
            response[:] = [status, payload, slow]
            requests.clear()
            trace_path = root / ('startup-trace-' + label + '.log')
            child_env = dict(env, OC_STARTUP_TRACE=str(trace_path))
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
            original = termios.tcgetattr(slave)
            child = subprocess.Popen([binary, '--data-dir', str(home / ('data-' + label))],
                                     cwd=project, env=child_env, stdin=slave, stdout=slave,
                                     stderr=slave)
            try:
                screen = drain(master)
                assert child.poll() is None, (label, 'premature exit')
                assert expected in screen, (label, 'wrong startup category')
                assert (label == 'present') == ('Native startup error' not in screen), (label, 'unexpected home/error')
                for marker in (key, body_marker, str(root), 'Authorization:'):
                    assert marker not in screen, (label, 'private data reached TUI')
                if label != 'present':
                    assert chosen not in screen, (label, 'selected id reached error UI')
                os.write(master, b'\x03')
                drain(master, .15)
                assert child.wait(timeout=5) == (0 if label == 'present' else 1), (label, 'exit code')
                assert termios.tcgetattr(slave) == original, (label, 'terminal state')
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)
                os.close(master)
                os.close(slave)
            assert trace_path.exists(), (label, 'startup trace missing')
            trace = trace_path.read_bytes()
            for marker in (key, body_marker):
                assert marker.encode() not in trace, (label, 'private data reached trace')
            trace_text = trace.decode('utf-8', 'replace')
            assert 'startup.begin' in trace_text, (label, 'startup stage missing')
            if label == 'unauthorized':
                assert re.search(
                    r'discovery\.attempt: n=\d+ status=401 class=Unauthorized',
                    trace_text), (label, 'no 401 discovery attempt line')
                assert 'discovery.fail: class=Unauthorized' in trace_text, (label, 'no discovery fail line')
                assert 'spawn.fail: category=DiscoveryUnauthorized' in trace_text, (label, 'no typed spawn fail')
                assert 'tui.exit: code=1' in trace_text, (label, 'no failing tui exit')
            if label == 'present':
                assert 'discovery.ok: models=1 selected_present=true' in trace_text, (label, 'no discovery ok')
                assert 'spawn.ok' in trace_text, (label, 'no spawn.ok')
                assert 'tui.begin' in trace_text, (label, 'no tui.begin')
                assert 'tui.exit: code=0' in trace_text, (label, 'no clean tui exit')
            assert requests == [('GET', '/v1/models', True)], (label, 'wrong discovery wire')
            if detailed:
                requests.clear()
                headless_trace = root / ('startup-trace-headless-' + label + '.log')
                headless_env = dict(env, OC_STARTUP_TRACE=str(headless_trace))
                result = subprocess.run([binary, 'run', 'no network call'],
                                        cwd=project, env=headless_env, capture_output=True, timeout=5)
                assert result.returncode == 1 and detailed.encode() in result.stderr, (label, 'headless detail')
                assert key.encode() not in result.stderr and body_marker.encode() not in result.stderr, (label, 'headless leak')
                assert headless_trace.exists(), (label, 'headless trace missing')
                headless_bytes = headless_trace.read_bytes()
                for marker in (key, body_marker):
                    assert marker.encode() not in headless_bytes, (label, 'private data reached headless trace')
                assert b'headless.exit: code=1' in headless_bytes, (label, 'no headless exit line')
                assert requests == [('GET', '/v1/models', True)], (label, 'headless discovery wire')
            print(label + ': discovery GET, no Responses, expected exit, terminal restored')
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)
