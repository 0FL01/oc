#!/usr/bin/env python3
"""Source-derived DCP3.1.15 cadence, normal native ELF / exact loopback wire.

Algorithm provenance: Opencode-DCP/opencode-dynamic-context-pruning,
11f6517780a502512a3467645074be447cb0369e, AGPL-3.0-or-later,
lib/messages/inject/{inject,utils}.ts. Complete license is retained at
evidence/tui/recovery-v00/dcp-oracle20260928-native01/sources/LICENSE.
Native transient developer placement and successful-only cooldown are retained.
"""
import argparse
import hashlib
import json
from pathlib import Path
from native_dcp_controls import Controls, configure, wait_idle, receipt, t50
from native_dcp_defaults import qualify, invalid, identity


def reminders(request):
    return [item['content'][0]['text'] for item in request['input']
            if item.get('role') == 'developer' and 'DCP reminder (' in json.dumps(item)]


def closed(native, requests, turns):
    t50.until(lambda: len(native.requests) == requests and native.settled(turns),
              'source cadence native turn did not settle')
    wait_idle(native)


def last_user(binary):
    with Controls(binary, lambda *_: t50.completed('closed'), {'compress': 'allow'}) as native:
        configure(native, {'compress': {'minContextLimit': 1, 'maxContextLimit': 100000}})
        native.start()
        for i in range(3):
            native.send(f'user {i}\r'.encode()); closed(native, i + 1, i + 1)
        actual = [bool(reminders(req)) for req in native.requests]
        assert actual == [False, True, True], actual
        assert all('advisory' in hint for req in native.requests for hint in reminders(req))
        receipt(native, 'source-last-user-pair', reminder_wire=actual)


def context(binary):
    with Controls(binary, lambda *_: t50.completed('closed'), {'compress': 'allow'}) as native:
        configure(native, {'compress': {'minContextLimit': 1, 'maxContextLimit': 1}})
        native.start()
        for i in range(4):
            native.send(f'context {i}\r'.encode()); closed(native, i + 1, i + 1)
        actual = [bool(reminders(req)) for req in native.requests]
        assert actual == [True, False, False, True], actual
        assert all('required before more work' in hint for req in native.requests for hint in reminders(req))
        native.stop(); native.start()
        native.send(b'context restart\r'); closed(native, 5, 5)
        assert not reminders(native.requests[-1]), 'latest visible context anchor lost on restart'
        receipt(native, 'source-context-visible-distance', reminder_wire=actual + [False])


def iteration(binary):
    def response(owner, number, request):
        if number == 1 or number == 22:
            return t50.completed('closed')
        return t50.tool('read', {'path': str(owner.project / 'fact.txt')}, f'read-{number}')
    with Controls(binary, response, {'compress': 'allow', 'read': 'allow'}) as native:
        (native.project / 'fact.txt').write_text('source iteration fact')
        configure(native, {'compress': {'minContextLimit': 1, 'maxContextLimit': 100000}})
        native.start(); native.send(b'prime\r'); closed(native, 1, 1)
        native.send(b'iterate visible assistant messages\r'); closed(native, 22, 2)
        actual = [bool(reminders(req)) for req in native.requests]
        # New user pair; then 15 non-ignored assistant messages after that user;
        # next iteration anchor is five visible messages later, not five turns.
        expected = [False, True] + [False] * 14 + [True] + [False] * 4 + [True]
        assert actual == expected, actual
        assert 'advisory' in reminders(native.requests[1])[0]
        for index in [16, 21]:
            assert 'required before more work' in reminders(native.requests[index])[0]
        assert native.rows('SELECT count(*) FROM tool_operations') == [(20,)]
        assert native.rows("SELECT count(*) FROM tool_operations WHERE state='completed'") == [(20,)]
        receipt(native, 'source-last-user-iteration-threshold-distance', reminder_wire=actual)


