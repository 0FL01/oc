#!/usr/bin/env python3
"""Real busy picker, current RAW/HOT compaction and restart, owned offline ELF."""
import hashlib
import http.server
import json
from pathlib import Path
import sys
import time
import sqlite3

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'evidence/T50'))
import native_live_model_switch as switch


class Current(switch.SwitchNative):
    def __init__(self, binary, fault=None):
        super().__init__(binary, 'compact')
        self.fault = fault
        self.groups = 0
        self.reopened = False
        owner = self

        class Peer(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                with owner.request_counter_lock:
                    owner.physical_requests += 1
                try:
                    size = int(self.headers['Content-Length'])
                    assert 0 < size < 1048576
                    request = json.loads(self.rfile.read(size))
                    assert self.path == '/v1/responses'
                    assert self.headers['Authorization'] == 'Bearer SYNTHETIC_SWITCH'
                    self.send_response(200)
                    self.send_header('Content-Type','text/event-stream')
                    self.end_headers()
                    if not request.get('tools'):
                        if request['max_output_tokens'] == 256:
                            owner.auxiliary.append(request)
                            switch.finish(self,[switch.message('Synthetic title')])
                            return
                        owner.summaries.append(request)
                        assert 'TASK_CONTROL' in json.dumps(request['input'])
                        if len(owner.summaries) == 1:
                            assert request['model'] == switch.A
                            owner.arrived[2].set()
                            assert owner.gates[2].wait(15)
                            if owner.fault == 'before_commit':
                                with sqlite3.connect(owner.data/'oc.sqlite') as db:
                                    db.execute("CREATE TRIGGER owned_segment_fault BEFORE INSERT ON turn_raw_segments BEGIN SELECT RAISE(ABORT,'owned before-commit fault'); END")
                        switch.finish(self,[switch.message('## Work State\nTASK_CONTROL: committed file effect complete; latest SELECTED_FACT. Continue same task.')])
                        return
                    owner.requests.append(request)
                    family = {t['name'] for t in request['tools']} & {'apply_patch','edit','write'}
                    assert family == ({'apply_patch'} if request['model']==switch.A else {'edit','write'})
                    assert request['max_output_tokens'] == (2048 if request['model']==switch.A else 1536)
                    assert 'TASK_CONTROL' in json.dumps(request['input'])
                    outputs = [i for i in request['input'] if i.get('type')=='function_call_output']
                    assert len(outputs) <= 4
                    if owner.summaries:
                        assert request['model'] == switch.B
                        assert owner.rows('SELECT count(*) FROM turn_raw_segments')[0][0] > 0
                        assert 'A-opaque' not in json.dumps(request)
                        assert owner.project.joinpath('file').read_bytes() == b'once\n'
                    if owner.reopened or owner.groups == 8:
                        if owner.fault == 'after_commit' and not owner.reopened:
                            owner.arrived[3].set()
                            assert owner.gates[3].wait(15)
                        switch.finish(self,[switch.message('TASK_CONTROL done')])
                        return
                    owner.groups += 1
                    calls = []
                    if owner.groups == 1:
                        calls.append(switch.item('effect','apply_patch',{'patchText':'*** Begin Patch\n*** Add File: file\n+once\n*** End Patch'}))
                        calls.append({'type':'reasoning','id':'original-reason','encrypted_content':'A-opaque','summary':[]})
                    calls.append(switch.item('group-'+str(owner.groups),'bash',{'argv':['sh','-c',"printf '%8192s' 'SELECTED_FACT'"]}))
                    switch.finish(self,calls)
                except (BrokenPipeError, ConnectionResetError):
                    if owner.fault != 'after_commit':
                        owner.errors.append('unexpected connection loss')
                except Exception as error:
                    owner.errors.append(repr(error))
                finally:
                    self.close_connection = True

        self.server.RequestHandlerClass = Peer
        config_path = self.home/'config/opencode/opencode.json'
        config = json.loads(config_path.read_text())
        config['compaction'] = {'auto':True,'buffer':24000}
        for model in (switch.A,switch.B):
            config['provider']['fixture']['models'][model]['limit']['context'] = 32000
        config_path.write_text(json.dumps(config))

    def __exit__(self, *args):
        try:
            return super().__exit__(*args)
        finally:
            print(json.dumps({'owner_cleanup':str(self.root),'all_joined':not self.thread.is_alive(),'exact_temp_removed':not self.root.exists()}),flush=True)


def run(binary):
    with Current(binary) as native:
        print(json.dumps({'owner_acquired':str(native.root)}),flush=True)
        native.start()
        native.send(b'TASK_CONTROL; finish current closed groups\r')
        assert native.arrived[2].wait(15), native.errors
        native.choose(switch.B)
        assert native.owner() == switch.A
        native.commit(switch.B)
        native.gates[2].set()
        switch.until(lambda:native.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(1,)], ('task not settled',native.errors),seconds=25)
        assert not native.errors, native.errors
        raw = native.rows('SELECT ordinal,payload FROM turn_raw_segments ORDER BY ordinal')
        hashes = [hashlib.sha256(payload.encode()).hexdigest() for _,payload in raw]
        assert len(raw) >= 2
        journals = [json.loads(payload)['journal'] for _,payload in raw]
        receipts = [r for journal in journals for r in journal['requests']]
        assert {r['model']['id'] for r in receipts} == {switch.A,switch.B}
        assert sum(len(j['input']) for j in journals)>10
        effect_rows = native.rows("SELECT state FROM tool_operations WHERE name='apply_patch'")
        assert effect_rows == [('completed',)]
        native.stop()
        posts = native.physical_requests
        native.start()
        time.sleep(.3)
        assert native.physical_requests == posts, 'restart replayed provider/tool work'
        assert native.project.joinpath('file').read_bytes()==b'once\n'
        native.reopened = True
        native.send(b'TASK_CONTROL; reopen selected HOT only\r')
        switch.until(lambda:native.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(2,)], 'reopened bounded continuation absent')
        assert not native.errors, native.errors
        assert hashes == [hashlib.sha256(payload.encode()).hexdigest() for _,payload in native.rows('SELECT ordinal,payload FROM turn_raw_segments ORDER BY ordinal')]
        lanes = dict(native.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        assert sum(lanes.values()) == native.physical_requests
        result = {'status':'PASS','groups':8,'current_task_model_switch':'A->B','issuing_summary_model':switch.A,
                  'next_primary_model':switch.B,'raw_segments':len(raw),'raw_hashes':hashes,'physical_posts':native.physical_requests,
                  'dispatches':lanes,'effect_count':1,'restart_replay_posts':0,'reopen_wire_bytes':len(json.dumps(native.requests[-1]).encode())}
        owned = str(native.root)
    assert not Path(owned).exists()
    result['owner_cleanup'] = {'path':owned,'all_joined':True,'exact_temp_removed':True}
    return result


def fault_case(binary, fault):
    with Current(binary, fault) as native:
        print(json.dumps({'owner_acquired':str(native.root),'fault':fault}),flush=True)
        native.start()
        native.send(b'TASK_CONTROL; close current groups with a committed effect\r')
        assert native.arrived[2].wait(15), native.errors
        native.choose(switch.B)
        native.commit(switch.B)
        native.gates[2].set()
        if fault == 'before_commit':
            switch.until(lambda:native.rows("SELECT status FROM turns")==[('failed',)], 'failed transaction did not preserve terminal old HOT')
            assert native.rows('SELECT count(*) FROM turn_raw_segments') == [(0,)]
            journal = json.loads(native.rows('SELECT result FROM turns')[0][0])
            assert any(i.get('call_id')=='effect' and i.get('type')=='function_call_output' for i in journal['input'])
        else:
            assert native.arrived[3].wait(15), native.errors
            assert native.rows('SELECT count(*) FROM turn_raw_segments')[0][0] >= 2
            native.process.kill()
            native.process.wait(timeout=5)
            native.gates[3].set()
        native.stop(crash=fault == 'after_commit')
        assert not native.errors, native.errors
        before_posts = native.physical_requests
        raw_hashes = [hashlib.sha256(payload.encode()).hexdigest() for payload, in native.rows('SELECT payload FROM turn_raw_segments ORDER BY ordinal')]
        native.start()
        time.sleep(.3)
        assert native.physical_requests == before_posts, 'recovery replayed current task'
        assert native.project.joinpath('file').read_bytes() == b'once\n'
        assert native.rows("SELECT state FROM tool_operations WHERE name='apply_patch'")==[('completed',)]
        assert raw_hashes == [hashlib.sha256(payload.encode()).hexdigest() for payload, in native.rows('SELECT payload FROM turn_raw_segments ORDER BY ordinal')]
        status = native.rows('SELECT status FROM turns')[0][0]
        assert status == ('failed' if fault=='before_commit' else 'unknown'), status
        result={'case':fault,'status':'PASS','durable_turn_status':status,'raw_hashes':raw_hashes,
                'restart_provider_posts':0,'effect_count':1,'old_hot_preserved':fault=='before_commit'}
    return result


if __name__=='__main__':
    binary=(ROOT/(sys.argv[1] if len(sys.argv)>1 else 'target/debug/oc')).resolve()
    assert binary in (ROOT/'target/debug/oc',ROOT/'target/release/oc')
    image=binary.read_bytes()
    assert image[:4]==b'\x7fELF'
    before=hashlib.sha256(image).hexdigest()
    result=run(binary)
    faults=[fault_case(binary,fault) for fault in ('before_commit','after_commit')]
    assert hashlib.sha256(binary.read_bytes()).hexdigest()==before
    print(json.dumps({'binary':str(binary.relative_to(ROOT)),'sha256_before':before,'sha256_after':before,'faults':faults,**result}),flush=True)
