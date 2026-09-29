#!/usr/bin/env python3
"""T51/R4B opt-in, metadata-only real-user qualification (no generation).

Run from the repository: python3 scripts/real_user_startup.py --ack-real-user
The inherited environment is passed unchanged. Protected config is read only by
the product, never by this harness. Only fixed public UI markers/enum facts and
native SQLite aggregate counts/digests leave the private run directory.
"""
import argparse
import ast
import ctypes
import errno
import fcntl
import hashlib
import json
import os
import pty
import re
import select
import shutil
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import time
import unicodedata
from pathlib import Path

REPO = Path('/home/opencode/ai/oc')
CACHE = Path('/home/opencode/.cache/opencode-tmp/opencode')
EXPECTED_HEAD = 'bb527947a41dc209c3c62bbe8d53277592ae0f2a'
EXPECTED_BINARY = '9c56cb0ce2f67de8c01411032547b0c630b29aeb7af1e19ab3111ee7209e7af4'
LOG_CAP = 16 * 1024 * 1024
TOTAL_CAP = 1024 * 1024 * 1024
HEADROOM = 100 * 1024 * 1024
PROFILE_SECONDS = 180
GRACE_SECONDS = 15
TRACE_CALLS = ('openat,openat2,newfstatat,statx,flock,fcntl,connect,clone,clone3,'
               'fork,vfork,execve,execveat,wait4,waitid,exit,exit_group,close,chdir')
TABLES = ('sessions', 'messages', 'turns', 'turn_acceptances', 'tool_operations',
          'events', 'patch_effects', 'prefs')
PUBLIC_CODES = ('invalid_config', 'invalid_document', 'invalid_definition',
                'missing_configuration', 'trust_refused', 'source_unavailable',
                'capacity_exceeded', 'data_root_busy', 'unsafe_data_root',
                'storage_unavailable', 'recovery_failed', 'runtime_failed',
                'query_failed', 'invalid_stored_state', 'agent_unavailable',
                'variant_unavailable', 'ignored_setting', 'unsupported_plugin',
                'missing_credential', 'provider_pending', 'model_unavailable',
                'connection_failed', 'spawn_failed', 'deadline', 'transport',
                'unauthorized', 'forbidden', 'cleanup_failed')
PUBLIC_MARKERS = ('Commands', 'Settings', 'Select model', 'Select variant',
                  'Select agent', 'Sessions', 'MCP servers', 'No results found',
                  'No variants available', 'No sessions available', 'Compiled plugins',
                  'Configuration diagnostics', 'unsupported_plugin', 'ignored_setting')


def digest(value):
    return hashlib.sha256(value).hexdigest()


def opaque_path(path):
    return 'path-' + digest(os.fsencode(path))[:16]


def metadata(path):
    try:
        stat = path.stat()
        return {'exists': True, 'owned_current_uid': stat.st_uid == os.getuid(),
                'mode': oct(stat.st_mode & 0o777), 'bytes': stat.st_size}
    except FileNotFoundError:
        return {'exists': False}
    except OSError:
        return {'accessible': False}


def snapshot(path):
    """Only the observed native DB, SELECT aggregates and opaque pref bytes."""
    if not path.is_file():
        return {'status': 'ABSENT'}
    try:
        with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=1) as db:
            db.execute('BEGIN')
            counts = {table: db.execute(f'SELECT COUNT(*) FROM {table}').fetchone()[0]
                      for table in TABLES}
            hasher = hashlib.sha256()
            for row in db.execute('SELECT key,value,updated_at FROM prefs ORDER BY key'):
                for value in row:
                    encoded = value.encode('utf-8')
                    hasher.update(len(encoded).to_bytes(8, 'big'))
                    hasher.update(encoded)
            return {'status': 'OK', 'counts': counts, 'prefs_digest': hasher.hexdigest()}
    except sqlite3.Error:
        return {'status': 'UNAVAILABLE'}


