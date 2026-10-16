#!/usr/bin/env python3
"""Owned offline normal-ELF mixed renewal and metadata-first compact recovery."""
import argparse
import codecs
import hashlib
import importlib.util
import json
import re
import sqlite3
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT/path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


t50 = load('renewal_native', 'evidence/T50/native_background.py')
sys.modules.setdefault('native_background',t50)
ui = load('renewal_terminal','evidence/T50/native_shell_controls.py')
read = load('renewal_read', 'evidence/T50/native_read.py')
metrics = load('renewal_metrics', 'evidence/T45/native_hot_raw_red.py')
CONTROL = 'CONTROL_OBJECTIVE CHANGED_REQUIREMENT selected/path RESULT_OK NEXT_MOVE'
INITIAL = 'CONTROL_OBJECTIVE OLD_REQUIREMENT obsolete/path INITIAL_RESULT INITIAL_MOVE'
SUMMARY = '## Objective\n'+CONTROL+'\n## WorkState\nClosed work settled; continue selected next move.'
INITIAL_SUMMARY = '## Objective\n'+INITIAL+'\nOLD_SUMMARY\n## WorkState\nInitial selected work.'
ANCHORS = 'DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: '


class Mixed(t50.Native):
    consume_pty = ui.Consumer.consume_pty
    screen = ui.Consumer.screen

    def start(self):
        self.grid_lock = threading.Lock()
        self.grid = [[' ' for _ in range(110)] for _ in range(self.height)]
        self.cursor, self.pending = (0,0), ''
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        super().start()

    def __init__(self, binary, archive, mode='mixed'):
        self.phase, self.step = 'prime', 0
        self.archive, self.mode = archive, mode
        self.summary_posts, self.measurements = 0, []
        self.hold = threading.Event()
        self.arrived = threading.Event()
        self.barrier = False
        super().__init__(binary, self.response, {'read':'allow','bash':'allow','compress':'allow'})
        config = self.home/'config/opencode/opencode.json'
        settings = json.loads(config.read_text())
        settings['agent'] = {}
        settings['compaction'] = {'auto':False,'keep':{'tokens':0}}
        settings['provider']['fixture']['models']['m']['modalities'] = {'input':['text','image'],'output':['text']}
        config.write_text(json.dumps(settings))
        self.project.joinpath('image.png').write_bytes(read.png_bytes())

    def __exit__(self, *args):
        self.hold.set()
        owned = str(self.root)
        try:
            super().__exit__(*args)
        finally:
            assert not Path(owned).exists()
            print(json.dumps({'owner_cleanup':owned,'all_joined':True,'exact_temp_removed':True}),flush=True)

    def response(self, owner, number, request):
        if not request.get('tools'):
            self.summary_posts += 1
            if self.barrier:
                self.arrived.set()
                assert self.hold.wait(20), 'owned compaction barrier expired'
            assert 'OBSOLETE_HOST' not in json.dumps(request['input'])
            assert 'OBSOLETE_MODEL' not in json.dumps(request['input'])
            if self.mode in ('host','model'):
                return t50.completed('## Objective\nContinue latest user task.\n## WorkState\nOlder unprotected context was omitted.')
            assert CONTROL in json.dumps(request['input']), 'selected facts missing before counted summary'
            return t50.completed(SUMMARY)
        wire = json.dumps(request['input'])
        with sqlite3.connect(self.data/'oc.sqlite') as db:
            checkpoint = db.execute("SELECT length(CAST(result AS BLOB)),json_extract(result,'$.display.history_read_counters'),json_extract(result,'$.display.renewal_read_counters') FROM turns ORDER BY rowid DESC LIMIT 1").fetchone()
            active = db.execute("SELECT count(*) FROM compression_blocks WHERE hot->>'$.active'=1").fetchone()[0]
            ancestry = db.execute("SELECT count(*) FROM compression_blocks WHERE hot->>'$.active'=1 AND hot->>'$.standalone'!=1").fetchone()[0]
        wal = Path(str(self.data/'oc.sqlite')+'-wal')
        size = wal.stat().st_size if wal.exists() else 0
        self.measurements.append({'phase':self.phase,'process_pid':self.process.pid,'wire_bytes':len(json.dumps(request).encode()),
            'checkpoint_bytes':checkpoint[0],'history_reads':json.loads(checkpoint[1]),
            'renewal_reads':json.loads(checkpoint[2]),'active_blocks':active,'nonstandalone_live_blocks':ancestry,
            'db_bytes':self.data.joinpath('oc.sqlite').stat().st_size,'wal_bytes':size,
            'wal_frames':max(0,(size-32)//4120),**metrics.process_metrics(self.process.pid)})
        self.step += 1
        if self.phase == 'prime':
            if self.step == 1:
                return t50.tool('read',{'path':'image.png'},'image')
            if self.step == 2:
                assert 'input_image' in wire
                self.project.joinpath('image.png').unlink()
                events = t50.tool('bash',{'argv':['/bin/sh','-c',"printf once >> effect; printf '%32768s' OLD_TOOL_PAYLOAD"]},'effect')
                events.insert(0,{'type':'response.output_item.done','item':{'type':'reasoning','id':'cold-reasoning','encrypted_content':'OPAQUE_OLD','summary':[{'type':'summary_text','text':'OLD_REASONING'}]}})
                return events
            out = [i for i in request['input'] if i.get('type')=='function_call_output' and i.get('call_id')=='effect']
            assert out and 'exit 0' in json.dumps(out[0]['output']), str(out)[:500]
            return t50.completed('OLD_INVESTIGATION OLD_REQUIREMENT '+('obsolete work '*700))
        assert (INITIAL if self.phase == 'compress-first' else CONTROL) in wire
        if self.phase == 'crash-primary':
            self.arrived.set()
            assert self.hold.wait(20)
        if self.phase.startswith('compress'):
            if self.step == 1:
                if self.phase == 'compress-first':
                    print(json.dumps({'first_wire_shape':[(i.get('type'),i.get('role'),i.get('call_id'),len(json.dumps(i))) for i in request['input']],
                        'first_raw_shape':[(i.get('type'),i.get('role'),i.get('call_id'),len(json.dumps(i))) for i in json.loads(self.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0])['input']],
                        'original_request_starts':[s.get('request',{}).get('input_start') for s in json.loads(self.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0])['spans']]}),flush=True)
                anchors = []
                for item in request['input']:
                    for part in item.get('content',[]):
                        if isinstance(part,dict) and part.get('text','').startswith(ANCHORS):
                            anchors = json.loads(part['text'][len(ANCHORS):])
                closed = [a for a in anchors if a['closed']]
                assert len(closed)>=1, anchors
                return t50.tool('compress',{'topic':'selected working state','content':[{
                    'startId':closed[0]['id'],'endId':closed[-1]['id'],'summary':INITIAL_SUMMARY if self.phase=='compress-first' else SUMMARY}]},'reused-renewal')
            outputs = [i for i in request['input'] if i.get('type')=='function_call_output']
            assert any('success' in json.dumps(i.get('output','')).lower() or 'compressed' in json.dumps(i.get('output','')).lower() for i in outputs), [str(outputs[-1].get('output'))[:500] if outputs else 'no output']
            if self.phase == 'compress-first':
                assert 'input_image' in wire and 'OLD_TOOL_PAYLOAD' in wire
            else:
                assert 'input_image' not in wire and 'OLD_TOOL_PAYLOAD' not in wire and 'OPAQUE_OLD' not in wire
                assert 'OLD_REASONING' not in wire and 'OLD_INVESTIGATION' not in wire
                assert 'OLD_REQUIREMENT' not in wire and 'OLD_SUMMARY' not in wire, ('obsolete selected state survives',[(i.get('type'),i.get('name'),i.get('call_id')) for i in request['input'] if 'OLD_REQUIREMENT' in json.dumps(i) or 'OLD_SUMMARY' in json.dumps(i)])
            return t50.completed('Closed selected result. '+('fresh work '*600)+'\nDONE_'+self.phase)
        if self.phase == 'continue-first':
            assert 'input_image' in wire
            assert 'OLD_INVESTIGATION' not in wire
        else:
            assert 'OLD_TOOL_PAYLOAD' not in wire and 'OPAQUE_OLD' not in wire and 'input_image' not in wire
            assert 'OLD_REASONING' not in wire and 'OLD_INVESTIGATION' not in wire
            assert 'OLD_REQUIREMENT' not in wire and 'OLD_SUMMARY' not in wire
        return t50.completed('Closed current work. '+('fresh work '*600)+'\nDONE_'+self.phase)

    def turn(self, phase):
        self.phase, self.step = phase, 0
        before = self.rows("SELECT count(*) FROM turns WHERE status='completed'")[0][0]
        self.tail.clear()
        prompt = INITIAL if phase == 'prime' else CONTROL if phase == 'continue-first' else 'Continue the current selected work.'
        self.send((prompt+' PHASE_'+phase+'\r').encode())
        try:
            t50.until(lambda:self.rows("SELECT count(*) FROM turns WHERE status='completed'")==[(before+1,)] or self.errors,
                      ('turn not settled',phase),seconds=20)
        except AssertionError:
            print(json.dumps({'turn_ack_failure':phase,'turn_states':self.rows('SELECT id,status FROM turns'),
                              'physical_posts':self.physical_requests,'errors':self.errors,'screen':self.screen()}),flush=True)
            raise
        assert not self.errors,self.errors
        t50.until(lambda:self.completed_frame(None if phase=='prime' else 'DONE_'+phase), 'exact finished primary frame missing',seconds=10)

    def compact(self, expected='completed'):
        before = self.rows('SELECT count(*) FROM session_compactions')[0][0]
        self.tail.clear()
        self.send(b'/compact\r')
        t50.until(lambda:self.rows('SELECT count(*) FROM session_compactions')[0][0]>before,'compact command not admitted')
        t50.until(lambda:self.rows("SELECT json_extract(snapshot,'$.state') FROM session_compactions ORDER BY rowid DESC LIMIT 1")==[(expected,)] or self.errors,
                  ('compact not settled',expected),seconds=20)
        assert not self.errors,self.errors
        time.sleep(.08)

    def archive_seed(self):
        with sqlite3.connect(self.data/'oc.sqlite') as db:
            first,last = db.execute("SELECT min(id),max(id) FROM messages WHERE session_id='t50-background'").fetchone()
            hot = json.dumps({'version':1,'active':False,'standalone':True,'protected':[],'logs':[]})
            db.executemany('INSERT INTO compression_blocks(id,session_id,topic,summary,start_msg,end_msg,created_at,hot) VALUES(?,?,?,?,?,?,?,?)',
                [('archive-'+str(i),'t50-background','old','INACTIVE_SENTINEL',first,last,i,hot) for i in range(self.archive)])
            db.executemany('INSERT INTO compression_members VALUES(?,?)',
                [('archive-'+str(i),first) for i in range(self.archive)])
            db.executemany('INSERT INTO dcp_tool_projection_v2(session_id,call_id,occurrence,action) VALUES(?,?,?,?)',
                [('t50-background','inactive-'+str(i),0,'hidden') for i in range(self.archive)])
            db.commit()
            db.execute('PRAGMA wal_checkpoint(TRUNCATE)')

    def receipts(self):
        lanes = dict(self.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        assert sum(lanes.values())==self.physical_requests,(lanes,self.physical_requests)
        assert self.project.joinpath('effect').read_bytes()==b'once'
        assert not self.project.joinpath('image.png').exists()
        assert all(m['history_reads'][2:4]==[0,0] for m in self.measurements)
        assert all(m['nonstandalone_live_blocks']==0 for m in self.measurements)
        prior = [0]*4
        prior_pid = None
        deltas = []
        for sample in self.measurements:
            if sample['process_pid'] != prior_pid:
                prior = [0]*4
            values = sample['renewal_reads']
            deltas.append([max(0,new-old) for new,old in zip(values,prior)])
            prior = values
            prior_pid = sample['process_pid']
        return {'status':'PASS','archive':self.archive,'mode':self.mode,'physical_posts':self.physical_requests,
                'dispatches':lanes,'effect_count':1,'cold_image_removed':True,'measurements':self.measurements,
                'max_checkpoint_bytes':max(m['checkpoint_bytes'] for m in self.measurements),
                'max_wire_bytes':max(m['wire_bytes'] for m in self.measurements),
                'max_renewal_read_delta':[max(row[i] for row in deltas) for i in range(4)],
                'max_live_blocks':max(m['active_blocks'] for m in self.measurements),
                'changed_requirement_once':True,'old_summary_requirement_forgotten':True,
                'live_ancestry_depth':0,'peak_process_HWM':max(m['VmHWM_bytes'] for m in self.measurements)}


def mixed(binary, archive):
    with Mixed(binary,archive) as native:
        print(json.dumps({'owner_acquired':str(native.root)}),flush=True)
        native.start()
        native.turn('prime')
        raw = native.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0]
        raw_hash = hashlib.sha256(raw.encode()).hexdigest()
        native.archive_seed()
        native.turn('compress-first')
        native.turn('continue-first')
        native.turn('compress-second')
        native.compact()
        assert native.rows("SELECT count(*) FROM compression_blocks WHERE hot->>'$.active'=1")==[(0,)], 'compact leaves old live roots/payloads'
        for i in range(10):
            native.turn('continue-'+str(i))
            native.turn('compress-'+str(i))
            if i == 4:
                before = native.physical_requests
                native.stop()
                native.start()
                time.sleep(.15)
                assert native.physical_requests==before,'restart replayed provider work'
                native.compact()
        native.compact()
        native.turn('continue-final')
        assert native.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0]==raw
        assert not native.project.joinpath('.git').exists()
        result = native.receipts()
        result['canonical_first_raw_sha256'] = raw_hash
        result['standalone_replacements'] = 12
    return result


