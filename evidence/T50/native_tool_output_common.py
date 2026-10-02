#!/usr/bin/env python3
"""FIRST R10 common atomic: normal ELF, owned offline Responses, joined IO.

No paid endpoint, real HOME or authoring config. Bounded facts only are printed.
The skill remains below its existing 1MiB snapshot cap. The large shell proof
uses ONLY the retained legacy per-stream prefixes, marked ProducerLimited;
the distant sentinel is in retained stderr beyond stdout's 1MiB byte prefix.
It is NOT full shell producer capture.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import signal
import sqlite3
import subprocess
import tempfile
import threading


def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-common-") as temporary:
        root = Path(temporary)
        home, project, data = root / "home", root / "project", root / "data"
        config = home / "config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        destination=root/"destination"
        destination.mkdir()
        current_project=project
        skill = config / "skills/large/SKILL.md"
        skill.parent.mkdir(parents=True)
        payload = ("---\nname: large\ndescription: Synthetic admitted common result\n---\n"
                   "SYNTHETIC_COMMON_KEY\n" + "é ordinary padded line 012345678901234567890123456789012345678901234567890123456789\n" * 5000 + "DISTANT_SENTINEL 二\nnext line\n")
        skill.write_text(payload)
        distant = payload.splitlines().index("DISTANT_SENTINEL 二") + 1
        padding="é ordinary padded line 012345678901234567890123456789012345678901234567890123456789\n"
        # Keep the command bounded; repetitions occur inside the owned producer.
        legacy="python3 -c \"import sys;sys.stdout.write('SYNTHETIC_COMMON_KEY\\n'+'x'*2097152);sys.stderr.write("+repr(padding)+"*1000+'DISTANT_SENTINEL 二\\n'+"+repr(padding)+"*5000+'next line\\n')\""
        requests, failures, outputs, auxiliary = [], [], {}, []
        phase, step, reference = "chain", 0, None
        process = None

        def call(name, arguments, call_id):
            return {"type": "function_call", "id": "item-" + call_id,
                    "call_id": call_id, "name": name, "arguments": json.dumps(arguments), "status": "completed"}

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)

            def log_message(self, *_):
                pass

            def do_POST(self):
                nonlocal step, reference, distant
                try:
                    length = int(self.headers.get("content-length", "0"))
                    assert 0 < length <= 2097152, "request exceeded projection ceiling"
                    request = json.loads(self.rfile.read(length))
                    if request.get("max_output_tokens")==256 and not request.get("tools"):
                        auxiliary.append({"phase":phase,"bytes":length})
                        assert sum(row["phase"]==phase for row in auxiliary)<=1,"unexpected auxiliary retry"
                        assert len(auxiliary)<=5,"unexpected auxiliary request count"
                        self.send_response(200)
                        self.send_header("Content-Type","text/event-stream")
                        self.end_headers()
                        self.wfile.write(b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
                        return
                    requests.append({"phase": phase, "bytes": length, "calls": [i.get("call_id") for i in request["input"] if i.get("type") == "function_call_output"]})
                    for item in request["input"]:
                        if item.get("role")=="system":assert "DISTANT_SENTINEL" not in json.dumps(item),"artifact promoted to system authority"
                        if item.get("type") == "function_call_output":
                            text = item["output"]
                            assert isinstance(text, str) and len(text.encode()) <= 65536, "unbounded tool projection"
                            assert "SYNTHETIC_COMMON_KEY" not in text, "secret in hot projection"
                            outputs[item["call_id"]] = text
                    items = []
                    if phase == "chain":
                        if step == 0:
                            with sqlite3.connect(data / "oc.sqlite") as db:
                                if case == "fault_effect":
                                    db.execute("CREATE TRIGGER fail_common_publication BEFORE UPDATE OF name ON tool_output_resources BEGIN SELECT RAISE(ABORT,'directed fixture failure'); END")
                                elif case == "quota":
                                    orphan = data / "blobs/owned-common-orphan"
                                    with orphan.open("wb") as stream:
                                        stream.truncate(2 * 1024 * 1024 * 1024)
                            if case == "fault_effect":
                                items = [call("shell", {"command": "printf 'effect\\n' >> effect.txt; python3 -c \"print('x'*70000)\"; exit 17"}, "effect")]
                            elif case=="chain":
                                items = [call("shell", {"command":legacy}, "large")]
                            else:
                                items = [call("skill", {"id": "large"}, "large")]
                        elif case == "fault_effect":
                            assert "no usable path" in outputs["effect"], "publication failure advertised a path"
                            assert (project / "effect.txt").read_text() == "effect\n", "effect replay/loss"
                        elif step == 1:
                            text = outputs["large"]
                            assert "[tool output:" in text and "DISTANT_SENTINEL" not in text
                            match = re.search(r"read\(path=(\"[^\"]+\")", text)
                            assert match, "no actionable registered reference"
                            reference = json.loads(match[1])
                            with sqlite3.connect(data / "oc.sqlite") as db:
                                descriptor, extent, state = db.execute("SELECT descriptor,extent,state FROM tool_output_resources").fetchone()
                            descriptor = json.loads(descriptor)
                            assert descriptor["path"] == reference and descriptor["bytes"] == extent
                            assert Path(reference).is_file() and Path(reference).stat().st_size == extent
                            if case == "quota":
                                assert state == "Quota" and extent == 0 and "capture Quota" in text
                            else:
                                assert state == ("ProducerLimited" if case=="chain" else "Complete")
                                assert extent > (1048576 if case=="chain" else 65536)
                                with Path(reference).open("rb") as stream:
                                    assert b"SYNTHETIC_COMMON_KEY" not in stream.read(4096)
                                if case=="chain":
                                    with Path(reference).open(encoding="utf-8") as stream:
                                        distant=next(n for n,line in enumerate(stream,1) if "DISTANT_SENTINEL" in line)
                                assert descriptor["generation"] >= 1 and descriptor["source"] != "native defaults"
                                items = [call("read", {"path": reference, "offset": distant, "limit": 1}, "distant-read")]
                        elif step == 2:
                            assert "DISTANT_SENTINEL 二" in outputs["distant-read"] and f"next_offset Some({distant+1})" in outputs["distant-read"]
                            items = [call("grep", {"path": reference, "pattern": "DISTANT_SENTINEL", "literal": True}, "distant-grep")]
                        elif step == 3:
                            assert outputs["distant-grep"].startswith("{"), "distant grep failed: "+outputs["distant-grep"][:400]
                            result = json.loads(outputs["distant-grep"])
                            assert result["matches"][0]["line"] == distant and result["source"]["validated"] is True
                            items = [call("read", {"path": str(data / "oc.sqlite")}, "sqlite-deny"),
                                     call("glob", {"path": str(data / "tool-output"), "pattern": "**/*"}, "glob-deny"),
                                     call("write", {"path": reference, "content": "UNAUTHORIZED"}, "mutation-deny")]
                        elif step == 4:
                            assert all(outputs[key].startswith("error:") for key in ("sqlite-deny", "glob-deny", "mutation-deny")), "own-root boundary bypass"
                    elif phase in ("restart","moved"):
                        label="restart-read" if phase=="restart" else "moved-read"
                        if step == 0:
                            items = [call("read", {"path": reference, "offset": distant, "limit": 1}, label)]
                        else:
                            assert "DISTANT_SENTINEL" in outputs[label]
                    elif phase=="move":
                        if step==0:items=[call("opencode_session_move",{"directory":str(destination)},"move")]
                        else:
                            assert outputs["move"].startswith("{"),"move failed: "+outputs["move"][:400]
                            assert json.loads(outputs["move"])["status"]=="pending"
                    elif phase == "deny":
                        if step == 0:
                            items = [call("grep", {"path": reference, "pattern": "DISTANT_SENTINEL", "literal": True}, "deny-grep")]
                        else:
                            assert "error:" in outputs["deny-grep"] and "DISTANT_SENTINEL 二" not in outputs["deny-grep"], "current read Deny lost"
                    step += 1
                    if not items:
                        items = [{"type": "message", "id": "done", "role": "assistant", "content": [{"type": "output_text", "text": "DONE"}]}]
                    events = []
                    for item in items:
                        events += [{"type": "response.output_item.added", "item": item}, {"type": "response.output_item.done", "item": item}]
                    events.append({"type": "response.completed", "response": {"status": "completed", "output": items}})
                    self.send_response(200)
                    self.send_header("Content-Type", "text/event-stream")
                    self.end_headers()
                    for event in events:
                        self.wfile.write(("data: " + json.dumps(event) + "\n\n").encode())
                    self.wfile.flush()
                except Exception as error:
                    failures.append(f"{phase}/{step}: {str(error)[:1000]}")
                    self.send_error(400,"owned fixture assertion failed")
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        configuration = {"model": "fixture/common-model", "compaction": {"auto": False}, "agent": {"title": {"disable": True}},
                         "tool_output": {"max_lines": 20, "max_bytes": 4096},
                         "permission": {"skill": "allow", "read": "allow", "grep": "allow", "glob": "allow", "write": "allow", "shell": "allow","opencode_session_move":"allow", "external_directory":{str(destination):"allow"}},
                         "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {"baseURL": f"http://127.0.0.1:{server.server_port}/v1", "apiKey": "SYNTHETIC_COMMON_KEY"},
                                                     "models": {"common-model": {"limit": {"context": 1048576, "input": 1048576, "output": 2048}}}}}}
        (config / "opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME": str(home), "XDG_CONFIG_HOME": str(home / "config"), "PATH": "/usr/bin:/bin", "OC_TEST_ALLOW_LOOPBACK": "1"}
        thread.start()

        def invoke(prompt):
            nonlocal process
            process = subprocess.Popen([str(binary), "--data-dir", str(data), "run", "--json", "--session", "common-root", prompt],
                                       cwd=current_project, env=environment, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            try:
                stdout, stderr = process.communicate(timeout=30)
            except subprocess.TimeoutExpired as error:
                raise AssertionError({"native_timeout_seconds":30,"peer_failures":failures}) from error
            assert not failures, failures
            assert process.returncode == 0, (process.returncode, stderr[-1000:].decode(errors="replace"))
            assert len(stdout) + len(stderr) < 1048576 and not failures, failures

        try:
            invoke("Exercise admitted common result")
            if case in ("chain","head"):
                phase, step = "restart", 0
                invoke("Read registered prefix after real restart")
                phase,step="move",0
                invoke("Move same session at full terminal boundary")
                current_project=destination
                phase,step="moved",0
                invoke("Read original registered artifact after same-ID move")
                configuration["permission"]["read"] = "deny"
                (config / "opencode.json").write_text(json.dumps(configuration))
                phase, step = "deny", 0
                invoke("Verify current read Deny on exact artifact")
            with sqlite3.connect(data / "oc.sqlite") as db:
                resources = db.execute("SELECT extent,state FROM tool_output_resources").fetchall()
                rows = db.execute("SELECT name,state,length(CAST(output AS BLOB)) FROM tool_operations ORDER BY rowid").fetchall()
                logging = db.execute("SELECT count(*) FROM events WHERE kind='tool_output_execution'").fetchone()[0]
                if case == "fault_effect":
                    outcome = json.loads(db.execute("SELECT outcome FROM shell_jobs").fetchone()[0])
                    assert outcome["exit"] == 17 and outcome["state"] == "failed" and logging >= 1
                elif case == "quota":
                    assert rows[0][1] == "failed" and logging == 1
                else:
                    assert len(resources) == 1, "recursive duplicate archive"
                    descriptor=json.loads(db.execute("SELECT descriptor FROM tool_output_resources").fetchone()[0])
                    assert descriptor["location"]==str(project),"move rewrote capture provenance"
            return {"case": case, "status": "PASS", "provider_requests": len(requests)+len(auxiliary),"main_requests":len(requests),"auxiliary_requests":len(auxiliary), "request_bytes": [r["bytes"] for r in requests],
                    "native_tool_rows": rows, "resources": resources, "logging_failure_facts": logging,
                    "retained_resource_bytes": sum(r[0] for r in resources), "shell_full_producer": "NEXT"}
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            assert not thread.is_alive(), "owned server failed to join"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--case", choices=("chain", "head", "quota", "fault_effect"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    before = digest(binary)
    failed = 0
    for case in [args.case] if args.case else ("chain", "head", "quota", "fault_effect"):
        try:
            result = run(binary, case)
        except Exception as error:
            failed += 1
            result = {"case": case, "status": "FAIL", "error": str(error)[:1500]}
        print(json.dumps(result, ensure_ascii=False))
    after = digest(binary)
    assert before == after
    print(json.dumps({"binary": str(binary), "sha256_before": before, "sha256_after": after, "failures": failed,
                      "owned_cleanup": "all processes/HTTP handler threads joined before exact TempDir cleanup"}))
    raise SystemExit(bool(failed))


if __name__ == "__main__":
    main()
