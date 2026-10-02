#!/usr/bin/env python3
"""Bounded offline native background fixture; all paths/credentials are synthetic."""
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
import contextlib
import fcntl
import pty
import select
import signal
import struct
import termios
import time

QUESTION_SUPPORTED = False
SHELL_CONTROLS_SUPPORTED = False
import re
import unicodedata


def launch(binary):
    with tempfile.TemporaryDirectory(prefix="t50-background-") as tmp:
        root = Path(tmp)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        requests = []

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                size = int(self.headers.get("content-length", "0"))
                assert 0 < size <= 1048576
                request = json.loads(self.rfile.read(size))
                auxiliary = request.get("max_output_tokens") == 256 and not request.get("tools")
                if not auxiliary:
                    requests.append(request)
                if not auxiliary and len(requests) == 1:
                    item = {"type": "function_call", "id": "fc", "call_id": "background-call",
                            "name": "shell", "status": "completed", "arguments": json.dumps({
                                "command": "printf '%s' $$ > leader; printf launched; sleep 5", "background": True})}
                    events = [{"type": "response.output_item.added", "item": item},
                              {"type": "response.output_item.done", "item": item}]
                else:
                    if not auxiliary:
                        until(lambda: project.joinpath("leader").exists(), "headless launch did not reach execution barrier")
                    events = [{"type": "response.output_text.delta", "delta": "done"}]
                events.append({"type": "response.completed", "response": {"status": "completed"}})
                body = "".join("data: " + json.dumps(e) + "\n\n" for e in events).encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        config.joinpath("opencode.json").write_text(json.dumps({
            "model": "fixture/m", "compaction": {"auto": False},
            "permission": {"shell": "allow"}, "agent": {"title": {"disable": True}},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}))
        env = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
               "PATH": "/usr/bin:/bin", "SHELL": "/bin/bash", "OC_TEST_ALLOW_LOOPBACK": "1",
               "OPENAI_API_KEY": "synthetic-trap", "T50_SECRET": "synthetic-trap"}
        try:
            result = subprocess.run([str(binary), "--data-dir", str(data), "run", "--json", "launch"],
                                    cwd=project, env=env, capture_output=True, timeout=15)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            assert not thread.is_alive(), "owned fake HTTP worker did not join"
        assert requests, result.stderr.decode()
        assert result.returncode == 0, "headless owned shutdown failed"
        shell = next(t for t in requests[0]["tools"] if t["name"] == "shell")
        assert shell["parameters"]["properties"].get("background") == {"type": "boolean", "default": False}, "background schema absent"
        output = next(i["output"] for i in requests[1]["input"] if i.get("type") == "function_call_output")
        running = json.loads(output)
        assert running["status"] == "running" and running["shellID"]
        db = sqlite3.connect(data / "oc.sqlite")
        jobs = db.execute("SELECT count(*) FROM shell_jobs").fetchone()[0]
        assert jobs == 1
        phase, outcome, message = db.execute("SELECT phase,outcome,message_id FROM shell_jobs").fetchone()
        assert phase == "terminal" and json.loads(outcome)["state"] == "cancelled" and message
        assert db.execute("SELECT count(*) FROM events WHERE kind='shell_notice'").fetchone()[0] == 1
        pid = int(project.joinpath("leader").read_text())
        assert not Path(f"/proc/{pid}").exists(), "headless shutdown did not reap owned leader"
        db.close()
        print(json.dumps({"case": "actual-background-launch-shutdown", "status": "PASS", "requests": len(requests), "jobs": jobs, "terminal": 1, "notices": 1, "leaderReaped": True}))


def completed(text="done"):
    return [{"type": "response.output_text.delta", "delta": text},
            {"type": "response.completed", "response": {"status": "completed"}}]


def tool(name, args, call="background-call"):
    item = {"type": "function_call", "id": call, "call_id": call,
            "name": name, "status": "completed", "arguments": json.dumps(args)}
    return [{"type": "response.output_item.added", "item": item},
            {"type": "response.output_item.done", "item": item},
            {"type": "response.completed", "response": {"status": "completed"}}]


def until(predicate, reason, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.01)
    raise AssertionError(reason)


