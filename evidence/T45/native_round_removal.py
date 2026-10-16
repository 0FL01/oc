#!/usr/bin/env python3
"""Offline actual normal-ELF sequential root/child + existing RET01 follow-up."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('t54_native', ROOT/'evidence/T54/native_runtime.py')
t54 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t54)


def tool(call_id, name, arguments):
    call = {'type': 'function_call', 'id': 'fc_'+call_id, 'call_id': call_id,
            'name': name, 'arguments': json.dumps(arguments), 'status': 'completed'}
    return t54.event({'type': 'response.output_item.done', 'output_index': 0, 'item': call}) + t54.event({
        'type': 'response.completed', 'response': {'output': [call]}})


class OwnedFixture(t54.Fixture):
    def __init__(self, binary, mode):
        super().__init__(binary, mode)
        self.server.daemon_threads = False  # server_close joins every request handler


class Sequential(OwnedFixture):
    def __init__(self, binary, mode):
        self.issued = 0
        self.early = self.late = False
        self.tokens = []
        self.final = False
        self.reopening = False
        super().__init__(binary, mode)

    def response(self, lane, number):
        if lane == 'title':
            return 200, {}, t54.completed('Sequential fixture')
        if self.reopening and self.mode == 'child' and lane == 'main':
            assert any(item.get('type') == 'function_call_output' and item.get('call_id') == 'delegate'
                       and 'genuine final after17' in item['output'] for item in self.lanes[lane][-1]['input'])
            return 200, {}, t54.completed('reopened parent without child replay')
        if self.mode == 'child' and lane == 'main':
            if number == 1:
                return 200, {}, tool('delegate', 'subagent', {
                    'agent': 'helper', 'prompt': '17 causally dependent steps', 'description': 'sequential child'})
            assert self.final, 'parent final before genuine child completion'
            return 200, {}, t54.completed('parent genuine final')
        request = self.lanes[lane][-1]
        assert request.get('tool_choice') != 'none'
        outputs = [item for item in request['input'] if item.get('type') == 'function_call_output']
        if self.final:
            assert len(outputs) >= 17
            return 200, {}, t54.completed('reopened without replay')
        assert len(outputs) == self.issued, (len(outputs), self.issued)
        if self.issued:
            preceding = outputs[-1]
            assert preceding['call_id'] == 'seq-'+str(self.issued)
            tokens = re.findall(r'TOOL_\d+(?:_\d+)*', preceding['output'])
            assert tokens, preceding
            token = tokens[-1]
            if self.issued > 1:
                assert token == self.tokens[self.issued-2]+'_'+str(self.issued)
            if len(self.tokens) < self.issued:
                self.tokens.append(token)
        if self.mode == 'root' and not self.early:
            self.early = True
            return 429, {'retry-after-ms': '0'}, b'{"error":{"code":"rate_limit_exceeded"}}'
        if self.issued == 17:
            if self.mode in ('root', 'cancel') and not self.late:
                self.late = True
                return 429, {'retry-after-ms': '900000' if self.mode == 'cancel' else '0'}, b'{"error":{"code":"rate_limit_exceeded"}}'
            self.final = True
            return 200, {}, t54.completed('genuine final after17')
        self.issued += 1
        command = ("printf effect >> sequential-effects; printf 'TOOL_%s' \"$$\"" if self.issued == 1 else
                   "printf '%s' '"+self.tokens[-1]+'_'+str(self.issued)+"'")
        return 200, {}, tool('seq-'+str(self.issued), 'bash', {'argv': ['sh', '-c', command]})


def sequential(binary, mode):
    fixture = Sequential(binary, mode)
    process = None
    try:
        if mode == 'cancel':
            process = subprocess.Popen([str(binary), 'run', '--json', 'cancel after17 steps'],
                                       cwd=fixture.project, env=fixture.env, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, start_new_session=True)
            def waiting():
                with fixture.db() as db:
                    return db.execute("SELECT count(*) FROM events WHERE kind='retry_scheduled'").fetchone()[0] == 1
            t54.wait_for(lambda: fixture.late and waiting(), 20)
            process.send_signal(signal.SIGINT)
            stdout, stderr = process.communicate(timeout=10)
            code, text = process.returncode, (stdout+stderr).decode()
            assert code == 130, (code, text)
        else:
            code, text = fixture.run()
            assert code == 0, (mode, code, text)
        facts = fixture.facts()
        lane = 'child' if mode == 'child' else 'main'
        expected = 18 if mode == 'child' else 18 if mode == 'cancel' else 20
        assert facts['sockets'][lane] == expected, facts['sockets']
        assert fixture.issued == 17 and len(fixture.tokens) == 17
        assert (fixture.project/'sequential-effects').read_text() == 'effect'
        with fixture.db() as db:
            rows = db.execute('SELECT session_id,status,result FROM turns ORDER BY rowid').fetchall()
            retries = [json.loads(row[0]) for row in db.execute("SELECT payload FROM events WHERE kind='retry_scheduled'")]
            states = list(db.execute('SELECT state,count(*) FROM tool_operations GROUP BY state'))
        assert all(state == 'completed' for state, _ in states), states
        assert sum(count for _, count in states) == (18 if mode == 'child' else 17)
        logical = next((session, status, json.loads(raw)) for session, status, raw in rows
                       if json.loads(raw)['display'].get('agent') == ('helper' if mode == 'child' else 'build'))
        session, status, log = logical
        assert status == ('cancelled' if mode == 'cancel' else 'completed'), status
        assert max(span['step'] for span in log['spans']) == 18
        if mode == 'root':
            assert [item['retry']['attempt'] for item in retries] == [2, 2], retries
        before = {lane: len(requests) for lane, requests in fixture.lanes.items()}
        # A new process opens the same durable session; only its genuine new final
        # is requested. No historical shell/subagent call is executed again.
        fixture.final = True
        fixture.reopening = True
        if mode == 'child':
            session = rows[0][0]  # reopen the root, inspect its durable linked child
        code, text = fixture.run(session)
        assert code == 0, text
        after = {lane: len(requests) for lane, requests in fixture.lanes.items()}
        assert after['main'] == before['main']+1 and after['child'] == before['child']
        with fixture.db() as db:
            assert db.execute('SELECT count(*) FROM tool_operations').fetchone()[0] == sum(c for _, c in states)
        assert (fixture.project/'sequential-effects').read_text() == 'effect'
        return {'case': mode, 'exit': 130 if mode == 'cancel' else 0,
                'tool_bearing_steps': 17, 'successful_steps': 17 if mode == 'cancel' else 18,
                'physical_lanes_before_reopen': before, 'physical_lanes_after_reopen': after,
                'retry_attempts': [item['retry']['attempt'] for item in retries],
                'effect_count': 1, 'durable_tool_states': states, 'replay': False,
                'preceding_actual_outputs': 17, 'cancelled_wait': mode == 'cancel',
                'process_reaped': True}
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        fixture.close()
        assert not fixture.thread.is_alive()


class PartialThrottle(OwnedFixture):
    def response(self, lane, number):
        if lane != 'main' or number == 1:
            return super().response(lane, number)
        if number == 2:
            return 200, {'Retry-After': '900'}, t54.event({
                'type': 'response.output_text.delta', 'delta': 'partial'}) + t54.event({
                'type': 'response.reasoning_summary_text.delta', 'delta': 'partial reasoning'}) + t54.event({
                'type': 'response.failed', 'response': {'error': {'code': 'rate_limit_exceeded'}}})
        return 200, {}, t54.event({'type': 'response.reasoning_summary_text.delta', 'delta': 'continued reasoning'}) + t54.completed('effect preserved')


def partial(binary):
    fixture = PartialThrottle(binary, 'effect')
    try:
        code, text = fixture.run()
        assert code == 0, text
        facts = fixture.facts()
        assert facts['sockets']['main'] == 3 and facts['tool_intents'] == 1
        assert (fixture.project/'retry-effects').read_text() == 'effect'
        assert 'partial' in json.dumps(fixture.lanes['main'][2]['input'])
        with fixture.db() as db:
            log = json.loads(db.execute('SELECT result FROM turns').fetchone()[0])
        spans = log['spans']
        assert spans[1]['status'] == 'failed' and spans[1]['retry']['attempt'] == 2
        assert spans[1]['id'] != spans[2]['id'] and spans[2]['status'] == 'completed'
        reasoning = [part for part in log['display_parts'] if 'reasoning' in part]
        assert len(reasoning) == 2, reasoning
        assert [part['span'] for part in reasoning] == [spans[1]['id'], spans[2]['id']], reasoning
        return {'case': 'partial-sse-throttle-effect', 'exit': code, 'physical_lanes': facts['sockets'],
                'failed_span': spans[1]['id'], 'continuation_span': spans[2]['id'],
                'retry_attempt': 2, 'effect_count': 1, 'reasoning_producer_attribution': True}
    finally:
        fixture.close()
        assert not fixture.thread.is_alive()


def main():
    binary = (ROOT/sys.argv[1]).resolve()
    assert binary in ((ROOT/'target/debug/oc'), (ROOT/'target/release/oc'))
    data = binary.read_bytes()
    assert data[:4] == b'\x7fELF'
    modes = sys.argv[2:] or ['root', 'child', 'partial', 'cancel']
    for mode in modes:
        result = partial(binary) if mode == 'partial' else sequential(binary, mode)
        print(json.dumps({'binary': str(binary.relative_to(ROOT)), 'sha256': hashlib.sha256(data).hexdigest(), **result}), flush=True)
    assert hashlib.sha256(binary.read_bytes()).digest() == hashlib.sha256(data).digest()


if __name__ == '__main__':
    main()
