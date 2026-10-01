#!/usr/bin/env python3
"""Directed existing native regressions after the last normal TOOL18 builds."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

import native_background
import native_question
import native_read
import native_search
import native_webfetch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "T45"))
import native_instructions


def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def tui_title(binary):
    title = "R7TITLE7"
    def script(owner, count, request):
        definitions = {tool["name"]: tool for tool in request["tools"]}
        assert definitions["opencode_session_rename"]["parameters"]["required"] == ["title"]
        return native_background.tool("opencode_session_rename", {"title": "  " + title + "  "}, "title-call") if count == 1 else native_background.completed()
    with native_background.Native(binary, script, {"opencode_session_rename": "allow"}) as native:
        native.start()
        native.send(b"Rename native session\r")
        native_background.until(native.settled, "native rename turn did not settle")
        assert native.rows("SELECT title FROM sessions WHERE id='t50-background'") == [(title,)]
        assert native.rows("SELECT count(*) FROM events WHERE kind='session_updated'") == [(1,)]
        def painted_header():
            grid = [[" "] * 110 for _ in range(34)]
            row = col = 0
            for part in re.split(r"(\x1b\[[0-9;?]*[ -/]*[@-~])", native.tail.decode("utf-8", "replace")):
                if part.startswith("\x1b["):
                    if part[-1] in "Hf":
                        values = [int(v or "1") for v in part[2:-1].split(";")]
                        row, col = (values + [1])[0] - 1, (values + [1])[1] - 1
                    elif part.endswith("2J"):
                        grid = [[" "] * 110 for _ in range(34)]
                    elif part.endswith("K") and 0 <= row < 34:
                        grid[row][max(col, 0):] = [" "] * (110 - max(col, 0))
                    continue
                for char in part:
                    if char == "\r": col = 0
                    elif char == "\n": row += 1
                    elif char.isprintable():
                        if 0 <= row < 34 and 0 <= col < 110: grid[row][col] = char
                        col += 1
            return any(title in "".join(line) for line in grid[:3])
        native_background.until(painted_header, "actual frontend tab title was not painted")
        assert len(native.requests) == 2 and not native.errors
        outputs = [item for item in native.requests[1]["input"] if item.get("type") == "function_call_output"]
        assert len(outputs) == 1 and outputs[0]["call_id"] == "title-call"
        assert json.loads(outputs[0]["output"]) == {"sessionID": "t50-background", "title": title}
    print(json.dumps({"case": "native-rename-real-PTY-title", "status": "PASS", "requests": 2,
        "title_events": 1, "real_frontend_tab": True, "owned_cleanup": "joined"}))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--tui-title-only", action="store_true")
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = digest(binary)
    if options.tui_title_only:
        tui_title(binary)
        after = digest(binary)
        assert before == after
        print(json.dumps({"sha256_before": before, "sha256_after": after, "cases": 1, "status": "PASS"}))
        return
    native_search.QUESTION_SUPPORTED = True
    native_search.MODEL_SESSION_SUPPORTED = True
    native_background.QUESTION_SUPPORTED = True
    native_question.run(binary, "directed-headless-ask-auto", auto=True, permission="ask")
    native_question.run(binary, "directed-custom-reopen-auto",
        keys=b"3discarded draft\x03\x033custom answer\r", expected=[["custom answer"]], auto=True)
    permitted = {"read": {"*": "allow", "src/denied.rs": "deny"},
        "grep": "allow", "glob": "allow", "subagent": "allow"}
    canonical = [
        ("grep", {"pattern": "^foo[0-9]+$", "path": "src", "include": "*.rs"},
            [native_search.hit("src/a.rs", 1, "foo12")]),
        ("glob", {"pattern": "*.rs", "path": "src", "offset": 1, "limit": 1}, ["src/git.rs"]),
    ]
    native_search.run(binary, "directed-search-root", canonical, permission=permitted)
    native_search.run(binary, "directed-search-explore", canonical, permission=permitted, child=True)
    for case in ("nested_lifecycle", "image"):
        print(json.dumps(native_read.run(binary, case), ensure_ascii=False))
    for case in ("markdown", "ask"):
        print(json.dumps(native_webfetch.run(binary, case), ensure_ascii=False))
    instructions = native_instructions.qualify_ask(binary)
    assert instructions["status"] == "PASS" and len(instructions["ask_scenarios"]) == 6
    print(json.dumps(instructions))
    native_background.review_revert(binary)
    after = digest(binary)
    assert before == after
    print(json.dumps({"binary": str(binary), "sha256_before": before, "sha256_after": after,
        "status": "PASS", "cases": 15, "question": 2, "search": 2, "read": 2,
        "webfetch": 2, "instructions_ask": 6, "RET01": 1,
        "owned_cleanup": "existing fixtures joined owned processes and HTTP threads, removed exact TempDirs"}))


if __name__ == "__main__":
    main()
