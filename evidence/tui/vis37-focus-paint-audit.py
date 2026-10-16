"""Independent current005 whole-frame recount and every-byte manual inverse."""
import sys
sys.dont_write_bytecode = True
import importlib.util
import json
import os
from pathlib import Path
import resource

resource.setrlimit(resource.RLIMIT_AS, (384 * 1024**2, 384 * 1024**2))
REPO = Path('/home/opencode/ai/oc')
TMP = Path('/home/opencode/.cache/opencode-tmp/opencode')
OUT = REPO / 'evidence/tui'
HERE = TMP / 'vis37-form-independent-20261012-sole-verifier'
spec = importlib.util.spec_from_file_location('immutable_question_observer', HERE / 'owner_refinement.py')
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)
v, check = observer.v, observer.v.inverse
BASE = '30f4e707e5c3b870d3ad8b571d321ac601ec8c4a'
v.ROOTS = {**{'current-' + p: TMP / 'vis37-form-current-005' / p for p in ('small', 'wide', 'tall')},
           'current-logs': TMP / 'vis37-form-current-005', 'current-driver': TMP / 'vis37-form-capture-r5.py'}
check.ROOTS = v.ROOTS
check.HEAD = v.frames.HEAD = BASE
STAGES = list(check.STAGES)
STAGES.insert(3, 'question-multiple-focused')
BEFORE = OUT / 'vis37-focus-paint-before.json'
AFTER = OUT / 'vis37-focus-paint-after.json'


def save(path, value):
    data = (json.dumps(value, ensure_ascii=False, separators=(',', ':')) + '\n').encode()
    check.require(len(data) <= check.FILE and check.quota()['headroom_bytes'] >= (len(data) + 4095) // 4096 * 4096,
                  'bounded independent report within unchanged cap')
    with path.open('xb') as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps(dict(output=str(path), bytes=len(data), sha256=check.sha(data), status=value['status'],
                          totals=value.get('totals'), memory=check.proc_memory(), quota=check.quota())))


def strict_commands(episode, lock, commands):
    comparisons = [c for c in commands if len(c['argv']) > 1 and
                   c['argv'][1] == str(REPO / 'tui-recovery/scripts/compare_frames.py')]
    check.require(len(comparisons) == len(lock['attempts']) == 36, 'all exact executed strict comparisons')
    for attempt in lock['attempts']:
        name, mode, status = attempt['scenario'], attempt['mode'], attempt['status']
        check.require(mode in ('grid', 'png') and status in ('EQUAL', 'DIFFERENT'), 'complete actual comparisons only')
        suffix = '.cells.json' if mode == 'grid' else '.png'
        reference, actual = episode / 'upstream' / (name + suffix), episode / 'oc' / (name + suffix)
        report = episode / (name + '.' + mode + '-diff.json')
        argv = ['/usr/bin/python3', str(REPO / 'tui-recovery/scripts/compare_frames.py'), mode,
                str(reference), str(actual), '--report', str(report)]
        found = [c for c in comparisons if c['argv'] == argv]
        check.require(len(found) == 1 and found[0]['exit_code'] == attempt['exit_code'] == (0 if status == 'EQUAL' else 1),
                      'exact strict argv/result, no mask/crop/tolerance or assumed failure')
        recorded = check.load(report)
        check.require(json.loads(found[0]['stdout']) == recorded and recorded['kind'] == mode and
                      recorded['status'] == ('PASS' if status == 'EQUAL' else 'FAIL') and
                      recorded['reference_sha256'] == check.digest(reference) and recorded['actual_sha256'] == check.digest(actual),
                      'exact comparator input/output association')
        if mode == 'grid':
            check.require(recorded['scenario'] == name and recorded['fixture_sha256'] == lock['fixture_sha256'] and
                          recorded['environment_id'] == v.frames.producer_seal(lock['profile']) and
                          recorded['reference_commit'] == check.DONOR and recorded['actual_commit'] == BASE,
                          'full styled-grid provenance')