def process_fact(pid):
    """Owned descendants only; never cmdline, environ, status text or payload."""
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        return int(fields[19]), fields[0]
    except (OSError, IndexError, ValueError):
        return None


def parser():
    # Reuse exactly the nearest trusted repository PTY grid function without
    # importing/executing that file's hermetic test campaign or config writes.
    source = REPO / 'crates/oc/tests/support/startup.py'
    tree = ast.parse(source.read_text())
    function = next(node for node in tree.body
                    if isinstance(node, ast.FunctionDef) and node.name == 'visible_rows')
    scope = {'re': re, 'unicodedata': unicodedata}
    exec(compile(ast.Module(body=[function], type_ignores=[]), str(source), 'exec'), scope)
    return scope['visible_rows']


class Profile:
    def __init__(self, label, run, visible_rows, global_root, data_root, fresh, prelaunch=None):
        self.label, self.run, self.visible_rows = label, run, visible_rows
        self.global_root, self.data_root, self.fresh = global_root, data_root, fresh
        self.prelaunch = prelaunch
        self.start = time.monotonic()
        self.deadline = self.start + PROFILE_SECONDS
        self.output = bytearray()
        self.trace = bytearray()
        self.trace_bytes = 0
        self.truncated = False
        self.closing = False
        self.owned = {}
        self.exe_labels = set()
        self.db_seen = False
        self.baseline = None
        self.ui = {}
        self.actions = []
        self.marker_steps = []
        self.raw_file = run / f'{label}.trace'
        self.raw = self.raw_file.open('xb')
        os.chmod(self.raw_file, 0o600)
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
        self.original = termios.tcgetattr(self.slave)
        self.trace_read, trace_write = os.pipe()
        command = ['strace', '-f', '-qq', '-yy', '-s', '256', '-e', 'trace=' + TRACE_CALLS,
                   '-e', 'raw=execve,execveat', '-o', f'/proc/self/fd/{trace_write}',
                   'target/release/oc']
        if fresh:
            command += ['--data-dir', str(data_root)]
        # No env argument: exact inherited product environment, no overrides.
        self.child = subprocess.Popen(command, cwd=REPO, stdin=self.slave,
                                      stdout=self.slave, stderr=self.slave,
                                      pass_fds=(trace_write,), start_new_session=True)
        os.close(trace_write)
        for fd in (self.master, self.trace_read):
            os.set_blocking(fd, False)
        fact = process_fact(self.child.pid)
        if fact:
            self.owned[self.child.pid] = fact[0]

    def track(self):
        for pid, ticks in list(self.owned.items()):
            fact = process_fact(pid)
            if fact is None or fact[0] != ticks:
                continue
            try:
                executable = os.readlink(f'/proc/{pid}/exe')
                name = Path(executable).name
                self.exe_labels.add(name if name in ('oc', 'strace', 'node', 'bun', 'npx',
                                                    'chromium', 'chrome', 'python3')
                                    else 'other-' + digest(os.fsencode(executable))[:16])
                tasks = Path(f'/proc/{pid}/task')
                for task in tasks.iterdir():
                    for raw in (task / 'children').read_text().split():
                        descendant = int(raw)
                        child_fact = process_fact(descendant)
                        if child_fact:
                            self.owned.setdefault(descendant, child_fact[0])
            except (OSError, ValueError):
                continue

    def pump(self, seconds=.15):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            if not self.closing and time.monotonic() >= self.deadline - GRACE_SECONDS:
                raise TimeoutError('profile_watchdog')
            self.track()
            ready, _, _ = select.select([self.master, self.trace_read], [], [], .04)
            for fd in ready:
                try:
                    chunk = os.read(fd, 65536)
                except OSError as error:
                    if error.errno in (errno.EIO, errno.EAGAIN):
                        continue
                    raise
                if not chunk:
                    continue
                if fd == self.master:
                    if len(self.output) + len(chunk) <= LOG_CAP:
                        self.output.extend(chunk)
                    else:
                        self.truncated = True
                else:
                    room = max(0, LOG_CAP - self.trace_bytes)
                    self.raw.write(chunk[:room])
                    self.raw.flush()
                    self.trace.extend(chunk[:room])
                    self.trace_bytes += min(room, len(chunk))
                    if len(chunk) > room:
                        self.truncated = True
            if self.truncated and not self.closing:
                raise RuntimeError('output_cap')

    def rows(self):
        return self.visible_rows(self.output.decode('utf-8', 'replace'))

    def has(self, marker):
        return any(marker in row for row in self.rows())

    def wait_marker(self, marker, seconds=5):
        end = min(self.deadline - GRACE_SECONDS, time.monotonic() + seconds)
        while time.monotonic() < end:
            self.pump()
            if self.has(marker):
                return True
            if self.child.poll() is not None:
                break
        return False

    def send(self, value, action):
        if not self.closing and time.monotonic() >= self.deadline - GRACE_SECONDS:
            raise TimeoutError('profile_watchdog')
        os.write(self.master, value)
        self.actions.append(action)
        self.pump(.3)
        self.marker_steps.append({'action': action,
                                  'markers': [marker for marker in PUBLIC_MARKERS if self.has(marker)]})

    def close_panel(self):
        self.send(b'\x1b', 'close_view_Esc')
        if any(self.has(marker) for marker in PUBLIC_MARKERS[:7]):
            self.send(b'\x1b', 'close_remaining_view_Esc')

    def palette(self, title, panel):
        self.send(b'\x10', 'open_command_palette')
        if not self.wait_marker('Commands'):
            return False
        self.send(title.encode(), 'filter_public_command_' + title.replace(' ', '_'))
        if not self.wait_marker(title):
            if title == 'Switch model variant':
                self.ui['variant_command_absent'] = self.has('No results found')
            self.close_panel()
            return False
        # Enter invokes a fixed public VIEW command only, never a persisted choice.
        self.send(b'\r', 'invoke_public_command_' + title.replace(' ', '_'))
        if title == 'Exit the app':
            end = min(self.deadline, time.monotonic() + 5)
            while self.child.poll() is None and time.monotonic() < end:
                self.pump()
            return self.child.poll() == 0
        return self.wait_marker(panel)

    def observe_baseline(self):
        # Observe only after the trace shows this product accessing its native
        # root. A busy owner may prevent the SQLite open; lock-path evidence is
        # sufficient to identify that same native root for read-only aggregates.
        trace = self.trace.decode('utf-8', 'replace')
        if str(self.data_root / 'oc.lock') in trace:
            self.baseline = snapshot(self.data_root / 'oc.sqlite')
        self.db_seen = str(self.data_root / 'oc.sqlite') in trace

    def exercise(self):
        end = self.deadline - 55
        while time.monotonic() < end:
            self.pump()
            if self.has('Native startup error') or self.has('█▀▀█'):
                break
            if self.child.poll() is not None:
                break
        self.observe_baseline()
        self.ui['home'] = self.has('█▀▀█') and not self.has('Native startup error')
        self.ui['fatal_frame'] = self.has('Native startup error')
        if self.ui['fatal_frame']:
            self.send(b'\x1b', 'close_fatal_frame_Esc')
            return
        if not self.ui['home']:
            return
        # Give the ordinary initial services 35s without changing product
        # deadlines. Longer configured services may still truthfully be Pending.
        self.pump(35)
        for label, keys, marker in (
                ('history_list', b'\x18l', 'Sessions'),
                ('agent_selector', b'\x1b[Z', 'Select agent'),
                ('model_selector', b'\x18m', 'Select model')):
            self.send(keys, 'open_view_' + label)
            self.ui[label] = self.wait_marker(marker)
            self.close_panel()
        self.ui['diagnostics'] = self.palette('Open settings', 'Settings')
        if self.ui['diagnostics']:
            # Filtering changes only the dialog query, not configuration.
            self.send(b'failed', 'filter_diagnostics_failed_entries')
            # The failed filter excludes the only mutating Settings item,
            # Permissions. All remaining Enter handlers display details only.
            self.send(b'\r', 'view_failed_diagnostic_Enter')
            self.ui['plugin_diagnostic'] = self.has('unsupported_plugin')
            self.send(b'\x7f' * 6, 'clear_dialog_filter_Backspace')
            self.send(b'ignored', 'filter_diagnostics_ignored_settings')
            self.ui['ignored_setting_diagnostic'] = self.has('ignored_setting')
        self.close_panel()
        self.ui['variant_selector'] = self.palette('Switch model variant', 'Select variant')
        self.close_panel()
        self.ui['mcp_inventory'] = self.palette('MCP servers', 'MCP servers')
        if self.ui['mcp_inventory']:
            statuses = {'Connected ✓': 'connected', 'Disabled ○': 'disabled',
                        'Failed !': 'failed', 'Connecting …': 'pending',
                        'Sign in required (unsupported)': 'needs_auth'}
            observed = {}
            # Bounded read-only browse, no Enter/Space/toggle/retry or config change.
            for _ in range(8):
                for row in self.rows():
                    for marker, status in statuses.items():
                        if row.rstrip().endswith(marker):
                            observed[digest(row.strip().encode())] = status
                self.send(b'\x1b[B', 'browse_MCP_inventory_Down')
            self.ui['mcp_visible_status_counts'] = {
                status: list(observed.values()).count(status) for status in statuses.values()}
        self.close_panel()
        self.ui['quit_command'] = self.palette('Exit the app', '')

    def shutdown(self):
        self.closing = True
        forced = False
        end = min(self.deadline, time.monotonic() + GRACE_SECONDS)
        while self.child.poll() is None and time.monotonic() < end:
            self.pump()
        if self.child.poll() is None:
            self.send(b'\x03', 'owned_graceful_Ctrl_C')
            end = time.monotonic() + 5
            while self.child.poll() is None and time.monotonic() < end:
                self.pump()
        for sig in (signal.SIGTERM, signal.SIGKILL):
            if self.child.poll() is not None:
                break
            forced = True
            self.track()
            # Exact PID/startticks only, not process-group or foreign owners.
            for pid, ticks in sorted(self.owned.items(), reverse=True):
                fact = process_fact(pid)
                if fact and fact[0] == ticks and pid != self.child.pid:
                    os.kill(pid, sig)
            self.pump(1)
            if self.child.poll() is None and sig == signal.SIGKILL:
                self.child.kill()
        code = self.child.wait(timeout=5)
        self.pump(.2)
        alive = []
        reaped = 0
        for pid, ticks in self.owned.items():
            fact = process_fact(pid)
            if fact and fact[0] == ticks:
                try:
                    waited, _ = os.waitpid(pid, os.WNOHANG)
                    reaped += bool(waited)
                except ChildProcessError:
                    pass
                if process_fact(pid) is not None:
                    alive.append(pid)
        return {'exit': code, 'forced_signal': forced, 'owned_remaining': len(alive),
                'owned_processes_observed': len(self.owned), 'orphan_reaped': reaped,
                'terminal_restored': termios.tcgetattr(self.slave) == self.original,
                'alt_leave': b'\x1b[?1049l' in self.output}

    def trace_summary(self):
        text = self.trace.decode('utf-8', 'replace')
        roots = [('global', self.global_root), ('Location', REPO),
                 ('.opencode', REPO / '.opencode'), ('nativeStore', self.data_root)]
        summaries = {}
        lines = text.splitlines()
        for label, root in roots:
            candidates = [line for line in lines if str(root) in line]
            # Location excludes the separately classified .opencode subtree.
            if label == 'Location':
                candidates = [line for line in candidates if str(REPO / '.opencode') not in line]
            config_lines = [line for line in candidates
                            if re.search(r'opencode\.jsonc?(?:["/>]|$)', line)]
            summaries[label] = {
                'opaque_root': opaque_path(root), 'metadata_lines': len(candidates),
                'successful_opens': sum(bool(re.search(r'openat2?\(.* = [0-9]', line))
                                        for line in candidates),
                'config_successful_opens': sum(bool(re.search(r'openat2?\(.* = [0-9]', line))
                                               for line in config_lines)}
        lock_lines = '\n'.join(line for line in lines if str(self.data_root / 'oc.lock') in line)
        return {'roots': summaries, 'native_sqlite_observed': self.db_seen,
                'lock_would_block': bool(re.search(r'flock\(.*= -1 E(?:WOULDBLOCK|AGAIN)', lock_lines)),
                'lock_acquired': bool(re.search(r'flock\(.*LOCK_EX.* = 0', lock_lines)),
                'connect_syscalls': sum('connect(' in line for line in lines),
                'exec_successes': sum(bool(re.search(r'execve(?:at)?\(.* = 0', line)) for line in lines),
                'exec_argv_env_raw': all('0x' in line for line in lines if 'execve(' in line),
                'observed_executable_classes': sorted(self.exe_labels),
                'npx_path_metadata': sum('/npx' in line for line in lines),
                'browser_path_metadata': sum(bool(re.search(r'/(?:chrome|chromium)(?:["/>-]|$)', line))
                                             for line in lines),
                'trace_bytes': self.trace_bytes, 'truncated': self.truncated}

    def finish(self):
        error = None
        try:
            self.exercise()
        except Exception as problem:
            # No exception text: it may contain private path/terminal data.
            error = type(problem).__name__
        cleanup = self.shutdown()
        after = snapshot(self.data_root / 'oc.sqlite') if self.baseline is not None else {'status': 'NOT_OBSERVED'}
        trace = self.trace_summary()
        text = self.output.decode('utf-8', 'replace')
        # Exact common owner Display shape, not keyword inference from free text.
        codes = sorted({code for code in PUBLIC_CODES
                        if re.search(r': ' + code + r' \(retryable=(?:true|false)\)', text)})
        status = 'FAIL'
        if trace['lock_would_block'] and self.ui.get('fatal_frame') and cleanup['exit'] == 1:
            status = 'BLOCKED_LOCK'
        elif (all(self.ui.get(key) for key in ('home', 'history_list', 'agent_selector',
                                              'model_selector', 'diagnostics', 'variant_selector'))
              and cleanup['exit'] == 0 and cleanup['terminal_restored']
              and cleanup['owned_remaining'] == 0 and not cleanup['forced_signal']
              and not trace['truncated'] and error is None
              and self.baseline is not None and self.baseline == after
              and (self.prelaunch is None or self.prelaunch == after)
              and self.baseline['status'] == 'OK'):
            status = 'PASS'
        result = {'profile': self.label, 'status': status, 'harness_error_class': error,
                  'elapsed_seconds': round(time.monotonic() - self.start, 2),
                  'ui': self.ui, 'actions': self.actions, 'public_marker_steps': self.marker_steps,
                  'typed_public_codes': codes,
                  'trace': trace, 'cleanup': cleanup, 'native_before_views': self.baseline,
                  'native_before_launch': self.prelaunch,
                  'native_after_exit': after, 'pty_bytes_memory_only': len(self.output),
                  'data_root_metadata': metadata(self.data_root),
                  'native_lock_metadata': metadata(self.data_root / 'oc.lock'),
                  'native_database_metadata': metadata(self.data_root / 'oc.sqlite')}
        self.raw.close()
        for fd in (self.master, self.slave, self.trace_read):
            os.close(fd)
        self.output.clear()
        self.trace.clear()
        return result


