#!/usr/bin/env python3
"""DCP12 normal native ELF: exact synthetic wire, current PTY and SQLite facts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
from native_dcp_defaults import Defaults, t50


class Controls(Defaults):
    def panel(self, fragments, refuse_enter=False):
        self.send(b'/dcp\r')
        def ready():
            rows=self.screen()
            return rows if all(fragment in '\n'.join(rows) for fragment in fragments) else None
        rows=t50.until(ready,'current controls panel differs: '+repr(fragments))
        self.send(b'\r\x1b' if refuse_enter else b'\x1b')
        t50.until(lambda: not any('DCP context' in row and 'esc' in row for row in self.screen()),'DCP panel did not close')
        return [row.strip() for row in rows if any(fragment in row for fragment in ('context ','blocks ','compression:','reminders min','model fixture/'))]

    def __exit__(self, kind, value, tb):
        if kind:
            print(json.dumps({'failure_screen': self.screen(), 'request_count':len(self.requests),
                              'turns': self.rows('SELECT status FROM turns'),
                              'tools': self.rows('SELECT name,state FROM tool_operations'),
                              'peer_errors':self.errors}),flush=True)
        # Use the shared stop bounds. Even a failed normal-stop assertion must
        # join/reap this fixture's resources before its exact TempDir cleanup.
        owned=self.root
        try:
            self.stop()
        finally:
            if self.process is not None:
                assert self.process.poll() is not None, 'owned native process not reaped'
                self.reader.join(timeout=1)
                assert not self.reader.is_alive(), 'owned PTY reader not joined'
                os.close(self.fd)
                self.process=None
            self.server.shutdown()
            self.server.server_close()
            self.thread.join(timeout=2)
            assert not self.thread.is_alive(), 'owned HTTP worker not joined'
            self.temp.cleanup()
            assert not owned.exists()
            print(json.dumps({'owner_cleanup':str(owned),'all_joined':True,'exact_temp_removed':True}),flush=True)


def configure(native, fragment):
    (native.project / 'dcp.jsonc').write_text(json.dumps(fragment))
    native.env['OC_STARTUP_TRACE'] = str(native.root / 'trace.log')


def wait_closed(native, count):
    t50.until(lambda: len(native.requests) == count and native.settled(count),
              'native control turn did not settle')
    wait_idle(native)


def wait_idle(native):
    def ready():
        rows=native.screen()
        return (any(row.strip().startswith('Build · m ·') for row in rows)
                and not any('esc interrupt' in row or 'esc again to interrupt' in row for row in rows))
    t50.until(ready, 'current terminal turn footer not consumed')


def names(request):
    return [tool['name'] for tool in request.get('tools', [])]


def input_text(request):
    return json.dumps(request['input'], ensure_ascii=False)


def ordinary(request):
    assert 'compress' not in names(request)
    text = input_text(request)
    assert 'DCP context anchors' not in text and 'closed=true' not in text
    assert 'DCP reminder (' not in text
    assert 'Explicit manual DCP compression admitted' not in text
    assert 'Stable text-message IDs' in text


def trace(native, debug):
    text = (native.root / 'trace.log').read_text()
    lines = [line for line in text.splitlines() if 'dcp.debug:' in line]
    assert bool(lines) == debug, lines
    for line in lines:
        assert re.fullmatch(r'\+\d+ms pid=\d+ dcp.debug: dcp (request.prepare|compress.committed) nudges=\d+ compressions=\d+ prunes=\d+', line), line
        for forbidden in ('SELECTED_FACT', 'SUMMARY_SENTINEL', str(native.root), 'synthetic-trap', 'arguments'):
            assert forbidden not in line
    return lines


def receipt(native, case, **facts):
    print(json.dumps(dict(case=case, status='PASS', physical_posts=native.physical_requests,
                         main_posts=len(native.requests), auxiliary_posts=native.auxiliary_requests,
                         exact_requests=native.requests, sql=native.rows('SELECT name,state FROM tool_operations ORDER BY rowid'),
                         **facts)), flush=True)
    assert not native.errors, native.errors


def manual(binary, ask=False):
    def response(owner, number, request):
        if number == 1:
            return t50.completed('SELECTED_FACT ' + 'discard closed filler ' * 100)
        if number == 2:
            anchor = next(item['content'][0]['text'] for item in request['input']
                          if 'DCP context anchors' in json.dumps(item))
            anchors = json.loads(anchor[anchor.index('[{'):])
            closed = [item for item in anchors if item['closed']]
            return t50.tool('compress', {'topic': 'manual atomic', 'content': [{
                'startId': closed[0]['id'], 'endId': closed[-1]['id'],
                'summary': 'SUMMARY_SENTINEL SELECTED_FACT retained; next work'}]}, 'manual-call')
        return t50.completed('done')
    with Controls(binary, response, {'compress': 'ask' if ask else 'allow'}) as native:
        configure(native, {'manualMode': {'enabled': True}, 'debug': True,
                           'compress': {'minContextLimit': 1, 'maxContextLimit': 1}})
        native.start()
        native.send(b'Manual context compression request. Call the compress tool ordinary text.\r')
        wait_closed(native, 1)
        ordinary(native.requests[0])
        native.send(b'/dcp-compress preserve SELECTED_FACT\r')
        t50.until(lambda: len(native.requests) >= 2, 'manual command did not reach provider')
        assert 'compress' in names(native.requests[1]), 'genuine manual schema missing'
        if ask:
            t50.until(lambda: 'Permission required' in '\n'.join(native.screen()), 'real manual Ask consumer absent')
            native.send(b'\r')
        t50.until(lambda: len(native.requests) == 3 and native.rows('SELECT count(*) FROM compression_blocks') == [(1,)]
                  and native.settled(2), 'manual tool did not really commit')
        assert native.rows("SELECT name,state FROM tool_operations") == [('compress', 'completed')]
        assert 'discard closed filler' not in input_text(native.requests[2])
        t50.until(lambda: 'compressions 1' in '\n'.join(native.screen()), 'current manual completion panel not refreshed')
        wait_idle(native)
        native.send(b'\x1b')
        t50.until(lambda: not any('DCP context' in row and 'esc' in row for row in native.screen()), 'manual panel did not close')
        native.send(b'ordinary next turn\r')
        t50.until(lambda: len(native.requests) == 4 and native.settled(3), 'ordinary next turn did not finish')
        ordinary(native.requests[3])
        assert 'SUMMARY_SENTINEL' in input_text(native.requests[3])
        rows=native.panel(['explicit manual compression required', 'manual: available', 'compressions 1'])
        lines=trace(native, True)
        native.stop(); native.start()
        native.panel(['explicit manual compression required', 'manual: available', 'compressions 1'])
        native.send(b'after restart\r')
        t50.until(lambda: len(native.requests) == 5 and native.settled(4), 'manual restart ordinary turn failed')
        ordinary(native.requests[4])
        assert 'SUMMARY_SENTINEL' in input_text(native.requests[4])
        assert native.rows('SELECT count(*) FROM compression_blocks') == [(1,)]
        receipt(native, 'manual-ask' if ask else 'manual-allow', current_panel=rows,
                manual_schema=[False,True,True,False,False], debug_metadata=lines)


def refusal(binary, fragment, reason):
    with Controls(binary, lambda *_: t50.completed('closed'), {'compress': 'allow'}) as native:
        configure(native, fragment)
        native.start()
        native.send(b'prime\r'); wait_closed(native, 1)
        before=native.rows('SELECT count(*) FROM turns')[0][0]
        native.send(b'/dcp-compress refuse\r')
        t50.until(lambda: reason in '\n'.join(native.screen()), 'explicit refusal not consumed')
        assert len(native.requests)==1
        assert native.rows('SELECT count(*) FROM turns')==[(before,)]
        assert native.rows('SELECT count(*) FROM tool_operations')==[(0,)]
        assert native.rows('SELECT count(*) FROM compression_blocks')==[(0,)]
        native.send(b'\x1b\x15')
        native.panel([reason], refuse_enter=True)
        assert len(native.requests)==1
        assert native.rows('SELECT count(*) FROM turns')==[(before,)]
        assert native.rows('SELECT count(*) FROM tool_operations')==[(0,)]
        assert native.rows('SELECT count(*) FROM compression_blocks')==[(0,)]
        receipt(native, 'zero-effects-'+reason, debug_metadata=trace(native,False), new_turns=0, panel_enter_refused=True)


def stale_and_commands(binary):
    def response(owner, number, request):
        if number == 1:
            return t50.tool('compress', {'topic':'stale', 'content':[{'startId':'alien','endId':'alien','summary':'must not store'}]}, 'stale-call')
        return t50.completed('done')
    with Controls(binary,response,{'compress':'allow'}) as native:
        configure(native, {'compress': {'enabled':False}})
        native.start(); native.send(b'raw word compress stays\r')
        t50.until(lambda: len(native.requests)==2 and native.settled(1), 'stale call not paired')
        wait_idle(native)
        for req in native.requests: ordinary(req)
        paired=[item for item in native.requests[1]['input'] if item.get('type')=='function_call_output']
        assert len(paired)==1 and paired[0]['call_id']=='stale-call'
        assert 'excluded by issuing request' in paired[0]['output']
        assert native.rows('SELECT name,state FROM tool_operations')==[('compress','failed')]
        assert native.rows('SELECT count(*) FROM compression_blocks')==[(0,)]
        # Reload enables the model tool but unregisters both DCP commands.
        configure(native, {'commands': {'enabled':False}})
        native.send(b'/reload\r')
        t50.until(lambda: 'Configuration reloaded' in '\n'.join(native.screen()), 'reload did not acknowledge')
        native.send(b'/d')
        t50.until(lambda: any(row.strip().startswith('\u2503  /d') for row in native.screen()), 'completion input did not paint')
        assert 'dcp-compress' not in '\n'.join(native.screen())
        native.send(b'\x7f\x7f\x10')
        t50.until(lambda: 'Commands' in '\n'.join(native.screen()), 'palette did not open')
        screen='\n'.join(native.screen()); assert 'DCP context' not in screen and 'Compress DCP' not in screen
        native.send(b'\x1b')
        t50.until(lambda: not any('Commands' in row and 'esc' in row for row in native.screen()), 'commands palette did not close')
        native.send(b'commands off tool still enabled\r')
        t50.until(lambda: len(native.requests)==3 and native.settled(2), 'commands-off ordinary turn failed')
        assert 'compress' in names(native.requests[2])
        native.stop(); native.start()
        native.send(b'/dcp\r')
        t50.until(lambda: 'No matching commands' in '\n'.join(native.screen()), 'restart hidden commands completion gate absent')
        assert len(native.requests)==3
        native.send(b'\x7f\x7f\x7f\x7f')
        assert native.root.is_dir()
        destination=native.root/'destination'; destination.mkdir()
        native.send(('/location '+str(destination)+'\r').encode())
        t50.until(lambda: 'destination' in '\n'.join(native.screen()), 'native Location did not publish')
        native.send(b'destination request\r')
        t50.until(lambda: len(native.requests)==4 and native.settled(3),'destination request did not finish')
        assert 'compress' in names(native.requests[3])
        assert 'Explicit manual DCP compression admitted' not in input_text(native.requests[3])
        assert native.rows('SELECT count(*) FROM compression_blocks')==[(0,)]
        location_panel=native.panel(['compression: available','manual: available'])
        receipt(native,'stale-pair-and-commands-reload-restart', paired_refusal=paired,
                debug_metadata=trace(native,False), current_palette=screen,
                stale_slash_refusal='No matching commands', location_panel=location_panel)


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('binary',type=Path); parser.add_argument('--manual-only',action='store_true')
    args=parser.parse_args(); binary=args.binary.resolve(); before=hashlib.sha256(binary.read_bytes()).hexdigest()
    manual(binary)
    if not args.manual_only:
        manual(binary,True)
        refusal(binary,{'compress':{'enabled':False,'permission':'ask'}},'compression disabled')
        refusal(binary,{'enabled':False},'DCP disabled')
        refusal(binary,{'compress':{'permission':'deny'}},'compression denied')
        stale_and_commands(binary)
    after=hashlib.sha256(binary.read_bytes()).hexdigest(); assert before==after
    print(json.dumps({'binary':str(binary),'sha256_before':before,'sha256_after':after,'unchanged':True}),flush=True)

if __name__=='__main__': main()