def source_and_elf():
    source = v.source()
    check.require(len(source) == 713, 'all actual713 source inputs')
    prior = check.load(TMP / 'vis37-form-current-004/wide/source-manifest.json')
    changed = sorted(name for name in source if prior.get(name) != source[name])
    check.require(set(prior) == set(source) and changed == ['scripts/tui_capture/question_forms.mjs'],
                  'only the probe changed; native Rust/config/compiler inputs preserved')
    elf = {}
    for profile in ('small', 'wide', 'tall'):
        episode = v.ROOTS['current-' + profile]
        lock, manifest, commands = (check.load(episode / n) for n in ('capture.lock.json', 'source-manifest.json', 'commands.json'))
        check.require(manifest == source and check.producer_sha(source) == lock['oc']['source_manifest_sha256'] and
                      lock['oc']['commit'] == BASE, 'complete actual current source and original producer identity')
        builds = [item for item in commands if item['argv'][0] == 'cargo']
        check.require(len(builds) == 1 and builds[0]['argv'] == ['cargo', 'build', '--locked'] and builds[0]['exit_code'] == 0,
                      'one exact captured repository build')
        strict_commands(episode, lock, commands)
        old = check.load(TMP / 'vis37-form-current-004' / profile / 'capture.lock.json')
        for side in ('oc', 'upstream'):
            actual = v.frames.elf(Path(lock[side]['executable_path']))
            check.require(actual['sha256'] == lock[side]['executable_sha256'] == old[side]['executable_sha256'],
                          'same actual native/reference ELF, no runtime cursor anchor')
            elf[side] = actual
    return dict(source_manifest_sha256=check.producer_sha(source), changed_inputs=changed, elf=elf)


