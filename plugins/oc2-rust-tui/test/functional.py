#!/usr/bin/env python3
"""Actual plugin/Rust UI against the pinned candidate and a loopback provider.

All processes and data belong to a fresh UID/PID/network namespace. This is
synthetic executable evidence, not a user's live service or visual qualification.
"""
import argparse
import ctypes
import fcntl
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from urllib.parse import urlencode


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--opencode", type=Path, required=True)
    parser.add_argument("--oc2", type=Path, required=True)
    parser.add_argument("--screen", type=Path, required=True)
    parser.add_argument("--inner", action="store_true")
    parser.add_argument("--network", action="store_true")
    parser.add_argument("--case", choices=["basic", "tools", "selection", "history", "controls", "unsupported", "parent-loss", "lifecycle"], default="basic")
    args = parser.parse_args()
    if not args.inner and not args.network:
        # bwrap's nested user namespace cannot administer its outer network
        # namespace. Configure lo while owning that namespace, then let bwrap
        # drop capabilities before it launches the isolated product fixture.
        return subprocess.call([
            "unshare", "--user", "--map-current-user", "--net", "--keep-caps",
            sys.executable, "-I", str(Path(__file__).resolve()), "--network",
            "--opencode", str(args.opencode.resolve()), "--oc2", str(args.oc2.resolve()), "--screen", str(args.screen.resolve()), "--case", args.case,
        ], env={"PATH": "/usr/bin:/bin", "TMPDIR": os.environ.get("TMPDIR", "/home/opencode/.cache/opencode-tmp/opencode")})
    if not args.inner:
        assert os.getuid() != 0
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as interface:
            fcntl.ioctl(interface, 0x8914, struct.pack("16sH22x", b"lo", 1))
        assert ctypes.CDLL(None, use_errno=True).capset(ctypes.create_string_buffer(struct.pack("II", 0x20080522, 0)), ctypes.create_string_buffer(bytes(24))) == 0
        root = Path(tempfile.mkdtemp(prefix="oc2-functional-", dir=os.environ.get("TMPDIR")))
        print(f"artifacts: {root}", flush=True)
        return subprocess.call([
            "bwrap", "--unshare-user", "--uid", str(os.getuid()), "--gid", str(os.getgid()),
            "--unshare-pid", "--die-with-parent", "--ro-bind", "/", "/",
            "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/home", "--tmpfs", "/run",
            "--tmpfs", "/opt", "--tmpfs", "/tmp", "--bind", str(root), "/tmp/work",
            "--ro-bind", str(args.opencode.resolve()), "/opt/opencode",
            "--ro-bind", str(args.oc2.resolve()), "/opt/oc2",
            "--ro-bind", str(args.screen.resolve()), "/opt/screen",
            "--ro-bind", str(Path(__file__).resolve()), "/opt/functional.py",
            "--ro-bind", str(Path(__file__).resolve().parents[1]), "/opt/plugin",
            "--chdir", "/tmp/work", "--", "/usr/bin/python3", "-I", "/opt/functional.py",
            "--inner", "--opencode", "/opt/opencode", "--oc2", "/opt/oc2", "--screen", "/opt/screen", "--case", args.case,
        ], env={"PATH": "/usr/bin:/bin"})

    assert os.getuid() != 0
    assert all(int(line.split()[1], 16) == 0 for line in Path("/proc/self/status").read_text().splitlines() if line.startswith(("CapEff:", "CapPrm:", "CapAmb:")))
    links = subprocess.check_output(["/usr/sbin/ip", "-o", "link"], text=True).splitlines()
    assert len(links) == 1 and ": lo:" in links[0], links
    assert not subprocess.check_output(["/usr/sbin/ip", "route", "show"])
    assert not subprocess.check_output(["/usr/sbin/ip", "-6", "route", "show", "default"])
    root = Path("/tmp/work")
    requests = []
    provider_errors = []
    release = threading.Event()

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            assert self.path == "/v1/chat/completions", self.path
            size = int(self.headers["Content-Length"])
            assert size <= 8 * 1024 * 1024
            body = json.loads(self.rfile.read(size))
            # Only synthetic request bodies; never headers or credentials.
            requests.append(body)
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            def emit(delta, finish=None):
                self.wfile.write(("data: " + json.dumps({"choices": [{"delta": delta, "finish_reason": finish}], "usage": None}) + "\n\n").encode())
                self.wfile.flush()
            def content(value):
                if isinstance(value, str):
                    return value
                return "".join(part.get("text", "") for part in value if part.get("type") == "text")
            users = [(index, content(value.get("content", ""))) for index, value in enumerate(body["messages"]) if value["role"] == "user"]
            index, current = users[-1]
            tools = any(value["role"] == "tool" for value in body["messages"][index + 1:])
            try:
                emit({"role": "assistant"})
                # These are exact synthetic user markers, not model-output parsing.
                if current in ["fixture-read", "fixture-question"] and not tools:
                    name = "read" if current == "fixture-read" else "question"
                    arguments = {"path": "notes.txt"} if name == "read" else {"questions": [{"header": "Choice", "question": "Choose one", "options": [{"label": "A", "description": "First"}, {"label": "B", "description": "Second"}]}]}
                    emit({"tool_calls": [{"index": 0, "id": f"call_{name}", "type": "function", "function": {"name": name, "arguments": ""}}]})
                    emit({"tool_calls": [{"index": 0, "function": {"arguments": json.dumps(arguments)}}]})
                    emit({}, "tool_calls")
                else:
                    if current.startswith("Here is the first message of the conversation:"):
                        answer = "Second fixture" if "fixture-second" in current else "Primary fixture"
                    elif current == "fixture-read":
                        answer = "Fixture read complete."
                    elif current == "fixture-question":
                        answer = "Fixture question complete."
                    elif current == "fixture-hold":
                        answer = "Fixture hold."
                    else:
                        answer = "Fixture answer."
                    emit({"reasoning_content": "Fixture reasoning."})
                    emit({"content": answer})
                    if current == "fixture-hold":
                        while not release.wait(0.05):
                            self.wfile.write(b": heartbeat\n\n")
                            self.wfile.flush()
                    emit({}, "stop")
                self.wfile.write(b"data: [DONE]\n\n")
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass  # Expected only when the explicit interrupt closes its stream.
            except Exception as error:
                provider_errors.append(str(error))

    provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=provider.serve_forever, daemon=True).start()
    for name in ["home", "config", "data", "cache", "state", "runtime", "project"]:
        (root / name).mkdir(mode=0o700, exist_ok=True)
    model = {"name": "First Fixture Model", "capabilities": {"tools": True, "input": ["text"], "output": ["text"]}, "cost": {"input": 0, "output": 0}, "limit": {"context": 100000, "output": 10000}}
    config = {
        "$schema": "https://opencode.ai/config.json", "update": "disable",
        "plugins": ["/opt/plugin"], "model": "test/test-model", "default_agent": "build",
        "providers": {"test": {"name": "Fixture", "package": "aisdk:@ai-sdk/openai-compatible", "settings": {"apiKey": "fixture-only", "baseURL": f"http://127.0.0.1:{provider.server_port}/v1"}, "models": {"test-model": model, "second-model": {**model, "name": "Second Fixture Model"}}}},
        "permissions": [{"action": "read", "resource": "*", "effect": "ask"}],
    }
    (root / "project/opencode.json").write_text(json.dumps(config))
    (root / "project/notes.txt").write_text("Saved fixture file.\n")
    env = {
        "PATH": "/opt:/usr/bin:/bin", "HOME": str(root / "home"), "TERM": "xterm-256color",
        "TMPDIR": "/tmp", "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_STATE_HOME": str(root / "state"), "XDG_RUNTIME_DIR": str(root / "runtime"),
        "OPENCODE_CONFIG_DIR": str(root / "config"), "OPENCODE_DISABLE_MODELS_FETCH": "true",
        "OPENCODE_DISABLE_AUTOUPDATE": "true",
    }
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))
    original = termios.tcgetattr(slave)
    process = subprocess.Popen([str(args.opencode)], cwd=root / "project", env=env,
                               stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    capture = bytearray()
    result = {}

    def pump():
        if select.select([master], [], [], 0.02)[0]:
            data = os.read(master, 65536)
            capture.extend(data)
            assert len(capture) <= 16 * 1024 * 1024, "PTY output limit"
            if b"\x1b[6n" in data:
                os.write(master, b"\x1b[1;1R")

    def until(predicate, description, seconds=15):
        deadline = time.monotonic() + seconds
        while not predicate():
            assert process.poll() is None, f"host exited during {description}"
            assert time.monotonic() < deadline, f"timeout: {description}"
            pump()

    def children():
        path = Path(f"/proc/{process.pid}/task/{process.pid}/children")
        return [int(pid) for pid in path.read_text().split()] if path.exists() else []

    def rust_children():
        return [pid for pid in children() if Path(f"/proc/{pid}/cmdline").exists() and b"__opencode-tui" in Path(f"/proc/{pid}/cmdline").read_bytes()]

    def api(method, path, data=None):
        command = [str(args.opencode), "api", method, path]
        if data is not None:
            command += ["--data", json.dumps(data)]
        reply = subprocess.run(command, cwd=root / "project", env=env, capture_output=True, timeout=15)
        if reply.returncode:
            (root / "api-error.txt").write_bytes(reply.stderr)
            raise AssertionError(f"fixture API failed: {method} {path}")
        return json.loads(reply.stdout) if reply.stdout.strip() else None

    def messages(session_id):
        return api("get", f"/api/session/{session_id}/message?limit=200&order=desc")["data"]

    def screen():
        return subprocess.check_output([str(args.screen)], input=bytes(capture), timeout=2).decode()

    def active(session_id):
        return session_id in api("get", "/api/session/active")["data"]

    def prompt(value, answer):
        before = len(capture)
        os.write(master, value.encode() + b"\r")
        until(lambda: answer in capture[before:], answer.decode())

    def open_session(title):
        os.write(master, b"/sessions\r")
        def picker():
            value = screen()
            return "Sessions" in value and "delete ctrl+d" in value
        until(picker, "session picker painted")
        os.write(master, b"\x7f" * 80)
        until(lambda: "Search" in screen(), "session filter cleared")
        os.write(master, title.encode())
        until(lambda: screen().count(title) >= 2 and "No sessions" not in screen(), "exact filtered session row")
        os.write(master, b"\r")
        until(lambda: not picker() and screen().count(title) >= 2, "session route painted")

    def create_empty(title):
        return api("post", "/api/session", {"title": title, "location": {"directory": str(root / "project")}})["data"]

    try:
        session_id = None
        until(lambda: b"local tabs" in capture, "actual Rust home frame")
        pids = rust_children()
        assert len(pids) == 1, pids
        location_query = urlencode({"location[directory]": str(root / "project")})
        configured = api("get", f"/api/model/default?{location_query}")["data"]
        assert configured and configured["providerID"] == "test" and configured["id"] == "test-model", configured
        if args.case == "lifecycle":
            def stock_keyboard():
                before = len(capture)
                os.write(master, b"\x10")
                until(lambda: b"Commands" in capture[before:], "stock UI keyboard after cleanup")
                os.write(master, b"\x1b")
                pump()

            def enable():
                before = len(capture)
                (root / "project" / "opencode.json").write_text(json.dumps(config))
                until(lambda: len(rust_children()) == 1 and b"local tabs" in capture[before:], "one re-enabled Rust UI")
                pids.extend(rust_children())

            os.kill(pids[-1], signal.SIGKILL)
            until(lambda: not rust_children(), "crashed Rust child joined")
            stock_keyboard()
            disabled = {**config, "plugins": []}
            (root / "project" / "opencode.json").write_text(json.dumps(disabled))
            # Observe the effective server inventory, not a guessed watcher delay.
            until(lambda: not any(plugin["id"] == "oc2-rust-tui" for plugin in api("get", f"/api/plugin?{location_query}")["data"]), "plugin disabled after crash")
            enable()
            (root / "project" / "opencode.json").write_text(json.dumps(disabled))
            until(lambda: not rust_children(), "active Rust UI disabled and joined")
            stock_keyboard()
            enable()
            process.send_signal(signal.SIGHUP)
            assert not requests
        elif args.case == "history":
            template = create_empty("Import template")
            session_id = "ses_fixture_history"
            api("post", "/api/experimental/session/import", {"location": {"directory": str(root / "project")}, "info": {**template, "id": session_id, "title": "Paged fixture"}, "messages": [{"id": f"msg_fixture_history_{index:03}", "type": "user", "text": f"history-{index:03}", "time": {"created": index + 1}} for index in range(80)]})
            open_session("Paged fixture")
            until(lambda: "history-079" in screen(), "newest history page")
            # PageUp/Down are prompt no-ops; Up/Down recall prompt history.
            # Wheel input scrolls the real transcript without editing it.
            for _ in range(160):
                os.write(master, b"\x1b[<64;60;20M")
                pump()
                if "history-000" in screen():
                    break
            until(lambda: "history-000" in screen(), "older opaque-cursor page")
            for _ in range(160):
                os.write(master, b"\x1b[<65;60;20M")
                pump()
                if "history-079" in screen():
                    break
            until(lambda: "history-079" in screen(), "newer opaque-cursor page")
            assert not requests
        elif args.case == "unsupported":
            session_id = create_empty("Unsupported fixture")["id"]
            open_session("Unsupported fixture")
            before = len(capture)
            until(lambda: b"Unsupported fixture" in capture[before:] or b"Unsupported fixture" in capture, "real session route")
            detail = api("post", f"/api/session/{session_id}/form", {"title": "Unsupported number", "fields": [{"key": "q0", "type": "number", "description": "A number"}]})["data"]
            until(lambda: not rust_children(), "unsupported form returns stock")
            assert api("get", f"/api/session/{session_id}/form/{detail['id']}")["data"]["state"]["status"] == "pending"
            assert not requests
        else:
            first = "fixture-read" if args.case == "tools" else "fixture-hold" if args.case in ["controls", "parent-loss"] else "fixture-basic"
            if args.case == "tools":
                os.write(master, first.encode() + b"\r")
                until(lambda: b"Allow once" in capture, "real read permission")
                os.write(master, b"\r")
                until(lambda: b"Fixture read complete." in capture, "read tool result")
            else:
                prompt(first, b"Fixture hold." if first == "fixture-hold" else b"Fixture answer.")
            session_id = api("get", "/api/session")["data"][0]["id"]
            until(lambda: any(value["type"] == "user" and value["text"] == first for value in messages(session_id)), "same saved user identity")
            if args.case == "basic":
                until(lambda: any(value["type"] == "assistant" and any(part.get("text") == "Fixture answer." for part in value["content"]) for value in messages(session_id)), "settled saved assistant")
                assert any(part["type"] == "reasoning" and part["text"] == "Fixture reasoning." for value in messages(session_id) if value["type"] == "assistant" for part in value["content"])
                before = len(capture)
                os.write(master, b"/thinking-mode\r")
                until(lambda: b"Fixture reasoning." in capture[before:], "Rust reasoning presentation")
                saved = messages(session_id)
                before = len(capture)
                os.write(master, b"/close-tab\r")
                until(lambda: "█▀▀█".encode() in capture[before:], "close only local tab and render Home")
                assert len(api("get", "/api/session")["data"]) == 1
                open_session("Primary fixture")
                until(lambda: "Fixture answer." in screen(), "reopened settled Rust history")
                assert messages(session_id) == saved, "reopen changed durable history"
            elif args.case == "tools":
                assert any(part["type"] == "tool" and part["id"] == "call_read" and part["state"]["status"] == "completed" for value in messages(session_id) if value["type"] == "assistant" for part in value["content"])
                os.write(master, b"fixture-question\r")
                until(lambda: b"Choose one" in capture, "question-compatible form")
                form_id = api("get", f"/api/session/{session_id}/form")["data"][0]["id"]
                os.write(master, b"2")
                until(lambda: b"Fixture question complete." in capture, "question tool settled")
                assert api("get", f"/api/session/{session_id}/form/{form_id}")["data"]["state"]["answer"] == {"q0": "B"}
                assert any(part["type"] == "tool" and part["id"] == "call_question" and part["state"]["status"] == "completed" for value in messages(session_id) if value["type"] == "assistant" for part in value["content"])
                template = api("get", f"/api/session/{session_id}")["data"]
                child_id = "ses_fixture_child"
                api("post", "/api/experimental/session/import", {"location": {"directory": str(root / "project")}, "info": {**template, "id": child_id, "parentID": session_id, "title": "Child fixture"}, "messages": []})
                child_info = api("get", f"/api/session/{child_id}")["data"]
                assert child_info["parentID"] == session_id and child_info["location"]["directory"] == str(root / "project")
                child_form = api("post", f"/api/session/{child_id}/form", {"title": "Child choice", "metadata": {"kind": "question"}, "fields": [{"key": "q0", "type": "string", "description": "Choose for child", "custom": True, "options": [{"value": "A", "label": "A"}, {"value": "B", "label": "B"}]}]})["data"]["id"]
                before = len(capture)
                until(lambda: b"Choose for child" in capture[before:], "child-session form routed to real root family")
                os.write(master, b"1")
                until(lambda: api("get", f"/api/session/{child_id}/form/{child_form}")["data"]["state"]["status"] == "answered", "exact child form reply")
                assert api("get", f"/api/session/{child_id}/form/{child_form}")["data"]["state"]["answer"] == {"q0": "A"}
                pending_id = create_empty("Pending fixture")["id"]
                pending_form = api("post", f"/api/session/{pending_id}/form", {"title": "Pending choice", "metadata": {"kind": "question"}, "fields": [{"key": f"q{i}", "type": "string", "description": label, "custom": True, "options": [{"value": "A", "label": "A"}, {"value": "B", "label": "B"}]} for i, label in enumerate(["Choose before opening", "Choose after refresh"])]})["data"]["id"]
                open_session("Pending fixture")
                until(lambda: "Choose before opening" in screen(), "already-pending question survives initial open")
                os.write(master, b"2")
                until(lambda: "Choose after refresh" in screen(), "first question draft retained")
                # Revisit the retained neighboring view, whose empty question
                # list must not clear this view's partially answered draft.
                os.write(master, b"\x1b[<0;8;1M\x1b[<0;8;1m")
                until(lambda: "Choose after refresh" not in screen() and "Fixture question complete." in screen(), "neighboring tab refreshed")
                assert api("get", f"/api/session/{pending_id}/form/{pending_form}")["data"]["state"]["status"] == "pending"
                open_session("Pending fixture")
                until(lambda: "Choose after refresh" in screen(), "question draft survives neighboring view")
                os.write(master, b"1\r")
                until(lambda: api("get", f"/api/session/{pending_id}/form/{pending_form}")["data"]["state"]["status"] == "answered", "pending form answered after reopen")
                assert api("get", f"/api/session/{pending_id}/form/{pending_form}")["data"]["state"]["answer"] == {"q0": "B", "q1": "A"}
            elif args.case == "selection":
                before = len(capture)
                os.write(master, b"/model\r")
                until(lambda: b"Second Fixture Model" in capture[before:], "model catalog")
                os.write(master, b"Second Fixture Model")
                pump()
                variant = len(capture)
                os.write(master, b"\r")
                until(lambda: b"Default" in capture[variant:], "model variant draft")
                assert api("get", f"/api/session/{session_id}")["data"]["model"]["id"] == "test-model", "picker draft committed"
                variant = len(capture)
                os.write(master, b"\r")
                until(lambda: b"Second Fixture Model" in capture[variant:], "composer model draft")
                assert api("get", f"/api/session/{session_id}")["data"]["model"]["id"] == "test-model", "variant draft committed"
                os.write(master, b"\r")
                until(lambda: api("get", f"/api/session/{session_id}")["data"]["model"]["id"] == "second-model", "separate model commit")
                before = len(capture)
                os.write(master, b"/agents\r")
                until(lambda: b"plan" in capture[before:], "agent catalog")
                original_agent = api("get", f"/api/session/{session_id}")["data"].get("agent")
                os.write(master, b"plan")
                pump()
                os.write(master, b"\r")
                assert api("get", f"/api/session/{session_id}")["data"].get("agent") == original_agent, "agent draft committed"
                os.write(master, b"\r")
                until(lambda: api("get", f"/api/session/{session_id}")["data"]["agent"] == "plan", "separate agent commit")
                os.write(master, b"/new\r")
                api("get", "/api/session")
                pump()
                assert len(api("get", "/api/session")["data"]) == 1, "blank tab created backend session"
                prompt("fixture-second", b"Fixture answer.")
                until(lambda: len(api("get", "/api/session")["data"]) == 2, "second real session")
                second = next(value for value in api("get", "/api/session")["data"] if value["id"] != session_id)
                assert api("get", f"/api/session/{second['id']}")["data"].get("agent") == "plan", "new-session composer agent was not captured"
                open_session("Primary fixture")
                foreign = root / "foreign"
                foreign.mkdir()
                foreign_config = {**config, "model": "test/foreign-model", "default_agent": "plan", "providers": {"test": {**config["providers"]["test"], "models": {"foreign-model": {**model, "name": "Foreign Fixture Model"}}}}}
                (foreign / "opencode.json").write_text(json.dumps(foreign_config))
                foreign_id = api("post", "/api/session", {"title": "Foreign fixture", "location": {"directory": str(foreign)}})["data"]["id"]
                open_session("Foreign fixture")
                before = len(capture)
                os.write(master, b"/model\r")
                until(lambda: b"Foreign Fixture Model" in capture[before:], "catalog follows opened session location")
                assert api("get", f"/api/session/{foreign_id}")["data"]["location"]["directory"] == str(foreign)
                os.write(master, b"\x1b")
                until(lambda: "Select model" not in screen(), "foreign model picker dismissed")
            elif args.case in ["controls", "parent-loss"]:
                assert active(session_id)
                if args.case == "parent-loss":
                    process.kill()
                    process.wait()
                    deadline = time.monotonic() + 5
                    while any(Path(f"/proc/{pid}").exists() for pid in pids):
                        for pid in pids:
                            try:
                                os.waitpid(pid, os.WNOHANG)
                            except ChildProcessError:
                                pass
                        assert time.monotonic() < deadline, "Rust child IPC-loss cleanup"
                        pump()
                    assert active(session_id), "viewer loss interrupted backend work"
                    api("post", f"/api/session/{session_id}/interrupt")  # Explicit fixture cleanup.
                else:
                    os.write(master, b"fixture-queued\x18\r")
                    until(lambda: any(item["payload"]["text"] == "fixture-queued" for item in api("get", f"/api/session/{session_id}/inbox")["data"]), "durable queued input")
                    os.write(master, b"\x18q")
                    until(lambda: b"Queued prompts" in capture, "queued input controls")
                    os.write(master, b"\x04")
                    until(lambda: not api("get", f"/api/session/{session_id}/inbox")["data"], "pending input cancelled")
                    assert active(session_id), "pending cancellation interrupted execution"
                    os.write(master, b"\x1b")
                    api("get", "/api/session/active")
                    pump()
                    assert active(session_id), "first Esc bypassed interrupt guard"
                    os.write(master, b"\x1b")
                    until(lambda: not active(session_id), "explicit execution interrupt")
                    assert not any(value["type"] == "user" and value["text"] == "fixture-queued" for value in messages(session_id)), "cancelled input delivered"
        if args.case not in ["parent-loss", "lifecycle"]:
            if rust_children():
                before_return = len(capture)
                os.write(master, b"/exit\r")
                until(lambda: not rust_children(), "Rust child exit")
                if args.case == "basic":
                    until(lambda: b"Fixture answer." in capture[before_return:], "stock UI renders the same settled session")
            if args.case == "unsupported":
                # Stock owns a focused generic form, whose keys intentionally
                # take precedence over the command palette. Edit, do not submit.
                until(lambda: "A number" in screen(), "stock generic form painted")
                os.write(master, b"42")
                until(lambda: "42" in screen(), "stock form keyboard after return")
                assert api("get", f"/api/session/{session_id}/form/{detail['id']}")["data"]["state"]["status"] == "pending"
            else:
                before = len(capture)
                os.write(master, b"\x10")
                until(lambda: b"Commands" in capture[before:], "stock UI keyboard after return")
            process.send_signal(signal.SIGHUP)
        deadline = time.monotonic() + 15
        while process.poll() is None:
            assert time.monotonic() < deadline, "host shutdown"
            pump()
        for _ in range(3):
            pump()
        assert not provider_errors, provider_errors
        result = {"case": args.case, "session": session_id, "provider_requests": len(requests), "rust_child": pids[0], "terminal_restored": termios.tcgetattr(slave) == original, "cursor_shown": capture.rfind(b"\x1b[?25h") > capture.rfind(b"\x1b[?25l")}
        assert result["terminal_restored"] and result["cursor_shown"], result
        assert all(not Path(f"/proc/{pid}").exists() for pid in pids), "owned Rust orphan"
        print(json.dumps(result))
    finally:
        release.set()
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        provider.shutdown()
        provider.server_close()
        (root / "pty.bin").write_bytes(capture)
        provider_log = json.dumps(requests, separators=(",", ":"))
        assert len(provider_log.encode()) <= 16 * 1024 * 1024, "synthetic provider log limit"
        (root / "provider.json").write_text(provider_log)
        (root / "result.json").write_text(json.dumps(result, indent=2))
        os.close(master)
        os.close(slave)
    return 0


if __name__ == "__main__":
    sys.exit(main())
