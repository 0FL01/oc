import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

import tool_preview_fixture as fixture


class ToolPreviewFixtureTests(unittest.TestCase):
    def test_execute_profile_uses_real_codemode_with_only_exact_nested_authorities(self):
        for origin, tool in [('oc', 'visexecute__output'), ('upstream', 'visexecute_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                spec = {'origin':origin, 'execute_preview':True, 'execute_fixture_port':45678}
                fixture.configure(spec, pathlib.Path(directory), pathlib.Path(directory),
                                  config, {'session':{}})
                server = config['mcp']['servers']['visexecute']
                self.assertIs(server['codemode'], True)
                self.assertEqual(server['command'][-1], '--small')
                self.assertEqual(config['permissions'], [
                    {'action':'*', 'resource':'*', 'effect':'deny'},
                     {'action':'execute', 'resource':'*', 'effect':'allow'},
                     {'action':tool, 'resource':'*', 'effect':'allow'},
                     *[{'action':name, 'resource':'*', 'effect':'allow'} for name in ('read','grep','glob')],
                     *[{'action':'webfetch', 'resource':'http://localhost:45678/'+name, 'effect':'allow'}
                       for name in ('vis46-direct','vis46-fetch')]])
                self.assertNotIn('setCacheKey', config)

    def test_generic_preview_uses_explicit_direct_mcp_on_both_binaries(self):
        for origin, tool in [('oc', 'vis16__output'), ('upstream', 'vis16_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                fixture.configure({'origin': origin}, pathlib.Path(directory),
                                  pathlib.Path(directory), config, {'session': {}})
                self.assertIs(config['mcp']['servers']['vis16']['codemode'], False)
                self.assertEqual(config['permissions'][0]['effect'], 'deny')
                self.assertEqual([rule['action'] for rule in config['permissions']
                                  if rule['effect'] == 'allow'], [tool, 'shell'])

    def test_small_profile_keeps_limits_and_identical_exact_shell_permission(self):
        for origin, tool in [('oc', 'vispreview__output'), ('upstream', 'vispreview_output')]:
            with self.subTest(origin=origin), tempfile.TemporaryDirectory() as directory:
                config = {}
                spec = {'origin':origin, 'tool_preview_small':True}
                fixture.configure(spec, pathlib.Path(directory), pathlib.Path(directory),
                                  config, {'session':{}})
                server = config['mcp']['servers']['vispreview']
                self.assertIs(server['codemode'], False)
                self.assertEqual(server['command'][-1], '--small')
                self.assertEqual(config['tool_output'], {'max_lines':12, 'max_bytes':1024})
                self.assertEqual(config['permissions'], [
                    {'action':'*', 'resource':'*', 'effect':'deny'},
                    {'action':tool, 'resource':'*', 'effect':'allow'},
                    *[{'action':'shell', 'resource':resource, 'effect':'allow'} for resource in
                      ([fixture.SMALL_COMMAND] if origin == 'oc' else
                       [fixture.SMALL_PRINTF, fixture.SMALL_EFFECT])]])
                self.assertEqual(fixture.command(spec), fixture.SMALL_COMMAND)

    def test_small_peer_returns_real_complete_literal_marker_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            requests = [{'jsonrpc':'2.0', 'id':index, 'method':'tools/call',
                         'params':{'name':'output', 'arguments':{'index':index}}}
                        for index in [1,2]]
            peer = pathlib.Path(fixture.__file__).with_name('tool_preview_mcp.py')
            process = subprocess.run([sys.executable, str(peer), '--small'],
                                     input=''.join(json.dumps(request)+'\n' for request in requests),
                                     capture_output=True, text=True, check=True, timeout=10,
                                     env={'HOME':directory})
            results = [json.loads(line)['result'] for line in process.stdout.splitlines()]
            self.assertEqual(len(results), 2)
            for result in results:
                self.assertIs(result['isError'], False)
                body = result['content'][0]['text']
                self.assertLess(len(body.encode()), 1024)
                self.assertLess(len(body.splitlines()), 12)
                for marker in ['[Part preview truncated]',
                               '[output preview truncated; full result retained]', '[truncated]']:
                    self.assertIn(marker, body)


if __name__ == '__main__':
    unittest.main()
