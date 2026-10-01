#!/usr/bin/env python3
"""TOOL17 normal ELF/fake HTTP+Responses; bounded RAM facts, fully joined cleanup."""
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
import time

HTML = ("<html><head><script>HIDDEN_SCRIPT</script></head><body><h1>Привет &amp; 🦀</h1>"
        "<ul><li>one</li><li>二</li></ul><p><a href='/docs'>guide</a> <code>x &lt; y</code></p>"
        "<pre><code class='language-rust'>fn main() {\n  println!(\"é\");\n}</code></pre>"
        "<table><tr><th>Name</th><th>Value</th></tr><tr><td>é</td><td>42</td></tr></table></body></html>")
CASES = ("default", "markdown", "text", "html", "redirect", "plain", "json", "status",
         "seconds", "invalid_format", "invalid_timeout", "invalid_null", "unknown_option",
         "auth", "password", "scheme", "private", "loopback_refused", "denied", "ask",
         "private_redirect", "password_redirect", "scheme_redirect", "redirect_limit", "oversize",
         "timeout_body", "cancel_body")
NO_GET = {"invalid_format", "invalid_timeout", "invalid_null", "unknown_option", "auth", "password", "scheme", "private", "loopback_refused", "denied", "ask"}
ERROR = NO_GET | {"private_redirect", "password_redirect", "scheme_redirect", "redirect_limit", "timeout_body"}

