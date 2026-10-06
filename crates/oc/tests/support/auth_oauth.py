"""Non-default auth-fixture ELF: local fake approval and actual native requests.

No host/DNS/TLS/proxy interception, real credentials, cloud traffic or private dumps.
"""
import base64
import fcntl
import hashlib
import http.server
import json
import os
import pathlib
import pty
import select
import signal
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import urllib.parse
import urllib.request

BINARY = os.path.abspath(sys.argv[1])
ACCESS, REFRESH, ACCOUNT, KEY = ('ACCESS_PRIVATE_CANARY', 'REFRESH_PRIVATE_CANARY', 'ACCOUNT_PRIVATE_CANARY', 'KEY_PRIVATE_CANARY')
LOCK = threading.Lock()
CONTROL, TOKENS, FRAMES, HTTP = [], [], [], []
APPROVED = threading.Event()
CLAIM = base64.urlsafe_b64encode(json.dumps({'chatgpt_account_id': ACCOUNT}).encode()).decode().rstrip('=')


def wait(predicate, label, seconds=12):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if predicate():
            return
        time.sleep(.01)
    raise AssertionError(label)


def callback_ports(pid):
    sockets = set()
    for fd in pathlib.Path('/proc/' + str(pid) + '/fd').iterdir():
        try:
            sockets.add(os.readlink(fd))
        except FileNotFoundError:
            pass
    owned = set()
    for row in pathlib.Path('/proc/net/tcp').read_text().splitlines()[1:]:
        fields = row.split()
        address, port = fields[1].split(':')
        if fields[3] == '0A' and address == '0100007F' and 'socket:[' + fields[9] + ']' in sockets:
            owned.add(int(port, 16))
    return owned & {1455, 1457}


