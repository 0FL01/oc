#!/usr/bin/env python3
"""TOOL20/R1 direct normal ELF; typed wire/DB/bytes, isolated joined fake peers."""
import argparse
import fcntl
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import threading
import time

from native_question import read_until, stop_owned, tty_command

CASES = ("write", "edit", "invalid", "patch", "excluded_patch", "excluded_text",
         "deny", "ask_absent", "ask_once", "ask_stale", "ask_cancel", "child",
         "child_deny", "protected_path", "protected_content", "home", "reopen",
         "intent_failure", "finish_failure", "symlink", "data_root", "absolute_outside")


def send(handler, calls):
    handler.send_response(200)
    handler.send_header("Content-Type", "text/event-stream")
    handler.end_headers()
    items = []
    for call_id, name, args in calls:
        item = {"type":"function_call", "id":"item-"+call_id, "call_id":call_id,
                "name":name, "arguments":json.dumps(args), "status":"completed"}
        items.append(item)
        handler.wfile.write(("data: "+json.dumps({"type":"response.output_item.done","item":item})+"\n\n").encode())
    if not items:
        items = [{"type":"message","role":"assistant","id":"done",
                  "content":[{"type":"output_text","text":"DONE"}]}]
    handler.wfile.write(("data: "+json.dumps({"type":"response.completed","response":{"status":"completed","output":items}})+"\n\n").encode())
    handler.wfile.flush()


