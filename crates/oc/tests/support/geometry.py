import errno, fcntl, json, os, pty, re, select, signal, struct, subprocess, sys, termios, time
from pathlib import Path

binary, root = sys.argv[1], Path(sys.argv[2])
home = root / 'home'
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'),
       'XDG_DATA_HOME': str(home / 'data'), 'TERM': 'xterm-256color',
       'OC_TEST_ALLOW_LOOPBACK': '1', 'LANG': 'C.UTF-8'}

def size(fd, columns, rows):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', rows, columns, 0, 0))

def drain(fd, minimum=.2):
    data = b''
    deadline = time.monotonic() + minimum + .3
    quiet = time.monotonic() + minimum
    while time.monotonic() < deadline:
        if select.select([fd], [], [], .05)[0]:
            try:
                chunk = os.read(fd, 65536)
            except OSError as e:
                if e.errno == errno.EIO: break
                raise
            if not chunk: break
            data += chunk
            quiet = time.monotonic() + .15
        elif time.monotonic() >= quiet:
            break
    return data.decode('utf-8', errors='replace')

def launch(session, columns=160, rows=80):
    master, slave = pty.openpty()
    size(slave, columns, rows)
    argv = [binary] if session is None else [binary, 'tui', '--session', session]
    child = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                             cwd=root / 'project', env=env)
    os.close(slave)
    return child, master

def stop(child, fd):
    # Ctrl+C uses the existing quit route; no draft gets submitted to the peer.
    os.write(fd, b'\x03')
    drain(fd)
    try:
        code = child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        child.kill(); child.wait(); raise
    finally:
        os.close(fd)
    assert code == 0, code

for mode in ['normal', 'hidden', 'child', 'debug', 'vertical', 'default-debug']:
    config = {'session': {'sidebar': 'hide' if mode == 'hidden' else 'auto'},
              'debug': {'devtools': mode == 'debug'},
              'tabs': {'layout': 'vertical' if mode == 'vertical' else 'horizontal'}}
    if mode == 'default-debug': del config['debug']
    (home / 'config/opencode/cli.json').write_text(json.dumps(config))
    child, fd = launch('child' if mode == 'child' else 'long')
    try:
        initial = drain(fd, .8)
        assert len(set(re.findall(r'ROW-\d+', initial))) > 35, (mode, initial)
        assert ('Native runtime' in initial) == (mode in ['debug', 'default-debug']), mode
        assert ('Context' in initial) == (mode not in ['hidden', 'child', 'vertical']), mode
        os.write(fd, b'\x1b[200~draft-one\ndraft-two\ndraft-three\x1b[201~')
        drain(fd)
        for width, height in [(80,24),(120,40),(160,48),(43,48),(44,48),(119,48),(120,48),(121,48),(160,48)]:
            size(fd, width, height)
            os.kill(child.pid, signal.SIGWINCH)
            frame = drain(fd, .3)
            assert '[Pasted ~3 lines]' in frame and 'draft-three' not in frame, (mode, width, frame)
            expected = width > 120 and mode not in ['hidden', 'child', 'vertical']
            assert ('Context' in frame) == expected, (mode, width, frame)
            assert ('Native runtime' in frame) == (mode in ['debug', 'default-debug']), (mode, width)
        print(mode, 'resize/draft/sidebar/debug PASS')
    finally:
        stop(child, fd)

(home / 'config/opencode/cli.json').write_text('{}')
child, fd = launch('wrapped', 120, 80)
try:
    initial = drain(fd, .8)
    assert 'LAST-ANCHOR' in initial and 'FIRST-ANCHOR' not in initial
    os.write(fd, b'\x1b[A' * 200)
    scrolled = drain(fd, 1)
    assert 'FIRST-ANCHOR' in scrolled, scrolled
    print('wrapped rendered-row Up PASS')
finally:
    stop(child, fd)

for session in [None, 'empty']:
    child, fd = launch(session)
    try:
        initial = drain(fd, .8)
        # Bare launch restores the previously opened 'wrapped' real tab;
        # an explicit empty session selects its own tab without losing the deck.
        assert '█▀▀█' not in initial, initial
        assert 'Context' in initial, initial
        assert ('LAST-ANCHOR' in initial) == (session is None), initial
    finally:
        stop(child, fd)
print('bare retained tab and explicit empty session PASS')

child, fd = launch('foreign')
failed = drain(fd, .8)
assert 'Native startup error' in failed and 'Session / catalog query failed' in failed, failed
assert 'Context' not in failed and 'other-project' not in failed, failed
os.write(fd, b'\x03')
drain(fd)
assert child.wait(timeout=5) != 0
os.close(fd)
print('session query error route PASS')

# A hostile config error must become a distinct safe TUI, not raw stderr.
(home / 'config/opencode/opencode.json').write_text('{"model":"DO-NOT-LEAK-KEY", INVALID}')
child, fd = launch(None)
failed = drain(fd, .8)
assert 'Native startup error' in failed and 'Configuration load failed' in failed, failed
assert 'DO-NOT-LEAK-KEY' not in failed and 'Context' not in failed and '█▀▀█' not in failed, failed
os.write(fd, b'\x03')
drain(fd)
assert child.wait(timeout=5) != 0
os.close(fd)
non_tty = subprocess.run([binary], cwd=root / 'project', env=env, capture_output=True)
assert non_tty.returncode != 0 and b'TTY' in non_tty.stderr, non_tty
print('startup error and non-TTY PASS')
