#!/usr/bin/env python3
"""Offline actual normal-ELF external read/search amendment; owned joined fixtures."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading

PATTERNS = ["~/.cargo/registry/src/*", "~/.local/lib/python*/site-packages/*",
            "~/.cache/uv/archive-v0/*/lib/python*/site-packages/*", "~/go/pkg/mod/*"]
TREES = [".cargo/registry/src/vendor/pkg", ".local/lib/python3.13/site-packages/pkg",
         ".cache/uv/archive-v0/pkg/lib/python3.13/site-packages/pkg", "go/pkg/mod/pkg@v1"]


def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-external-") as temporary:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        scripts = []
        for index, tree in enumerate(TREES):
            directory = home / tree
            directory.mkdir(parents=True)
            (directory / "sentinel.txt").write_text(f"EXTERNAL_SENTINEL_{index}\nsecond\n")
            (directory / "AGENTS.md").write_text("EXTERNAL_INSTRUCTION_TRAP\n")
            (directory / "denied.txt").write_text("PROTECTED_READ_TRAP\n")
            path = "~/" + tree
            scripts.extend([("read", {"path": path + "/sentinel.txt"}, f"EXTERNAL_SENTINEL_{index}"),
                            ("read", {"path": path, "limit": 4}, "sentinel.txt"),
                            ("glob", {"path": path, "pattern": "*.txt"}, "sentinel.txt"),
                            ("grep", {"path": path, "pattern": "EXTERNAL_SENTINEL"}, f"EXTERNAL_SENTINEL_{index}")])
        scripts += [("grep", {"path": "~/" + TREES[0] + "/sentinel.txt", "pattern": "EXTERNAL_SENTINEL_0"}, "EXTERNAL_SENTINEL_0"),
                    ("read", {"path": "../home/" + TREES[0] + "/../pkg/sentinel.txt", "offset": 2, "limit": 1}, "second")]
        effects = {"boundary-ask": "ask", "boundary-deny": "deny"}
        rules = [{"action": "external_directory", "resource": pattern, "effect": effects.get(case, "allow")}
                 for pattern in PATTERNS]
        rules += [{"action": name, "resource": "*", "effect": "ask" if case == "tool-ask" else "deny" if case == "tool-deny" else "allow"}
                  for name in ["read", "glob", "grep"]]
        rules.append({"action": "read", "resource": str(home / "**/denied.txt"), "effect": "deny"})
        # Permission wildcard '*' spans separators; no globstar interpretation needed.
        rules[-1]["resource"] = str(home / "*/denied.txt")
        if case == "sibling":
            sibling = home / "ungranted"
            sibling.mkdir()
            (sibling / "sentinel.txt").write_text("PROTECTED_SIBLING_TRAP")
            scripts = [("read", {"path": str(sibling / "sentinel.txt")}, "approval_required")]
        elif case == "symlink":
            link = home / ".cargo/registry/src/link"
            link.symlink_to(home / TREES[0], target_is_directory=True)
            scripts = [("read", {"path": str(link / "sentinel.txt")}, "symlink")]
        elif case == "data-root":
            data = home / TREES[0] / "private"
            data.mkdir()
            (data / "secret.txt").write_text("PROTECTED_DATA_TRAP")
            scripts = [("read", {"path": str(data / "secret.txt")}, "own data root"),
                       ("glob", {"path": str(data), "pattern": "*"}, "own data root"),
                       ("grep", {"path": str(data), "pattern": "PROTECTED"}, "own data root")]
        elif case == "budget":
            large = home / TREES[0] / "oversized.txt"
            large.write_bytes(b"x" * (1024 * 1024 + 1))
            scripts = [("read", {"path": str(large)}, "budget exhausted"),
                       ("grep", {"path": str(large), "pattern": "x"}, "budget exhausted")]
        elif case in effects or case in ["tool-ask", "tool-deny"]:
            scripts = scripts[:4]
        auto = case in ["boundary-ask", "tool-ask"]
        no_consumer = case in ["sibling", "boundary-no-consumer", "tool-no-consumer"]
        if case == "boundary-no-consumer":
            for rule in rules[:4]:
                rule["effect"] = "ask"
            scripts = [scripts[0]]
        if case == "tool-no-consumer":
            rules[4]["effect"] = "ask"
            scripts = [scripts[0]]
        requests, failures, auxiliary = [], [], []
        phase = [0]

        class Peer(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    self.connection.settimeout(5)
                    request = json.loads(self.rfile.read(int(self.headers["content-length"])))
                    if not request.get("tools"):
                        assert request.get("max_output_tokens") == 256, "unexpected auxiliary request"
                        auxiliary.append(1)
                        body = b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n'
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.send_header("Content-Length", str(len(body)))
                        self.end_headers()
                        self.wfile.write(body)
                        return
                    requests.append(request)
                    first = phase[0] % 2 == 0
                    phase[0] += 1
                    output = [{"type": "function_call", "id": f"item-{i}", "call_id": f"call-{i}",
                               "name": name, "arguments": json.dumps(args), "status": "completed"}
                              for i, (name, args, _) in enumerate(scripts)] if first else [
                                  {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]}]
                    body = ("data: " + json.dumps({"type": "response.completed", "response": {"status": "completed", "output": output}}) + "\n\n").encode()
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except Exception as error:
                    failures.append(str(error))

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        peer = threading.Thread(target=server.serve_forever)
        peer.start()
        configuration = {"model": "fixture/m", "compaction": {"auto": False}, "agent": {"title": {"disable": True}},
                         "permissions": rules, "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                             "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                             "models": {"m": {"limit": {"context": 131072, "output": 2048}}}}}}
        if case == "legacy":
            configuration.pop("permissions")
            configuration["permission"] = {"external_directory": dict.fromkeys(PATTERNS, "allow"),
                                            "read": {"*": "allow", str(home / "*/denied.txt"): "deny"},
                                            "glob": "allow", "grep": "allow"}
        (config / "opencode.jsonc").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"), "PATH": "/usr/bin:/bin",
                       "OC_TEST_ALLOW_LOOPBACK": "1"}
        receipts = []
        try:
            for iteration in range(2 if case in ["allow", "legacy"] else 1):
                command = [str(binary), "--data-dir", str(data), "run", "--json", "--session", "external-root", "External fixture"]
                if auto:
                    command.append("--auto")
                process = subprocess.Popen(command, cwd=project, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    stdout, stderr = process.communicate(timeout=35)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.communicate(timeout=5)
                assert process.returncode == (1 if no_consumer else 0), (case, process.returncode, stderr.decode())
                events = [json.loads(line) for line in stdout.splitlines()]
                approvals = sum("approval" in json.dumps(event).lower() for event in events)
                with sqlite3.connect(data / "oc.sqlite") as db:
                    assert db.execute("SELECT count(*) FROM permission_grants").fetchone()[0] == 0
                    assert db.execute("SELECT id FROM sessions").fetchall() == [("external-root",)]
                    rows = db.execute("SELECT name,state,output FROM tool_operations ORDER BY rowid").fetchall()
                if no_consumer:
                    assert not rows and len(requests) == 1 and "approval_required" in stderr.decode(), (case, rows, stderr)
                else:
                    outputs = {item["call_id"]: item["output"] for item in requests[-1]["input"] if item.get("type") == "function_call_output"}
                    for i, (name, _, sentinel) in enumerate(scripts):
                        output = outputs[f"call-{i}"]
                        assert rows[iteration * len(scripts) + i] == (name, "failed" if case in ["boundary-deny", "tool-deny", "symlink", "data-root", "budget"] else "completed", output), (case, i, rows[iteration * len(scripts) + i], output)
                        assert ("denied" if case in ["boundary-deny", "tool-deny"] else sentinel) in output, (case, name, output)
                        assert not any(trap in output for trap in ["PROTECTED_READ_TRAP", "PROTECTED_DATA_TRAP", "PROTECTED_SIBLING_TRAP"])
                        if case in ["allow", "legacy"]:
                            assert "denied.txt" not in output
                    assert "EXTERNAL_INSTRUCTION_TRAP" not in json.dumps(requests[-1]["input"])
                if case in ["allow", "legacy"]:
                    assert approvals == 0
                receipts.append({"restart": iteration, "exit": process.returncode, "calls": len(scripts) if not no_consumer else 0,
                                 "approval_events": 0 if not auto else None, "once_consumer": auto,
                                 "saved_grants": 0, "same_session": "external-root", "process_reaped": True,
                                 "successful_read_calls": sum(name == "read" for name, _, _ in scripts) if case in ["allow", "legacy", "boundary-ask", "tool-ask"] else 0})
            for index, tree in enumerate(TREES):
                assert (home / tree / "sentinel.txt").read_text() == f"EXTERNAL_SENTINEL_{index}\nsecond\n"
        finally:
            server.shutdown()
            server.server_close()
            peer.join(timeout=5)
        assert not failures and not peer.is_alive(), failures
        print(json.dumps({"case": case, "status": "PASS", "requests": len(requests), "auxiliary_requests": len(auxiliary), "receipts": receipts, "peer_joined": True, "fixture_removed_on_return": True}))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--case", default="all")
    args = parser.parse_args()
    binary = args.binary.resolve()
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    print(json.dumps({"binary": str(binary), "sha256_before": before}), flush=True)
    cases = ["allow", "legacy", "boundary-ask", "tool-ask", "boundary-no-consumer", "tool-no-consumer",
             "boundary-deny", "tool-deny", "sibling", "symlink", "data-root", "budget"] if args.case == "all" else [args.case]
    for case in cases:
        run(binary, case)
    after = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert before == after
    print(json.dumps({"sha256_after": after, "same_elf": True, "status": "PASS"}))


if __name__ == "__main__":
    main()
