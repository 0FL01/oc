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

def stop(child, fd, draft_session=None):
    if draft_session is not None:
        import sqlite3
        def user_count():
            with sqlite3.connect(home / 'data/oc/oc.sqlite') as db:
                return db.execute('SELECT COUNT(*) FROM messages WHERE session_id=? AND role=?',
                                  (draft_session, 'user')).fetchone()[0]
        before = user_count()
        assert before == 0, 'resize/paste submitted a user message'
        os.write(fd, b'\x03')
        cleared = drain(fd, .3)
        assert child.poll() is None, 'nonempty Ctrl+C exited instead of clearing'
        assert '[Pasted ~3 lines]' not in cleared, 'draft still visible after Ctrl+C'
        assert user_count() == before, 'cleared draft was submitted'
    os.write(fd, b'\x03')
    restored = drain(fd)
    try:
        code = child.wait(timeout=5)
        restored += drain(fd, .1)
        flags = termios.tcgetattr(fd)[3]
    except subprocess.TimeoutExpired:
        child.kill(); child.wait(); raise
    finally:
        os.close(fd)
    assert code == 0, code
    assert '\x1b[?1049l' in restored, 'alternate screen not left'
    assert flags & (termios.ICANON | termios.ECHO) == (termios.ICANON | termios.ECHO), 'terminal not restored'

def screen_rows(output, columns, height):
    # Reconstruct cells so a sidebar title cannot accidentally match its tab.
    cells = [[' '] * columns for _ in range(height)]
    x = y = 0
    for token in re.findall(r'\x1b\[[0-?]*[ -/]*[@-~]|.', output, re.S):
        if token.startswith('\x1b['):
            code, args = token[-1], token[2:-1]
            if args.startswith('?'): continue
            values = [int(p) if p else 0 for p in args.split(';')]
            n = values[0] or 1
            if code in 'Hf':
                y, x = n - 1, (values[1] if len(values) > 1 and values[1] else 1) - 1
            elif code == 'G': x = n - 1
            elif code == 'd': y = n - 1
            elif code == 'A': y = max(0, y - n)
            elif code == 'B': y += n
            elif code == 'C': x += n
            elif code == 'D': x = max(0, x - n)
            elif code == 'J' and values[0] in (2, 3): cells = [[' '] * columns for _ in range(height)]
            elif code == 'K' and 0 <= y < height:
                start, end = (0 if values[0] in (1, 2) else x), (columns if values[0] in (0, 2) else x + 1)
                cells[y][start:end] = [' '] * max(0, end - start)
        elif token == '\r': x = 0
        elif token == '\n': y += 1
        elif token >= ' ':
            if 0 <= y < height and 0 <= x < columns: cells[y][x] = token
            x += 1
    return [''.join(row) for row in cells]

for mode in ['normal', 'hidden', 'child', 'debug', 'vertical', 'default-debug']:
    config = {'session': {'sidebar': 'hide' if mode == 'hidden' else 'auto'},
              'debug': {'devtools': mode == 'debug'},
              'tabs': {'layout': 'vertical' if mode == 'vertical' else 'horizontal'}}
    if mode == 'default-debug': del config['debug']
    (home / 'config/opencode/cli.json').write_text(json.dumps(config))
    child, fd = launch('child' if mode == 'child' else 'long')
    drafted = False
    try:
        initial = drain(fd, .8)
        assert len(set(re.findall(r'ROW-\d+', initial))) > 35, (mode, initial)
        assert ('Native runtime' in initial) == (mode == 'debug'), mode
        assert ('Context' in initial) == (mode not in ['hidden', 'child', 'vertical']), mode
        os.write(fd, b'\x1b[200~draft-one\ndraft-two\ndraft-three\x1b[201~')
        drafted = True
        drain(fd)
        for width, height in [(80,24),(120,40),(160,48),(43,48),(44,48),(119,48),(120,48),(121,48),(160,48)]:
            size(fd, width, height)
            os.kill(child.pid, signal.SIGWINCH)
            frame = drain(fd, .3)
            assert '[Pasted ~3 lines]' in frame and 'draft-three' not in frame, (mode, width, frame)
            expected = width > 120 and mode not in ['hidden', 'child', 'vertical']
            assert ('Context' in frame) == expected, (mode, width, frame)
            assert ('Native runtime' in frame) == (mode == 'debug'), (mode, width)
        print(mode, 'resize/draft/sidebar/debug PASS')
    finally:
        stop(child, fd, ('child' if mode == 'child' else 'long') if drafted else None)

(home / 'config/opencode/cli.json').write_text('{}')
child, fd = launch('wrapped', 120, 80)
try:
    initial = drain(fd, .8)
    assert 'LAST-ANCHOR' in initial and 'FIRST-ANCHOR' not in initial
    # Up now owns shared prompt-input history, including retained drafts from
    # the preceding resize cases. Use the actual transcript wheel owner here;
    # editor PageUp is deliberately inert, and neither action recalls input.
    os.write(fd, b'\x1b[<64;40;20M' * 200)
    scrolled = drain(fd, 1)
    assert 'FIRST-ANCHOR' in scrolled, scrolled
    print('wrapped rendered-row wheel scroll PASS')
finally:
    stop(child, fd)

for session in [None, 'empty']:
    child, fd = launch(session)
    try:
        initial = drain(fd, .8)
        # A bare restart restores the authority's active wrapped root;
        # --session explicitly selects the requested empty session instead.
        if session is None:
            assert '█▀▀█' not in initial and 'LAST-ANCHOR' in initial, initial
            assert 'Context' in initial, initial
        else:
            assert '█▀▀█' not in initial and 'Context' not in initial, initial
            assert any('Empty sidebar witness' in row[160-42:] for row in screen_rows(initial, 160, 80)[1:]), initial
            assert 'LAST-ANCHOR' not in initial, initial
    finally:
        stop(child, fd)
print('shared active history on bare restart and explicit empty session PASS')

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
