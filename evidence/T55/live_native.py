#!/usr/bin/env python3
"""T55 PROV10 operator: explicit existing campaign; no initialization or fallback.

Only --run authorizes catalog/native external dispatch. Private native histories
stay in the owned cache; this program prints structural aggregates only.
"""
import hashlib
import http.client
import http.server
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
CACHE = Path('/home/opencode/.cache/opencode-tmp/opencode')
sys.path.insert(0, str(ROOT/'evidence/T55'))
from native_completed import Relay, Fixture, bounded, group_gone, event, terminal, message

ELF = ROOT/'target/release/oc'
ELF_SHA = '7044b05c932ff7b96139024d8898335b12f1df2f414f2245eb4c87617c57dd1e'
HEAD = '19b224ef065496eb7de9e4c8ef8384bf46a84e3f'
EXPECTED = b'T55 native live tool roundtrip\n'
PROMPT = ('Perform this small file task using tools, not a description: first use apply_patch '
          'to create ONLY proof.txt containing exactly one line: T55 native live tool roundtrip '
          '(with a final newline). After apply_patch succeeds, call read with path proof.txt. '
          'After reading it, finish with a short final answer. Do not use any other tool or file.')
FOLLOWUP = ('This is a new instruction after reopening the same session. The earlier proof.txt '
            'task is finished: do not repeat apply_patch or any tool. Based on the retained '
            'history, give a short final answer acknowledging the completed file task. '
            'Do not modify or read any file.')
EXPLICIT_PROMPT = ('Execute exactly this patch using apply_patch now, without preliminary commentary:\n'
                   '*** Begin Patch\n*** Add File: proof.txt\n+T55 native live tool roundtrip\n*** End Patch\n'
                   'Wait for its result. Then read proof.txt using read. After that result finish with '
                   'a short final answer. Only these two tool calls are needed; do not use any other file.')


class Refused(Exception):
    """Only fixed operator codes; never interpolate remote/private values."""


def check(condition, code):
    if not condition:
        raise Refused(code)


def emit(value):
    print(json.dumps(value, sort_keys=True), flush=True)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def identity():
    path = CACHE/'responses-diagnose-20260930.campaign-id'
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        check(info.st_uid == os.getuid() and info.st_nlink == 1 and info.st_size <= 4096,
              'identity_file_ownership')
        campaign = os.read(fd, 4097).decode().strip()
    finally:
        os.close(fd)
    ledger = bounded.Ledger(campaign)
    check(ledger.identity['root'] == str(CACHE/'responses-diagnose-20260930-campaign')
          and ledger.identity['id'] == '39d44cb58b834de99544daf3c2eedab1', 'existing_identity')
    with ledger.locked() as (fd, _):
        os.lseek(fd, 0, os.SEEK_SET)
        prefix = os.read(fd, bounded.JOURNAL_CAP)
    return campaign, ledger, prefix


def preflight():
    check(os.getuid() != 0, 'non_root')
    check(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == HEAD,
          'source_head_changed')
    check(digest(ELF) == ELF_SHA and ELF.open('rb').read(4) == b'\x7fELF', 'retained_elf')
    campaign, ledger, prefix = identity()
    state = ledger.snapshot()
    emit({'preflight': 'PASS', 'source_head': HEAD, 'release_sha256': ELF_SHA,
          'campaign_id': state['id'], 'before_counts': state['counts'],
          'input_bytes': state['input_bytes'], 'remaining_generation': 24-state['counts']['generation'],
          'journal_prefix_bytes': len(prefix), 'network_requests': 0})
    return campaign, ledger, prefix, state