class Peer(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *args):
        pass

    def body(self):
        length = int(self.headers['Content-Length'])
        assert length <= 2 * 1024 * 1024, 'fixture body bound'
        return self.rfile.read(length)

    def reply(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        body = self.body()
        if self.path == '/oauth/token':
            with LOCK:
                TOKENS.append(urllib.parse.parse_qs(body.decode()))
                assert len(TOKENS) <= 8, 'token exchange bound'
            self.reply(200, {'id_token': 'x.' + CLAIM + '.x', 'access_token': ACCESS, 'refresh_token': REFRESH, 'expires_in': 3600})
        elif self.path.startswith('/api/accounts/deviceauth/'):
            value = json.loads(body)
            with LOCK:
                CONTROL.append((self.path, time.monotonic(), value))
                count = sum(path.endswith('/token') for path, _, _ in CONTROL)
                assert len(CONTROL) <= 16, 'control request bound'
            if self.path.endswith('/usercode'):
                self.reply(200, {'device_auth_id': 'device-fixture', 'user_code': 'SAFE-USER-CODE', 'interval': '1'})
            elif count == 1:
                self.reply(403, {})
            elif count == 2 or not APPROVED.is_set():
                self.reply(404, {})
            else:
                self.reply(200, {'authorization_code': 'SYNTHETIC_AUTHORIZATION_CODE', 'code_verifier': 'SYNTHETIC_VERIFIER'})
        else:
            assert self.path in ('/key/responses', '/codex/responses'), 'owned native HTTP route'
            value = json.loads(body)
            with LOCK:
                HTTP.append((self.path, dict(self.headers), value))
                assert len(HTTP) + len(FRAMES) <= 24, 'physical model request bound'
            events = self.events(value)
            encoded = ''.join('data: ' + json.dumps(event) + '\n\n' for event in events).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    def events(self, value):
        assert value['model'] == 'gpt-5.5', 'captured API model'
        assert int(value.get('max_output_tokens', 2048)) <= 2048, 'fixture smoke output cap'
        source = json.dumps(value.get('input'))
        with LOCK:
            number = len(FRAMES) + len(HTTP)
        id = 'fixture-r' + str(number)
        created = {'type': 'response.created', 'response': {'id': id}}
        has_result = 'function_call_output' in source
        if has_result:
            assert 'NATIVE_NOTE' in source, 'actual paired read result'
        if not has_result and 'REOPEN_ONLY' not in source and value.get('tools'):
            assert any(tool.get('name') == 'read' for tool in value['tools']), 'native read exposure'
            item = {'id': id + '-read', 'type': 'function_call', 'call_id': id + '-call', 'name': 'read',
                    'arguments': '{"path":"note.txt"}', 'status': 'completed'}
            added = dict(item, arguments='', status='in_progress')
            return [created, {'type': 'response.output_item.added', 'output_index': 0, 'item': added},
                    {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
                    {'type': 'response.completed', 'response': {'id': id, 'status': 'completed', 'output': [item],
                     'usage': {'input_tokens': 10, 'output_tokens': 3}}}]
        text = 'FINAL_NATIVE_NOTE'
        item = {'id': id + '-message', 'type': 'message', 'role': 'assistant', 'status': 'completed',
                'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
        return [created, {'type': 'response.output_item.added', 'output_index': 0, 'item': dict(item, content=[], status='in_progress')},
                {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0, 'delta': text},
                {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
                {'type': 'response.completed', 'response': {'id': id, 'status': 'completed', 'output': [item],
                 'usage': {'input_tokens': 12, 'output_tokens': 4}}}]

    def do_GET(self):
        assert self.path in ('/key/responses', '/codex/responses'), 'only owned native fixture routes'
        assert self.headers.get('Upgrade', '').lower() == 'websocket', 'native default WS'
        accept = base64.b64encode(hashlib.sha1((self.headers['Sec-WebSocket-Key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
        self.send_response(101)
        self.send_header('Upgrade', 'websocket')
        self.send_header('Connection', 'Upgrade')
        self.send_header('Sec-WebSocket-Accept', accept)
        self.end_headers()
        self.connection.settimeout(5)
        try:
            while True:
                header = self.rfile.read(2)
                if not header:
                    break
                opcode, length = header[0] & 15, header[1] & 127
                if length == 126:
                    length = struct.unpack('!H', self.rfile.read(2))[0]
                elif length == 127:
                    length = struct.unpack('!Q', self.rfile.read(8))[0]
                assert header[1] & 128 and length <= 2 * 1024 * 1024, 'bounded masked client frame'
                mask, data = self.rfile.read(4), self.rfile.read(length)
                data = bytes(byte ^ mask[index % 4] for index, byte in enumerate(data))
                if opcode == 8:
                    self.frame(data, 8)
                    break
                assert opcode == 1, 'native text frame'
                value = json.loads(data)
                assert value['type'] == 'response.create' and 'stream' not in value, 'native channel final body'
                with LOCK:
                    FRAMES.append((self.path, dict(self.headers), value))
                    assert len(HTTP) + len(FRAMES) <= 24, 'physical model request bound'
                for event in self.events(value):
                    self.frame(json.dumps(event).encode())
        except (socket.timeout, BrokenPipeError, ConnectionResetError):
            pass
        self.close_connection = True

    def frame(self, data, opcode=1):
        header = bytes([128 | opcode])
        header += bytes([len(data)]) if len(data) < 126 else b'\x7e' + struct.pack('!H', len(data))
        self.wfile.write(header + data)
        self.wfile.flush()


with tempfile.TemporaryDirectory(prefix='t57-auth-binary-', dir=os.environ['TMPDIR']) as temp:
    root = pathlib.Path(temp)
    (root / 'project').mkdir()
    config = root / 'home/config/opencode'
    config.mkdir(parents=True)
    (root / 'project/note.txt').write_text('NATIVE_NOTE\n')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Peer)
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    origin = 'http://127.0.0.1:' + str(server.server_port)
    env = {'HOME': str(root / 'home'), 'XDG_CONFIG_HOME': str(root / 'home/config'), 'PATH': '/usr/bin:/bin',
           'TERM': 'xterm-256color', 'LANG': 'C.UTF-8', 'OC_AUTH_FIXTURE_ORIGIN': origin}
    args = [BINARY, '--data-dir', str(root / 'data')]

    def run(extra):
        result = subprocess.run(args + extra, cwd=root / 'project', env=env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)
        assert result.returncode == 0, 'native command exit (private diagnostics withheld)'
        assert not any(value.encode() in result.stdout + result.stderr for value in (ACCESS, REFRESH, ACCOUNT, KEY)), 'no credential output'
        return result.stdout

    def pending(method, label):
        process = subprocess.Popen(args + ['auth', 'login', 'openai', '--method', method, '--answer', 'label=' + label],
                                   cwd=root / 'project', env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        output = bytearray()
        def ready():
            if select.select([process.stdout], [], [], .01)[0]:
                output.extend(os.read(process.stdout.fileno(), 8192))
            return b'Waiting for authorization' in output
        wait(ready, 'pending native authorization')
        return process, output

    def finish(process):
        output, error = process.communicate(timeout=12)
        assert process.returncode == 0 and b'Connected to OpenAI' in output, 'durable native authorization acknowledgement'
        assert not any(value.encode() in output + error for value in (ACCESS, REFRESH, ACCOUNT, KEY)), 'no private token metadata'

    try:
        run(['auth', 'list', '--format', 'json'])
        for bad in (None, 'http://localhost:1234', 'https://127.0.0.1:1234',
                    'http://127.0.0.1:1234/path', 'http://user@127.0.0.1:1234', 'http://192.0.2.1:1234'):
            denied_env = dict(env)
            if bad is None:
                denied_env.pop('OC_AUTH_FIXTURE_ORIGIN')
            else:
                denied_env['OC_AUTH_FIXTURE_ORIGIN'] = bad
            denied = subprocess.run(args + ['auth', 'login', 'openai', '--method', 'chatgpt-browser'],
                                    cwd=root / 'project', env=denied_env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5)
            assert denied.returncode == 1 and b'Waiting' not in denied.stdout, 'fixture authority fails closed before attempt'
        assert not CONTROL and not TOKENS and not FRAMES and not HTTP, 'invalid fixture authority cannot dial'
        source = {'source': 'https://models.dev/api.json', 'fetched_at_ms': int(time.time() * 1000), 'record': None,
                  'openai': {'id': 'openai', 'npm': '@ai-sdk/openai', 'models': {'gpt-5.5': {'id': 'gpt-5.5', 'name': 'Native fixture',
                   'limit': {'context': 32000, 'output': 512}, 'tool_call': True}}}}
        with sqlite3.connect(root / 'data/oc.sqlite') as db:
            db.execute('INSERT INTO prefs(key,value,updated_at) VALUES (?,?,?)',
                       ('public-catalog:https://models.dev/api.json:opencode-go:v1', json.dumps(source), '1'))
        device, shown = pending('chatgpt-headless', 'Device')
        assert b'SAFE-USER-CODE' in shown and b'/codex/device' in shown, 'structured native device instructions'
        assert device.poll() is None and not callback_ports(device.pid), 'pipe device waits without callback listener'
        with sqlite3.connect('file:' + str(root / 'data/oc.sqlite') + '?mode=ro', uri=True) as db:
            assert db.execute('SELECT count(*) FROM credential_accounts').fetchone()[0] == 0, 'no account before fake approval'
        APPROVED.set()
        finish(device)
        polls = [row for row in CONTROL if row[0].endswith('/token')]
        assert len(polls) == 3 and all(b[1] - a[1] >= 3.9 for a, b in zip(polls, polls[1:])), 'source pending interval plus margin'
        assert TOKENS[0]['redirect_uri'] == [origin + '/deviceauth/callback'], 'native device form exchange'
        browser, shown = pending('chatgpt-browser', 'Browser')
        url = next(line.decode() for line in shown.splitlines() if line.startswith(b'http://'))
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(url).query)
        callback = query['redirect_uri'][0] + '?' + urllib.parse.urlencode({'code': 'SYNTHETIC_BROWSER_CODE', 'state': query['state'][0]})
        with urllib.request.urlopen(callback, timeout=5) as response:
            assert response.status == 200 and b'Authorization successful' in response.read(), 'native callback durable success page'
        finish(browser)
        assert TOKENS[1]['grant_type'] == ['authorization_code'] and len(TOKENS[1]['code_verifier'][0]) == 43, 'native browser exchange'

        def write_config(http=False):
            value = {'model': 'openai/gpt-5.5', 'disabled_providers': ['opencode-go'], 'permissions': {'read': 'allow'},
                     'provider': {'openai': {'npm': '@ai-sdk/openai', 'options': {'baseURL': 'https://api.openai.com/v1',
                      **({'transport': 'http'} if http else {})}, 'models': {'gpt-5.5': {'name': 'Native fixture'}}}}}
            (config / 'opencode.json').write_text(json.dumps(value))

        write_config()
        assert b'FINAL_NATIVE_NOTE' in run(['run', 'Read note and finish', '--session', 'oauth-root']), 'native OAuth WS read/result/final'
        before = len(FRAMES)
        assert b'FINAL_NATIVE_NOTE' in run(['run', 'REOPEN_ONLY', '--session', 'oauth-root']), 'native OAuth reopen'
        assert len(FRAMES) == before + 1, 'reopen does not replay settled read'

        master, slave = pty.openpty()
        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        key = subprocess.Popen(args + ['auth', 'login', 'openai', '--method', 'key', '--answer', 'label=Key'],
                               cwd=root / 'project', env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=controlling_terminal)
        output = bytearray()
        def prompt():
            if select.select([master], [], [], .01)[0]:
                output.extend(os.read(master, 8192))
            return b'API key:' in output
        wait(prompt, 'native masked key prompt')
        os.write(master, KEY.encode() + b'\r')
        wait(lambda: key.poll() is not None, 'native key durable acknowledgement')
        assert key.returncode == 0, 'key acknowledgement'
        os.close(master)
        os.close(slave)
        assert b'FINAL_NATIVE_NOTE' in run(['run', 'Read note and finish', '--session', 'key-root']), 'native Key WS read/result/final'
        assert any(path.startswith('/key') for path, _, _ in FRAMES) and not HTTP, 'both native auth kinds default to physical WS'
        write_config(True)
        before_ws, before_http = len(FRAMES), len(HTTP)
        assert b'FINAL_NATIVE_NOTE' in run(['run', 'Read note and finish', '--session', 'key-http']), 'native explicit Key HTTP'
        assert len(HTTP) >= before_http + 2 and len(FRAMES) == before_ws, 'explicit Key HTTP is physically SSE, never WS'
        rows = json.loads(run(['auth', 'list', '--format', 'json']))
        assert {row['methodID'] for row in rows} == {'key', 'chatgpt-browser', 'chatgpt-headless'}, 'same native reopened method metadata'
        run(['auth', 'switch', 'openai', next(row['id'] for row in rows if row['label'] == 'Device')])
        before_http = len(HTTP)
        assert b'FINAL_NATIVE_NOTE' in run(['run', 'Read note and finish', '--session', 'oauth-http']), 'native explicit OAuth HTTP'
        assert len(HTTP) >= before_http + 2 and len(FRAMES) == before_ws, 'explicit OAuth HTTP is physically SSE, never WS'
        for path, headers, value in FRAMES + HTTP:
            lower = {key.lower(): val for key, val in headers.items()}
            assert lower['session-id'] if path.startswith('/codex') else 'session-id' not in lower, 'captured actor versus normal Key'
            assert lower['authorization'] == 'Bearer ' + (ACCESS if path.startswith('/codex') else KEY), 'captured credential kind'
            assert (lower.get('chatgpt-account-id') == ACCOUNT) == path.startswith('/codex'), 'subscription-only account routing'
        with sqlite3.connect('file:' + str(root / 'data/oc.sqlite') + '?mode=ro', uri=True) as db:
            assert db.execute('SELECT count(*) FROM tool_operations').fetchone()[0] == 4, 'four settled native reads, no replay'
            assert db.execute('SELECT count(*) FROM credential_accounts').fetchone()[0] == 3, 'durable shared native accounts'
        print('AUTH02/AUTH04 fixture-feature ELF: browser/device durable approval, native Key/OAuth WS+HTTP read/result/final/reopen PASS; no real issuer/provider')
        print('Local fixture counters:', json.dumps({'ws_frames': len(FRAMES), 'http_requests': len(HTTP),
              'token_exchanges': len(TOKENS), 'device_controls': len(CONTROL)}))
    finally:
        for name in ('device', 'browser', 'key'):
            child = locals().get(name)
            if child is not None and child.poll() is None:
                child.kill()
                child.wait(timeout=5)
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