class Native:
    def __init__(self, binary, script, permission=None):
        self.temp = tempfile.TemporaryDirectory(prefix="t50-background-owned-")
        self.root = Path(self.temp.name)
        self.project, self.home, self.data = [self.root / x for x in ("project", "home", "data")]
        config = self.home / "config/opencode"
        config.mkdir(parents=True)
        self.project.mkdir()
        self.project.joinpath("seed").write_text("fixture seed")
        self.requests, self.errors = [], []
        self.physical_requests,self.auxiliary_requests=0,0
        self.request_counter_lock=threading.Lock()
        self.process, self.fd, self.reader = None, None, None
        self.tail = bytearray()
        self.height = 34
        owner = self

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                with owner.request_counter_lock:
                    owner.physical_requests+=1
                try:
                    size = int(self.headers.get("content-length", "0"))
                    assert 0 < size <= 1048576
                    request = json.loads(self.rfile.read(size))
                    auxiliary = request.get("max_output_tokens") == 256 and not request.get("tools")
                    if auxiliary:
                        with owner.request_counter_lock:
                            owner.auxiliary_requests+=1
                        events = completed("Synthetic title")
                    else:
                        owner.requests.append(request)
                        events = script(owner, len(owner.requests), request)
                    body = "".join("data: " + json.dumps(e) + "\n\n" for e in events).encode()
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except Exception as error:
                    owner.errors.append(repr(error))

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        config.joinpath("opencode.json").write_text(json.dumps({
            "model": "fixture/m", "compaction": {"auto": False},
            "permission": permission or {"shell": "allow", "read": "allow"},
            "agent": {"title": {"disable": True}},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{self.server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}))
        self.env = {"HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
                    "PATH": "/usr/bin:/bin", "SHELL": "/bin/bash", "TERM": "xterm-256color",
                    "OC_TEST_ALLOW_LOOPBACK": "1", "OPENAI_API_KEY": "synthetic-trap",
                    "T50_SECRET": "synthetic-trap", "BASH_ENV": str(self.project / "trap")}
        self.project.joinpath("trap").write_text("touch forbidden-startup")
        self.binary = binary

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.stop()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        assert not self.thread.is_alive(), "owned fake HTTP worker did not join"
        self.temp.cleanup()

    def start(self):
        self.tail.clear()
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", self.height, 110, 0, 0))
        self.fd = master
        self.process = subprocess.Popen([str(self.binary), "--data-dir", str(self.data), "tui", "--session", "t50-background"],
            cwd=self.project, env=self.env, stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
        os.close(slave)
        process = self.process
        def drain():
            while process.poll() is None:
                if select.select([master], [], [], .05)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        return
                    self.tail.extend(chunk)
                    del self.tail[:-16384]
                    if hasattr(self, "consume_pty"):
                        self.consume_pty(chunk)
        self.reader = threading.Thread(target=drain, daemon=True)
        self.reader.start()
        until(lambda: b"Untitled session" in self.tail or b"What" in self.tail or b"Synthetic title" in self.tail,
              "native TUI did not become ready: " + str(self.process.poll()) + repr(bytes(self.tail[-500:])))

    def stop(self, crash=False, expected_exit=0):
        if self.process is None:
            return
        if self.process.poll() is None:
            if crash:
                self.process.kill()
            else:
                self.send(b"\x03\x03")
            try:
                self.process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)
                raise AssertionError("owned native application did not stop")
        self.reader.join(timeout=1)
        assert not self.reader.is_alive(), "owned PTY reader did not join"
        if not crash:
            assert self.process.returncode == expected_exit, "native application cleanup status mismatch"
        os.close(self.fd)
        self.process = None

    def send(self, data):
        assert self.process.poll() is None, "native application exited"
        os.write(self.fd, data)

    def message_action(self, text, index):
        # Decode only the bounded live PTY tail to locate the real painted user
        # block. No screenshot/frame artifact or product-only test route.
        def position():
            grid = [[" " for _ in range(110)] for _ in range(self.height)]
            row = col = 0
            for part in re.split(r"(\x1b\[[0-9;?]*[ -/]*[@-~])", self.tail.decode("utf-8", "replace")):
                if part.startswith("\x1b["):
                    if part[-1] in "Hf":
                        values = [int(v or "1") for v in part[2:-1].split(";")]
                        row, col = (values + [1])[0]-1, (values + [1])[1]-1
                    elif part.endswith("2J"):
                        grid = [[" " for _ in range(110)] for _ in range(self.height)]
                    elif part.endswith("K") and 0 <= row < self.height:
                        for x in range(max(col, 0), 110): grid[row][x] = " "
                    continue
                for ch in part:
                    if ch == "\r": col = 0
                    elif ch == "\n": row += 1
                    elif ch.isprintable():
                        if 0 <= row < self.height and 0 <= col < 110: grid[row][col] = ch
                        col += 2 if unicodedata.east_asian_width(ch) in "WF" else 1
            for y, line in enumerate(grid):
                x = "".join(line).find(text)
                if x >= 0:
                    after = "\n".join("".join(row) for row in grid[y+1:])
                    if re.search(r"Build\s*·\s*(?:fixture/)?m\s*·\s*\d", after):
                        return x+1, y+1
            return None
        x, y = until(position, "painted user block not found: " + text)
        self.tail.clear()
        self.send(f"\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m".encode())
        until(lambda: b"Jump to" in self.tail and b"Fork" in self.tail, "real message action panel did not open")
        self.send(b"\x1b[B" * index + b"\r")

    def completed_frame(self, after_text=None):
        # A durable completed-turn footer is painted only after the consumer
        # reconciles its finished turn. DB settlement alone is not a key ACK.
        text = re.sub(rb"\x1b\[[0-9;?]*[ -/]*[@-~]", b"", bytes(self.tail)).decode("utf-8", "replace")
        if after_text is not None:
            if after_text not in text:
                return False
            text = text.split(after_text, 1)[1]
        return re.search(r"Build\s*·\s*(?:fixture/)?m\s*·\s*\d", text)

    def rows(self, sql, args=()):
        if not self.data.joinpath("oc.sqlite").exists():
            return []
        with sqlite3.connect(self.data / "oc.sqlite", timeout=2) as db:
            return db.execute(sql, args).fetchall()

    def job(self):
        rows = self.rows("SELECT operation_id,session_id,phase,provenance,process,outcome,delivery_id,message_id FROM shell_jobs")
        assert len(rows) <= 1
        return rows[0] if rows else None

    def settled(self, count=1):
        return self.rows("SELECT id FROM turns WHERE status='completed'") if len(self.rows("SELECT id FROM turns WHERE status='completed'")) >= count else None

    def assert_notice(self, state):
        job = until(lambda: self.job() if self.job() and self.job()[7] else None, "automatic notice not committed")
        outcome = json.loads(job[5])
        assert outcome["state"] == state, outcome["state"]
        assert self.rows("SELECT count(*) FROM events WHERE kind='shell_terminal'") == [(1,)]
        assert self.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(1,)]
        messages = self.rows("SELECT session_id,role,text FROM messages WHERE id=?", (job[7],))
        assert len(messages) == 1 and messages[0][:2] == (job[1], "user")
        assert job[6] in messages[0][2] and job[0] in messages[0][2]
        assert "synthetic-trap" not in outcome["stdout"] + outcome["stderr"]
        assert not self.project.joinpath("forbidden-startup").exists()
        assert self.rows("SELECT count(*) FROM permission_grants") == [(0,)]
        return job, outcome


