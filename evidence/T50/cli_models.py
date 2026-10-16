#!/usr/bin/env python3
"""Offline normal-ELF TOOL18 qualification; no Cargo, live auth or user HOME."""
import argparse
import contextlib
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import signal
import select
import sqlite3
import subprocess
import tempfile
import threading
import time

BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')


class Peer(http.server.ThreadingHTTPServer):
    daemon_threads = False
    block_on_close = True


@contextlib.contextmanager
def peer(mode, rows):
    counts = {'catalog': 0, 'other_get': 0, 'generation': 0, 'auth_ok': 0}
    requested = threading.Event()
    stop = threading.Event()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            counts['generation'] += 1
            self.send_error(500)

        def do_GET(self):
            counts['catalog' if self.path == '/v1/models' else 'other_get'] += 1
            counts['auth_ok'] += self.headers.get('Authorization') == 'Bearer offline-catalog-key'
            requested.set()
            if mode in ('timeout', 'cancel'):
                stop.wait(40)
                return
            status = 401 if mode == 'auth' else 200
            body = json.dumps({'object': 'list', 'data': rows}).encode()
            if mode == 'body-cap':
                body = b'x' * (8 * 1024 * 1024 + 1)
            if mode == 'syntax':
                body = b'{bad syntax offline-secret-payload'
            self.send_response(status)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    server = Peer(('127.0.0.1', 0), Handler)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()
    try:
        yield f'http://127.0.0.1:{server.server_port}/v1', counts, requested
    finally:
        stop.set()
        server.shutdown()
        server.server_close()
        worker.join(5)
        assert not worker.is_alive()


def fingerprint(binary):
    return hashlib.sha256(binary.read_bytes()).hexdigest()


def snapshot(root):
    return {str(p.relative_to(root)): (p.stat().st_size, hashlib.sha256(p.read_bytes()).hexdigest())
            for p in root.rglob('*') if p.is_file() and not p.is_symlink()}


