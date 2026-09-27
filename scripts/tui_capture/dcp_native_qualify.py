#!/usr/bin/env python3
"""VIS38 actual-owner qualification on an explicitly parent-released binary, offline.

Public oc run seeds history and executes the advertised ordinary compress tool.
SQLite is read-only. No native run snapshot, history row or compression is injected.
This bounded single/multi/recompression/restart probe does not replace A07.
"""
import argparse
from collections import Counter
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import threading

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--oc', required=True, type=Path)
parser.add_argument('--released-sha256', required=True,
                    help='Parent-attested final binary digest; execution is deferred until release')
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
output = args.output.resolve()
assert output.parent == repo / 'evidence/tui/recovery-v00'
assert output.name.startswith('dcp-native') and output.name.replace('-', '').isalnum()
assert args.oc.is_absolute()
binary_sha256 = hashlib.sha256(args.oc.read_bytes()).hexdigest()
assert binary_sha256 == args.released_sha256, 'Binary does not match parent release'
output.mkdir()  # immutable evidence attempt
root = Path('/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs') / output.name
root.mkdir()  # no existing state replay
home, project = root / 'home', root / 'project'
for directory in (home / 'config/opencode', home / 'data', home / 'cache', home / 'state', project):
    directory.mkdir(parents=True)
session = 's-vis38-actual-owner'
events, stage_requests = [], []
stage, compression_args, count = '', None, 0
lock = threading.Lock()


def save(name, value):
    with (output / name).open('x') as stream:
        json.dump(value, stream, indent=2, ensure_ascii=False)
        stream.write('\n')


def observe():
    found = list((home / 'data').rglob('*.sqlite3')) + list((home / 'data').rglob('*.db')) + list((home / 'data').rglob('*.sqlite'))
    observations = []
    for database in sorted(set(found)):
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as connection:
            connection.row_factory = sqlite3.Row
            tables = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")}
            wanted = ('messages', 'conversation_messages', 'compression_blocks', 'compression_members',
                      'prune_marks', 'tool_operations', 'dcp_run_views', 'dcp_accounting',
                      'dcp_coverage', 'dcp_run_identity')
            views = {r[0] for r in connection.execute("SELECT name FROM sqlite_master WHERE type='view'")}
            data = {name: [dict(r) for r in connection.execute(f'SELECT * FROM "{name}" LIMIT 101')]
                    for name in wanted if name in tables | views}
            assert all(len(value) <= 100 for value in data.values()), 'Bounded fixture observation cap exceeded'
            observations.append({'database': str(database), 'tables': sorted(tables), 'data': data})
    return observations


def rows(observations, table):
    return [row for observation in observations for row in observation['data'].get(table, [])]


