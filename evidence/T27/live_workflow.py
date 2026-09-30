#!/usr/bin/env python3
"""T27 native workflow; --run is the sole external dispatch opt-in.

Imports the reviewed T55 guard lease and literal-input/artifact/graph readers.
No real campaign creation, product change, raw response capture or secret export.
"""
import argparse
import hashlib
import http.client
import http.server
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'evidence/T55'))
import live_native as prior
from native_completed import Relay, Fixture, bounded, event, terminal, message, group_gone

HEAD = 'e2dbb8b1fcda0bc5d09b6d256ae80517c61ab192'
SHA = 'acdb29ce8b9a5dd42a29eb164b147e24183d0536f2c988d4216c4611fffcaec1'
BENCH = prior.CACHE/'oc-test-bench-20260924'
TOOLS = {'read', 'apply_patch', 'shell', 'compress', 'webfetch', 'codex_web__search'}
BUG = b'pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n'
FIXED = BUG.replace(b'a - b', b'a + b')
TEST_COMMAND = 'cargo test --offline --quiet --jobs 3'
WEB = 'https://example.com'
check, emit = prior.check, prior.emit


def run_owned(args, project, env, campaign_deadline):
    check(time.monotonic() < campaign_deadline, 'campaign_deadline')
    process = subprocess.Popen(args, cwd=project, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    tick = Path(f'/proc/{process.pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
    deadline = min(campaign_deadline, time.monotonic()+300)
    pipes, sizes, timed_out, descendants = [process.stdout, process.stderr], [0, 0], False, {}
    try:
        while pipes:
            if time.monotonic() >= deadline:
                timed_out = True
                break
            # Observe only this launched tree, retaining identity/startticks.
            pending = [process.pid]
            while pending:
                pid = pending.pop()
                try:
                    children = Path(f'/proc/{pid}/task/{pid}/children').read_text().split()
                except OSError:
                    continue
                for raw in children:
                    child = int(raw)
                    try:
                        descendants[child] = Path(f'/proc/{child}/stat').read_text().rsplit(')', 1)[1].split()[19]
                        pending.append(child)
                    except OSError:
                        pass
            for stream in select.select(pipes, [], [], .05)[0]:
                chunk = os.read(stream.fileno(), 65536)
                if not chunk:
                    pipes.remove(stream)
                    continue
                sizes[0 if stream is process.stdout else 1] += len(chunk)
                check(max(sizes) <= 16*1024*1024, 'subprocess_output_cap')
        if timed_out and process.poll() is None:
            process.send_signal(signal.SIGINT)
        process.wait(timeout=10)
        return {'exit': 124 if timed_out else process.returncode, 'pid': process.pid,
                'startticks': tick, 'stdout_bytes': sizes[0], 'stderr_bytes': sizes[1],
                'watchdog': timed_out, 'observed_descendants': len(descendants), 'groups_joined': True}
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
        # Native shell starts its own group; signal only retained owned identities.
        for pid, started in descendants.items():
            try:
                current = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
                if current == started:
                    os.kill(pid, signal.SIGKILL)
            except OSError:
                pass
        group_gone(process.pid)
        process.stdout.close()
        process.stderr.close()


class Observer:
    """Validate native schema/selection; forward unmodified bytes ONLY to Relay."""
    def __init__(self, relay, wire, deadline):
        self.facts, self.errors, self.topologies, self.phase = [], [], [], 'coding'
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                connection, topology = None, None
                phase, title = owner.phase, False
                try:
                    length = int(self.headers['Content-Length'])
                    check(0 < length <= bounded.REQUEST_CAP and self.path == '/provider/responses', 'native_route')
                    body = self.rfile.read(length)
                    request = bounded.exact_json(body)
                    check(request.get('model') == wire and not request.get('reasoning', {}).get('effort'), 'native_selection')
                    check(type(request.get('max_output_tokens')) is int and 0 < request['max_output_tokens'] <= 2048, 'output_cap')
                    tools = {t['name'] for t in request.get('tools', [])}
                    title = not tools
                    if not (tools <= TOOLS and (title or {'read', 'apply_patch', 'shell', 'compress', 'webfetch'} <= tools)):
                        owner.errors.append({'catalog_observed': sorted(name if name in TOOLS or name == 'bash' else 'other' for name in tools)})
                    check(tools <= TOOLS and (title or {'read', 'apply_patch', 'shell', 'compress', 'webfetch'} <= tools), 'native_tool_catalog')
                    graph = prior.pairs(request['input'])
                    text = '\n'.join(part.get('text', '') for item in request['input']
                                     for part in item.get('content', []) if isinstance(part, dict))
                    owner.facts.append({'phase': phase, 'title': title, 'pairs': graph, 'tools': sorted(tools),
                                        'output_cap': request['max_output_tokens'],
                                        'has_summary': '[compressed b' in text,
                                        'input_bytes': len(json.dumps(request['input']).encode())})
                    check(time.monotonic() < deadline, 'campaign_deadline')
                    target = urlsplit(relay.ready['provider_base'])
                    connection = http.client.HTTPConnection(target.hostname, target.port, timeout=min(300, deadline-time.monotonic()))
                    headers = {k: v for k, v in self.headers.items() if k.lower() not in {'host', 'connection', 'content-length'}}
                    connection.request('POST', target.path+'/responses', body, headers)
                    reply = connection.getresponse()
                    self.send_response(reply.status)
                    for key, value in reply.getheaders():
                        if key.lower() not in {'connection', 'transfer-encoding', 'content-length'}:
                            self.send_header(key, value)
                    self.send_header('connection', 'close')
                    self.end_headers()
                    self.close_connection = True
                    total, topology = 0, prior.Topology()
                    while True:
                        chunk = reply.read1(16384)
                        if not chunk:
                            break
                        total += len(chunk)
                        check(total <= bounded.RESPONSE_CAP and time.monotonic() < deadline, 'observer_bound')
                        topology.feed(chunk)
                        self.wfile.write(chunk)
                        self.wfile.flush()
                except (BrokenPipeError, ConnectionResetError):
                    self.close_connection = True
                except Exception as error:
                    owner.errors.append(error.args[0] if isinstance(error, prior.Refused) else 'observer_io')
                    self.close_connection = True
                finally:
                    if topology:
                        owner.topologies.append({'phase': phase, 'title': title, **topology.summary()})
                    if connection:
                        connection.close()
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.base = f'http://127.0.0.1:{self.server.server_port}/provider'

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        check(not self.thread.is_alive(), 'observer_join')


def seed(project):
    files = {
        'Cargo.toml': b'[package]\nname = "t27-fixture"\nversion = "0.1.0"\nedition = "2024"\n\n[workspace]\n',
        'Cargo.lock': b'# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = "t27-fixture"\nversion = "0.1.0"\n',
        'src/lib.rs': BUG,
        'tests/add.rs': b'use t27_fixture::add;\n\n#[test]\nfn public_addition_contract() {\n    for (a,b,c) in [(1,2,3),(-4,7,3),(0,9,9),(8,-2,6)] {\n        assert_eq!(add(a,b), c);\n        assert_eq!(add(b,a), c);\n    }\n}\n',
        'foreign.txt': b'T27 foreign file must remain unchanged.\n',
    }
    for path, data in files.items():
        target = project/path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return files


def audit_files(project, files):
    check((project/'src/lib.rs').read_bytes() == FIXED, 'actual_fixed_library')
    check(all((project/path).read_bytes() == data for path, data in files.items() if path != 'src/lib.rs'), 'foreign_or_api_fixture_changed')
    actual = {str(p.relative_to(project)) for p in project.rglob('*') if p.is_file() and 'target' not in p.relative_to(project).parts}
    check(actual == set(files), 'unexpected_fixture_file')
    return {path: prior.digest(project/path) for path in files}


def snapshots(root):
    with prior.database(root) as db:
        ops = list(db.execute('SELECT id,name,state,input,output FROM tool_operations ORDER BY rowid'))
        rows = list(db.execute('SELECT id,status,result FROM turns ORDER BY rowid'))
        messages = list(db.execute('SELECT * FROM messages ORDER BY rowid'))
        blocks = list(db.execute('SELECT id,summary,start_msg,end_msg FROM compression_blocks ORDER BY rowid'))
        patches = db.execute('SELECT count(*) FROM patch_effects').fetchone()[0]
        events = dict(db.execute('SELECT kind,count(*) FROM events GROUP BY kind'))
        sessions = list(db.execute('SELECT id FROM sessions WHERE parent_id IS NULL'))
        selection = [json.loads(row[0]) for row in db.execute('SELECT model_ref FROM turn_acceptances ORDER BY rowid')]
    return ops, rows, messages, blocks, patches, events, sessions, selection


def graph_check(ops, rows):
    for turn_id, status, raw in rows:
        check(status == 'completed', 'turn_not_completed')
        log = json.loads(raw)
        check(log['spans'][-1].get('finish') == 'stop' and log['spans'][-1]['status'] == 'completed', 'genuine_final_stop')
        graph = prior.pairs(log.get('input', []))
        owned = [op for op in ops if op[0].startswith(turn_id+'-')]
        check(len(owned) == len(graph) and all(op[1] == name and op[0].endswith('-'+call) for op, (name, call) in zip(owned, graph)), 'operation_call_pair')


def audit_fixture(path):
    """Read-only post-run audit of an owned fixture; no Relay/native dispatch."""
    root = Path(path)
    check(root.parent == BENCH and root.name.startswith('t27-native-') and not root.is_symlink(), 'audit_owned_path')
    info = root.stat()
    check(info.st_uid == os.getuid() and info.st_mode & 0o777 == 0o700, 'audit_owned_mode')
    ops, rows, messages, blocks, patches, events, sessions, accepted = snapshots(root)
    graph_check(ops, rows)
    check(patches == 1 and len(blocks) == 1 and len(sessions) == 1, 'audit_effect_counts')
    # Independent public API and test/foreign bytes; no helper creates a patch.
    check((root/'project/src/lib.rs').read_bytes() == FIXED, 'audit_library')
    tool_facts = []
    for _, name, state, raw, output in ops:
        args = json.loads(raw)
        fact = {'name': name, 'state': state, 'output_bytes': len((output or '').encode())}
        if name == 'shell':
            check(args.get('command') == TEST_COMMAND and output.startswith('exit 0\n'), 'audit_shell')
            fact['exit'] = 0
        elif name == 'codex_web__search':
            check(args.get('response_length') == 'short' and state == 'completed' and output, 'audit_search')
            fact['short'] = True
        elif name == 'compress':
            outcome = json.loads(output)
            check(outcome['status'] == 'compressed' and outcome['afterBytes'] < outcome['beforeBytes'], 'audit_compress')
            fact['gain_bytes'] = outcome['beforeBytes']-outcome['afterBytes']
        elif name == 'webfetch':
            fact['source_input_retained'] = bool(args.get('url'))
        tool_facts.append(fact)
    emit({'readonly_fixture_audit': 'PASS', 'fixture_root': str(root), 'turn_ids':[r[0] for r in rows],
          'turns':[r[1] for r in rows], 'operations':tool_facts, 'raw_history_rows':len(messages),
          'patch_effects':patches, 'compression_blocks':len(blocks), 'graph_paired':True,
          'retry_events':events.get('retry_scheduled',0), 'network_dispatches':0})


def qualify(relay, selection, deadline, web=WEB, offline=False):
    _, wire, variant, effort, entry = selection
    check(variant is None and effort is None, 'configured_default_only')
    root = Path(tempfile.mkdtemp(prefix='t27-native-', dir=BENCH))
    home, project = root/'home', root/'project'
    project.mkdir()
    files = seed(project)
    (home/'config/opencode').mkdir(parents=True)
    toolbin = subprocess.check_output(['/home/opencode/.cargo/bin/rustup', 'which', 'cargo'], text=True).strip()
    check(Path(toolbin).is_file() and Path(toolbin).parent.joinpath('rustc').is_file(), 'offline_toolchain_missing')
    env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home/'config'), 'XDG_DATA_HOME': str(home/'data'),
           'XDG_STATE_HOME': str(home/'state'), 'XDG_CACHE_HOME': str(home/'cache'), 'CARGO_HOME': str(home/'cargo'),
           'PATH': str(Path(toolbin).parent)+':/usr/bin:/bin', 'LANG': 'C.UTF-8', 'TERM': 'xterm-256color',
           'TMPDIR': str(root), 'RUST_TEST_THREADS': '1', 'OC_TEST_ALLOW_LOOPBACK': '1'}
    observer = Observer(relay, wire, deadline)
    result = {'fixture_root': str(root), 'wire_id': wire, 'variant': 'Default', 'gates': {}, 'processes': [],
              'proof_mode': 'synthetic-native' if offline else 'real-api-native'}
    try:
        config = {'model': 'live-fixture/'+wire, 'default_agent': 'build',
            'agent': {'build': {'mode': 'primary', 'model': 'live-fixture/'+wire,
                # bash is a policy alias of shell, not a separately exposed tool.
                'tools': {name: name in TOOLS for name in ['read','apply_patch','shell','glob','grep','webfetch','compress','skill','question','subagent','opencode_models','opencode_session_rename','opencode_session_move']}}},
            'compaction': {'auto': False},
            'permission': {'*': 'deny', 'read': 'allow', 'edit': 'allow', 'apply_patch': 'allow',
                'shell': {TEST_COMMAND: 'allow'}, 'compress': 'allow', 'webfetch': {web: 'allow'}, 'codex_web__search': 'allow'},
            'mcp': {'codex_web': {'type': 'remote', 'oauth': False, **relay.ready['mcp']['codex_web']},
                    'chrome-devtools': {'type': 'local', 'command': ['npx','-y','chrome-devtools-mcp@latest'], 'enabled': False}},
            'provider': {'live-fixture': {'npm': '@ai-sdk/openai', 'options': {'baseURL': observer.base, 'apiKey': bounded.PLACEHOLDER}, 'models': {wire: entry}}}}
        (home/'config/opencode/opencode.json').write_text(json.dumps(config))
        baseline = run_owned([toolbin, 'test', '--offline', '--quiet', '--jobs', '3'], project, env, deadline)
        result['seeded_tests'] = baseline
        check(baseline['exit'] == 101, 'seeded_bug_not_red')
        prompt = (f'Fix the seeded Rust addition bug without changing the public API or tests or foreign files. '
            f'FIRST call read on src/lib.rs, and wait for its result (also lets the MCP catalog attach). '
            f'Then author the minimal corrective patch yourself using apply_patch. Immediately after that patch succeeds '
            f'call shell with command exactly "{TEST_COMMAND}". In that same batch AFTER apply_patch, '
            f'also call webfetch once for {web} and the advertised codex_web__search exactly once '
            f'with query "Rust addition unit test documentation" and response_length "short". '
            f'You can place apply_patch FIRST and shell, webfetch, search AFTER it in one ordered tool-call response; '
            f'they execute in that order. Do not test before patching. Once all results arrive finish briefly. '
            f'Do not use compress in this first turn. Do not use any other commands or edit any other files. '
            f'This long completed-task context can later be summarized: '+ 'fixture context '.join(['']*1800))
        result['processes'].append(run_owned([str(prior.ELF), 'run', '--json', prompt], project, env, deadline))
        ops, rows, messages, blocks, patches, events, sessions, accepted = snapshots(root)
        result['initial_states'] = {'turns': [r[1] for r in rows], 'operations': [(o[1],o[2]) for o in ops], 'retry_events': events.get('retry_scheduled', 0)}
        check(result['processes'][-1]['exit'] == 0 and len(rows) == 1, 'coding_process')
        graph_check(ops, rows)
        required = {'read','apply_patch','shell','webfetch','codex_web__search'}
        check({o[1] for o in ops} == required and len(ops) == 5 and
              all(o[2] == ('failed' if offline and o[1] == 'webfetch' else 'completed') for o in ops), 'coding_and_web_tool_states')
        check(ops[0][1] == 'read' and [o[1] for o in ops].index('apply_patch') < [o[1] for o in ops].index('shell'), 'native_read_patch_test_order')
        check(patches == 1, 'one_patch_effect')
        for _, name, _, raw, output in ops:
            args = json.loads(raw)
            if offline and name == 'webfetch':
                check(output == 'error: private host refused', 'offline_web_static_refusal')
                continue
            check(not (output or '').startswith('error:'), 'tool_error_output')
            if name == 'read':
                check(args.get('path') == 'src/lib.rs' and 'a - b' in output, 'native_read_bug')
            elif name == 'shell':
                check(args.get('command') == TEST_COMMAND and output.startswith('exit 0\n'), 'native_test_success')
            elif name == 'webfetch':
                check(args.get('url') == web and bool(output), 'web_source_result')
            elif name == 'codex_web__search':
                check(args.get('response_length') == 'short' and bool(output), 'short_search_result')
        result['file_hashes'] = audit_files(project, files)
        result['independent_tests'] = run_owned([toolbin, 'test', '--offline', '--quiet', '--jobs', '3'], project, env, deadline)
        check(result['independent_tests']['exit'] == 0, 'independent_tests')
        result['gates'].update(coding='PASS', webfetch='PASS', MCP06='PASS', E2E04='PASS')
        if offline:
            result['gates'].update(webfetch='NOT_RUN', E2E04='NOT_RUN', offline_web_refusal='PASS')
        original_ops, original_rows, original_messages = ops, rows, messages
        check(len(sessions) == 1 and all(a['id'] == wire and not a.get('variant') for a in accepted), 'native_selection_session')
        session = sessions[0][0]
        result['session_id'] = session
        observer.phase = 'compress-reopen'
        remaining = 24-relay.snapshot()['counts']['generation']
        if remaining < 2:
            result['gates'].update(E2E02='BLOCKED', reopen_next_command='BLOCKED', compress='BLOCKED', post_compress_restart='BLOCKED')
            result['code'] = 'remaining_generation_less_than_two'
            return result
        followup = (f'The earlier addition fix is complete. This is a new instruction in the same reopened session. '
            f'Use compress on the first closed user anchor in the DCP anchors, with startId=endId equal to that '
            f'exact existing anchor ID. Summarize the task and completed read, authored patch, passing tests, webfetch '
            f'and short search in a compact truthful summary; discard repetitive context. '
            f'In the SAME tool-call response, after compress call shell with command exactly "{TEST_COMMAND}" '
            f'to verify the retained corrected addition fact. Do not replay apply_patch, read, webfetch or search. '
            f'After those two results arrive, finish with a brief final answer; no further tools.')
        result['processes'].append(run_owned([str(prior.ELF), 'run','--json','--session',session,followup], project, env, deadline))
        ops, rows, messages, blocks, patches, events, _, accepted = snapshots(root)
        result['reopen_states'] = {'turns': [r[1] for r in rows], 'operations': [(o[1],o[2]) for o in ops], 'compression_blocks': len(blocks)}
        check(result['processes'][-1]['exit'] == 0 and len(rows) == 2, 'reopen_process')
        graph_check(ops, rows)
        check(ops[:len(original_ops)] == original_ops and rows[:1] == original_rows and messages[:len(original_messages)] == original_messages, 'settled_history_modified_or_replayed')
        new = ops[len(original_ops):]
        check([(o[1],o[2]) for o in new] == [('compress','completed'),('shell','completed')], 'compress_next_command')
        outcome = json.loads(new[0][4])
        check(outcome['status'] == 'compressed' and outcome['afterBytes'] < outcome['beforeBytes'] and blocks, 'actual_compression_gain')
        check(json.loads(new[1][3]).get('command') == TEST_COMMAND and new[1][4].startswith('exit 0\n'), 'next_command_success')
        check(any(f['phase'] == 'compress-reopen' and not f['title'] and f['has_summary'] for f in observer.facts), 'actual_compressed_outbound_context')
        check(patches == 1, 'patch_replay')
        audit_files(project, files)
        result.update(compression={k: outcome[k] for k in ['status','beforeBytes','afterBytes','savedTokens']},
                      settled_rows_unchanged=True, operation_graph_paired=True, patch_effects=patches,
                      tool_call_ids=[call for _, call in prior.pairs(json.loads(rows[0][2])['input'])])
        result['gates'].update(E2E02='PASS', reopen_next_command='PASS', compress='PASS', post_compress_restart='BLOCKED')
        if 24-relay.snapshot()['counts']['generation'] >= 1:
            saved_ops, saved_rows, saved_messages = ops, rows, messages
            observer.phase = 'post-compress-restart'
            result['processes'].append(run_owned([str(prior.ELF),'run','--json','--session',session,
                'New process after compression. No tools: based on retained history briefly acknowledge the completed addition fix and passing tests.'], project, env, deadline))
            ops, rows, messages, blocks, patches, events, _, accepted = snapshots(root)
            check(result['processes'][-1]['exit'] == 0 and len(rows) == 3, 'post_compress_process')
            graph_check(ops, rows)
            check(ops == saved_ops and rows[:2] == saved_rows and messages[:len(saved_messages)] == saved_messages, 'post_compress_no_replay')
            check(any(f['phase'] == 'post-compress-restart' and f['has_summary'] and not f['title'] for f in observer.facts), 'post_compress_actual_projection')
            result['gates']['post_compress_restart'] = 'PASS'
        else:
            result['code'] = 'physical_generation_allowance_exhausted_after_compression'
        result['gates']['T27'] = 'PASS' if not offline and result['gates']['post_compress_restart'] == 'PASS' else 'BLOCKED'
        check(not observer.errors, 'observer_errors')
        result['final_turns'] = [r[1] for r in rows]
        result['final_operations'] = [(o[1],o[2]) for o in ops]
        result['retry_events'] = events.get('retry_scheduled', 0)
        return result
    except Exception as error:
        result.update(status='FAIL', code=error.args[0] if isinstance(error, prior.Refused) else 'operator_fact_or_io')
        return result
    finally:
        observer.close()
        if offline:
            # The shared verifier's assertions qualify the harness, not real-API IDs.
            result['offline_checks'] = dict(result['gates'])
            for gate in ('E2E02', 'E2E04', 'MCP06'):
                result['gates'][gate] = 'NOT_RUN'
        result['native_posts'] = [{k: v for k,v in f.items() if k != 'pairs'} | {'paired_calls': len(f['pairs'])} for f in observer.facts]
        result['observer_errors'] = observer.errors
        result['stream_topologies'] = observer.topologies
        result['observer_joined'] = True
        result['fixture_bytes'] = sum(p.stat().st_size for p in root.rglob('*') if p.is_file())
        emit(result)


