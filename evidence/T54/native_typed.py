#!/usr/bin/env python3
"""Bounded offline qualification of the final existing ELF, without Cargo.

Only synthetic loopback credentials/configuration; output is safe facts, not
provider payloads or subprocess logs. Temporary files live under owned TMPDIR.
"""
import hashlib
import http.server
import json
from pathlib import Path
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading


ROOT = Path(__file__).resolve().parents[2]
TMP = Path("/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924")


def event(value):
    return ("data: " + json.dumps(value) + "\n\n").encode()


def check(binary, name, status, body, expected, exit_code=1, hold=False):
    count = 0
    lanes = {"main": 0, "title": 0}
    closed = threading.Event()
    received = threading.Event()

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            self.send_error(404)

        def do_POST(self):
            nonlocal count
            length = int(self.headers["Content-Length"])
            assert 0 < length < 1024 * 1024
            request = json.loads(self.rfile.read(length))
            assert self.path == "/v1/responses"
            assert request["model"] == "native-fixture-model"
            count += 1
            # Fresh headless submission owns a separate automatic-title lane.
            # Keep actual sockets distinct from an adapter retry assertion.
            lane = "title" if "Generate a short session title" in json.dumps(request["input"]) else "main"
            lanes[lane] += 1
            self.send_response(status)
            self.send_header("Content-Type", "text/event-stream" if status == 200 else "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("x-should-retry", "false")
            self.send_header("retry-after", "900")
            self.end_headers()
            if lane == "main":
                received.set()
            if hold:
                self.connection.settimeout(5)
                if self.rfile.read(1) == b"":
                    closed.set()
            else:
                self.wfile.write(body)
                self.wfile.flush()

    with tempfile.TemporaryDirectory(prefix="t54-native-", dir=TMP) as directory:
        location = Path(directory)
        home, project = location / "home", location / "project"
        config = home / "config" / "opencode"
        config.mkdir(parents=True)
        project.mkdir()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        settings = {
            "model": "fixture/native-fixture-model",
            "agent": {"title": {"disable": True}},
            "compaction": {"auto": False},
            "permission": {"bash": "allow"},
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "auth-canary"},
                "models": {"native-fixture-model": {"limit": {"context": 65536, "output": 4096}}},
            }},
        }
        (config / "opencode.json").write_text(json.dumps(settings))
        env = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
               "XDG_DATA_HOME": str(home / "data"), "OC_TEST_ALLOW_LOOPBACK": "1"}
        process = subprocess.Popen([str(binary), "run", "--json", "T54 offline physical fixture"],
                                   cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            if hold:
                assert received.wait(5), "held request never reached socket"
                process.send_signal(signal.SIGINT)
            stdout, stderr = process.communicate(timeout=15)
            assert process.returncode == exit_code, (name, "unexpected exit", process.returncode)
            diagnostic = stderr.decode()
            assert expected in diagnostic, (name, "missing safe classification")
            for canary in ("auth-canary", "body-canary", "env-canary", "private.invalid"):
                assert canary not in diagnostic + stdout.decode(), (name, "unsafe projection")
            assert lanes["main"] == 1 and lanes["title"] <= 1, (name, "physical lane counts", lanes)
            if hold:
                assert closed.wait(2), "cancel did not close held error body socket"
            database = list((home / "data").rglob("oc.sqlite"))
            assert len(database) == 1, "isolated database count"
            with sqlite3.connect(database[0]) as connection:
                assert connection.execute("SELECT COUNT(*) FROM tool_operations").fetchone()[0] == 0
                if name == "length":
                    result = json.loads(connection.execute("SELECT result FROM turns").fetchone()[0])
                    assert result["display"]["finish_reason"] == "length", "length fact was lost"
            assert not (project / "length-effects").exists(), "uncompleted tool performed a shell effect"
            return {"case": name, "exit": process.returncode, "physical_posts": count,
                    "physical_lanes": lanes,
                    "safe_classification": expected, "tool_intents": 0, "effect_files": 0,
                    **({"held_socket_closed": True} if hold else {})}
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)


def main():
    binary = (ROOT / (sys.argv[1] if len(sys.argv) > 1 else "target/debug/oc")).resolve()
    executable = binary.read_bytes()
    assert executable[:4] == b"\x7fELF"
    results = []
    for name, status, code, expected in [
        ("throttle", 429, "rate_limit_exceeded", "rate limited"),
        ("quota", 429, "insufficient_quota", "quota exceeded"),
        ("payment", 402, "", "quota exceeded"),
        ("timeout", 408, "", "provider internal error"),
        ("conflict", 409, "", "provider internal error"),
        ("server", 503, "context_length_exceeded", "provider internal error"),
        ("invalid", 400, "server_error", "invalid provider request"),
        ("auth", 401, "", "authentication rejected"),
        ("forbidden", 403, "", "authentication rejected"),
        ("payload", 413, "", "payload too large"),
        ("policy-precedence", 402, "content_filter", "content policy rejection"),
    ]:
        body = json.dumps({"error": {"code": code}, "raw": "body-canary env-canary https://private.invalid"}).encode()
        results.append(check(binary, name, status, body, expected))
    delta = event({"type": "response.output_text.delta", "delta": "partial"})
    results.append(check(binary, "sse-failed", 200, delta + event({"type": "response.failed", "status_code": 429, "response": {"error": {"code": "insufficient_quota"}}}), "quota exceeded"))
    results.append(check(binary, "length", 200, delta + event({"type": "response.incomplete", "response": {"output": [{"type": "message", "role": "assistant", "status": "in_progress", "content": [{"type": "output_text", "text": "partial"}]}], "incomplete_details": {"reason": "max_output_tokens"}, "usage": {"input_tokens": 2, "output_tokens": 3}}}), "session", 0))
    results.append(check(binary, "filter", 200, delta + event({"type": "response.incomplete", "response": {"incomplete_details": {"reason": "content_filter"}}}), "content policy rejection"))
    results.append(check(binary, "partial-eof", 200, delta, "incomplete stream"))
    # A well-formed terminal-only lookalike must not recover a tool on an
    # incomplete finish: no prior output_item.done exists for this call.
    call = {"type": "function_call", "id": "fc", "call_id": "c", "name": "bash",
            "arguments": json.dumps({"argv": ["/bin/sh", "-c", "printf effect >> length-effects"]}), "status": "completed"}
    results.append(check(binary, "incomplete-tool", 200, delta + event({"type": "response.incomplete", "response": {"output": [call], "incomplete_details": {"reason": "max_output_tokens"}}}), "incomplete stream"))
    results.append(check(binary, "malformed-sse", 200, b"data: invalid\n\n", "invalid provider output"))
    results.append(check(binary, "cancel-held-error", 401, b"{}", "", 130, True))
    print(json.dumps({"binary": str(binary.relative_to(ROOT)), "sha256": hashlib.sha256(executable).hexdigest(),
                      "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                      "source_dirty": True, "cases": results, "total_physical_posts": sum(r["physical_posts"] for r in results),
                      "scope": "T54 R1 physical facts only; R2/R3/R4 pending"}, indent=2))


if __name__ == "__main__":
    main()