def effects(episode, side):
    checks, protocol, inputs = (check.load(episode / side / n) for n in ('tool-preview-checks.json', 'protocol.json', 'inputs.json'))
    check.require(checks['status'] == 'PASS_BEHAVIOR_ONLY' and [s['stage'] for s in checks['stages']] == STAGES and
                  [s['requests'] for s in checks['stages']] == [0] + [2]*8 + [3]*5 + [4]*4 and
                  [s['calls'] for s in checks['stages']] == [0] + [1]*13 + [2]*4,
                  'all18 real stages, no focus/edit/review continuation')
    expected = [('question_python_focus', 'G1tC'), ('question_select_python', 'IA=='),
                ('question_empty_custom_tab', 'CQ=='), ('question_empty_custom_backtab', 'G1ta'),
                ('question_custom_tab_commit', 'CQ=='), ('question_custom_backtab', 'G1ta'),
                ('question_custom_reopen_edit', 'DQ=='), ('question_commit_custom', 'DQ=='), ('question_explicit_reply', 'DQ==')]
    check.require([(p['kind'], p['base64']) for p in inputs if p['kind'] in dict(expected)] == expected and
                  checks['question_forms']['multiselect_focus_painted_before_toggle'] is True,
                  'real separate focus/toggle, optional-empty/custom navigation and explicit reply bytes')
    providers = [p for p in protocol if p['kind'] == 'provider']
    mains = [p for p in providers if p['operation'] == 'question_main']
    calls = [p for p in protocol if p['kind'] == 'fixture_tool_call']
    check.require(len(providers) == 4 and len(mains) == 3 and all(p['valid'] for p in providers) and
                  [p['user_texts'] for p in mains] == [[check.PROMPT], [check.PROMPT], [check.PROMPT, check.CANCEL]] and
                  all(p['registered_tools'] == ['question'] for p in mains) and
                  [p['call_id'] for p in calls] == ['call_vis37_answer', 'call_vis37_cancel'] and
                  all(p['arguments'] == check.QUESTIONS for p in calls), 'typed request/call binding and no replay')
    result = mains[1]['actual_results']
    check.require(mains[0]['actual_results'] == [] and result == mains[2]['actual_results'] and len(result) == 1 and
                  result[0]['call_id'] == 'call_vis37_answer' and
                  (json.loads(result[0]['output']) == {**check.QUESTIONS, 'answers': check.ANSWERS} if side == 'oc' else
                   result[0]['output'] == check.DONOR_RESULT), 'one actual owned result, not a guessed answer')
    snapshots = [p for p in protocol if p['kind'] == 'tool_preview_snapshot']
    check.require(len(snapshots) == 5 and snapshots == [checks['question_forms'][k] for k in
                  ('completed', 'reopened', 'restarted', 'dismissed', 'final')] and
                  snapshots[0]['observations'] == snapshots[1]['observations'] == snapshots[2]['observations'] and
                  snapshots[3]['observations'] == snapshots[4]['observations'], 'actual RAW/IDs preserved through viewing/restart')
    for index, snapshot in enumerate(snapshots):
        check.require(snapshot['artifacts'] == {} and snapshot['shell_effect'] is None and
                      snapshot['mcp_calls'] == snapshot['mcp_lifecycle'] == [], 'no extra observed effects')
        records = [o['data']['question_records'] for o in snapshot['observations'] if 'question_records' in o['data']]
        check.require(len(records) == 1 and records[0]['readonly'] and len(records[0]['operations']) == (1 if index < 3 else 2),
                      'real readonly owner records')
        operations = records[0]['operations']
        if side == 'oc':
            check.require(operations[0]['provider_call_id'] == 'call_vis37_answer' and operations[0]['state'] == 'completed' and
                          operations[0]['output'] == result[0]['output'], 'durable native typed answer')
            if index >= 3:
                check.require(operations[1]['provider_call_id'] == 'call_vis37_cancel' and operations[1]['state'] == 'cancelled' and
                              operations[1]['output'] == 'error: question dismissed', 'native cancellation is not an answer')
        else:
            check.require(operations[0]['data']['id'] == 'call_vis37_answer' and
                          operations[0]['data']['state']['metadata']['answers'] == check.ANSWERS, 'actual Original V2 answer')
            for operation in operations:
                message = next(m for m in records[0]['messages'] if m['id'] == operation['message_id'])
                check.require(json.loads(message['data'])['content'][operation['part_index']] == operation['data'], 'source part/RAW IDs bound')
            if index >= 3:
                check.require(operations[1]['data']['state']['status'] == 'error' and
                              not operations[1]['data']['state'].get('metadata', {}).get('answers'), 'honest Original dismissal')
    exits = [p for p in protocol if p['kind'] == 'exit']
    check.require(len(exits) == 2 and exits[0]['generation'] == 0 and exits[0]['code'] == 0 and
                  exits[1]['generation'] == 1 and exits[1]['termination'] == 'forced_stop', 'honest natural/forced lifecycle scope')
    return dict(valid_provider_requests=4, main_requests=3, title_requests=1, question_calls=2,
                explicit_user_submissions=2, explicit_answer_submissions=1, readonly_snapshots=5)


check.require(os.getuid() == 1003 and check.git('rev-parse', 'HEAD').decode().strip() == BASE, 'owned current base')
helper_paths = [Path(__file__), HERE / 'verifier.py', HERE / 'owner_refinement.py', *sorted(v.OLD_HELPERS.glob('*.py'))]
helpers = {str(p): check.digest(p) for p in helper_paths}
originals, source = v.inventory(), source_and_elf()
if sys.argv[1:] == ['before']:
    prior = {str(p): dict(sha256=check.digest(p), metadata=check.metadata(p)) for p in sorted(OUT.iterdir())
             if p.name.endswith(('.tar.xz', '.inventory.json'))}
    save(BEFORE, dict(status='INDEPENDENT_CURRENT005_SEALED_BEFORE', originals=originals, source=source, helpers=helpers, prior=prior))