def compression(binary):
    observed = {}
    def response(owner, number, request):
        if number == 1:
            return t50.completed('SELECTED_FACT ' + 'closed removable filler ' * 100)
        if number in [2, 3, 7]:
            text = next(item['content'][0]['text'] for item in request['input']
                        if 'DCP context anchors' in json.dumps(item))
            anchors = json.loads(text[text.index('[{'):])
            eligible = [item for item in anchors if item['closed']]
            start, end = eligible[0]['id'], eligible[1]['id']
            if number == 2: start = end = 'alien'
            summary = 'N' * 7000 if number == 3 else 'SUMMARY_SENTINEL SELECTED_FACT retained'
            observed[number] = json.loads(owner.rows("SELECT value FROM prefs WHERE key LIKE 'dcp.nudge.%'")[0][0])
            return t50.tool('compress', {'topic': 'cadence', 'content': [{
                'startId': start, 'endId': end, 'summary': summary}]}, f'compress-{number}')
        if number < 12:
            return t50.tool('read', {'path': str(owner.project / 'fact.txt')}, f'read-{number}')
        return t50.completed('closed')
    with Controls(binary, response, {'compress': 'allow', 'read': 'allow'}) as native:
        (native.project / 'fact.txt').write_text('source cooldown fact')
        configure(native, {'compress': {'minContextLimit': 1, 'maxContextLimit': 1}})
        native.start(); native.send(b'prime\r'); closed(native, 1, 1)
        native.send(b'real failures then compression and continuation\r'); closed(native, 12, 2)
        operations = native.rows("SELECT name,state FROM tool_operations ORDER BY rowid")
        assert [state for name, state in operations if name == 'compress'] == ['failed', 'no_gain', 'completed'], operations
        assert [state for name, state in operations if name == 'read'] == ['completed'] * 7, operations
        actual = [bool(reminders(req)) for req in native.requests]
        # Prior assistant + new user are already two visible messages after
        # the first context anchor; the third settled tool round reaches five.
        assert actual == [True, False, False, False, True, False, False, False, False, False, False, True], actual
        assert [observed[index]['iteration'] for index in [2, 3, 7]] == [2, 3, 7], observed
        assert all(not value['cadence']['cooldown'] for value in observed.values()), observed
        assert native.rows('SELECT count(*) FROM compression_blocks') == [(1,)]
        assert 'closed removable filler' not in json.dumps(native.requests[7]['input'])
        state = json.loads(native.rows("SELECT value FROM prefs WHERE key LIKE 'dcp.nudge.%'")[0][0])
        assert state['iteration'] == 5 and not state['cadence']['cooldown'], state
        assert state['cadence']['turn'] is None and state['cadence']['iteration'] is None, state
        assert state['cadence']['context'] not in [observed[2]['cadence']['context'], observed[7]['cadence']['context']]
        native.stop(); native.start()
        native.send(b'after successful cadence restart\r'); closed(native, 13, 3)
        if reminders(native.requests[-1]):
            print(json.dumps({'restart_state_before': state,
                              'restart_state_after': native.rows("SELECT value FROM prefs WHERE key LIKE 'dcp.nudge.%'"),
                              'retained_turn': json.loads(native.rows('SELECT result FROM turns ORDER BY rowid LIMIT 1 OFFSET 1')[0][0]),
                              'restart_request': native.requests[-1]}), flush=True)
        assert not reminders(native.requests[-1]), 'success state was reset/reinjected at restart'
        assert 'SUMMARY_SENTINEL' in json.dumps(native.requests[-1]['input'])
        rows = native.panel(['compressions 1', 'compression: available'])
        receipt(native, 'source-failure-no-gain-success-cooldown-restart', reminder_wire=actual + [False],
                before_operations=observed, persisted_state=state, current_panel=rows)


