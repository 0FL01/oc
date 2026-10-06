"""Native auth CLI: no model/config/MCP dependency, no real authorization dispatch."""
import copy, json, os, pathlib, pty, select, signal, socket, sqlite3, subprocess, sys, tempfile, termios, time, urllib.parse

BIN = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix='t57-auth-cli-', dir=os.environ['TMPDIR']) as temp:
    root = pathlib.Path(temp)
    project, home, data = root/'project', root/'home', root/'data'
    project.mkdir(); home.mkdir()
    # These auth-only commands do not load a runtime generation or attach MCP.
    (project/'opencode.json').write_text('{malformed runtime configuration')
    env = {'PATH':'/usr/bin:/bin', 'HOME':str(home), 'XDG_CONFIG_HOME':str(home/'config'),
           'XDG_DATA_HOME':str(home/'data'), 'XDG_STATE_HOME':str(home/'state'),
           'TERM':'xterm-256color', 'LANG':'C.UTF-8', 'SHELL':'/bin/bash'}
    tools = root/'bin'; tools.mkdir()
    opened = root/'browser-opened'
    opener = tools/'xdg-open'
    opener.write_text('#!/usr/bin/python3\nimport os, pathlib\n'
                      'assert not any(k in os.environ for k in ["OPENAI_API_KEY","OC_API_KEY"])\n'
                      + 'pathlib.Path('+repr(str(opened))+').write_text("opened")\n')
    opener.chmod(0o700)
    env.update(PATH=str(tools)+':/usr/bin:/bin', OPENAI_API_KEY='PARENT_KEY_CANARY', OC_API_KEY='PARENT_OC_CANARY')
    def command(*args, directory=data):
        return [BIN, '--data-dir', str(directory), 'auth', *args]
    def run(*args, success=True, directory=data):
        result = subprocess.run(command(*args, directory=directory), cwd=project, env=env,
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=8)
        assert (result.returncode == 0) == success, (args, result.returncode, result.stderr)
        assert b'KEY_CANARY' not in result.stdout+result.stderr
        assert b'ARGV_CANARY' not in result.stdout+result.stderr
        return result
    absent = root/'absent'
    for args in [('login',), ('login','openai'), ('login','openai','--method','key'),
                 ('login','openai','--method','key','--answer','apiKey=ARGV_CANARY'),
                 ('login','foreign','--method','key')]:
        run(*args, success=False, directory=absent)
        assert not absent.exists()
    assert json.loads(run('list','--format','json').stdout) == []

    def native_key(label, key, cancel=False, choose=False):
        master, slave = pty.openpty()
        before = copy.deepcopy(termios.tcgetattr(slave))
        def tty():
            os.setsid()
            import fcntl
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        args = ('login','openai','--answer','label='+label) if choose else ('login','openai','--method','key','--answer','label='+label)
        proc = subprocess.Popen(command(*args),
                                cwd=project, env=env, stdin=slave, stdout=slave, stderr=slave, preexec_fn=tty)
        transcript = bytearray()
        def drain():
            if select.select([master],[],[],0.02)[0]:
                try: transcript.extend(os.read(master, 65536))
                except OSError: pass
        try:
            if choose:
                deadline = time.monotonic()+8
                while b'OpenAI authentication method' not in transcript and time.monotonic()<deadline: drain()
                assert b'ChatGPT Pro/Plus (browser)' in transcript and b'ChatGPT Pro/Plus (headless)' in transcript
                os.write(master, b'3\r')
            deadline = time.monotonic()+8
            while b'API key: ' not in transcript and time.monotonic()<deadline:
                drain(); assert proc.poll() is None
            assert b'API key: ' in transcript
            os.write(master, b'\x1b[200~'+key+b'\x1b[201~')
            deadline = time.monotonic()+8
            while b'*****' not in transcript and time.monotonic()<deadline: drain()
            assert b'*****' in transcript and key not in transcript
            os.write(master, b'\x1b' if cancel else b'\r')
            deadline = time.monotonic()+8
            while proc.poll() is None and time.monotonic()<deadline: drain()
            assert proc.poll() == (130 if cancel else 0), bytes(transcript)
            while select.select([master],[],[],0)[0]:
                try: transcript.extend(os.read(master,65536))
                except OSError: break
            assert key not in transcript
            assert termios.tcgetattr(slave) == before
            assert b'\x1b[?2004l' in transcript
            if not cancel: assert b'Server authorization has not been validated.' in transcript
        finally:
            if proc.poll() is None: proc.kill()
            proc.wait(); os.close(master); os.close(slave)
    native_key('Alpha', b'FIRST_KEY_CANARY')
    native_key('Beta', b'SECOND_KEY_CANARY', choose=True)
    native_key('Cancelled', b'CANCELLED_KEY_CANARY', True)
    rows = json.loads(run('list','--format','json').stdout)
    assert len(rows) == 2 and [r['label'] for r in rows] == ['Alpha','Beta']
    assert all(r['provider']=='openai' and r['kind']=='key' and r['methodID']=='key' for r in rows)
    assert [r['active'] for r in rows] == [False,True]
    run('switch','openai', success=False)
    run('logout','openai', success=False)
    run('switch','openai',rows[0]['id'])
    assert [r['active'] for r in json.loads(run('list','--format','json').stdout)] == [True,False]
    run('logout','openai',rows[0]['id'])
    remaining = json.loads(run('list','--format','json').stdout)
    assert len(remaining)==1 and remaining[0]['active'] and remaining[0]['id']==rows[1]['id']

    def browser(interrupt):
        proc = subprocess.Popen(command('login','openai','--method','chatgpt-browser'), cwd=project, env=env,
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        out = bytearray()
        callback = None
        try:
            deadline = time.monotonic()+8
            while callback is None and time.monotonic()<deadline:
                assert proc.poll() is None
                if select.select([proc.stdout],[],[],0.05)[0]: out.extend(os.read(proc.stdout.fileno(),65536))
                for line in bytes(out).decode().splitlines():
                    if line.startswith('https://auth.openai.com/oauth/authorize?'):
                        query = urllib.parse.parse_qs(urllib.parse.urlsplit(line).query)
                        assert query['client_id']==['app_EMoamEEZ73f0CkXaXp7hrann'] and query['code_challenge_method']==['S256']
                        assert len(query['state'][0])==43
                        callback = urllib.parse.urlsplit(query['redirect_uri'][0])
            assert callback is not None and callback.port in [1455,1457]
            if interrupt:
                os.kill(proc.pid, signal.SIGINT)
            else:
                sock = socket.create_connection(('127.0.0.1',callback.port),timeout=2)
                sock.sendall(b'GET /auth/callback?state=wrong&code=CODE_CANARY HTTP/1.1\r\nHost: localhost\r\n\r\n')
                response = bytearray()
                while True:
                    block = sock.recv(65536)
                    if not block: break
                    response.extend(block)
                sock.close()
                assert response.startswith(b'HTTP/1.1 400') and b'CODE_CANARY' not in response
                assert b'Authorization state mismatch' in response
            stdout, stderr = proc.communicate(timeout=8)
            assert proc.returncode == (130 if interrupt else 1)
            assert b'CODE_CANARY' not in stdout+stderr and b'Connected to OpenAI' not in stdout
            probe = socket.socket(); probe.settimeout(0.2)
            assert probe.connect_ex(('127.0.0.1',callback.port)) != 0
            probe.close()
        finally:
            if proc.poll() is None: proc.kill()
            proc.wait()
    browser(False); browser(True)
    assert not opened.exists()  # Pipe browser login never opens the desktop.
    master, slave = pty.openpty()
    before = copy.deepcopy(termios.tcgetattr(slave))
    def controlling():
        os.setsid()
        import fcntl
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    proc = subprocess.Popen(command('login','openai','--method','chatgpt-browser'), cwd=project, env=env,
                            stdin=slave,stdout=slave,stderr=slave,preexec_fn=controlling)
    transcript = bytearray(); callback = None
    try:
        deadline = time.monotonic()+8
        while (callback is None or not opened.exists()) and time.monotonic()<deadline:
            assert proc.poll() is None
            if select.select([master],[],[],0.05)[0]: transcript.extend(os.read(master,65536))
            for line in bytes(transcript).decode().splitlines():
                if line.startswith('https://auth.openai.com/oauth/authorize?'):
                    query = urllib.parse.parse_qs(urllib.parse.urlsplit(line).query)
                    callback = urllib.parse.urlsplit(query['redirect_uri'][0])
        assert callback is not None and opened.read_text() == 'opened'
        sock = socket.create_connection(('127.0.0.1',callback.port),timeout=2)
        sock.sendall(b'GET /auth/callback?state=wrong&code=CODE_CANARY HTTP/1.1\r\nHost: localhost\r\n\r\n')
        while sock.recv(65536): pass
        sock.close()
        deadline = time.monotonic()+8
        while proc.poll() is None and time.monotonic()<deadline:
            if select.select([master],[],[],0.05)[0]: transcript.extend(os.read(master,65536))
        assert proc.poll()==1 and b'CANARY' not in transcript
        assert termios.tcgetattr(slave)==before
    finally:
        if proc.poll() is None: proc.kill()
        proc.wait(); os.close(master); os.close(slave)
    with sqlite3.connect('file:'+str(data/'oc.sqlite')+'?mode=ro', uri=True) as db:
        assert db.execute('SELECT COUNT(*) FROM credential_accounts').fetchone()[0] == 1
        for table in ['sessions','turns','tool_operations']:
            assert db.execute('SELECT COUNT(*) FROM '+table).fetchone()[0] == 0
        assert db.execute("SELECT json_extract(tagged_value_json,'$.key') FROM credential_accounts").fetchone()[0] == 'SECOND_KEY_CANARY'
    print('AUTH05 native CLI: pipes/metadata/switch/logout/masked PTY/cancel/owned callback PASS; no real issuer login')
