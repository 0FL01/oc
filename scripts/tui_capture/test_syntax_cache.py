import hashlib
import importlib.util
import pathlib
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("syntax_cache", pathlib.Path(__file__).with_name("syntax_cache.py"))
cache = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cache)


class SyntaxCache(unittest.TestCase):
    def test_real_download_keys_use_utf16_and_signed_js_int32(self):
        values = ["https://raw.githubusercontent.com/alex-pinkus/tree-sitter-swift/main/queries/highlights.scm",
                  "https://example.test/😀/query.scm", ""]
        import json
        script = "import {DownloadUtils} from '/home/opencode/.cache/opencode-tmp/opencode/t44-opentui-0.5.10/packages/core/src/lib/tree-sitter/download-utils.ts'; console.log(JSON.stringify(" + json.dumps(values) + ".map(value=>DownloadUtils.hashUrl(value))));"
        actual = json.loads(subprocess.check_output(["bun", "-e", script], text=True, timeout=30))
        self.assertEqual(actual, [cache.url_key(value) for value in values])

    def test_complete_original_url_cache_is_sealed_and_cannot_replace_old_data(self):
        with tempfile.TemporaryDirectory(dir=cache.CACHE, prefix="syntax-cache-check-") as directory:
            home = pathlib.Path(directory)
            result = cache.preload(home)
            grammar_files = [record for record in result["files"] if record["kind"] == "grammar"]
            self.assertEqual(len(grammar_files), 34)
            for record in result["files"]:
                self.assertEqual(hashlib.sha256((pathlib.Path(result["cache"]) / record["file"]).read_bytes()).hexdigest(), record["sha256"])
            swift = next(record for record in result["files"] if record["kind"] == "highlights" and "/tree-sitter-swift/main/" in record["url"])
            self.assertIn("/tree-sitter-swift/0.7.1/", swift["source_url"])
            with self.assertRaises(ValueError):
                cache.preload(home)


if __name__ == "__main__":
    unittest.main()
