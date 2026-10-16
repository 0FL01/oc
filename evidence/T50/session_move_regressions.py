#!/usr/bin/env python3
"""Directed current-catalog native regressions on unchanged normal TOOL19 ELFs."""
import argparse
import json
from pathlib import Path
import sys

import native_background
import native_model_session
import native_search
import model_session_regressions


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = native_model_session.digest(binary)
    native_model_session.SESSION_MOVE_SUPPORTED = True
    native_search.SESSION_MOVE_SUPPORTED = True
    for case in ("catalog", "dynamic", "rename", "models_deny", "child_models"):
        print(json.dumps(native_model_session.run(binary, case), ensure_ascii=False), flush=True)
    model_session_regressions.tui_title(binary)
    # Reuse the delivered bounded question/read/image/search/fetch/instruction
    # and real RET01 fixtures, retaining their historical defaults/assertions.
    sys.argv = [sys.argv[0], str(binary)]
    model_session_regressions.main()
    native_background.busy_notice(binary)
    native_background.review_fork(binary)
    after = native_model_session.digest(binary)
    assert before == after
    print(json.dumps({"sha256_before": before, "sha256_after": after, "status": "PASS",
        "cases": 23, "TOOL18_directed": 5, "real_title_PTY": 1,
        "delivered_directed": 15, "BG_busy_notice": 1, "RET01_real_fork": 1,
        "owned_cleanup": "joined fixtures, exact TempDirs removed"}))


if __name__ == "__main__":
    main()
