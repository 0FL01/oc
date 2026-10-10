import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import archive_failed_capture as archive


class FailedCaptureArchiveTests(unittest.TestCase):
    def test_complete_bytes_and_directory_metadata_roundtrip(self):
        with tempfile.TemporaryDirectory(prefix="failed-archive-test-", dir=archive.TEMP) as private:
            root = Path(private) / "failed-own-capture"
            nested = root / "oc"
            nested.mkdir(parents=True)
            (nested / "frame.vt").write_bytes(b"\x1b[?1049hfull raw \xe4\xb8\xad\x00\n")
            os.chmod(nested, 0o750)
            os.utime(nested, ns=(1_001_000_000_123_456_789, 1_002_000_000_987_654_321))
            files, rows, directories = archive.snapshot(root)
            encoded = archive.archive_bytes(files, rows, directories)
            archive.audit(encoded, files, rows, directories)
            self.assertEqual(directories[1][1:],
                             [0o750, os.getuid(), os.getgid(), 1_002_000_000_987_654_321])
            bad_metadata = [row[:] for row in directories]
            bad_metadata[1][-1] += 1
            with self.assertRaisesRegex(ValueError, "metadata"):
                archive.audit(encoded, files, rows, bad_metadata)
            with self.assertRaisesRegex(ValueError, "framing"):
                archive.audit(encoded + b"junk", files, rows, directories)

    def test_explicit_encoded_bound_and_existing_write_refusal(self):
        with patch.object(archive, "ENCODED", 8):
            with self.assertRaisesRegex(ValueError, "encoded-archive"):
                archive.archive_bytes({"failed/a": b"a"},
                                      [["failed/a", 1, archive.sha(b"a"), 0o644, 1003, 1003, 0]],
                                      [["failed", 0o755, 1003, 1003, 0]])
        with tempfile.TemporaryDirectory(prefix="failed-archive-test-", dir=archive.TEMP) as private:
            target = Path(private) / "immutable"
            archive.durable(target, b"first\x00\xff")
            with self.assertRaises(FileExistsError):
                archive.durable(target, b"replacement")
            self.assertEqual(target.read_bytes(), b"first\x00\xff")


if __name__ == "__main__":
    unittest.main()