def busy_notice(binary):
    busy, release = threading.Event(), threading.Event()
    command = "printf '%s' $$ > leader; printf one >> effect; cat barrier >/dev/null; printf BG_COMPLETE; printf '%s' \"${OPENAI_API_KEY:-}${T50_SECRET:-}\""
    def script(owner, count, request):
        if count == 1:
            return tool("shell", {"command": command, "background": True})
        if count == 3:
            busy.set()
            assert release.wait(10), "busy fake response not released"
            return tool("read", {"path": "seed"}, "read-call")
        return completed(f"R2_NOTICE_STEP_{count}") if SHELL_CONTROLS_SUPPORTED else completed()
    with Native(binary, script) as native:
        os.mkfifo(native.project / "barrier")
        native.start()
        native.send(b"launch\r")
        until(native.settled, "calling turn did not complete while background blocked")
        if SHELL_CONTROLS_SUPPORTED:
            until(lambda: native.completed_frame("R2_NOTICE_STEP_2"), "calling turn consumer did not acknowledge its exact completed footer")
        job = native.job()
        assert job[2] == "running" and not job[5] and not job[7]
        assert len(native.requests) == 2
        running = json.loads(next(i["output"] for i in native.requests[1]["input"] if i.get("type") == "function_call_output"))
        assert running["shellID"] == job[0] and running["status"] == "running"
        native.send(b"busy\r")
        assert busy.wait(5), "next prompt blocked by job/global turn lock"
        with open(native.project / "barrier", "wb", buffering=0) as barrier:
            barrier.write(b"release")
        job, outcome = native.assert_notice("completed")
        assert outcome["stdout"] == "BG_COMPLETE" and outcome["exit"] == 0
        assert len(native.requests) == 3, "completion generated an extra provider request"
        release.set()
        until(lambda: native.settled(2), "busy continuation did not finish")
        if SHELL_CONTROLS_SUPPORTED:
            until(lambda: native.completed_frame("R2_NOTICE_STEP_4"), "busy continuation consumer did not acknowledge its exact completed footer")
        assert len(native.requests) == 4
        wire = json.dumps(native.requests[3]["input"])
        assert wire.count(job[6]) == 1, "busy boundary omitted or duplicated notice"
        native.send(b"next\r")
        until(lambda: native.settled(3), "history continuation failed")
        assert json.dumps(native.requests[4]["input"]).count(job[6]) == 1, [(item.get("type"),item.get("role"),json.dumps(item).count(job[6])) for item in native.requests[4]["input"]]
        assert native.project.joinpath("effect").read_text() == "one"
        provenance = json.loads(job[3])
        assert provenance["session"] == job[1] and provenance["operation"] == job[0]
        assert provenance["location"] == str(native.project) and provenance["generation"] > 0
        assert provenance["selected_shell"] == "/bin/bash"
        assert provenance["agent"] == "build" and provenance["model"] == native.requests[0]["model"]
        assert provenance["provider"] == "fixture"
        assert native.rows("SELECT turn_id FROM tool_operations WHERE id=?",(job[0],)) == [(provenance["turn"],)]
        assert not native.errors, native.errors
        print(json.dumps({"case": "calling-turn-idle-busy-boundary-history", "status": "PASS", "requests": 5,
                          "jobs": 1, "terminal": 1, "notices": 1, "effects": 1}))


