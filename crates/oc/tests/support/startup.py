"""Isolated native startup PTY exercise; also runnable with target/release/oc.

Uses only an empty project, a static dummy provider and the approved temp root.
No input is submitted to the provider. env is a fresh allowlist per child.
"""
import errno
import fcntl
import json
import os
import pty
import re
import select
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unicodedata
from pathlib import Path

binary = str(Path(sys.argv[1]).resolve())
base = '/home/opencode/.cache/opencode-tmp/opencode'
assert Path(base).is_dir()


def drain(fd, seconds=.55):
    chunks = []
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if select.select([fd], [], [], .05)[0]:
            try:
                chunks.append(os.read(fd, 65536))
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
    return b''.join(chunks).decode('utf-8', errors='replace')


def visible_rows(stream, cols=120, rows=40):
    """Reconstruct the final PTY grid across sparse Ratatui cursor repaints."""
    grid = [[' '] * cols for _ in range(rows)]
    x = y = 0
    tokens = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|.', re.S)
    for match in tokens.finditer(stream):
        char = match.group()
        if char.startswith('\x1b['):
            code = char[-1]
            args = char[2:-1].lstrip('?').split(';')
            number = int(args[0]) if args[0].isdigit() else 1
            if code in 'Hf':
                x = (int(args[1]) if len(args) > 1 and args[1].isdigit() else 1) - 1
                y = number - 1
            elif code == 'G':
                x = number - 1
            elif code == 'A':
                y -= number
            elif code == 'B':
                y += number
            elif code == 'C':
                x += number
            elif code == 'D':
                x -= number
            elif code == 'J' and number in (1, 2):
                grid = [[' '] * cols for _ in range(rows)]
            elif code == 'K' and 0 <= y < rows:
                start, end = (0, x + 1) if number == 1 else (x, cols)
                grid[y][max(0, start):min(cols, end)] = [' '] * max(0, min(cols, end) - max(0, start))
        elif char.startswith('\x1b'):
            continue
        elif char == '\r':
            x = 0
        elif char == '\n':
            y += 1
        elif char >= ' ' and 0 <= y < rows and 0 <= x < cols:
            width = 2 if unicodedata.east_asian_width(char) in 'WF' else 1
            if unicodedata.combining(char):
                continue
            grid[y][x] = char
            if width == 2 and x + 1 < cols:
                grid[y][x + 1] = ' '
            x += width
    return [''.join(row) for row in grid]


def assert_dcp_warning(stream, label):
    rows = visible_rows(stream)
    # At 120 columns the exact category and first clause are on the first
    # content row; the complete remainder occupies the adjacent content row.
    # Check both rows, not sparse escape-separated bytes or loose fragments.
    first = 'warning: DCP settings have unsupported entries;'
    second = 'review native dcp settings'
    assert any(first in row and second in rows[index + 1]
               for index, row in enumerate(rows[:-1])), (label, rows)


