#!/usr/bin/env python3
"""TOOL15 direct ELF, synthetic loopback only; owns and joins all fixtures."""
import argparse
import hashlib
import http.server
import json
import os
import pty
import fcntl
import struct
import termios
from pathlib import Path
import select
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time

QUESTIONS = [{"question": "Choose the implementation", "header": "Choice", "options": [
    {"label": "Native", "description": "Use native code"},
    {"label": "Other", "description": "Another choice"}]}]


def own_tty():
    # /dev/tty must name the fixture slave, never the authoring terminal.
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def tty_command(command):
    # Attach in a fresh interpreter after setsid, not a threaded preexec hook.
    return [sys.executable, str(Path(__file__).resolve()), "--exec-pty", *command]


def stop_owned(process):
    if process is not None and process.poll() is None:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


def read_until(process, master, marker, buffer=b""):
    deadline = time.monotonic() + 8
    while marker not in buffer and time.monotonic() < deadline:
        assert process.poll() is None, ("premature PTY exit", process.returncode, buffer[-2000:])
        if select.select([master], [], [], 0.1)[0]:
            buffer = (buffer + os.read(master, 65536))[-262144:]
    assert marker in buffer, ("missing readiness", marker, buffer[-3000:])
    return buffer


def run(binary, case, *, auto=False, keys=None, questions=None, expected=None,
        dismiss=False, permission="allow", child=False, crash=False):
    with tempfile.TemporaryDirectory(prefix="t50-question-") as tmp:
        root = Path(tmp)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        requests, failures = [], []
        answered = threading.Event()
        prompts = QUESTIONS if questions is None else questions

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
                    if request.get("max_output_tokens") == 256 and not request.get("tools"):
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        self.wfile.write(b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
                        self.close_connection = True
                        return
                    requests.append(request)
                    if expected is not None and len(requests) == (3 if child else 2):
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            rows = db.execute("SELECT id,state,output FROM tool_operations WHERE name='question'").fetchall()
                            assert len(rows) == 1 and rows[0][1] == "completed", rows
                            output = json.loads(rows[0][2])
                            assert output["answers"] == expected, output
                            assert [q["header"] for q in output["questions"]] == [q["header"] for q in prompts]
                            wire = [i for i in request["input"] if i.get("type") == "function_call_output"]
                            assert len(wire) == 1 and wire[0]["call_id"] == "ask-1"
                            assert json.loads(wire[0]["output"]) == output
                        answered.set()
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    events = []
                    if len(requests) == 1 or (child and len(requests) == 2):
                        item = {"type": "function_call", "id": "fc-question", "call_id": "ask-1",
                                "name": "question", "arguments": json.dumps({"questions": prompts}),
                                "status": "completed"}
                        if child and len(requests) == 1:
                            item.update(name="subagent", arguments=json.dumps({"agent":child if isinstance(child, str) else "general", "description":"Child fixture", "prompt":"Ask fixture"}))
                        events = [{"type": "response.output_item.added", "item": item},
                                  {"type": "response.output_item.done", "item": item}]
                        if dismiss:
                            patch = {"type":"function_call", "id":"fc-patch", "call_id":"patch-queued", "name":"apply_patch", "arguments":json.dumps({"patch":"*** Begin Patch\n*** Add File: forbidden.txt\n+effect\n*** End Patch\n"}), "status":"completed"}
                            events.extend([{"type":"response.output_item.added", "item":patch}, {"type":"response.output_item.done", "item":patch}])
                    events.append({"type": "response.completed", "response": {"status": "completed", "output": [] if events else [{"type":"message", "id":"m-done", "role":"assistant", "content":[{"type":"output_text", "text":"QUESTION_DONE"}]}]}})
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                    self.close_connection = True
                except Exception as error:
                    failures.append(str(error))
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        peer = threading.Thread(target=server.serve_forever)
        peer.start()
        configuration = {"model": "fixture/m", "compaction": {"auto": False},
            "agent": {"title": {"disable": True}}, "permission": {"question": permission, "apply_patch":"allow", "subagent":"allow"},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}
        if child:
            agent = child if isinstance(child, str) else "general"
            configuration["agent"][agent] = {"mode":"subagent", "prompt":"Child synthetic", "permission":{"question":"allow"}}
        if permission is None:
            del configuration["permission"]["question"]
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": "/usr/bin:/bin", "TERM":"xterm-256color", "OC_TEST_ALLOW_LOOPBACK": "1"}
        command = [str(binary), "--data-dir", str(data), "run", "--json", "Ask fixture"]
        master = None
        reopened = None
        if keys is not None:
            command = [str(binary), "--data-dir", str(data)]
            master, slave = pty.openpty()
            tty_path = os.ttyname(slave)
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))
        if auto:
            command.append("--auto")
        if master is not None:
            command = tty_command(command)
        process = subprocess.Popen(command, cwd=project, env=environment,
                                   stdin=slave if master is not None else subprocess.DEVNULL,
                                   stdout=slave if master is not None else subprocess.PIPE,
                                   stderr=slave if master is not None else subprocess.PIPE,
                                   start_new_session=True)
        if master is not None:
            os.close(slave)
        try:
            if master is not None:
                read_until(process, master, b"Ask anything")
                os.write(master, b"Ask fixture\r")
                read_until(process, master, b"Type your own answer")
                assert len(requests) == (2 if child else 1), "real waiter must not continue without input"
                with sqlite3.connect(data / "oc.sqlite") as db:
                    assert db.execute("SELECT state FROM tool_operations WHERE name='question'").fetchone()[0] == "started"
                if crash:
                    stop_owned(process)
                else:
                    os.write(master, keys)
                if crash:
                    assert len(requests) == 1
                elif dismiss:
                    deadline = time.monotonic() + 8
                    while time.monotonic() < deadline:
                        if select.select([master], [], [], 0.1)[0]:
                            os.read(master, 65536)
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            if db.execute("SELECT count(*) FROM turns WHERE status='cancelled'").fetchone()[0] == 1:
                                break
                    else:
                        raise AssertionError("dismiss did not settle turn")
                    assert len(requests) == 1
                    assert not (project / "forbidden.txt").exists()
                else:
                    read_until(process, master, b"QUESTION_DONE")
                    assert answered.wait(1), failures
                    deadline = time.monotonic() + 4
                    while time.monotonic() < deadline:
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            if db.execute("SELECT count(*) FROM turns WHERE status='started'").fetchone()[0] == 0:
                                break
                        if select.select([master], [], [], 0.1)[0]:
                            os.read(master, 65536)
                    else:
                        raise AssertionError("provider completion did not durably settle")
                if not crash:
                    os.write(master, b"\x03")
                    process.wait(timeout=5)
                    assert process.returncode == 0
                if not dismiss:
                    with sqlite3.connect(data / "oc.sqlite") as db:
                        session = db.execute("SELECT session_id FROM tool_operations WHERE name='question'").fetchone()[0]
                    # Reopen the real TUI from journal; no synthetic form or provider RPC.
                    drain_deadline = time.monotonic() + 2
                    while time.monotonic() < drain_deadline and select.select([master], [], [], 0)[0]:
                        try:
                            if not os.read(master, 65536):
                                break
                        except OSError:
                            break
                    # The supervisor must not acquire this controlling tty and
                    # receive SIGHUP when closing the master after the reopen.
                    slave = os.open(tty_path, os.O_RDWR | os.O_NOCTTY)
                    reopened = subprocess.Popen(tty_command([str(binary), "--data-dir", str(data), "tui", "--session", session]), cwd=project, env=environment, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
                    os.close(slave)
                    screen = read_until(reopened, master, b"question" if crash else b"Questions answered")
                    assert len(requests) == (1 if crash else 4 if child else 2) and not failures, failures
                    if crash:
                        assert b"Type your own answer" not in screen
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            assert db.execute("SELECT state FROM tool_operations WHERE name='question'").fetchone()[0] == "unknown"
                            assert db.execute("SELECT status FROM turns").fetchone()[0] == "unknown"
                    os.write(master, b"\x03")
                    reopened.wait(timeout=5)
                    assert reopened.returncode == 0
                assert not failures, failures
                print(json.dumps({"case":case, "status":"PASS", "requests":len(requests), "typed_answers":expected, "dismissed":dismiss}))
                return
            stdout, stderr = process.communicate(timeout=15)
            if permission == "deny" or child or prompts == []:
                assert process.returncode == 0, (case, stderr.decode())
                if permission == "deny" or child:
                    assert all(t["name"] != "question" for t in requests[1 if child else 0]["tools"]), "Denied catalog must omit question"
                with sqlite3.connect(data / "oc.sqlite") as db:
                    rows = db.execute("SELECT state FROM tool_operations WHERE name='question'").fetchall()
                    assert len(rows) == 1 and rows[0][0] in ("denied", "failed"), rows
                assert not failures, failures
                print(json.dumps({"case":case, "status":"PASS", "requests":len(requests)}))
                return
            assert process.returncode == 1, (case, process.returncode, stderr.decode())
            assert "question_required" in stderr.decode(), (case, stderr.decode())
            assert len(requests) == 1
            definition = next(t for t in requests[0]["tools"] if t["name"] == "question")
            assert definition["parameters"]["properties"]["questions"]["minItems"] == 1
            with sqlite3.connect(data / "oc.sqlite") as db:
                rows = db.execute("SELECT name,state,output FROM tool_operations").fetchall()
                assert len(rows) == 1 and rows[0][:2] == ("question", "failed"), rows
                assert all(status != "completed" for (status,) in db.execute("SELECT status FROM turns"))
            assert not failures
            print(json.dumps({"case": case, "status": "PASS", "requests": 1, "operation_rows": 1}))
        finally:
            stop_owned(process)
            stop_owned(reopened)
            if master is not None:
                os.close(master)
            server.shutdown()
            server.server_close()
            peer.join(timeout=5)
            assert not peer.is_alive()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    print(json.dumps({"binary": str(binary), "sha256_before": before}))
    run(binary, "headless")
    run(binary, "headless_auto", auto=True)
    run(binary, "headless_ask_auto", auto=True, permission="ask")
    run(binary, "deny", permission="deny")
    run(binary, "general_ceiling", child=True)
    run(binary, "explore_ceiling", child="explore")
    run(binary, "invalid_empty_questions", questions=[])
    run(binary, "single", keys=b"2", expected=[["Other"]], permission=None)
    # Clear nonempty custom text, then close the empty non-textual editor without
    # dismissing the real request; reopen and submit only the final answer.
    run(binary, "single_custom_auto", keys=b"3discarded draft\x03\x033custom answer\r", expected=[["custom answer"]], auto=True)
    multiple = [dict(QUESTIONS[0], multiple=True)]
    run(binary, "multiple_submit", questions=multiple, keys=b"21\t\r", expected=[["Native", "Other"]])
    run(binary, "ordered_questions", questions=[QUESTIONS[0], dict(QUESTIONS[0], header="Second")], keys=b"23custom\r\r", expected=[["Other"], ["custom"]])
    run(binary, "child_origin_consumer", child="helper", keys=b"1", expected=[["Native"]])
    run(binary, "dismiss_queued_effect", keys=b"\x1b", dismiss=True)
    run(binary, "unanswered_restart_unknown", keys=b"", crash=True)
    after = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert before == after
    print(json.dumps({"sha256_after": after, "cases": 14}))


if __name__ == "__main__":
    if sys.argv[1:2] == ["--exec-pty"]:
        own_tty()
        os.execv(sys.argv[2], sys.argv[2:])
    else:
        main()
