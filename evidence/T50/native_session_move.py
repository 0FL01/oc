#!/usr/bin/env python3
"""TOOL19 normal ELF, real PTY/DB/files/process barriers, fake Responses only."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import sqlite3
import socket
import subprocess
import threading
import traceback

import native_background as fixture

CASES = ("boundary", "relative", "home", "explicit", "children", "child_move",
         "deny", "ask_cancel", "ask_stale", "missing", "not_directory", "symlink",
         "protected", "untrusted", "bad_generation", "foreign", "unknown",
         "child_parent", "child_sibling", "general", "rollback",
         "busy_child", "ask_generation", "ask_binding", "bad_variant", "bad_model", "bad_agent", "bad_credential", "bad_policy",
         "quarantine", "crash_preintent", "crash_pending", "crash_terminal", "crash_applied", "explicit_tab")


def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def painted_screen(native):
    grid = [[" "] * 110 for _ in range(native.height)]
    row = col = 0
    for part in re.split(r"(\x1b\[[0-9;?]*[ -/]*[@-~])", native.tail.decode("utf8", "replace")):
        if part.startswith("\x1b["):
            if part[-1] in "Hf":
                values = [int(v or "1") for v in part[2:-1].split(";")]
                row, col = (values + [1])[0] - 1, (values + [1])[1] - 1
            elif part.endswith("2J"): grid = [[" "] * 110 for _ in range(native.height)]
            elif part.endswith("K") and 0 <= row < native.height:
                grid[row][max(col, 0):] = [" "] * (110 - max(col, 0))
            continue
        for char in part:
            if char == "\r": col = 0
            elif char == "\n": row += 1
            elif char.isprintable():
                if 0 <= row < native.height and 0 <= col < 110: grid[row][col] = char
                col += 2 if fixture.unicodedata.east_asian_width(char) in "WF" else 1
    return "\n".join("".join(line) for line in grid)


def painted_footer(native):
    return "\n".join(painted_screen(native).splitlines()[-4:])


def explicit_tab(binary):
    pending, finish = threading.Event(), threading.Event()
    def script(owner, count, request):
        assert "opencode_session_move" in {t["name"] for t in request["tools"]}
        if count == 1: return fixture.tool("opencode_session_rename", {"title": "ORIGINAL_R8_TAB"}, "original-title")
        if count == 3:
            return batch([("opencode_session_rename", {"title": "CALLER_R8_TAB"}, "caller-title"),
                ("opencode_session_move", {"directory": str(owner.destination), "sessionID": "t50-background"}, "parked-move")])
        if count == 4:
            result, = [i for i in request["input"] if i.get("call_id") == "parked-move" and i["type"] == "function_call_output"]
            assert json.loads(result["output"])["status"] == "pending"
            pending.set(); assert finish.wait(10)
        return fixture.completed("TAB_DONE")
    with fixture.Native(binary, script, {"*": "allow", "external_directory": "allow"}) as native:
        native.destination = native.root / "destination"; native.destination.mkdir()
        try:
            native.start(); native.send(b"Initial root\r")
            fixture.until(native.settled, "initial real root not terminal")
            fixture.until(native.completed_frame, "initial source footer not reconciled")
            native.tail.clear(); native.send(b"/new\r")
            fixture.until(lambda: b"Ask anything" in native.tail, "real Home/new root not opened")
            native.send(b"Move parked original\r")
            fixture.until(pending.is_set, "explicit parked move not admitted")
            caller, = native.rows("SELECT session_id FROM turns WHERE status='started'")[0]
            assert caller != "t50-background"
            native.send(b"KEEP_DRAFT_R8"); finish.set()
            fixture.until(lambda: native.rows("SELECT phase FROM session_moves") == [("applied",)], "parked target not moved")
            fixture.until(lambda: "CALLER_R8_TAB" in "\n".join(painted_screen(native).splitlines()[:3]) and "ORIGINAL_R8_TAB" not in "\n".join(painted_screen(native).splitlines()[:3]) and "KEEP_DRAFT_R8" in painted_screen(native), "real parked title removal/caller draft not reconciled")
            assert native.project.name in painted_footer(native) and native.destination.name not in painted_footer(native)
            assert native.rows("SELECT value FROM prefs WHERE key=?", ("tui.session_location."+caller,)) == [(str(native.project),)]
            assert native.rows("SELECT value FROM prefs WHERE key='tui.session_location.t50-background'") == [(str(native.destination),)]
            assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(1,)]
            assert not native.errors, native.errors
        finally: finish.set()
        native.stop()
        return {"case":"explicit_tab","status":"PASS","requests":len(native.requests),"moves":1,"applied":1,"operations":native.rows("SELECT count(*) FROM tool_operations")[0][0],"turns":2,"caller_draft_preserved":True,"owned_cleanup":"joined"}


def batch(calls, reasoning=None):
    events = []
    items = []
    for name, args, call in calls:
        items.append({"type": "function_call", "id": call, "call_id": call,
                      "name": name, "arguments": json.dumps(args), "status": "completed"})
        events.extend(fixture.tool(name, args, call)[:-1])
    if reasoning:
        item = {"type": "reasoning", "id": "source-reasoning", "encrypted_content": reasoning, "summary": [], "status": "completed"}
        items.insert(0, item)
        events.insert(0, {"type": "response.output_item.done", "item": item})
    events.append({"type": "response.completed", "response": {"status": "completed", "output": items}})
    return events


def run(binary, case):
    if case == "explicit_tab": return explicit_tab(binary)
    pending, release, terminal, finish = [threading.Event() for _ in range(4)]
    state = {"root": 0, "child": 0, "restart": False}
    child_cases = {"children", "child_move", "child_parent", "child_sibling", "general"}
    success = case in {"boundary", "relative", "home", "explicit", "children", "child_move", "quarantine", "crash_terminal", "crash_applied"}
    schema = {}
    def script(owner, count, request):
        definitions = {t["name"]: t for t in request["tools"]}
        is_child = any(i.get("role") == "user" and "CHILD_R8_ONLY" in json.dumps(i.get("content")) for i in request["input"])
        if is_child:
            state["child"] += 1
            if case == "general":
                assert "opencode_session_move" not in definitions
            else:
                assert definitions["opencode_session_move"]["parameters"]["required"] == ["directory"]
            if state["child"] == 1:
                child, = owner.rows("SELECT id FROM sessions WHERE parent_id='t50-background'")[0]
                if case in {"child_move", "child_parent", "child_sibling", "general"}:
                    args = {"directory": str(owner.destination)}
                    if case == "child_parent": args["sessionID"] = "t50-background"
                    if case == "child_sibling":
                        with sqlite3.connect(owner.data / "oc.sqlite") as db:
                            db.execute("INSERT INTO sessions(id,created_at,parent_id) VALUES('sibling','fixture','t50-background')")
                            db.execute("INSERT INTO prefs(key,value,updated_at) VALUES('tui.session_location.sibling',?,'fixture')", (str(owner.project),))
                        args["sessionID"] = "sibling"
                    return batch([("opencode_session_move", args, "child-move")])
                return fixture.tool("shell", {"command": "pwd > child-cwd"}, "child-cwd")
            return fixture.completed("CHILD_RESULT")
        state["root"] += 1
        step = state["root"]
        assert "bash" not in definitions
        if case == "deny":
            assert "opencode_session_move" not in definitions
        else:
            move = definitions["opencode_session_move"]["parameters"]
            schema.update(move)
            assert move["additionalProperties"] is False and move["required"] == ["directory"]
        if state["restart"]:
            return fixture.completed("RESTART_SAFE")
        if step == 1:
            if case == "crash_preintent":
                pending.set()
                assert release.wait(10), "preintent response barrier not released"
                return fixture.completed("NO_INTENT")
            args = {"directory": str(owner.destination)}
            if case == "relative": args["directory"] = "../destination"
            if case == "home": args["directory"] = "~/destination"
            if case == "explicit": args["sessionID"] = "other"
            if case == "ask_stale": args["sessionID"] = "other"
            if case == "busy_child": args["sessionID"] = "other"
            if case == "foreign": args["sessionID"] = "foreign"
            if case == "unknown": args["sessionID"] = "unknown"
            if case == "missing": args["directory"] = str(owner.root / "missing")
            if case == "not_directory": args["directory"] = str(owner.project / "seed")
            if case == "symlink": args["directory"] = str(owner.root / "linked")
            if case == "protected": args["directory"] = str(owner.data)
            calls = []
            if case == "quarantine":
                name, = [name for name in definitions if "uncertain" in name]
                return batch([("opencode_session_move", args, "move-call"), (name, {}, "remote-unknown")], "R8_SOURCE_OPAQUE")
            if case in child_cases:
                calls.append(("subagent", {"agent": "general" if case == "general" else "helper", "prompt": "CHILD_R8_ONLY", "description": "move child proof"}, "spawn"))
            if success and case not in {"explicit", "child_move", "crash_terminal", "quarantine"}:
                calls.append(("shell", {"command": "printf '%s' $$ > leader; while [ ! -f bg-release ]; do sleep .02; done; pwd > bg-cwd; printf BG_ORIGINAL", "background": True}, "background-source"))
            if case not in {"child_move", "child_parent", "child_sibling", "general"}:
                calls.extend([("opencode_session_move", args, "move-call"),
                    ("shell", {"command": "pwd > source-cwd; printf SOURCE_EFFECT >> source-effect"}, "source-shell"),
                    ("read", {"path": "destination-only"}, "destination-assumption")])
            return batch(calls, "R8_SOURCE_OPAQUE")
        if step == 2:
            if success or case == "crash_pending":
                assert owner.rows("SELECT phase FROM session_moves") == [("pending",)]
                assert owner.rows("SELECT value FROM prefs WHERE key='tui.session_location.t50-background'") == [(str(owner.project),)]
                assert owner.rows("SELECT status FROM turns WHERE session_id='t50-background'")[-1] == ("started",)
                if case != "child_move":
                    out = [i for i in request["input"] if i.get("call_id") == "move-call" and i.get("type") == "function_call_output"]
                    assert len(out) == 1 and json.loads(out[0]["output"])["status"] == "pending"
                assert request["model"] == "m"
                assert request.get("reasoning", {}).get("effort") == "high", "source variant changed before full terminal"
                assert "R8_SOURCE_INSTRUCTIONS" in json.dumps(request["input"])
                assert "R8_SOURCE_OPAQUE" in json.dumps(request["input"])
                assert "R8_DESTINATION_INSTRUCTIONS" not in json.dumps(request["input"])
                pending.set()
                assert release.wait(10), "source request barrier not released"
                return fixture.tool("shell", {"command": "pwd > after-pending-cwd; printf STILL_SOURCE > after-pending-effect"}, "after-pending")
            return fixture.completed("REFUSED_SAFE")
        if step == 3:
            assert request["model"] == "m"
            assert owner.rows("SELECT phase FROM session_moves") == [("pending",)]
            terminal.set()
            assert finish.wait(10), "full source terminal barrier not released"
            return fixture.completed("SOURCE_TERMINAL")
        assert request["model"] == "n", (case, "destination model", request["model"])
        assert request.get("reasoning", {}).get("effort") == "low", "destination variant not admitted"
        wire = json.dumps(request["input"])
        assert "R8_DESTINATION_INSTRUCTIONS" in wire and "R8_SOURCE_INSTRUCTIONS" not in wire
        assert "R8_SOURCE_OPAQUE" not in wire
        assert all(i.get("call_id") in {"destination-shell", "destination-read"} for i in request["input"] if i.get("type") == "function_call_output"), "source tool causality carried into destination"
        if step == 4:
            return batch([("shell", {"command": "pwd > destination-cwd; printf DESTINATION_EFFECT > destination-effect"}, "destination-shell"), ("read", {"path": "destination-only"}, "destination-read")])
        denied = [i for i in request["input"] if i.get("type") == "function_call_output" and i.get("call_id") == "destination-read"]
        assert len(denied) == 1 and "denied" in denied[0]["output"]
        return fixture.completed("DESTINATION_TERMINAL")

    permission = {"shell": "allow", "read": "allow", "subagent": "allow", "external_directory": "deny" if case == "untrusted" else "allow",
                  "opencode_session_move": "deny" if case == "deny" else "ask" if case.startswith("ask_") else "allow"}
    if case == "quarantine": permission["*"] = "allow"
    with fixture.Native(binary, script, permission) as native:
        original_project = native.project
        state["responses_posts"] = 0
        count_lock = threading.Lock()
        wire_bindings = []
        original_post = native.server.RequestHandlerClass.do_POST
        def counted_post(handler):
            with count_lock:
                state["responses_posts"] += 1
                assert state["responses_posts"] <= 64, "fake Responses request bound exhausted"
                destination = handler.path.startswith("/destination/")
                expected_key = "SYNTHETIC_DESTINATION_KEY" if destination else "synthetic"
                wire_bindings.append({"destination": destination, "binding_preserved": handler.headers.get("authorization") == "Bearer " + expected_key})
            original_post(handler)
        native.server.RequestHandlerClass.do_POST = counted_post
        rpc = {"initialize": 0, "tools/list": 0, "tools/call": 0}
        remote, remote_thread = None, None
        if case == "quarantine":
            class Remote(http.server.BaseHTTPRequestHandler):
                def setup(self):
                    super().setup(); self.connection.settimeout(2)
                def log_message(self, *_): pass
                def do_GET(self):
                    self.send_error(405)
                def do_POST(self):
                    size = int(self.headers.get("content-length", "0"))
                    assert 0 < size <= 65536
                    request = json.loads(self.rfile.read(size))
                    method = request["method"]
                    if method not in rpc:
                        assert method == "notifications/initialized"
                        self.send_response(202); self.send_header("Content-Length", "0"); self.end_headers(); return
                    rpc[method] += 1
                    if method == "tools/call":
                        native.root.joinpath("remote-effect").write_text("exactly one uncertain remote effect")
                        self.connection.shutdown(socket.SHUT_RDWR); self.connection.close(); return
                    result = {"protocolVersion": request["params"]["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "R8Fake", "version": "1"}} if method == "initialize" else {"tools": [{"name": "uncertain", "inputSchema": {"type": "object", "additionalProperties": False}}]}
                    body = json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}).encode()
                    self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
            remote = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Remote)
            remote.daemon_threads = False
            remote_thread = threading.Thread(target=remote.serve_forever)
            remote_thread.start()
        native.destination = native.home / "destination" if case == "home" else native.root / "destination"
        native.destination.mkdir()
        native.destination.joinpath("destination-only").write_text("destination fixture")
        native.project.joinpath("AGENTS.md").write_text("R8_SOURCE_INSTRUCTIONS")
        native.destination.joinpath("AGENTS.md").write_text("R8_DESTINATION_INSTRUCTIONS")
        config = native.home / "config/opencode/opencode.json"
        value = json.loads(config.read_text())
        value["provider"]["fixture"]["models"]["m"]["variants"] = {"source": {"reasoningEffort": "high"}}
        value["provider"]["fixture"]["models"]["n"] = {"limit": {"context": 65536, "output": 2048}, "variants": {"destination": {"reasoningEffort": "low"}}}
        value["agent"]["build"] = {"variant": "source"}
        value["agent"]["helper"] = {"mode": "subagent", "permission": {"opencode_session_move": "allow", "shell": "allow"}}
        value["agent"]["general"] = {"mode": "subagent"}
        if remote:
            value["mcp"] = {"peer": {"type": "remote", "url": f"http://127.0.0.1:{remote.server_port}/mcp", "oauth": False, "timeout": {"startup": 1000, "catalog": 1000, "execution": 1000}}}
        config.write_text(json.dumps(value))
        native.destination.joinpath("opencode.json").write_text(json.dumps({"model": "destination/n", "agent": {"build": {"variant": "destination"}},
            "provider": {"destination": {"npm": "@ai-sdk/openai", "options": {"baseURL": f"http://127.0.0.1:{native.server.server_port}/destination/v1", "apiKey": "SYNTHETIC_DESTINATION_KEY"}, "models": {"n": value["provider"]["fixture"]["models"]["n"]}}},
            "permission": {"read": {"*": "allow", "destination-only": "deny"}}}) if case != "bad_generation" else '{"model":12}')
        if case in {"bad_variant", "bad_model", "bad_agent", "bad_credential", "bad_policy"}:
            path = native.destination / "opencode.json"
            bad = json.loads(path.read_text())
            if case == "bad_variant": bad["agent"]["build"]["variant"] = "retired"
            if case == "bad_model": bad["model"] = "destination/retired"
            if case == "bad_agent": bad["default_agent"] = "retired"
            if case == "bad_credential": bad["provider"]["destination"]["options"]["apiKey"] = ""
            if case == "bad_policy": bad["permission"]["read"] = 12
            path.write_text(json.dumps(bad))
        os.symlink(native.destination, native.root / "linked")
        try:
            native.start()
            if remote: fixture.until(lambda: rpc["tools/list"] == 1, "admitted fake MCP did not initialize")
            with sqlite3.connect(native.data / "oc.sqlite") as db:
                for target, location in (("other", native.project), ("foreign", native.root / "foreign")):
                    db.execute("INSERT INTO sessions(id,created_at) VALUES(?,'fixture')", (target,))
                    db.execute("INSERT INTO prefs(key,value,updated_at) VALUES(?,?,'fixture')", ("tui.session_location." + target, str(location)))
                if case == "rollback":
                    db.execute("CREATE TRIGGER fail_move BEFORE INSERT ON events WHEN NEW.kind='session_move_admitted' BEGIN SELECT RAISE(ABORT,'private fixture'); END")
                if case == "crash_terminal":
                    db.execute("CREATE TRIGGER fail_apply BEFORE INSERT ON events WHEN NEW.kind='session_moved' BEGIN SELECT RAISE(ABORT,'private fixture'); END")
                if case == "busy_child":
                    db.execute("INSERT INTO sessions(id,created_at,parent_id) VALUES('busy-child','fixture','other')")
                    db.execute("INSERT INTO turns(id,session_id,status,prompt) VALUES('busy-turn','busy-child','started','held')")
            native.send(b"Move source prompt\r")
            if case.startswith("ask_"):
                fixture.until(lambda: b"opencode_session_move" in native.tail and b"Allow once" in native.tail, "move approval not painted")
                assert native.rows("SELECT count(*) FROM session_moves") == [(0,)]
                if case == "ask_stale":
                    with sqlite3.connect(native.data / "oc.sqlite") as db:
                        db.execute("UPDATE prefs SET value=? WHERE key='tui.session_location.other'", (str(native.root / "foreign"),))
                    native.send(b"\r")
                elif case == "ask_generation":
                    native.destination.joinpath("AGENTS.md").write_text("changed during approval")
                    native.send(b"\r")
                elif case == "ask_binding":
                    path = native.destination / "opencode.json"
                    changed = json.loads(path.read_text())
                    changed["provider"]["destination"]["options"]["apiKey"] = "SYNTHETIC_CHANGED_BINDING"
                    path.write_text(json.dumps(changed))
                    native.send(b"\r")
                else:
                    native.send(b"\x1b")
            if case == "quarantine":
                fixture.until(lambda: native.rows("SELECT phase FROM session_moves") == [("applied",)], "quarantined source terminal move not applied")
                assert native.rows("SELECT status FROM turns WHERE session_id='t50-background'") == [("failed",)]
                native.send(b"Destination cannot retry uncertain remote\r")
                fixture.until(lambda: b"unsafe_retry" in native.tail or b"retry unsafe" in native.tail or b"Restart" in native.tail, "destination unsafe quarantine not visible")
                assert rpc == {"initialize": 1, "tools/list": 1, "tools/call": 1}, rpc
                assert len(native.requests) == 1, "unsafe destination created a provider retry"
                assert native.rows("SELECT state FROM tool_operations WHERE id LIKE '%remote-unknown'") == [("unknown",)]
                assert native.rows("SELECT value FROM prefs WHERE key='tui.session_location.t50-background'") == [(str(native.destination),)]
                assert native.root.joinpath("remote-effect").read_text() == "exactly one uncertain remote effect"
            elif success or case in {"crash_pending", "crash_preintent"}:
                assert pending.wait(5), "pending source boundary not observed: " + repr(native.errors)
                if case != "crash_preintent":
                    assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(0,)]
                    assert not native.destination.joinpath("source-effect").exists()
                if case in {"crash_preintent", "crash_pending"}:
                    before_requests = len(native.requests)
                    native.stop(crash=True)
                    release.set()
                    state["restart"] = True
                    fixture.until(lambda: not native.server._threads or all(not t.is_alive() for t in native.server._threads), "crashed HTTP worker did not settle")
                    native.start()
                    assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(0,)]
                    assert native.rows("SELECT value FROM prefs WHERE key='tui.session_location.t50-background'") == [(str(native.project),)]
                    assert native.rows("SELECT count(*) FROM tool_operations WHERE state='started'") == [(0,)]
                    assert len(native.requests) == before_requests, "restart automatically dispatched the old turn"
                    if case == "crash_pending":
                        assert native.project.joinpath("source-effect").read_text() == "SOURCE_EFFECT"
                        assert not native.project.joinpath("after-pending-effect").exists()
                else:
                    release.set()
                    assert terminal.wait(5), "source full terminal boundary not observed"
                    assert native.rows("SELECT phase FROM session_moves") == [("pending",)]
                    assert native.project.joinpath("after-pending-cwd").read_text().strip() == str(native.project)
                    finish.set()
                    if case == "crash_terminal":
                        fixture.until(lambda: native.process.poll() is not None, "failed placement was hidden")
                        native.stop(expected_exit=1)
                        assert native.rows("SELECT phase FROM session_moves") == [("pending",)]
                        assert native.rows("SELECT status FROM turns WHERE session_id='t50-background'") == [("completed",)]
                        with sqlite3.connect(native.data / "oc.sqlite") as db:
                            db.execute("DROP TRIGGER fail_apply")
                        state["restart"] = True
                        native.start()
                    fixture.until(lambda: native.rows("SELECT phase FROM session_moves") == [("applied",)], "move was not durably applied")
                    if case == "crash_terminal":
                        assert len(native.requests) == 3, "known-terminal recovery automatically replayed the old turn"
                        assert original_project.joinpath("source-effect").read_text() == "SOURCE_EFFECT"
                        native.stop()
                        native.project = native.destination
                        native.start()
                        native.send(b"Reopen recovered destination\r")
                        fixture.until(lambda: len(native.requests) == 4 and native.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='completed'") == [(2,)], "recovered same-ID destination not reopened")
                        assert native.requests[-1]["model"] == "n" and native.requests[-1].get("reasoning",{}).get("effort") == "low"
                        assert "R8_DESTINATION_INSTRUCTIONS" in json.dumps(native.requests[-1]["input"])
                    if case not in {"explicit", "child_move", "crash_terminal", "quarantine"}:
                        native.project.joinpath("bg-release").write_text("release")
                        fixture.until(lambda: native.project.joinpath("bg-cwd").exists(), "old background did not complete after move")
                        assert native.project.joinpath("bg-cwd").read_text().strip() == str(native.project)
                        native.assert_notice("completed")
                    if case in {"boundary", "relative", "home", "children", "crash_applied"}:
                        fixture.until(lambda: native.destination.name in painted_footer(native), "actual destination footer placement not painted")
                        if case == "crash_applied":
                            native.stop()
                            native.project = native.destination
                            state["restart"] = True
                            native.start()
                            assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(1,)]
                        else:
                            native.send(b"Fresh destination prompt\r")
                            fixture.until(lambda: native.destination.joinpath("destination-effect").exists(), "new destination turn not executed")
                            fixture.until(lambda: native.settled(2), "new destination turn not terminal")
                            assert native.destination.joinpath("destination-cwd").read_text().strip() == str(native.destination)
                    if case == "children":
                        assert native.rows("SELECT p.value FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id WHERE s.parent_id='t50-background'") == [(str(native.project),)]
                        assert native.project.joinpath("child-cwd").read_text().strip() == str(native.project)
                    if case == "explicit":
                        assert native.rows("SELECT value FROM prefs WHERE key='tui.session_location.t50-background'") == [(str(native.project),)]
                        native.stop()
                        process = subprocess.Popen([str(binary), "--data-dir", str(native.data), "run", "--json", "--session", "other", "Fresh explicit destination"], cwd=native.destination, env=native.env,
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                        try:
                            stdout, stderr = process.communicate(timeout=15)
                            assert process.returncode == 0, stderr.decode()[-512:]
                            assert len(stdout) + len(stderr) <= 1048576
                        finally:
                            if process.poll() is None:
                                os.killpg(process.pid, fixture.signal.SIGKILL); process.wait(timeout=3)
                        assert native.destination.joinpath("destination-cwd").read_text().strip() == str(native.destination)
                    if case == "quarantine":
                        native.send(b"Destination cannot retry uncertain remote\r")
                        fixture.until(lambda: b"unsafe_retry" in native.tail or b"retry unsafe" in native.tail or b"Restart" in native.tail, "destination unsafe quarantine not visible")
                        assert rpc == {"initialize": 1, "tools/list": 1, "tools/call": 1}, rpc
                        assert len(native.requests) == 3, "unsafe destination created a provider retry"
                        assert native.rows("SELECT state FROM tool_operations WHERE id LIKE '%remote-unknown'") == [("unknown",)]
                    target = "other" if case == "explicit" else native.rows("SELECT id FROM sessions WHERE parent_id='t50-background'")[0][0] if case == "child_move" else "t50-background"
                    assert native.rows("SELECT value FROM prefs WHERE key=?", ("tui.session_location." + target,)) == [(str(native.destination),)]
                    assert native.rows("SELECT count(*) FROM events WHERE kind='session_move_admitted'") == [(1,)]
                    assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(1,)]
                    record = native.rows("SELECT operation_id,session_id,source_turn FROM session_moves")[0]
                    original = native.rows("SELECT state,output,turn_id FROM tool_operations WHERE id=?", (record[0],))[0]
                    assert original[0] == "completed" and original[2] == record[2]
                    result = json.loads(original[1])
                    assert result["operationID"] == record[0] and result["sessionID"] == record[1] and result["status"] == "pending"
                    boundary = native.rows("SELECT seq FROM events WHERE kind='turn_finished' AND payload=?", (record[2],))[0][0]
                    applied = native.rows("SELECT seq FROM events WHERE kind='session_moved'")[0][0]
                    assert boundary < applied
                    assert native.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched' AND json_extract(payload,'$.owner')=? AND seq>?", (record[2], applied)) == [(0,)]
                    if native.destination.joinpath("destination-effect").exists():
                        destination_turn, destination_log = native.rows("SELECT id,result FROM turns WHERE session_id=? ORDER BY rowid DESC LIMIT 1", (record[1],))[0]
                        log = json.loads(destination_log)
                        assert destination_turn != record[2] and log["model"] == "n"
                        assert log["display"]["location"] == str(native.destination) and log["display"]["move_epoch"] == applied
            else:
                fixture.until(lambda: native.rows("SELECT id FROM turns WHERE session_id='t50-background' AND status!='started'"), "refused source turn not terminal: " + repr(native.errors))
                assert native.rows("SELECT count(*) FROM session_moves") == [(0,)]
                assert native.rows("SELECT count(*) FROM events WHERE kind='session_moved'") == [(0,)]
                assert not native.destination.joinpath("source-effect").exists()
                move_rows = native.rows("SELECT state FROM tool_operations WHERE name='opencode_session_move'")
                assert move_rows and all(row[0] != "completed" for row in move_rows), move_rows
            assert native.rows("SELECT count(*) FROM permission_grants") == [(0,)]
            assert not native.errors, native.errors
            counters = {"requests": len(native.requests), "moves": native.rows("SELECT count(*) FROM session_moves")[0][0],
                        "applied": native.rows("SELECT count(*) FROM events WHERE kind='session_moved'")[0][0],
                        "operations": native.rows("SELECT count(*) FROM tool_operations")[0][0],
                        "turns": native.rows("SELECT count(*) FROM turns")[0][0], "remote_effects": rpc["tools/call"]}
        finally:
            release.set(); finish.set()
            if original_project.joinpath("leader").exists():
                original_project.joinpath("bg-release").write_text("cleanup")
            if remote:
                native.stop()
                remote.shutdown(); remote.server_close(); remote_thread.join(timeout=2)
                assert not remote_thread.is_alive(), "owned fake remote MCP worker not joined"
        native.stop()
        counters["responses_posts"] = state["responses_posts"]
        counters["requests"] = len(native.requests)
        counters["operations"] = native.rows("SELECT count(*) FROM tool_operations")[0][0]
        counters["turns"] = native.rows("SELECT count(*) FROM turns")[0][0]
        counters["auxiliary_requests"] = state["responses_posts"] - len(native.requests)
        counters["generation_dispatches"] = native.rows("SELECT count(*) FROM events WHERE kind='generation_dispatched'")[0][0]
        assert all(binding["binding_preserved"] for binding in wire_bindings), "source/destination routing credential binding changed"
        counters["destination_requests"] = sum(binding["destination"] for binding in wire_bindings)
        counters["source_binding_preserved"] = True
        if original_project.joinpath("leader").exists():
            assert not Path("/proc/" + original_project.joinpath("leader").read_text()).exists(), "owned shell leader not reaped"
    return {"case": case, "status": "PASS", **counters, "owned_cleanup": "joined"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--case", choices=CASES)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = digest(binary)
    results = []
    for case in [options.case] if options.case else CASES:
        try:
            result = run(binary, case)
        except Exception as error:
            result = {"case": case, "status": "FAIL", "error": str(error)}
            traceback.print_exc(limit=3)
        results.append(result)
        print(json.dumps(result, ensure_ascii=False), flush=True)
    assert digest(binary) == before, "normal ELF changed during fixture"
    print(json.dumps({"sha256_before": before, "sha256_after": digest(binary), "cases": len(results), "failed": sum(r["status"] != "PASS" for r in results)}))
    raise SystemExit(int(any(r["status"] != "PASS" for r in results)))


if __name__ == "__main__":
    main()
