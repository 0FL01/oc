#!/usr/bin/env python3
"""Synthetic offline peer + normal native ELF + current bounded DCP panel.

Policy provenance: retained DCP3.1.15/11f6517780a502512a3467645074be447cb0369e;
native defaults are the explicit docs/DCP.md:294-365 fork contract.
"""
import argparse
import codecs
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys
import threading

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


t50 = load('defaults_native', 'evidence/T50/native_background.py')
sys.modules.setdefault('native_background', t50)
ui = load('defaults_terminal', 'evidence/T50/native_shell_controls.py')


class Defaults(t50.Native):
    consume_pty = ui.Consumer.consume_pty
    screen = ui.Consumer.screen

    def start(self):
        self.grid_lock = threading.Lock()
        self.grid = [[' ' for _ in range(110)] for _ in range(self.height)]
        self.cursor, self.pending = (0, 0), ''
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        super().start()

    def __exit__(self, *args):
        owned = str(self.root)
        try:
            super().__exit__(*args)
        finally:
            assert not Path(owned).exists()
            print(json.dumps({'owner_cleanup': owned, 'all_joined': True,
                              'exact_temp_removed': True}), flush=True)

    def panel(self, fragments):
        self.send(b'/dcp\r')
        def ready():
            text = '\n'.join(self.screen())
            return text if all(fragment in text for fragment in fragments) else None
        result = t50.until(ready, 'current native DCP panel differs: ' + repr(fragments))
        self.send(b'\x1b')
        t50.until(lambda: 'reminders min' not in '\n'.join(self.screen()), 'DCP close key was not acknowledged')
        return [row[row.index(word):].rstrip() for row in result.splitlines()
                for word in ('reminders min', 'model fixture/', 'budget:') if word in row]


def identity(native):
    result = json.loads(native.rows('SELECT result FROM turns ORDER BY rowid DESC LIMIT 1')[0][0])
    return result['spans'][-1]['request']


def reminder(request):
    return [item for item in request['input']
            if item.get('role') == 'developer' and 'DCP reminder (' in json.dumps(item)]


