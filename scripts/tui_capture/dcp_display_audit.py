#!/usr/bin/env python3
"""Independent full-grid/full-PNG/cursor audit of one source-traced VIS38 stage.

No normalization, cropping, masking, alignment, or tolerance. All differing
styled cells retain their exact one-based coordinates and both complete values.
PNG pixels are decoded and compared over the entire terminal image.
"""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import re

import PIL
from PIL import Image

parser = argparse.ArgumentParser(description=__doc__)
for name in ('native', 'oracle', 'reference', 'fixture', 'output'):
    parser.add_argument('--' + name, required=True, type=Path)
parser.add_argument('--released-sha256', required=True)
args = parser.parse_args()
native, oracle, reference, fixture, output = (getattr(args, n).resolve() for n in ('native', 'oracle', 'reference', 'fixture', 'output'))
assert native.parent == oracle.parent == reference.parent == output.parent
assert re.fullmatch(r'dcp-pair-check[a-z0-9-]+\.json', output.name)


def read(file):
    return json.loads(file.read_text())


def sha(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


owner = read(native / 'result.json')
assert owner['status'] == 'PASS_ACTUAL_SINGLE_MULTI_RESTART_OWNER_PROBE'
assert owner['oc_binary_sha256'] == owner['parent_released_sha256'] == args.released_sha256
for file, digest in owner['evidence_seals'].items():
    assert Path(file).name == file and sha(native / file) == digest, file
association = read(oracle / 'owner-association.json')
assert association['result_sha256'] == sha(native / 'result.json')
assert association['binary_sha256'] == args.released_sha256
manifest = read(oracle / 'source-manifest.json')
assert manifest['dcp_pin'] == '11f6517780a502512a3467645074be447cb0369e'
assert manifest['oc_pin'] == '2670273ff17da96f85c5826ced57aa1b368754fa'
for source in manifest['sources']:
    assert sha(oracle / source['local']) == source['sha256']
goldens = read(oracle / 'goldens.json')
oracle_result = read(oracle / 'result.json')
assert oracle_result['status'] == 'PASS_PINNED_DISPLAY_ORACLE' and oracle_result['body_changes'] is False
assert oracle_result['goldens_sha256'] == sha(oracle / 'goldens.json')
mapping = read(fixture)
assert mapping['owner'] == association and mapping['goldens_sha256'] == sha(oracle / 'goldens.json')
repo = Path(__file__).resolve().parents[2]
for source in mapping['source_contracts']:
    assert sha(repo / source['path']) == source['sha256']
entry = next(c for c in goldens['notifications'] if c['id'] == mapping['case'])
stage = entry['native_context']['stage']
held = next(c for c in owner['display_cases'] if c['stage'] == stage)
assert held['owner_state_unchanged'] and held['provider_requests'] == 0
assert held['typed_snapshot'] == entry['native_context']['typed_snapshot']
assert mapping['context_snapshot'] == held['context_snapshot']
observation = read(native / held['context_snapshot'])


def rows(table):
    return [r for o in observation for r in o['data'][table]]


actual_context = sorted(rows('conversation_messages'), key=lambda r: r['seq'])
assert actual_context == entry['native_context']['messages']
actual_session = next(s for s in rows('sessions') if s['id'] == entry['native_context']['session'])
assert mapping['transfer']['info']['title'] == actual_session['title']
assert mapping['transfer']['info']['time']['created'] == int(Decimal(actual_session['created_at']) * 1000)
actual_turns = {t['id']: t for t in rows('turns')}
mapped_messages = {m['id']: m for m in mapping['transfer']['messages']}
assert len(mapped_messages) == len(mapping['transfer']['messages'])
for trace, original in zip(mapping['field_traces'], actual_context, strict=True):
    assert trace['native_message_id'] == original['id']
    message = mapped_messages[trace['original_message_id']]
    actual_turn = actual_turns[trace['native_turn_id']]
    result = json.loads(actual_turn['result'])
    assert actual_turn['status'] == 'completed'
    assert message['type'] == original['role']
    assert message['time']['created'] == int(actual_turn['id'].rsplit('-', 3)[1])
    assert message['metadata']['vis38_native']['display'] == result['display']
    if message['type'] == 'user':
        assert message['text'] == original['text'] and result['user_message'] == original['id']
    else:
        assert message['content'] == [{'type': 'text', 'text': original['text']}]
        assert message['agent'] == result['display']['agent']
        assert message['model'] == {'providerID': result['provider'], 'id': result['model']}
        assert message['time']['completed'] - message['time']['created'] == result['display']['duration_ms']
        assert message['tokens']['input'] == result['usage'][0] and message['tokens']['output'] == result['usage'][1]
        assert message['finish'] == 'stop' and result['input'][-1]['status'] == 'completed'
        idle = mapped_messages['msg_vis38_idle_' + original['id']]
        assert idle['outcome'] == 'succeeded' and idle['time']['created'] == message['time']['completed']
for notification in goldens['notifications']:
    if notification['native_context']['typed_snapshot']['ordinal'] > entry['native_context']['typed_snapshot']['ordinal']:
        continue
    report = mapped_messages['msg_vis38_dcp_' + str(notification['native_context']['typed_snapshot']['ordinal'])]
    assert report['text'] == notification['payload']
    assert report['metadata']['vis38_native']['operation_id'] == notification['native_context']['operation_id']
    report_index = mapping['transfer']['messages'].index(report)
    assert mapping['transfer']['messages'][report_index + 1]['id'] == 'msg_vis38_native_' + notification['native_context']['messages'][-1]['id']

capture_evidence = read(native / ('native-capture-' + stage + '.json'))
assert capture_evidence['exit_code'] == 0
assert capture_evidence['provider_requests_before'] == capture_evidence['provider_requests_after']
native_dir = Path(capture_evidence['output'])
assert sha(native_dir / 'capture.lock.json') == capture_evidence['lock_sha256']
native_lock, reference_lock = read(native_dir / 'capture.lock.json'), read(reference / 'capture.lock.json')
assert native_lock['binary']['sha256'] == args.released_sha256
assert sha(Path(native_lock['binary']['path'])) == args.released_sha256
assert reference_lock['binary']['sha256'] == '2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a'
assert sha(Path(reference_lock['binary']['path'])) == reference_lock['binary']['sha256']
assert native_lock['frontend'] == reference_lock['frontend']
assert reference_lock['goldens_sha256'] == sha(oracle / 'goldens.json')
assert reference_lock['display_fixture']['sha256'] == sha(fixture)
assert len(native_lock['attempts']) == len(reference_lock['attempts']) == 3
assert all(a['status'] == 'PASS_NATIVE_STAGE_CAPTURE' for a in native_lock['attempts'])
assert all(a['status'] == 'PASS_REFERENCE_CAPTURE' for a in reference_lock['attempts'])
cases, file_seals = [], []
fields = {'text': 'symbol', 'foreground': 'fg', 'background': 'bg', 'modifiers': 'modifiers', 'width': 'width'}
for columns, height in ((80, 24), (120, 40), (160, 48)):
    name = f'{mapping["case"]}-{columns}x{height}'
    directories = [native_dir / name, reference / name]
    grids = []
    exports = []
    for origin, directory, lock in zip(('oc', 'upstream'), directories, (native_lock, reference_lock), strict=True):
        protocol = read(directory / 'protocol.json')
        assert any(e['kind'] == 'exit' and e['code'] == 0 and e['termination'] == 'natural' for e in protocol)
        assert not any(e['kind'] == 'unexpected_provider_request' for e in protocol)
        launch = next(e for e in protocol if e['kind'] == 'launch')
        assert launch['cwd'] == mapping['cwd'] and launch['inherited_env'] is False
        if origin == 'oc':
            assert launch['history_ingress'] is False and launch['binary_sha256'] == args.released_sha256
            assert launch['native_spec']['typed_snapshot'] == held['typed_snapshot']
            assert launch['native_spec']['context_snapshot'] == held['context_snapshot']
        else:
            assert launch['cli'] == mapping['cli']
            assert any(e['kind'] == 'exact_import_export_verified' for e in protocol)
            transfer = next(e['transfer'] for e in protocol if e['kind'] == 'fixture')
            assert transfer == mapping['transfer']
            exported = json.loads(next(e['stdout'] for e in protocol if e['kind'] == 'command' and e['argv'][1:3] == ['session', 'export']))
            assert exported['messages'] == transfer['messages']
            for field in ('title', 'agent', 'model', 'location', 'cost', 'tokens'):
                assert exported['info'][field] == transfer['info'][field]
            exports.append({'time_created': exported['info']['time']['created'], 'time_updated': exported['info']['time']['updated'], 'projectID': exported['info']['projectID']})
        capture = next(c for c in lock['captures'] if c['name'] == name and c['stage'] == 'bottom')
        for extension, digest in capture['files'].items():
            file = directory / ('bottom.' + extension)
            assert sha(file) == digest
            file_seals.append({'file': str(file.relative_to(repo)), 'sha256': digest})
        grid = read(directory / 'bottom.cells.json')
        assert grid['origin'] == origin and grid['columns'] == columns and grid['rows'] == height
        assert len(grid['cells']) == height and all(len(row) == columns for row in grid['cells'])
        render = read(directory / 'bottom.render.json')
        assert render['unmasked'] and render['before']['screen_rect'] == render['after']['screen_rect']
        grids.append(grid)
    counts = {'cells': columns * height, 'unequal': 0, **{field: 0 for field in fields}}
    differences, row_results = [], []
    for y, (native_row, original_row) in enumerate(zip(grids[0]['cells'], grids[1]['cells'], strict=True), 1):
        row_counts = {'row': y, 'unequal': 0, **{field: 0 for field in fields}}
        for x, (a, b) in enumerate(zip(native_row, original_row, strict=True), 1):
            changed = [field for field, key in fields.items() if a[key] != b[key]]
            if a != b:
                counts['unequal'] += 1
                row_counts['unequal'] += 1
                differences.append({'x': x, 'y': y, 'fields': changed, 'native': a, 'original': b})
            for field in changed:
                counts[field] += 1
                row_counts[field] += 1
        row_results.append(row_counts)
    png_paths = [directory / 'bottom.png' for directory in directories]
    with Image.open(png_paths[0]) as a, Image.open(png_paths[1]) as b:
        assert a.size == b.size
        size = a.size
        aa, bb = a.convert('RGBA'), b.convert('RGBA')
        unequal_pixels, first_pixel, bbox = 0, None, None
        for index, (av, bv) in enumerate(zip(aa.getdata(), bb.getdata(), strict=True)):
            if av == bv:
                continue
            x, y = index % size[0] + 1, index // size[0] + 1
            unequal_pixels += 1
            first_pixel = first_pixel or {'x': x, 'y': y, 'native_rgba': av, 'original_rgba': bv}
            bbox = [x, y, x, y] if bbox is None else [min(bbox[0], x), min(bbox[1], y), max(bbox[2], x), max(bbox[3], y)]
    geometry_keys = ('screen_rect', 'measured_cell_width', 'measured_cell_height', 'columns', 'rows', 'device_pixel_ratio')
    renders = [read(d / 'bottom.render.json') for d in directories]
    assert {key: renders[0]['before'][key] for key in geometry_keys} == {key: renders[1]['before'][key] for key in geometry_keys}
    locations = []
    for grid in grids:
        located = {}
        for y, row in enumerate(grid['cells'], 1):
            text = ''.join(c['symbol'] if c['width'] else ' ' for c in row)
            for label, needle in [('dcp_header', '▣ DCP |'), ('dcp_run', '▣ Compression #3 '), ('assistant_footer', 'Build · VIS38 Display Model ·'), ('prompt_metadata', 'Build · VIS38 Display Model VIS38 Display Fixture')]:
                if needle in text:
                    located.setdefault(label, []).append({'x': text.index(needle) + 1, 'y': y})
        locations.append(located)
    cases.append({'name': name, 'size': [columns, height], 'full_grid': counts, 'row_results': row_results,
        'cell_differences': differences, 'cursor': {'native': grids[0]['cursor'], 'original': grids[1]['cursor'], 'equal': grids[0]['cursor'] == grids[1]['cursor']},
        'full_png': {'size': size, 'pixels': size[0] * size[1], 'unequal_pixels': unequal_pixels, 'first_unequal_pixel': first_pixel, 'difference_bbox_inclusive': bbox,
                     'native_sha256': sha(png_paths[0]), 'original_sha256': sha(png_paths[1]), 'bytes_equal': sha(png_paths[0]) == sha(png_paths[1]), 'pixels_equal': unequal_pixels == 0},
        'locations': {'native': locations[0], 'original': locations[1]}, 'public_export_info': exports,
        'terminal_geometry_equal': True, 'unmasked': True})
totals = {field: sum(c['full_grid'][field] for c in cases) for field in ('cells', 'unequal', *fields)}
totals.update({'cursor_unequal_frames': sum(not c['cursor']['equal'] for c in cases),
               'png_equal_frames': sum(c['full_png']['pixels_equal'] for c in cases),
               'png_pixels': sum(c['full_png']['pixels'] for c in cases),
               'png_unequal_pixels': sum(c['full_png']['unequal_pixels'] for c in cases)})
full_display_equal = all(c['full_grid']['unequal'] == 0 and c['cursor']['equal']
                         and c['full_png']['bytes_equal'] and c['full_png']['pixels_equal'] for c in cases)
report = {'status': 'PASS_SOURCE_TRACED_THREE_SIZE_FULL_DISPLAY' if full_display_equal else 'PASS_SOURCE_TRACED_CAPTURE_INTEGRITY_WITH_DIFFERENCES',
    'qualification': 'Only this captured stage at three sizes; not overall VIS38/T44 acceptance. Full display PASS requires every full cell, cursor, PNG byte and decoded PNG pixel to match.',
    'coordinates': 'one-based cells and pixels; inclusive pixel difference bbox', 'normalization': None, 'masked_cells': 0,
    'native_owner': association, 'fixture_sha256': sha(fixture), 'native_capture_lock_sha256': sha(native_dir / 'capture.lock.json'),
    'reference_capture_lock_sha256': sha(reference / 'capture.lock.json'), 'display_provider_requests': 0,
    'native_capture_provider_request_window': [capture_evidence['provider_requests_before'], capture_evidence['provider_requests_after']],
    'totals': totals, 'cases': cases, 'file_seals': file_seals, 'mapping_limits': mapping['mapping_limits'],
    'runner_hashes': {'native': native_lock['runner_hashes'], 'original': reference_lock['runner_hashes']},
    'helper_sha256': sha(Path(__file__)), 'png_decoder': {'name': 'Pillow', 'version': PIL.__version__}}
with output.open('x') as stream:
    json.dump(report, stream, indent=2, ensure_ascii=False)
    stream.write('\n')
print(json.dumps({'output': str(output), 'status': report['status'], 'totals': totals,
                  'cases': [{'name': c['name'], 'full_grid': c['full_grid'], 'cursor_equal': c['cursor']['equal'], 'png_unequal_pixels': c['full_png']['unequal_pixels']} for c in cases]}))
