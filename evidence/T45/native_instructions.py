#!/usr/bin/env python3
"""PRM01 normal-ELF instruction lifecycle, loopback-only and joined ownership."""
import argparse
import ctypes
import contextlib
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


def qualify(binary):
    with tempfile.TemporaryDirectory(prefix="t45-instructions-") as temporary, contextlib.ExitStack() as ownership:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        (project / "nested").mkdir(parents=True)
        (project / "evil").mkdir()
        other = root / "other"
        other.mkdir()
        global_file = config / "AGENTS.md"
        root_file = project / "AGENTS.md"
        nested_file = project / "nested/AGENTS.md"
        global_file.write_text("GLOBAL_RULE\n")
        root_file.write_text("ROOT_RULE\n")
        nested_file.write_text("NESTED_OLD_RULE\n")
        (other / "AGENTS.md").write_text("LOCATION_B_RULE\n")
        (other / "nested").mkdir()
        (other / "nested/AGENTS.md").write_text("CHILD_NESTED_RULE\n")
        (other / "nested/file.txt").write_text("child file\n")
        (project / "nested/file.txt").write_text("first\nsecond\n")
        external = root / "external.md"
        external.write_text("EXTERNAL_FORBIDDEN_INSTRUCTION\n")
        (project / "evil/AGENTS.md").symlink_to(external)
        (project / "evil/file.txt").write_text("safe file\n")
        denied_project = root / "denied"
        (denied_project / "nested").mkdir(parents=True)
        (denied_project / "AGENTS.md").write_text("LOCATION_C_RULE\n")
        denied_file = denied_project / "nested/AGENTS.md"
        denied_file.write_text("DENIED_NESTED_RULE\n")
        (denied_project / "nested/file.txt").write_text("allowed read\n")
        # Independent actual IO oracle, without strace or reading the watched bodies.
        libc = ctypes.CDLL(None, use_errno=True)
        watch = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
        assert watch >= 0
        ownership.callback(os.close, watch)
        assert libc.inotify_add_watch(watch, os.fsencode(external), 0x20) >= 0  # IN_OPEN
        assert libc.inotify_add_watch(watch, os.fsencode(denied_file), 0x20) >= 0
        failures, requests = [], []

        def facts():
            with sqlite3.connect(data / "oc.sqlite") as database:
                return [(seq, json.loads(raw)) for seq, raw in database.execute(
                    "SELECT seq,payload FROM events WHERE kind='instructions_updated' ORDER BY seq")]

        def text(item):
            value = item.get("content", "")
            return value if isinstance(value, str) else "".join(part.get("text", "") for part in value)

        def instructions(request):
            return [(i, text(item)) for i, item in enumerate(request["input"])
                    if item.get("role") == "developer" and text(item).startswith("Instructions from:")]

        def assert_sources(request, bodies):
            sources = instructions(request)
            assert [body for _, rendered in sources for body in bodies if rendered.endswith(body)] == bodies, sources
            assert len(sources) == len(bodies), sources
            for _, rendered in sources:
                metadata = json.loads(rendered.splitlines()[1].removeprefix("<!-- oc-instructions-source: ").removesuffix(" -->"))
                body = rendered.split("\n", 2)[2]
                assert metadata["digest"] == hashlib.sha256(body.encode()).hexdigest()
                assert metadata["generation"] > 0 and metadata["revision"] > 0 and metadata["event"] > 0
                assert Path(metadata["path"]).is_absolute() and Path(metadata["root"]).is_absolute()
                stored = next(fact for seq, fact in facts() if seq == metadata["event"])
                assert stored["source"]["path"] == metadata["path"]
                assert stored["source"]["origin"] == metadata["origin"]
                assert stored["source"]["digest"] == metadata["digest"]
            return sources

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
                    number = len(requests)
                    action = None
                    if number == 1:
                        sources = assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n"])
                        user = next(i for i, item in enumerate(request["input"]) if item.get("role") == "user")
                        assert sources[-1][0] < user
                        action = "nested/file.txt"
                    elif number in (2, 3):
                        sources = assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n", "NESTED_OLD_RULE\n"])
                        output = next(i for i, item in enumerate(request["input"]) if item.get("type") == "function_call_output")
                        assert sources[-1][0] > output, "nested source is not chronological"
                        assert len(facts()) == 3, "unchanged read duplicated durable source"
                        if number == 3:
                            nested_file.write_text("NESTED_CHANGED_RULE\n")
                        action = "nested/file.txt"
                    elif number == 4:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n", "NESTED_CHANGED_RULE\n"])
                        assert "NESTED_OLD_RULE" not in json.dumps(request["input"])
                        assert len(facts()) == 4 and facts()[-1][1]["change"] == "changed"
                        assert facts()[2][1]["source"]["content"] == "NESTED_OLD_RULE\n"
                        nested_file.unlink()
                        action = "nested/file.txt"
                    elif number == 5:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n"])
                        assert "NESTED_CHANGED_RULE" not in json.dumps(request["input"])
                        assert facts()[-1][1]["change"] == "removed"
                        assert sum("no longer apply" in text(item) for item in request["input"]) == 1
                    elif number == 6:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n"])
                        assert "NESTED_OLD_RULE" not in json.dumps(request["input"])
                        action = "nested/file.txt"
                    elif number == 7:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n", "NESTED_REOPEN_RULE\n"])
                        action = "evil/missing.txt"
                    elif number == 8:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n", "NESTED_REOPEN_RULE\n"])
                        assert len(facts()) == 6, "failed read loaded instructions"
                        action = "evil/file.txt"
                    elif number == 9:
                        assert_sources(request, ["GLOBAL_RULE\n", "ROOT_RULE\n", "NESTED_REOPEN_RULE\n"])
                        assert len(facts()) == 6, "symlink instructions were admitted"
                        assert "EXTERNAL_FORBIDDEN_INSTRUCTION" not in json.dumps(request)
                    elif number == 10:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_B_RULE\n"])
                        assert "ROOT_RULE" not in json.dumps(request["input"])
                        assert "NESTED_REOPEN_RULE" not in json.dumps(request["input"])
                        action = "subagent"
                    elif number == 11:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_B_RULE\n"])
                        assert sum("CHILD_PROFILE" in text(item) for item in request["input"] if item.get("role") == "developer") == 1
                        assert "CHILD_TASK" in json.dumps(request["input"])
                        assert "Read fixture" not in json.dumps(request["input"]), "parent transcript copied"
                        action = "nested/file.txt"
                    elif number == 12:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_B_RULE\n", "CHILD_NESTED_RULE\n"])
                    elif number == 13:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_B_RULE\n"])
                    elif number == 14:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_C_RULE\n"])
                        action = "nested/file.txt"
                    elif number == 15:
                        assert_sources(request, ["GLOBAL_RULE\n", "LOCATION_C_RULE\n"])
                        assert "DENIED_NESTED_RULE" not in json.dumps(request)
                    else:
                        raise AssertionError("unexpected model call")
                    if action is not None:
                        arguments = {"agent": "helper", "description": "Instruction child", "prompt": "CHILD_TASK"} if action == "subagent" else {"path": action, "limit": 2}
                        item = {"type": "function_call", "id": f"read-{number}", "call_id": f"call-{number}",
                                "name": "subagent" if action == "subagent" else "read", "arguments": json.dumps(arguments), "status": "completed"}
                        events = [{"type": "response.output_item.added", "item": item},
                                  {"type": "response.output_item.done", "item": item},
                                  {"type": "response.completed", "response": {"status": "completed", "output": [item]}}]
                    else:
                        events = [{"type": "response.completed", "response": {"status": "completed", "output": [
                            {"type": "message", "id": f"done-{number}", "role": "assistant", "content": [{"type": "output_text", "text": "DONE"}]}]}}]
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
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
            "agent": {"title": {"disable": True}, "helper": {"mode": "subagent", "prompt": "CHILD_PROFILE", "permission": {"read": "allow"}}},
            "permission": {"read": "allow", "subagent": "allow"},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"read-model": {"limit": {"context": 65536, "output": 2048}}}}}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}

        def invoke(location, session):
            process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", "--session", session, "Read fixture"],
                cwd=location, env=environment, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                try:
                    stdout, stderr = process.communicate(timeout=20)
                except subprocess.TimeoutExpired:
                    raise AssertionError({"requests": len(requests), "peer_failures": failures}) from None
                assert not failures, failures
                assert process.returncode == 0, (process.returncode, stderr[-1024:].decode())
                assert len(stdout) < 1048576 and len(stderr) < 1048576
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)

        try:
            invoke(project, "prm01")
            with sqlite3.connect(data / "oc.sqlite") as database:
                old = database.execute("SELECT id,result FROM turns WHERE session_id='prm01' ORDER BY rowid").fetchall()
            nested_file.write_text("NESTED_REOPEN_RULE\n")
            invoke(project, "prm01")
            with sqlite3.connect(data / "oc.sqlite") as database:
                for turn, raw in old:
                    assert database.execute("SELECT result FROM turns WHERE id=?", (turn,)).fetchone()[0] == raw
                states = database.execute("SELECT state FROM tool_operations ORDER BY rowid").fetchall()
                assert states == [("completed",)] * 5 + [("failed",), ("completed",)], states
            invoke(other, "location-b")
            configuration["permission"]["read"] = {"*": "allow", str(denied_file): "deny"}
            (config / "opencode.json").write_text(json.dumps(configuration))
            invoke(denied_project, "location-c")
            assert len(requests) == 15
            try:
                opened = os.read(watch, 4096)
            except BlockingIOError:
                opened = b""
            assert not opened, "denied/external instruction source was opened"
            return {"status": "PASS", "model_requests": 15, "facts": len(facts()), "root_child_shared": True,
                    "denied_external_instruction_opens": 0,
                    "restart": True, "old_turns_immutable": True, "failed_and_symlink_guard": True,
                    "location_global_retained": True, "owned_cleanup": "joined"}
        finally:
            server.shutdown()
            worker.join(timeout=5)
            server.server_close()
            assert not worker.is_alive()


