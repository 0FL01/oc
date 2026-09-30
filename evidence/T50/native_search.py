#!/usr/bin/env python3
"""TOOL14: bounded held fake Responses calls against the supplied native ELF.

All HOME/config/storage/workspace inputs are fresh owned temporary fixtures.
Only small sanitized summaries survive joining provider/process owners.
"""
import argparse
import hashlib
import http.server
import json
import os
import signal
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading
import time


def run(binary, case, calls, *, permission=None, child=False, auto=False, budget=False, cancel=False):
    with tempfile.TemporaryDirectory(prefix="t50-search-") as temporary:
        root = Path(temporary)
        project, home = root / "project", root / "home"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        data = project / "private"
        for path, body in {
            "src/a.rs": "foo12\nf.o\nÉCOLE\nNEEDLE\n",
            "src/nested/b.txt": "foo34\nNEEDLE\n",
            "src/.dot.rs": "NEEDLE\n", "src/.hidden/h.rs": "NEEDLE\n",
            "src/git.rs": "NEEDLE\n", "src/ignore.rs": "NEEDLE\n",
            "src/rg.rs": "NEEDLE\n", "src/nested/negated.rs": "NEEDLE\n",
            "src/denied.rs": "NEEDLE DENIED_CONTENT\n",
            "src/.git/internal.rs": "NEEDLE GIT_CONTENT\n",
            "src/.gitignore": "git.rs\nnegated.rs\n",
            "src/.ignore": "ignore.rs\n!git.rs\n",
            "src/.rgignore": "rg.rs\n",
            "src/nested/.gitignore": "!negated.rs\n!rg.rs\n",
            "private/secret.rs": "NEEDLE DATA_CONTENT\n",
        }.items():
            target = project / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(body)
        outside = root / "outside"
        outside.mkdir()
        (outside / "secret.rs").write_text("NEEDLE OUTSIDE_CONTENT\n")
        (project / "escape").symlink_to(outside, target_is_directory=True)
        (project / "data-link").symlink_to(data, target_is_directory=True)
        os.mkfifo(project / "src/fifo")
        if budget:
            (project / "scan").mkdir()
            for index in range(17):
                (project / f"scan/{index:02}.txt").write_bytes(b"x" * 1048576)
        calls = [(name, {k: (str(project / "src/a.rs") if v == "$ABS_FILE" else
                        str(project / "src/denied.rs") if v == "$ABS_DENIED" else
                        str(outside) if v == "$OUTSIDE" else v) for k, v in args.items()}, expected)
                 for name, args, expected in calls]
        requests, failures = [], []
        held, release = threading.Event(), threading.Event()
        definition_digest = []

        class Peer(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get("content-length", "0"))
                    assert 0 < length <= 1048576
                    request = json.loads(self.rfile.read(length))
                    if request.get("max_output_tokens") == 256 and not request.get("tools"):
                        body = ('data: {"type":"response.completed","response":{"status":"completed",'
                                '"output":[{"type":"message","role":"assistant","content":'
                                '[{"type":"output_text","text":"Fixture title"}]}]}}\n\n').encode()
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.send_header("Content-Length", str(len(body)))
                        self.end_headers()
                        self.wfile.write(body)
                        return
                    requests.append(request)
                    number = len(requests)
                    if child and number == 1:
                        script = [("subagent", {"agent": "explore", "description": "Search fixture",
                                               "prompt": "T50_EXPLORE_SEARCH"}, None)]
                    elif number == (2 if child else 1):
                        script = calls
                    else:
                        script = []
                    events = []
                    for index, (name, args, _) in enumerate(script):
                        item = {"type": "function_call", "id": f"fc-{number}-{index}",
                                "call_id": f"call-{number}-{index}", "name": name,
                                "arguments": json.dumps(args), "status": "completed"}
                        events.extend([{"type": "response.output_item.added", "item": item},
                                       {"type": "response.output_item.done", "item": item}])
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    if not events:
                        events.append({"type": "response.output_text.delta", "delta": "done"})
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                    if number == 1:
                        held.set()
                        assert release.wait(8), "completion barrier not released"
                    self.wfile.write(('data: {"type":"response.completed","response":'
                        '{"status":"completed","output":[],"usage":'
                        '{"input_tokens":1,"output_tokens":1}}}\n\n').encode())
                    self.wfile.flush()
                    self.close_connection = True
                except Exception as error:
                    if not cancel or not isinstance(error, (BrokenPipeError, ConnectionResetError)):
                        failures.append(str(error))
                    held.set()

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        peer = threading.Thread(target=server.serve_forever)
        peer.start()
        permissions = permission or {"read": "allow", "glob": "allow", "grep": "allow", "subagent": "allow"}
        headless_ask = not auto and any(permissions.get(name) == "ask" for name in ["grep", "glob"])
        configuration = {"model": "fixture/m", "compaction": {"auto": False},
            "agent": {"title": {"disable": True}}, "permission": permissions,
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}
        if child:
            configuration["agent"]["explore"] = {"mode": "subagent", "prompt": "T50_EXPLORE_SEARCH",
                "tools": {"shell": False, "apply_patch": False, "subagent": False},
                "permission": {"read": "allow", "grep": "allow", "glob": "allow"}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        command = [str(binary), "--data-dir", str(data), "run", "--json", "Search fixture"]
        if auto:
            command.append("--auto")
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}
        process = subprocess.Popen(command, cwd=project, env=environment,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            assert held.wait(8), "no held native request"
            # Completed calls do not execute until the response is truly closed.
            with sqlite3.connect(data / "oc.sqlite") as db:
                assert db.execute("SELECT count(*) FROM tool_operations").fetchone()[0] == 0
            if cancel:
                # Match the established foreground fixture's signal-consumer
                # readiness window; the provider completion remains held.
                time.sleep(0.15)
                process.send_signal(signal.SIGINT)
            else:
                release.set()
            stdout, stderr = process.communicate(timeout=35)
            assert process.returncode == (130 if cancel else 1 if headless_ask else 0), (case, process.returncode, stderr.decode())
            if headless_ask:
                assert "approval_required" in stderr.decode()
        finally:
            release.set()
            if process.poll() is None:
                process.kill()
                process.communicate(timeout=5)
            server.shutdown()
            server.server_close()
            peer.join(timeout=5)
        assert not failures, (case, failures)
        assert not peer.is_alive() and process.poll() is not None
        if cancel:
            with sqlite3.connect(data / "oc.sqlite") as db:
                assert db.execute("SELECT count(*) FROM tool_operations").fetchone()[0] == 0
                assert all(status != "completed" for (status,) in db.execute("SELECT status FROM turns"))
            print(json.dumps({"case":case, "status":"PASS", "requests":len(requests),
                              "operation_rows":0, "exit":process.returncode, "owners_joined":True}))
            return
        active = requests[1 if child else 0]
        names = {tool["name"] for tool in active["tools"]}
        assert not names & {"write", "edit", "execute", "websearch", "question", "bash",
                            "opencode_models", "opencode_session_rename", "opencode_session_move"}
        if child:
            assert not names & {"shell", "apply_patch", "subagent"}
            assert "T50_EXPLORE_SEARCH" in json.dumps(active["input"])
        for name in ["grep", "glob"]:
            if permissions.get(name) == "deny":
                assert name not in names
                continue
            tool = next(tool for tool in active["tools"] if tool["name"] == name)
            expected = {"pattern", "path", "offset", "limit"} | (
                {"include", "literal", "caseSensitive"} if name == "grep" else {"hidden"})
            assert set(tool["parameters"]["properties"]) == expected
            assert tool["parameters"]["required"] == ["pattern"]
            assert tool["parameters"]["additionalProperties"] is False
            if name == "grep":
                assert tool["parameters"]["properties"]["literal"]["default"] is False
                assert tool["parameters"]["properties"]["caseSensitive"]["default"] is True
            else:
                assert tool["parameters"]["properties"]["hidden"]["default"] is False
            definition_digest.append(hashlib.sha256(json.dumps(tool, sort_keys=True).encode()).hexdigest())
        if headless_ask:
            with sqlite3.connect(data / "oc.sqlite") as db:
                assert db.execute("SELECT count(*) FROM tool_operations").fetchone()[0] == 0
                assert db.execute("SELECT count(*) FROM permission_grants").fetchone()[0] == 0
            assert len(requests) == 1
            print(json.dumps({"case":case, "status":"PASS", "requests":1, "schema_sha256":definition_digest,
                              "operation_rows":0, "exit":1, "error":"approval_required", "owners_joined":True}))
            return
        continuation = None if headless_ask else requests[2 if child else 1]
        outputs = {} if continuation is None else {item["call_id"]: item["output"] for item in continuation["input"]
                   if item.get("type") == "function_call_output"}
        facts = []
        with sqlite3.connect(data / "oc.sqlite") as db:
            rows = db.execute("SELECT name,state,output FROM tool_operations ORDER BY rowid").fetchall()
            assert len(rows) == len(calls) + int(child), (case, rows)
            assert db.execute("SELECT count(*) FROM permission_grants").fetchone()[0] == 0
            journals = [json.loads(value) for (value,) in db.execute("SELECT result FROM turns WHERE result IS NOT NULL")]
            durable = {item["call_id"]: item["output"] for journal in journals for item in journal.get("input", [])
                       if item.get("type") == "function_call_output"}
            for index, (name, args, expected) in enumerate(calls):
                row = rows[index + int(child)]
                call_id = f"call-{2 if child else 1}-{index}"
                output = durable[call_id]
                if not headless_ask:
                    assert outputs[call_id] == output
                assert row[0] == name and row[2] == output, (case, row, output)
                assert all(trap not in output for trap in ["DATA_CONTENT", "GIT_CONTENT", "OUTSIDE_CONTENT", "DENIED_CONTENT"])
                if isinstance(expected, str):
                    assert expected in output, (case, args, output)
                    assert "error" in output
                    facts.append({"tool": name, "state": row[1], "error": expected})
                else:
                    parsed = json.loads(output)
                    key = "matches" if name == "grep" else "items"
                    assert parsed[key] == expected, (case, args, parsed, expected)
                    assert parsed["pagination"]["returned"] == len(expected)
                    truncated = args.get("limit") == 1 and args.get("path") == "src" and name == "glob"
                    assert parsed["pagination"]["truncated"] == truncated, (case, parsed)
                    assert parsed["pagination"]["next_offset"] == (args.get("offset", 0) + len(expected) if truncated else None)
                    facts.append({"tool": name, "state": row[1], key: parsed[key], "pagination": parsed["pagination"]})
        print(json.dumps({"case": case, "status": "PASS", "requests": len(requests),
                          "closed_response_barrier": True, "schema_sha256": definition_digest, "facts": facts}, ensure_ascii=False))


def hit(path, line, text):
    return {"path": path, "line": line, "text": text}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    print(json.dumps({"binary": str(binary), "sha256_before": before}))
    permitted = {"read": {"*": "allow", "src/denied.rs": "deny"}, "grep": "allow", "glob": "allow", "subagent": "allow"}
    canonical = [
        ("grep", {"pattern": "^foo[0-9]+$", "path": "src", "include": "*.rs"}, [hit("src/a.rs", 1, "foo12")]),
        ("grep", {"pattern": "f.o", "literal": True, "path": "src/a.rs"}, [hit("src/a.rs", 2, "f.o")]),
        ("grep", {"pattern": "école", "caseSensitive": False, "path": "$ABS_FILE"}, [hit("src/a.rs", 3, "ÉCOLE")]),
        ("grep", {"pattern": "école", "path": "src/a.rs"}, []),
        ("glob", {"pattern": "*.{rs,txt}", "path": "src/nested"}, ["src/nested/b.txt", "src/nested/negated.rs"]),
        ("glob", {"pattern": "*.rs", "path": "src", "limit": 1}, ["src/a.rs"]),
        ("glob", {"pattern": "*.rs", "path": "src", "offset": 1, "limit": 1}, ["src/git.rs"]),
    ]
    run(binary, "canonical-root", canonical, permission=permitted)
    run(binary, "canonical-explore-child", canonical[:5], permission=permitted, child=True)
    run(binary, "deny-explore-child", [
        (name, {"pattern":"NEEDLE", "path":"missing"}, "denied") for name in ["grep", "glob"]
    ], permission={"read":"allow", "grep":"deny", "glob":"deny", "subagent":"allow"}, child=True)
    run(binary, "ask-once-explore-child", canonical[:5],
        permission={"read":"allow", "grep":"ask", "glob":"ask", "subagent":"allow"}, child=True, auto=True)
    visible = ["src/a.rs", "src/git.rs", "src/ignore.rs", "src/nested/negated.rs", "src/rg.rs"]
    hidden = ["src/.dot.rs", "src/.hidden/h.rs"] + visible
    run(binary, "ignore-hidden-data", [
        ("glob", {"pattern": "**/*.rs", "path": "src"}, visible),
        ("glob", {"pattern": "**/*.rs", "path": "src", "hidden": True}, hidden),
        ("glob", {"pattern": "private/**/*.rs", "hidden": True}, []),
        ("grep", {"pattern": "NEEDLE", "path": "src"}, [hit("src/.dot.rs", 1, "NEEDLE"), hit("src/.hidden/h.rs", 1, "NEEDLE"),
            hit("src/a.rs", 4, "NEEDLE"), hit("src/git.rs", 1, "NEEDLE"), hit("src/nested/b.txt", 2, "NEEDLE"), hit("src/nested/negated.rs", 1, "NEEDLE")]),
        ("grep", {"pattern": "NEEDLE", "path": "src", "include": "*.rs"}, [hit(path, 4 if path == "src/a.rs" else 1, "NEEDLE") for path in hidden]),
    ], permission=permitted)
    run(binary, "invalid-before-scan", [
        ("grep", {"pattern": "[", "path": "missing"}, "invalid/unsupported regex"),
        ("grep", {"pattern": "(?=private)", "path": "missing"}, "invalid/unsupported regex"),
        ("glob", {"pattern": "[", "path": "missing"}, "invalid glob"),
        ("grep", {"pattern": "x", "literal": "true"}, "literal must be a boolean"),
        ("grep", {"pattern": "x", "caseSensitive": "false"}, "caseSensitive must be a boolean"),
        ("glob", {"pattern": "*", "hidden": "true"}, "hidden must be a boolean"),
        ("glob", {"pattern": "*", "limit": 0}, "limit must be between"),
    ], permission=permitted)
    run(binary, "boundaries", [
        ("grep", {"pattern": "NEEDLE", "path": "private"}, "own data root"),
        ("glob", {"pattern": "*", "path": "$OUTSIDE"}, "outside trusted root"),
        ("grep", {"pattern": "NEEDLE", "path": "escape"}, "symlink escape"),
        ("grep", {"pattern": "NEEDLE", "path": "src/denied.rs"}, "denied grep: read scope"),
        ("grep", {"pattern": "NEEDLE", "path": "$ABS_DENIED"}, "denied grep: read scope"),
    ], permission=permitted)
    for name in ["grep", "glob"]:
        for permission, auto, expected in [("deny", False, "denied"), ("ask", False, "approval required")]:
            run(binary, f"{name}-{permission}", [(name, {"pattern": "NEEDLE", "path": "missing"}, expected)],
                permission={"read": "allow", "grep": permission, "glob": permission}, auto=auto)
    run(binary, "ask-once-regex", canonical[:1], permission={"read": "allow", "grep": "ask", "glob": "ask"}, auto=True)
    run(binary, "scan-budget", [("grep", {"pattern": "absent", "path": "scan"}, "search budget exhausted")], permission=permitted, budget=True)
    run(binary, "cancel-held-search", canonical[:1], permission=permitted, cancel=True)
    after = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert before == after
    print(json.dumps({"sha256_after": after, "same_elf": True, "status": "PASS"}))


if __name__ == "__main__":
    main()
