import hashlib
import os
from pathlib import Path
import pwd
import tempfile
import unittest
from unittest.mock import patch

import syntax_cache as cache


class ExecuteCache(unittest.TestCase):
    def test_only_json_cache_is_new_and_bundled_typescript_is_not_overridden(self):
        root = Path(pwd.getpwuid(os.getuid()).pw_dir)/'.cache/opencode-tmp/opencode'
        with tempfile.TemporaryDirectory(dir=root, prefix='execute-cache-test-') as directory:
            base = Path(directory)
            home, assets = base/'home', base/'assets'
            home.mkdir()
            assets.mkdir()
            wasm = assets/'tree-sitter-json.wasm'
            wasm.write_bytes(b'unit fixture for validated external WASM')
            original_asset = cache.asset
            def checked_asset(path, expected):
                if path == wasm:
                    self.assertEqual(expected, 'd2119fb98d5912719b13f9458574f8608d2d29dfbe45f6be1f860ea1fe2a2405')
                    return wasm.read_bytes()
                return original_asset(path, expected)
            with patch.object(cache, 'asset', side_effect=checked_asset):
                result = cache.preload_execute(home, assets, root)
                with self.assertRaises(ValueError):
                    cache.preload_execute(home, assets, root)
            self.assertEqual([record['file'] for record in result['files']],
                             ['languages/tree-sitter-json.wasm','queries/json-256dee0a.scm'])
            self.assertFalse(result['bundled_typescript']['override'])
            self.assertEqual(hashlib.sha256((Path(result['cache'])/result['files'][1]['file']).read_bytes()).hexdigest(),
                             result['files'][1]['sha256'])
            self.assertEqual(cache.url_key(result['files'][1]['url']), '256dee0a')

    def test_changed_payload_or_unowned_root_creates_no_cache(self):
        root = Path(pwd.getpwuid(os.getuid()).pw_dir)/'.cache/opencode-tmp/opencode'
        with tempfile.TemporaryDirectory(dir=root, prefix='execute-cache-test-') as directory:
            base = Path(directory)
            home, assets = base/'home', base/'assets'
            home.mkdir()
            assets.mkdir()
            (assets/'tree-sitter-json.wasm').write_bytes(b'changed WASM')
            with self.assertRaises(ValueError):
                cache.preload_execute(home, assets, root)
            self.assertFalse((home/'data').exists())
            with self.assertRaises(ValueError):
                cache.preload_execute(home, assets, base)


if __name__ == '__main__':
    unittest.main()
