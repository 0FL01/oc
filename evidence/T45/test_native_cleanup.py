#!/usr/bin/env python3
"""No-process regression for the shared owned-PTY cleanup observation race."""
import sys
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "T50"))
from native_background import Native


class CleanupRaceTests(unittest.TestCase):
    def fixture(self):
        native = Native.__new__(Native)
        native.fd = 123
        native.process = Mock()
        native.process.returncode = None
        native.process.poll.return_value = None
        native.reader = Mock()
        native.reader.is_alive.return_value = False
        return native

    def test_exit_between_poll_and_shutdown_write_still_joins_and_checks_status(self):
        native = self.fixture()
        process = native.process

        def write_after_exit(*_):
            process.returncode = 0
            process.poll.return_value = 0
            raise OSError("synthetic closed PTY")

        # Interactive send intentionally rejects a departed process. Shutdown
        # must not reuse that assertion after its earlier liveness observation.
        def interactive_send(_):
            process.returncode = 0
            process.poll.return_value = 0
            raise AssertionError("native application exited")

        native.send = interactive_send
        with patch("native_background.os.write", side_effect=write_after_exit), \
                patch("native_background.os.close") as close:
            native.stop()
        process.wait.assert_called_once_with(timeout=8)
        process.kill.assert_not_called()
        native.reader.join.assert_called_once_with(timeout=1)
        close.assert_called_once_with(123)
        self.assertIsNone(native.process)

    def test_write_error_while_process_alive_is_not_suppressed(self):
        native = self.fixture()
        with patch("native_background.os.write", side_effect=OSError("synthetic failure")), \
                patch("native_background.os.close") as close:
            with self.assertRaises(OSError):
                native.stop()
        close.assert_not_called()
        native.process.wait.assert_not_called()

    def test_natural_nonzero_exit_is_still_a_cleanup_failure(self):
        native = self.fixture()
        native.process.returncode = 1
        native.process.poll.return_value = 1
        with patch("native_background.os.write") as write, \
                patch("native_background.os.close") as close:
            with self.assertRaisesRegex(AssertionError, "cleanup status mismatch"):
                native.stop()
        write.assert_not_called()
        close.assert_not_called()
        native.reader.join.assert_called_once_with(timeout=1)


if __name__ == "__main__":
    unittest.main()