class OfflineMcpWeb:
    def __init__(self):
        self.methods, self.searches, self.webs = [], 0, 0
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                if self.path == '/mcp':
                    self.send_response(405)
                    self.send_header('content-length','0')
                    self.end_headers()
                    return
                owner.webs += 1
                body = b'<html><body>Offline public web source fixture.</body></html>'
                self.send_response(200)
                self.send_header('content-type','text/html')
                self.send_header('content-length',str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                rpc = body['method']
                owner.methods.append(rpc)
                if rpc == 'initialize':
                    result = {'protocolVersion':'2025-11-25','capabilities':{'tools':{}},'serverInfo':{'name':'offline-codex','version':'1'}}
                elif rpc == 'tools/list':
                    result = {'tools':[{'name':'search','description':'Offline short source search','inputSchema':{'type':'object','properties':{'query':{'type':'string'},'response_length':{'type':'string','enum':['short']}},'required':['query','response_length']}}]}
                elif rpc == 'tools/call':
                    check(body['params']['name'] == 'search' and body['params']['arguments']['response_length'] == 'short', 'offline_short_search')
                    owner.searches += 1
                    result = {'content':[{'type':'text','text':'Offline source https://example.com Rust addition documentation'}], 'isError':False}
                elif rpc.startswith('notifications/'):
                    self.send_response(202)
                    self.send_header('content-length','0')
                    self.end_headers()
                    return
                else:
                    result = {}
                data = json.dumps({'jsonrpc':'2.0','id':body['id'],'result':result}).encode()
                self.send_response(200)
                self.send_header('content-type','application/json')
                self.send_header('content-length',str(len(data)))
                self.end_headers()
                self.wfile.write(data)
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever)
        self.thread.start()
        self.base = f'http://127.0.0.1:{self.server.server_port}'
    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        check(not self.thread.is_alive(), 'offline_server_join')


