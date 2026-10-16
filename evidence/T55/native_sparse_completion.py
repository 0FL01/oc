#!/usr/bin/env python3
"""Offline existing-ELF baseline diagnosis, NOT a post-fix acceptance gate.

Synthetic fixtures only; sparse must fail today, full/omitted must succeed.
No build, credentials, user config/data, external API or paid generation.
"""
import http.server
import json
import os
from pathlib import Path
import select
import signal
import sqlite3
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
BINARY = ROOT / 'target/release/oc'


def event(value):
    return ('data: ' + json.dumps(value) + '\n\n').encode()


def terminal(output=None, omit=False):
    response = {'status': 'completed', 'usage': {'input_tokens': 100, 'output_tokens': 40}}
    if not omit:
        response['output'] = output or []
    return event({'type': 'response.completed', 'response': response})


def run(mode):
    requests, titles, handler_errors = [], [], []
    fixture = json.loads((ROOT / 'evidence/T55/sparse-completed.fixture.json').read_text())
    with tempfile.TemporaryDirectory(prefix='responses-native-probe-', dir=TMP) as directory:
        root = Path(directory)
        home, project = root / 'home', root / 'project'
        config = home / 'config/opencode'
        config.mkdir(parents=True)
        project.mkdir()

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers['Content-Length'])
                    assert 0 < length <= 1024 * 1024
                    request = json.loads(self.rfile.read(length))
                    assert self.path == '/v1/responses'
                    instruction = "Generate a short session title from the user's request. Output only the title, in at most 100 characters."
                    if any(item.get('role') == 'developer' and any(part.get('text') == instruction for part in item.get('content', []) if isinstance(part, dict)) for item in request['input']):
                        titles.append(request)
                        body = terminal([{'type': 'message', 'role': 'assistant', 'status': 'completed', 'content': [{'type': 'output_text', 'text': 'probe title'}]}])
                    else:
                        requests.append(request)
                        number = len(requests)
                        if number == 1:
                            events = fixture['events']
                            done = events[-2]['item']
                            body = b''.join(event(value) for value in events[:-1])
                            body += terminal([] if mode == 'sparse' else [done], omit=mode == 'absent')
                        elif number == 2:
                            assert any(item.get('type') == 'function_call_output' and item.get('call_id') == 'call_probe' for item in request['input'])
                            call = {'type': 'function_call', 'id': 'fc_read', 'call_id': 'call_read', 'name': 'read', 'arguments': json.dumps({'path': 'probe.txt'}), 'status': 'completed'}
                            body = event({'type': 'response.output_item.done', 'output_index': 0, 'item': call}) + terminal([call])
                        else:
                            assert number == 3
                            assert any(item.get('type') == 'function_call_output' and item.get('call_id') == 'call_read' for item in request['input'])
                            body = terminal([{'type': 'message', 'role': 'assistant', 'status': 'completed', 'content': [{'type': 'output_text', 'text': 'probe done'}]}])
                    self.send_response(200)
                    self.send_header('content-type', 'text/event-stream')
                    self.send_header('content-length', str(len(body)))
                    self.send_header('x-should-retry', 'false')
                    self.end_headers()
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except Exception:
                    # Fixed safe code only, never request/payload/headers in output.
                    handler_errors.append('fixture_request_rejected')
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        settings = {'model': 'fixture/fixture-model', 'compaction': {'auto': False},
                    'permission': {'read': 'allow', 'edit': 'allow', 'apply_patch': 'allow'},
                    'provider': {'fixture': {'npm': '@ai-sdk/openai',
                                'options': {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'synthetic-canary'},
                                'models': {'fixture-model': {'limit': {'context': 65536, 'output': 2048}}}}}}
        (config / 'opencode.json').write_text(json.dumps(settings))
        env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
               'XDG_DATA_HOME': str(home / 'data'), 'XDG_STATE_HOME': str(home / 'state'),
               'OC_TEST_ALLOW_LOOPBACK': '1', 'PATH': '/usr/bin:/bin'}
        process = None
        try:
            process = subprocess.Popen([str(BINARY), 'run', '--json', 'offline differential probe'], cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            collected, pending, retry_cancel = bytearray(), bytearray(), False
            deadline = time.monotonic() + 20
            streams = {process.stdout.fileno(), process.stderr.fileno()}
            while streams:
                assert time.monotonic() <= deadline, 'external_watchdog'
                ready, _, _ = select.select(list(streams), [], [], 0.05)
                for fd in ready:
                    chunk = os.read(fd, 16384)
                    if not chunk:
                        streams.remove(fd)
                        continue
                    collected.extend(chunk)
                    assert len(collected) < 1024 * 1024, 'output_cap'
                    if fd == process.stderr.fileno():
                        pending.extend(chunk)
                        while b'\n' in pending:
                            line, _, rest = pending.partition(b'\n')
                            pending = bytearray(rest)
                            try:
                                value = json.loads(line)
                            except ValueError:
                                continue
                            if value.get('type') == 'retry' and not retry_cancel:
                                retry_cancel = True
                                process.send_signal(signal.SIGINT)
            process.wait(timeout=3)
            database = next((home / 'data').rglob('oc.sqlite'))
            with sqlite3.connect(database) as db:
                operations = db.execute('SELECT count(*) FROM tool_operations').fetchone()[0]
            content = (project / 'probe.txt').read_bytes() if (project / 'probe.txt').exists() else None
            diagnostic = collected.decode()
            assert not handler_errors, 'fixture_request_rejected'
            assert 'synthetic-canary' not in diagnostic, 'auth_leaked'
            success = mode in {'full', 'absent'}
            assert process.returncode == (0 if success else 130), 'exit'
            assert operations == (2 if success else 0), 'tool_operations'
            assert content == (b'hello\n' if success else None), 'file_bytes'
            assert len(requests) == (3 if success else 1), 'main_posts'
            assert len(titles) == 1, 'title_posts'
            assert ('incomplete stream (HTTP 200)' in diagnostic) == (not success), 'diagnostic'
            return {'case': mode, 'exit': process.returncode, 'main_posts': len(requests),
                    'synthetic_title_posts': len(titles), 'tool_operations': operations,
                    'expected_file_bytes': content == b'hello\n',
                    'incomplete_http200': not success, 'cancelled_after_retry_notice': retry_cancel}
        finally:
            if process is not None and process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            server.shutdown()
            server.server_close()
            thread.join(timeout=3)


def main():
    assert os.getuid() != 0, 'root_refused'
    before = BINARY.stat()
    cases = [run(mode) for mode in ('sparse', 'full', 'absent')]
    after = BINARY.stat()
    assert (before.st_ino, before.st_mtime_ns, before.st_size) == (after.st_ino, after.st_mtime_ns, after.st_size), 'binary_changed'
    print(json.dumps({'binary': 'target/release/oc', 'binary_mtime_ns': before.st_mtime_ns,
                      'binary_size': before.st_size, 'binary_stable_during_checks': True,
                      'network': 'synthetic loopback only', 'cases': cases}, indent=2))


if __name__ == '__main__':
    main()