def numeric_buffer(binary):
    # The delivered counter-only fixture expects [0,1,0] for this soft profile.
    # Its file/assertions stay untouched as historical evidence. Exact source
    # last-user pairing instead requires [0,1,1] across two genuine new users,
    # including restart; numerical limits and native buffer accounting agree.
    with Controls(binary, lambda owner, number, req: t50.completed('x' * 80000 if number == 1 else 'closed'),
                  {'compress': 'allow'}) as native:
        path = native.home / 'config/opencode/opencode.json'
        config = json.loads(path.read_text())
        config['provider']['fixture']['models'] = {'m': {'limit': {'context': 40000, 'output': 2048}}}
        config['dcp'] = {'compress': {'minContextLimit': 100, 'maxContextLimit': 50000, 'summaryBuffer': True}}
        path.write_text(json.dumps(config))
        native.start(); native.send(b'prime\r'); closed(native, 1, 1)
        native.send(b'continue\r'); closed(native, 2, 2)
        facts = identity(native)
        assert (facts['context_limit'], facts['dcp_min_context'], facts['dcp_max_context']) == (40000, 100, 50000), facts
        rows = native.panel(['reminders min 100', 'max 50K', 'summary buffer true'])
        native.stop(); native.start()
        reopened = native.panel(['reminders min 100', 'max 50K', 'summary buffer true'])
        # Config/model basis is immutable; transient context estimates are not
        # the published threshold facts (the delivered helper compares these).
        assert [row for row in rows if row.startswith(('reminders min', 'model fixture/'))] == [
            row for row in reopened if row.startswith(('reminders min', 'model fixture/'))]
        native.send(b'after restart\r'); closed(native, 3, 3)
        actual = [bool(reminders(req)) for req in native.requests]
        assert actual == [False, True, True], actual
        assert all('advisory' in hint for req in native.requests for hint in reminders(req))
        assert identity(native)['dcp_min_context'] == 100
        assert native.rows('SELECT count(*) FROM tool_operations') == [(0,)]
        assert native.rows('SELECT count(*) FROM compression_blocks') == [(0,)]
        assert native.physical_requests == 4 and native.auxiliary_requests == 1
        receipt(native, 'source-numeric-buffer-last-user-restart', reminder_wire=actual,
                current_panel=rows, restart_panel=reopened, exact_owner_facts=facts)


def percentage_regression(binary):
    # Reuse delivered assertions unchanged for the six over-max profiles and
    # invalid pre-effect case. The soft profile is directly source-qualified.
    qualify(binary, 'no-file-known', {'context': 40000, 'output': 2048})
    qualify(binary, 'no-file-missing', {})
    qualify(binary, 'no-file-zero', {'context': 0, 'output': 0})
    qualify(binary, 'context-only-warning', {'context': 40000})
    numeric_buffer(binary)
    qualify(binary, 'partial-numeric', {'context': 40000, 'output': 2048},
            {'minContextLimit': 100}, thresholds=(100, 22000))
    qualify(binary, 'exact-provider-nested-model', {'context': 40000, 'output': 2048},
            {'modelMinLimits': {'fixture/nested/m': '25%'},
             'modelMaxLimits': {'fixture/nested/m': '60%'}}, model='nested/m', thresholds=(10000, 24000))
    invalid(binary)


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('binary', type=Path)
    parser.add_argument('--case', choices=['last-user', 'context', 'iteration', 'compression', 'numeric-buffer'])
    parser.add_argument('--defaults-regression', action='store_true')
    args = parser.parse_args(); binary = args.binary.resolve()
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    cases = {'last-user': last_user, 'context': context, 'iteration': iteration, 'compression': compression}
    if args.defaults_regression:
        percentage_regression(binary)
    elif args.case == 'numeric-buffer':
        numeric_buffer(binary)
    else:
        for name, case in cases.items():
            if args.case is None or args.case == name: case(binary)
    after = hashlib.sha256(binary.read_bytes()).hexdigest(); assert before == after
    print(json.dumps({'binary': str(binary), 'sha256_before': before, 'sha256_after': after}), flush=True)


if __name__ == '__main__': main()
