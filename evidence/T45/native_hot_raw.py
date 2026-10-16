#!/usr/bin/env python3
"""Owned normal-ELF R9 current-turn compaction/RAW continuation experiment."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('red', ROOT/'evidence/T45/native_hot_raw_red.py')
red = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(red)


class Growing(red.Growing):
    def __init__(self, binary, groups, child=False):
        self.child = child
        self.hashes = {}
        self.late_retry = False
        self.partial_retry = False
        super().__init__(binary, groups, True)

    def response(self, lane, number):
        if lane == 'title':
            return 200, {}, red.t54.completed('Current task')
        if lane == 'compaction':
            if number == 1:
                return 429, {'retry-after-ms': '0'}, b'{"error":{"code":"rate_limit_exceeded"}}'
            request = self.lanes[lane][-1]
            assert request['tools'] == []
            assert 'TASK_CONTROL' in json.dumps(request['input'])
            return 200, {}, red.t54.completed('## Objective\nTASK_CONTROL.\n## Work State\nCommitted effect is complete; newest selected working fact is SELECTED_FACT. Continue the same task.')
        if self.child and lane == 'main':
            if number == 1:
                return 200, {}, red.round_fixture.tool('owned-child','subagent',{'agent':'helper','prompt':'TASK_CONTROL; continue closed groups and consolidate newest working fact','description':'Owned R9 child'})
            assert number == 2
            return 200, {}, red.t54.completed('Parent received settled child')
        assert lane == ('child' if self.child else 'main')
        assert self.pid_ready.wait(5)
        request = self.lanes[lane][-1]
        outputs = [i for i in request['input'] if i.get('type') == 'function_call_output']
        assert len(outputs) <= 3, ('old raw on wire', len(outputs))
        assert 'TASK_CONTROL' in json.dumps(request['input'])
        if number == 1 or (self.issued == 4 and not self.late_retry):
            self.late_retry |= self.issued == 4
            return 429, {'retry-after-ms': '0'}, b'{"error":{"code":"rate_limit_exceeded"}}'
        if self.issued == 6 and not self.partial_retry:
            self.partial_retry = True
            return 200, {}, red.t54.event({'type':'response.output_text.delta','delta':'settled partial'}) + red.t54.event({'type':'response.failed','response':{'error':{'code':'rate_limit_exceeded'}}})
        with self.db() as db:
            raw = db.execute('SELECT result FROM turns ORDER BY rowid DESC LIMIT 1').fetchone()[0]
            segments = list(db.execute('SELECT ordinal,payload FROM turn_raw_segments ORDER BY ordinal'))
            for ordinal, payload in segments:
                digest = hashlib.sha256(payload.encode()).hexdigest()
                assert self.hashes.setdefault(ordinal, digest) == digest, 'old RAW rewritten'
        log = json.loads(raw)
        database = next((self.home/'data').rglob('oc.sqlite'))
        wal = Path(str(database)+'-wal')
        wal_bytes = wal.stat().st_size if wal.exists() else 0
        self.measurements.append({
            'closed_groups': self.issued,
            'wire_json_bytes': len(json.dumps(request,separators=(',',':')).encode()),
            'checkpoint_serialized_bytes': len(raw.encode()),
            'hot_input_items': len(log['input']), 'hot_requests': len(log['requests']),
            'hot_spans': len(log['spans']), 'raw_segments': len(segments),
            'history_read_counters':log['display']['history_read_counters'],
            'db_bytes': database.stat().st_size, 'wal_bytes': wal_bytes,
            'wal_frames': max(0,(wal_bytes-32)//4120), **red.process_metrics(self.process.pid),
        })
        if self.issued == self.groups:
            self.done = True
            return 200, {}, red.t54.completed('TASK_CONTROL; newest selected fact=done')
        self.issued += 1
        command = ('printf effect >> r9-effects; ' if self.issued == 1 else '')
        command += "printf '%8192s' 'SELECTED_FACT'"
        # Reused provider IDs exercise stable original occurrence ownership.
        return 200, {}, red.round_fixture.tool('reused', 'bash', {'argv':['sh','-c',command]})


def workload(binary, groups, child=False):
    fixture = Growing(binary, groups, child)
    owned = str(fixture.root)
    started = time.monotonic()
    print(json.dumps({'owner_acquired':owned}),flush=True)
    try:
        fixture.process = subprocess.Popen([str(binary),'run','--json','TASK_CONTROL; continue closed groups and consolidate newest working fact'],cwd=fixture.project,env=fixture.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
        fixture.pid_ready.set()
        stdout,stderr = fixture.process.communicate(timeout=120)
        text=(stdout+stderr).decode()
        fixture.safe(text)
        assert fixture.process.returncode == 0 and fixture.done, text
        assert (fixture.project/'r9-effects').read_text() == 'effect'
        with fixture.db() as db:
            turn,status,hot=db.execute('SELECT id,status,result FROM turns ORDER BY rowid DESC LIMIT 1').fetchone()
            segments=list(db.execute('SELECT ordinal,payload FROM turn_raw_segments WHERE turn_id=? ORDER BY ordinal',(turn,)))
            calls=list(db.execute('SELECT provider_call_id,call_occurrence,original_input_index,state FROM tool_operations WHERE turn_id=? ORDER BY rowid',(turn,)))
            counted=dict(db.execute("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        sockets={lane:len(rows) for lane,rows in fixture.lanes.items()}
        assert counted == {k:v for k,v in sockets.items() if v}, (counted,sockets)
        assert status == 'completed' and len(calls)==groups and segments
        assert [row[1] for row in calls] == list(range(groups)), calls
        assert all(row[0]=='reused' and row[3]=='completed' for row in calls)
        assert all(v['history_read_counters'][2:4]==[0,0] for v in fixture.measurements), 'ordinary path loaded RAW pages'
        journals=[json.loads(payload)['journal'] for _,payload in segments]+[json.loads(hot)]
        outputs=[i for journal in journals for i in journal['input'] if i.get('type')=='function_call_output']
        assert len(outputs)==groups and all(len(i['output'].split('\n',1)[1])==8192 for i in outputs)
        spans=[span for journal in journals for span in journal['spans']]
        failed=[span for span in spans if span['status']=='failed']
        assert len(failed)==1 and failed[0]['completed'] is not None
        raw=''.join(payload for _,payload in segments)+hot
        prior=[0]*6
        read_deltas=[]
        for measurement in fixture.measurements:
            counters=measurement['history_read_counters']
            read_deltas.append([now-old for now,old in zip(counters,prior)])
            prior=counters
        result={'groups':groups,'child':child,'exit':0,'effect_count':1,'physical_posts':sockets,'dispatches':counted,
            'raw_bytes':len(raw.encode()),'raw_sha256':hashlib.sha256(raw.encode()).hexdigest(),
            'raw_segment_hashes_stable':len(fixture.hashes),'raw_segments':len(segments),
            'final_checkpoint_bytes':len(hot.encode()),'first_boundary':fixture.measurements[0],
            'last_boundary':fixture.measurements[-1],
            'max_checkpoint_bytes':max(v['checkpoint_serialized_bytes'] for v in fixture.measurements),
            'max_wire_bytes':max(v['wire_json_bytes'] for v in fixture.measurements),
            'max_history_read_delta':[max(v[i] for v in read_deltas) for i in range(6)],
            'max_hot_arrays':{key:max(v[key] for v in fixture.measurements) for key in ('hot_input_items','hot_requests','hot_spans')},
            'observed_checkpoint_bytes_sum':sum(v['checkpoint_serialized_bytes'] for v in fixture.measurements),
            'process_write_amplification_per_raw_byte':round(fixture.measurements[-1]['write_bytes']/len(raw.encode()),3),
            'peak_process_RSS':max(v['VmRSS_bytes'] for v in fixture.measurements),
            'peak_process_HWM':max(v['VmHWM_bytes'] for v in fixture.measurements),
            'seconds':round(time.monotonic()-started,3),'failed_span_distinct':True,
            'continuing_closed_current_task':True}
    finally:
        if fixture.process is not None and fixture.process.poll() is None:
            os.killpg(fixture.process.pid,signal.SIGKILL)
            fixture.process.wait(timeout=5)
        fixture.close()
        assert not fixture.thread.is_alive() and not Path(owned).exists()
        print(json.dumps({'owner_cleanup':owned,'process_reaped':True,'http_handlers_joined':True,'exact_temp_removed':True}),flush=True)
    return result


def main():
    binary=(ROOT/(sys.argv[1] if len(sys.argv)>1 else 'target/debug/oc')).resolve()
    assert binary in (ROOT/'target/debug/oc',ROOT/'target/release/oc')
    image=binary.read_bytes()
    assert image[:4]==b'\x7fELF'
    digest=hashlib.sha256(image).hexdigest()
    results=[]
    for groups,child in ((8,False),(32,False),(32,True)):
        result=workload(binary,groups,child)
        print(json.dumps({'binary':str(binary.relative_to(ROOT)),'sha256':digest,**result}),flush=True)
        results.append(result)
    assert results[1]['max_checkpoint_bytes'] < results[0]['max_checkpoint_bytes']+4096
    assert results[1]['max_wire_bytes'] < results[0]['max_wire_bytes']+4096
    assert results[1]['max_history_read_delta'][1] <= results[0]['max_history_read_delta'][1]+4096
    assert hashlib.sha256(binary.read_bytes()).hexdigest()==digest


if __name__=='__main__':
    main()
