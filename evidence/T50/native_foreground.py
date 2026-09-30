#!/usr/bin/env python3
"""Bounded offline foreground shell qualification against a supplied native ELF."""
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
BACKGROUND_SUPPORTED = False
QUESTION_SUPPORTED = False
import time


def run(binary, name, tool, args, permission, expected, *, auto=False, shell=None, path="/usr/bin:/bin", profile=None, child=False, cancel=False, plural=None):
    with tempfile.TemporaryDirectory(prefix="t50-foreground-") as tmp:
        root = Path(tmp)
        home, project = root / "home", root / "project"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        (project / "space dir").mkdir(parents=True)
        requests = []

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                length = int(self.headers.get("content-length", "0"))
                assert 0 < length <= 1048576
                request = json.loads(self.rfile.read(length))
                if request.get("max_output_tokens") == 256 and not request.get("tools"):
                    # Existing native auxiliary-title boundary; never consume
                    # the main/child execution script on a concurrent title.
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
                if child and len(requests) == 1:
                    item = {"type": "function_call", "id": "fc-parent", "call_id": "parent-call",
                            "name": "subagent", "arguments": json.dumps({"agent": profile,
                            "description": "Read only", "prompt": "Observe read-only restrictions"}), "status": "completed"}
                    events = [{"type": "response.output_item.added", "item": item},
                              {"type": "response.output_item.done", "item": item}]
                elif len(requests) == (2 if child else 1):
                    item = {"type": "function_call", "id": "fc", "call_id": "call",
                            "name": tool, "arguments": json.dumps(args), "status": "completed"}
                    events = [{"type": "response.output_item.added", "item": item},
                              {"type": "response.output_item.done", "item": item}]
                else:
                    events = [{"type": "response.output_text.delta", "delta": "done"}]
                events.append({"type": "response.completed", "response": {
                    "status": "completed", "usage": {"input_tokens": 1, "output_tokens": 1}}})
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
        data = root / "data"
        configuration = {
            "model": "fixture/m", "compaction": {"auto": False},
            "agent": {"title": {"disable": True}}, "permission": permission,
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                "models": {"m": {"limit": {"context": 65536, "output": 2048}}}}}}
        if plural is not None:
            configuration["permissions"] = plural
        if profile:
            configuration["agent"][profile] = {"mode": "subagent" if child else "primary",
                "prompt": f"T50_{profile.upper()}_READ_ONLY. Use the effective read-only catalog.",
                "tools": {"shell": False, "apply_patch": False, "subagent": False},
                "permission": {"read": "allow", "glob": "allow", "grep": "allow", "compress": "allow"}}
            if not child:
                configuration["default_agent"] = profile
        config.joinpath("opencode.json").write_text(json.dumps(configuration))
        wrapper = root / ("fish" if shell == "fish" else "selected-shell")
        wrapper.write_text('#!/bin/sh\nprintf selected > "shell-selected"\nexec /bin/bash "$@"\n')
        wrapper.chmod(0o700)
        startup = root / "startup-trap"
        startup.write_text("printf startup > startup-effect\n")
        env = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
               "PATH": path, "OC_TEST_ALLOW_LOOPBACK": "1",
               "SHELL": str(wrapper) if shell in {"probe", "fish"} else (shell or "/bin/bash"),
               "BASH_ENV": str(startup),
               "OPENAI_API_KEY": "synthetic-trap-key", "T50_SECRET": "synthetic-trap-secret"}
        cmd = [str(binary), "--data-dir", str(data), "run", "--json", "invoke foreground"]
        if auto:
            cmd.append("--auto")
        try:
            if cancel:
                process = subprocess.Popen(cmd, cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    until = time.monotonic() + 4
                    while not project.joinpath("cancel-pid").exists():
                        assert process.poll() is None and time.monotonic() < until, "native shell never started"
                        time.sleep(0.01)
                    time.sleep(0.15)
                    process.send_signal(signal.SIGINT)
                    stdout, stderr = process.communicate(timeout=15)
                    result = subprocess.CompletedProcess(cmd, process.returncode, stdout, stderr)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.communicate(timeout=3)
            else:
                result = subprocess.run(cmd, cwd=project, env=env, capture_output=True, timeout=15)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)
            assert not thread.is_alive(), "owned fake HTTP worker did not join"
        assert requests, (name, result.stderr.decode())
        tools = requests[0]["tools"]
        names = [t["name"] for t in tools]
        assert "bash" not in names, (name, "legacy bash advertised", names)
        assert not set(names) & {"websearch", "execute", "write", "edit",
                                  "opencode_models", "opencode_session_rename", "opencode_session_move"}
        if not QUESTION_SUPPORTED or (profile in {"general", "explore"} and not child):
            assert "question" not in names
        else:
            assert "question" in names
        active_tools = requests[1]["tools"] if child else tools
        active_names = [t["name"] for t in active_tools]
        assert "bash" not in active_names
        if profile in {"general", "explore"}:
            assert "question" not in active_names
        if profile:
            assert "shell" not in active_names and "apply_patch" not in active_names and "subagent" not in active_names
            assert f"T50_{profile.upper()}_READ_ONLY" in json.dumps(requests[1 if child else 0]["input"])
        elif "deny" in (permission.get("bash"), permission.get("shell"),
                        (plural or {}).get("bash"), (plural or {}).get("shell")):
            assert "shell" not in names, (name, "denied shell advertised")
        else:
            definition = next(t for t in tools if t["name"] == "shell")
            assert definition["parameters"]["required"] == ["command"]
            assert set(definition["parameters"]["properties"]) == ({"command", "workdir", "timeout", "background"} if BACKGROUND_SUPPORTED else {"command", "workdir", "timeout"})
            expected_timeout = {"type":"integer", "minimum":0, "maximum":600000}
            if not BACKGROUND_SUPPORTED:
                expected_timeout["default"] = 120000
            assert definition["parameters"]["properties"]["timeout"] == expected_timeout
        db = sqlite3.connect(data / "oc.sqlite")
        operations = db.execute("SELECT name,state,output FROM tool_operations").fetchall()
        grants = db.execute("SELECT count(*) FROM permission_grants").fetchone()[0]
        if BACKGROUND_SUPPORTED:
            assert db.execute("SELECT count(*) FROM shell_jobs").fetchone()[0] == 0
        assert grants == 0, "direct --auto must remain Once"
        assert len(operations) == (2 if child else 1), (name, operations, result.stderr.decode())
        operation = next(op for op in operations if op[0] == tool)
        output = operation[2]
        # Existing shell::RETAIN_CAP_BYTES is 1 MiB per stdout/stderr stream.
        assert len(output.encode()) <= 2 * 1024 * 1024 + 256
        if cancel:
            assert operation[1] == "cancelled" and "[truncated]" in output
        elif name == "timeout":
            assert operation[1] == "timed_out"
        assert expected in output, (name, operations, result.stderr.decode())
        assert "synthetic-trap-key" not in output and "synthetic-trap-secret" not in output
        assert not project.joinpath("startup-effect").exists() and not project.joinpath("space dir/startup-effect").exists()
        if shell == "fish":
            assert not project.joinpath("shell-selected").exists(), "incompatible inherited fish executed"
        if name == "selected-operators":
            assert project.joinpath("space dir/marker").read_text() == "quoted space\n"
            assert project.joinpath("space dir/shell-selected").read_text() == "selected"
        if name == "legacy-literal":
            assert not project.joinpath("wrong-effect").exists()
        for filename in ["cancel-pid", "timeout-pid", "timeout-child"]:
            recorded = project / filename
            if recorded.exists():
                pid = int(recorded.read_text())
                assert pid > 1
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    pass
                else:
                    raise AssertionError((name, "fixture child not reaped", pid))
        if profile or name.startswith("deny") or name in {"background", "invalid-timeout", "invalid-workdir"}:
            assert not project.joinpath("marker").exists()
            assert operation[1] not in {"started", "completed"}
        if len(requests) > 1:
            outputs = [i for i in requests[2 if child else -1]["input"] if i.get("type") == "function_call_output"]
            assert outputs and outputs[-1]["call_id"] == "call" and expected in outputs[-1]["output"], (
                name, [(r.get("model"), len(r.get("tools", [])),
                        [(i.get("type"), i.get("call_id"), str(i.get("output", ""))[:100])
                         for i in r["input"]]) for r in requests])
        db.close()
        print(json.dumps({"case": name, "status": "PASS", "requests": len(requests),
                          "operation": operation[:2], "grants": grants,
                          "effect_rows": len(operations), "provider": list(server.server_address)}))