def terminal_cases(binary):
    for name, args, state in [
        ("explicit-timeout", {"command": "printf '%s' $$ > leader; trap '' TERM; while :; do :; done", "background": True, "timeout": 100}, "timed_out"),
        ("idle-cancel-zero-output", {"command": "printf '%s' $$ > leader; trap '' TERM; printf '%2097152s' ''; printf '%2097152s' '' >&2; touch pressure-ready; while :; do sleep .01; done", "background": True, "timeout": 0}, "cancelled"),
    ]:
        def script(owner, count, request):
            return tool("shell", args) if count == 1 else completed()
        with Native(binary, script) as native:
            native.start()
            native.send(b"launch\r")
            until(native.settled, "calling turn not complete")
            if state == "cancelled":
                until(native.completed_frame, "foreground consumer did not paint its completed-turn footer")
                until(lambda: native.project.joinpath("leader").exists(), "zero job not launched")
                until(lambda: native.project.joinpath("pressure-ready").exists(), "both bounded output readers did not drain producer pressure")
                # Output pressure is measured by the producer's writes, not a time assertion.
                until(lambda: native.job() and native.job()[2] == "running", "job not running")
                native.tail.clear()
                native.send(b"\x1b")
                until(lambda: native.job()[2] == "terminal" or b"again to interrupt" in re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", bytes(native.tail)), "native cancellation key was neither accepted nor armed")
                if native.job()[2] != "terminal":
                    native.send(b"\x1b")
            job, outcome = native.assert_notice(state)
            assert outcome["timeout"] == (state == "timed_out")
            assert outcome["cancelled"] == (state == "cancelled")
            assert len(outcome["stdout"].encode()) <= 1048576
            assert len(outcome["stderr"].encode()) <= 1048576
            if state == "cancelled":
                assert outcome["stdout_truncated"] and outcome["stderr_truncated"]
                assert len(outcome["stdout"]) == len(outcome["stderr"]) == 1048576
            pid = int(native.project.joinpath("leader").read_text())
            assert not Path(f"/proc/{pid}").exists(), "owned child not reaped"
            assert len(native.requests) == 2, "idle completion called provider"
            print(json.dumps({"case": name, "status": "PASS", "requests": 2, "terminal": 1, "notices": 1, "reaped": True}))


def restart_cases(binary):
    for terminal in [False, True]:
        def script(owner, count, request):
            if count == 1:
                return tool("shell", {"command": "printf '%s' $$ > leader; printf one >> effect; cat barrier >/dev/null; printf FINISHED", "background": True})
            return completed()
        with Native(binary, script) as native:
            os.mkfifo(native.project / "barrier")
            native.start()
            native.send(b"launch\r")
            until(native.settled, "calling turn not complete before crash")
            if terminal:
                with open(native.project / "barrier", "wb", buffering=0) as barrier:
                    barrier.write(b"release")
                native.assert_notice("completed")
            native.stop(crash=True)
            native.start()
            job, outcome = native.assert_notice("completed" if terminal else "unknown")
            assert native.project.joinpath("effect").read_text() == "one", "restart replayed command"
            assert len(native.requests) == 2, "restart generated a provider request"
            native.stop()
            native.start()
            native.assert_notice("completed" if terminal else "unknown")
            assert native.project.joinpath("effect").read_text() == "one"
            assert not native.errors, native.errors
            print(json.dumps({"case": "reopen-terminal-once" if terminal else "crash-running-no-replay", "status": "PASS",
                              "requests": 2, "terminal": 1, "notices": 1, "effects": 1}))


def crash_fences(binary):
    for boundary in ["before-fork", "terminal-before-notice"]:
        def script(owner, count, request):
            return tool("shell", {"command": "printf one >> effect; cat barrier >/dev/null; printf COMMITTED_RESULT", "background": True}) if count == 1 else completed()
        with Native(binary, script) as native:
            os.mkfifo(native.project / "barrier")
            if boundary == "before-fork":
                library = native.root / "fork_barrier.so"
                result = subprocess.run(["cc", "-shared", "-fPIC", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror",
                    str(Path(__file__).with_name("fork_barrier.c")), "-o", str(library), "-ldl"], env=native.env,
                    capture_output=True, timeout=15)
                assert result.returncode == 0, result.stderr.decode()
                native.root.joinpath("arm").touch()
                native.env.update({"LD_PRELOAD":str(library),"T50_FORK_ARM":str(native.root / "arm"),
                    "T50_FORK_REACHED":str(native.root / "reached")})
            native.start()
            if boundary == "terminal-before-notice":
                with sqlite3.connect(native.data / "oc.sqlite") as db:
                    db.execute("CREATE TRIGGER t50_delivery_fault BEFORE INSERT ON messages WHEN NEW.text LIKE 'Automatic background shell result%' BEGIN SELECT RAISE(ABORT,'owned fixture delivery fence'); END")
            native.send(b"launch\r")
            if boundary == "before-fork":
                until(lambda: native.root.joinpath("reached").exists(), "actual native fork barrier not reached")
                assert native.job()[2] == "admitted" and native.job()[4] is None
                assert not native.project.joinpath("effect").exists()
            else:
                until(native.settled, "calling turn did not complete")
                with open(native.project / "barrier", "wb", buffering=0) as barrier:
                    barrier.write(b"release")
                until(lambda: native.job()[2] == "terminal", "terminal outcome not frozen")
                assert json.loads(native.job()[5])["stdout"] == "COMMITTED_RESULT"
                assert native.job()[7] is None
                assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(0,)]
            native.stop(crash=True)
            for key in ["LD_PRELOAD","T50_FORK_ARM","T50_FORK_REACHED"]:
                native.env.pop(key,None)
            if boundary == "terminal-before-notice":
                with sqlite3.connect(native.data / "oc.sqlite") as db:
                    db.execute("DROP TRIGGER t50_delivery_fault")
            requests = len(native.requests)
            native.start()
            job, outcome = native.assert_notice("unknown" if boundary == "before-fork" else "completed")
            assert len(native.requests) == requests, "restart forked/generated"
            if boundary == "before-fork":
                assert not native.project.joinpath("effect").exists()
            else:
                assert native.project.joinpath("effect").read_text() == "one"
                assert outcome["stdout"] == "COMMITTED_RESULT" and outcome["exit"] == 0
            print(json.dumps({"case": boundary, "status":"PASS","requests":requests,"terminal":1,"notices":1,
                              "effects":0 if boundary == "before-fork" else 1}))