def qualify(binary):
    results = []
    initial_hash = fingerprint(binary)
    with tempfile.TemporaryDirectory(prefix='t50-cli-models-', dir=BENCH) as temp:
        root = Path(temp)
        project, home, global_root = root / 'project', root / 'home', root / 'global'
        for directory in (project, home, global_root, project / '.opencode'):
            directory.mkdir()
        data = root / 'blocked-data-root'
        data.write_bytes(b'not a directory: native store must never open here')
        (global_root / 'cli.json').write_bytes(b'malformed unavailable saved preferences; must not read')
        config = project / 'opencode.json'
        (project / 'must-not-read').write_text('unavailable/selection-file-sentinel')
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'XDG_CONFIG_HOME': str(root / 'xdg'),
               'XDG_DATA_HOME': str(root / 'data'), 'OPENCODE_CONFIG_DIR': str(global_root),
               'OC_TEST_ALLOW_LOOPBACK': '1', 'TMPDIR': str(root), 'NO_PROXY': '*'}

        def run(name, value, expected, code=0, counts=None, cancel_event=None, broken=False):
            config.write_text(json.dumps(value))
            before = snapshot(root)
            command = [str(binary), '--data-dir', str(data), 'models']
            started = time.monotonic()
            if broken:
                read_fd, write_fd = os.pipe()
                os.close(read_fd)
                try:
                    proc = subprocess.Popen(command, cwd=project, env=env, stdout=write_fd,
                                            stderr=subprocess.PIPE, start_new_session=True)
                finally:
                    os.close(write_fd)
                stdout, stderr = proc.communicate(timeout=40)
                stdout = b''
            elif cancel_event:
                proc = subprocess.Popen(command, cwd=project, env=env, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, start_new_session=True)
                try:
                    assert cancel_event.wait(5), 'catalog request did not start'
                    os.killpg(proc.pid, signal.SIGINT)
                    stdout, stderr = proc.communicate(timeout=5)
                finally:
                    if proc.poll() is None:
                        os.killpg(proc.pid, signal.SIGKILL)
                        proc.wait()
            else:
                proc = subprocess.run(command, cwd=project, env=env, capture_output=True, timeout=40)
                stdout, stderr = proc.stdout, proc.stderr
            assert proc.returncode == code, (name, proc.returncode, stderr.decode(errors='replace'))
            expected_bytes = ''.join(f'{ref}\n' for ref in sorted(expected)).encode()
            assert stdout == expected_bytes, (name, stdout[:400], expected_bytes[:400])
            assert snapshot(root) == before, (name, 'fixture mutated or store created')
            for forbidden in (b'offline-catalog-key', b'offline-secret-payload', b'http://', b'\x1b', b'\r'):
                assert forbidden not in stderr, (name, 'unsafe diagnostic')
            if code and not broken:
                assert stderr, (name, 'missing safe diagnostic')
            if counts is not None:
                assert counts['generation'] == counts['other_get'] == 0, (name, counts)
                assert counts['auth_ok'] == counts['catalog'], (name, counts)
            results.append({'case': name, 'exit': proc.returncode, 'stdout': stdout.decode(),
                            'stderr': stderr.decode(), 'elapsed_s': round(time.monotonic()-started, 3),
                            'effects': 0, 'counts': dict(counts) if counts is not None else {}})

        models = {f'same-family-{n:02}': {'family': 'same', 'name': 'Display, not ID'} for n in range(25)}
        models.update({'org/route/model': {}, 'shared': {}, 'É': {}, '!punctuation': {}})
        mcp = {'browser': {'type': 'local', 'enabled': True,
                          'command': ['/bin/sh', '-c', f'touch {root / "browser-marker"}'],
                          'environment': {'KEY': '{file:must-not-read}'}}}
        static = {'model': 'unavailable/retired', 'default_agent': 'unavailable',
                  'provider': {'z': {'npm': '@ai-sdk/openai', 'options': {'apiKey': '{file:must-not-read}',
                                                                            'baseURL': '{file:must-not-read}'}, 'models': models},
                               'a': {'models': {'shared': {}, 'org/route/model': {}}},
                               'foreign': {'npm': '@foreign/sdk', 'options': {'apiKey': '{file:must-not-read}'},
                                           'models': {'hidden': {}}}}, 'mcp': mcp,
                  'plugin': ['unsupported-loader.js']}
        refs = [f'z/{id}' for id in models] + ['a/shared', 'a/org/route/model']
        run('static-multiple-unavailable-no-store', static, refs)
        run('static-default-file-template-inert', dict(static, model='{file:must-not-read}'), refs)
        run('static-default-missing-env-template-inert', dict(static, model='{env:OC_MISSING_CATALOG_MODEL}'), refs)
        static.pop('model')
        static.pop('default_agent')
        run('static-no-default', static, refs)
        run('healthy-empty', {}, [])
        run('healthy-no-config', {}, [])
        config.unlink()
        before = snapshot(root)
        empty = subprocess.run([str(binary), 'models'], cwd=project, env=env, capture_output=True, timeout=5)
        assert empty.returncode == 0 and empty.stdout == b'' and snapshot(root) == before
        results.append({'case': 'absent-config', 'exit': 0, 'effects': 0})
        run('disabled-dynamic-no-secret-read', {'disabled_providers': ['ludka2'], 'provider': {
            'ludka2': {'options': {'apiKey': '{file:never-read}'}, 'models': {'hidden': {}}}}}, [])
        run('enabled-empty-filter', dict(static, enabled_providers=[]), [])
        (global_root / 'opencode.json').write_text(json.dumps({'provider': {'order': {'models': {'global': {}}}}}))
        (project / 'opencode.jsonc').write_text(json.dumps({'provider': {'order': {'models': {'jsonc': {}}}}}))
        (project / '.opencode' / 'opencode.json').write_text(json.dumps({'provider': {'order': {'models': {'local': {}}}}}))
        run('global-project-jsonc-local-precedence', {'provider': {'order': {'models': {'project': {}}}}}, ['order/local'])
        for path in (global_root / 'opencode.json', project / 'opencode.jsonc', project / '.opencode' / 'opencode.json'):
            path.unlink()
        run('fatal-policy', {'permission': {'read': 17}, 'provider': {'p': {'models': {'prefix': {}}}}}, [], 1)
        run('fatal-metadata-cap', {'provider': {'p': {'models': {'prefix': {}, 'oversized': {'name': 'x' * 12289}}}}}, [], 1)
        run('fatal-full-catalog-row-cap', {'provider': {'p': {'models': {f'm{n}': {} for n in range(100001)}}}}, [], 1)
        run('fatal-control-id', {'provider': {'p': {'models': {'prefix': {}, 'hostile\nline': {}}}}}, [], 1)
        run('fatal-dynamic-secret-trust', {'provider': {'ludka2': {'options': {'apiKey': '{file:../outside}'}}}}, [], 1)
        outside = root / 'outside'
        outside.mkdir()
        (project / '.opencode').rmdir()
        (project / '.opencode').symlink_to(outside, target_is_directory=True)
        run('fatal-source-root-trust', {}, [], 1)
        (project / '.opencode').unlink()
        (project / '.opencode').mkdir()
        (project / 'dcp.json').write_bytes(b'{malformed')
        run('fatal-standalone-config', {}, [], 1)
        (project / 'dcp.json').unlink()
        run('missing-dynamic-key-known-static', {'provider': {'ludka2': {'models': {'known': {}}}, 'p': {'models': {'independent': {}}}}},
            ['ludka2/known', 'p/independent'], 1)
        for mode, rows, expected, code, requests in (
            ('success', [{'id': 'org/route/model'}, {'id': 'shared'}], ['ludka2/org/route/model', 'ludka2/shared', 'p/independent'], 0, 1),
            ('auth', [], ['ludka2/retired', 'p/independent'], 1, 1),
            ('invalid', [{'id': 'candidate'}, {'id': 'bad', 'context_length': -1}], ['ludka2/retired', 'p/independent'], 1, 1),
            ('syntax', [], ['ludka2/retired', 'p/independent'], 1, 4),
            ('rows-cap', [{'id': f'm{n}'} for n in range(10001)], ['ludka2/retired', 'p/independent'], 1, 1),
            ('body-cap', [], ['ludka2/retired', 'p/independent'], 1, 1),
            ('control', [{'id': 'candidate'}, {'id': 'hostile\nline'}], [], 1, 1),
            ('timeout', [], ['ludka2/retired', 'p/independent'], 1, 2),
            ('cancel', [], [], 130, 1),
        ):
            with peer(mode, rows) as (base, counts, requested):
                value = {'provider': {'ludka2': {'npm': '@ai-sdk/openai', 'options': {'baseURL': base, 'apiKey': 'offline-catalog-key'},
                                               'models': {'retired': {'name': 'must retire after success'}}},
                                      'p': {'models': {'independent': {}}}}, 'mcp': mcp}
                run('dynamic-' + mode, value, expected, code, counts, requested if mode == 'cancel' else None)
                assert counts['catalog'] == requests, (mode, counts)
        run('broken-output-nonzero', static, [], 1, broken=True)
        long_models = {f'm{n:04}-' + 'x' * 200: {} for n in range(5000)}
        config.write_text(json.dumps({'provider': {'p': {'models': long_models}}}))
        before = snapshot(root)
        read_fd, write_fd = os.pipe()
        flags_before = fcntl.fcntl(write_fd, fcntl.F_GETFL)
        proc = subprocess.Popen([str(binary), '--data-dir', str(data), 'models'], cwd=project, env=env,
                                stdout=write_fd, stderr=subprocess.PIPE, start_new_session=True)
        try:
            deadline = time.monotonic() + 5
            # A clock delay can interrupt synchronous config admission before
            # stdout starts. Wait for an actually undrained/full pipe instead.
            while select.select([], [write_fd], [], 0)[1]:
                assert proc.poll() is None, 'fixture exited before output blocked'
                assert time.monotonic() < deadline, 'output pipe never filled'
                time.sleep(0.01)
            assert proc.poll() is None, 'fixture must block on an undrained pipe'
            os.killpg(proc.pid, signal.SIGINT)
            _, stderr = proc.communicate(timeout=5)
            assert proc.returncode == 130 and stderr == b'error: catalog cancelled\n', (proc.returncode, stderr)
            assert fcntl.fcntl(write_fd, fcntl.F_GETFL) == flags_before, 'shared output flags not restored'
            os.close(write_fd)
            write_fd = None
            prefix = os.read(read_fd, 65536)
            expected = ''.join(f'p/{id}\n' for id in sorted(long_models)).encode()
            assert prefix and expected.startswith(prefix)
            assert snapshot(root) == before
            results.append({'case': 'cancel-blocked-output', 'exit': 130, 'prefix_bytes': len(prefix),
                            'output_complete': False, 'effects': 0, 'flags_restored': True})
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            os.close(read_fd)
            if write_fd is not None:
                os.close(write_fd)
        with peer('success', [{'id': 'not-admitted'}]) as (base, counts, _):
            run('disabled-peer-zero-requests', {'disabled_providers': ['ludka2'], 'provider': {
                'ludka2': {'options': {'baseURL': base, 'apiKey': 'offline-catalog-key'}, 'models': {'hidden': {}}}}}, [], counts=counts)
            assert counts['catalog'] == 0

        # Opaque saved unavailable selection/history/recovery state must remain
        # unopened. Trace actual syscalls with the data-root lock already held;
        # do not depend on this intentionally isolated fixture's schema to parse.
        saved = root / 'existing-data'
        saved.mkdir()
        with sqlite3.connect(saved / 'oc.sqlite') as db:
            db.executescript('CREATE TABLE prefs(key TEXT,value TEXT); CREATE TABLE history(body TEXT); CREATE TABLE operations(state TEXT);')
            db.execute('INSERT INTO prefs VALUES (?,?)', ('tui.model', '{file:must-not-read}'))
            db.execute('INSERT INTO history VALUES (?)', ('immutable history sentinel',))
            db.execute('INSERT INTO operations VALUES (?)', ('unknown recovery sentinel',))
        config.write_text(json.dumps(dict(static, model='{file:must-not-read}', default_agent='unavailable')))
        with (saved / 'oc.lock').open('wb') as lock, tempfile.NamedTemporaryFile(prefix='t50-cli-syscalls-', dir=BENCH) as trace:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            before = snapshot(root)
            result = subprocess.run(['strace', '-f', '-qq', '-e', 'trace=openat,openat2,creat,execve', '-o', trace.name,
                                     str(binary), '--data-dir', str(saved), 'models'], cwd=project, env=env,
                                    capture_output=True, timeout=10)
            assert result.returncode == 0 and result.stdout == ''.join(f'{r}\n' for r in sorted(refs)).encode()
            assert snapshot(root) == before
            syscalls = Path(trace.name).read_text()
            assert len(syscalls.encode()) < 65536
            for forbidden in ('oc.sqlite', 'oc.lock', 'cli.json', 'must-not-read', 'browser-marker', 'unsupported-loader.js', 'execve("/bin/sh'):
                assert forbidden not in syscalls, forbidden
            results.append({'case': 'locked-existing-state-syscalls', 'exit': 0, 'store_opens': 0,
                            'prefs_opens': 0, 'loader_opens': 0, 'effects': 0, 'syscall_bytes': len(syscalls.encode())})
        for extra in ('list', '--refresh', '--verbose', '--json', 'ludka2'):
            result = subprocess.run([str(binary), 'models', extra], cwd=project, env=env, capture_output=True, timeout=5)
            assert result.returncode == 2 and result.stdout == b''
        assert not (root / 'browser-marker').exists()
    assert fingerprint(binary) == initial_hash, 'ELF changed during qualification'
    return {'binary': str(binary), 'sha256': initial_hash, 'cases': len(results), 'results': results,
            'owned_cleanup': 'TemporaryDirectory removed; HTTP workers joined; child process groups reaped'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(qualify(args.binary.resolve()), ensure_ascii=False, indent=2))
