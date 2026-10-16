#!/usr/bin/env python3
"""Actual oversized answered question: one cold copy, bounded wire, real TUI restart."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import sqlite3
import struct
import subprocess
import tempfile
import termios
import fcntl
import threading
import time
from native_question import tty_command, read_until, stop_owned


def run(binary):
    with tempfile.TemporaryDirectory(prefix="t50-output-question-") as temporary:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        configuration = home / "config/opencode"
        configuration.mkdir(parents=True)
        project.mkdir()
        prompts = [{"question": "Choose the actual implementation", "header": "Decision",
                    "options": [{"label": f"choice-{i}", "description": "description " * 330}
                                for i in range(14)]}]
        expected = [["choice-0"]]
        requests, auxiliary_requests, failures, facts = [], [], [], {}
        answered = threading.Event()

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                try:
                    length = int(self.headers.get("content-length", "0"))
                    assert 0 < length <= 1048576
                    request = json.loads(self.rfile.read(length))
                    auxiliary = request.get("max_output_tokens") == 256 and not request.get("tools")
                    if auxiliary:
                        auxiliary_requests.append(1)
                        assert len(auxiliary_requests) <= 2
                    if not auxiliary:
                        requests.append(request)
                    if not auxiliary and len(requests) == 2:
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            operation, state, output = db.execute("SELECT id,state,output FROM tool_operations WHERE name='question'").fetchone()
                            assert state == "completed" and len(output.encode()) < 51200
                            hot = json.loads(output)
                            assert hot["status"] == "answered" and hot["answer_count"] == 1
                            descriptor, extent = db.execute("SELECT descriptor,extent FROM tool_output_resources WHERE operation_id=?", (operation,)).fetchone()
                            resource = json.loads(descriptor)
                            assert resource["state"] == "Complete" and extent > 51200
                            assert db.execute("SELECT count(*) FROM tool_output_resources").fetchone()[0] == 1
                            full = Path(resource["path"]).read_bytes()
                            assert len(full) == extent
                            native = json.loads(full)
                            assert native["answers"] == expected
                            assert [q["options"] for q in native["questions"]] == [q["options"] for q in prompts]
                            event = db.execute("SELECT payload FROM events WHERE kind='tool_output_question'").fetchone()[0]
                            assert len(event.encode()) < 1024 and "result" not in json.loads(event)
                            assert json.loads(event)["question_resource"]["source_operation"] == operation
                            wire = [item for item in request["input"] if item.get("type") == "function_call_output"]
                            assert len(wire) == 1 and wire[0]["call_id"] == "oversized-question"
                            assert wire[0]["output"] == output
                            facts.update(output_bytes=len(output.encode()), resource_bytes=extent,
                                         event_bytes=len(event.encode()), answered=expected, native_rows=1)
                        answered.set()
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    events = []
                    if not auxiliary and len(requests) == 1:
                        item = {"type": "function_call", "id": "item-question", "call_id": "oversized-question",
                                "name": "question", "arguments": json.dumps({"questions": prompts}), "status": "completed"}
                        events = [{"type": "response.output_item.added", "item": item},
                                  {"type": "response.output_item.done", "item": item}]
                    events.append({"type": "response.completed", "response": {"status": "completed", "output": [] if events or auxiliary else [
                        {"type": "message", "id": "done", "role": "assistant", "content": [{"type": "output_text", "text": "OVERSIZED_QUESTION_DONE"}]}]}})
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                    self.close_connection = True
                except Exception as error:
                    failures.append(str(error))
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        thread.start()
        config = {"model": "fixture/m", "compaction": {"auto": False}, "agent": {"title": {"disable": True}},
                  "permission": {"question": "allow"}, "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                      "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                      "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}
        (configuration / "opencode.json").write_text(json.dumps(config))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"), "PATH": "/usr/bin:/bin",
                       "TERM": "xterm-256color", "OC_TEST_ALLOW_LOOPBACK": "1"}
        master, slave = pty.openpty()
        tty_path = os.ttyname(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
        process = reopened = None
        try:
            process = subprocess.Popen(tty_command([str(binary), "--data-dir", str(data)]), cwd=project,
                                       env=environment, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
            os.close(slave)
            slave = None
            read_until(process, master, b"Ask anything")
            os.write(master, b"Ask actual oversized question\r")
            # Large real option descriptions fill the existing viewport; wait on
            # the visible prompt rather than an off-screen custom-answer row.
            read_until(process, master, b"Choose the actual implementation")
            with sqlite3.connect(data / "oc.sqlite") as db:
                assert db.execute("SELECT state FROM tool_operations WHERE name='question'").fetchone()[0] == "started"
            os.write(master, b"1")
            read_until(process, master, b"OVERSIZED_QUESTION_DONE")
            assert answered.wait(1) and not failures, failures
            deadline = time.monotonic() + 4
            while time.monotonic() < deadline:
                with sqlite3.connect(data / "oc.sqlite") as db:
                    if db.execute("SELECT count(*) FROM turns WHERE status='started'").fetchone()[0] == 0:
                        session = db.execute("SELECT session_id FROM tool_operations WHERE name='question'").fetchone()[0]
                        break
                if select.select([master], [], [], 0.1)[0]:
                    os.read(master, 65536)
            else:
                raise AssertionError("question turn did not durably settle")
            os.write(master, b"\x03")
            process.wait(timeout=5)
            assert process.returncode == 0
            while select.select([master], [], [], 0)[0]:
                try:
                    if not os.read(master, 65536):
                        break
                except OSError:
                    break
            slave = os.open(tty_path, os.O_RDWR | os.O_NOCTTY)
            reopened = subprocess.Popen(tty_command([str(binary), "--data-dir", str(data), "tui", "--session", session]),
                                        cwd=project, env=environment, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
            os.close(slave)
            slave = None
            read_until(reopened, master, b"Questions answered")
            assert len(requests) == 2 and not failures, failures
            os.write(master, b"\x03")
            reopened.wait(timeout=5)
            assert reopened.returncode == 0
            print(json.dumps({"case": "actual_oversized_answered_restart", "status": "PASS",
                              "provider_requests": len(requests) + len(auxiliary_requests),
                              "main_requests": len(requests), "auxiliary_requests": len(auxiliary_requests), **facts}))
        except Exception:
            diagnostic = {"peer_failures": failures, "requests": len(requests)}
            if (data / "oc.sqlite").exists():
                with sqlite3.connect(data / "oc.sqlite") as db:
                    diagnostic["operations"] = db.execute("SELECT name,state,length(output),substr(output,1,256) FROM tool_operations").fetchall()
            print(json.dumps(diagnostic))
            raise
        finally:
            stop_owned(process)
            stop_owned(reopened)
            if slave is not None:
                os.close(slave)
            os.close(master)
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
            assert not thread.is_alive()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    binary = parser.parse_args().binary.resolve()
    before = hashlib.file_digest(binary.open("rb"), "sha256").hexdigest()
    print(json.dumps({"binary": str(binary), "sha256_before": before}))
    run(binary)
    after = hashlib.file_digest(binary.open("rb"), "sha256").hexdigest()
    assert before == after
    print(json.dumps({"sha256_after": after}))
