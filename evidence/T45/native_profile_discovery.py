#!/usr/bin/env python3
"""R6: direct normal ELFs, actual CLI IDs, real loader/PTY/Db, offline wires."""
import argparse
import codecs
import contextlib
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
BENCH = Path('/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924')


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


native = load('native_background', 'evidence/T50/native_background.py')
ui = load('profile_terminal', 'evidence/T50/native_shell_controls.py')


def text(item):
    return ''.join(part.get('text', '') for part in item.get('content', []) if isinstance(part, dict))


def fingerprint(binary):
    with binary.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


class Profile(native.Native):
    consume_pty = ui.Consumer.consume_pty
    screen = ui.Consumer.screen

    def __init__(self, binary):
        self.binary = binary
        self.temp = tempfile.TemporaryDirectory(prefix='t45-profile-owned-', dir=BENCH)
        self.root = Path(self.temp.name)
        self.project, self.home, self.data = [self.root / x for x in ('project', 'home', 'data')]
        self.global_root = self.home / 'config/opencode'
        self.global_root.mkdir(parents=True)
        self.project.mkdir()
        self.requests, self.errors, self.cli_receipts, self.wires = [], [], [], []
        self.pty_receipts = []
        self.physical_requests, self.auxiliary_requests = 0, 0
        self.process, self.fd, self.reader = None, None, None
        self.tail, self.height = bytearray(), 40
        self.delegated = False
        owner = self

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                owner.physical_requests += 1
                try:
                    size = int(self.headers.get('Content-Length', '0'))
                    assert 0 < size <= 1048576
                    request = json.loads(self.rfile.read(size))
                    auxiliary = request.get('max_output_tokens') == 256 and not request.get('tools')
                    owner.wires.append({'path': self.path, 'auth_ok': self.headers.get('Authorization') == 'Bearer synthetic-profile', 'auxiliary': auxiliary, 'body': request})
                    if auxiliary:
                        owner.auxiliary_requests += 1
                        events = native.completed('Synthetic title')
                    else:
                        owner.requests.append(request)
                        inputs = '\n'.join(text(item) for item in request['input'] if item.get('role') == 'user')
                        if 'DELEGATE_PROFILE' in inputs and 'CHILD_PROFILE' not in inputs and not owner.delegated:
                            owner.delegated = True
                            events = native.tool('subagent', {'agent': 'team/child', 'description': 'Own model profile', 'prompt': 'CHILD_PROFILE'}, 'own-profile-child')
                        else:
                            events = native.completed('PROFILE_DONE')
                    body = ''.join('data: ' + json.dumps(event) + '\n\n' for event in events).encode()
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/event-stream')
                    self.send_header('Content-Length', str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except Exception as error:
                    owner.errors.append(repr(error))

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Peer)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.env = {'PATH': '/usr/bin:/bin', 'HOME': str(self.home), 'XDG_CONFIG_HOME': str(self.home / 'config'), 'XDG_DATA_HOME': str(self.root / 'xdg-data'), 'TMPDIR': str(self.root), 'OPENCODE_CONFIG_DIR': str(self.global_root), 'SHELL': '/bin/bash', 'TERM': 'xterm-256color', 'NO_PROXY': '*', 'OC_TEST_ALLOW_LOOPBACK': '1'}
        entry = {'limit': {'context': 500000, 'output': 4096}, 'variants': {'fast': {'reasoningEffort': 'high'}, 'slow': {'reasoningEffort': 'low'}}}
        providers = {provider: {'npm': '@ai-sdk/openai', 'options': {'baseURL': f'http://127.0.0.1:{self.server.server_port}/{provider}/v1', 'apiKey': 'synthetic-profile'}, 'models': {model: entry for model in ('base', 'org/nested/model')}} for provider in ('alpha', 'beta')}
        self.config = {'model': 'alpha/base', 'compaction': {'auto': False}, 'permission': {'subagent': 'allow', 'shell': 'deny', 'edit': 'deny'}, 'provider': providers}
        self.save()
        assert binary.read_bytes()[:4] == b'\x7fELF'

    def save(self):
        (self.global_root / 'opencode.json').write_text(json.dumps(self.config))

    def __exit__(self, kind, error, traceback):
        if error is not None:
            print(json.dumps({'binary': str(self.binary), 'status': 'FAIL', 'hypothesis': repr(error), 'physical_requests': self.physical_requests, 'auxiliary_requests': self.auxiliary_requests, 'cli': self.cli_receipts, 'wires': self.wires}), flush=True)
        super().__exit__(kind, error, traceback)

    def command(self, label, *args, expected=0):
        argv = [str(self.binary), '--data-dir', str(self.data), *args]
        process = subprocess.Popen(argv, cwd=self.project, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            stdout, stderr = process.communicate(timeout=40)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
        assert len(stdout) + len(stderr) <= 16 * 1024 * 1024
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            pass
        else:
            raise AssertionError('owned native process group still alive')
        receipt = {'case': label, 'argv': argv, 'exit': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode(), 'pid': process.pid, 'reaped': True}
        self.cli_receipts.append(receipt)
        assert process.returncode == expected, receipt
        assert not self.errors, self.errors
        return stdout.decode(), stderr.decode()

    def start(self):
        self.grid_lock = threading.Lock()
        self.grid = [[' ' for _ in range(110)] for _ in range(self.height)]
        self.cursor, self.pending = (0, 0), ''
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        super().start()

    def stop(self, *args, **kwargs):
        pid = self.process.pid if self.process is not None else None
        super().stop(*args, **kwargs)
        if pid is not None:
            try:
                os.killpg(pid, 0)
            except ProcessLookupError:
                pass
            else:
                raise AssertionError('owned PTY native group still alive')
            self.pty_receipts.append({'pid': pid, 'reaped': True, 'reader_joined': True, 'group_absent': True})

    def rows(self, query, params=()):
        with contextlib.closing(sqlite3.connect(self.data / 'oc.sqlite')) as db:
            return db.execute(query, params).fetchall()

    def put(self, root, relative, body):
        file = root / relative
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(body)

    def assert_wire(self, start, provider, model, variant, marker):
        views = [wire for wire in self.wires[start:] if not wire['auxiliary']]
        assert views
        for wire in views:
            body = wire['body']
            assert wire['path'] == f'/{provider}/v1/responses' and wire['auth_ok'], wire
            assert body['model'] == model and body.get('reasoning', {}).get('effort') == variant, body
            assert marker in '\n'.join(text(item) for item in body['input']), body
        return len(views)


def qualify(binary):
    before = fingerprint(binary)
    with Profile(binary) as fixture:
        fixture.command('normal-help', '--help')
        stdout, _ = fixture.command('actual-models-stdout', 'models')
        ids = stdout.splitlines()
        assert ids == ['alpha/base', 'alpha/org/nested/model', 'beta/base', 'beta/org/nested/model'], ids
        assert fixture.physical_requests == 0 and not fixture.data.exists()
        alpha = next(value for value in ids if value.startswith('alpha/') and value.count('/') == 3)
        beta = next(value for value in ids if value.startswith('beta/') and value.count('/') == 3)
        provider, model = alpha.split('/', 1)
        fixture.config['agents'] = {
            'team/canonical': {'model': {'providerID': provider, 'model': model, 'variant': 'fast'}, 'variant': 'slow', 'system': 'CANONICAL_PROFILE'},
            'team/child': {'mode': 'subagent', 'model': {'providerID': provider, 'model': model, 'variant': 'fast'}, 'variant': 'slow', 'system': 'OWN_CHILD_PROFILE'},
            'hidden-profile': {'hidden': True, 'system': 'EXPLICIT_HIDDEN_PROFILE'},
            'disabled-profile': {'disabled': True},
            'beta-profile': {'model': beta + '#fast', 'variant': 'slow', 'system': 'BETA_PROFILE'},
            'missing-profile': {'model': provider + '/unavailable', 'system': 'MISSING_PROFILE'},
        }
        fixture.config['agent'] = {'team/legacy': {'model': alpha + '#fast', 'variant': 'slow', 'system': 'LEGACY_PROFILE'}, 'team/separate': {'model': alpha, 'variant': 'slow', 'system': 'SEPARATE_PROFILE'}}
        fixture.put(fixture.global_root, 'agent/team/markdown.md', '---\nmodel: ' + alpha + '#fast\nvariant: slow\n---\nGLOBAL_OLD_PROFILE')
        fixture.put(fixture.global_root, 'agents/team/native-markdown.md', '---\nmodel:\n  providerID: ' + provider + '\n  model: ' + model + '\n  variant: fast\nvariant: slow\n---\nNATIVE_MARKDOWN_PROFILE')
        fixture.put(fixture.project / '.opencode', 'agent/team/project-markdown.md', '---\nmodel: ' + alpha + '\nvariant: fast\n---\nPROJECT_SINGULAR_PROFILE')
        fixture.put(fixture.project / '.opencode', 'agents/team/markdown.md', '---\ndescription: project partial\n---\nPROJECT_MARKDOWN_PROFILE')
        fixture.put(fixture.project / '.opencode', 'mode/compat.md', '---\nmode: subagent\n---\nCOMPAT_PROFILE')
        fixture.put(fixture.project / '.opencode', 'modes/nested/ignored.md', 'UNREACHABLE_COMPAT')
        fixture.save()
        counts = {}
        for profile, marker, effort in [('team/canonical', 'CANONICAL_PROFILE', 'high'), ('team/legacy', 'LEGACY_PROFILE', 'high'), ('team/separate', 'SEPARATE_PROFILE', 'low'), ('team/markdown', 'PROJECT_MARKDOWN_PROFILE', 'high'), ('team/native-markdown', 'NATIVE_MARKDOWN_PROFILE', 'high'), ('team/project-markdown', 'PROJECT_SINGULAR_PROFILE', 'high')]:
            start = len(fixture.wires)
            fixture.command(profile, 'run', '--json', '--session', profile.replace('/', '-'), '--agent', profile, 'PROFILE_TEST')
            counts[profile] = fixture.assert_wire(start, provider, model, effort, marker)
        saved = fixture.rows("SELECT key,value FROM prefs WHERE key LIKE 'tui.selection.session:%'")
        for profile in ('team/canonical', 'team/legacy', 'team/separate', 'team/markdown'):
            selection = next(json.loads(value) for key, value in saved if profile.replace('/', '-') in key)
            assert selection['agent'] == profile and selection['models'][profile]['id'] == model, selection
        start = len(fixture.wires)
        fixture.command('durable-restart', 'run', '--json', '--session', 'team-markdown', 'PROFILE_RESTART')
        counts['restart'] = fixture.assert_wire(start, provider, model, 'high', 'PROJECT_MARKDOWN_PROFILE')
        fixture.config['default_agent'] = 'team/canonical'
        fixture.config.pop('model')
        fixture.save()
        start = len(fixture.wires)
        fixture.command('canonical-default', 'run', '--json', '--session', 'canonical-default', 'PROFILE_DEFAULT')
        counts['default'] = fixture.assert_wire(start, provider, model, 'high', 'CANONICAL_PROFILE')
        fixture.config['model'] = 'alpha/base'
        fixture.config['default_agent'] = 'team/native-markdown'
        fixture.save()
        start = len(fixture.wires)
        fixture.command('native-markdown-default', 'run', '--json', '--session', 'native-markdown-default', 'PROFILE_DEFAULT')
        counts['markdown-default'] = fixture.assert_wire(start, provider, model, 'high', 'NATIVE_MARKDOWN_PROFILE')
        fixture.config['default_agent'] = 'beta-profile'
        fixture.save()
        start = len(fixture.wires)
        fixture.command('colliding-provider-default', 'run', '--json', '--session', 'beta-default', 'PROFILE_BETA')
        counts['beta'] = fixture.assert_wire(start, 'beta', model, 'high', 'BETA_PROFILE')
        fixture.config.pop('default_agent')
        fixture.save()
        start = len(fixture.wires)
        fixture.command('own-model-child', 'run', '--json', '--session', 'delegate', 'DELEGATE_PROFILE')
        child = [wire for wire in fixture.wires[start:] if not wire['auxiliary'] and 'CHILD_PROFILE' in '\n'.join(text(item) for item in wire['body']['input'] if item.get('role') == 'user')]
        assert len(child) == 1 and child[0]['body']['model'] == model
        assert child[0]['body']['reasoning']['effort'] == 'high' and 'OWN_CHILD_PROFILE' in json.dumps(child[0]['body']['input'])
        assert fixture.rows("SELECT agent,model FROM sessions WHERE parent_id='delegate'") == [('team/child', alpha + '#fast')]
        assert fixture.rows("SELECT state FROM tool_operations WHERE name='subagent'") == [('completed',)]
        start = len(fixture.wires)
        fixture.command('explicit-hidden', 'run', '--json', '--session', 'hidden', '--agent', 'hidden-profile', 'PROFILE_HIDDEN')
        fixture.assert_wire(start, provider, 'base', None, 'EXPLICIT_HIDDEN_PROFILE')
        refused = []
        for profile in ('missing-profile', 'disabled-profile', 'team/child', 'nested/ignored'):
            posts = fixture.physical_requests
            fixture.command('refuse-' + profile, 'run', '--json', '--session', 'refuse-' + profile.replace('/', '-'), '--agent', profile, 'FORBIDDEN_EFFECT', expected=1)
            assert fixture.physical_requests == posts
            refused.append(profile)
        retired = fixture.config['provider'][provider]['models'].pop(model)
        fixture.save()
        posts = fixture.physical_requests
        fixture.command('retired-saved-no-fallback', 'run', '--json', '--session', 'team-markdown', 'FORBIDDEN_EFFECT', expected=1)
        assert fixture.physical_requests == posts
        fixture.config['provider'][provider]['models'][model] = retired
        retired['variants']['fast']['disabled'] = True
        fixture.save()
        fixture.command('disabled-saved-variant-no-fallback', 'run', '--json', '--session', 'team-markdown', 'FORBIDDEN_EFFECT', expected=1)
        assert fixture.physical_requests == posts
        assert not fixture.rows("SELECT text FROM messages WHERE text='FORBIDDEN_EFFECT'")
        retired['variants']['fast'].pop('disabled')
        fixture.config['agents']['beta-profile']['hidden'] = True
        fixture.config['agents']['missing-profile']['hidden'] = True
        fixture.put(fixture.project / '.opencode', 'agents/team/malformed.md', '---\nmode: banana\n---\nMALFORMED_PROFILE')
        fixture.config['default_agent'] = 'team/malformed'
        fixture.save()
        fixture.command('malformed-nested-default-no-sibling-fallback', 'run', '--json', '--session', 'malformed-nested', 'FORBIDDEN_EFFECT', expected=1)
        assert fixture.physical_requests == posts
        fixture.config.pop('default_agent')
        fixture.save()
        fixture.start()
        fixture.send(b'/agents\r')
        native.until(lambda: 'Agents' in '\n'.join(fixture.screen()), 'real primary picker missing')
        painted = '\n'.join(fixture.screen())
        assert 'hidden-profile' not in painted and 'disabled-profile' not in painted and 'team/child' not in painted, painted
        fixture.send(b'team/markdown')
        time.sleep(.1)
        fixture.send(b'\r')
        native.until(lambda: fixture.rows("SELECT json_extract(value,'$.agent') FROM prefs WHERE key LIKE 'tui.selection.session:%' AND key LIKE '%t50-background%'") == [('team/markdown',)], 'real nested profile picker commit missing')
        start = len(fixture.wires)
        fixture.send(b'PTY_PROFILE\r')
        native.until(lambda: fixture.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='completed'") == [(1,)], 'real PTY selected turn incomplete')
        fixture.assert_wire(start, provider, model, 'high', 'PROJECT_MARKDOWN_PROFILE')
        native.until(lambda: 'PROFILE_DONE' in '\n'.join(fixture.screen()), 'real completed frame missing')
        time.sleep(.2)
        picker_ids = ['build', 'plan', 'team/legacy', 'team/separate', 'team/canonical', 'team/markdown', 'team/native-markdown', 'team/project-markdown', 'compat']
        assert all(value in painted for value in picker_ids), painted
        fixture.stop()
        posts = fixture.physical_requests
        fixture.start()
        assert fixture.rows("SELECT json_extract(value,'$.agent') FROM prefs WHERE key LIKE 'tui.selection.session:%' AND key LIKE '%t50-background%'") == [('team/markdown',)]
        fixture.stop()
        assert fixture.physical_requests == posts, 'idle reopen emitted provider request'
        fixture.put(fixture.project / '.opencode', 'agents/team/capacity.md', '---\nmodel: ' + alpha + '#fast\n---\nVALID_CAPACITY_PROFILE')
        fixture.config['default_agent'] = 'team/capacity'
        fixture.save()
        agents = fixture.project / '.opencode/agents'
        for index in range(4095):
            (agents / f'foreign-{index:04}.txt').write_text('')
        for label in ('capacity-exact-nested-default', 'capacity-overflow-nested-default', 'capacity-overflow-known-default'):
            if label == 'capacity-overflow-nested-default':
                (agents / 'foreign-extra.txt').write_text('')
            if label == 'capacity-overflow-known-default':
                fixture.config['default_agent'] = 'team/canonical'
                fixture.save()
            _, stderr = fixture.command(label, 'run', '--json', '--session', label, 'FORBIDDEN_EFFECT', expected=1)
            assert 'capacity_exceeded' in stderr, stderr
            assert fixture.physical_requests == posts
            refused.append(label)
        assert not fixture.rows("SELECT text FROM messages WHERE text='FORBIDDEN_EFFECT'")
        after = fingerprint(binary)
        assert before == after
        print(json.dumps({'binary': str(binary), 'sha256_before': before, 'sha256_after': after, 'status': 'PASS', 'models_stdout': stdout, 'profiles': counts, 'pre_effect_refusals': refused + ['retired-saved', 'disabled-saved-variant', 'malformed-nested-default'], 'own_model_children': len(child), 'physical_requests': fixture.physical_requests, 'auxiliary_requests': fixture.auxiliary_requests, 'generation_requests': len(fixture.requests), 'picker': painted, 'picker_ids': picker_ids, 'direct_cycle_input': 'T44 PAUSED; ordered owner catalog eligibility qualified, no invented Tab binding', 'idle_reopen_requests': 0, 'cli': fixture.cli_receipts, 'pty': fixture.pty_receipts, 'wires': fixture.wires}), flush=True)
    assert not fixture.root.exists() and not fixture.thread.is_alive()
    print(json.dumps({'cleanup': 'joined/reaped native CLI/PTY and HTTP before exact TempDir removal', 'root_removed': True, 'binary': str(binary)}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', action='append', required=True, type=Path)
    for binary in parser.parse_args().binary:
        qualify(binary.resolve())
