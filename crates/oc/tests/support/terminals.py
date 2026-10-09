"""TERM01: real native binary, outer PTY, SQLite and independent Linux facts.

Only isolated temporary projects and a loopback scripted Responses peer. No live
API, host env, credential reads, production test switches or synthetic pane bytes.
The explicit binary argument permits the identical fixture on debug and release.
"""
import ctypes
import errno
import fcntl
import http.server
import json
import os
import pty
import re
import select
import signal
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import unicodedata
from pathlib import Path

BINARY = str(Path(sys.argv[1]).resolve())
BASE = Path('/home/opencode/.cache/opencode-tmp/opencode')
assert BASE.is_dir()
# Adopt only this fixture's orphaned children, enabling independent reap checks.
assert ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) == 0


def wait(predicate, label, seconds=8):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(.025)
    raise AssertionError(label)


def rows(stream, cols, height):
    # Existing startup fixture's sparse-Ratatui reconstruction, kept local to this
    # binary qualification; this is not the production PTY VT parser.
    grid = [[' '] * cols for _ in range(height)]
    x = y = 0
    tokens = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|.', re.S)
    for match in tokens.finditer(stream):
        char = match.group()
        if char.startswith('\x1b['):
            code = char[-1]
            args = char[2:-1].lstrip('?').split(';')
            n = int(args[0]) if args[0].isdigit() else 1
            if code in 'Hf':
                x = (int(args[1]) if len(args) > 1 and args[1].isdigit() else 1) - 1
                y = n - 1
            elif code == 'G': x = n - 1
            elif code == 'A': y -= n
            elif code == 'B': y += n
            elif code == 'C': x += n
            elif code == 'D': x -= n
            elif code == 'J' and n in (1, 2): grid = [[' '] * cols for _ in range(height)]
            elif code == 'K' and 0 <= y < height:
                a, b = (0, x + 1) if n == 1 else (0, cols) if n == 2 else (x, cols)
                grid[y][max(0, a):min(cols, b)] = [' '] * max(0, min(cols, b) - max(0, a))
        elif char.startswith('\x1b'): continue
        elif char == '\r': x = 0
        elif char == '\n': y += 1
        elif char >= ' ' and 0 <= y < height and 0 <= x < cols:
            width = 2 if unicodedata.east_asian_width(char) in 'WF' else 1
            if unicodedata.combining(char): continue
            grid[y][x] = char
            if width == 2 and x + 1 < cols: grid[y][x + 1] = ' '
            x += width
    return [''.join(row) for row in grid]