def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-fetch-") as temporary:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        requests, auxiliary, gets, failures = [], [], [], []
        held = threading.Event()
        release = threading.Event()
        arguments = {}

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def do_GET(self):
                try:
                    gets.append(self.path)
                    assert self.headers.get("authorization") is None, "unexpected credential"
                    assert self.headers.get("cookie") is None, "unexpected cookie"
                    status, mime, body, target = 200, "text/html; charset=utf-8", HTML.encode(), None
                    if self.path == "/redirect":
                        status, target = 302, "/page"
                    elif self.path == "/private_redirect":
                        status, target = 302, "http://10.0.0.1/UNAUTHORIZED"
                    elif self.path == "/password_redirect":
                        status, target = 302, f"http://user:DO_NOT_LEAK@127.0.0.1:{server.server_port}/UNAUTHORIZED"
                    elif self.path == "/scheme_redirect":
                        status, target = 302, "file:///UNAUTHORIZED"
                    elif self.path == "/redirect_limit":
                        status, target = 302, "/redirect_limit"
                    elif self.path == "/plain":
                        mime, body = "text/plain; charset=utf-8", b"<p>literal</p>"
                    elif self.path == "/json":
                        mime, body = "application/json", '{"é":"<p>literal</p>"}'.encode()
                    elif self.path == "/status":
                        status = 404
                    elif self.path == "/oversize":
                        mime, body = "text/plain", b"x" * (1048576 + 64)
                    elif self.path == "/held":
                        self.send_response(200)
                        self.send_header("Content-Type", "text/plain")
                        self.send_header("Content-Length", "100")
                        self.end_headers()
                        self.wfile.write(b"held")
                        self.wfile.flush()
                        held.set()
                        assert release.wait(8), "owned held-body release absent"
                        return
                    assert "UNAUTHORIZED" not in self.path
                    self.send_response(status)
                    self.send_header("Content-Type", mime)
                    if target is not None:
                        self.send_header("Location", target)
                        body = b""
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass  # capped body/cancel closes the response deliberately
                except Exception as error:
                    failures.append(str(error))
                finally:
                    self.close_connection = True

            def do_POST(self):
                try:
                    length = int(self.headers.get("content-length", "0"))
                    assert 0 < length <= 2097152
                    request = json.loads(self.rfile.read(length))
                    if request.get("max_output_tokens") == 256 and not request.get("tools"):
                        auxiliary.append(request)
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        self.wfile.write(b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
                        return
                    requests.append(request)
                    assert len(requests) <= 2, "unexpected physical provider request"
                    definitions = [t for t in request["tools"] if t["name"] == "webfetch"]
                    if case == "denied":
                        assert not definitions, "Deny catalog ceiling"
                    else:
                        definition, = definitions
                        props = definition["parameters"]["properties"]
                        assert props["format"]["enum"] == ["text", "markdown", "html"]
                        assert props["format"]["default"] == "markdown"
                        assert props["timeout"]["maximum"] == 120 and props["timeout"]["default"] == 30
                        assert props["timeout"]["type"] == "number" and "seconds" in definition["description"]
                        assert definition["parameters"]["additionalProperties"] is False
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    if len(requests) == 1:
                        item = {"type":"function_call", "id":"fetch-item", "call_id":"fetch-call",
                                "name":"webfetch", "arguments":json.dumps(arguments), "status":"completed"}
                        events = [{"type":"response.output_item.added", "item":item},
                                  {"type":"response.output_item.done", "item":item},
                                  {"type":"response.completed", "response":{"status":"completed", "output":[item]}}]
                    else:
                        outputs = [i for i in request["input"] if i.get("type") == "function_call_output" and i.get("call_id") == "fetch-call"]
                        assert len(outputs) == 1
                        assert len(outputs[0]["output"].encode()) <= 1048576
                        assert "DO_NOT_LEAK" not in outputs[0]["output"]
                        with sqlite3.connect(data / "oc.sqlite") as db:
                            rows = db.execute("SELECT state,output FROM tool_operations WHERE name='webfetch'").fetchall()
                        if rows:
                            assert len(rows) == 1 and rows[0][1] == outputs[0]["output"], "journal/wire mismatch"
                            assert rows[0][0] == ("failed" if case in ERROR else "completed"), rows[0][0]
                        events = [{"type":"response.completed", "response":{"status":"completed", "output":[{
                            "type":"message", "id":"done", "role":"assistant", "content":[{"type":"output_text", "text":"DONE"}]}]}}]
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                except Exception as error:
                    failures.append(str(error))
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        base = f"http://127.0.0.1:{server.server_port}"
        route = case if case in ("redirect", "plain", "json", "status", "private_redirect", "password_redirect", "scheme_redirect", "redirect_limit", "oversize") else ("held" if case in ("timeout_body", "cancel_body") else "page")
        arguments.update(url=f"{base}/{route}")
        if case in ("markdown", "text", "html"):
            arguments["format"] = case
        if case == "seconds":
            arguments["timeout"] = 0.5
        if case == "timeout_body":
            arguments["timeout"] = 0.15
        if case == "invalid_format":
            arguments["format"] = "xml"
        if case == "invalid_timeout":
            arguments["timeout"] = 121
        if case == "invalid_null":
            arguments["format"] = None
        if case == "unknown_option":
            arguments["timeoutSeconds"] = 30
        if case == "auth":
            arguments["headers"] = {"Authorization":"DO_NOT_LEAK"}
        if case == "password":
            arguments["url"] = f"http://user:DO_NOT_LEAK@127.0.0.1:{server.server_port}/UNAUTHORIZED"
        if case == "scheme":
            arguments["url"] = "file:///UNAUTHORIZED"
        if case == "private":
            arguments["url"] = "http://10.0.0.1/UNAUTHORIZED"
        permission = "deny" if case == "denied" else "ask" if case == "ask" else "allow"
        configuration = {"model":"fixture/fetch-model", "compaction":{"auto":False},
            "agent":{"title":{"disable":True}}, "permission":{"webfetch":permission},
            "provider":{"fixture":{"npm":"@ai-sdk/openai", "options":{"baseURL":base+"/v1", "apiKey":"synthetic"},
                "models":{"fetch-model":{"limit":{"context":1048576,"input":1048576,"output":2048}}}}}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME":str(home), "XDG_CONFIG_HOME":str(home/"config"), "PATH":"/usr/bin:/bin",
                       "OC_TEST_ALLOW_LOOPBACK":"1"}
        # Provider and webfetch exceptions are independent; default webfetch
        # refusal is proven using the same counted fake endpoint.
        if case != "loopback_refused":
            environment["OC_TEST_WEBFETCH_ALLOW_LOOPBACK"] = "1"
        process = None
        thread.start()
        started = time.monotonic()
        try:
            process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", "Fetch fixture"],
                cwd=project, env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            if case == "cancel_body":
                assert held.wait(5), "no real held GET"
                os.kill(process.pid, signal.SIGINT)
            try:
                stdout, stderr = process.communicate(timeout=12)
            except subprocess.TimeoutExpired as error:
                raise AssertionError(("native timeout", "provider_requests", len(requests), "gets", gets,
                    "peer_failures", failures, "stderr", (error.stderr or b"")[-1200:].decode(errors="replace"))) from error
            assert len(stdout) < 1048576 and len(stderr) < 1048576
            assert not failures, failures
            assert gets == ([] if case in NO_GET else (["/redirect", "/page"] if case == "redirect" else ["/redirect_limit"] * 6 if case == "redirect_limit" else [f"/{route}"])), gets
            with sqlite3.connect(data / "oc.sqlite") as db:
                rows = db.execute("SELECT state,output FROM tool_operations WHERE name='webfetch'").fetchall()
            if case == "cancel_body":
                assert len(requests) == 1 and len(rows) == 1 and rows[0][0] == "cancelled", (len(requests), rows)
                assert rows[0][1] == "error: cancelled"
                assert time.monotonic() - started < 5
            elif case == "ask":
                assert len(requests) == 1 and not rows and process.returncode != 0, (len(requests), rows, process.returncode)
            else:
                assert len(requests) == 2 and process.returncode == 0, (len(requests), process.returncode, stderr[-1000:].decode())
                output = next(i["output"] for i in requests[1]["input"] if i.get("type") == "function_call_output" and i.get("call_id") == "fetch-call")
                assert len(output.encode()) <= 1048576
                assert "DO_NOT_LEAK" not in output
                if case in ERROR:
                    assert output.startswith("error:"), output[:200]
                    if case == "timeout_body":
                        assert "deadline" in output and time.monotonic() - started < 3
                else:
                    header, content = output.split("\n\n", 1)
                    metadata = json.loads(header)
                    assert metadata["url"] == arguments["url"]
                    assert metadata["final_url"] == base + ("/page" if case == "redirect" else f"/{route}")
                    assert metadata["status"] == (404 if case == "status" else 200)
                    assert metadata["format"] == arguments.get("format", "markdown")
                    assert metadata["timeout_seconds"] == arguments.get("timeout", 30)
                    assert metadata["content_type"] == ("text/plain" if case in ("plain","oversize") else "application/json" if case == "json" else "text/html")
                    assert metadata["truncated"] == (case == "oversize")
                    if case == "html":
                        assert content == HTML
                    elif case == "text":
                        assert "Привет & 🦀" in content and "<h1>" not in content and "HIDDEN_SCRIPT" not in content
                    elif case == "plain":
                        assert content == "<p>literal</p>"
                    elif case == "json":
                        assert content == '{"é":"<p>literal</p>"}'
                    elif case != "oversize":
                        for expected in ("# Привет & 🦀", "- one", "- 二", "[guide](/docs)", "`x < y`", "```rust", "| Name | Value |", "| --- | --- |", "| é | 42 |"):
                            assert expected in content, (expected, content)
                        assert "HIDDEN_SCRIPT" not in content
            return {"case":case,"status":"PASS","provider_requests":len(requests)+len(auxiliary),
                    "main_requests":len(requests),"auxiliary_requests":len(auxiliary),"get_requests":len(gets),
                    "tool_rows":len(rows),"terminal_states":[r[0] for r in rows],"process_exit":process.returncode}
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            release.set()
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()  # joins every non-daemon HTTP handler
            assert not thread.is_alive(), "owned HTTP loop failed to join"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--case", choices=CASES)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = digest(binary)
    failures = 0
    totals = {"cases":0,"provider_requests":0,"main_requests":0,"auxiliary_requests":0,"get_requests":0,"tool_rows":0}
    for case in [options.case] if options.case else CASES:
        try:
            result = run(binary, case)
            for key in totals:
                totals[key] += 1 if key == "cases" else result[key]
        except Exception as error:
            failures += 1
            result = {"case":case,"status":"FAIL","error":str(error)}
        print(json.dumps(result, ensure_ascii=False))
    after = digest(binary)
    assert before == after
    print(json.dumps({"binary":str(binary),"sha256_before":before,"sha256_after":after,"failures":failures,
                      "owned_cleanup":"all PGIDs/HTTP threads joined before TempDir cleanup",**totals}))
    raise SystemExit(bool(failures))

if __name__ == "__main__":
    main()