def estimate(text):
    # Independently declared DcpEstimateMethod: half-up UTF-16 code units / 4.
    return (len(text.encode('utf-16-le')) // 2 + 2) // 4


def wire_contents(body, current_call):
    """Measure actual provider content, omitting only explicit non-accounting lanes.

    Current compress call/result do not exist in the owner's pre-commit wire.
    The generated developer anchor lane is outside DcpRunSnapshot.net_saved.
    No envelope-byte estimator, summary stub, or native formatter is substituted.
    """
    content, excluded = [], []
    for index, item in enumerate(body['input']):
        if item.get('type') in ('function_call', 'function_call_output') and item.get('call_id') == current_call:
            excluded.append({'index': index, 'reason': 'current-compress-call/result', 'item': item})
            continue
        parts = item.get('content', []) if item.get('type') != 'reasoning' else item.get('summary', [])
        texts = [p['text'] for p in parts if isinstance(p, dict) and isinstance(p.get('text'), str)]
        if item.get('role') == 'developer' and any(t.startswith('DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: ') for t in texts):
            assert len(texts) == 1, 'Unexpected mixed anchor-lane content'
            excluded.append({'index': index, 'reason': 'generated-DCP-anchor-lane', 'item': item})
            continue
        if item.get('type') == 'function_call':
            content.append(item['arguments'])
        elif item.get('type') == 'function_call_output':
            assert isinstance(item['output'], str), 'Fixture expects textual tool output'
            content.append(item['output'])
        else:
            content.extend(texts)
    return content, excluded


class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        global count
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        with lock:
            count += 1
            index = count
        system = body.get('instructions', '') + json.dumps([x for x in body.get('input', []) if x.get('role') in ('system', 'developer')])
        title = 'title generator' in system.lower() or (not body.get('tools') and 'title' in system.lower())
        event = {'kind': 'request', 'index': index, 'stage': stage, 'title': title, 'path': self.path,
                 'body': body, 'request_sha256': hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest()}
        events.append(event)
        if self.path != '/v1/responses' or body.get('model') != 'vis38-native-model' or body.get('stream') is not True or index > 16 or stage.startswith('capture-'):
            self.send_error(400, 'VIS38 bounded request contract')
            return
        outputs = [x for x in body.get('input', []) if x.get('type') == 'function_call_output'
                   and x.get('call_id') == 'call_vis38_' + stage]
        if not title:
            stage_requests.append(event)
        tool = stage.startswith('compress-') and not title and not outputs
        if tool:
            definitions = {t['name']: t for t in body.get('tools', [])}
            assert 'compress' in definitions, 'Native owner did not advertise compress'
            assert {'topic', 'content'} <= set(definitions['compress']['parameters']['required'])
            anchor_prefix = 'DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: '
            anchor_texts = [p['text'] for x in body['input'] if x.get('role') == 'developer'
                            for p in x.get('content', []) if p.get('text', '').startswith(anchor_prefix)]
            assert len(anchor_texts) == 1, 'Missing unique actual owner anchor lane'
            anchors = json.loads(anchor_texts[0][len(anchor_prefix):])
            positions = {a['id']: i for i, a in enumerate(anchors)}
            for span in compression_args['content']:
                assert span['startId'] in positions and span['endId'] in positions, 'Actual owner did not advertise span endpoints'
                start, end = positions[span['startId']], positions[span['endId']]
                assert start <= end and all(a['closed'] for a in anchors[start:end + 1]), 'Span is not closed in actual owner anchor schema'
            item = {'id': f'fc_vis38_{index}', 'type': 'function_call', 'status': 'completed',
                    'call_id': 'call_vis38_' + stage, 'name': 'compress', 'arguments': json.dumps(compression_args)}
        else:
            if title:
                text = 'VIS38 actual compression owner'
            elif stage.startswith('seed-'):
                text = f'VIS38-ARCHIVE-{stage[-1]}: closed analysis complete; retain requirement {stage[-1]}.'
            else:
                text = 'VIS38-NATIVE-CONTINUED: retain requirements; no tool replay.'
            if outputs:
                assert all(not str(x.get('output', '')).startswith('error:') for x in outputs), 'Actual owner compression failed'
            item = {'id': f'msg_vis38_{index}', 'type': 'message', 'role': 'assistant', 'status': 'completed',
                    'content': [{'type': 'output_text', 'text': text, 'annotations': []}]}
        response = {'id': f'resp_vis38_{index}', 'object': 'response', 'created_at': 1700000000,
                    'model': body['model'], 'status': 'in_progress', 'output': [], 'error': None, 'incomplete_details': None}
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        sequence = 0

        def send(value):
            nonlocal sequence
            self.wfile.write(('event: ' + value['type'] + '\ndata: ' + json.dumps({**value, 'sequence_number': sequence}) + '\n\n').encode())
            self.wfile.flush()
            sequence += 1

        send({'type': 'response.created', 'response': response})
        send({'type': 'response.output_item.added', 'output_index': 0, 'item': {**item, 'status': 'in_progress', **({'arguments': ''} if tool else {'content': []})}})
        if tool:
            send({'type': 'response.function_call_arguments.delta', 'item_id': item['id'], 'output_index': 0, 'delta': item['arguments']})
        else:
            send({'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}})
            send({'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'delta': text})
            send({'type': 'response.output_text.done', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'text': text})
            send({'type': 'response.content_part.done', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'part': item['content'][0]})
        send({'type': 'response.output_item.done', 'output_index': 0, 'item': item})
        send({'type': 'response.completed', 'response': {**response, 'status': 'completed', 'output': [item], 'usage': {'input_tokens': 10000, 'output_tokens': 2000, 'total_tokens': 12000}}})


server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
threading.Thread(target=server.serve_forever, daemon=True).start()
config = {'model': 'fixture/vis38-native-model', 'plugin': ['@tarquinen/opencode-dcp@3.1.15'],
          'permissions': {'*': 'deny', 'compress': 'allow'},
          'provider': {'fixture': {'name': 'VIS38 Display Fixture', 'npm': '@ai-sdk/openai',
              'options': {'baseURL': f'http://127.0.0.1:{server.server_port}/v1', 'apiKey': 'offline-fixture-not-a-secret'},
              'models': {'vis38-native-model': {'name': 'VIS38 Display Model', 'limit': {'context': 1000000, 'output': 64000}}}}}}
(home / 'config/opencode/opencode.json').write_text(json.dumps(config))
(project / 'dcp.jsonc').write_text(json.dumps({'enabled': True, 'autoUpdate': False,
    'pruneNotification': 'detailed', 'pruneNotificationType': 'chat',
    'compress': {'mode': 'range', 'permission': 'allow', 'showCompression': True,
                 'minContextLimit': 1000000, 'maxContextLimit': 1000000},
    'strategies': {'deduplication': {'enabled': False}, 'purgeErrors': {'enabled': False}}}))
(home / 'config/opencode/cli.json').write_text(json.dumps({'session': {'sidebar': 'hide', 'tps': False}, 'tabs': {'layout': 'horizontal'}}))
env = {'HOME': str(home), 'XDG_CONFIG_HOME': str(home / 'config'), 'XDG_DATA_HOME': str(home / 'data'),
       'XDG_CACHE_HOME': str(home / 'cache'), 'XDG_STATE_HOME': str(home / 'state'),
       'OC_TEST_ALLOW_LOOPBACK': '1', 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8', 'TZ': 'UTC'}
commands, checks = [], []
result = {'status': 'IN_PROGRESS', 'qualification': 'Actual native compression/continuation and durable typed snapshot, independent of display reference',
           'oc_binary_sha256': binary_sha256, 'parent_released_sha256': args.released_sha256,
           'build_source_association': 'Explicit parent-attested digest; parent retains build/source proof',
           'source_HEAD': subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repo, capture_output=True, text=True, check=True).stdout.strip(),
            'checks': checks, 'display_cases': [], 'not_qualified': ['failure/cancel/no-gain', 'Undo/Redo routing', 'bounded archive/resources', 'live ToolCallFinished publication / public ToolOpView DTO equality']}


def run(label, prompt):
    global stage
    stage = label
    stage_requests.clear()
    argv = [str(args.oc), 'run', '--session', session, prompt]
    process = subprocess.Popen(argv, cwd=project, env=env, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=45)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        raise RuntimeError('Actual native subprocess timeout; owned process group stopped')
    commands.append({'argv': argv, 'exit_code': process.returncode, 'stdout': stdout.decode(), 'stderr': stderr.decode()})
    assert process.returncode == 0, stderr.decode()
    observation = observe()
    save(label + '-snapshot.json', observation)
    return observation, list(stage_requests)


def capture_stage(label, context_file, run_snapshot, display=None):
    global stage
    display = display or {'notification': 'detailed', 'channel': 'chat', 'show_compression': True}
    observation = observe()
    canonical = sorted(rows(observation, 'conversation_messages'), key=lambda r: r['seq'])
    spec_file = 'native-tui-' + label + '-spec.json'
    save(spec_file, {'binary': str(args.oc), 'binary_sha256': binary_sha256,
         'origin': 'oc', 'session': session, 'operation_id': run_snapshot['operation_id'], 'stage': label,
         'context_snapshot': context_file, 'isolated_root': str(root),
         'argv': [str(args.oc), 'tui', '--session', session], 'cwd': str(project), 'env': env,
         'source_HEAD': result['source_HEAD'], 'typed_snapshot': run_snapshot, 'display': display,
         'capture_after': canonical[-1]['text'],
         'capture_gate': 'Synchronous genuine stage before subsequent mutation; no history ingress.'})
    capture_output = output.parent / ('dcp-nativecapture-' + output.name + '-' + label)
    held_requests = count
    stage = 'capture-' + label  # Any provider call during display is refused and recorded.
    capture = subprocess.run(['/usr/bin/node', str(repo / 'scripts/tui_capture/dcp_reference_capture.mjs'),
               '--native-spec', str(output / spec_file), '--output', str(capture_output)],
               capture_output=True, text=True, timeout=360)
    save('native-capture-' + label + '.json', {'argv': capture.args, 'exit_code': capture.returncode,
         'stdout': capture.stdout, 'stderr': capture.stderr, 'output': str(capture_output),
         'lock_sha256': hashlib.sha256((capture_output / 'capture.lock.json').read_bytes()).hexdigest(),
         'provider_requests_before': held_requests, 'provider_requests_after': count})
    assert count == held_requests, 'Opening/closing genuine stage unexpectedly requested provider'
    assert observe() == observation, 'Read-only stage capture mutated observed history/accounting'
    assert capture.returncode == 0, capture.stderr
    result['display_cases'].append({'stage': label, 'context_snapshot': context_file,
        'typed_snapshot': run_snapshot, 'display': display, 'output': str(capture_output),
        'provider_requests': 0, 'owner_state_unchanged': True})


try:
    for suffix in ('A', 'B', 'C'):
        seeded, _ = run('seed-' + suffix, f'VIS38 seed {suffix}: closed analysis, retain requirement {suffix}. ' + ('Closed analysis 日本語 résumé 👩‍💻 é. ' * 300))
    baseline = rows(seeded, 'messages')
    assert len(baseline) == 6, 'Seeded public run history must contain three user/assistant pairs'
    baseline.sort(key=lambda r: r['seq'])
    for label, covered in [('single', baseline[:2]), ('multi', baseline[2:6]), ('recompression', baseline[:2])]:
        ranges = [{'startId': covered[i]['id'], 'endId': covered[i + 1]['id'],
                   'summary': f'Closed range {i // 2 + 1}; preserve requirements; continue safely.' + (' Bounded real summary detail retained for later actual recompression.' * 4 if label == 'single' else '')} for i in range(0, len(covered), 2)]
        compression_args = {'topic': 'VIS38 ' + label + (' long topic 日本語 résumé 👩‍💻 é — preserve closed analysis and requirements across ranges with full real summaries' if label == 'multi' else ''), 'content': ranges}
        if label == 'recompression':
            ranges = [{'startId': 'b0001', 'endId': 'b0001', 'summary': 'Requirement A retained.'}]
            compression_args['content'] = ranges
        precommit = observe()
        save('compress-' + label + '-before.json', precommit)
        before_runs = rows(precommit, 'dcp_run_views')
        committed, requests = run('compress-' + label, 'VIS38 compress ' + label + ': execute the closed ranges, then continue.')
        assert len(requests) == 2, 'Exactly tool-call + continuation requests expected'
        before, after = [json.dumps(r['body']['input'], ensure_ascii=False).encode() for r in requests]
        if label != 'recompression':
            assert len(after) < len(before), 'Actual next provider input did not shrink'
        calls = {x['call_id'] for x in requests[1]['body']['input'] if x.get('type') == 'function_call'}
        assert all(x['call_id'] in calls for x in requests[1]['body']['input'] if x.get('type') == 'function_call_output'), 'Broken call/result graph'
        current_call = 'call_vis38_compress-' + label
        outcomes = [x for x in requests[1]['body']['input'] if x.get('type') == 'function_call_output' and x.get('call_id') == current_call]
        assert len(outcomes) == 1 and json.loads(outcomes[0]['output'])['status'] == 'compressed', 'No successful actual compress outcome'
        assert sum(x.get('type') == 'function_call' and x.get('call_id') == current_call for x in requests[1]['body']['input']) == 1, 'Compress call replay or missing call'
        raw_after = rows(committed, 'messages')
        assert all(r in raw_after for r in baseline), 'Compression changed immutable seeded raw history'
        run_views = rows(committed, 'dcp_run_views')
        assert len(run_views) == len(before_runs) + 1, 'Real successful owner operation lacks exactly one durable typed run'
        newest = next(r for r in run_views if r not in before_runs)
        snapshot = json.loads(newest['snapshot'])
        assert snapshot['session'] == session and snapshot['operation_id'] == newest['operation_id']
        operations = [r for r in rows(committed, 'tool_operations') if r['id'] == snapshot['operation_id']]
        assert len(operations) == 1 and operations[0]['name'] == 'compress' and operations[0]['state'] == 'completed'
        blocks = [r for r in rows(committed, 'compression_blocks') if r['id'] in snapshot['block_ids']]
        blocks.sort(key=lambda b: snapshot['block_ids'].index(b['id']))
        assert snapshot['ordinal'] == {'single': 1, 'multi': 2, 'recompression': 3}[label]
        assert len(blocks) == len(ranges), 'Multi-range must share one run/card'
        expected_new = 0 if label == 'recompression' else len(covered)
        assert snapshot['new_messages'] == expected_new and snapshot['new_tools'] == 0
        assert snapshot['summary'] == sum(estimate(r['summary']) for r in blocks), 'Summary estimate differs from stored summary content'
        assert snapshot['method'] == 'utf16_round_quarter_fallback'
        before_content, before_excluded = wire_contents(requests[0]['body'], current_call)
        after_content, after_excluded = wire_contents(requests[1]['body'], current_call)
        removed_content = list((Counter(before_content) - Counter(after_content)).elements())
        independent_removed = sum(map(estimate, removed_content))
        inherited_content = [f"[compressed {b['id']}] {b['summary']}" for b in rows(precommit, 'compression_blocks')
                             if label == 'recompression' and b['id'] == 'b0001']
        independent_removed -= sum(map(estimate, inherited_content))
        independent_net = max(0, sum(map(estimate, before_content)) - sum(map(estimate, after_content)))
        assert independent_net > 0, 'Actual measured wire content has no token gain'
        assert sum(len(t.encode()) for t in after_content) < sum(len(t.encode()) for t in before_content), 'Actual wire content has no byte gain'
        assert snapshot['removed'] == independent_removed, 'Gross removal differs from actual projected content multiset'
        assert snapshot['net_saved'] == independent_net, 'Net savings differ from wire content excluding anchor lane/new result'
        save('compress-' + label + '-measurement.json', {'before_content': before_content, 'after_content': after_content,
             'removed_content': removed_content, 'before_excluded': before_excluded, 'after_excluded': after_excluded,
              'inherited_content_excluded_from_gross': inherited_content,
              'independent_removed': independent_removed, 'independent_net_saved': independent_net})
        assert len(snapshot['bar']) == 50 and set(snapshot['bar']) <= {'█', '░', '⣿'}
        # At commit the current user exists, while the continuation assistant has
        # not yet been appended. Our actual fixture has one assistant per run.
        canonical_after = sorted(rows(committed, 'conversation_messages'), key=lambda r: r['seq'])
        assert canonical_after[-1]['role'] == 'assistant' and canonical_after[-1]['text'] == 'VIS38-NATIVE-CONTINUED: retain requirements; no tool replay.'
        canonical = canonical_after[:-1]
        memberships = rows(committed, 'compression_members')
        previous_coverage = {(r['kind'], r['identity']) for r in rows(precommit, 'dcp_coverage')}
        new_coverage = {(r['kind'], r['identity']) for r in rows(committed, 'dcp_coverage')} - previous_coverage
        recent_ids = {identity for kind, identity in new_coverage if kind == 'message'}
        recent_tools = {identity for kind, identity in new_coverage if kind == 'call'}
        assert recent_ids == (set() if label == 'recompression' else {r['id'] for r in covered}) and not recent_tools, 'Unexpected coverage delta'
        assert snapshot['new_messages'] == len(recent_ids) and snapshot['new_tools'] == len(recent_tools)
        assert not rows(committed, 'prune_marks'), 'Probe requires unpruned disjoint ranges; cannot assume lifetime summaries are active'
        prior_ids = {r['message_id'] for r in memberships} - recent_ids
        range_inputs = []
        for index, (block, span) in enumerate(zip(blocks, ranges)):
            members = {r['message_id'] for r in memberships if r['block_id'] == block['id']}
            range_rows = covered[index * 2:index * 2 + 2]
            assert members == {r['id'] for r in range_rows}
            if label != 'recompression':
                assert block['start_msg'] == span['startId'] and block['end_msg'] == span['endId']
            assert '(b' not in block['summary'] and '{block_' not in block['summary'], 'Nested-summary probe requires additional qualification'
            range_removed = sum(estimate(r['text']) for r in range_rows if r['text'] in removed_content)
            range_inputs.append({'block_id': block['id'], 'topic': block['topic'], 'summary': block['summary'],
                                 'removed': range_removed,
                                 'summary_tokens': estimate(block['summary']), 'new_message_ids': sorted(members & recent_ids),
                                 'new_tool_ids': []})
        assert sum(r['removed'] for r in range_inputs) == snapshot['removed']
        assert sum(r['summary_tokens'] for r in range_inputs) == snapshot['summary']
        expected_bar = ['█'] * 50
        for position, message in enumerate(canonical):
            category = '⣿' if message['id'] in recent_ids else '░' if message['id'] in prior_ids else '█'
            for cell in range(position * 50 // len(canonical), (position + 1) * 50 // len(canonical)):
                expected_bar[cell] = category
        assert snapshot['bar'] == ''.join(expected_bar), 'Frozen categorical bar differs from canonical commit positions'
        active_ids = {r['block_id'] for r in memberships}
        active_blocks = [b for b in rows(committed, 'compression_blocks') if b['id'] in active_ids]
        assert snapshot['cumulative']['active_summary'] == sum(estimate(r['summary']) for r in active_blocks)
        assert snapshot['cumulative']['gross_removed'] == sum(c['independent_removed'] for c in checks) + snapshot['removed']
        assert snapshot['cumulative']['net_saved'] == sum(c['independent_net_saved'] for c in checks) + independent_net
        assert snapshot['cumulative']['compressions'] == snapshot['ordinal'] and snapshot['cumulative']['prunes'] == 0 and snapshot['cumulative']['complete']
        assert snapshot['cumulative']['method'] == snapshot['method']
        assert [json.loads(r['snapshot']) for r in rows(committed, 'dcp_accounting')] == [snapshot['cumulative']]
        assert [r['high_water'] for r in rows(committed, 'dcp_run_identity')] == [snapshot['ordinal']]
        checks.append({'stage': label, 'operation_id': snapshot['operation_id'], 'ordinal': snapshot['ordinal'],
                       'before_input_bytes': len(before), 'after_input_bytes': len(after),
                        'independent_removed': independent_removed, 'independent_summary': snapshot['summary'], 'independent_net_saved': independent_net,
                         'new_messages': expected_new, 'ranges': len(ranges), 'typed_snapshot': snapshot,
                        'commit_message_ids': [r['id'] for r in canonical], 'prior_message_ids': sorted(prior_ids),
                        'new_message_ids': sorted(recent_ids), 'new_tool_ids': sorted(recent_tools), 'range_inputs': range_inputs,
                         'active_summaries': [{'block_id': b['id'], 'summary_tokens': estimate(b['summary'])} for b in active_blocks],
                         'display': {'notification': 'detailed', 'channel': 'chat', 'show_compression': True},
                        'categorical_bar_independently_checked': True})
        capture_stage(label, 'compress-' + label + '-snapshot.json', snapshot)
    stable = rows(committed, 'dcp_run_views')
    restarted, requests = run('restart', 'VIS38 restart: continue with saved requirements; do not replay tools.')
    assert len(requests) == 1
    assert rows(restarted, 'dcp_run_views') == stable, 'Restart rewrote/replayed historical typed runs'
    assert all(r in rows(restarted, 'messages') for r in rows(committed, 'messages')), 'Restart changed raw history'
    checks.append({'stage': 'restart', 'requests': 1, 'typed_runs_unchanged': True, 'raw_history_preserved': True})
    capture_stage('restart', 'restart-snapshot.json', snapshot)
    dcp_file = project / 'dcp.jsonc'
    original_config = json.loads(dcp_file.read_text())
    for label, display in [('minimal', {'notification': 'minimal', 'channel': 'chat', 'show_compression': True}),
                           ('off', {'notification': 'off', 'channel': 'chat', 'show_compression': True}),
                           ('show-false', {'notification': 'detailed', 'channel': 'chat', 'show_compression': False}),
                           ('toast', {'notification': 'detailed', 'channel': 'toast', 'show_compression': True})]:
        configured = json.loads(json.dumps(original_config))
        configured['pruneNotification'] = display['notification']
        configured['pruneNotificationType'] = display['channel']
        configured['compress']['showCompression'] = display['show_compression']
        dcp_file.write_text(json.dumps(configured))
        save('control-' + label + '-config.json', configured)
        capture_stage(label, 'restart-snapshot.json', snapshot, display)
    dcp_file.write_text(json.dumps(original_config))
    save('native-tui-spec.json', {'binary': str(args.oc), 'origin': 'oc', 'session': session, 'isolated_root': str(root),
                                'argv': [str(args.oc), 'tui', '--session', session], 'cwd': str(project), 'env': env,
                                'note': 'Use the same real saved owner session for deferred native PTY; no transcript/snapshot injection'})
    result['status'] = 'PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE'
except Exception as error:
    result['status'] = 'FAILED_ACTUAL_OWNER_PROBE'
    result['reason'] = str(error)
finally:
    server.shutdown()
    server.server_close()
    result['provider_requests'] = count
    save('commands.json', commands)
    save('requests.json', events)
    result['evidence_seals'] = {file.name: hashlib.sha256(file.read_bytes()).hexdigest()
                              for file in sorted(output.glob('*.json'))}
    result['helper_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    save('result.json', result)
print(json.dumps({'output': str(output), 'status': result['status'], 'reason': result.get('reason'),
                  'provider_requests': count, 'qualified_runs': [c['stage'] for c in checks if 'typed_snapshot' in c],
                  'captured_stages': [c['stage'] for c in result['display_cases']]}, ensure_ascii=False))
sys_exit = 0 if result['status'].startswith('PASS_') else 1
raise SystemExit(sys_exit)
