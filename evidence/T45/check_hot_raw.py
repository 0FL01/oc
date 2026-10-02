#!/usr/bin/env python3
"""R9 logs use the existing single offline owner/watchdog, separate evidence."""
import importlib.util
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('offline_check', HERE/'check_round_removal.py')
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)
check.LOGS = HERE/'hot-raw-logs'

if __name__ == '__main__':
    sys.exit(check.main())