def overflow(binary, mode):
    with Mixed(binary,0,mode) as native:
        print(json.dumps({'owner_acquired':str(native.root),'case':mode}),flush=True)
        native.start()
        native.turn('prime')
        native.stop()
        with sqlite3.connect(native.data/'oc.sqlite') as db:
            seq = db.execute('SELECT max(seq) FROM messages').fetchone()[0]
            body = ('OBSOLETE_HOST '*(16777216//14+2)) if mode=='host' else ('OBSOLETE_MODEL '*18000)
            for n,(role,text) in enumerate((('user','old closed legacy task'),('assistant',body),('user',CONTROL)),seq+1):
                db.execute('INSERT INTO messages VALUES(?,?,?,?,?)',(f'm{n}','t50-background',n,role,text))
        before=native.physical_requests
        attempt=subprocess.run([str(binary),'--data-dir',str(native.data),'run','--session','t50-background',CONTROL+' attempted ordinary overflow'],
            cwd=native.project,env=native.env,capture_output=True,timeout=15)
        diagnostic=(attempt.stdout+attempt.stderr).decode('utf8','replace')
        codes=('safety budget','input budget','input limit','context limit','capacity_exceeded')+ (('invalid_config',) if mode=='model' else ())
        assert attempt.returncode==1 and any(s in diagnostic.lower() for s in codes),diagnostic[:1000]
        assert native.physical_requests==before,'ordinary overflow spent provider call'
        native.start()
        native.compact()
        native.turn('continue-recovered')
        result=native.receipts()
        result['ordinary_overflow_provider_posts']=0
        result['manual_compact_recovered']=True
        result['ordinary_overflow_diagnostic']=diagnostic[:1000]
        result['progress_receipts']=native.rows("SELECT payload FROM events WHERE kind='compaction_progress'")
    return result


def fault(binary, mode):
    with Mixed(binary,0,mode) as native:
        print(json.dumps({'owner_acquired':str(native.root),'case':mode}),flush=True)
        native.start()
        native.turn('prime')
        native.turn('compress-first')
        native.turn('continue-first')
        native.turn('compress-second')
        raw=native.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0]
        before_progress=native.rows("SELECT count(*) FROM events WHERE kind='compaction_progress'")[0][0]
        native.barrier=True
        native.send(b'/compact\r')
        assert native.arrived.wait(15),native.errors
        if mode=='fault':
            with sqlite3.connect(native.data/'oc.sqlite') as db:
                db.execute("CREATE TRIGGER fail_renewal_checkpoint BEFORE INSERT ON session_checkpoint BEGIN SELECT RAISE(ABORT,'owned checkpoint fault'); END")
            native.hold.set()
            t50.until(lambda:native.rows("SELECT json_extract(snapshot,'$.state') FROM session_compactions ORDER BY rowid DESC LIMIT 1")==[('failed',)],'fault did not settle')
            assert native.rows("SELECT count(*) FROM events WHERE kind='compaction_progress'")==[(before_progress,)]
            assert native.rows("SELECT count(*) FROM compression_blocks WHERE hot->>'$.active'=1")==[(1,)]
            with sqlite3.connect(native.data/'oc.sqlite') as db:
                db.execute('DROP TRIGGER fail_renewal_checkpoint')
            native.barrier=False
            native.compact()
        elif mode=='cancel':
            # HTTP arrival is not a UI key ACK. Reconstruct the bounded painted
            # running snapshot. Manual compaction has no streaming armed footer;
            # use two unambiguous CSI-u Escape presses and durable cancellation.
            t50.until(lambda:'Compaction' in '\n'.join(native.screen()),'running compaction was not painted')
            native.send(b'\x1b[27u')
            time.sleep(.15)
            native.send(b'\x1b[27u')
            t50.until(lambda:native.rows("SELECT json_extract(snapshot,'$.state') FROM session_compactions ORDER BY rowid DESC LIMIT 1")==[('cancelled',)],'cancel did not settle')
            native.hold.set()
            native.barrier=False
            assert native.rows("SELECT count(*) FROM events WHERE kind='compaction_progress'")==[(before_progress,)]
            native.compact()
        else:
            native.hold.set()
            t50.until(lambda:native.rows("SELECT json_extract(snapshot,'$.state') FROM session_compactions ORDER BY rowid DESC LIMIT 1")==[('completed',)],'checkpoint did not commit')
            native.barrier=False
            native.hold.clear()
            native.arrived.clear()
            native.phase,native.step='crash-primary',0
            native.send(b'Continue the current selected work; crash open primary\r')
            assert native.arrived.wait(15),native.errors
            native.process.kill()
            native.process.wait(timeout=5)
            native.hold.set()
            native.stop(crash=True)
        before=native.physical_requests
        checkpoint=native.rows('SELECT summary,selection FROM session_checkpoint')
        if native.process is not None:
            native.stop()
        native.start()
        time.sleep(.15)
        assert native.physical_requests==before,'restart replayed work'
        assert checkpoint==native.rows('SELECT summary,selection FROM session_checkpoint')
        assert raw==native.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1')[0][0]
        native.turn('continue-recovered')
        result=native.receipts()
        result.update({'case':mode,'restart_replay_posts':0,'canonical_raw_unchanged':True,'rollback_no_false_progress':mode!='crash'})
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('binary',choices=('target/debug/oc','target/release/oc'))
    parser.add_argument('--case',choices=('mixed','compare','host','model','fault','cancel','crash'),default='mixed')
    parser.add_argument('--archive',type=int,default=8)
    args = parser.parse_args()
    binary = (ROOT/args.binary).resolve()
    image = binary.read_bytes()
    assert image[:4]==b'\x7fELF'
    before = hashlib.sha256(image).hexdigest()
    if args.case=='compare':
        small,large=mixed(binary,8),mixed(binary,4100)
        assert small['physical_posts']==large['physical_posts']
        assert large['max_checkpoint_bytes']<=small['max_checkpoint_bytes']+256
        assert large['max_wire_bytes']<=small['max_wire_bytes']+256
        assert large['max_renewal_read_delta'][0]==small['max_renewal_read_delta'][0]
        assert large['max_renewal_read_delta'][1]<=small['max_renewal_read_delta'][1]+256
        result={'status':'PASS','equal_hot_small_large':[small,large]}
    else:
        result = mixed(binary,args.archive) if args.case=='mixed' else overflow(binary,args.case) if args.case in ('host','model') else fault(binary,args.case)
    assert hashlib.sha256(binary.read_bytes()).hexdigest()==before
    print(json.dumps({'binary':args.binary,'sha256_before':before,'sha256_after':before,**result}),flush=True)