class View:
    def __init__(self, root, session='root', cols=120, height=40):
        self.root, self.cols, self.height = root, cols, height
        self.prefix = b'\x18'
        self.master, self.slave = pty.openpty()
        self.resize(cols, height)
        self.original = termios.tcgetattr(self.slave)
        env = {'HOME': str(root/'home'), 'XDG_CONFIG_HOME': str(root/'home/config'),
               'XDG_DATA_HOME': str(root/'home/data'), 'PATH': '/usr/bin:/bin',
               'LANG': 'C.UTF-8', 'TERM': 'xterm-256color', 'SHELL': '/bin/bash',
               'OC_API_KEY': 'SYNTHETIC-TERM01-CANARY', 'OC_TEST_ALLOW_LOOPBACK': '1'}
        def controlling_terminal():
            # Deliberately reproduce background-launch signal policy. The
            # native PTY child, not the fixture or parent application, must reset
            # inherited ignores/masks for normal interactive job control.
            for sig in (signal.SIGHUP, signal.SIGINT, signal.SIGQUIT):
                signal.signal(sig, signal.SIG_IGN)
            signal.pthread_sigmask(signal.SIG_BLOCK, (signal.SIGINT, signal.SIGQUIT))
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        self.child = subprocess.Popen([BINARY, '--data-dir', str(root/'data'), 'tui', '--session', session],
                                      cwd=root/'project', env=env, stdin=self.slave,
                                      stdout=self.slave, stderr=self.slave, preexec_fn=controlling_terminal)
        self.output = b''
        try:
            self.see('ctrl+p commands')
        except BaseException:
            self.close()
            raise

    def drain(self, seconds=.08):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if select.select([self.master], [], [], .01)[0]:
                try: self.output += os.read(self.master, 65536)
                except OSError as e:
                    if e.errno == errno.EIO: break
                    raise
        return self.output

    def screen(self):
        self.drain()
        return rows(self.output.decode('utf-8', errors='replace'), self.cols, self.height)

    def see(self, text):
        return wait(lambda: any(text in row for row in self.screen()),
                    f'missing {text!r}: {self.screen()}')

    def send(self, data):
        os.write(self.master, data)
        self.drain()

    def command(self, text):
        self.send(b'\x1b[200~' + text.encode() + b'\x1b[201~\r')

    def leader(self, key):
        self.send(self.prefix)
        self.send(key)

    def resize(self, cols, height):
        self.cols, self.height = cols, height
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', height, cols, 0, 0))

    def quit(self):
        self.leader(b'\x1b[D')
        self.send(b'\x01\x1b[1;2F\x7f')
        self.command('/quit')
        wait(lambda: self.child.poll() is not None, 'native shutdown')
        assert self.child.returncode == 0
        self.drain()
        assert termios.tcgetattr(self.slave) == self.original
        assert b'\x1b[?1049l' in self.output
        self.close()

    def close(self):
        if self.child.poll() is None:
            self.child.kill()
            self.child.wait(timeout=5)
        os.close(self.master)
        os.close(self.slave)


def entries(root):
    with sqlite3.connect(root/'data/oc.sqlite') as db:
        return [(json.loads(raw), live, json.loads(identity)) for raw, live, identity in
                db.execute('SELECT entry,live,process FROM terminals ORDER BY rowid')]


def selected(root, session='root'):
    with sqlite3.connect(root/'data/oc.sqlite') as db:
        value = db.execute('SELECT terminal_id FROM terminal_selection WHERE session_id=?', (session,)).fetchone()
        return value[0] if value else None


def proc_sample(pid):
    path = Path(f'/proc/{pid}')
    fields = (path/'stat').read_text().rsplit(') ', 1)[1].split()
    status = (path/'status').read_text().splitlines()
    return {'rss_kib': int(next(s.split()[1] for s in status if s.startswith('VmRSS:'))),
            'threads': int(next(s.split()[1] for s in status if s.startswith('Threads:'))),
            'fds': len(list((path/'fd').iterdir())), 'cpu_ticks': int(fields[11])+int(fields[12])}


def dead(pid):
    try: os.waitpid(pid, os.WNOHANG)
    except ChildProcessError: pass
    return not Path(f'/proc/{pid}').exists()


def foreground(pid):
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()
    group = int(fields[5])
    try: name = Path(f'/proc/{group}/comm').read_text().strip()
    except FileNotFoundError: name = ''
    return group, name


VT = r'''
import os, sys, termios, tty
old=termios.tcgetattr(0); tty.setraw(0)
try:
    os.write(1,b'\x1b[2J\x1b[H\x1b[31mRED\x1b[0m '+ '界'.encode()+b'\x1b[2;1HVT_READY\x1b[?25l\x1b[5n')
    reply=os.read(0,4)
    assert reply==b'\x1b[0n',reply
    os.write(1,b'\x1b[3;1HDSR_OWNED\x1b]52;c;PRIVATE_CLIPBOARD\x07\x1bPPRIVATE_DEVICE\x1b\\')
    old_leader=os.read(0,1)
    assert old_leader==b'\x18',old_leader
    os.write(1,b'\x1b[4;1HOLD_LEADER_RAW_24')
    command=os.read(0,1)
    assert command==b'f',command
    os.write(1,b'\x1b]52;c;'+b'x'*1000000+b'\x07'+b'a'*100000+b'\r\nFLOOD_DONE\r\n')
    os.write(1,b'\x1b[?25hINPUT_READY\r\n')
    value=os.read(0,1)
    os.write(1,b'RAW_'+str(value[0]).encode()+b'\r\n')
finally: termios.tcsetattr(0,termios.TCSANOW,old)
'''

