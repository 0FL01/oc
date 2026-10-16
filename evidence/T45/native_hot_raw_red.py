#!/usr/bin/env python3
"""Offline R9 RED: actual unmodified normal ELF, growing one current turn.

Only this runner's exact TemporaryDirectory is removed, after process and all
HTTP handlers join. No auth/config or transcript is published in measurements.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('round_fixture', ROOT/'evidence/T45/native_round_removal.py')
round_fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(round_fixture)
t54 = round_fixture.t54


def process_metrics(pid):
    status = Path(f'/proc/{pid}/status').read_text()
    values = {}
    for line in status.splitlines():
        key, _, value = line.partition(':')
        if key in ('VmRSS', 'VmHWM'):
            values[key+'_bytes'] = int(value.split()[0])*1024
    for line in Path(f'/proc/{pid}/io').read_text().splitlines():
        key, _, value = line.partition(':')
        if key in ('rchar', 'wchar', 'read_bytes', 'write_bytes', 'syscr', 'syscw'):
            values[key] = int(value)
    return values


class Growing(round_fixture.OwnedFixture):
    def __init__(self, binary, groups, overflow):
        self.groups, self.overflow = groups, overflow
        self.issued = 0
        self.process = None
        self.pid_ready = threading.Event()
        self.measurements = []
        self.output_bytes = 8192
        self.done = False
        super().__init__(binary, 'effect')
        config = self.home/'config/opencode/opencode.json'
        settings = json.loads(config.read_text())
        settings['compaction'] = {'auto': overflow, 'buffer': 24000}
        settings['provider']['fixture']['models']['fixture-model']['limit']['context'] = 32000 if overflow else 500000
        config.write_text(json.dumps(settings))

    def response(self, lane, number):
        if lane == 'title':
            return 200, {}, t54.completed('R9 baseline')
        if lane == 'compaction':
            return 200, {}, t54.completed('## Objective\nKeep TASK_CONTROL and newest selected fact. Closed work is complete.')
        assert lane == 'main'
        assert self.pid_ready.wait(5)
        request = self.lanes[lane][-1]
        outputs = [i for i in request['input'] if i.get('type') == 'function_call_output']
        assert len(outputs) == self.issued
        if self.issued:
            assert outputs[-1]['call_id'] == 'closed-'+str(self.issued)
            envelope, public = outputs[-1]['output'].split('\n', 1)
            assert envelope == 'exit 0'
            assert len(public.encode()) == self.output_bytes
        assert 'TASK_CONTROL' in json.dumps(request['input'])
        with self.db() as db:
            checkpoint_bytes, input_items, requests, spans = db.execute(
                "SELECT length(CAST(result AS BLOB)),json_array_length(result,'$.input'),"
                "json_array_length(result,'$.requests'),json_array_length(result,'$.spans') "
                "FROM turns ORDER BY rowid DESC LIMIT 1").fetchone()
        databases = list((self.home/'data').rglob('oc.sqlite'))
        assert len(databases) == 1
        database = databases[0]
        wal = Path(str(database)+'-wal')
        wal_bytes = wal.stat().st_size if wal.exists() else 0
        self.measurements.append({
            'closed_groups': self.issued,
            'wire_json_bytes': len(json.dumps(request, separators=(',', ':')).encode()),
            'checkpoint_serialized_bytes': checkpoint_bytes,
            'hot_input_items': input_items, 'hot_requests': requests, 'hot_spans': spans,
            'db_bytes': database.stat().st_size,
            'wal_bytes': wal_bytes, 'wal_frames': max(0, (wal_bytes-32)//4120),
            **process_metrics(self.process.pid),
        })
        if self.issued == self.groups:
            self.done = True
            return 200, {}, t54.completed('TASK_CONTROL; newest selected fact=done')
        self.issued += 1
        # One committed effect. All later output has exactly the same bounded
        # size. The provider needs only TASK_CONTROL and the newest output.
        command = ('printf effect >> r9-effects; ' if self.issued == 1 else '')
        command += "printf '%8192s' 'SELECTED_FACT'"
        return 200, {}, round_fixture.tool('closed-'+str(self.issued), 'bash', {'argv': ['sh', '-c', command]})


def baseline(binary, groups, overflow=False):
    fixture = Growing(binary, groups, overflow)
    owned = str(fixture.root)
    before = time.monotonic()
    try:
        fixture.process = subprocess.Popen(
            [str(binary), 'run', '--json', 'TASK_CONTROL; execute closed groups; keep only newest selected fact'],
            cwd=fixture.project, env=fixture.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True)
        fixture.pid_ready.set()
        stdout, stderr = fixture.process.communicate(timeout=60)
        text = (stdout+stderr).decode()
        fixture.safe(text)
        assert (fixture.project/'r9-effects').read_text() == 'effect'
        with fixture.db() as db:
            status, raw = db.execute('SELECT status,result FROM turns ORDER BY rowid LIMIT 1').fetchone()
            states = list(db.execute('SELECT state,count(*) FROM tool_operations GROUP BY state'))
            counted = dict(db.execute("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
            snapshots = list(db.execute("SELECT json_extract(snapshot,'$.state'),json_extract(snapshot,'$.error') FROM session_compactions"))
        sockets = {lane: len(rows) for lane, rows in fixture.lanes.items()}
        assert sum(counted.values()) == sum(sockets.values()), (counted, sockets)
        assert all(state == 'completed' for state, _ in states)
        completed = sum(n for _, n in states)
        log = json.loads(raw)
        closed_outputs = [i for i in log['input'] if i.get('type') == 'function_call_output']
        assert len(closed_outputs) == completed
        assert [i['call_id'] for i in closed_outputs] == ['closed-'+str(i) for i in range(1, completed+1)]
        if overflow:
            assert fixture.process.returncode == 1 and not fixture.done
            assert sockets['compaction'] == 0 and not snapshots
            assert 'context' in text.lower() or 'budget' in text.lower(), 'expected truthful budget diagnostic'
        else:
            assert fixture.process.returncode == 0 and status == 'completed' and fixture.done
            assert completed == groups
        final_checkpoint = len(raw.encode())
        results = {
            'case': 'current-only-compact-unreachable' if overflow else 'growing-current-task',
            'requested_groups': groups, 'settled_groups': completed,
            'exit': fixture.process.returncode, 'turn_status': status,
            'seconds': round(time.monotonic()-before, 3),
            'effect_count': 1, 'physical_posts': sockets, 'dispatches': counted,
            'compaction_snapshots': snapshots,
            'raw_bytes': final_checkpoint, 'raw_sha256': hashlib.sha256(raw.encode()).hexdigest(),
            'raw_input_items': len(log['input']), 'raw_requests': len(log['requests']), 'raw_spans': len(log['spans']),
            'first_boundary': fixture.measurements[0], 'last_boundary': fixture.measurements[-1],
            'max_VmRSS_bytes': max(v['VmRSS_bytes'] for v in fixture.measurements),
            'max_VmHWM_bytes': max(v['VmHWM_bytes'] for v in fixture.measurements),
            'checkpoint_bytes_all_observed_boundaries': sum(v['checkpoint_serialized_bytes'] or 0 for v in fixture.measurements),
            'loaded_raw_rows_bytes': 'not instrumented; baseline current checkpoint remains entire journal',
            'equal_newest_selected_fact': 'SELECTED_FACT',
            'bounded_hot': False, 'result': 'RED',
            'exact_owned_temp': owned,
        }
    finally:
        if fixture.process is not None and fixture.process.poll() is None:
            os.killpg(fixture.process.pid, signal.SIGKILL)
            fixture.process.wait(timeout=5)
        fixture.close()
        assert not fixture.thread.is_alive() and not Path(owned).exists()
        print(json.dumps({'owner_cleanup': owned, 'process_reaped': True,
                          'http_handlers_joined': True, 'exact_temp_removed': True}), flush=True)
    results['process_and_http_joined'] = True
    results['exact_owned_temp_removed'] = True
    return results


def main():
    binary = (ROOT/(sys.argv[1] if len(sys.argv) > 1 else 'target/debug/oc')).resolve()
    assert binary in (ROOT/'target/debug/oc', ROOT/'target/release/oc')
    executable = binary.read_bytes()
    assert executable[:4] == b'\x7fELF'
    digest = hashlib.sha256(executable).hexdigest()
    for groups, overflow in ((8, False), (32, False), (32, True)):
        print(json.dumps({'binary': str(binary.relative_to(ROOT)), 'sha256': digest, **baseline(binary, groups, overflow)}), flush=True)
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == digest


if __name__ == '__main__':
    main()
