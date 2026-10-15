import pathlib
import tempfile
import unittest

import tool_preview_fixture as fixture


class ToolPreviewFixtureTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