with tempfile.TemporaryDirectory(prefix='oc-startup-', dir=base) as tmp:
    root = Path(tmp)
    home = root / 'home'
    project = root / 'project'
    config = home / 'config' / 'opencode'
    data = home / 'data' / 'oc'
    project.mkdir()
    config.mkdir(parents=True)
    (home / 'data').mkdir()
    fixture = {
        'model': 'fixture/offline',
        'provider': {'fixture': {
            'npm': '@ai-sdk/openai', 'name': 'Offline fixture',
            'options': {'baseURL': 'http://127.0.0.1:9/v1', 'apiKey': 'DUMMY-STARTUP-SECRET'},
            'models': {'offline': {'name': 'Offline', 'limit': {'context': 10000, 'output': 1000}}}
        }}
    }
    config_file = config / 'opencode.json'
    config_file.write_text(json.dumps(fixture))
    env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
           'XDG_DATA_HOME': str(home / 'data'), 'TERM': 'xterm-256color',
           'LANG': 'C.UTF-8', 'PATH': '/usr/bin:/bin'}

    def launch(data_dir=None, session=None):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
        original = termios.tcgetattr(slave)
        argv = [binary]
        if data_dir is not None:
            argv += ['--data-dir', str(data_dir)]
        if session is not None:
            argv += ['tui', '--session', session]
        child = subprocess.Popen(argv,
                                 cwd=project, env=env, stdin=slave, stdout=slave, stderr=slave)
        return child, master, slave, original

    def check(label, expected, data_dir=None, forbidden=(), session=None):
        child, master, slave, original = launch(data_dir, session)
        try:
            output = drain(master)
            assert child.poll() is None, (label, 'premature exit', child.returncode)
            for text in expected:
                assert text in output, (label, text, output)
            for text in (*forbidden, 'DUMMY-STARTUP-SECRET', str(root)):
                assert text not in output, (label, 'leaked', text)
            os.write(master, b'\x03')
            drain(master, .2)
            code = child.wait(timeout=5)
            assert (code == 0) == (label in ('success', 'lock released', 'warning',
                                            'credential present')), (label, code)
            assert termios.tcgetattr(slave) == original, (label, 'terminal not restored')
            print(f'{label}: exit {code}, terminal restored')
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
            os.close(master)
            os.close(slave)

    check('success', ['█▀▀█'], forbidden=('Native startup error',))
    # Hold the actual data-root lock with a live owner, then verify release.
    owner, master, slave, original = launch()
    try:
        assert '█▀▀█' in drain(master)
        check('busy', ['Native startup error', 'Data root busy', 'Close the other oc process'])
    finally:
        os.write(master, b'\x03')
        drain(master, .2)
        assert owner.wait(timeout=5) == 0
        assert termios.tcgetattr(slave) == original
        os.close(master)
        os.close(slave)
    check('lock released', ['█▀▀█'], forbidden=('Native startup error',))
    # A real selected provider with an env template must be distinguished from
    # malformed configuration before any request reaches its loopback endpoint.
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        listener.listen()
        listener.setblocking(False)
        fixture['provider']['fixture']['options']['baseURL'] = (
            f'http://127.0.0.1:{listener.getsockname()[1]}/v1')
        fixture['provider']['fixture']['options']['apiKey'] = '{env:FIXTURE_KEY}'
        config_file.unlink()
        config_file = config / 'opencode.jsonc'
        config_file.write_text(json.dumps(fixture))
        expected = ['Native startup error', 'Selected provider credential missing',
                    'export that variable in the launching shell']
        env.pop('FIXTURE_KEY', None)
        check('credential missing', expected,
              forbidden=('Configuration load failed', 'FIXTURE_KEY'))
        headless = subprocess.run([binary, 'run', 'no network call'], cwd=project,
                                  env=env, capture_output=True, timeout=5)
        assert headless.returncode == 1 and b'missing credential for provider.fixture.options.apiKey' in headless.stderr
        env['FIXTURE_KEY'] = ''
        check('credential empty env', expected, forbidden=('Configuration load failed',))
        env['FIXTURE_KEY'] = 'DUMMY-STARTUP-SECRET'
        check('credential present', ['█▀▀█'], forbidden=('Native startup error',))
        fixture['provider']['fixture']['options']['apiKey'] = ''
        config_file.write_text(json.dumps(fixture))
        check('credential empty literal', expected, forbidden=('Configuration load failed',))
        try:
            accepted, _ = listener.accept()
        except BlockingIOError:
            pass
        else:
            accepted.close()
            raise AssertionError('provider traffic during startup diagnostics')
        config_file.unlink()
        config_file = config / 'opencode.json'
        fixture['provider']['fixture']['options']['baseURL'] = 'http://127.0.0.1:9/v1'
        fixture['provider']['fixture']['options']['apiKey'] = 'DUMMY-STARTUP-SECRET'
        config_file.write_text(json.dumps(fixture))
    with sqlite3.connect(data / 'oc.sqlite') as db:
        db.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?,?,?)",
                   ('tui.session_location.foreign', str(root / 'another-project'), 'fixture'))
    check('query', ['Native startup error', 'Session / catalog query failed'],
          forbidden=('Configuration load failed',), session='foreign')
    epoch_key = 'tui.selection.legacy_epoch:' + json.dumps(
        [str(project), 'fixture'], separators=(',', ':'))
    with sqlite3.connect(data / 'oc.sqlite') as db:
        db.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?,?,?)",
                   (epoch_key, 'CORRUPT-PREF-SECRET', 'fixture'))
    check('selection corrupt', ['Native startup error', 'Storage or saved selection failed'],
          forbidden=('Native runtime initialization failed',))
    headless = subprocess.run([binary, 'run', 'no network call'], cwd=project, env=env,
                              capture_output=True, timeout=5)
    assert headless.returncode == 1 and b'malformed scoped selection preference' in headless.stderr
    assert b'CORRUPT-PREF-SECRET' not in headless.stderr
    print('selection corrupt headless: detailed error, exit 1, no provider call')
    with sqlite3.connect(data / 'oc.sqlite') as db:
        db.execute('DELETE FROM prefs WHERE key = ?', (epoch_key,))

    # An in-session Location switch must never render an application error's
    # raw config values/paths. A refused switch keeps the old generation live.
    target = root / 'bad-location'
    target.mkdir()
    (target / 'opencode.json').write_text(json.dumps({
        **fixture, 'model': 'fixture/LEAKME-MODEL-SECRET'}))
    broken = root / 'broken-location'
    broken.mkdir()
    (broken / 'opencode.json').write_text('{"apiKey":"LEAKME-CONFIG-SECRET", INVALID}')
    damaged = root / 'damaged-selection'
    damaged.mkdir()
    (damaged / 'opencode.json').write_text(json.dumps(fixture))
    with sqlite3.connect(data / 'oc.sqlite') as db:
        db.execute('INSERT INTO prefs(key,value,updated_at) VALUES (?,?,?)',
                   ('tui.selection.legacy_epoch:' + json.dumps(
                       [str(damaged), 'fixture'], separators=(',', ':')),
                    'LEAKME-SELECTION-SECRET', 'fixture'))
    for name, destination, marker, reason in [
        ('unknown model', target, 'LEAKME-MODEL-SECRET', 'Location configuration failed'),
        ('malformed config', broken, 'LEAKME-CONFIG-SECRET', 'Location configuration failed'),
        ('damaged selection', damaged, 'LEAKME-SELECTION-SECRET', 'Location storage failed'),
    ]:
        child, master, slave, original = launch()
        try:
            initial = drain(master)
            assert '█▀▀█' in initial
            os.write(master, f'/location {destination}\r'.encode())
            screen = drain(master, .8)
            assert reason in screen, (name, 'safe category missing', screen)
            assert marker not in screen, (name, 'leaked config', marker)
            assert 'application: ' not in screen and str(destination / 'opencode.json') not in screen, (name, screen)
            # Refusal retains the draft and old Location: retry with a valid
            # target in the same running application and observe publication.
            os.write(master, b'\x7f' * 512)
            erased = drain(master, .2)
            other = root / 'valid-location'
            other.mkdir(exist_ok=True)
            (other / 'opencode.json').write_text(json.dumps({
                **fixture, 'dcp': {'LEAKME-SWITCH-WARNING': True}}))
            os.write(master, f'/location {other}\r'.encode())
            result = drain(master, .8)
            assert_dcp_warning(initial + screen + erased + result, name)
            assert 'LEAKME-SWITCH-WARNING' not in result, (name, result)
            # Sessionless Home may leave an unchanged model label on-screen
            # without repainting its bytes. Open the actual target catalog to
            # assert the selected model survived the failed switch and retry.
            os.write(master, b'/models\r')
            catalog = drain(master, .3)
            assert 'Offline' in catalog, (name, 'target model missing', catalog)
            os.write(master, b'\x1b')
            drain(master, .1)
            os.write(master, b'\x03')
            drain(master, .2)
            assert child.wait(timeout=5) == 0, name
            assert termios.tcgetattr(slave) == original, name
            print(f'{name} switch: safe refusal, retained owner, retry exit 0')
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
            os.close(master)
            os.close(slave)
    config_file.write_text('{"apiKey":"DUMMY-STARTUP-SECRET", INVALID}')
    check('config', ['Native startup error', 'Configuration load failed',
                     'Check opencode.json/jsonc'])
    config_file.write_text(json.dumps(fixture))
    config_file.unlink()
    check('missing config', ['Native startup error', 'Configuration load failed'])
    config_file.write_text(json.dumps(fixture))
    (config / 'cli.json').write_text('{"session": INVALID}')
    check('cli config', ['Native startup error', 'Configuration load failed'])
    (config / 'cli.json').unlink()
    fixture['dcp'] = {'DUMMY-STARTUP-SECRET': True}
    config_file.write_text(json.dumps(fixture))
    # Startup notices are emitted on stderr before entering the alternate
    # screen; unlike the switch toast, their exact bytes are contiguous.
    check('warning', ['█▀▀█',
                      'warning: DCP settings have unsupported entries; review native dcp settings'],
          forbidden=('Native startup error',))
    del fixture['dcp']
    config_file.write_text(json.dumps(fixture))
    link = root / 'unsafe-link'
    link.symlink_to(data, target_is_directory=True)
    check('unsafe', ['Native startup error', 'Unsafe data root',
                     'Choose a private, owned data directory'], data_dir=link)
    blocked = root / 'not-a-directory'
    blocked.write_text('fixture')
    check('unavailable', ['Native startup error', 'Data root unavailable',
                          'Check data-directory access'], data_dir=blocked / 'oc')
