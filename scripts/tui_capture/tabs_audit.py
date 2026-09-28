#!/usr/bin/env python3
"""Full-frame diagnosis and actual monotonic idle-counter qualification.

Never chooses a synthetic epoch, changes a frame or turns a region diagnostic
into parity. All frame comparisons remain complete-grid/full-PNG comparisons.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reports', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
output = Path(args.output).resolve()
assert output.parent == Path('/home/opencode/ai/oc/evidence/tui/recovery-v00')
result = {'scope': 'T44 VIS41 / VIS39 own-running', 'reports': [], 'attempts': [], 'frames': [],
          'method': 'All complete styled cells/cursor and all PNG pixels are compared without masks/crops/tolerance. Per-row counts are diagnostics only. Exported Linux CLOCK_MONOTONIC draw/wake timestamps qualify actual process-sample windows directly; historical metrics without those timestamps retain only a causal epoch interval, never a fabricated point or global animation clock.'}


def read(file):
    return json.loads(file.read_bytes())


def digest(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()


def message_rhythm(grid):
    """Describe actual reply/caption/next-user rows, without changing a frame."""
    rows = [''.join(cell['symbol'] for cell in row) for row in grid['cells']]
    measured = []
    for y, row in enumerate(rows):
        x = row.find('VIS41-DONE ')
        if x < 0:
            continue
        caption = next((index for index in range(y + 1, len(rows)) if 'Build · VIS41 Fixture ·' in rows[index][x:]), None)
        next_user = next((index for index in range((caption or y) + 1, len(rows))
                          if any(f'VIS41 {case}:' in rows[index][x:] for case in ('seed-a', 'seed-b', 'complete', 'cancel'))), None)
        frame_top = next_user - 1 if next_user is not None and '┃' in rows[next_user - 1][max(0, x - 3):x] else None
        measured.append({'assistant_text_row': y, 'assistant_column': x,
            'assistant_text': row[x:].rstrip(), 'agent_model_caption_row': caption,
            'blank_rows_text_to_caption': caption - y - 1 if caption is not None else None,
            'next_user_text_row': next_user, 'next_user_frame_top_row': frame_top,
            'blank_rows_caption_to_next_user_frame': frame_top - caption - 1 if frame_top is not None and caption is not None else None,
            'qualification': 'Actual complete-grid row coordinates and arithmetic; diagnostic only, no cropped/masked comparison'})
    return measured


def clock_gate(local, attempt):
    metrics_file = local / 'native-ui-metrics.json'
    if not metrics_file.exists():
        return {'gate': 'MISSING_NATIVE_METRICS'}
    metrics = read(metrics_file)
    protocol = read(local / 'protocol.json')
    observations = read(local / 'observations.json')
    launch = next(event for event in protocol if event['kind'] == 'launch')
    home = next((event for event in observations if any('Ask anything' in row for row in event['symbols'])), None)
    complete = len(metrics['frame_samples_ns']) == metrics['frame_count']
    direct = metrics.get('monotonic_epoch_ns') is not None and isinstance(metrics.get('frame_monotonic_ns'), list)
    epoch = None if direct else [launch['at_ns'], home['source_at_ns']] if home else None
    frame_times, wake_times = metrics.get('frame_monotonic_ns', []), metrics.get('wake_monotonic_ns', [])
    frames_complete = direct and complete and len(frame_times) == metrics['frame_count']
    wakes_complete = direct and isinstance(metrics.get('wake_monotonic_ns'), list) and len(wake_times) == metrics['wakeups']
    if direct:
        assert frame_times == sorted(frame_times) and wake_times == sorted(wake_times)
        assert frame_times and frame_times[0] == metrics['monotonic_epoch_ns']
        assert launch['at_ns'] <= frame_times[0]
        assert not home or frame_times[0] <= home['source_at_ns']
    qualified, busy = [], []
    for window in attempt['idle']:
        start, end = window['start']['at_ns'], window['end']['at_ns']
        possible = None if not epoch or not complete else [index for index, (relative, duration) in enumerate(metrics['frame_samples_ns'])
            if epoch[0] + relative < end and epoch[1] + relative + duration > start]
        frame_indices = [index for index, at in enumerate(frame_times) if start <= at < end] if frames_complete else None
        draw_overlap = [index for index, (at, (_, duration)) in enumerate(zip(frame_times, metrics['frame_samples_ns']))
                        if at < end and at + duration > start] if frames_complete else None
        wake_indices = [index for index, at in enumerate(wake_times) if start <= at < end] if wakes_complete else None
        first = next((thread for thread in window['start']['threads'] if thread['tid'] == window['start']['pid']), None)
        last = next((thread for thread in window['end']['threads'] if thread['tid'] == window['end']['pid']), None)
        runtime = None if not first or not last or 'scheduler_runtime_ns' not in first else last['scheduler_runtime_ns'] - first['scheduler_runtime_ns']
        # FrameMetrics is updated by the single UI/main event-loop thread. Zero
        # kernel-accounted execution for that thread certifies no counter work
        # inside the actual read interval, without interpreting PTY writes.
        inner = None if runtime is None else [max(start, first['sample_end_ns']), min(end, last['sample_start_ns'])]
        quiescent = runtime == 0 and first['state'].startswith('S') and last['state'].startswith('S') and inner[0] < inner[1]
        frame_zero = draw_overlap == [] if direct else possible == []
        measured = {'label': window['label'], 'window_ns': [start, end], 'window_semantics': 'half-open [actual process-sample event start, end)',
            'frame_delta': len(frame_indices) if frames_complete else 0 if frame_zero else None,
            'frame_gate': ('ZERO_ACTUAL_MONOTONIC_DRAW_INTERVALS' if frame_zero else 'ACTUAL_MONOTONIC_DRAW_WORK' if draw_overlap is not None else 'INCOMPLETE_MONOTONIC_DRAW_SAMPLES') if direct else 'ZERO_FOR_ALL_CAUSAL_EPOCHS' if frame_zero else 'OPEN_EPOCH_OVERLAP_OR_INCOMPLETE_SAMPLES',
            'frame_start_indices': frame_indices, 'draw_indices_overlapping_window': draw_overlap,
            'wake_delta': len(wake_indices) if wakes_complete else None, 'wake_indices': wake_indices,
            'possible_frame_indices': possible, 'main_thread_runtime_delta_ns': runtime,
            'kernel_certified_inner_interval_ns': inner,
            'main_thread_counter_delta_inside_inner_interval': {'frames': 0, 'wakeups': 0} if quiescent else None,
            'wake_window_gate': ('ZERO_ACTUAL_MONOTONIC_WAKE_SAMPLES' if wake_indices == [] else 'ACTUAL_MONOTONIC_WAKEUPS' if wake_indices is not None else 'INCOMPLETE_MONOTONIC_WAKE_SAMPLES') if direct else 'ZERO_MAIN_THREAD_EXECUTION_INSIDE_ACTUAL_KERNEL_SAMPLE_INTERVAL' if quiescent else 'PENDING_TIMESTAMPED_NATIVE_WAKEUP_SAMPLES',
            'cpu_ticks': window['cpu_ticks'], 'cpu_percent': window['cpu_percent'],
            'all_thread_voluntary_context_switches': window['voluntary_context_switches'],
            'all_thread_involuntary_context_switches': window['involuntary_context_switches'],
            'kernel_quiescent_inside_inner_interval': quiescent}
        (busy if window['label'].startswith('busy') else qualified).append(measured)
    return {'clock': 'LINUX_CLOCK_MONOTONIC_EXPORTED' if direct else 'HISTORICAL_CAUSAL_EPOCH_INTERVAL',
            'actual_monotonic_epoch_ns': metrics.get('monotonic_epoch_ns'), 'epoch_bounds_ns': epoch, 'epoch_bound_width_ms': (epoch[1] - epoch[0]) / 1e6 if epoch else None,
            'epoch_basis': 'Direct native clock_gettime(CLOCK_MONOTONIC) timestamps share the Linux clock with Python time.monotonic_ns; no offset, fitted epoch or animation synchronization.' if direct else 'launch is emitted before Popen; first actual parsed Home output can only follow drawing. Rust samples are relative to the first draw. No point epoch is asserted.',
            'frame_samples_complete': frames_complete if direct else complete, 'wake_samples_complete': wakes_complete,
            'actual_final_counters': {key: metrics[key] for key in ('frame_count', 'changed_frames', 'wakeups', 'input_events', 'worker_events', 'terminal_write_calls', 'terminal_flush_calls', 'terminal_write_bytes')},
            'metrics_sha256': digest(metrics_file), 'windows': qualified,
            'zero_frame_windows': sum(window['frame_gate'] in ('ZERO_ACTUAL_MONOTONIC_DRAW_INTERVALS', 'ZERO_FOR_ALL_CAUSAL_EPOCHS') for window in qualified),
            'zero_full_window_wakeups': sum(window['wake_delta'] == 0 for window in qualified),
            'zero_main_thread_counter_inner_windows': sum(window['kernel_quiescent_inside_inner_interval'] for window in qualified),
            'window_count': len(qualified), 'busy_windows': busy,
            'idle_full_window_gate': 'PASS_ACTUAL_MONOTONIC_ZERO_DRAW_AND_WAKE_WINDOWS' if qualified and direct and frames_complete and wakes_complete and all(window['frame_gate'] == 'ZERO_ACTUAL_MONOTONIC_DRAW_INTERVALS' and window['wake_delta'] == 0 for window in qualified) else 'OPEN_COUNTER_WORK_OR_HISTORICAL_INCOMPLETE_CLOCK'}


for report_file in map(Path, args.reports.split(',')):
    report = read(report_file)
    associations = []
    for directory in report['sides']['native']['directories']:
        lock_file = Path(directory['directory']) / 'capture.lock.json'
        lock = read(lock_file)
        source_files = ['crates/oc-tui/src/app.rs', 'crates/oc-tui/src/history.rs', 'crates/oc-tui/src/layout.rs', 'crates/oc-tui/src/shell.rs', 'crates/oc/src/tui_cmd.rs', 'crates/oc/tests/pty_t39.rs']
        recorded = {file: lock['native_source_manifest_before'][file] for file in source_files}
        current = {file: digest(Path('/home/opencode/ai/oc') / file) for file in source_files}
        associations.append({'directory': directory['directory'], 'capture_lock_sha256': digest(lock_file), 'source_HEAD': lock['source_HEAD'],
            'released_binaries': lock['binaries'], 'native_source_changed_during_capture': lock['native_source_changed'],
            'native_source_sha256': recorded, 'current_source_sha256': current, 'current_source_matches_recorded': recorded == current,
            'runner_hashes': lock['runner_hashes'], 'actual_commands': lock['commands']})
    result['reports'].append({'path': str(report_file), 'sha256': digest(report_file),
        'capture_gate': report['capture_gate'], 'comparison_status_counts': dict(Counter(pair['status'] for pair in report['comparisons'])),
        'summaries': {side: entry['summary'] for side, entry in report['sides'].items()}, 'source_associations': associations})
    reference = {(capture['name'], capture['label']): capture for capture in report['sides']['reference']['captures']}
    native = {(capture['name'], capture['label']): capture for capture in report['sides']['native']['captures']}
    selected = {(item['directory'], item['name']) for item in report['sides']['native']['selected_attempts']}
    for attempt in report['sides']['native']['attempts']:
        if (attempt['directory'], attempt['name']) not in selected:
            continue
        local = Path(attempt['directory']) / attempt['name']
        checks = read(local / 'checks.json')
        hover = []
        for item in checks['hover']:
            base = next((sample['text'] for sample in item['samples'] if sample['source_elapsed_ms'] is not None), None)
            changed = [sample for sample in item['samples'] if base is not None and sample['text'] != base and sample['source_elapsed_ms'] is not None]
            hover.append({key: item[key] for key in ('kind', 'width', 'overflow', 'changed', 'returned_to_entered_text', 'exactly_one_pointer_input', 'owner_rows_unchanged', 'requests_unchanged', 'project_unchanged')}
                | {'first_actual_distinct_text_source_ms': min((sample['source_elapsed_ms'] for sample in changed), default=None),
                   'captured_actual_offsets': item['captures'], 'samples': len(item['samples'])})
        result['attempts'].append({'directory': str(local), 'geometry': checks.get('geometry'), 'compact': checks.get('compact'), 'hover': hover,
            'functional': attempt['functional'], 'temporal': attempt['temporal'], 'settled': attempt['settled'], 'spinner': attempt['spinner'],
            'idle_clock_counters': clock_gate(local, attempt)})
    for pair in report['comparisons']:
        left = reference[(pair['name'], pair['label'])]
        right = native.get((pair['name'].replace('upstream-', 'oc-', 1), pair['label']))
        diagnostic = {'name': pair['name'], 'label': pair['label'], 'status': pair['status'], 'phase_match': pair['phase_match']}
        if not right:
            result['frames'].append(diagnostic)
            continue
        lp = Path(left['directory']) / left['name'] / (left['label'] + '.cells.json')
        rp = Path(right['directory']) / right['name'] / (right['label'] + '.cells.json')
        a, b = read(lp), read(rp)
        rows, fields, samples = [], Counter(), []
        for y in range(max(len(a['cells']), len(b['cells']))):
            ar = a['cells'][y] if y < len(a['cells']) else []
            br = b['cells'][y] if y < len(b['cells']) else []
            count = 0
            for x in range(max(len(ar), len(br))):
                ac, bc = ar[x] if x < len(ar) else None, br[x] if x < len(br) else None
                if ac == bc:
                    continue
                count += 1
                keys = ['missing_reference_cell'] if ac is None else ['missing_native_cell'] if bc is None else [key for key in dict.fromkeys((*ac, *bc)) if ac.get(key) != bc.get(key)]
                fields.update(keys)
                if len(samples) < 12:
                    samples.append({'x': x, 'y': y, 'fields': keys, 'reference': ac, 'native': bc})
            if count:
                rows.append({'y': y, 'different_cells': count})
        diagnostic.update({'reference_cells': str(lp), 'native_cells': str(rp), 'different_cells_per_row': rows,
            'dimensions_reference': [a['columns'], a['rows']], 'dimensions_native': [b['columns'], b['rows']],
            'different_field_counts': dict(fields), 'different_cells_full_grid': sum(row['different_cells'] for row in rows),
            'cursor_reference': a['cursor'], 'cursor_native': b['cursor'], 'samples': samples,
            'message_rhythm_reference': message_rhythm(a), 'message_rhythm_native': message_rhythm(b),
            'target_title_phase_match': left.get('observed_offset') == right.get('observed_offset') and left.get('expected_offset') == right.get('expected_offset') and left.get('title') == right.get('title'),
            'independent_tab_glyphs_reference': pair.get('reference_observed_tab_glyphs'), 'independent_tab_glyphs_native': pair.get('native_observed_tab_glyphs'),
            'qualification': 'Full-grid field diagnosis only, including independent glyph-phase mismatches; not cropped comparison or PASS'})
        result['frames'].append(diagnostic)
idle = [window for attempt in result['attempts'] for window in attempt['idle_clock_counters'].get('windows', [])]
result['summary'] = {
    'native_attempts': len(result['attempts']), 'reference_frame_comparison_records': len(result['frames']),
    'comparison_status_counts': dict(Counter(frame['status'] for frame in result['frames'])),
    'matching_actual_title_phase_records': sum(frame.get('target_title_phase_match', False) for frame in result['frames']),
    'matching_actual_tab_glyph_phase_records': sum(frame.get('independent_tab_glyphs_reference') == frame.get('independent_tab_glyphs_native') for frame in result['frames'] if 'cursor_reference' in frame),
    'identical_cursor_records': sum(frame['cursor_reference'] == frame['cursor_native'] for frame in result['frames'] if 'cursor_reference' in frame),
    'cursor_difference_examples': [{'name': frame['name'], 'label': frame['label'], 'reference': frame['cursor_reference'], 'native': frame['cursor_native']}
        for frame in result['frames'] if 'cursor_reference' in frame and frame['cursor_reference'] != frame['cursor_native']][:8],
    'identical_message_rhythm_records': sum(frame['message_rhythm_reference'] == frame['message_rhythm_native'] for frame in result['frames'] if 'cursor_reference' in frame),
    'native_full_monotonic_idle_windows': len(idle),
    'zero_draw_and_wake_full_windows': sum(window['frame_gate'] == 'ZERO_ACTUAL_MONOTONIC_DRAW_INTERVALS' and window['wake_delta'] == 0 for window in idle),
    'all_actual_process_idle_cpu_ticks_zero': all(window['cpu_ticks'] == 0 for window in idle),
    'all_native_clock_samples_complete': all(attempt['idle_clock_counters'].get('frame_samples_complete') and attempt['idle_clock_counters'].get('wake_samples_complete') for attempt in result['attempts']),
    'busy_progress_windows': [{'name': Path(attempt['directory']).name, 'label': window['label'], 'frame_starts': window['frame_delta'], 'wakeups': window['wake_delta'], 'cpu_ticks': window['cpu_ticks']}
        for attempt in result['attempts'] for window in attempt['idle_clock_counters'].get('busy_windows', [])],
    'qualification': 'Counts describe retained whole-frame comparisons and real timestamped counters; equal row rhythm/cursor/glyph counts alone never establish pixel parity'}
with output.open('x') as file:
    json.dump(result, file, indent=2, ensure_ascii=False)
    file.write('\n')
print(json.dumps({'output': str(output), 'reports': len(result['reports']), 'attempts': len(result['attempts']), 'frames': len(result['frames']),
    'summary': result['summary'],
    'source_associations_current': all(association['current_source_matches_recorded'] for report in result['reports'] for association in report['source_associations']),
    'equal_full_frames': sum(frame['status'] == 'EQUAL' for frame in result['frames']),
    'horizontal_header_diagnostics': [{'name': frame['name'], 'label': frame['label'], 'different_cells_on_row_zero': next((row['different_cells'] for row in frame.get('different_cells_per_row', []) if row['y'] == 0), 0)}
        for frame in result['frames'] if '-horizontal-' in frame['name'] and frame['label'] in ('long-offset-1', 'unicode-offset-2', 'long-settled', 'running-glyph-280b')],
    'clock_gates': [{'name': Path(attempt['directory']).name, 'zero_draw_windows': attempt['idle_clock_counters'].get('zero_frame_windows'),
         'zero_main_thread_counter_inner_windows': attempt['idle_clock_counters'].get('zero_main_thread_counter_inner_windows'),
         'zero_full_window_wakeups': attempt['idle_clock_counters'].get('zero_full_window_wakeups'),
         'idle_full_window_gate': attempt['idle_clock_counters'].get('idle_full_window_gate'),
        'windows': attempt['idle_clock_counters'].get('window_count')} for attempt in result['attempts']]}))
