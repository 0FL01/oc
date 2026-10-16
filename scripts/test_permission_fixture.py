"""VIS36 patch fixture uses the actual model-selected file-tool family."""
import io
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).parent / 'tui_capture'))
import apply_patch_fixture
import permission_fixture


class PermissionFixtureTests(unittest.TestCase):
    def test_patch_model_is_explicit_and_schema_admission_stays_strict(self):
        for origin in ('oc', 'upstream'):
            for mode in ('prompt', 'auto-config', 'auto-cli'):
                with self.subTest(origin=origin, mode=mode), tempfile.TemporaryDirectory() as temporary:
                    root = pathlib.Path(temporary)
                    home = root / origin / 'home'
                    (home / 'config/opencode').mkdir(parents=True)
                    providers = 'providers'
                    config = {'model': 'fixture/fixture-model-1', 'plugins': [],
                              providers: {'fixture': {'models': {'fixture-model-1': {'name': 'Patch fixture'}}}}}
                    spec = {'origin': origin, 'isolated_root': temporary,
                            'permission': True, 'permission_mode': mode}
                    cli = {'session': {}}
                    permission_fixture.configure(spec, home, root / 'project', config, cli)
                    model = config['model'].split('/', 1)[1]
                    self.assertEqual(model, 'gpt-fixture-model-1')
                    self.assertEqual(spec['fixture_model'], model)
                    self.assertIn(model, config[providers]['fixture']['models'])
                    self.assertEqual(cli['session']['permissions'], 'autoaccept' if mode == 'auto-config' else 'prompt')
                    server = config['mcp']['servers']['vis36']
                    self.assertIs(server.get('codemode'), False, 'VIS36 calls an advertised direct MCP tool')
                    self.assertEqual(spec['permission_calls']['shell'],
                                     {'name': 'shell', 'arguments': {'command': '/usr/bin/touch shell-marker'}})
                    name = 'apply_patch' if origin == 'oc' else 'patch'
                    request = {'model': model, 'stream': True,
                               'input': [{'role': 'user', 'content': [{'type': 'input_text', 'text': 'VIS36 once: ordinary patch'}]}],
                               'tools': [{'type': 'function', 'name': name,
                                          'parameters': {'properties': {'patchText': {'type': 'string'}}}}]}
                    class Handler:
                        path = '/v1/responses'
                        wfile = io.BytesIO()
                        def send_response(self, status): self.status = status
                        def send_header(self, *args): pass
                        def end_headers(self): pass
                        def send_error(self, status, message): self.status = status
                    with mock.patch.object(apply_patch_fixture, 'index', 0), mock.patch.object(apply_patch_fixture.time, 'sleep'):
                        handler, events = Handler(), []
                        permission_fixture.respond(handler, request, spec, events.append)
                        self.assertEqual(handler.status, 200)
                        self.assertTrue(next(event for event in events if event['kind'] == 'provider')['valid'])
                        for invalid in ({**request, 'tools': []}, {**request, 'model': 'fixture-model-1'}):
                            handler, events = Handler(), []
                            permission_fixture.respond(handler, invalid, spec, events.append)
                            self.assertEqual(handler.status, 400)
                            self.assertFalse(next(event for event in events if event['kind'] == 'provider')['valid'])
                        shell = {**request,
                                 'input': [{'role': 'user', 'content': [{'type': 'input_text', 'text': 'VIS36 shell: ordinary command'}]}],
                                 'tools': [{'type': 'function', 'name': 'shell',
                                            'parameters': {'properties': {'command': {'type': 'string'}}}}]}
                        handler, events = Handler(), []
                        permission_fixture.respond(handler, shell, spec, events.append)
                        self.assertEqual(handler.status, 200)
                        call = next(event for event in events if event['kind'] == 'fixture_tool_call')
                        self.assertEqual(call['name'], 'shell')
                        self.assertEqual(call['arguments'], {'command': '/usr/bin/touch shell-marker'})
                        handler, events = Handler(), []
                        permission_fixture.respond(handler, {**shell, 'tools': []}, spec, events.append)
                        self.assertEqual(handler.status, 400)


if __name__ == '__main__':
    unittest.main()