REQUESTS = []
CHILD_STARTED = threading.Event()
CHILD_RELEASE = threading.Event()


def event(value):
    return ('data: ' + json.dumps(value) + '\n\n').encode()


class Peer(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        REQUESTS.append(request)
        assert not any(t['name'] in ('terminal', 'terminal_input', 'terminal_create') for t in request.get('tools', []))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Connection', 'close')
        self.end_headers()
        if request['model'] == 'child':
            self.wfile.write(event({'type': 'response.output_text.delta', 'delta': 'CHILD_BLOCKED'}))
            self.wfile.flush()
            CHILD_STARTED.set()
            assert CHILD_RELEASE.wait(15), 'child fixture barrier'
            self.wfile.write(event({'type': 'response.completed', 'response': {'status': 'completed', 'output': []}}))
        elif any(i.get('role') == 'user' and any(c.get('text') == 'DELEGATE_TERM01' for c in i.get('content', [])) for i in request['input']) and not any(i.get('type') == 'function_call_output' for i in request['input']):
            args = json.dumps({'agent': 'helper', 'description': 'Term01 child', 'prompt': 'CHILD_TASK'})
            self.wfile.write(event({'type': 'response.output_item.added', 'item': {'type': 'function_call', 'id': 'child-item', 'call_id': 'child-call', 'name': 'subagent'}}))
            self.wfile.write(event({'type': 'response.function_call_arguments.delta', 'item_id': 'child-item', 'delta': args}))
            self.wfile.write(event({'type': 'response.completed', 'response': {'status': 'completed', 'output': [{'type': 'function_call', 'id': 'child-item', 'call_id': 'child-call', 'name': 'subagent', 'arguments': args, 'status': 'completed'}]}}))
        else:
            self.wfile.write(event({'type': 'response.output_text.delta', 'delta': 'Fixture final'}))
            self.wfile.write(event({'type': 'response.completed', 'response': {'status': 'completed', 'output': []}}))


peer = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Peer)
peer.daemon_threads = True
threading.Thread(target=peer.serve_forever, daemon=True).start()