def qualify_ask(binary):
    results = []
    # Small, focused permission scenarios; each runs one real read and one
    # continuation, not a product-wide pair matrix.
    for file_effect, source_effect in [("allow", "ask"), ("ask", "allow"), ("ask", "ask")]:
        for absolute in (False, True):
            with tempfile.TemporaryDirectory(prefix="t45-ask-source-") as temporary, contextlib.ExitStack() as ownership:
                root = Path(temporary)
                project, home, data = root / "project", root / "home", root / "data"
                config = home / "config/opencode"
                config.mkdir(parents=True)
                (project / "nested").mkdir(parents=True)
                (project / "nested/file.txt").write_text("approved first\napproved second\n")
                source = project / "nested/AGENTS.md"
                source.write_text("AUTOMATIC_SOURCE_BODY\n")
                (config / "AGENTS.md").write_text("PINNED_CONFIG_BASELINE\n")
                libc = ctypes.CDLL(None, use_errno=True)
                watch = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
                assert watch >= 0
                ownership.callback(os.close, watch)
                assert libc.inotify_add_watch(watch, os.fsencode(source), 0x20) >= 0
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
                            assert 0 < length <= 65536
                            request = json.loads(self.rfile.read(length))
                            if request.get("max_output_tokens") == 256 and not request.get("tools"):
                                output = []
                            else:
                                requests.append(request)
                                if len(requests) == 1:
                                    output = [{"type": "function_call", "id": "read-ask", "call_id": "read-ask",
                                        "name": "read", "arguments": json.dumps({"path": "nested/file.txt", "limit": 2}), "status": "completed"}]
                                else:
                                    assert len(requests) == 2
                                    developers = [item for item in request["input"] if item.get("role") == "developer"]
                                    def body(item):
                                        content = item.get("content", "")
                                        return content if isinstance(content, str) else "".join(part.get("text", "") for part in content)
                                    assert sum(body(item).endswith("PINNED_CONFIG_BASELINE\n") for item in developers) == 1
                                    present = [item for item in developers if body(item).endswith("AUTOMATIC_SOURCE_BODY\n")]
                                    assert len(present) == (1 if source_effect == "allow" else 0), "unapproved automatic source body reached Developer input"
                                    if present:
                                        metadata = json.loads(body(present[0]).splitlines()[1].removeprefix("<!-- oc-instructions-source: ").removesuffix(" -->"))
                                        assert metadata["path"] == str(source) and metadata["origin"] == "Nested"
                                        assert metadata["digest"] == hashlib.sha256(b"AUTOMATIC_SOURCE_BODY\n").hexdigest()
                                    tool_output = next(item for item in request["input"] if item.get("type") == "function_call_output")
                                    assert "approved first" in tool_output["output"] and "approved second" in tool_output["output"]
                                    output = [{"type": "message", "id": "done", "role": "assistant", "content": [{"type": "output_text", "text": "DONE"}]}]
                            events = []
                            if output and output[0]["type"] == "function_call":
                                events = [{"type": "response.output_item.added", "item": output[0]},
                                          {"type": "response.output_item.done", "item": output[0]}]
                            events.append({"type": "response.completed", "response": {"status": "completed", "output": output}})
                            self.send_response(200)
                            self.send_header("Content-Type", "text/event-stream")
                            self.end_headers()
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
                source_rule = str(source) if absolute else "nested/AGENTS.md"
                configuration = {"model": "fixture/read-model", "compaction": {"auto": False},
                    "agent": {"title": {"disable": True}},
                    "permission": {"read": {"*": "ask", "nested/file.txt": file_effect, source_rule: source_effect}},
                    "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                        "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                        "models": {"read-model": {"limit": {"context": 65536, "output": 2048}}}}}}
                (config / "opencode.json").write_text(json.dumps(configuration))
                environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                    "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}
                process = None
                try:
                    command = [str(binary), "--data-dir", str(data)]
                    if file_effect == "ask":
                        command.append("--auto")  # Existing broker approves the original file once.
                    command.extend(["run", "--json", "--session", "ask-source", "Read fixture"])
                    process = subprocess.Popen(command, cwd=project, env=environment, stdin=subprocess.DEVNULL,
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                    try:
                        stdout, stderr = process.communicate(timeout=20)
                    except subprocess.TimeoutExpired:
                        try:
                            observed_open = bool(os.read(watch, 4096))
                        except BlockingIOError:
                            observed_open = False
                        raise AssertionError({"peer_failures": failures, "source_open": observed_open,
                            "file": file_effect, "source": source_effect, "form": "absolute" if absolute else "relative"}) from None
                    assert not failures, failures
                    assert process.returncode == 0, (process.returncode, stderr[-1024:].decode())
                    assert len(requests) == 2 and len(stdout) < 65536 and len(stderr) < 65536
                    with sqlite3.connect(data / "oc.sqlite") as database:
                        assert database.execute("SELECT state FROM tool_operations WHERE name='read'").fetchall() == [("completed",)]
                        assert database.execute("SELECT status FROM turns WHERE session_id='ask-source'").fetchall() == [("completed",)]
                        facts = [json.loads(row[0]) for row in database.execute("SELECT payload FROM events WHERE kind='instructions_updated'")]
                        nested = [fact for fact in facts if fact["source"]["origin"] == "Nested"]
                        assert len(nested) == (1 if source_effect == "allow" else 0)
                    try:
                        opened = bool(os.read(watch, 4096))
                    except BlockingIOError:
                        opened = False
                    assert opened == (source_effect == "allow"), "automatic source open differed from permanent authority"
                    results.append({"file": file_effect, "source": source_effect, "form": "absolute" if absolute else "relative",
                                    "status": "PASS", "source_open": opened, "read_state": "completed", "requests": 2})
                finally:
                    if process is not None and process.poll() is None:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
                    server.shutdown()
                    worker.join(timeout=5)
                    server.server_close()
                    assert not worker.is_alive()
    return {"status": "PASS", "ask_scenarios": results, "owned_cleanup": "joined"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--ask-only", action="store_true")
    arguments = parser.parse_args()
    binary = arguments.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    try:
        result = qualify_ask(binary) if arguments.ask_only else qualify(binary)
    except Exception as error:
        result = {"status": "FAIL", "error": str(error), "owned_cleanup": "joined"}
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == digest
    print(json.dumps({**result, "elf_sha256": digest}))
    raise SystemExit(0 if result["status"] == "PASS" else 1)


if __name__ == "__main__":
    main()
