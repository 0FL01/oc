#!/usr/bin/env python3
"""Reference-only, source-traced OC2 public transfer for one genuine native stage.

Native accepts no fixture. Duration endpoints are explicitly derived display
coordinates (admission epoch + stored elapsed duration), not claimed wall-clock
assistant events. Real terminal turn status supplies OC2 idle boundaries.
"""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import re

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--native', required=True, type=Path)
parser.add_argument('--oracle', required=True, type=Path)
parser.add_argument('--case', required=True)
parser.add_argument('--output', required=True, type=Path)
args = parser.parse_args()
repo = Path(__file__).resolve().parents[2]
native, oracle, output = args.native.resolve(), args.oracle.resolve(), args.output.resolve()
parent = repo / 'evidence/tui/recovery-v00'
assert native.parent == oracle.parent == output.parent == parent
assert re.fullmatch(r'dcp-display[a-z0-9-]+', output.name)


def read(file):
    return json.loads(file.read_text())


def sha(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


owner = read(native / 'result.json')
assert owner['status'] == 'PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE'
assert sha(repo / 'scripts/tui_capture/dcp_native_qualify.py') == owner['helper_sha256'], 'Response fixture source must match actual probe'
for name, digest in owner['evidence_seals'].items():
    assert Path(name).name == name and sha(native / name) == digest, name
association = read(oracle / 'owner-association.json')
assert association['result_sha256'] == sha(native / 'result.json')
goldens = read(oracle / 'goldens.json')
assert read(oracle / 'result.json')['goldens_sha256'] == sha(oracle / 'goldens.json')
case = next(c for c in goldens['notifications'] if c['id'] == args.case)
display_case = next(c for c in owner['display_cases'] if c['stage'] == case['native_context']['stage'])
assert display_case['typed_snapshot'] == case['native_context']['typed_snapshot']
observations = read(native / display_case['context_snapshot'])


def rows(table):
    return [r for o in observations for r in o['data'][table]]


context = sorted(rows('conversation_messages'), key=lambda r: r['seq'])
assert context == case['native_context']['messages']
session = next(s for s in rows('sessions') if s['id'] == case['native_context']['session'])
fixture_config = read(native / 'fixture-config.json')
native_spec = read(native / ('native-tui-' + display_case['stage'] + '-spec.json'))
assert fixture_config['cwd'] == native_spec['cwd']
acceptances = {a['turn_id']: a for a in rows('turn_acceptances')}
by_message = {}
turns = []
for turn in rows('turns'):
    assert turn['session_id'] == session['id'] and turn['status'] == 'completed'
    result = json.loads(turn['result'])
    accepted = acceptances[turn['id']]
    model = json.loads(accepted['model_ref'])
    assert model['provider'] == result['provider'] and model['id'] == result['model']
    assert accepted['user_message'] == result['user_message']
    match = re.fullmatch(r't' + re.escape(session['id']) + r'-(\d+)-(\d+)-(\d+)', turn['id'])
    assert match, 'Only source-verified native turn identity epoch is supported'
    created = int(match[1])
    display = result['display']
    assert display['agent'] == 'build' and display['agent_color_index'] == 0
    assert result['usage'] == list(display['context_usage']), 'Last physical response usage must equal displayed context'
    item = {'native_turn_id': turn['id'], 'native_status': turn['status'], 'result': result,
            'created': created, 'completed': created + display['duration_ms'], 'display': display,
            'accepted_model': {'providerID': model['provider'], 'id': model['id']}}
    turns.append(item)
    for identity in (result['user_message'], result['assistant_message']):
        assert identity not in by_message
        by_message[identity] = item
assert set(by_message) == {r['id'] for r in context}, 'Do not silently drop unmatched actual messages'

notifications = list(case['native_context']['prior_notifications'])
if not case['native_context'].get('reference_delivery'):
    notifications.append({'operation_id': case['native_context']['operation_id'],
                          'before_message_id': context[-1]['id'], 'payload': case['payload']})
assert len(notifications) == 3, 'This bounded display maps exactly three real successful operations'
stored_runs = {r['operation_id']: json.loads(r['snapshot']) for r in rows('dcp_run_views')}
blocks = {b['id']: b for b in rows('compression_blocks')}
messages, traces = [], []
for row in context:
    turn = by_message[row['id']]
    metadata = {'vis38_native': {'message_id': row['id'], 'turn_id': turn['native_turn_id'],
        'display': turn['display'], 'time_mapping': 'admission epoch; completed = admission + recorded elapsed display duration; not absolute assistant event time'}}
    for notification in notifications:
        if notification['before_message_id'] != row['id'] or notification['payload'] is None:
            continue
        run = stored_runs[notification['operation_id']]
        created_seconds = blocks[run['block_ids'][0]]['created_at']
        messages.append({'id': 'msg_vis38_dcp_' + str(run['ordinal']), 'type': 'user',
            'text': notification['payload'], 'time': {'created': int(Decimal(created_seconds) * 1000)},
            'metadata': {'vis38_native': {'operation_id': run['operation_id'],
                'created_at_precision': 'stored whole seconds', 'delivery': 'unchanged D05 ignored/noReply -> U34 display wrapper'}}})
    identity = 'msg_vis38_native_' + row['id']
    if row['role'] == 'user':
        assert row['text'] == turn['result']['input'][0]['content'][0]['text']
        messages.append({'id': identity, 'type': 'user', 'text': row['text'],
                         'time': {'created': turn['created']}, 'metadata': metadata})
    else:
        assert row['role'] == 'assistant'
        terminal = turn['result']['input'][-1]
        assert terminal['type'] == 'message' and terminal['role'] == 'assistant' and terminal['status'] == 'completed'
        assert terminal['content'] == [{'type': 'output_text', 'text': row['text'], 'annotations': []}]
        usage = turn['result']['usage']
        messages.append({'id': identity, 'type': 'assistant', 'agent': turn['display']['agent'],
            'model': turn['accepted_model'], 'content': [{'type': 'text', 'text': row['text']}], 'finish': 'stop',
            'tokens': {'input': usage[0], 'output': usage[1], 'reasoning': 0, 'cache': {'read': 0, 'write': 0}},
            'time': {'created': turn['created'], 'completed': turn['completed']}, 'metadata': metadata})
        # Native's real completed terminal execution corresponds to OC2 Idle.
        # Without it OC2 would attribute duration to the nearer DCP user wrapper.
        messages.append({'id': 'msg_vis38_idle_' + row['id'], 'type': 'idle', 'outcome': 'succeeded',
                         'time': {'created': turn['completed']}, 'metadata': {'vis38_native': {'turn_id': turn['native_turn_id'], 'native_status': turn['native_status'], 'time_mapping': 'derived display endpoint'}}})
    traces.append({'native_message_id': row['id'], 'original_message_id': identity,
                   'native_turn_id': turn['native_turn_id'], 'source': 'turns.result + turn_acceptances + conversation_messages',
                   'admission_epoch_ms': turn['created'], 'recorded_duration_ms': turn['display']['duration_ms'],
                   'derived_display_completed_ms': turn['completed']})

latest = by_message[context[-1]['id']]
model = latest['accepted_model']
provider = fixture_config['config']['provider'][model['providerID']]
declaration = provider['models'][model['id']]
assert declaration['name'] == latest['display']['model_label']
session_tokens = {'input': sum(t['display']['usage'][0] for t in turns),
                  'output': sum(t['display']['usage'][1] for t in turns),
                  'reasoning': 0, 'cache': {'read': 0, 'write': 0}}
transfer = {'info': {'id': 'ses_vis38_native_' + session['id'].replace('-', '_'),
    'projectID': 'global', 'agent': latest['display']['agent'], 'model': {**model, 'variant': 'default'},
    'title': session['title'], 'location': {'directory': native_spec['cwd']},
    'time': {'created': int(Decimal(session['created_at']) * 1000), 'updated': int(Decimal(session['updated_at']) * 1000)},
    'cost': 0, 'tokens': session_tokens}, 'messages': messages}
source_paths = [
    ('native clock identity and elapsed display metadata', 'crates/oc-adapters/src/runtime.rs', '867-885,1352-1398'),
    ('native turn acceptance and completed report', 'crates/oc-adapters/src/runtime/turn.rs', '895-921,1843-1854'),
    ('original public transfer', 'opencode/packages/core/src/session/transfer.ts', '64-153'),
    ('original session schema', 'opencode/packages/schema/src/session.ts', '31-59'),
    ('original default session variant', 'opencode/packages/core/src/session/info.ts', '29-35'),
    ('original message/idle schema', 'opencode/packages/schema/src/session-message.ts', '23-36,212-236,283-293'),
    ('original duration attribution', 'opencode/packages/tui/src/routes/session/rows.ts', '353-404'),
    ('original footer terminal requirement', 'opencode/packages/tui/src/routes/session/rows.ts', '326-329'),
    ('original completed response normalization', 'opencode/packages/ai/src/protocols/open-responses.ts', '872-881,1324-1368'),
    ('actual bounded response fixture', 'scripts/tui_capture/dcp_native_qualify.py', '143-181'),
    ('original context usage', 'opencode/packages/tui/src/util/session.ts', '58-72'),
    ('original U34/assistant footer', 'opencode/packages/tui/src/routes/session/index.tsx', '1934-1983,2273-2345'),
]
value = {'schema_version': 1, 'case': args.case, 'goldens_sha256': sha(oracle / 'goldens.json'),
    'owner': association, 'context_snapshot': display_case['context_snapshot'], 'cwd': native_spec['cwd'],
    'cli': fixture_config['cli'], 'model': {**model, 'provider_name': provider['name'], 'declaration': declaration},
    'transfer': transfer, 'field_traces': traces,
    'source_contracts': [{'role': role, 'path': file, 'lines': lines, 'sha256': sha(repo / file)} for role, file, lines in source_paths],
    'mapping_limits': [
        'Native has no absolute per-message created/streamed/completed event timestamps. Created uses the actual source-verified turn-ID epoch; completed is an explicitly derived coordinate preserving the actual stored elapsed display duration, not a fabricated absolute event measurement. Streamed is omitted.',
        'Real completed native turns map to supported OC2 succeeded idle boundaries, preventing DCP display-only user wrappers from resetting turn duration. No failed/interrupted outcome is fabricated.',
        'Actual final response item is a completed assistant message, with no outstanding calls; actual probe emits response.completed with incomplete_details:null. Pinned OpenResponses mapFinishReason normalizes this to stop. This supported finish field is required for the original assistant footer.',
        'Assistant tokens use stored last physical response usage, equal to native display.context_usage here, not aggregated display.usage. Fixture emits no reasoning/cache usage. Session required tokens sum stored per-turn display usage; not a provider billing assertion.',
        'Session required cost is OC2 zero-default scaffolding: no cost was recorded or configured by this native offline fixture. Not measured billing.',
        'Original public import resolves projectID/location and overwrites time.updated with actual import time. Session model null/absent variant becomes default per session/info.ts:34, matching native no explicit variant. Title/agent/model/location and all message data are checked by exact public export.',
        'Native IDs retain an explicit one-to-one source trace under required OC2 msg_/ses_ prefixes. DCP text is display-only user content from unchanged pinned D05, never a native history ingress.',
    ], 'helper_sha256': sha(Path(__file__))}
output.mkdir()
(output / 'fixture.json').write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
print(json.dumps({'output': str(output / 'fixture.json'), 'case': args.case, 'native_messages': len(context),
                  'imported_messages': len(messages), 'title': session['title'], 'actual_durations_ms': [t['display']['duration_ms'] for t in turns]}))