def parked_source(binary):
    command = "printf '%s' $$ > leader; pwd > cwd; test -z \"${OPENAI_API_KEY:-}${T50_SECRET:-}\"; cat barrier >/dev/null; touch marker; printf PARKED_COMPLETE"
    def script(owner, count, request):
        return tool("shell", {"command":command,"workdir":"space dir","background":True}) if count == 1 else completed()
    with Native(binary,script) as native:
        workdir = native.project / "space dir"
        workdir.mkdir()
        os.mkfifo(workdir / "barrier")
        selector = native.root / "selected shell"
        selector.write_text("#!/bin/sh\nprintf selected > shell-selected\nexec /bin/bash \"$@\"\n")
        selector.chmod(0o700)
        native.env["SHELL"] = str(selector)
        native.start()
        native.send(b"launch\r")
        until(native.settled,"calling turn not complete before parking")
        until(native.completed_frame,"calling turn consumer not complete before parking")
        native.tail.clear()
        native.send(b"/new\r")
        until(lambda:b"Ask anything" in native.tail,"actual /new Home route not acknowledged")
        # The next prompt creates a genuinely different source session.
        native.send(b"other\r")
        until(lambda:native.settled(2),"parked source prevented another session turn")
        sessions = native.rows("SELECT DISTINCT session_id FROM turns WHERE status='completed'")
        assert len(sessions) == 2, "actual /new did not create a different session"
        with open(workdir / "barrier","wb",buffering=0) as barrier:
            barrier.write(b"release")
        job,outcome = native.assert_notice("completed")
        assert outcome["stdout"] == "PARKED_COMPLETE"
        other = next(session[0] for session in sessions if session[0] != job[1])
        assert native.rows("SELECT count(*) FROM messages WHERE session_id=? AND text LIKE 'Automatic background shell result%'",(other,)) == [(0,)]
        assert len(native.requests) == 3, "parked completion generated"
        assert workdir.joinpath("cwd").read_text().strip() == str(workdir)
        assert workdir.joinpath("shell-selected").read_text() == "selected"
        assert workdir.joinpath("marker").exists() and not native.project.joinpath("marker").exists()
        provenance = json.loads(job[3])
        assert provenance["cwd"] == str(workdir) and provenance["selected_shell"] == str(selector)
        print(json.dumps({"case":"parked-source-selected-shell-pinned-cwd-env","status":"PASS","requests":3,"sessions":2,"terminal":1,"notices":1}))


def foreground_barrier(binary):
    def script(owner,count,request):
        return tool("shell",{"command":"printf '%s' $$ > leader; cat barrier >/dev/null; printf FG_COMPLETE","background":False}) if count == 1 else completed()
    with Native(binary,script) as native:
        os.mkfifo(native.project / "barrier")
        native.start()
        native.send(b"foreground\r")
        until(lambda:native.project.joinpath("leader").exists(),"foreground shell did not reach barrier")
        assert len(native.requests) == 1 and not native.settled()
        if SHELL_CONTROLS_SUPPORTED:
            job = native.job()
            assert job[2] == "running" and json.loads(job[4])["pid"] == int(native.project.joinpath("leader").read_text())
        else:
            assert native.job() is None
        with open(native.project / "barrier","wb",buffering=0) as barrier:
            barrier.write(b"release")
        until(native.settled,"foreground did not finish after release")
        assert len(native.requests) == 2
        if SHELL_CONTROLS_SUPPORTED:
            job = native.job()
            assert job[2] == "terminal" and job[7] is None and json.loads(job[5])["state"] == "completed"
            assert native.rows("SELECT count(*) FROM events WHERE kind='shell_foreground'") == [(1,)]
        else:
            assert native.job() is None
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(0,)]
        output = next(i["output"] for i in native.requests[1]["input"] if i.get("type") == "function_call_output")
        assert "FG_COMPLETE" in output and "running" not in output
        print(json.dumps({"case":"foreground-false-barrier-waits","status":"PASS","requests":2,"jobs":int(SHELL_CONTROLS_SUPPORTED),"notices":0}))