def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-r9-") as temporary:
        root = Path(temporary)
        home, data = root/"home", root/"data"
        project, config = home/"project", home/"config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        target = project/"file"
        target.write_bytes(b"before\n")
        requests, failures, outputs = [], [], {}
        phase = 0
        process, master = None, None
        child = case in ("child","child_deny")
        patch = case in ("patch","excluded_text") or child
        model = "gpt-fixture" if patch else "text-fixture"
        args = {"path":"file","content":"after\n"}
        calls = [("mutate","write",args)]
        expected = b"after\n"
        if case == "write":
            target.write_bytes(b"\xef\xbb\xbfold\r\n")
            calls = [("overwrite","write",{"path":"file","content":"new\nEOF"}),
                     ("create","write",{"path":"nested/new","content":"\ufeff\u03bb\r\nEOF"}),
                     ("empty","write",{"path":"empty","content":""})]
            expected = b"\xef\xbb\xbfnew\nEOF"
        elif case == "edit":
            target.write_bytes("\ufeff‘x’ 'x'\r\na  \r\nb\t\r\nx x\r\nEOF".encode())
            calls = [("exact","edit",{"path":"file","oldString":"'x'","newString":"y"}),
                     ("typography","edit",{"path":"file","oldString":"'x'","newString":"z"}),
                     ("lines","edit",{"path":"file","oldString":"a\nb","newString":"c\nd"}),
                     ("all","edit",{"path":"file","oldString":"x","newString":"","replaceAll":True})]
            expected = "\ufeffz y\r\nc\r\nd\r\n \r\nEOF".encode()
        elif case == "invalid":
            target.write_bytes(b"x x")
            calls = [("empty","edit",{"path":"file","oldString":"","newString":"y"}),
                     ("identical","edit",{"path":"file","oldString":"x","newString":"x"}),
                     ("missing","edit",{"path":"absent","oldString":"x","newString":"y"}),
                     ("no_match","edit",{"path":"file","oldString":"absent","newString":"y"}),
                     ("ambiguous","edit",{"path":"file","oldString":"x","newString":"y"})]
            expected = b"x x"
        elif case in ("patch","excluded_patch"):
            calls = [("mutate","apply_patch",{"patchText":"*** Begin Patch\n*** Update File: file\n@@\n-before\n+after\n*** End Patch"})]
        elif case == "protected_path":
            calls = [("mutate","write",{"path":"private/file","content":"after"})]
        elif case == "protected_content":
            calls = [("mutate","write",{"path":"file","content":"private/secret\n"})]
            expected = b"private/secret\n"
        elif case == "home":
            calls = [("mutate","write",{"path":"~/project/file","content":"after\n"})]
        elif case in ("symlink","data_root","absolute_outside"):
            outside = root/"outside"
            outside.write_bytes(b"outside\n")
            if case=="symlink":
                (project/"link").symlink_to(outside)
            calls = [("mutate","write",{"path":"link" if case=="symlink" else str(data/"forbidden") if case=="data_root" else str(outside),"content":"unapproved"})]
        is_ask = case.startswith("ask_")
        negative = case in ("excluded_patch","excluded_text","deny","child_deny","protected_path","intent_failure","symlink","data_root","absolute_outside")
        if negative or case in ("ask_absent","ask_cancel"):
            expected = b"before\n"
        if case == "ask_stale":
            expected = b"foreign\n"

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)
            def log_message(self, *_):
                pass
            def do_POST(self):
                nonlocal phase
                try:
                    length = int(self.headers.get("content-length",0))
                    assert 0 < length < 2097152
                    request = json.loads(self.rfile.read(length))
                    assert self.path == "/v1/responses"
                    assert self.headers.get("authorization") == "Bearer SYNTHETIC_R9"
                    if not request.get("tools"):
                        assert request.get("max_output_tokens") == 256
                        send(self, [])
                        return
                    requests.append(request)
                    assert len(requests) <= 8
                    definitions = {tool["name"]:tool for tool in request["tools"]}
                    own_child = child and request["model"] == "text-fixture"
                    assert request["model"] == ("text-fixture" if own_child else model)
                    denied = case == "deny" or (own_child and case == "child_deny")
                    family = set(definitions) & {"write","edit","apply_patch"}
                    assert family == (set() if denied else {"edit","write"} if own_child or not patch else {"apply_patch"}), family
                    for name in family:
                        assert definitions[name]["parameters"]["additionalProperties"] is False
                    guidance = [item for item in request["input"] if item.get("role")=="developer" and "Current request file-mutation tools:" in json.dumps(item)]
                    assert bool(guidance) == bool(family)
                    incoming = [item for item in request["input"] if item.get("type")=="function_call_output"]
                    assert len({item["call_id"] for item in incoming}) == len(incoming)
                    for item in incoming:
                        outputs[item["call_id"]] = item["output"]
                    if phase == 99:
                        send(self, [])
                    elif phase == 0 and child:
                        phase = 1
                        send(self,[("spawn","subagent",{"agent":"helper","description":"Own model files","prompt":"mutate"})])
                    elif phase == 0 or (phase == 1 and own_child):
                        phase = 2
                        if case in ("intent_failure","finish_failure"):
                            with sqlite3.connect(data/"oc.sqlite") as db:
                                sql = "CREATE TRIGGER fault BEFORE INSERT ON tool_operations BEGIN SELECT RAISE(ABORT,'fixture'); END" if case=="intent_failure" else "CREATE TRIGGER fault BEFORE UPDATE OF state ON tool_operations WHEN NEW.state='completed' BEGIN SELECT RAISE(ABORT,'fixture'); END"
                                db.execute(sql)
                        send(self,calls)
                    else:
                        send(self,[])
                except Exception as error:
                    failures.append(repr(error))
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1",0),Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        configuration = {"model":"fixture/"+model,"compaction":{"auto":False},
            "permission":{"apply_patch":"deny" if case=="deny" else "ask" if is_ask else "allow","subagent":"allow"},
            "agent":{"title":{"disable":True},"helper":{"mode":"subagent","model":"fixture/text-fixture","permission":{"apply_patch":"deny" if case=="child_deny" else "allow"}}},
            "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":f"http://127.0.0.1:{server.server_port}/v1","apiKey":"SYNTHETIC_R9"},
                "models":{name:{"limit":{"context":1048576,"output":4096}} for name in ("gpt-fixture","text-fixture")}}}}
        if case.startswith("protected_"):
            configuration["permission"]["apply_patch"] = {"*":"allow","private/**":"deny"}
            configuration["dcp"] = {"protectedFilePatterns":["private/**"]}
        (config/"opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME":str(home),"XDG_CONFIG_HOME":str(home/"config"),"PATH":"/usr/bin:/bin","TERM":"xterm-256color","OC_TEST_ALLOW_LOOPBACK":"1"}
        thread.start()
        command = [str(binary),"--data-dir",str(data),"run","--json","--session","root","mutate"]
        def invoke():
            nonlocal process
            process = subprocess.Popen(command,cwd=project,env=environment,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
            out,err = process.communicate(timeout=15)
            assert len(out)+len(err)<1048576
            return process.returncode
        try:
            if case in ("ask_once","ask_stale","ask_cancel"):
                master,slave = pty.openpty()
                fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",36,120,0,0))
                process = subprocess.Popen(tty_command([str(binary),"--data-dir",str(data),"tui","--session","root"]),cwd=project,env=environment,stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
                os.close(slave)
                read_until(process,master,b"Untitled session")
                os.write(master,b"mutate\r")
                read_until(process,master,b"Allow once")
                with sqlite3.connect(data/"oc.sqlite") as db:
                    assert db.execute("SELECT count(*) FROM tool_operations").fetchone()[0]==0
                assert target.read_bytes()==b"before\n"
                if case=="ask_stale":
                    target.write_bytes(b"foreign\n")
                os.write(master,b"\x1b" if case=="ask_cancel" else b"\r")
                deadline = time.monotonic()+8
                while time.monotonic()<deadline:
                    if select.select([master],[],[],0.05)[0]:
                        os.read(master,65536)
                    with sqlite3.connect(data/"oc.sqlite") as db:
                        if db.execute("SELECT count(*) FROM turns WHERE status!='started'").fetchone()[0]:
                            break
                else:
                    raise AssertionError("approval did not settle")
                os.write(master,b"\x03")
                process.wait(timeout=5)
                assert process.returncode==0
            else:
                code = invoke()
                assert code==(1 if case in ("ask_absent","intent_failure","finish_failure") else 0), (case,code,failures)
            assert not failures,failures
            assert target.read_bytes()==expected,(case,target.read_bytes(),expected,outputs.get("mutate"))
            if case=="write":
                assert (project/"nested/new").read_bytes()=="\ufeff\u03bb\r\nEOF".encode()
                assert (project/"empty").read_bytes()==b""
                assert json.loads(outputs["overwrite"])["existed"] is True
            if case=="edit":
                assert [json.loads(outputs[key])["replacements"] for key in ("exact","typography","lines","all")]==[1,1,1,2]
            if case=="protected_path":
                assert not (project/"private").exists()
            if case in ("symlink","data_root","absolute_outside"):
                assert outside.read_bytes()==b"outside\n" and not (data/"forbidden").exists()
            with sqlite3.connect(data/"oc.sqlite") as db:
                rows = db.execute("SELECT name,state,output,id FROM tool_operations ORDER BY rowid").fetchall()
                metadata = dict(db.execute("SELECT op_id,metadata FROM patch_effects"))
                raw = db.execute("SELECT result FROM conversation_turns ORDER BY rowid").fetchall()
            if case in ("ask_absent","intent_failure"):
                assert not rows and not metadata
            elif case=="finish_failure":
                assert len(rows)==1 and rows[0][1]=="started" and not metadata
            else:
                mutations = [row for row in rows if row[0] in ("edit","write","apply_patch")]
                assert len(mutations)==len(calls)
                failed = negative or case in ("invalid","ask_stale","ask_cancel")
                for row in mutations:
                    assert (row[1]!="completed")==failed,row
                    if not failed:
                        assert json.loads(metadata[row[3]])["total_files"]==1
                    elif row[3] in metadata:
                        assert json.loads(metadata[row[3]])["files"]==[]
                    if case!="ask_cancel":
                        call_id = next(key for key,name,_ in calls if name==row[0] and outputs.get(key)==row[2])
                        assert outputs[call_id]==row[2]
                    else:
                        assert json.loads(row[2])["status"] in ("permission_rejected","permission_cancelled")
            if case in ("reopen","finish_failure"):
                if case=="finish_failure":
                    with sqlite3.connect(data/"oc.sqlite") as db:
                        db.execute("DROP TRIGGER fault")
                target.write_bytes(b"present-day\n")
                phase = 99
                count = len(requests)
                assert invoke()==0 and len(requests)==count+1 and not failures
                assert target.read_bytes()==b"present-day\n"
                with sqlite3.connect(data/"oc.sqlite") as db:
                    current = db.execute("SELECT name,state,output,id FROM tool_operations ORDER BY rowid").fetchall()
                    if case=="reopen":
                        assert current==rows
                        assert db.execute("SELECT result FROM conversation_turns ORDER BY rowid LIMIT 1").fetchone()==raw[0]
                    else:
                        assert len(current)==1 and current[0][1]=="unknown"
                if case=="reopen":
                    count = len(requests)
                    master,slave = pty.openpty()
                    fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",36,120,0,0))
                    process = subprocess.Popen(tty_command([str(binary),"--data-dir",str(data),"tui","--session","root"]),cwd=project,env=environment,stdin=slave,stdout=slave,stderr=slave,start_new_session=True)
                    os.close(slave)
                    read_until(process,master,b"# Wrote")
                    assert len(requests)==count and target.read_bytes()==b"present-day\n"
                    os.write(master,b"\x03")
                    process.wait(timeout=5)
                    assert process.returncode==0
            return {"case":case,"status":"PASS","requests":len(requests),"operations":len(rows)}
        finally:
            stop_owned(process)
            if master is not None:
                os.close(master)
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            assert not thread.is_alive()


def main():
    assert os.getuid()!=0
    parser = argparse.ArgumentParser()
    parser.add_argument("binary",type=Path)
    parser.add_argument("--case",choices=CASES)
    args = parser.parse_args()
    binary = args.binary.resolve()
    with binary.open("rb") as stream:
        digest = hashlib.file_digest(stream,"sha256").hexdigest()
    failures = 0
    for case in [args.case] if args.case else CASES:
        try:
            result = run(binary,case)
        except Exception as error:
            failures += 1
            result = {"case":case,"status":"FAIL","error":repr(error)}
        print(json.dumps({**result,"elf_sha256":digest,"owned_cleanup":"joined"}))
    with binary.open("rb") as stream:
        assert hashlib.file_digest(stream,"sha256").hexdigest()==digest
    raise SystemExit(1 if failures else 0)


if __name__=="__main__":
    main()
