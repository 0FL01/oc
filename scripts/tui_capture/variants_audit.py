#!/usr/bin/env python3
"""Strict full-frame VIS09/VIS29 audit. Retains all differences, never masks."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from PIL import Image, ImageChops

campaign, report = map(lambda value: Path(value).resolve(), sys.argv[1:])
assert campaign.parent == report.parent
assert report.name.startswith('variants-check') and report.suffix == '.json'
diff_root = report.with_suffix('')
diff_root.mkdir()


def read(file):
    return json.loads(file.read_text())


def sha(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


lock = read(campaign / 'capture.lock.json')
assert lock['source_unchanged'] and lock['native_sha256'] == lock['native_binary_after']
assert lock['fixture_sha256'] == sha(campaign / 'fixture.json')
assert lock['reference_sha256'] == '2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a'
repo = Path(__file__).resolve().parents[2]
assert all(sha(repo / source) == digest for source, digest in lock['native_sources_after'].items()), 'Current native sources differ from capture'
assert sha(repo / 'target/debug/oc') == lock['native_sha256'], 'Current debug binary differs from capture'
source_diff = subprocess.run(['git', 'diff', '--no-ext-diff', '--', 'crates', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'],
                             cwd=repo, check=True, capture_output=True).stdout
(diff_root / 'native-source.patch').write_bytes(source_diff)
fixture = read(campaign / 'fixture.json')
same_controls = {'normal-default', 'cycle-10', 'variant-filtered', 'variant-selected', 'model-initial',
                 'model-scrolled', 'model-no-match', 'model-filtered', 'model-selected', 'model-reopened', 'draft-restored'}
order_frames = {*(f'cycle-{i}' for i in range(10)), *(f'cycle-picker-{i}' for i in range(11)),
                'variant-initial', 'variant-reopened', 'model-selected-variant'}
required_frames = same_controls | order_frames
results, missing, actions = [], [], []
for attempt in lock['attempts']:
    name = attempt['name']
    protocol = read(campaign / name / 'protocol.json')
    launches = [e for e in protocol if e['kind'] == 'launch']
    assert len(launches) == 1 and launches[0]['inherited_environment'] is False
    native = name.startswith('oc-')
    assert launches[0]['history_ingress'] is not native
    assert launches[0]['binary_sha256'] == (lock['native_sha256'] if native else lock['reference_sha256'])
    if not native:
        assert any(e['kind'] == 'exact_import_export_verified' for e in protocol)
    exits = [e for e in protocol if e['kind'] == 'exit']
    successful_exit = len(exits) == 1 and exits[0]['code'] == 0 and exits[0]['termination'] == 'natural' and exits[0]['terminal_restored']
    selection_checks = [c for c in attempt.get('checks', []) if c['label'] != 'wire']
    assert all(c['snapshot']['requests'] == 0 and c['snapshot']['readonly_sqlite'] and c['snapshot']['project_files'] == [] for c in selection_checks)
    if native:
        assert all(o['rows'] == [] for c in selection_checks for o in c['snapshot']['observations'] if o['table'] in ('conversation_messages', 'turns', 'tool_ops', 'turn_acceptances'))
    sequence = fixture['canonical'] if native else fixture['declared']
    expected_cycle = [*sequence, None]
    cycle_checks = [c for c in selection_checks if c['label'].startswith('cycle-')]
    cycle_pass = ([c['expected']['variant'] for c in cycle_checks] == expected_cycle
                  and all(any(v['id'] == fixture['initial_model'] and v.get('variant') == c['expected']['variant'] for v in c['observed']) for c in cycle_checks))
    provider = [e for e in protocol if e['kind'] == 'provider']
    wire_pass = (len(provider) == 1 and provider[0]['valid'] and provider[0]['request']['model'] == fixture['chosen_model']
                 and provider[0]['request']['reasoning']['effort'] == 'low') if native else provider == []
    actions.append({'name': name, 'status': attempt['status'], 'natural_restored_exit': successful_exit,
                    'full_persisted_cycle': cycle_pass, 'selection_requests': 0, 'generation_requests': len(provider),
                    'exact_wire_or_reference_rejection': wire_pass, 'cycle': [c['expected'] for c in cycle_checks]})
for columns, rows in lock['profiles']:
    native_name, reference_name = f'oc-{columns}x{rows}', f'upstream-{columns}x{rows}'
    native = {c['label']: c for c in lock['captures'] if c['name'] == native_name and c['label'] != 'failure'}
    reference = {c['label']: c for c in lock['captures'] if c['name'] == reference_name and c['label'] != 'failure'}
    for side, frames in (('native', native), ('reference', reference)):
        missing.extend({'size': [columns, rows], 'label': label, 'side': side} for label in required_frames - frames.keys())
    missing.extend({'size': [columns, rows], 'label': label, 'side': 'reference' if label in native else 'native'} for label in native.keys() ^ reference.keys())
    for label in sorted(native.keys() & reference.keys()):
        nd, rd = campaign / native_name, campaign / reference_name
        for directory, capture in ((nd, native[label]), (rd, reference[label])):
            for ext, digest in capture['files'].items():
                assert sha(directory / (label + '.' + ext)) == digest
        n, r = read(nd / (label + '.cells.json')), read(rd / (label + '.cells.json'))
        assert n['columns'] == r['columns'] == columns and n['rows'] == r['rows'] == rows
        assert n['scenario'] == r['scenario'] and n['environment_id'] == r['environment_id']
        assert n['fixture_sha256'] == r['fixture_sha256'] == lock['fixture_sha256']
        assert len(n['cells']) == len(r['cells']) == rows
        differences = []
        counts = {'cells': columns * rows, 'unequal': 0, 'symbol': 0, 'fg': 0, 'bg': 0, 'width': 0, 'modifiers': 0}
        for y, (nr, rr) in enumerate(zip(n['cells'], r['cells'], strict=True)):
            assert len(nr) == len(rr) == columns
            for x, (nc, rc) in enumerate(zip(nr, rr, strict=True)):
                if nc == rc:
                    continue
                fields = [field for field in ('symbol', 'fg', 'bg', 'width', 'modifiers') if nc[field] != rc[field]]
                counts['unequal'] += 1
                for field in fields:
                    counts[field] += 1
                differences.append({'x': x, 'y': y, 'fields': fields, 'native': nc, 'original': rc})
        name = f'{columns}x{rows}-{label}'
        if differences:
            (diff_root / (name + '.cells-diff.json')).write_text(json.dumps(differences, indent=2) + '\n')
        ni, ri = Image.open(nd / (label + '.png')).convert('RGB'), Image.open(rd / (label + '.png')).convert('RGB')
        assert ni.size == ri.size
        diff = ImageChops.difference(ni, ri)
        differing_pixels = sum(pixel != (0, 0, 0) for pixel in diff.getdata())
        if differing_pixels:
            diff.save(diff_root / (name + '.png'))
        results.append({'name': name, 'size': [columns, rows], 'label': label, 'grid': counts,
                        'cursor_equal': n['cursor'] == r['cursor'], 'native_cursor': n['cursor'], 'original_cursor': r['cursor'],
                        'png': {'dimensions': ni.size, 'differing_pixels': differing_pixels, 'bbox': diff.getbbox(),
                                'native_sha256': sha(nd / (label + '.png')), 'original_sha256': sha(rd / (label + '.png'))},
                        'full_equality': counts['unequal'] == 0 and n['cursor'] == r['cursor'] and differing_pixels == 0})
actions_pass = len(actions) == 2 * len(lock['profiles']) and all(a['status'] == 'PASS_ACTIONS_CAPTURE' and a['natural_restored_exit'] and a['full_persisted_cycle'] and a['exact_wire_or_reference_rejection'] for a in actions)
equal = sum(r['full_equality'] for r in results)
canonical_pass = lock['order'] == 'canonical' and actions_pass and not missing and equal == len(results) and len(results) == len(lock['profiles']) * 35
unsorted_pass = (lock['order'] == 'unsorted' and actions_pass and not missing and len(results) == len(lock['profiles']) * 35
                 and all(r['full_equality'] == (r['label'] in same_controls) for r in results))
value = {'status': 'PASS_CANONICAL_FULL_PRESENTATION' if canonical_pass else 'PASS_UNSORTED_ACTIONS_WITH_ORDER_DIFFERENCES' if lock['order'] == 'unsorted' and actions_pass and not missing else 'FAIL',
         'campaign': str(campaign), 'capture_lock_sha256': sha(campaign / 'capture.lock.json'), 'order': lock['order'],
         'source_HEAD': lock['source_HEAD'], 'native_sha256': lock['native_sha256'], 'actions_pass': actions_pass,
         'current_native_source_and_binary_verified': True, 'source_patch': str(diff_root / 'native-source.patch'),
         'source_patch_sha256': hashlib.sha256(source_diff).hexdigest(),
         'frame_pairs': len(results), 'equal_full_frame_pairs': equal, 'missing': missing,
         'full_cells': sum(r['grid']['cells'] for r in results), 'unequal_cells': sum(r['grid']['unequal'] for r in results),
         'differing_pixels': sum(r['png']['differing_pixels'] for r in results), 'cursor_differences': sum(not r['cursor_equal'] for r in results),
         'actions': actions, 'pairs': results, 'diffs': str(diff_root),
         'auditor_sha256': sha(Path(__file__)), 'required_frames': sorted(required_frames),
         'unchanged_controls': sorted(same_controls), 'order_affected_frames': sorted(order_frames),
         'unsorted_differences_confined_to_order_states': unsorted_pass if lock['order'] == 'unsorted' else None,
         'qualification': 'All full styled cells, wide continuations, modifiers, cursor and full raster pixels compared without masks. Canonical requires exact equality. Unsorted is approved native rank vs original declaration, not universal pixel PASS.'}
if lock['order'] == 'unsorted' and not unsorted_pass:
    value['status'] = 'FAIL'
with report.open('x') as file:
    json.dump(value, file, indent=2)
    file.write('\n')
print(json.dumps({key: value[key] for key in ('status', 'frame_pairs', 'equal_full_frame_pairs', 'unequal_cells', 'differing_pixels', 'cursor_differences', 'actions_pass')}))
if value['status'] == 'FAIL':
    sys.exit(1)
