from contextlib import contextmanager
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import archive_failed_capture as archive


class FailedCaptureArchiveTests(unittest.TestCase):
    @contextmanager
    def failed_campaign(self, campaign="model-shell-001"):
        with tempfile.TemporaryDirectory(prefix="failed-archive-test-", dir=archive.TEMP) as private:
            repo = Path(private)
            evidence = repo / "evidence/tui"
            root = evidence / campaign
            for side, status in (("oc", "FAILED"), ("upstream", "PASS_BEHAVIOR_ONLY")):
                target = root / side
                target.mkdir(parents=True)
                (target / "tool-preview-checks.json").write_text(json.dumps({"status": status}))
            (root / "oc/frame.vt").write_bytes(b"\x1b[?1049hcomplete bytes \xe4\xb8\xad\x00\n")
            with patch.multiple(archive, REPO=repo, EVIDENCE=evidence, TEMP=repo), \
                    patch.object(archive.subprocess, "check_output", return_value=b""), \
                    patch.object(archive.resource, "setrlimit"), \
                    patch.object(archive, "owned_allocation", return_value=1024**2) as quota:
                yield root, quota

    def test_explicit_model_shell_preflight_requires_actual_failure_not_pixel_difference(self):
        for campaign in ("model-shell-001", "model-shell-002", "model-shell-004", "model-shell-005", "model-shell-006", "model-shell-008", "model-shell-009", "model-shell-019"):
            with self.subTest(campaign=campaign), self.failed_campaign(campaign) as (root, _), \
                    patch.object(sys, "argv", ["archive", campaign]):
                before = archive.snapshot(root)
                with patch.object(sys, "stdout", new_callable=io.StringIO) as output:
                    archive.main()
                summary = json.loads(output.getvalue())
                self.assertEqual(summary["state"], "VERIFIED_PREFLIGHT")
                self.assertEqual(summary["statuses_unchanged"], {"oc": "FAILED", "upstream": "PASS_BEHAVIOR_ONLY"})
                self.assertEqual(archive.snapshot(root), before)
                self.assertFalse((root.parent / (campaign + ".tar.xz")).exists())
                (root / "oc/tool-preview-checks.json").write_text(json.dumps({"status": "PASS_BEHAVIOR_ONLY"}))
                (root / "capture.lock.json").write_text(json.dumps({"attempts": [{"status": "DIFFERENT"}]}))
                with patch.object(archive, "archive_bytes") as encode:
                    with self.assertRaisesRegex(ValueError, "actual failed"):
                        archive.main()
                    encode.assert_not_called()

    def test_reserved_capacity_boundary_refuses_before_any_transaction(self):
        with self.failed_campaign() as (root, quota):
            with patch.object(sys, "argv", ["archive", "model-shell-001"]), \
                    patch.object(sys, "stdout", new_callable=io.StringIO) as output:
                archive.main()
            predicted = json.loads(output.getvalue())["predicted_after_allocated"]
            reservation = archive.CAP - predicted
            before = archive.snapshot(root)
            with patch.object(sys, "argv", ["archive", "model-shell-001", "--reserve-bytes", str(reservation)]), \
                    patch.object(sys, "stdout", new_callable=io.StringIO) as output:
                archive.main()
            self.assertEqual(json.loads(output.getvalue())["reserved_bytes"], reservation)
            for value in (reservation + 1, -1, archive.CAP + 1):
                with self.subTest(reserve=value), \
                        patch.object(sys, "argv", ["archive", "model-shell-001", "--archive", "--reserve-bytes", str(value)]), \
                        patch.object(archive.tempfile, "mkdtemp") as transaction, \
                        patch.object(archive, "durable") as write:
                    with self.assertRaisesRegex(ValueError, "reserv|cap"):
                        archive.main()
                    transaction.assert_not_called()
                    write.assert_not_called()
                    self.assertEqual(archive.snapshot(root), before)
            quota.assert_called()

    def test_post_rename_failure_keeps_quarantine_and_every_original_byte(self):
        with self.failed_campaign() as (root, _):
            before = archive.snapshot(root)
            private = archive.TEMP / "interrupted-transaction"
            private.mkdir()
            real_write = archive.durable

            def fail_public_write(path, data):
                if path == root.parent / "model-shell-001.tar.xz":
                    raise OSError("injected public write failure")
                real_write(path, data)

            with patch.object(sys, "argv", ["archive", "model-shell-001", "--archive"]), \
                    patch.object(sys, "stdout", new_callable=io.StringIO), \
                    patch.object(archive.tempfile, "mkdtemp", return_value=str(private)), \
                    patch.object(archive, "durable", side_effect=fail_public_write), \
                    patch.object(archive.shutil, "rmtree") as cleanup:
                with self.assertRaisesRegex(OSError, "public write failure"):
                    archive.main()
                cleanup.assert_not_called()
            self.assertEqual(archive.snapshot(private / root.name), before)
            self.assertTrue((private / "candidate.tar.xz").is_file())
            self.assertTrue((private / "transaction.json").is_file())
            self.assertFalse((root.parent / "model-shell-001.tar.xz").exists())

    def test_final_reservation_refusal_keeps_quarantine_and_verified_durable_archive(self):
        with self.failed_campaign() as (root, quota):
            before = archive.snapshot(root)
            private = archive.TEMP / "capacity-changed-transaction"
            private.mkdir()
            reservation = 262144
            quota.side_effect = [1024**2, archive.CAP - reservation + 1]
            with patch.object(sys, "argv", ["archive", "model-shell-001", "--archive", "--reserve-bytes", str(reservation)]), \
                    patch.object(sys, "stdout", new_callable=io.StringIO), \
                    patch.object(archive.tempfile, "mkdtemp", return_value=str(private)), \
                    patch.object(archive.shutil, "rmtree") as cleanup:
                with self.assertRaisesRegex(ValueError, "final.*reservation"):
                    archive.main()
                cleanup.assert_not_called()
            self.assertEqual(archive.snapshot(private / root.name), before)
            archive.audit((root.parent / "model-shell-001.tar.xz").read_bytes(), *before)
            self.assertTrue((root / "ARCHIVED.md").is_file())
            self.assertTrue((private / "transaction.json").is_file())

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