def audit_private_run(name):
    """No product launch: refine metadata from a completed owned trace only."""
    assert re.fullmatch(r'T51-real-user-[a-z0-9_]+', name)
    run = CACHE / name
    assert run.is_dir() and run.stat().st_uid == os.getuid()
    home = Path(os.environ['HOME'])
    global_root = Path(os.environ.get('OPENCODE_CONFIG_DIR') or
                       str(Path(os.environ.get('XDG_CONFIG_HOME') or str(home / '.config')) / 'opencode'))
    result = {}
    for label in ('existing', 'fresh'):
        path = run / f'{label}.trace'
        assert path.stat().st_size <= LOG_CAP and path.stat().st_uid == os.getuid()
        config_sources = set()
        config_order = []
        js_source_opens = 0
        thread_clones = 0
        nonthread_clones = 0
        endpoint_counts = {}
        calls = {}
        native_lock = {'acquired': 0, 'released': 0, 'would_block': 0}
        with path.open() as stream:
            for line in stream:
                syscall = re.search(r'\b([a-z0-9_]+)\(', line)
                if syscall:
                    call = syscall.group(1)
                    calls[call] = calls.get(call, 0) + 1
                opened = re.search(r'openat2?\(.* = [0-9]+<([^>]+)>', line)
                if opened and Path(opened.group(1)).name in ('opencode.json', 'opencode.jsonc'):
                    source_id = opaque_path(opened.group(1))
                    if source_id not in config_sources:
                        source_path = Path(opened.group(1))
                        root_label = ('global' if source_path.is_relative_to(global_root)
                                      else '.opencode' if source_path.is_relative_to(REPO / '.opencode')
                                      else 'Location' if source_path.is_relative_to(REPO) else 'other')
                        config_order.append({'root': root_label, 'opaque_source': source_id})
                    config_sources.add(source_id)
                if opened and Path(opened.group(1)).suffix in ('.js', '.mjs', '.cjs', '.ts', '.tsx'):
                    js_source_opens += 1
                if re.search(r'\bclone3?\(.* = [0-9]+', line):
                    if 'CLONE_THREAD' in line:
                        thread_clones += 1
                    else:
                        nonthread_clones += 1
                if 'flock(' in line and '/oc.lock>' in line:
                    native_lock['acquired'] += bool(re.search(r'LOCK_EX.* = 0', line))
                    native_lock['released'] += bool(re.search(r'LOCK_UN.* = 0', line))
                    native_lock['would_block'] += bool(re.search(r'= -1 E(?:AGAIN|WOULDBLOCK)', line))
                if 'connect(' in line:
                    family = re.search(r'sa_family=(AF_[A-Z0-9]+)', line)
                    address = re.search(r'connect\([^,]+, (.*), [0-9]+\)\s+=', line)
                    if family and address:
                        key = family.group(1) + ':' + opaque_path(address.group(1))
                        endpoint_counts[key] = endpoint_counts.get(key, 0) + 1
        result[label] = {'config_file_open_opaque_ids': sorted(config_sources),
                         'first_config_open_order': config_order,
                         'successful_js_ts_source_opens': js_source_opens,
                         'successful_thread_clones': thread_clones,
                         'successful_nonthread_clones_complete_lines': nonthread_clones,
                         'connect_family_and_opaque_endpoint_counts': endpoint_counts,
                         'syscall_counts': calls, 'native_lock': native_lock,
                         'raw_bytes': path.stat().st_size,
                         'raw_mode_private': path.stat().st_mode & 0o777 == 0o600}
    result['storage'] = {
        'run_directory_mode_private': run.stat().st_mode & 0o777 == 0o700,
        'own_aggregate_bytes': sum(file.stat().st_size for owned in CACHE.glob('T51-real-user-*')
                                   for file in owned.rglob('*') if file.is_file()),
        'aggregate_cap_bytes': TOTAL_CAP}
    print(json.dumps(result, indent=2))