def product_manifest():
    spec = importlib.util.spec_from_file_location('literal_input', CACHE/'T46-r4-input.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.ALLOWED = {'LUDKA2_API_URL', 'LUDKA2_API_KEY', 'OC_TEST_MODEL', 'OC_TEST_VARIANT'}
    values = module.product_input()
    check(all(values.get(k) for k in ('LUDKA2_API_URL', 'LUDKA2_API_KEY', 'OC_TEST_MODEL')),
          'approved_inputs_missing')
    base = values['LUDKA2_API_URL'].rstrip('/')
    parsed = urlsplit(base)
    check(parsed.scheme == 'https' and parsed.hostname and not parsed.username
          and not parsed.password and not parsed.query and not parsed.fragment, 'approved_base_shape')
    provider = {'generation_url': base+'/responses', 'discovery_url': base+'/models',
                'headers': {'authorization': 'Bearer '+values['LUDKA2_API_KEY']}}
    # Existing guard manifest requires an MCP entry. This explicitly unused,
    # private negative route cannot dial in live mode; no MCP is exposed to oc.
    manifest = {'provider': provider, 'mcp': {'unused': {'url': 'https://127.0.0.1:9/unused', 'headers': {}}}}
    return values['OC_TEST_MODEL'], manifest


def catalog(relay, configured):
    address = urlsplit(relay.ready['provider_base'])
    connection = http.client.HTTPConnection(address.hostname, address.port, timeout=310)
    try:
        connection.request('GET', address.path+'/models', headers=relay.ready['provider_headers'])
        reply = connection.getresponse()
        body = reply.read(bounded.RESPONSE_CAP+1)
        check(reply.status == 200 and len(body) <= bounded.RESPONSE_CAP, 'catalog_http_or_limit')
        value = bounded.exact_json(body)
    finally:
        connection.close()
    rows = value.get('data') if isinstance(value, dict) else None
    check(isinstance(rows, list), 'catalog_data_shape')
    selections = []
    for label, wire, variant in [('incident', 'cx/gpt-6-luna', 'high'), ('configured', configured, None)]:
        matches = [row for row in rows if isinstance(row, dict) and row.get('id') == wire]
        check(len(matches) == 1, 'exact_catalog_binding_missing_or_duplicate')
        row = matches[0]
        metadata = row.get('opencode', {})
        check(isinstance(metadata, dict) and metadata.get('tool_call') is not False, 'catalog_tool_capability')
        limits = {'context': row.get('context_length'), 'output': row.get('max_completion_tokens')}
        limits.update(metadata.get('limit', {}))
        check(all(type(limits.get(k)) is int and limits[k] > 0 for k in ('context', 'output')),
              'catalog_limits_missing')
        entry = {k: metadata[k] for k in ('tool_call', 'reasoning', 'modalities', 'attachment') if k in metadata}
        entry['limit'] = {k: v for k, v in limits.items() if type(v) is int and v > 0}
        entry['limit']['output'] = min(limits['output'], 2048)
        effort = None
        if variant:
            variants = metadata.get('variants')
            check(isinstance(variants, dict) and isinstance(variants.get(variant), dict), 'catalog_high_shape')
            high = variants[variant]
            check(high.get('disabled') is not True and high.get('reasoningEffort') == 'high', 'catalog_high_unavailable')
            effort = high['reasoningEffort']
            entry['variants'] = {variant: {'reasoningEffort': effort}}
        selections.append((label, wire, variant, effort, entry))
    emit({'catalog': 'PASS', 'http_status': 200, 'data_array': True, 'entry_count': len(rows),
          'selections': [{'label': label, 'wire_id': wire, 'variant': variant or 'Default',
                          'context_limit': entry['limit']['context'], 'output_limit': entry['limit']['output'],
                          'tool_capability_not_disabled': True, 'high_metadata_admitted': variant == 'high'}
                         for label, wire, variant, _, entry in selections]})
    return selections


def pairs(items):
    calls = [i for i in items if i.get('type') == 'function_call']
    outputs = [i for i in items if i.get('type') == 'function_call_output']
    ids = [i.get('call_id') for i in calls]
    check(len(set(ids)) == len(ids) and all(isinstance(i, str) and i for i in ids), 'unique_call_graph')
    check(all(sum(o.get('call_id') == call['call_id'] for o in outputs) == 1 for call in calls)
          and len(outputs) == len(calls), 'matching_call_output_graph')
    return [(call['name'], call['call_id']) for call in calls]


class Observer:
    """RAM-only native request facts; all upstream IO remains behind Relay."""
    def __init__(self, relay, wire, effort):
        self.facts, self.errors, self.topologies, self.phase = [], [], [], 'initial'
        self.wire, self.effort = wire, effort
        self.relay = relay
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                connection = None
                topology = None
                title = False
                phase = owner.phase
                try:
                    length = int(self.headers['Content-Length'])
                    check(0 < length <= bounded.REQUEST_CAP and self.path == '/provider/responses', 'native_route')
                    body = self.rfile.read(length)
                    request = bounded.exact_json(body)
                    check(request.get('model') == owner.wire, 'native_exact_wire')
                    check(type(request.get('max_output_tokens')) is int and 0 < request['max_output_tokens'] <= 2048,
                          'native_output_cap')
                    tools = request.get('tools', [])
                    title = not tools
                    if not title:
                        check({tool['name'] for tool in tools} == {'apply_patch', 'read'}, 'native_tool_exposure')
                        check(request.get('reasoning', {}).get('effort') == owner.effort, 'native_variant_overlay')
                    graph = pairs(request['input'])
                    owner.facts.append({'phase': owner.phase, 'title': title, 'max_output_tokens': request['max_output_tokens'],
                                        'pairs': graph, 'reasoning_overlay': 'effort' in request.get('reasoning', {}),
                                        'tools': sorted(tool['name'] for tool in tools)})
                    target = urlsplit(owner.relay.ready['provider_base'])
                    connection = http.client.HTTPConnection(target.hostname, target.port, timeout=310)
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
                    total = 0
                    topology = Topology()
                    while True:
                        chunk = reply.read1(16384)
                        if not chunk:
                            break
                        total += len(chunk)
                        check(total <= bounded.RESPONSE_CAP, 'observer_response_cap')
                        topology.feed(chunk)
                        self.wfile.write(chunk)
                        self.wfile.flush()
                except (BrokenPipeError, ConnectionResetError):
                    self.close_connection = True
                except Exception as error:
                    owner.errors.append(error.args[0] if isinstance(error, Refused) else 'observer_io')
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


class Topology:
    """Bounded transient SSE structural comparison; no raw/body/id export."""
    def __init__(self):
        self.pending = b''
        self.done = []
        self.added = []
        self.terminal = []
        self.events = 0
        self.parse_failures = 0

    def feed(self, chunk):
        self.pending += chunk
        check(len(self.pending) <= 2*1024*1024, 'topology_line_cap')
        while b'\n' in self.pending:
            line, self.pending = self.pending.split(b'\n', 1)
            if not line.startswith(b'data:'):
                continue
            try:
                value = json.loads(line[5:])
            except (ValueError, UnicodeError):
                self.parse_failures += 1
                continue
            self.events += 1
            check(self.events <= 10000, 'topology_event_cap')
            kind = value.get('type')
            if kind in ('response.output_item.done', 'response.output_item.added'):
                item = value.get('item')
                if isinstance(item, dict):
                    (self.done if kind.endswith('.done') else self.added).append((value.get('output_index'), item))
            elif kind in ('response.completed', 'response.failed', 'response.incomplete'):
                response = value.get('response', {})
                self.terminal.append((kind, response.get('status'), response.get('output')))

    def summary(self):
        def item_type(item):
            return item.get('type') if item.get('type') in {'reasoning', 'message', 'function_call'} else 'other'
        compared = []
        for kind, status, output in self.terminal:
            for position, item in enumerate(output if isinstance(output, list) else []):
                matched = [(index, prior) for index, prior in self.done if
                           (item.get('id') and prior.get('id') == item.get('id')) or
                           (item.get('call_id') and prior.get('call_id') == item.get('call_id'))]
                for index, prior in matched:
                    shared = set(prior) & set(item)
                    differences = sorted(k for k in shared if prior[k] != item[k]
                                         and k in {'type','id','call_id','name','arguments','status','content','summary','encrypted_content','phase','role'})
                    arguments_equal = None
                    if 'arguments' in differences:
                        try:
                            arguments_equal = json.loads(prior['arguments']) == json.loads(item['arguments'])
                        except (ValueError, TypeError):
                            arguments_equal = False
                    compared.append({'type': item_type(item), 'terminal_position': position,
                                     'done_index': index if type(index) is int and 0 <= index < 10000 else None,
                                     'shared_field_differences': differences, 'parsed_arguments_equal': arguments_equal})
        return {'events': self.events, 'parse_failures': self.parse_failures,
                'added': [{'type': item_type(item), 'index': index if type(index) is int and 0 <= index < 10000 else None}
                          for index, item in self.added],
                'done': [{'type': item_type(item), 'index': index if type(index) is int and 0 <= index < 10000 else None}
                         for index, item in self.done],
                'terminal': [{'kind': kind, 'status': status if status in {'completed','failed','incomplete'} else 'other',
                              'output_count': len(output) if isinstance(output, list) else None}
                             for kind, status, output in self.terminal], 'same_identity_comparisons': compared}


def run_owned(args, project, env):
    process = subprocess.Popen(args, cwd=project, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    starttick = Path(f'/proc/{process.pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
    sizes = [0, 0]
    # No raw output retention: drain bounded pipes and keep byte counts only.
    deadline = time.monotonic()+660
    pipes = [process.stdout, process.stderr]
    timeout = False
    try:
        while pipes:
            if time.monotonic() >= deadline:
                timeout = True
                break
            for stream in select.select(pipes, [], [], .1)[0]:
                chunk = os.read(stream.fileno(), 65536)
                if not chunk:
                    pipes.remove(stream)
                    continue
                index = 0 if stream is process.stdout else 1
                sizes[index] += len(chunk)
                check(sizes[index] <= 16*1024*1024, 'native_output_bytes_cap')
        if timeout and process.poll() is None:
            process.send_signal(signal.SIGINT)
        process.wait(timeout=10)
        return {'exit': 124 if timeout else process.returncode, 'stdout_bytes': sizes[0],
                'stderr_bytes': sizes[1], 'pid': process.pid, 'startticks': starttick, 'group_joined': True}
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
        group_gone(process.pid)
        process.stdout.close()
        process.stderr.close()


def database(root):
    matches = list((root/'home/data').rglob('oc.sqlite'))
    check(len(matches) == 1, 'native_database_count')
    return sqlite3.connect(matches[0].as_uri()+'?mode=ro', uri=True)


def durable(root):
    with database(root) as db:
        operations = list(db.execute('SELECT id,name,state FROM tool_operations ORDER BY rowid'))
        turns = list(db.execute('SELECT id,status,result FROM turns ORDER BY rowid'))
        sessions = list(db.execute('SELECT id FROM sessions WHERE parent_id IS NULL'))
        events = dict(db.execute('SELECT kind,count(*) FROM events GROUP BY kind'))
        messages = dict(db.execute('SELECT role,count(*) FROM messages GROUP BY role'))
        acceptance = [json.loads(row[0]) for row in db.execute('SELECT model_ref FROM turn_acceptances ORDER BY rowid')]
    logs = [json.loads(raw) if raw else {} for _, _, raw in turns]
    return operations, turns, logs, sessions, events, messages, acceptance


def qualify(relay, selection, prompt=PROMPT):
    label, wire, variant, effort, entry = selection
    root = Path(tempfile.mkdtemp(prefix='t55-live-'+label+'-', dir=CACHE/'oc-test-bench-20260924'))
    home, project = root/'home', root/'project'
    project.mkdir()
    (home/'config/opencode').mkdir(parents=True)
    env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home/'config'), 'XDG_DATA_HOME': str(home/'data'),
           'XDG_STATE_HOME': str(home/'state'), 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8',
           'OC_TEST_ALLOW_LOOPBACK': '1', 'TERM': 'xterm-256color'}
    observer = Observer(relay, wire, effort)
    before = relay.snapshot()
    result = {'label': label, 'wire_id': wire, 'variant': variant or 'Default', 'fixture_root': str(root),
              'before_counts': before['counts']}
    try:
        agent = {'mode': 'primary', 'model': 'live-fixture/'+wire,
                 'tools': {'read': True, 'apply_patch': True, 'shell': False, 'bash': False, 'glob': False,
                           'grep': False, 'webfetch': False, 'skill': False, 'question': False,
                           'subagent': False, 'compress': False, 'opencode_models': False,
                           'opencode_session_rename': False, 'opencode_session_move': False}}
        if variant:
            agent['variant'] = variant
        config = {'model': 'live-fixture/'+wire, 'default_agent': 'build', 'agent': {'build': agent},
                  'compaction': {'auto': False}, 'permission': {'*': 'deny', 'read': 'allow', 'edit': 'allow', 'apply_patch': 'allow'},
                  'provider': {'live-fixture': {'npm': '@ai-sdk/openai',
                     'options': {'baseURL': observer.base, 'apiKey': bounded.PLACEHOLDER}, 'models': {wire: entry}}}}
        (home/'config/opencode/opencode.json').write_text(json.dumps(config))
        result['initial_process'] = run_owned([str(ELF), 'run', '--json', prompt], project, env)
        operations, turns, logs, sessions, events, messages, acceptance = durable(root)
        result['initial_states'] = {'operations': [(name, state) for _, name, state in operations],
                                   'turns': [state for _, state, _ in turns], 'retries': events.get('retry_scheduled', 0)}
        allowed = {f'invalid provider output ({s}/{c})': (s, c)
                   for s in ('Decode', 'Added', 'Done', 'Completion')
                   for c in ('InvalidJson', 'InvalidField', 'InvalidIndex', 'InvalidStatus', 'InvalidArguments',
                             'IdentityConflict', 'IndexConflict', 'MissingDone')}
        result['typed_structural_errors'] = [allowed[span['error']] for log in logs for span in log.get('spans', [])
                                            if span.get('error') in allowed]
        check(result['initial_process']['exit'] == 0 and len(turns) == 1 and turns[0][1] == 'completed', 'initial_final_success')
        check([(name, state) for _, name, state in operations] == [('apply_patch', 'completed'), ('read', 'completed')], 'durable_tools')
        check((project/'proof.txt').read_bytes() == EXPECTED and sorted(p.name for p in project.iterdir()) == ['proof.txt'], 'independent_file_bytes')
        graph = pairs(logs[0].get('input', []))
        check([name for name, _ in graph] == ['apply_patch', 'read'], 'durable_pair_names')
        check(all(op.endswith('-'+call) for (op, _, _), (_, call) in zip(operations, graph)), 'operation_call_id_pairing')
        check(logs[0]['spans'][-1].get('finish') == 'stop' and logs[0]['spans'][-1]['status'] == 'completed', 'genuine_final_finish')
        check(any(not fact['title'] and fact['pairs'] == graph for fact in observer.facts), 'actual_function_output_request')
        old_rows = (operations, turns)
        observer.phase = 'reopen'
        check(len(sessions) == 1, 'single_root_session')
        result['reopen_process'] = run_owned([str(ELF), 'run', '--json', '--session', sessions[0][0], FOLLOWUP], project, env)
        operations, turns, logs, sessions, events, messages, acceptance = durable(root)
        check(result['reopen_process']['exit'] == 0 and [state for _, state, _ in turns] == ['completed', 'completed'], 'reopen_final_success')
        check(operations == old_rows[0] and turns[:1] == old_rows[1], 'immutable_settled_history_no_replay')
        check((project/'proof.txt').read_bytes() == EXPECTED, 'reopen_file_unchanged')
        check(any(fact['phase'] == 'reopen' and not fact['title'] and fact['pairs'] == graph for fact in observer.facts), 'reopen_actual_history_graph')
        check(logs[-1]['spans'][-1].get('finish') == 'stop' and logs[-1]['spans'][-1]['status'] == 'completed', 'reopen_genuine_finish')
        check(all(a['id'] == wire and a.get('variant') == variant for a in acceptance), 'durable_exact_selection')
        check(not observer.errors, 'observer_errors')
        result.update(status='PASS', operations=[(name, state) for _, name, state in operations],
                      turns=[state for _, state, _ in turns], call_ids=[call for _, call in graph],
                      native_graph_paired=True, operation_id_suffixes_match_call_ids=True, actual_result_request=True,
                      reopen_graph=True, settled_rows_unchanged=True, file_bytes=len(EXPECTED),
                      file_sha256=hashlib.sha256(EXPECTED).hexdigest(), final_finish='stop', reopen_finish='stop',
                      message_roles=messages, retry_events=events.get('retry_scheduled', 0),
                      generation_dispatches=events.get('generation_dispatched', 0))
    except Exception as error:
        result.update(status='FAIL', code=error.args[0] if isinstance(error, Refused) else 'operator_fact_or_io')
    finally:
        observer.close()
        result['native_posts'] = [{'phase': f['phase'], 'title': f['title'], 'pairs': len(f['pairs']),
                                  'max_output_tokens': f['max_output_tokens'], 'reasoning_overlay': f['reasoning_overlay'],
                                  'tools': f['tools']} for f in observer.facts]
        result['observer_errors'] = observer.errors
        result['stream_topologies'] = observer.topologies
        result['after_counts'] = relay.snapshot()['counts']
        result['observer_joined'] = True
    emit(result)
    return result


def main():
    campaign, ledger, prefix, before = preflight()
    if '--offline-relay' in sys.argv:
        fixture = Fixture(ELF, 'relay')
        try:
            emit({'offline_reuse': 'PASS', **fixture.relay_case()})
        finally:
            fixture.close()
        check(ledger.snapshot() == before, 'offline_touched_live_campaign')
        return 0
    if '--offline-operator' in sys.argv or '--offline-operator-high' in sys.argv:
        fixture = Fixture(ELF, 'headless')
        def response(number, request):
            if number == 1:
                call = {'type': 'function_call', 'id': 'fc_patch', 'call_id': 'call_patch', 'name': 'apply_patch',
                        'arguments': json.dumps({'patchText': '*** Begin Patch\n*** Add File: proof.txt\n+T55 native live tool roundtrip\n*** End Patch'}), 'status': 'completed'}
            elif number == 2:
                call = {'type': 'function_call', 'id': 'fc_read', 'call_id': 'call_read', 'name': 'read',
                        'arguments': json.dumps({'path': 'proof.txt'}), 'status': 'completed'}
            else:
                return 200, terminal([message('Synthetic completion')]), False
            return 200, event({'type': 'response.output_item.done', 'output_index': 0, 'item': call})+terminal([]), False
        fixture.response = response
        fixture.campaign = bounded.Ledger.create(str(fixture.root/'offline-operator'))
        upstream = f'http://127.0.0.1:{fixture.server.server_port}'
        fixture.manifest = {'provider': {'generation_url': upstream+'/v1/responses', 'discovery_url': upstream+'/v1/models',
                                        'headers': {'authorization': 'Bearer synthetic-canary'}},
                            'mcp': {'unused': {'url': upstream+'/unused', 'headers': {}}}}
        fixture.relay = Relay(fixture.campaign, fixture.manifest)
        try:
            high = '--offline-operator-high' in sys.argv
            entry = {'limit': {'context': 65536, 'output': 2048}}
            if high:
                entry['variants'] = {'high': {'reasoningEffort': 'high'}}
            result = qualify(fixture.relay, ('offline', 'fixture-model', 'high' if high else None,
                                             'high' if high else None, entry))
            check(result['status'] == 'PASS', 'offline_operator_failed')
            check(ledger.snapshot() == before, 'offline_touched_live_campaign')
        finally:
            fixture.close()
        return 0
    if '--run' not in sys.argv:
        return 0
    configured, manifest = product_manifest()
    relay = Relay(campaign, manifest, offline=False)
    relay_starttick = Path(f'/proc/{relay.process.pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
    results = []
    try:
        selections = catalog(relay, configured)
        if '--configured-experiment' in sys.argv:
            selections = [s for s in selections if s[0] == 'configured']
        for selection in selections:
            results.append(qualify(relay, selection, EXPLICIT_PROMPT if '--configured-experiment' in sys.argv else PROMPT))
    finally:
        relay.close()
        after = ledger.snapshot()
        with ledger.locked() as (fd, _):
            os.lseek(fd, 0, os.SEEK_SET)
            check(os.read(fd, len(prefix)) == prefix, 'prior_journal_bytes_changed')
        check(digest(ELF) == ELF_SHA, 'binary_changed_after_live')
        emit({'campaign_after': after['counts'], 'input_bytes': after['input_bytes'],
              'remaining_generation': 24-after['counts']['generation'], 'prior_journal_prefix_unchanged': True,
              'relay_joined': True, 'binary_hash_unchanged': True,
              'relay_pid': relay.process.pid, 'relay_startticks': relay_starttick,
              'new_receipts': [{k: a[k] for k in ('reserve', 'kind', 'outcome', 'http_status', 'failure', 'failure_stage') if k in a}
                               for a in after['attempts'][len(before['attempts']):]]})
    expected = 1 if '--configured-experiment' in sys.argv else 2
    return 0 if len(results) == expected and all(r['status'] == 'PASS' for r in results) else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as error:
        emit({'status': 'STOP', 'code': error.args[0] if isinstance(error, Refused) else 'operator_preflight_or_io',
              'unknown_reservations_remain_consumed': True})
        raise SystemExit(2)