def admission_and_ceiling(binary):
    for name,permission,args,auto,jobs,intents in [
        ("deny-alias",{"bash":"deny"},{"command":"touch effect","background":True},False,0,0),
        ("deny-mixed-alias",{"bash":"deny","shell":"allow"},{"command":"touch effect","background":True},False,0,0),
        ("ask-no-consumer",{"bash":"ask"},{"command":"touch effect","background":True},False,0,0),
        ("invalid-background-null",{"shell":"allow"},{"command":"touch effect","background":None},False,0,0),
        ("escape-root",{"shell":"allow"},{"command":"touch effect","workdir":"../","background":True},False,0,0),
        ("ask-once",{"bash":"ask"},{"command":"printf one >> effect; while [ ! -f release ]; do sleep .01; done","background":True},True,1,1),
        ("active-resource-ceiling",{"shell":"allow"},{"command":"while [ ! -f release ]; do sleep .01; done","background":True},False,8,8),
    ]:
        def script(owner,count,request):
            if count == 1:
                if name == "active-resource-ceiling":
                    events = []
                    for index in range(9):
                        events.extend(tool("shell",args,f"cap-{index}")[:-1])
                    return events + completed()[1:]
                return tool("shell",args)
            return completed()
        with Native(binary,script,permission) as native:
            native.start()
            native.stop()
            with sqlite3.connect(native.data / "oc.sqlite") as db:
                db.execute("CREATE TABLE t50_intents(op TEXT PRIMARY KEY)")
                db.execute("CREATE TRIGGER t50_intent_count AFTER INSERT ON tool_operations WHEN NEW.state='started' BEGIN INSERT INTO t50_intents VALUES(NEW.id); END")
            cmd = [str(binary),"--data-dir",str(native.data),"run","--json","--session","t50-background","admit"]
            if auto:
                cmd.append("--auto")
            result = subprocess.run(cmd,cwd=native.project,env=native.env,capture_output=True,timeout=20)
            expected_requests = 1 if name == "ask-no-consumer" else 2
            assert result.returncode == (1 if name == "ask-no-consumer" else 0), result.stderr.decode()[-1000:]
            assert len(native.requests) == expected_requests,(name,len(native.requests))
            assert native.rows("SELECT count(*) FROM shell_jobs") == [(jobs,)]
            assert native.rows("SELECT count(*) FROM t50_intents") == [(intents,)]
            assert native.rows("SELECT count(*) FROM permission_grants") == [(0,)]
            assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(jobs,)]
            if jobs:
                assert native.rows("SELECT count(*) FROM shell_jobs WHERE phase='terminal' AND message_id IS NOT NULL") == [(jobs,)]
                for (raw,) in native.rows("SELECT process FROM shell_jobs"):
                    identity = json.loads(raw)
                    assert identity["owner_root"] == str(native.data)
                    assert not Path(f"/proc/{identity['pid']}").exists(),"shutdown did not reap admitted job"
            else:
                assert not native.project.joinpath("effect").exists()
            if name == "ask-once":
                assert native.project.joinpath("effect").read_text() == "one"
            if name == "active-resource-ceiling":
                assert native.rows("SELECT count(*) FROM tool_operations WHERE state='failed' AND output LIKE '%resource ceiling%'") == [(1,)]
            assert not native.errors,native.errors
            print(json.dumps({"case":name,"status":"PASS","requests":expected_requests,"jobs":jobs,"intents":intents,"notices":jobs,"grants":0}))


def child_ceilings(binary):
    import native_foreground as foreground
    foreground.BACKGROUND_SUPPORTED = True
    foreground.QUESTION_SUPPORTED = QUESTION_SUPPORTED
    foreground.SHELL_CONTROLS_SUPPORTED = SHELL_CONTROLS_SUPPORTED
    for profile in ["plan","general","explore"]:
        foreground.run(binary,f"{profile}-background-ceiling","shell",{"command":"touch marker","background":True},
            {"shell":"allow","subagent":"allow","read":"allow","glob":"allow","grep":"allow","compress":"allow"},
            "denied",profile=profile,child=profile != "plan")


def normal_group_completion(binary):
    command = "printf '%s' $$ > leader; (trap '' TERM; touch descendant-ready; exec sleep 30) >/dev/null 2>&1 & printf '%s' $! > descendant; while [ ! -f descendant-ready ]; do sleep .01; done; printf LEADER_EXIT"
    def script(owner,count,request):
        return tool("shell",{"command":command,"background":True}) if count == 1 else completed()
    with Native(binary,script) as native:
        native.start()
        native.send(b"launch\r")
        job,outcome = native.assert_notice("completed")
        assert outcome["stdout"] == "LEADER_EXIT" and outcome["exit"] == 0
        leader = int(native.project.joinpath("leader").read_text())
        descendant = int(native.project.joinpath("descendant").read_text())
        assert not Path(f"/proc/{leader}").exists(),"leader not reaped"
        stat = Path(f"/proc/{descendant}/stat")
        until(lambda: not stat.exists() or stat.read_text().rsplit(") ",1)[1].split()[0] == "Z", "normal completion left an owned group member running")
        assert len(native.requests) == 2
        print(json.dumps({"case":"normal-leader-exit-closes-owned-group","status":"PASS","requests":2,"terminal":1,"notices":1,"leaderReaped":True,"descendantRunning":False}))