def qualify(binary, case, limits, fragment=None, model='m', thresholds=None):
    context = limits.get('context') or 32768
    expected = thresholds or (context * 40 // 100, context * 55 // 100)
    fallback = not limits.get('context')
    warning = not limits.get('context') or not limits.get('output')
    answer_bytes = 96000 if context == 40000 else 80000
    def response(owner, number, request):
        assert request.get('tools'), 'unexpected hidden summary request'
        return t50.completed('x' * answer_bytes if number == 1 else 'closed')
    with Defaults(binary, response, {'compress': 'allow'}) as native:
        path = native.home / 'config/opencode/opencode.json'
        config = json.loads(path.read_text())
        config['provider']['fixture']['models'] = {model: {'limit': limits}}
        config['model'] = 'fixture/' + model
        if fragment is not None:
            config['dcp'] = {'compress': fragment}
        path.write_text(json.dumps(config))
        assert not list(native.root.rglob('dcp.json*'))
        native.start()
        native.send(b'prime\r')
        t50.until(lambda: len(native.requests) == 1 and native.settled(1), 'prime did not close')
        t50.until(lambda: re.search(r'Build\s*·\s*(?:fixture/)?' + re.escape(model) + r'\s*·\s*\d',
                                   '\n'.join(native.screen())), 'prime consumer did not reconcile')
        native.send(b'continue\r')
        t50.until(lambda: len(native.requests) == 2 and native.settled(2), 'second native request did not close')
        facts = identity(native)
        assert (facts['context_limit'], facts['dcp_min_context'], facts['dcp_max_context']) == (context, *expected), facts
        assert facts['model']['id'] == model and facts['model']['provider'] == 'fixture', facts
        assert facts['input_limit'] != facts['dcp_max_context'], facts
        assert not reminder(native.requests[0])
        assert len(reminder(native.requests[1])) == 1, json.dumps(native.requests[1]['input'])[:500]
        if expected[1] == 50000:
            assert 'advisory' in json.dumps(reminder(native.requests[1]))
        else:
            assert 'required before more work' in json.dumps(reminder(native.requests[1]))
        buffered = bool(fragment and fragment.get('summaryBuffer'))
        # Existing UI formatter rounds display; assertions use exact owner facts above.
        def fmt(value):
            return f'{value/1000:.1f}'.rstrip('0').rstrip('.') + 'K' if value >= 1000 else str(value)
        fragments = [f'reminders min {fmt(expected[0])}', f'max {fmt(expected[1])}',
                     'summary buffer ' + str(buffered).lower(), f'model fixture/{model}',
                     'native fallback cap' if fallback else 'model metadata']
        if warning:
            fragments.append('budget:')
        rows = native.panel(fragments)
        native.stop()
        native.start()
        reopened = native.panel(fragments)
        assert rows == reopened, (rows, reopened)
        native.send(b'after restart\r')
        t50.until(lambda: len(native.requests) == 3 and native.settled(3), 'restart continuation did not close')
        assert not reminder(native.requests[2]), 'cadence was reset by restart'
        assert identity(native)['dcp_min_context'] == expected[0]
        assert native.rows('SELECT count(*) FROM tool_operations') == [(0,)]
        assert native.rows('SELECT count(*) FROM compression_blocks') == [(0,)]
        lanes = dict(native.rows("SELECT json_extract(payload,'$.lane'),count(*) FROM events WHERE kind='generation_dispatched' GROUP BY 1"))
        assert lanes == {'main': 3, 'title': 1}, lanes
        assert native.physical_requests == 4 and len(native.requests) == 3 and native.auxiliary_requests == 1
        assert not native.errors, native.errors
        print(json.dumps({'case': case, 'status': 'PASS', 'model': 'fixture/' + model,
                          'context': context, 'reminders': expected, 'input_limit': facts['input_limit'],
                          'current_panel': rows, 'restart_same': True, 'physical_posts': 4,
                          'main_posts': 3, 'auxiliary_posts': 1, 'dispatch_lanes': lanes, 'hidden_summary_posts': 0,
                          'compress_operations': 0, 'cadence_wire': [0, 1, 0]}), flush=True)


def invalid(binary):
    with Defaults(binary, lambda *_: t50.completed(), {'compress': 'allow'}) as native:
        path = native.home / 'config/opencode/opencode.json'
        config = json.loads(path.read_text())
        config['dcp'] = {'compress': {'minContextLimit': '60%'}}
        path.write_text(json.dumps(config))
        native.start()
        native.send(b'must fail before effects\r')
        try:
            t50.until(lambda: 'invalid_config' in '\n'.join(native.screen()),
                      'invalid effective threshold did not paint its pre-effect error')
        except AssertionError as error:
            raise AssertionError(str(error) + '\n' + '\n'.join(native.screen()) + '\n' +
                                 repr(native.rows('SELECT status,result FROM turns')) + '\n' +
                                 repr((native.physical_requests, native.errors))) from error
        assert native.physical_requests == 0
        assert native.rows('SELECT count(*) FROM tool_operations') == [(0,)]
        assert native.rows('SELECT count(*) FROM turns') == [(0,)]
        print(json.dumps({'case': 'invalid-effective-min-over-max', 'status': 'PASS',
                          'physical_posts': 0, 'tool_effects': 0}), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('binary', type=Path)
    parser.add_argument('--invalid-only', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve()
    if args.invalid_only:
        invalid(binary)
        return
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    qualify(binary, 'no-file-known', {'context': 40000, 'output': 2048})
    qualify(binary, 'no-file-missing', {})
    qualify(binary, 'no-file-zero', {'context': 0, 'output': 0})
    qualify(binary, 'context-only-warning', {'context': 40000})
    qualify(binary, 'explicit-numeric-buffer', {'context': 40000, 'output': 2048},
            {'minContextLimit': 100, 'maxContextLimit': 50000, 'summaryBuffer': True}, thresholds=(100, 50000))
    qualify(binary, 'partial-numeric', {'context': 40000, 'output': 2048},
            {'minContextLimit': 100}, thresholds=(100, 22000))
    qualify(binary, 'exact-provider-nested-model', {'context': 40000, 'output': 2048},
            {'modelMinLimits': {'fixture/nested/m': '25%'},
             'modelMaxLimits': {'fixture/nested/m': '60%'}}, model='nested/m', thresholds=(10000, 24000))
    invalid(binary)
    after = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert after == before
    print(json.dumps({'normal_elf': str(binary.relative_to(ROOT)), 'sha256_before': before,
                      'sha256_after': after, 'cases': 8, 'physical_posts': 28,
                      'main_posts': 21, 'auxiliary_posts': 7, 'hidden_summary_posts': 0}), flush=True)


if __name__ == '__main__':
    main()
