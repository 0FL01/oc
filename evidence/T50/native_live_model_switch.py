#!/usr/bin/env python3
"""TOOL12 native barriers; real composer + byte/durable/wire receipts, fake only."""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import sqlite3
import signal
import threading
import time
import traceback

from native_background import Native, until
from native_session_move import painted_screen

CASES = ("stream", "variant", "tool", "ask", "retry", "compact", "child")
A, B, CHILD = "gpt-fixture", "text-fixture", "child-fixture"
ANY_VARIANT = object()


def item(call, name, arguments):
    return {"type":"function_call", "id":"item-"+call, "call_id":call,
            "name":name, "arguments":json.dumps(arguments), "status":"completed"}


def event(handler, value):
    handler.wfile.write(("data: " + json.dumps(value) + "\n\n").encode())
    handler.wfile.flush()


def finish(handler, outputs):
    event(handler, {"type":"response.completed", "response":{"status":"completed", "output":outputs}})


def message(text):
    return {"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}


class SwitchNative(Native):
    def __init__(self, binary, case, shell_capture=False):
        super().__init__(binary, lambda *_: [])
        self.case, self.gates = case, [threading.Event() for _ in range(4)]
        self.arrived = [threading.Event() for _ in range(4)]
        self.summaries = []
        self.shell_capture=shell_capture
        self.shell_compact=False
        self.auxiliary = []
        self.stages = []
        owner = self

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def do_POST(self):
                with owner.request_counter_lock:
                    owner.physical_requests+=1
                try:
                    size = int(self.headers.get("content-length", 0))
                    assert 0 < size <= 1048576
                    request = json.loads(self.rfile.read(size))
                    assert self.path == "/v1/responses"
                    assert self.headers.get("authorization") == "Bearer SYNTHETIC_SWITCH"
                    if not request.get("tools"):
                        if request.get("max_output_tokens") == 256:
                            owner.auxiliary.append(request)
                            assert len(owner.auxiliary) <= 1
                            self.send_response(200)
                            self.send_header("Content-Type", "text/event-stream")
                            self.end_headers()
                            finish(self, [message("Synthetic title")])
                            return
                        if owner.shell_capture and owner.shell_compact:
                            assert len(json.dumps(request).encode())<131072,"cold shell rehydrated for compact"
                            # The immutable shell command itself names the distant
                            # sentinel. Measure repeated body facts instead of
                            # mistaking that legitimate call argument for rehydration.
                            assert json.dumps(request).count("ordinary padded shell line")<200,"cold shell body rehydrated"
                            owner.summaries.append(request)
                            assert len(owner.summaries)==1
                            self.send_response(200)
                            self.send_header("Content-Type","text/event-stream")
                            self.end_headers()
                            finish(self,[message("## Work State\nShell executed once; bounded registered reference remains in raw history.")])
                            return
                        assert case == "compact", "unexpected auxiliary generation"
                        owner.summaries.append(request)
                        assert len(owner.summaries) == 1 and request["model"] == B
                        assert "A-opaque" not in json.dumps(request)
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        owner.arrived[2].set()
                        assert owner.gates[2].wait(10), "summary barrier"
                        finish(self, [message("## Work State\nPrior ordinary exchange is closed.")])
                        return
                    owner.requests.append(request)
                    step = len(owner.requests)
                    assert step <= 6
                    definitions = {tool["name"]:tool for tool in request["tools"]}
                    family = set(definitions) & {"apply_patch", "edit", "write"}
                    assert family == ({"apply_patch"} if request["model"] == A else {"edit", "write"})
                    guidance = [entry for entry in request["input"] if entry.get("role") == "developer" and
                                "Current request file-mutation tools:" in json.dumps(entry)]
                    assert len(guidance) == 1
                    assert request["max_output_tokens"] == (2048 if request["model"] == A else 1536)
                    for name in family:
                        assert definitions[name]["parameters"]["additionalProperties"] is False
                    outputs = [entry for entry in request["input"] if entry.get("type") == "function_call_output"]
                    assert len({entry["call_id"] for entry in outputs}) == len(outputs)
                    if case == "compact" and step == 1:
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        finish(self, [message("PRELUDE " + "closed ordinary context " * 30)])
                        return
                    if case == "child":
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        if step == 1:
                            assert request["model"] == A
                            finish(self, [item("spawn", "subagent", {"agent":"helper", "description":"Owned child", "prompt":"Write child file"})])
                        elif step == 2:
                            assert request["model"] == CHILD
                            owner.arrived[0].set()
                            assert owner.gates[0].wait(10), "child barrier"
                            finish(self, [item("child-write", "write", {"path":"child-file", "content":"child-once\n"})])
                        elif step == 3:
                            assert request["model"] == CHILD and any(o["call_id"] == "child-write" for o in outputs)
                            finish(self, [message("child complete")])
                        else:
                            assert step == 4 and request["model"] == A and any(o["call_id"] == "spawn" for o in outputs)
                            finish(self, [message("parent complete")])
                        return
                    first = step == (2 if case == "compact" else 1)
                    if first:
                        assert request["model"] == A
                        if case == "retry":
                            owner.arrived[0].set()
                            assert owner.gates[0].wait(10), "retry failure barrier"
                            self.send_response(503)
                            self.send_header("Content-Type", "application/json")
                            self.send_header("Retry-After", "1")
                            self.end_headers()
                            self.wfile.write(b'{"error":{"message":"synthetic overload","code":"server_error"}}')
                            return
                        if case == "compact":
                            owner.arrived[0].set()
                            assert owner.gates[0].wait(10), "overflow barrier"
                            self.send_response(400)
                            self.send_header("Content-Type", "application/json")
                            self.end_headers()
                            self.wfile.write(b'{"error":{"message":"context length exceeded","code":"context_length_exceeded"}}')
                            return
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        event(self, {"type":"response.output_text.delta", "delta":"A prepared"})
                        owner.arrived[0].set()
                        if case != "tool":
                            assert owner.gates[0].wait(10), "A stream barrier"
                        calls = [item("a-patch", "apply_patch", {"patchText":"*** Begin Patch\n*** Add File: file\n+once\n*** End Patch"}),
                                 item("a-excluded", "write", {"path":"unapproved-a", "content":"bad"})]
                        if case == "tool":
                            held="touch tool-started; while [ ! -f tool-release ]; do sleep .01; done"
                            if owner.shell_capture:
                                held="printf one >> shell.effect; python3 -c \"import sys;sys.stdout.write(('ordinary padded shell line '*4+'\\n')*12000+'SHELL_DISTANT_SENTINEL\\n'+('ordinary padded shell line '*4+'\\n')*12000)\"; touch tool-started; while [ ! -f tool-release ]; do sleep .01; done; printf final-shell-flush"
                            calls.append(item("held-shell", "shell", {"command":held}))
                        finish(self, [{"type":"reasoning", "id":"a-reasoning", "encrypted_content":"A-opaque", "summary":[]}, message("A prepared"), *calls])
                    elif case == "compact":
                        assert step == 3 and request["model"] == A and len(owner.summaries) == 1
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        finish(self, [message("rebuilt latest A")])
                    elif step == 2:
                        assert request["model"] == B
                        if case == "variant":
                            assert request.get("reasoning",{}).get("effort") == "low"
                        if case != "retry":
                            assert any(o["call_id"] == "a-patch" for o in outputs)
                            assert any(o["call_id"] == "a-excluded" and o["output"].startswith("error:") for o in outputs)
                            assert "A-opaque" not in json.dumps(request)
                            assert owner.project.joinpath("file").read_bytes() == b"once\n"
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        event(self, {"type":"response.output_text.delta", "delta":"B prepared"})
                        owner.arrived[1].set()
                        assert owner.gates[1].wait(10), "B stream barrier"
                        call = item("b-edit", "write" if case == "retry" else "edit",
                                    {"path":"file", "content":"twice\n"} if case == "retry" else {"path":"file","oldString":"once","newString":"twice"})
                        finish(self, [{"type":"reasoning", "id":"b-reasoning", "encrypted_content":"B-opaque", "summary":[]}, message("B prepared"), call,
                                      item("b-excluded", "apply_patch", {"patchText":"*** Begin Patch\n*** Add File: unapproved-b\n+bad\n*** End Patch"})])
                    elif case == "variant" and step == 3:
                        assert request["model"] == B and not request.get("reasoning",{}).get("effort")
                        assert "B-opaque" in json.dumps(request)
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        owner.arrived[3].set()
                        assert owner.gates[3].wait(10), "Default variant barrier"
                        finish(self,[message("B Default prepared"),item("variant-read","read",{"path":"file"})])
                    else:
                        assert step in (3,4)
                        assert request["model"] == (B if step == 4 and case != "variant" else A)
                        assert any(o["call_id"] == "b-edit" for o in outputs)
                        assert "B-opaque" not in json.dumps(request) if request["model"] == A else True
                        if case == "variant":
                            assert any(o["call_id"] == "variant-read" for o in outputs)
                        self.send_response(200)
                        self.send_header("Content-Type", "text/event-stream")
                        self.end_headers()
                        finish(self, [message("A finished" if step == 3 else "Restart latest B")])
                except (BrokenPipeError, ConnectionResetError):
                    owner.errors.append("unexpected connection loss")
                except Exception as error:
                    owner.errors.append(str(error))
                finally:
                    self.close_connection = True

        self.server.RequestHandlerClass = Peer
        config = {
            "model":"fixture/"+A, "compaction":{"auto":case == "compact","buffer":0},
            "plugin":["@tarquinen/opencode-dcp"],
            "dcp":{"manualMode":True,"automaticStrategies":False,"compress":{"minContextLimit":"10%","maxContextLimit":"90%"}},
            "permission":{"*":"allow", "apply_patch":"ask" if case == "ask" else "allow"},
            "agent":{"title":{"disable":True},"helper":{"mode":"subagent","model":"fixture/"+CHILD,"permission":{"*":"allow"}}},
            "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":f"http://127.0.0.1:{self.server.server_port}/v1", "apiKey":"SYNTHETIC_SWITCH"},
                "models":{A:{"name":"A Patch","limit":{"context":65536,"output":2048},"variants":{"low":{"reasoningEffort":"low"}}},
                          B:{"name":"B Text","limit":{"context":32768,"output":1536},"variants":{"low":{"reasoningEffort":"low"}}},
                          CHILD:{"limit":{"context":32768,"output":1536}}}}}
        }
        if shell_capture:
            config["tool_output"]={"max_lines":20,"max_bytes":4096}
        (self.home/"config/opencode/opencode.json").write_text(json.dumps(config))

    def __exit__(self, *args):
        for gate in self.gates:
            gate.set()
        self.project.joinpath("tool-release").touch()
        cleanup_error = None
        try:
            self.stop()
        except Exception as error:
            cleanup_error = error
            if self.process is not None:
                if self.process.poll() is None:
                    pid, group, ticks = self.owned_process
                    current = Path(f"/proc/{pid}/stat").read_text().rsplit(")",1)[1].split()[19]
                    assert ticks == current and os.getpgid(pid) == group == pid
                    os.killpg(group, signal.SIGKILL)
                self.process.wait(timeout=3)
                self.reader.join(timeout=1)
                assert not self.reader.is_alive()
                os.close(self.fd)
                self.process = None
        finally:
            self.server.shutdown()
            self.server.server_close()
            self.thread.join(timeout=2)
            assert not self.thread.is_alive()
            self.temp.cleanup()
        if cleanup_error is not None and args[0] is None:
            raise cleanup_error

    def start(self):
        try:
            super().start()
        finally:
            if self.process is not None and self.process.poll() is None:
                pid = self.process.pid
                self.owned_process = (pid, os.getpgid(pid), Path(f"/proc/{pid}/stat").read_text().rsplit(")",1)[1].split()[19])

    def owner_choice(self):
        key = "tui.selection.session:"+json.dumps([str(self.project.resolve()),"fixture","t50-background"],separators=(",",":"))
        rows = self.rows("SELECT value FROM prefs WHERE key=?", (key,))
        return json.loads(rows[0][0])["models"]["build"] if rows else {"id":A,"variant":None}

    def owner(self):
        return self.owner_choice()["id"]

    def commits(self):
        return self.rows("SELECT count(*) FROM events WHERE kind='session_model_selected'")[0][0]

    def choose(self, model):
        self.tail.clear()
        self.send(b"/model\r")
        until(lambda:"Select model" in painted_screen(self), "busy picker unavailable")
        self.send(("A Patch" if model == A else "B Text").encode()+b"\r")
        until(lambda:"Select variant" in painted_screen(self) or
              ("A Patch" if model == A else "B Text") in "\n".join(painted_screen(self).splitlines()[-4:]), "local picker draft absent")
        if "Select variant" in painted_screen(self):
            self.send(b"\x1b")
            until(lambda:"Select variant" not in painted_screen(self), "draft variant dialog did not close")

    def commit(self, model, variant=ANY_VARIANT):
        before = (len(self.requests),len(self.auxiliary),len(self.summaries))
        self.send(b"\r")
        until(lambda:self.owner() == model and (variant is ANY_VARIANT or self.owner_choice().get("variant") == variant), "busy blank Enter not committed")
        assert (len(self.requests),len(self.auxiliary),len(self.summaries)) == before, "commit dispatched an extra request"

    def approve(self):
        until(lambda:b"Allow" in self.tail and b"permission" in self.tail.lower(), "real prepared permission absent")
        self.tail.clear()
        self.send(b"\r")


