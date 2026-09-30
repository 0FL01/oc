#!/usr/bin/env python3
"""TOOL16 direct normal-ELF guards; synthetic peer, joined owned processes."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import signal
import sqlite3
import subprocess
import tempfile
import threading


def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-read-") as temporary:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        nested = project / "nested"
        nested.mkdir()
        (project / "AGENTS.md").write_text("R5_ROOT_BASELINE\n")
        (nested / "AGENTS.md").write_text("R5_NESTED_ADMITTED_INSTRUCTION\n")
        (nested / "small.txt").write_text("first\nsecond\n")
        (project / "large.txt").write_text("".join(f"fixture_{n}\n" for n in range(1, 2006)))
        arguments = {"path": "large.txt"} if case == "text_default" else {
            "path": "nested/small.txt", "limit": 2}
        requests, failures = [], []

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get("content-length", "0"))
                    assert 0 < length <= 1048576
                    request = json.loads(self.rfile.read(length))
                    if request.get("max_output_tokens") == 256 and not request.get("tools"):
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        self.wfile.write(b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
                        return
                    assert request.get("tools"), "unexpected auxiliary request"
                    requests.append(request)
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    if len(requests) == 1:
                        item = {"type": "function_call", "id": "read-item", "call_id": "read-call",
                                "name": "read", "arguments": json.dumps(arguments), "status": "completed"}
                        events = [{"type": "response.output_item.added", "item": item},
                                  {"type": "response.output_item.done", "item": item},
                                  {"type": "response.completed", "response": {
                                      "status": "completed", "output": [item]}}]
                    else:
                        assert len(requests) == 2, "read guard must have exactly two model calls"
                        with sqlite3.connect(data / "oc.sqlite") as database:
                            rows = database.execute(
                                "SELECT state,output FROM tool_operations WHERE name='read'").fetchall()
                        assert len(rows) == 1 and rows[0][0] == "completed", "read not durably completed"
                        outputs = [entry for entry in request["input"]
                                   if entry.get("type") == "function_call_output"]
                        assert len(outputs) == 1 and outputs[0]["call_id"] == "read-call"
                        assert outputs[0]["output"] == rows[0][1], "wire/journal read result mismatch"
                        events = [{"type": "response.completed", "response": {
                            "status": "completed", "output": [{"type": "message", "id": "done",
                                "role": "assistant", "content": [{"type": "output_text", "text": "READ_DONE"}]}]}}]
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                except Exception as error:
                    failures.append(str(error))
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        worker = threading.Thread(target=server.serve_forever)
        worker.start()
        configuration = {"model": "fixture/read-model", "compaction": {"auto": False},
            "agent": {"title": {"disable": True}}, "permission": {"read": "allow"},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"read-model": {"limit": {"context": 65536, "output": 2048}}}}}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}
        process = None
        try:
            process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", "Read fixture"],
                cwd=project, env=environment, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            stdout, stderr = process.communicate(timeout=20)
            assert process.returncode == 0, ("application exit", process.returncode, stderr[-1024:].decode())
            assert len(stdout) < 1048576 and len(stderr) < 1048576
            assert not failures, failures
            assert len(requests) == 2, ("actual model requests", len(requests))
            output = next(entry["output"] for entry in requests[1]["input"]
                          if entry.get("type") == "function_call_output")
            if case == "text_default":
                assert "2000: fixture_2000" in output, (
                    "default 2000 numbered lines missing", "returned_fixture_lines",
                    sum("fixture_" in line for line in output.splitlines()))
                assert "2001: fixture_2001" not in output
                assert "2001" in output, "next 1-based page cursor missing"
                definition = next(tool for tool in requests[0]["tools"] if tool["name"] == "read")
                assert "2000" in definition["description"], "read catalog default is not truthful"
            else:
                assert "first" in output and "second" in output
                instructions = [entry for entry in requests[1]["input"] if entry.get("role") in ("system", "developer")]
                rendered = json.dumps(instructions)
                assert "R5_ROOT_BASELINE" in rendered, "baseline control absent"
                assert rendered.count("R5_NESTED_ADMITTED_INSTRUCTION") == 1, (
                    "successful read did not invoke admitted nested lifecycle", "nested_instruction_count",
                    rendered.count("R5_NESTED_ADMITTED_INSTRUCTION"))
            return {"case": case, "status": "PASS", "requests": len(requests)}
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            server.shutdown()
            worker.join(timeout=5)
            server.server_close()  # joins non-daemon HTTP workers before TempDir removal
            assert not worker.is_alive(), "owned peer failed to join"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--case", choices=("text_default", "nested_lifecycle"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    failures = 0
    for case in [args.case] if args.case else ["text_default", "nested_lifecycle"]:
        try:
            result = run(binary, case)
        except Exception as error:
            failures += 1
            result = {"case": case, "status": "FAIL", "error": str(error)}
        print(json.dumps({**result, "elf_sha256": digest, "owned_cleanup": "joined"}))
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
