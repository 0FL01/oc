#!/usr/bin/env python3
"""Physical-size CLI regressions; tiny isolated Git repositories, no workspace writes."""

import contextlib
import io
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import code_size


class CodeSizeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="oc-code-size-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.run_git("init", "-q")

    def run_git(self, *args):
        result = subprocess.run(["git", "-C", str(self.root), *args], capture_output=True, check=True)
        return result.stdout.decode().strip()

    def write(self, path, data):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        return target

    def commit(self):
        self.run_git("add", "crates", "scripts")
        # Command-local identity, no changes to repository or authoring Git config.
        self.run_git("-c", "user.name=Size test", "-c", "user.email=size@example.invalid",
                     "commit", "-qm", "baseline")
        return self.run_git("rev-parse", "HEAD")

    def test_physical_lines_and_unicode_bytes(self):
        for lines in (4999, 5000, 5001):
            for ending in (b"\n", b"\r\n"):
                for final_lf in (True, False):
                    with self.subTest(lines=lines, ending=ending, final_lf=final_lf):
                        data = ("界".encode() + ending) * lines
                        if not final_lf:
                            data = data[:-len(ending)]
                        self.assertEqual(code_size.measure(data), {"lines": lines, "bytes": len(data)})
        self.assertEqual(code_size.measure(b""), {"lines": 0, "bytes": 0})

    def test_warning_is_strictly_greater_and_exit_zero(self):
        for lines in (4999, 5000, 5001):
            self.write(f"crates/a/src/f{lines}.rs", b"x\n" * lines)
        with patch("pathlib.Path.cwd", return_value=self.root), contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(code_size.main([]), 0)
        text = out.getvalue()
        self.assertIn("WARN >5000 crates/a/src/f5001.rs", text)
        self.assertNotIn("f5000.rs", text)
        self.assertNotIn("f4999.rs", text)

    def test_base_staged_unstaged_new_rename_delete_and_deduplication(self):
        for name in ("staged", "unstaged", "renamed", "deleted", "unchanged"):
            self.write(f"crates/a/src/{name}.rs", b"// baseline\n")
        self.write("scripts/a.py", b"pass\n")
        base = self.commit()
        self.write("crates/a/src/staged.rs", b"// staged\n// next\n")
        self.run_git("add", "crates/a/src/staged.rs")
        self.write("crates/a/src/unstaged.rs", b"// different\n")
        self.write("crates/a/src/new.rs", b"// new\n")
        self.write("crates/a/src/new_staged.rs", b"// new staged\n")
        self.run_git("add", "crates/a/src/new_staged.rs")
        self.run_git("mv", "crates/a/src/renamed.rs", "crates/a/src/renamed_next.rs")
        (self.root / "crates/a/src/deleted.rs").unlink()
        records = code_size.inventory(self.root, base)
        items = {item["path"]: item for item in records}
        self.assertEqual(len(items), len(records))
        for name in ("staged", "unstaged"):
            self.assertEqual(items[f"crates/a/src/{name}.rs"]["status"], "modified")
        for name in ("new", "new_staged", "renamed_next"):
            self.assertEqual(items[f"crates/a/src/{name}.rs"]["status"], "new")
            self.assertIsNone(items[f"crates/a/src/{name}.rs"]["base"])
        for name in ("renamed", "deleted"):
            self.assertEqual(items[f"crates/a/src/{name}.rs"]["status"], "deleted")
            self.assertIsNone(items[f"crates/a/src/{name}.rs"]["current"])
        self.assertEqual(items["crates/a/src/unchanged.rs"]["status"], "unchanged")
        with patch("pathlib.Path.cwd", return_value=self.root), contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(code_size.main(["--base", base, "--changed", "--json"]), 0)
        self.assertNotIn('"path": "crates/a/src/unchanged.rs"', out.getvalue())

    def test_scope_roles_excluded_data_and_symlinks(self):
        for path in ("crates/a/src/app.rs", "crates/a/src/app/tests.rs", "crates/a/src/app/tests/input.rs",
                     "crates/a/src/application_tests.rs", "crates/a/tests/runtime/turns.rs", "scripts/a.py"):
            self.write(path, b"// code\n")
        for path in ("crates/a/tests/fixtures/data.json", "crates/a/src/vendor/a.rs", "scripts/assets/a.py",
                     "evidence/a.rs", "references/a.rs", ".opencode/a.rs", "opencode/a.rs"):
            self.write(path, b"// not inventory\n")
        self.write("crates/a/src/mixed.rs", b"#[cfg(test)]\nmod tests {}\n")
        external = self.write("outside.rs", b"\xff")
        (self.root / "crates/a/src/link.rs").symlink_to(external)
        (self.root / "crates/a/src/outside.rs").symlink_to("/unavailable-outside-source")
        items = {item["path"]: item for item in code_size.inventory(self.root)}
        self.assertEqual(len(items), 9)
        self.assertEqual(items["crates/a/src/app.rs"]["role"], "source")
        self.assertEqual(items["crates/a/src/mixed.rs"]["role"], "mixed/unknown")
        self.assertEqual(items["crates/a/src/app/tests/input.rs"]["role"], "test")
        self.assertEqual(items["crates/a/src/application_tests.rs"]["role"], "test")
        self.assertEqual(items["scripts/a.py"]["role"], "dev-tool")
        self.assertEqual(items["crates/a/src/link.rs"]["status"], "excluded-symlink")
        self.assertEqual(items["crates/a/src/outside.rs"]["status"], "excluded-symlink")

    def test_git_cli_io_and_invalid_encoding_errors_are_visible(self):
        with patch("pathlib.Path.cwd", return_value=self.root), contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(code_size.main(["--base", "no-such-commit"]), 1)
        self.assertIn("code_size: git", err.getvalue())
        self.write("crates/a/src/bad.rs", b"\xff")
        with patch("pathlib.Path.cwd", return_value=self.root), contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(code_size.main([]), 1)
        self.assertIn("code_size:", err.getvalue())
        with patch("pathlib.Path.cwd", return_value=self.root), patch("code_size.git", side_effect=OSError("unavailable git")), contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(code_size.main([]), 1)
        self.assertIn("unavailable git", err.getvalue())
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            code_size.main(["--changed"])
        self.assertNotEqual(error.exception.code, 0)
        with patch("pathlib.Path.cwd", return_value=self.root), patch("pathlib.Path.read_bytes", side_effect=OSError("unreadable")), contextlib.redirect_stderr(io.StringIO()) as err:
            self.assertEqual(code_size.main([]), 1)
        self.assertIn("unreadable", err.getvalue())


if __name__ == "__main__":
    unittest.main()