def offline(deadline):
    fixture, remote = Fixture(prior.ELF, 'headless'), OfflineMcpWeb()
    def calls(entries):
        return b''.join(event({'type':'response.output_item.done','output_index':index,'item':{'type':'function_call','id':'fc_'+cid,'call_id':cid,'name':name,'arguments':json.dumps(args),'status':'completed'}}) for index,(cid,name,args) in enumerate(entries))+terminal([])
    def reply(number, request):
        if number == 1:
            # Fake only: let the independent native startup/catalog complete
            # before closing this deliberately fast first synthetic generation.
            wait = time.monotonic()+5
            while 'tools/list' not in remote.methods and time.monotonic() < wait:
                time.sleep(.01)
            body = calls([('read','read',{'path':'src/lib.rs'})])
        elif number == 2:
            check('codex_web__search' in {t['name'] for t in request['tools']}, 'offline_actual_mcp_catalog')
            body = calls([('patch','apply_patch',{'patchText':'*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-    a - b\n+    a + b\n*** End Patch'}),
                ('test','shell',{'command':TEST_COMMAND}),('web','webfetch',{'url':remote.base+'/page'}),
                ('search','codex_web__search',{'query':'Rust addition documentation','response_length':'short'})])
        elif number == 4:
            anchor = next(part['text'] for item in request['input'] for part in item.get('content',[]) if part.get('text','').startswith('DCP context anchors in order.'))
            anchors = json.loads(anchor.split(': ',1)[1])
            first = next(a['id'] for a in anchors if a['closed'] and a['role'] == 'user')
            body = calls([('compress','compress',{'topic':'Addition task','content':[{'startId':first,'endId':first,'summary':'Completed addition task: read src/lib.rs, fix subtraction via patch, cargo tests pass; private-host webfetch refused offline, short source search succeeded.'}]}),('next-test','shell',{'command':TEST_COMMAND})])
        else:
            body = terminal([message('Offline genuine completion')])
        return 200, body, False
    fixture.response = reply
    fixture.campaign = bounded.Ledger.create(str(fixture.root/'offline-workflow'))
    base = f'http://127.0.0.1:{fixture.server.server_port}'
    manifest = {'provider': {'generation_url':base+'/v1/responses','discovery_url':base+'/v1/models','headers':{'authorization':'Bearer synthetic-canary'}},
                'mcp': {'codex_web':{'url':remote.base+'/mcp','headers':{'authorization':'Bearer synthetic-mcp-canary'}}}}
    fixture.relay = Relay(fixture.campaign, manifest)
    try:
        result = qualify(fixture.relay, ('offline','fixture-model',None,None,{'limit':{'context':65536,'output':2048}}), deadline, remote.base+'/page', True)
        check(all(result.get('offline_checks',{}).get(g) == 'PASS' for g in ['E2E02','compress','post_compress_restart','offline_web_refusal']), 'offline_workflow_failed')
        check(remote.searches == 1 and remote.webs == 0 and not fixture.errors, 'offline_external_effect_count')
        emit({'offline':'PASS','counts':fixture.relay.snapshot()['counts'],'short_searches':remote.searches,'webfetches':remote.webs,
              'web_private_refusal':True,'full_restart_proven':True,'positive_webfetch_live_only':True})
    finally:
        emit({'offline_diagnostics': {'rpc_methods': remote.methods, 'fixture_errors':len(fixture.errors),
              'receipts':[{k:a[k] for k in ['kind','target','outcome','http_status','failure','failure_stage'] if k in a} for a in fixture.relay.snapshot()['attempts']]}})
        fixture.close()
        remote.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--run',action='store_true')
    mode.add_argument('--offline',action='store_true')
    mode.add_argument('--audit-fixture', metavar='OWNED_CACHE_PATH')
    args = parser.parse_args()
    deadline = time.monotonic()+900
    campaign, ledger, prefix, before = prior.preflight(HEAD,SHA)
    if args.audit_fixture:
        audit_fixture(args.audit_fixture)
        check(ledger.snapshot() == before, 'audit_changed_live_ledger')
        emit({'reserved_or_uncertain_remain_spent': [a['reserve'] for a in before['attempts']
              if a['outcome'] in ('reserved','uncertain')], 'network_dispatches':0})
        return 0
    if args.offline:
        offline(deadline)
        check(ledger.snapshot() == before, 'offline_changed_live_ledger')
        emit({'live_ledger_unchanged':True})
        return 0
    if not args.run:
        return 0
    check(before['counts']['generation'] == 18 and before['counts']['control'] == 8 and before['counts']['mcp'] == 0, 'frozen_campaign_counts_changed')
    configured, manifest = prior.product_manifest()
    source = manifest['provider']['generation_url']
    check(source.endswith('/responses'), 'approved_codex_source_association')
    manifest['mcp'] = {'codex_web': {'url':source[:-len('/responses')]+'/mcp', 'headers':dict(manifest['provider']['headers'])}}
    relay = Relay(campaign,manifest,offline=False)
    tick = Path(f'/proc/{relay.process.pid}/stat').read_text().rsplit(')',1)[1].split()[19]
    result = {}
    try:
        selection = next(s for s in prior.catalog(relay,configured) if s[0] == 'configured')
        result = qualify(relay,selection,deadline)
    finally:
        relay.close()
        after = ledger.snapshot()
        with ledger.locked() as (fd,_):
            os.lseek(fd,0,os.SEEK_SET)
            check(os.read(fd,len(prefix)) == prefix, 'prior_journal_changed')
        check(prior.digest(prior.ELF) == SHA and subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip() == HEAD, 'artifact_changed_after_run')
        emit({'after_counts':after['counts'],'input_bytes':after['input_bytes'],'remaining_generation':24-after['counts']['generation'],
              'prior_journal_bytes_unchanged':len(prefix),'binary_hash_unchanged':True,'relay_joined':True,
              'relay_pid':relay.process.pid,'relay_startticks':tick,
              'new_receipts':[{k:a[k] for k in ['reserve','kind','target','outcome','http_status','failure','failure_stage'] if k in a} for a in after['attempts'][len(before['attempts']):]]})
    return 0 if result.get('gates',{}).get('T27') == 'PASS' else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as error:
        emit({'status':'STOP','code':error.args[0] if isinstance(error,prior.Refused) else 'operator_preflight_or_io','unknown_remains_consumed':True})
        raise SystemExit(2)
