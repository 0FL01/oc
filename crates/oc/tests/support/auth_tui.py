"""Production ELF auth TUI; active OAuth URL stays in memory, no real authorization."""
import base64, copy, errno, fcntl, json, os, pathlib, pty, re, select, socket, sqlite3, struct, subprocess, sys, tempfile, termios, time, unicodedata, urllib.parse

BINARY = str(pathlib.Path(sys.argv[1]).resolve())


def wait(predicate, label):
    deadline = time.monotonic()+8
    while time.monotonic() < deadline:
        if predicate(): return
        time.sleep(.02)
    raise AssertionError(label)  # Never publish active URL/code or a private screen.


def rows(stream, cols=120, height=40):
    """Only reconstruct fixture host cells across sparse Ratatui repaints."""
    grid = [[' ']*cols for _ in range(height)]
    x = y = 0
    for match in re.finditer(r'\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)|.', stream, re.S):
        char = match.group()
        if char.startswith('\x1b['):
            code, args = char[-1], char[2:-1].lstrip('?').split(';')
            number = int(args[0]) if args[0].isdigit() else 1
            if code in 'Hf':
                x = (int(args[1]) if len(args)>1 and args[1].isdigit() else 1)-1; y = number-1
            elif code == 'G': x = number-1
            elif code == 'A': y -= number
            elif code == 'B': y += number
            elif code == 'C': x += number
            elif code == 'D': x -= number
            elif code == 'J' and number in (1,2): grid = [[' ']*cols for _ in range(height)]
            elif code == 'K' and 0 <= y < height:
                start, end = (0,x+1) if number==1 else (x,cols)
                grid[y][max(0,start):min(cols,end)] = [' ']*max(0,min(cols,end)-max(0,start))
        elif char.startswith('\x1b'): continue
        elif char == '\r': x = 0
        elif char == '\n': y += 1
        elif char >= ' ' and 0 <= y < height and 0 <= x < cols:
            if unicodedata.combining(char): continue
            width = 2 if unicodedata.east_asian_width(char) in 'WF' else 1
            grid[y][x] = char
            if width==2 and x+1<cols: grid[y][x+1] = ' '
            x += width
    return [''.join(row) for row in grid]


class View:
    def __init__(self, root, env):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH',40,120,0,0))
        self.original = copy.deepcopy(termios.tcgetattr(self.slave))
        def controlling():
            os.setsid(); fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        self.proc = subprocess.Popen([BINARY,'--data-dir',str(root/'data'),'tui'], cwd=root/'project', env=env,
                                     stdin=self.slave,stdout=self.slave,stderr=self.slave,preexec_fn=controlling)
        self.output = b''
        try: self.see('ctrl+p commands')
        except BaseException:
            self.close(); raise

    def drain(self, seconds=.06):
        deadline = time.monotonic()+seconds
        while time.monotonic()<deadline:
            if select.select([self.master],[],[],.01)[0]:
                try: self.output += os.read(self.master,65536)
                except OSError as error:
                    if error.errno==errno.EIO: break
                    raise
        assert len(self.output)<8*1024*1024, 'bounded fixture output'
        assert b'KEY_CANARY' not in self.output and b'CODE_CANARY' not in self.output, 'private input leaked'

    def screen(self):
        self.drain(); return rows(self.output.decode('utf-8',errors='replace'))

    def see(self, text):
        wait(lambda: any(text in row for row in self.screen()), 'missing public UI state: '+text)

    def send(self, value):
        os.write(self.master,value); self.drain()

    def paste(self, value):
        self.send(b'\x1b[200~'+value.encode()+b'\x1b[201~')

    def command(self, value):
        self.send(b'\x10')
        try: self.see('Commands')
        except AssertionError:
            grid = self.screen()
            raise AssertionError(('command surface', {text:any(text in row for row in grid) for text in
                ('Connect / accounts','Commands','draft-kept-界','second-draft-界','Press esc again','Quit','Models')},self.proc.poll())) from None
        self.paste(value); self.send(b'\r')

    def click(self, text):
        grid = self.screen()
        for y,row in enumerate(grid):
            if text in row:
                x = row.index(text)+1
                self.send(f'\x1b[<0;{x};{y+1}M\x1b[<0;{x};{y+1}m'.encode()); return
        raise AssertionError('missing public mouse row: '+text)

    def connect(self):
        self.command('connect'); self.see('Choose provider connection')
        self.click('OpenAI (openai)'); self.see('Provider: openai')

    def methods(self):
        self.connect()
        if any('Add account' in row for row in self.screen()): self.send(b'\r')
        self.see('Choose authentication method')
        self.see('ChatGPT Pro/Plus (browser)'); self.see('ChatGPT Pro/Plus (headless)'); self.see('API key')

    def browser(self):
        mark = len(self.output)
        self.methods(); self.send(b'\r'); self.see('Waiting for authorization')
        self.see('o open'); wait(lambda: bool(owned_ports(self.proc.pid)), 'owned browser listener')
        assert b'\x1b]52;' not in self.output[mark:], 'auth clipboard must be explicit'

    def quit(self):
        # One Esc unmounts auth; idle Home's following Esc is native quit.
        if any('Connect / accounts' in row for row in self.screen()):
            self.send(b'\x1b')
            wait(lambda: not any('Connect / accounts' in row for row in self.screen()), 'auth surface unmounted')
        self.send(b'\x1b')
        def exited():
            self.drain(); return self.proc.poll() is not None
        wait(exited, 'native joined quit')
        self.drain(); assert self.proc.returncode==0, 'native clean exit'
        assert termios.tcgetattr(self.slave)==self.original, 'termios restoration'
        assert b'\x1b[?1049l' in self.output, 'alternate screen restoration'
        self.close()

    def close(self):
        if self.proc.poll() is None: self.proc.kill(); self.proc.wait(timeout=5)
        os.close(self.master); os.close(self.slave)