def unverified_identity(binary):
    def script(owner,count,request):
        return tool("shell",{"command":"printf '%s' $$ > leader; printf one >> effect; cat barrier >/dev/null","background":True}) if count == 1 else completed()
    with Native(binary,script) as native:
        os.mkfifo(native.project / "barrier")
        native.start()
        native.send(b"launch\r")
        until(native.settled,"calling turn did not finish")
        original = json.loads(native.job()[4])
        native.stop(crash=True)
        altered = dict(original,start_ticks=original["start_ticks"] + 1)
        with sqlite3.connect(native.data / "oc.sqlite") as db:
            db.execute("UPDATE shell_jobs SET process=?",(json.dumps(altered),))
        try:
            native.start()
            job,outcome = native.assert_notice("unknown")
            assert "unverified; no signal sent" in outcome["diagnostic"]
            stat = Path(f"/proc/{original['pid']}/stat").read_text().rsplit(") ",1)[1].split()
            assert int(stat[19]) == original["start_ticks"] and stat[0] != "Z", "unverified stored identity was signalled"
            assert native.project.joinpath("effect").read_text() == "one" and len(native.requests) == 2
        finally:
            # The checker owns the original fixture group, independently of the
            # intentionally corrupted recovery record. Verify before cleanup.
            path = Path(f"/proc/{original['pid']}/stat")
            if path.exists():
                fields = path.read_text().rsplit(") ",1)[1].split()
                assert int(fields[19]) == original["start_ticks"] and int(fields[2]) == original["pid"]
                os.killpg(original["pid"],signal.SIGKILL)
        print(json.dumps({"case":"recovery-unverified-identity-no-signal","status":"PASS","requests":2,"terminal":1,"notices":1,"effects":1,"replayed":False}))


def launch_freeze_failure(binary):
    def script(owner,count,request):
        return tool("shell",{"command":"printf one >> effect; while [ ! -f release ]; do sleep .01; done","background":True}) if count == 1 else completed()
    with Native(binary,script) as native:
        library = native.root / "fork_record.so"
        result = subprocess.run(["cc","-shared","-fPIC","-O2","-std=c11","-Wall","-Wextra","-Werror",
            str(Path(__file__).with_name("fork_barrier.c")),"-o",str(library),"-ldl"],env=native.env,capture_output=True,timeout=15)
        assert result.returncode == 0,result.stderr.decode()
        native.env.update({"LD_PRELOAD":str(library),"T50_FORK_PID":str(native.root / "fork_pid")})
        native.start()
        with sqlite3.connect(native.data / "oc.sqlite") as db:
            db.execute("CREATE TRIGGER t50_start_fault BEFORE UPDATE OF phase ON shell_jobs WHEN NEW.phase='running' BEGIN SELECT RAISE(ABORT,'owned fixture launch freeze fault'); END")
        native.send(b"launch\r")
        job,outcome = native.assert_notice("unknown")
        until(native.settled,"calling turn did not finish launch error")
        assert native.rows("SELECT state FROM tool_operations") == [("failed",)]
        assert all('"status":"running"' not in i.get("output","") for request in native.requests for i in request["input"])
        assert "durable launch admission failed" in outcome["diagnostic"]
        pid = int(native.root.joinpath("fork_pid").read_text())
        assert not Path(f"/proc/{pid}").exists(),"failed durable launch left an owned child"
        native.stop(expected_exit=1)
        native.env.pop("LD_PRELOAD")
        native.env.pop("T50_FORK_PID")
        with sqlite3.connect(native.data / "oc.sqlite") as db:
            db.execute("DROP TRIGGER t50_start_fault")
        count = len(native.requests)
        native.start()
        native.assert_notice("unknown")
        assert len(native.requests) == count
        print(json.dumps({"case":"launch-freeze-error-reap-nonsuccess","status":"PASS","requests":count,"terminal":1,"notices":1,"leaderReaped":True,"shutdownExit":1}))


def review_revert(binary):
    def script(owner, count, request):
        if count in (1, 3):
            name = "old" if count == 1 else "new"
            return tool("shell", {"command":f"printf one >> {name}-effect; cat {name}-barrier >/dev/null; printf {name}-result", "background":True}, name)
        return completed()
    with Native(binary, script) as native:
        for name in ("old", "new"): os.mkfifo(native.project / (name+"-barrier"))
        native.height=70
        native.start(); native.send(b"review-old-launch\r")
        until(native.settled,"old calling turn not completed")
        with open(native.project / "old-barrier","wb",buffering=0) as barrier: barrier.write(b"release")
        old, _ = native.assert_notice("completed")
        native.message_action("review-old-launch",1)
        until(lambda: native.rows("SELECT count(*) FROM events WHERE kind='conversation_changed'")==[(1,)],"real Revert not admitted")
        native.send(b"\x15replacement\r")
        until(lambda: native.settled(2),"replacement did not complete")
        assert old[6] not in json.dumps(native.requests[2]["input"]),"Revert resurrected the hidden shell notice"
        with open(native.project / "new-barrier","wb",buffering=0) as barrier: barrier.write(b"release")
        until(lambda: native.rows("SELECT count(*) FROM shell_jobs WHERE message_id IS NOT NULL")==[(2,)],"new visible notice not delivered")
        new = native.rows("SELECT delivery_id FROM shell_jobs WHERE operation_id!=?",(old[0],))[0][0]
        native.send(b"visible-continue\r"); until(lambda:native.settled(3),"visible continuation incomplete")
        wire=json.dumps(native.requests[4]["input"])
        assert old[6] not in wire and wire.count(new)==1
        assert all(native.project.joinpath(name+"-effect").read_text()=="one" for name in ("old","new"))
        print(json.dumps({"case":"review-real-revert-visible-notices","status":"PASS","requests":5,"jobs":2,"terminal":2,"notices":2,"oldVisible":0,"newVisible":1,"effects":2}))