elif sys.argv[1:] == ['after']:
    before = check.load(BEFORE)
    check.require(originals == before['originals'] and source == before['source'] and helpers == before['helpers'], 'all originals/meta/source/helpers unchanged')
    archive, receipt_path = OUT / 'vis37-form-r5-proof.tar.xz', OUT / 'vis37-form-r5-proof.inventory.json'
    receipt = check.load(receipt_path)
    raw, xz = check.inverse_xz(check.read_bytes(archive))
    restored, inverse = check.inverse_ustar(raw, originals['files'])
    check.require(check.digest(archive) == receipt['archive_sha256'] and check.sha(raw) == receipt['tar_sha256'] and
                  inverse['inventory'] == receipt['inventory'], 'complete manual inverse against every original byte')
    del restored, raw
    pairs, facts = [], {}
    for profile in ('small', 'wide', 'tall'):
        episode = v.ROOTS['current-' + profile]
        facts.update({profile + '-' + side: effects(episode, side) for side in ('oc', 'upstream')})
        for stage in STAGES:
            name = 'tool-preview-' + stage
            grids = {s: check.load(episode / s / (name + '.cells.json')) for s in ('oc', 'upstream')}
            metric, _ = v.frames.compare_grid(grids['upstream'], grids['oc'])
            png = v.frames.compare_png(episode / 'upstream' / (name + '.png'), episode / 'oc' / (name + '.png'))
            for mode, value in (('grid', metric), ('png', png)):
                recorded = check.load(episode / (name + '.' + mode + '-diff.json'))
                check.require(all(recorded[k] == value[k] for k in value if k != 'different_fields'), 'strict full-frame independent recount')
            pair = dict(profile=profile, stage=stage, grid=metric, png=png)
            if stage in STAGES[1:9] + ['question-cancel-pending']:
                pair['pending_owner'] = v.owner(grids, 'Questions')
            if stage == 'question-answered-card':
                pair['answered_owner'] = v.owner(grids, '# Questions')
            pairs.append(pair)
    totals = dict(paired_frames=len(pairs), whole_pngs=len(pairs)*2, checked_cells=sum(p['grid']['cells_checked'] for p in pairs),
                  checked_pixels=sum(p['png']['pixels_checked'] for p in pairs), different_cells=sum(p['grid']['different_cells'] for p in pairs),
                  different_pixels=sum(p['png']['different_pixels'] for p in pairs), equal_cursor_pairs=sum(not p['grid']['cursor_differs'] for p in pairs),
                  pending_owner_equal_pairs=sum(p.get('pending_owner', {}).get('all_owner_styled_cells_equal', False) for p in pairs),
                  answered_owner_equal_pairs=sum(p.get('answered_owner', {}).get('all_owner_styled_cells_equal', False) for p in pairs))
    check.require(v.inventory() == originals and source_and_elf() == source and
                  all(check.digest(p) == sha for p, sha in helpers.items()) and
                  all(check.digest(p) == row['sha256'] and check.metadata(p) == row['metadata'] for p, row in before['prior'].items()),
                  'all capture bytes/meta, sources, ELFs, helpers and prior proof archives preserved')
    save(AFTER, dict(status='INDEPENDENT_CURRENT005_FULL_BYTE_EXACT_AND_RECOUNT_T44_ACTIVE_NOT_PASS', totals=totals,
                     inverse=inverse['inventory'], xz=xz, archive_sha256=receipt['archive_sha256'], source=source, helpers=helpers,
                     preserved_prior_proofs=len(before['prior']), effects=facts, pairs=pairs, memory=check.proc_memory(),
                     scope='Separate actual focus paint, no hidden-cursor anchor/normalization. Whole frames still differ; DCP tables unread; final generation1 forced.'))
else:
    raise SystemExit('before or after required; existing reports are immutable')
