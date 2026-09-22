"""Isolated native startup PTY exercise; also runnable with target/release/oc.

Uses only an empty project, a static dummy provider and the approved temp root.
No input is submitted to the provider. env is a fresh allowlist per child.
"""
import errno
import fcntl
import json
import os
import pty
import select
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import time
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
            assert (code == 0) == (label in ('success', 'lock released', 'warning')), (label, code)
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
            assert '█▀▀█' in drain(master)
            os.write(master, f'/location {destination}\r'.encode())
            screen = drain(master, .8)
            assert reason in screen, (name, 'safe category missing', screen)
            assert marker not in screen, (name, 'leaked config', marker)
            assert 'application: ' not in screen and str(destination / 'opencode.json') not in screen, (name, screen)
            # Refusal retains the draft and old Location: retry with a valid
            # target in the same running application and observe publication.
            os.write(master, b'\x7f' * 512)
            drain(master, .2)
            other = root / 'valid-location'
            other.mkdir(exist_ok=True)
            (other / 'opencode.json').write_text(json.dumps({
                **fixture, 'dcp': {'LEAKME-SWITCH-WARNING': True}}))
            os.write(master, f'/location {other}\r'.encode())
            result = drain(master, .8)
            assert 'valid-location' in result and 'Offline' in result, (name, result)
            assert 'warning:' in result and 'DCP settings have unsupported' in result, (name, result)
            assert 'LEAKME-SWITCH-WARNING' not in result, (name, result)
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
    check('warning', ['█▀▀█', 'warning: DCP settings have unsupported entries'],
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
