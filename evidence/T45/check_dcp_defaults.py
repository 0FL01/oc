#!/usr/bin/env python3
"""Serial DCP-default atomic gates, reusing the existing lossless log owner."""
import importlib.util
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location('offline_check', HERE/'check_round_removal.py')
check = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check)
check.LOGS = HERE/'dcp-defaults-logs'

if __name__ == '__main__':
    sys.exit(check.main())