with tempfile.TemporaryDirectory(prefix='oc-term01-', dir=BASE) as directory:
    root = Path(directory)
    (root/'home/config/opencode').mkdir(parents=True)
    (root/'project').mkdir()
    config = {'disabled_providers': ['opencode-go'], 'animations': False,
              'keybinds': {'terminal.close': 'ctrl+c'}, 'model': 'fixture/root',
              'permission': {'*': 'allow'},
              'agent': {'helper': {'mode': 'subagent', 'model': 'fixture/child', 'prompt': 'Fixture child'}},
              'provider': {'fixture': {'npm': '@ai-sdk/openai',
                  'options': {'baseURL': f'http://127.0.0.1:{peer.server_port}/v1', 'apiKey': 'SYNTHETIC-TERM01-CANARY'},
                  'models': {name: {'name': 'Local fixture', 'limit': {'context': 32768, 'output': 2048}} for name in ('root', 'child')}}}}
    config_path = root/'project/opencode.json'
    config_path.write_text(json.dumps(config))
    # Pin the pane profile across local/debug and packaged/release channels;
    # the optional local devtools row has its own layout tests.
    (root/'project/cli.json').write_text(json.dumps({'debug': {'devtools': False}}))
    (root/'project/vt.py').write_text(VT)
    view = View(root)
    known = []
    births = {}
    def remember(entry):
        pid = entry['pid']
        known.append(pid)
        births[pid] = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()[19]
    try:
        baseline = proc_sample(view.child.pid)
        assert entries(root) == [], 'startup must not spawn a PTY'
        view.leader(b'\x1b[B')
        view.see('+ New terminal')
        view.send(b'\r')
        assert entries(root) == [], 'undefined initial Enter is a no-op'
        view.send(b'\x1b[A')
        view.send(b'\r')
        first = wait(lambda: entries(root), 'first real PTY')[0][0]
        remember(first)
        # The pinned pane is borderless: readiness is its real interactive
        # prompt, not an invented native title consuming a terminal row.
        wait(lambda: any('$ ' in row[view.cols//2:] for row in view.screen()),
             'actual borderless PTY prompt')
        assert first['cwd'] == str(root/'project') and first['shell'] == '/bin/bash'
        for entry, live, identity in entries(root):
            assert live == 1 and identity['pid'] == entry['pid']
            assert os.readlink(f'/proc/{entry["pid"]}/cwd') == entry['cwd']
            assert b'SYNTHETIC-TERM01-CANARY' not in Path(f'/proc/{entry["pid"]}/environ').read_bytes()
        view.command("printf 'one-%s\\n' OK")
        view.see('one-OK')
        view.command('sleep 30')
        sleep_group = wait(lambda: foreground(first['pid'])[0] if foreground(first['pid'])[1] == 'sleep' else None, 'actual foreground sleep before raw interrupt')
        view.send(b'\x03')
        def interrupted():
            # The fixture is the host terminal consumer. Keep its output flowing
            # while independently observing processes, rather than potentially
            # blocking the native UI writer during this input-delivery check.
            view.drain(.01)
            return foreground(first['pid'])[0] == first['pid'] and dead(sleep_group)
        try:
            wait(interrupted, 'raw Ctrl+C stops exact foreground process')
        except AssertionError:
            fd = os.open(f'/proc/{first["pid"]}/fd/0', os.O_RDONLY | os.O_NONBLOCK)
            try:
                attrs = termios.tcgetattr(fd)
                tty_state = {'isig': bool(attrs[3] & termios.ISIG), 'intr': repr(attrs[6][termios.VINTR])}
            finally:
                os.close(fd)
            ignored = [line for line in Path(f'/proc/{sleep_group}/status').read_text().splitlines()
                       if line.startswith(('SigIgn:', 'SigBlk:'))]
            print({'foreground': foreground(first['pid']), 'sleep_dead': dead(sleep_group),
                   'selected': selected(root), 'oc_exit': view.child.poll(),
                   'tty': tty_state, 'signals': ignored, 'screen': view.screen()}, file=sys.stderr)
            raise
        assert view.child.poll() is None and selected(root) == first['target']['id'], 'raw Ctrl+C must not run remapped close'
        view.command("printf 'control-%s\\n' OK")
        view.see('control-OK')
        if '--control-only' in sys.argv:
            view.quit()
            assert all(dead(pid) for pid in known)
            print('TERM01 foreground/raw Ctrl+C reproduction PASS')
            sys.exit(0)
        view.leader(b'\x1b[D')
        view.command('/terminal')
        second = wait(lambda: entries(root) if len(entries(root)) == 2 else None, 'second real PTY')[1][0]
        remember(second)
        assert selected(root) == second['target']['id']
        # Select the first, hide, then toggle-on must choose LAST without spawn.
        view.leader(b'\x1b[B')
        view.send(b'k\r')
        wait(lambda: selected(root) == first['target']['id'], 'select first')
        view.leader(b't')
        wait(lambda: selected(root) is None, 'hide clears persistent selection')
        assert all(Path(f'/proc/{pid}').exists() for pid in known)
        view.leader(b't')
        wait(lambda: selected(root) == second['target']['id'], 'toggle last')
        assert len(entries(root)) == 2
        view.command('stty size > size')
        wait(lambda: (root/'project/size').exists(), 'child size')
        old_size = (root/'project/size').read_text()
        view.resize(80, 24)
        time.sleep(.15)
        view.command('stty size > size')
        wait(lambda: (root/'project/size').read_text() != old_size, '80x24 real child resize')
        compact = tuple(map(int, (root/'project/size').read_text().split()))
        assert compact == (23, 38), ('full-height 80x24 pane with one-cell horizontal insets', compact)
        old_size = (root/'project/size').read_text()
        view.resize(160, 48)
        time.sleep(.15)
        view.command('stty size > size')
        wait(lambda: (root/'project/size').read_text() != old_size, 'real child resized')
        actual = tuple(map(int, (root/'project/size').read_text().split()))
        assert actual == (47, 78), ('full-height 160x48 pane with one-cell horizontal insets', actual)
        view.leader(b'\x1b[D')
        (root/'project/cli.json').write_text(json.dumps({'tabs': {'layout': 'vertical'}, 'debug': {'devtools': False}}))
        view.command('/reload')
        time.sleep(.2)
        view.leader(b'\x1b[C')
        view.command('stty size > vertical-size')
        wait(lambda: (root/'project/vertical-size').exists(), 'vertical child resize')
        vertical = tuple(map(int, (root/'project/vertical-size').read_text().split()))
        assert vertical != actual and vertical[1] < actual[1]
        view.leader(b'\x1b[D')
        (root/'project/cli.json').write_text(json.dumps({'tabs': {'layout': 'horizontal'}, 'debug': {'devtools': False}, 'keybinds': {'leader': 'ctrl+g'}}))
        view.command('/reload')
        time.sleep(.2)
        view.prefix = b'\x07'
        view.leader(b'\x1b[C')
        view.command('python3 vt.py')
        view.see('DSR_OWNED')
        frame = view.screen()
        assert any('RED 界' in row for row in frame)
        visibility = re.findall(rb'\x1b\[\?25([lh])', view.output)
        assert visibility and visibility[-1] == b'l', 'native VT hides host caret, not composer cursor'
        assert b'PRIVATE_CLIPBOARD' not in view.output and b'PRIVATE_DEVICE' not in view.output
        assert b'\x1b]52;' not in view.output
        view.send(b'\x18')
        view.see('OLD_LEADER_RAW_24')
        view.send(b'f')
        view.see('FLOOD_DONE')
        view.see('INPUT_READY')
        assert b'output gap' in view.output, 'bounded old cursor produces explicit native gap recovery'
        view.send(b'\x04')
        view.see('RAW_4')
        assert view.child.poll() is None
        view.leader(b'\x1b[D')
        (root/'project/cli.json').write_text(json.dumps({'tabs': {'layout': 'horizontal'}, 'debug': {'devtools': False}}))
        view.command('/reload')
        time.sleep(.2)
        view.prefix = b'\x18'
        # Transcript wheel must preserve raw terminal focus; first click releases
        # are consumed and restore the original Unicode draft and caret.
        view.leader(b'\x1b[D')
        view.send('draft界'.encode())
        view.leader(b'\x1b[C')
        view.send(b'\x1b[<64;8;8M')
        view.command("printf 'wheel-%s\\n' OK")
        view.see('wheel-OK')
        view.send(b'\x1b[<0;8;8M\x1b[<0;8;8m')
        view.see('draft界')
        view.send(b'\x01\x1b[1;2F\x7f')
        # Hidden output continues draining; reopening attaches its real screen.
        view.leader(b'\x1b[C')
        view.command("(sleep .3; printf 'hidden-%s\\n' OK) &")
        view.leader(b't')
        time.sleep(.4)
        view.leader(b't')
        view.see('hidden-OK')
        sampled = proc_sample(view.child.pid)
        assert sampled['fds'] <= baseline['fds'] + 8
        assert sampled['threads'] <= baseline['threads'] + 4
        assert sampled['rss_kib'] - baseline['rss_kib'] < 65536
        time.sleep(.3)
        idle = proc_sample(view.child.pid)['cpu_ticks'] - sampled['cpu_ticks']
        assert idle <= 6, 'idle terminal screens do not busy-spin'
        print('TERM01 native resource facts:', json.dumps({'baseline': baseline, 'two_pty': sampled, 'compact_child_size': compact, 'child_size': actual, 'vertical_child_size': vertical, 'idle_ticks_300ms': idle}))
        # Genuine Ctrl+D exits the selected shell, not the application/editor.
        view.send(b'\x04')
        wait(lambda: dead(second['pid']), 'raw EOF exit/reap')
        wait(lambda: selected(root) is None, 'exit clears selected ID')
        assert view.child.poll() is None
        assert REQUESTS == [], 'manual PTYs must not accept model/title/tool work'
        # Actual parent delegates; open the REAL held child, then activate New in
        # its lower composer. Closing returns to parent BEFORE captured dispatch.
        view.command('DELEGATE_TERM01')
        try:
            wait(CHILD_STARTED.is_set, 'real native live child request')
        except AssertionError:
            print('child admission diagnostic:', [{'model': r['model'], 'types': [i.get('type') for i in r['input']], 'users': [i.get('content') for i in r['input'] if i.get('role') == 'user']} for r in REQUESTS], view.screen())
            raise
        with sqlite3.connect(root/'data/oc.sqlite') as db:
            child_session = db.execute('SELECT id FROM sessions WHERE parent_id=?', ('root',)).fetchone()[0]
        view.send(b'parent-draft')
        view.send(b'\x07')
        view.see('Subagents')
        view.send(b'\r')
        view.see('CHILD_BLOCKED')
        view.leader(b'\x1b[B')
        view.see('+ New terminal')
        view.send(b'k\r')
        child_terminal = wait(lambda: next((e for e, live, _ in entries(root) if live and e['target']['session'] == child_session), None), 'captured child PTY')
        remember(child_terminal)
        view.see('parent-draft')
        assert selected(root, child_session) == child_terminal['target']['id']
        assert selected(root) is None and child_terminal['cwd'] == str(root/'project')
        view.send(b'\x07\r')
        view.see('CHILD_BLOCKED')
        view.leader(b'\x1b[C')
        view.command("printf 'child-route-%s\\n' OK")
        view.see('child-route-OK')
        view.leader(b'\x1b[B')
        view.send(b'\x1b')
        view.see('parent-draft')
        CHILD_RELEASE.set()
        wait(lambda: any(i.get('type') == 'function_call_output' for r in REQUESTS for i in r['input']), 'settled real parent follow-up')
        view.leader(b'\x1b[B')
        # The Terminals composer has no kill action. The configured close hides
        # the pane without killing; select the same process again for raw EOF.
        # Explicit owner Remove/reap is independently exercised by the real
        # application test, not an invented composer Ctrl+D action.
        view.send(b'j\r')
        wait(lambda: selected(root) == first['target']['id'], 'captured first PTY for explicit close')
        view.leader(b'\x1b[D')
        view.send(b'\x03')
        wait(lambda: selected(root) is None, 'configured close clears selection')
        assert not dead(first['pid']) and entries(root)[0][1] == 1, 'close must not kill'
        view.leader(b'\x1b[B')
        view.send(b'j\r')
        wait(lambda: selected(root) == first['target']['id'], 'same first PTY after close')
        view.send(b'\x04')
        def exited():
            # Process reap precedes the owner's durable publication/ack. Require
            # both facts instead of racing the final SQLite transaction.
            view.drain(.01)
            entry, live, _ = entries(root)[0]
            return dead(first['pid']) and entry['state'] == 'Exited' and live == 0
        wait(exited, 'selected PTY raw EOF/reap/publication')
        view.send(b'\x1b')
        view.quit()
        wait(lambda: all(dead(pid) for pid in known), 'clean shutdown owns both PTYs')
        assert all(live == 0 for _, live, _ in entries(root))
        view = View(root)
        assert len(entries(root)) == 3 and selected(root) is None, 'restart never respawns old PTYs'
        view.command('/terminal')
        third = wait(lambda: entries(root) if len(entries(root)) == 4 else None, 'new crash PTY')[3][0]
        remember(third)
        view.command("trap '' HUP; printf 'effect\\n' >> effect; sleep 30")
        wait(lambda: (root/'project/effect').exists(), 'real pre-crash effect')
        view.leader(b'\x1b[D')
        view.command('/terminal')
        fourth = wait(lambda: entries(root) if len(entries(root)) == 5 else None, 'second crash PTY')[4][0]
        remember(fourth)
        view.command("trap '' HUP; printf 'effect-two\\n' >> effect-two; sleep 30")
        wait(lambda: (root/'project/effect-two').exists(), 'second real pre-crash effect')
        time.sleep(.05)
        view.child.kill()
        view.child.wait(timeout=5)
        view.close()
        assert all(Path(f'/proc/{pid}').exists() for pid in (third['pid'], fourth['pid'])), 'real retained orphan leaders'
        view = View(root)
        wait(lambda: all(dead(pid) for pid in (third['pid'], fourth['pid'])), 'verified orphan quarantine')
        for interrupted in entries(root)[3:]:
            assert interrupted[1] == 0 and interrupted[0]['state'] == 'Interrupted'
        assert len(entries(root)) == 5 and selected(root) is None
        assert (root/'project/effect').read_text() == 'effect\n', 'unknown command must not replay'
        assert (root/'project/effect-two').read_text() == 'effect-two\n'
        view.quit()
        # A forged/stale starttime never grants authority over a same-UID live
        # session leader. This process is independently owned by the fixture.
        sentinel = subprocess.Popen(['/bin/sleep', '30'], env={'PATH': '/usr/bin:/bin'}, start_new_session=True)
        try:
            entry, _, identity = entries(root)[-1]
            entry['target']['id'] = 'pty-stale-fixture'
            entry['pid'] = sentinel.pid
            entry['state'] = 'Running'
            identity['pid'] = sentinel.pid
            identity['start_ticks'] = 0
            with sqlite3.connect(root/'data/oc.sqlite') as db:
                db.execute('INSERT INTO terminals(id,session_id,entry,process,live) VALUES(?,?,?,?,1)', (entry['target']['id'], 'root', json.dumps(entry), json.dumps(identity)))
                db.execute('UPDATE terminal_selection SET terminal_id=? WHERE session_id=?', (entry['target']['id'], 'root'))
            view = View(root)
            assert sentinel.poll() is None
            stale = entries(root)[-1]
            assert stale[1] == 0 and stale[0]['state'] == 'Interrupted' and selected(root) is None
            view.quit()
            assert sentinel.poll() is None
        finally:
            sentinel.kill()
            sentinel.wait(timeout=5)
        # Real refresh and create refusals retain Linux controls and their source
        # callsite error, while shutdown reports unavailable recovery as failure.
        with sqlite3.connect(root/'data/oc.sqlite') as db:
            db.execute('INSERT INTO terminals(id,session_id,entry,process,live) VALUES(?,?,?, ?,1)', ('bad-recovery', 'root', '{}', '{}'))
        view = View(root)
        count = len(entries(root))
        view.leader(b'\x1b[B')
        view.see('Unable to load terminal')
        view.see('+ New terminal')
        view.send(b'k\r')
        view.see('Unable to load terminal')
        assert len(entries(root)) == count
        view.command('/quit')
        wait(lambda: view.child.poll() is not None, 'unavailable cleanup is not success')
        assert view.child.returncode != 0 and termios.tcgetattr(view.slave) == view.original
        view.close()
        print('TERM01 actual binary:', Path(BINARY).parent.name, 'PASS; raw/control/VT/resize/hide/last/focus/shutdown/crash/no-replay')
    finally:
        if view.child.poll() is None: view.close()
        for pid in known:
            if not dead(pid):
                # Only exact fixture-owned, still-existing leaders from this run.
                fields = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()
                assert fields[19] == births[pid] and int(fields[2]) == pid, 'never kill recycled/unverified fixture PID'
                try: os.killpg(pid, signal.SIGKILL)
                except ProcessLookupError: pass
                wait(lambda: dead(pid), 'fixture orphan reap')