def owned_ports(pid):
    """Inspect only this fixture oc's socket identities; never probe foreign listeners."""
    inodes = set()
    try:
        for fd in pathlib.Path(f'/proc/{pid}/fd').iterdir():
            try: target = os.readlink(fd)
            except FileNotFoundError: continue
            if target.startswith('socket:['): inodes.add(target[8:-1])
    except FileNotFoundError: return []
    result = []
    for row in pathlib.Path('/proc/net/tcp').read_text().splitlines()[1:]:
        fields = row.split()
        if fields[3]=='0A' and fields[9] in inodes:
            address,port = fields[1].split(':')
            port = int(port,16)
            if address=='0100007F' and port in (1455,1457): result.append(port)
    return result


with tempfile.TemporaryDirectory(prefix='t57-auth-tui-',dir=os.environ['TMPDIR']) as temp:
    root = pathlib.Path(temp)
    for name in ('project','home','bin'): (root/name).mkdir()
    config = root/'home/config/opencode'; config.mkdir(parents=True)
    opened = root/'browser-opened'
    (root/'bin/xdg-open').write_text('#!/usr/bin/python3\nimport os,pathlib,sys\n'
        'assert not any(k in os.environ for k in ["OPENAI_API_KEY","OC_API_KEY"])\n'
        'p=pathlib.Path('+repr(str(opened))+')\n'
        'if p.exists(): sys.exit(7)\np.write_text("opened")\n')
    (root/'bin/xdg-open').chmod(0o700)
    env = {'HOME':str(root/'home'),'XDG_CONFIG_HOME':str(root/'home/config'),'XDG_DATA_HOME':str(root/'home/data'),
           'XDG_STATE_HOME':str(root/'home/state'),'PATH':str(root/'bin')+':/usr/bin:/bin','TERM':'xterm-256color',
           'LANG':'C.UTF-8','SHELL':'/bin/bash','OPENAI_API_KEY':'PARENT_KEY_CANARY','OC_API_KEY':'PARENT_OC_CANARY'}
    with socket.socket() as guard:
        guard.bind(('127.0.0.1',0)); guard.listen(); guard.setblocking(False)
        fixture = {'model':'fixture/gpt-5.5','disabled_providers':['opencode-go'],'provider':{'fixture':{
            'npm':'@ai-sdk/openai','name':'Collision fixture','options':{'baseURL':f'http://127.0.0.1:{guard.getsockname()[1]}/v1','apiKey':'FIXTURE_KEY_CANARY'},
            'models':{'gpt-5.5':{'name':'Collision fixture','limit':{'context':10000,'output':1000}}}}}}
        (config/'opencode.json').write_text(json.dumps(fixture))
        result = subprocess.run([BINARY,'--data-dir',str(root/'data'),'auth','list','--format','json'],cwd=root/'project',env=env,
                                stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=8)
        assert result.returncode==0 and json.loads(result.stdout)==[], 'native storage initialization'
        source = {'source':'https://models.dev/api.json','fetched_at_ms':int(time.time()*1000),'record':None,'openai':{
            'id':'openai','npm':'@ai-sdk/openai','models':{id:{'id':id,'name':name,'limit':{'context':32000,'output':1024},'tool_call':True}
            for id,name in [('gpt-5.5','OpenAI collision'),('gpt-4.1','OpenAI ordinary')]}}}
        with sqlite3.connect(root/'data/oc.sqlite') as db:
            db.execute('INSERT INTO prefs(key,value,updated_at) VALUES (?,?,?)',
                       ('public-catalog:https://models.dev/api.json:opencode-go:v1',json.dumps(source),'1'))

        def accounts():
            with sqlite3.connect('file:'+str(root/'data/oc.sqlite')+'?mode=ro',uri=True) as db:
                return [(id,label,active) for id,label,active in db.execute('SELECT id,label,active FROM credential_accounts ORDER BY label,id')]

        def selections():
            with sqlite3.connect('file:'+str(root/'data/oc.sqlite')+'?mode=ro',uri=True) as db:
                return dict(db.execute("SELECT key,value FROM prefs WHERE key LIKE 'tui.selection.%' AND key NOT LIKE 'tui.selection.draft:%'"))

        view = View(root,env)
        try:
            view.paste('draft-kept-界'); before_selection = selections()
            for label,key in [('Zulu','FIRST_KEY_CANARY'),('Alpha','SECOND_KEY_CANARY')]:
                view.methods(); view.send(b'jj\r'); view.see('Account label'); view.paste(label); view.send(b'\r')
                view.see('API key (masked)'); view.paste(key); view.see('•••••'); view.send(b'\r')
                view.see('OpenAI collision'); view.see('OpenAI ordinary')
                wait(lambda: len(accounts())==(1 if label=='Zulu' else 2), 'durable key acknowledgement')
                assert selections()==before_selection, 'account acknowledgement cannot commit model or retarget collision'
                view.send(b'\x1b'); view.see('draft-kept-界')
            view.connect(); view.see('Add account'); view.see('Alpha [active]'); view.see('Zulu')
            view.send(b'jj\r'); wait(lambda: accounts()==[(accounts()[0][0],'Alpha',0),(accounts()[1][0],'Zulu',1)], 'explicit activation')
            view.send(b'r'); view.see('Rename account')
            view.send(b'\x7f'*4); view.paste('Beta'); view.send(b'\r'); view.see('Beta [active]')
            view.send(b'd'); view.see('Press d again to confirm removal'); assert len(accounts())==2
            view.send(b'd'); wait(lambda: len(accounts())==1, 'confirmed removal')
            view.see('Alpha [active]'); view.send(b'\x1b'); view.see('draft-kept-界')
            view.quit(); view = None
        finally:
            if view is not None: view.close()

        view = View(root,env)
        try:
            view.paste('second-draft-界'); view.see('second-draft-界')
            view.connect(); view.see('Alpha [active]'); view.send(b'\r'); view.see('Choose authentication method')
            view.send(b'\x1b'); view.browser()
            assert not opened.exists(), 'TUI must not automatically launch a browser'
            view.send(b'o'); wait(opened.exists,'explicit sanitized browser action')
            mark = len(view.output); view.send(b'c')
            wait(lambda: bool(re.findall(rb'\x1b\]52;c;([^\x07]+)\x07',view.output[mark:])), 'explicit auth copy')
            copied = re.findall(rb'\x1b\]52;c;([^\x07]+)\x07',view.output[mark:])[-1]
            url = urllib.parse.urlsplit(base64.b64decode(copied).decode())
            query = urllib.parse.parse_qs(url.query)
            assert url.scheme=='https' and url.hostname=='auth.openai.com' and url.path=='/oauth/authorize', 'typed authorization authority'
            assert query['code_challenge_method']==['S256'] and len(query['state'][0])==43, 'native PKCE presentation'
            port = urllib.parse.urlsplit(query['redirect_uri'][0]).port
            assert port in owned_ports(view.proc.pid), 'copy targets only the owned listener'
            view.send(b'o'); view.see('Could not open the browser'); view.see('Waiting for authorization')
            assert port in owned_ports(view.proc.pid), 'manual-copy opening failure keeps the same attempt'
            view.send(b'\x1b'); wait(lambda: not owned_ports(view.proc.pid), 'owned Esc cleanup')
            assert len(accounts())==1, 'cancel is not account success'
            view.see('second-draft-界')
            # The same native listener can be mounted again; wrong state fails before issuer exchange.
            view.browser(); port = owned_ports(view.proc.pid)[0]
            with socket.create_connection(('127.0.0.1',port),timeout=2) as callback:
                callback.sendall(b'GET /auth/callback?state=wrong&code=CODE_CANARY HTTP/1.1\r\nHost: localhost\r\n\r\n')
                response = b''
                while True:
                    block = callback.recv(65536)
                    if not block: break
                    response += block
            assert response.startswith(b'HTTP/1.1 400') and b'CODE_CANARY' not in response, 'safe callback refusal'
            view.see('Authorization state mismatch'); wait(lambda: not owned_ports(view.proc.pid), 'failed attempt cleanup')
            assert len(accounts())==1, 'callback refusal cannot fabricate an account'
            view.browser(); assert owned_ports(view.proc.pid)
            view.quit(); view = None
        finally:
            if view is not None: view.close()
        assert selections()==before_selection, 'auth-only navigation cannot select a model'
        result = subprocess.run([BINARY,'--data-dir',str(root/'data'),'auth','list','--format','json'],cwd=root/'project',env=env,
                                stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=8)
        assert result.returncode==0 and [r['label'] for r in json.loads(result.stdout)]==['Alpha'], 'shared CLI restart metadata'
        with sqlite3.connect(root/'data/oc.sqlite') as db:
            for table in ('sessions','turns','tool_operations'):
                assert db.execute('SELECT COUNT(*) FROM '+table).fetchone()[0]==0, 'auth must not accept history or model work'
            assert db.execute("SELECT json_extract(tagged_value_json,'$.key') FROM credential_accounts").fetchone()[0]=='SECOND_KEY_CANARY'
        try: accepted,_ = guard.accept()
        except BlockingIOError: pass
        else:
            accepted.close(); raise AssertionError('unexpected generation traffic')
    print('AUTH05 native TUI: methods/masked keys/accounts/filter-no-commit/restart/explicit open-copy/owned cancel-failure-quit PASS; no real issuer login')