def main():
    global BACKGROUND_SUPPORTED, QUESTION_SUPPORTED
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--background-supported", action="store_true", help="current background slice schema; historical mode remains the default")
    parser.add_argument("--question-supported", action="store_true", help="current question slice catalog; historical mode remains the default")
    options = parser.parse_args()
    BACKGROUND_SUPPORTED = options.background_supported
    QUESTION_SUPPORTED = options.question_supported
    binary = options.binary.resolve()
    print(json.dumps({"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}))
    run(binary, "selected-operators", "shell", {"command":
        "printf '%s\\n' 'quoted space' > marker && test -z \"$OPENAI_API_KEY$T50_SECRET\" && printf 'operator-ok'",
        "workdir": "space dir"}, {"bash": "allow"}, "operator-ok", shell="probe")
    run(binary, "legacy-literal", "bash", {"argv": ["/usr/bin/printf", "%s", "literal; touch wrong-effect"]},
        {"shell": "allow"}, "literal; touch wrong-effect")
    run(binary, "zero", "shell", {"command": "sleep 0.08; printf zero-ok", "timeout": 0},
        {"shell": "allow"}, "zero-ok")
    run(binary, "timeout", "shell", {"command": "printf '%s' $$ > timeout-pid; sleep 5 & printf '%s' $! > timeout-child; wait", "timeout": 25},
        {"shell": "allow"}, "[timeout]")
    run(binary, "cancel-zero", "shell", {"command": "printf '%s' $$ > cancel-pid; trap '' TERM; while :; do printf '%32768s' ''; done", "timeout": 0},
        {"shell": "allow"}, "[cancelled]", cancel=True)
    run(binary, "deny-shell", "shell", {"command": "touch marker"}, {"bash": "deny"}, "denied")
    run(binary, "deny-bash", "bash", {"argv": ["/usr/bin/touch", "marker"]}, {"shell": "deny"}, "denied")
    run(binary, "deny-mixed", "shell", {"command": "touch marker"}, {"bash": "deny", "shell": "allow"}, "denied")
    run(binary, "deny-reverse-mixed", "bash", {"argv": ["/usr/bin/touch", "marker"]}, {"shell": "deny", "bash": "allow"}, "denied")
    run(binary, "deny-split-keys", "shell", {"command": "touch marker"},
        {"bash": "deny"}, "denied", plural={"shell": "allow"})
    run(binary, "deny-reverse-split-keys", "bash", {"argv": ["/usr/bin/touch", "marker"]},
        {"shell": "deny"}, "denied", plural={"bash": "allow"})
    run(binary, "ask-once", "shell", {"command": "printf approved"}, {"bash": "ask"}, "approved", auto=True)
    run(binary, "background", "shell", {"command": "touch marker", "background": "yes" if BACKGROUND_SUPPORTED else True},
        {"shell": "allow"}, "invalid" if BACKGROUND_SUPPORTED else "unsupported")
    run(binary, "invalid-timeout", "shell", {"command": "touch marker", "timeout": -1},
        {"shell": "allow"}, "invalid")
    run(binary, "invalid-workdir", "shell", {"command": "touch marker", "workdir": "../"},
        {"shell": "allow"}, "cwd")
    run(binary, "fallback", "shell", {"command": "printf '%s' \"$0\""}, {"shell": "allow"}, "/usr/bin/bash",
        shell="/does-not-exist")
    run(binary, "fish-fallback", "shell", {"command": "printf '%s' \"$0\""}, {"shell": "allow"}, "/usr/bin/bash", shell="fish")
    run(binary, "linux-default-sh", "shell", {"command": "printf '%s' \"$0\""}, {"shell": "allow"}, "/bin/sh",
        shell="/does-not-exist", path="/does-not-exist")
    run(binary, "plan-ceiling", "shell", {"command": "touch marker"}, {"shell": "allow"}, "denied", profile="plan")
    for profile in ["general", "explore"]:
        run(binary, f"{profile}-child-ceiling", "shell", {"command": "touch marker"},
            {"shell": "allow", "subagent": "allow", "read": "allow", "glob": "allow", "grep": "allow", "compress": "allow"},
            "denied", profile=profile, child=True)


if __name__ == "__main__":
    main()