def review_fork(binary):
    busy, release = threading.Event(), threading.Event()
    def script(owner,count,request):
        if count==1: return tool("shell",{"command":"printf one >> effect; cat barrier >/dev/null; printf result","background":True})
        if count==3:
            busy.set(); assert release.wait(10)
            return tool("read",{"path":"seed"},"read-call")
        return completed("")
    with Native(binary,script) as native:
        os.mkfifo(native.project / "barrier")
        native.height=70
        native.start(); native.send(b"launch\r"); until(native.settled,"launch incomplete")
        native.send(b"busy\r"); assert busy.wait(5)
        with open(native.project / "barrier","wb",buffering=0) as barrier: barrier.write(b"release")
        job,_=native.assert_notice("completed"); release.set()
        until(lambda:native.settled(2),"busy continuation incomplete")
        assert json.dumps(native.requests[3]["input"]).count(job[6])==1
        native.send(b"review-fork-boundary\r"); until(lambda:native.settled(3),"boundary incomplete")
        native.message_action("review-fork-boundary",3)
        fork=until(lambda:native.rows("SELECT id FROM sessions WHERE id LIKE 'fork-%'"),"real fork not admitted")[0][0]
        assert len(native.requests)==5,"fork generated a model request"
        until(lambda: b"(fork)" in native.tail,"fork refresh not adopted")
        native.send(b"\x15fork-continue\r"); until(lambda:native.settled(6),"fork continuation incomplete")
        assert json.dumps(native.requests[5]["input"]).count(job[6])==1,"fork notice duplicated"
        assert native.rows("SELECT count(*) FROM shell_jobs WHERE session_id=?",(fork,))==[(0,)]
        native.stop(); native.start(); native.send(b"original-continue\r")
        until(lambda:native.settled(7),"original continuation incomplete")
        assert json.dumps(native.requests[6]["input"]).count(job[6])==1
        assert native.project.joinpath("effect").read_text()=="one"
        assert native.rows("SELECT count(*) FROM shell_jobs")==[(1,)]
        print(json.dumps({"case":"review-real-fork-captured-notice","status":"PASS","requests":7,"jobs":1,"terminal":1,"notices":1,"forkNotice":1,"sourceNotice":1,"forkJobs":0,"effects":1}))


def review_capacity(binary):
    def script(owner,count,request):
        if count==1:
            events=[]
            for index in range(8):
                events.extend(tool("shell",{"command":f"printf one > effect-{index}; cat barrier-{index} >/dev/null", "background":True},f"job-{index}")[:-1])
            return events+[completed()[-1]]
        if count==3: return tool("shell",{"command":"printf one > effect-8", "background":True},"reused-slot")
        return completed()
    with Native(binary,script) as native:
        for index in range(8): os.mkfifo(native.project / f"barrier-{index}")
        native.start(); native.send(b"launch-batch\r"); until(native.settled,"batch calling turn incomplete")
        until(lambda:len(list(native.project.glob("effect-*")))==8,"actual admitted jobs did not reach barriers")
        assert native.rows("SELECT count(*) FROM shell_jobs WHERE phase='running'")==[(8,)]
        assert len(native.requests)==2
        for index in range(8):
            with open(native.project / f"barrier-{index}","wb",buffering=0) as barrier: barrier.write(b"release")
        until(lambda:native.rows("SELECT count(*) FROM shell_jobs WHERE message_id IS NOT NULL")==[(8,)],"idle batch completion lost receipts")
        assert len(native.requests)==2,"idle worker completion generated a model request"
        native.send(b"reuse-completed-slot\r"); until(lambda:native.settled(2),"reused slot calling turn incomplete")
        until(lambda:native.rows("SELECT count(*) FROM shell_jobs WHERE message_id IS NOT NULL")==[(9,)],"completed slots were stranded")
        assert len(native.requests)==4
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'")==[(9,)]
        assert all(native.project.joinpath(f"effect-{index}").read_text()=="one" for index in range(9))
        print(json.dumps({"case":"review-real-idle-joined-capacity-reuse","status":"PASS","requests":4,"jobs":9,"terminal":9,"notices":9,"idleRequests":0,"reusedSlots":1,"effects":9}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--review-only",nargs="?",const="both",choices=["both","revert","fork","capacity"])
    parser.add_argument("--question-supported",action="store_true",help="current question catalog for the shared foreground checker; historical mode remains the default")
    parser.add_argument("--shell-controls-supported",action="store_true",help="current foreground ledger and effective file family; keeps historical mode")
    args = parser.parse_args()
    QUESTION_SUPPORTED = args.question_supported
    SHELL_CONTROLS_SUPPORTED = args.shell_controls_supported
    binary = args.binary.resolve()
    print(json.dumps({"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}))
    if args.review_only:
        if args.review_only in ("both","revert"): review_revert(binary)
        if args.review_only in ("both","fork"): review_fork(binary)
        if args.review_only in ("both","capacity"): review_capacity(binary)
        raise SystemExit(0)
    launch(binary)
    busy_notice(binary)
    terminal_cases(binary)
    restart_cases(binary)
    crash_fences(binary)
    parked_source(binary)
    foreground_barrier(binary)
    admission_and_ceiling(binary)
    child_ceilings(binary)
    normal_group_completion(binary)
    unverified_identity(binary)
    launch_freeze_failure(binary)
    review_revert(binary)
    review_fork(binary)
    review_capacity(binary)