def run(binary, case):
    with SwitchNative(binary, case) as native:
        def settled(count=1):
            return native.rows("SELECT count(*) FROM turns WHERE session_id='t50-background' AND status='completed'") == [(count,)]
        native.start()
        if case == "compact":
            native.send(b"Prior real exchange\r")
            until(settled, "prelude not completed")
            until(lambda:b"A Patch" in native.tail and b"PRELUDE" in native.tail, "prelude consumer not settled")
            time.sleep(.1)
        native.send(b"One working task\r")
        assert native.arrived[0].wait(10), "initial barrier absent"
        until(lambda:len(native.auxiliary) == 1, "initial title admission did not settle before commit oracle")
        if case == "tool":
            until(lambda:native.project.joinpath("tool-started").exists(), "real shell barrier absent")
        before = len(native.requests)
        assert native.owner() == A
        native.choose(B)
        assert native.owner() == A and len(native.requests) == before, "picker selection committed"
        if case == "variant":
            native.tail.clear()
            native.send(b"\x14")
            until(lambda:len(native.tail)>0,"busy variant draft not painted")
            assert native.owner_choice() == {"id":A,"variant":None}
        native.commit(B,"low" if case == "variant" else ANY_VARIANT)
        assert native.rows("SELECT count(*) FROM messages WHERE session_id='t50-background' AND role='user'") == [(2 if case == "compact" else 1,)]
        native.send(b"\r")  # Identical committed choice is a true no-op.
        if case == "child":
            native.choose(A)
            assert native.owner() == B and len(native.requests) == before
            native.commit(A)
        native.gates[0].set()
        if case == "tool":
            native.project.joinpath("tool-release").touch()
        if case == "ask":
            native.approve()
        if case == "compact":
            assert native.arrived[2].wait(10), ("B summary absent",native.errors)
            native.choose(A)
            assert native.owner() == B and len(native.requests) == before
            native.commit(A)
            native.gates[2].set()
        elif case != "child":
            assert native.arrived[1].wait(10), ("next B primary absent",native.errors)
            if case == "variant":
                native.tail.clear()
                native.send(b"\x14")
                until(lambda:len(native.tail)>0,"Default variant draft not painted")
                assert native.owner_choice().get("variant") == "low"
                native.commit(B,None)
                native.gates[1].set()
                assert native.arrived[3].wait(10), ("very next Default request absent",native.errors)
            native.choose(A)
            assert native.owner() == B
            native.commit(A)
            native.gates[3 if case == "variant" else 1].set()
            if case == "ask":
                native.approve()
        count = 2 if case == "compact" else 1
        until(lambda:settled(count), ("working task not completed",native.errors))
        assert not native.errors, native.errors
        assert native.commits() == (3 if case == "variant" else 2)
        assert not native.project.joinpath("unapproved-a").exists()
        assert not native.project.joinpath("unapproved-b").exists()
        logs = [json.loads(row[0]) for row in native.rows("SELECT result FROM turns WHERE session_id='t50-background' AND result IS NOT NULL AND status='completed' ORDER BY rowid")]
        main = logs[-1]
        receipts = main["requests"]
        expected = [A,A] if case in ("compact","child") else [A,B,B,A] if case == "variant" else [A,B,A]
        assert [r["model"]["id"] for r in receipts] == expected, receipts
        assert all(r["context_fingerprint"] and r["tool_fingerprint"] and r["span"] for r in receipts)
        if case not in ("compact","child"):
            assert [r["output_limit"] for r in receipts] == ([2048,1536,1536,2048] if case == "variant" else [2048,1536,2048])
            assert [r["context_limit"] for r in receipts] == ([65536,32768,32768,65536] if case == "variant" else [65536,32768,65536])
            assert receipts[0]["tool_fingerprint"] == receipts[-1]["tool_fingerprint"] != receipts[1]["tool_fingerprint"]
            assert receipts[0]["dcp_max_context"] != receipts[1]["dcp_max_context"]
            assert native.project.joinpath("file").read_bytes() == b"twice\n"
        if case == "variant":
            assert [r["model"].get("variant") for r in receipts] == [None,"low",None,None]
            assert receipts[1]["model_label"].endswith("(low)") and not receipts[2]["model_label"].endswith("(low)")
        if case == "child":
            child = json.loads(native.rows("SELECT t.result FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.parent_id='t50-background' AND t.status='completed'")[0][0])
            assert [r["model"]["id"] for r in child["requests"]] == [CHILD,CHILD]
            assert native.project.joinpath("child-file").read_bytes() == b"child-once\n"
        if case == "compact":
            snapshots = [json.loads(row[0]) for row in native.rows("SELECT snapshot FROM session_compactions")]
            assert len(snapshots) == 1 and snapshots[0]["state"] == "completed" and snapshots[0]["model"]["id"] == B
        if case == "stream":
            native.stop()
            before = len(native.requests)
            native.project.joinpath("file").write_bytes(b"present-day-change\n")
            native.start()
            assert len(native.requests) == before
            until(lambda:b"A Patch" in native.tail and b"B Text" in native.tail, "per-request replay attribution absent")
            native.choose(B)
            assert native.owner() == A
            native.commit(B)
            assert len(native.requests) == before
            native.stop()
            native.start()
            assert native.owner() == B and len(native.requests) == before
            native.send(b"Restart current committed choice\r")
            until(lambda:settled(2), "reopened latest choice not used")
            assert native.requests[-1]["model"] == B
            assert native.project.joinpath("file").read_bytes() == b"present-day-change\n", "historical tool replayed"
        result = {"case":case,"status":"PASS","primary_requests":len(native.requests),"summaries":len(native.summaries),"auxiliary_requests":len(native.auxiliary),
                  "commits":native.commits(),"request_models":expected,"tool_rows":native.rows("SELECT count(*) FROM tool_operations")[0][0]}
    result["owned_cleanup"] = "all process/PTY/HTTP owners joined before exact TempDir cleanup"
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary",type=Path)
    parser.add_argument("--case",choices=CASES)
    options = parser.parse_args()
    binary = options.binary.resolve()
    before = hashlib.sha256(binary.read_bytes()).hexdigest()
    failures = 0
    for case in (options.case,) if options.case else CASES:
        try:
            result = run(binary, case)
        except Exception as error:
            failures += 1
            result = {"case":case,"status":"FAIL","error":str(error),"trace":traceback.format_exc(limit=2)}
        print(json.dumps(result))
    after = hashlib.sha256(binary.read_bytes()).hexdigest()
    assert before == after
    print(json.dumps({"binary":str(binary),"sha256_before":before,"sha256_after":after,"failures":failures}))
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
