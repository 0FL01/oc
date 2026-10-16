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
import struct
import zlib

CASES = ("text_default", "text_page", "nested_lifecycle", "directory", "directory_page",
         "nested_directory", "image", "malformed", "pdf", "unsupported_model", "over_budget",
         "denied", "symlink", "fifo", "binary", "invalid_offset", "incomplete", "model_budget", "image_dcp", "image_compaction")
ERRORS = {"malformed", "pdf", "unsupported_model", "over_budget", "denied", "symlink", "fifo", "binary", "invalid_offset"}
ERRORS.update(("incomplete", "model_budget"))

def png_bytes(size=1, compression=6):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress((b"\0" + b"\xff\0\0\xff" * size) * size, level=compression)) + chunk(b"IEND", b""))


def run(binary, case):
    image_case = case in ("image", "image_dcp", "image_compaction")
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
        listing = project / "listing"
        listing.mkdir()
        (listing / "z-dir").mkdir()
        (listing / "b.txt").write_text("b")
        (listing / "a.txt").write_text("a")
        image = png_bytes()
        (project / "image.png").write_bytes(image)
        (project / "budget.png").write_bytes(png_bytes(128, 0))
        (project / "malformed.png").write_bytes(b"\x89PNG\r\n\x1a\n\0FAKE")
        (project / "incomplete.png").write_bytes(image[:-12])
        (project / "document.pdf").write_bytes(b"%PDF-1.7 unsupported")
        (project / "oversize.png").write_bytes(image + b"x" * 1048576)
        (project / "binary.bin").write_bytes(b"safe\n\0PROTECTED_CANARY")
        (project / "link").symlink_to(project / "image.png")
        os.mkfifo(project / "fifo")
        arguments = {
            "text_default": {"path": "large.txt"},
            "text_page": {"path": "large.txt", "offset": 2001, "limit": 3},
            "nested_lifecycle": {"path": "nested/small.txt", "limit": 2},
            "directory": {"path": "listing", "limit": 2},
            "directory_page": {"path": "listing", "offset": 3, "limit": 2},
            "nested_directory": {"path": "nested"},
            "image": {"path": "image.png"},
            "malformed": {"path": "malformed.png"},
            "pdf": {"path": "document.pdf"},
            "unsupported_model": {"path": "image.png"},
            "over_budget": {"path": "oversize.png"},
            "denied": {"path": "image.png"},
            "symlink": {"path": "link"},
            "fifo": {"path": "fifo"},
            "binary": {"path": "binary.bin", "limit": 1},
            "invalid_offset": {"path": "large.txt", "offset": 0},
            "incomplete": {"path": "incomplete.png"},
            "model_budget": {"path": "budget.png"},
            "image_dcp": {"path": "image.png"},
            "image_compaction": {"path": "image.png"},
        }[case]
        requests, failures, compactions = [], [], []

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
                    if not request.get("tools") and "Summarize only what the user and assistant said and did" in json.dumps(request["input"]):
                        assert case == "image_compaction"
                        compactions.append(request)
                        parts = next(item["output"] for item in request["input"] if item.get("call_id") == "read-call" and item.get("type") == "function_call_output")
                        import base64
                        assert [part["type"] for part in parts] == ["input_text", "input_image"]
                        assert base64.b64decode(parts[1]["image_url"].split(",", 1)[1]) == image
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        self.wfile.write(('data: ' + json.dumps({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Work State\nKeep the admitted local image result."}]}]}}) + '\n\n').encode())
                        return
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
                        assert len(requests) <= (4 if case == "image_dcp" else (3 if image_case else 2)), "unexpected read requests"
                        with sqlite3.connect(data / "oc.sqlite") as database:
                            rows = database.execute(
                                "SELECT state,output FROM tool_operations WHERE name='read'").fetchall()
                        if case != "invalid_offset":
                            assert len(rows) == 1 and rows[0][0] == ("failed" if case in ERRORS else "completed"), ("durable read state", rows)
                        outputs = [entry for entry in request["input"]
                                   if entry.get("type") == "function_call_output" and entry.get("call_id") == "read-call"]
                        if case == "image_compaction" and len(requests) == 3:
                            assert len(compactions) == 1, "actual media compaction absent"
                            outputs = [entry for entry in requests[1]["input"] if entry.get("call_id") == "read-call" and entry.get("type") == "function_call_output"]
                        assert len(outputs) == 1 and outputs[0]["call_id"] == "read-call"
                        if image_case:
                            import base64
                            parts = outputs[0]["output"]
                            assert [p["type"] for p in parts] == ["input_text", "input_image"]
                            assert parts[0]["text"] == rows[0][1], "image presentation/journal mismatch"
                            prefix, encoded = parts[1]["image_url"].split(",", 1)
                            assert prefix == "data:image/png;base64" and base64.b64decode(encoded) == image
                            with sqlite3.connect(data / "oc.sqlite") as database:
                                logs = [json.loads(row[0]) for row in database.execute("SELECT result FROM conversation_turns WHERE result IS NOT NULL")]
                            facts = [f for log in logs for f in log.get("native_read_results", [])]
                            assert len(facts) == 1 and facts[0]["call_id"] == "read-call"
                            fact = facts[0]["result"]
                            assert fact["source"] == "local_file" and fact["mime"] == "image/png"
                            assert fact["source_path"] == str(project / "image.png")
                            assert base64.b64decode(fact["data"]) == image and fact["bytes"] == len(image)
                            assert not any(log.get("native_mcp_results") for log in logs)
                        elif case == "invalid_offset":
                            assert "error:" in outputs[0]["output"]
                        else:
                            assert outputs[0]["output"] == rows[0][1], "wire/journal read result mismatch"
                        events = [{"type": "response.completed", "response": {
                            "status": "completed", "output": [{"type": "message", "id": "done",
                                "role": "assistant", "content": [{"type": "output_text", "text": "READ_DONE"}]}]}}]
                        if case == "image_dcp" and len(requests) == 3:
                            with sqlite3.connect(data / "oc.sqlite") as database:
                                old = json.loads(database.execute("SELECT result FROM conversation_turns WHERE result IS NOT NULL ORDER BY rowid LIMIT 1").fetchone()[0])
                            item = {"type":"function_call","id":"dcp-item","call_id":"dcp-call","name":"compress","arguments":json.dumps({"topic":"closed read", "content":[{"startId":old["user_message"],"endId":old["assistant_message"],"summary":"Keep the admitted image; continue."}]}),"status":"completed"}
                            events = [{"type":"response.output_item.done","item":item},{"type":"response.completed","response":{"status":"completed","output":[item]}}]
                        if case == "image_dcp" and len(requests) == 4:
                            with sqlite3.connect(data / "oc.sqlite") as database:
                                compress = database.execute("SELECT state,output FROM tool_operations WHERE name='compress'").fetchall()
                            assert len(compress) == 1 and compress[0][0] == "completed" and not compress[0][1].startswith("error:"), compress
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
        configuration = {"model": "fixture/read-model", "compaction": {"auto": case == "image_compaction", "buffer": 65536 if case == "image_compaction" else 0},
             "agent": {"title": {"disable": True}}, "permission": {"read": "deny" if case == "denied" else "allow", "compress": "allow"},
            "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {
                "baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "synthetic"},
                 "models": {"read-model": {"limit": {"context": 65536, "input": 6000 if case == "model_budget" else 65536, "output": 2048},
                    "modalities": {"input": ["text"] if case == "unsupported_model" else ["text", "image"], "output": ["text"]}}}}}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"),
                       "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}
        process = None
        try:
            prompt = "Read fixture" + (" closed admitted image details" * 1000 if case == "image_dcp" else "")
            process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", prompt],
                cwd=project, env=environment, stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                stdout, stderr = process.communicate(timeout=20)
            except subprocess.TimeoutExpired as error:
                raise AssertionError(("application timeout", len(requests), failures)) from error
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
            elif case in ("nested_lifecycle", "nested_directory"):
                if case == "nested_lifecycle":
                    assert "first" in output and "second" in output
                instructions = [entry for entry in requests[1]["input"] if entry.get("role") in ("system", "developer")]
                rendered = json.dumps(instructions)
                assert "R5_ROOT_BASELINE" in rendered, "baseline control absent"
                assert rendered.count("R5_NESTED_ADMITTED_INSTRUCTION") == 1, (
                    "successful read did not invoke admitted nested lifecycle", "nested_instruction_count",
                     rendered.count("R5_NESTED_ADMITTED_INSTRUCTION"))
            elif case == "text_page":
                assert "2001: fixture_2001" in output and "2003: fixture_2003" in output and "fixture_2004" not in output and "offset: 2004" in output
            elif case == "directory":
                assert output.splitlines()[1:3] == ["z-dir/", "a.txt"] and "offset: 3" in output
            elif case == "directory_page":
                assert output.splitlines()[1:] == ["b.txt"] and "truncated" not in output
            elif case in ERRORS:
                assert isinstance(output, str) and "error:" in output and "PROTECTED_CANARY" not in output
                expected = {"pdf": "PDF", "unsupported_model": "selected model", "over_budget": "budget", "malformed": "image", "incomplete": "image", "model_budget": "budget", "invalid_offset": "offset"}.get(case)
                if expected:
                    assert expected in output, (case, output)
            elif image_case:
                with sqlite3.connect(data / "oc.sqlite") as database:
                    session = database.execute("SELECT id FROM sessions").fetchone()[0]
                    original = database.execute("SELECT output FROM tool_operations WHERE name='read'").fetchone()[0]
                    raw = database.execute("SELECT result FROM conversation_turns ORDER BY rowid LIMIT 1").fetchone()[0]
                (project / "image.png").write_text("changed current filesystem; must not be reread")
                process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", "--session", session, "Recall admitted image"],
                    cwd=project, env=environment, stdin=subprocess.DEVNULL,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                try:
                    stdout, stderr = process.communicate(timeout=20)
                except subprocess.TimeoutExpired as error:
                    raise AssertionError(("reopened application timeout", len(requests), failures)) from error
                assert process.returncode == 0 and not failures, (process.returncode, failures, stderr[-1024:].decode())
                assert len(requests) == (4 if case == "image_dcp" else 3)
                with sqlite3.connect(data / "oc.sqlite") as database:
                    assert database.execute("SELECT output FROM tool_operations WHERE name='read'").fetchall() == [(original,)]
                    assert database.execute("SELECT result FROM conversation_turns ORDER BY rowid LIMIT 1").fetchone()[0] == raw
            return {"case": case, "status": "PASS", "requests": len(requests), "compaction_requests": len(compactions)}
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
    parser.add_argument("--case", choices=CASES)
    args = parser.parse_args()
    binary = args.binary.resolve()
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    failures = 0
    for case in [args.case] if args.case else CASES:
        try:
            result = run(binary, case)
        except Exception as error:
            failures += 1
            result = {"case": case, "status": "FAIL", "error": str(error)}
        print(json.dumps({**result, "elf_sha256": digest, "owned_cleanup": "joined"}))
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