def main():
    args = argparse.ArgumentParser(description=__doc__)
    mode = args.add_mutually_exclusive_group(required=True)
    mode.add_argument('--ack-real-user', action='store_true')
    mode.add_argument('--audit-private-run')
    args.add_argument('--baseline-from-run',
                      help='Prior owned completed trace establishing the native root for SELECT-only preflight')
    options = args.parse_args()
    if options.audit_private_run:
        audit_private_run(options.audit_private_run)
        return
    assert os.getuid() != 0 and Path.cwd() == REPO and CACHE.is_dir()
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO).decode().strip()
    binary_hash = digest((REPO / 'target/release/oc').read_bytes())
    assert head == EXPECTED_HEAD and binary_hash == EXPECTED_BINARY
    assert shutil.which('strace')
    # Scope aggregate accounting to this harness's own runs; never inspect other
    # cache content, credentials, inherited config or the runner's state.
    prior_bytes = sum(file.stat().st_size for run in CACHE.glob('T51-real-user-*')
                      for file in run.rglob('*') if file.is_file())
    assert prior_bytes + HEADROOM <= TOTAL_CAP
    os.umask(0o077)
    run = Path(tempfile.mkdtemp(prefix='T51-real-user-', dir=CACHE))
    os.chmod(run, 0o700)
    # Adopt/reap only our own grandchildren if a traced external service orphans.
    assert ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) == 0
    home = Path(os.environ['HOME'])
    config = Path(os.environ.get('OPENCODE_CONFIG_DIR') or
                  str(Path(os.environ.get('XDG_CONFIG_HOME') or str(home / '.config')) / 'opencode'))
    xdg_data = os.environ.get('XDG_DATA_HOME')
    default_data = (Path(xdg_data) / 'oc' if xdg_data and Path(xdg_data).is_absolute()
                    else home / '.local/share/oc')
    available = next(int(line.split()[1]) * 1024 for line in Path('/proc/meminfo').read_text().splitlines()
                     if line.startswith('MemAvailable:'))
    preflight = {'nonroot': True, 'head': head, 'binary_sha256': binary_hash,
                 'terminal_inherited_present': bool(os.environ.get('TERM')),
                 'locale_inherited_present': bool(os.environ.get('LANG')),
                 'environment_overrides': 0, 'cwd_unchanged': True,
                 'terminal_geometry': [120, 40], 'profile_deadline_seconds': PROFILE_SECONDS,
                 'shutdown_grace_seconds': GRACE_SECONDS,
                 'trace_per_log_cap_bytes': LOG_CAP, 'own_aggregate_cap_bytes': TOTAL_CAP,
                 'own_prior_bytes': prior_bytes, 'aggregate_headroom_bytes': HEADROOM,
                 'ram_available_bytes': available, 'disk_free_bytes': shutil.disk_usage(CACHE).free,
                 'default_root_preflight_metadata': metadata(default_data),
                 'default_lock_preflight_metadata': metadata(default_data / 'oc.lock'),
                 'raw_run_id': run.name}
    before = None
    if options.baseline_from_run:
        name = options.baseline_from_run
        assert re.fullmatch(r'T51-real-user-[a-z0-9_]+', name)
        previous = CACHE / name / 'sanitized.json'
        assert previous.stat().st_uid == os.getuid()
        prior = json.loads(previous.read_text())['profiles'][0]['trace']
        assert prior['native_sqlite_observed']
        assert prior['roots']['nativeStore']['opaque_root'] == opaque_path(default_data)
        before = snapshot(default_data / 'oc.sqlite')
        preflight['native_baseline_prior_trace_association'] = name
    visible_rows = parser()
    results = []
    results.append(Profile('existing', run, visible_rows, config, default_data, False, before).finish())
    fresh = run / 'fresh-native'
    fresh.mkdir(mode=0o700)
    results.append(Profile('fresh', run, visible_rows, config, fresh, True).finish())
    assert digest((REPO / 'target/release/oc').read_bytes()) == binary_hash
    summary = {'preflight': preflight, 'profiles': results}
    (run / 'sanitized.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))
    return 0 if all(profile['status'] == 'PASS' for profile in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
