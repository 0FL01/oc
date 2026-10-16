#!/usr/bin/env python3
"""TOOL18 normal ELF only: fake Responses/discovery, RAM captures, joined cleanup."""
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
import traceback

SESSION_MOVE_SUPPORTED = False

CASES = ("catalog", "dynamic", "rename", "explicit", "restart", "deny", "ask", "unknown", "foreign", "invalid",
         "child_own", "child_parent", "child_sibling", "child_foreign", "general", "bad_lookup",
         "models_deny", "models_ask", "child_models")

def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def run(binary, case):
    with tempfile.TemporaryDirectory(prefix="t50-r7-") as temporary:
        root = Path(temporary)
        home, project, data = root/"home", root/"project", root/"data"
        config = home/"config/opencode"
        config.mkdir(parents=True)
        project.mkdir()
        requests, gets, failures, auxiliary = [], [], [], []
        outputs = {}
        child_schema = {}
        baseline_titles = {}
        child = case.startswith("child_") or case == "general"
        own_provider = "ludka2" if case == "dynamic" else "own"
        sibling = None
        phase = 0
        process = None

        def send(handler, calls):
            handler.send_response(200)
            handler.send_header("Content-Type", "text/event-stream")
            handler.end_headers()
            items = []
            for call_id, name, args in calls:
                item = {"type":"function_call", "id":"item-"+call_id, "call_id":call_id,
                        "name":name, "arguments":json.dumps(args), "status":"completed"}
                items.append(item)
                for kind in ("response.output_item.added", "response.output_item.done"):
                    handler.wfile.write(("data: "+json.dumps({"type":kind,"item":item})+"\n\n").encode())
            if not items:
                items = [{"type":"message","id":"done","role":"assistant","content":[{"type":"output_text","text":"DONE"}]}]
            handler.wfile.write(("data: "+json.dumps({"type":"response.completed","response":{"status":"completed","output":items}})+"\n\n").encode())
            handler.wfile.flush()

        class Peer(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(3)
            def log_message(self, *_):
                pass
            def do_GET(self):
                try:
                    gets.append(self.path)
                    assert case == "dynamic" and self.path == "/v1/models"
                    payload = json.dumps({"object":"list","data":[{"id":"m","context_length":1048576,"max_completion_tokens":2048,
                        "opencode":{"name":"Discovered Main","variants":{"z-low":{"reasoningEffort":"low"},"a-high":{"reasoningEffort":"high"}}}},
                        {"id":"new-unknown","opencode":{"name":"Brand New"}},
                        {"id":"old"},{"id":"new"},{"id":"unknown"}]}).encode()
                    self.send_response(200)
                    self.send_header("Content-Type","application/json")
                    self.send_header("Content-Length",str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
                except Exception as error:
                    failures.append(repr(error))
                finally:
                    self.close_connection = True
            def do_POST(self):
                nonlocal phase, sibling
                try:
                    length = int(self.headers.get("content-length",0))
                    assert 0 < length < 2097152
                    request = json.loads(self.rfile.read(length))
                    if not request.get("tools") and request.get("max_output_tokens") == 256:
                        auxiliary.append(request)
                        self.send_response(200)
                        self.send_header("Content-Type","text/event-stream")
                        self.end_headers()
                        self.wfile.write(b'data: {"type":"response.completed","response":{"status":"completed","output":[]}}\n\n')
                        return
                    requests.append(request)
                    assert len(requests) <= 12
                    assert request["model"] == "m", ("model",request["model"])
                    assert request.get("reasoning",{}).get("effort") == "high", "selection/variant changed"
                    assert self.headers.get("authorization") == "Bearer SYNTHETIC_R7_KEY"
                    definitions = {t["name"]:t for t in request.get("tools",[])}
                    assert "bash" not in definitions
                    if case == "models_deny":
                        assert "opencode_models" not in definitions
                    else:
                        assert "opencode_models" in definitions, ("catalog",list(definitions))
                        props = definitions["opencode_models"]["parameters"]
                        assert props["additionalProperties"] is False and props["properties"]["limit"]["maximum"] == 100
                        assert props["properties"]["offset"]["minimum"] == 0 and props["properties"]["limit"]["default"] == 20
                    is_child_request = child and phase in (1,2) and not any(i.get("call_id")=="spawn" for i in request["input"])
                    assert ("opencode_session_move" in definitions) == (SESSION_MOVE_SUPPORTED and not (case == "general" and is_child_request))
                    if is_child_request:
                        child_schema.update({name:definition["parameters"] for name,definition in definitions.items()
                            if name in ("opencode_models","opencode_session_rename")})
                    if case == "deny" or (case == "general" and is_child_request):
                        assert "opencode_session_rename" not in definitions
                    else:
                        assert definitions["opencode_session_rename"]["parameters"]["required"] == ["title"]
                        assert definitions["opencode_session_rename"]["parameters"]["additionalProperties"] is False
                    incoming = [i for i in request["input"] if i.get("type") == "function_call_output"]
                    assert len({i["call_id"] for i in incoming}) == len(incoming), "duplicate result call ID"
                    for i in incoming:
                        outputs[i["call_id"]] = i["output"]
                        assert len(i["output"].encode()) <= 65536
                        assert "SYNTHETIC_R7_KEY" not in i["output"]
                    if phase == 99:
                        send(self, [])
                        return
                    if phase == 0 and child:
                        phase = 1
                        send(self, [("spawn", "subagent", {"agent":"general" if case == "general" else "helper","prompt":"perform fixture","description":"TOOL18 child"})])
                    elif (phase == 0 and not child) or (phase == 1 and child):
                        phase = 2
                        if child:
                            with sqlite3.connect(data/"oc.sqlite") as db:
                                current, = db.execute("SELECT id FROM sessions WHERE parent_id='root'").fetchone()
                                sibling = current+"-sibling"
                                db.execute("INSERT INTO sessions(id,created_at,parent_id) VALUES (?1,'test','root')",(sibling,))
                                db.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test')",("tui.session_location."+sibling,str(project)))
                                baseline_titles.clear()
                                baseline_titles.update(db.execute("SELECT id,title FROM sessions"))
                        if case in ("catalog","dynamic","child_models"):
                            calls = [("page","opencode_models",{"limit":1}), ("all","opencode_models",{"all":True}),
                                     ("query","opencode_models",{"query":own_provider.upper()+"/old COOL","provider":"Own Display"}),
                                     ("other-provider","opencode_models",{"provider":"Other Display"}),
                                     ("unknowns","opencode_models",{"query":"unknown"}), ("offset","opencode_models",{"offset":1,"limit":1})]
                        elif case == "bad_lookup":
                            calls = [("bad","opencode_models",{"limit":101})]
                        elif case in ("models_deny","models_ask"):
                            calls = [("lookup","opencode_models",{})]
                        else:
                            args = {"title":"  Native title 🦀  "}
                            targets = {"explicit":"other","unknown":"missing","foreign":"foreign","child_parent":"root","child_sibling":sibling,"child_foreign":"foreign"}
                            if case in targets:
                                args["sessionID"] = targets[case]
                            if case == "invalid":
                                args["title"] = "   "
                            calls = [("rename","opencode_session_rename",args)]
                        send(self, calls)
                    else:
                        send(self, [])
                except Exception as error:
                    failures.append(repr(error))
                finally:
                    self.close_connection = True

        server = http.server.ThreadingHTTPServer(("127.0.0.1",0),Peer)
        server.daemon_threads = False
        thread = threading.Thread(target=server.serve_forever)
        base = f"http://127.0.0.1:{server.server_port}/v1"
        configuration = {"model":"own/m", "compaction":{"auto":False}, "agent":{"title":{"disable":True},
            "helper":{"mode":"subagent","variant":"a-high","permission":{"opencode_session_rename":"allow"}},
            "general":{"mode":"subagent","variant":"a-high"},"build":{"variant":"a-high"}},
            "permission":{"subagent":"allow","opencode_session_rename":"deny" if case=="deny" else "ask" if case=="ask" else "allow",
                "opencode_models":"deny" if case=="models_deny" else "ask" if case=="models_ask" else "allow"},
            "provider":{"own":{"name":"Own Display","npm":"@ai-sdk/openai","options":{"baseURL":base,"apiKey":"SYNTHETIC_R7_KEY"},
                "models":{"m":{"limit":{"context":1048576,"output":2048},"variants":{"a-high":{"reasoningEffort":"high"},"z-low":{"reasoningEffort":"low"}}},
                    "old":{"name":"Cool old","family":"cool","released":10},"new":{"name":"Cool new","family":"cool","released":20},"unknown":{}}},
                "other":{"name":"Other Display","npm":"@ai-sdk/foreign-inert","options":{"apiKey":"{file:DO_NOT_READ}"},"models":{"other":{"name":"Other static"}}}}}
        if case == "dynamic":
            configuration["plugin"] = ["openproxy-models.js"]
            configuration["model"] = own_provider+"/m"
            configuration["provider"][own_provider] = configuration["provider"].pop("own")
            configuration["provider"][own_provider]["models"]["m"]["name"] = "Local Main"
            del configuration["provider"][own_provider]["models"]["m"]["variants"]["z-low"]
        (config/"opencode.json").write_text(json.dumps(configuration))
        environment = {"HOME":str(home),"XDG_CONFIG_HOME":str(home/"config"),"PATH":"/usr/bin:/bin","OC_TEST_ALLOW_LOOPBACK":"1"}
        thread.start()
        def invoke(prompt):
            nonlocal process
            process = subprocess.Popen([str(binary),"--data-dir",str(data),"run","--json","--session","root",prompt],
                cwd=project,env=environment,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
            stdout,stderr = process.communicate(timeout=15)
            assert len(stdout)+len(stderr)<1048576
            assert not failures, failures
            return process.returncode, stderr.decode()
        try:
            # Initialize actual owner/schema/root binding with no tool effects.
            phase = 99
            code, error = invoke("Initialize native root")
            assert code == 0, error
            requests.clear()
            with sqlite3.connect(data/"oc.sqlite") as db:
                for session, location in [("other",str(project)),("foreign",str(root/"foreign"))]:
                    db.execute("INSERT INTO sessions(id,created_at) VALUES (?1,'test')",(session,))
                    db.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'test')",("tui.session_location."+session,location))
                baseline_titles.update(db.execute("SELECT id,title FROM sessions"))
            initial_gets = len(gets)
            phase = 0
            code,error = invoke("Exercise native model/session tool")
            assert code == (1 if case in ("ask","models_ask") else 0), error
            assert len(gets)-initial_gets == (1 if case=="dynamic" else 0), "lookup triggered discovery"
            with sqlite3.connect(data/"oc.sqlite") as db:
                rows=db.execute("SELECT name,state,output,id FROM tool_operations ORDER BY rowid").fetchall()
                titles=dict(db.execute("SELECT id,title FROM sessions"))
                updated=db.execute("SELECT session_id,payload FROM events WHERE kind='session_updated'").fetchall()
            failures_expected=("deny","unknown","foreign","invalid","child_parent","child_sibling","child_foreign","general","bad_lookup")
            if case in ("catalog","dynamic","child_models"):
                page=json.loads(outputs["page"])
                assert page["providers"][0]["id"]==own_provider and page["providers"][0]["models"][0]["id"]==own_provider+"/new"
                assert page["nextOffset"]==1
                all_models=json.loads(outputs["all"])
                assert all_models["total"]==page["total"]+1
                assert json.loads(outputs["query"])["providers"][0]["models"][0]["id"]==own_provider+"/old"
                assert json.loads(outputs["other-provider"])["providers"][0]["models"][0]["id"]=="other/other"
                unknown=json.loads(outputs["unknowns"])["providers"][0]["models"][0]
                for key in ("released","family","cost","status"):
                    assert unknown[key] is None
                own_models=all_models["providers"][0]["models"]
                main=next(m for m in own_models if m["id"]==own_provider+"/m")
                assert main["variants"].index("z-low") < main["variants"].index("a-high"), main["variants"]
                if case in ("catalog","child_models"):
                    assert main["variants"]==["z-low","a-high"]
                if case=="dynamic":
                    assert main["name"]=="Local Main", ("effective local override",main["name"])
                    assert any(m["id"]==own_provider+"/new-unknown" for m in own_models)
                assert not updated
            elif case in ("ask","models_ask"):
                assert not rows and not updated
            elif case=="models_deny":
                assert outputs["lookup"].startswith("error:") and not updated and titles == baseline_titles
            elif case in failures_expected:
                output=outputs["bad" if case=="bad_lookup" else "rename"]
                assert output.startswith("error:"), output
                assert not updated and titles == baseline_titles, (updated,titles,baseline_titles)
            else:
                result=json.loads(outputs["rename"])
                target="other" if case=="explicit" else next(s for s in titles if s.startswith("root-sub-") and not s.endswith("-sibling")) if child else "root"
                assert result=={"sessionID":target,"title":"Native title 🦀"}
                assert titles[target]=="Native title 🦀"
                assert updated==[(target,json.dumps({"title":"Native title 🦀"},ensure_ascii=False,separators=(",",":")))],updated
                if case=="restart":
                    phase=99
                    before=len(requests)
                    code,error=invoke("Continue after restart")
                    assert code==0,error
                    assert len(requests)==before+1
                    with sqlite3.connect(data/"oc.sqlite") as db:
                        assert db.execute("SELECT title FROM sessions WHERE id='root'").fetchone()[0]=="Native title 🦀"
                        assert db.execute("SELECT count(*) FROM events WHERE kind='session_updated'").fetchone()[0]==1
            for call_id,output in outputs.items():
                if call_id=="spawn":
                    continue
                matching=[r for r in rows if r[3].endswith("-"+call_id)]
                assert len(matching)==1 and matching[0][2]==output,"exact call ID/wire/durable outcome mismatch"
            captured = {t["name"]:t["parameters"] for t in requests[0].get("tools",[]) if t["name"] in ("opencode_models","opencode_session_rename")}
            return {"case":case,"status":"PASS","provider_requests":len(requests)+1+len(auxiliary),"auxiliary_requests":len(auxiliary),
                "discovery_gets":len(gets),"tool_rows":len(rows),"title_events":len(updated),
                "captured_schemas":captured,"captured_child_schemas":child_schema,
                "captured_results":{k:v for k,v in outputs.items() if k != "spawn"}}
        finally:
            if process is not None and process.poll() is None:
                os.killpg(process.pid,signal.SIGKILL)
                process.wait(timeout=5)
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
            assert not thread.is_alive()

def main():
    global SESSION_MOVE_SUPPORTED
    parser=argparse.ArgumentParser()
    parser.add_argument("binary",type=Path)
    parser.add_argument("--case",choices=CASES)
    parser.add_argument("--session-move-supported",action="store_true",help="Current R8 catalog; retains frozen historical R7 mode by default")
    options=parser.parse_args()
    SESSION_MOVE_SUPPORTED=options.session_move_supported
    binary=options.binary.resolve()
    before=digest(binary)
    failed=0
    totals={"cases":0,"provider_requests":0,"auxiliary_requests":0,"discovery_gets":0,"tool_rows":0,"title_events":0}
    for case in [options.case] if options.case else CASES:
        try:
            result=run(binary,case)
            for k in totals:
                totals[k]+=1 if k=="cases" else result[k]
        except Exception as error:
            failed+=1
            result={"case":case,"status":"FAIL","error":str(error),"trace":traceback.format_exc(limit=2)}
        print(json.dumps(result,ensure_ascii=False))
    after=digest(binary)
    assert before==after
    print(json.dumps({"binary":str(binary),"sha256_before":before,"sha256_after":after,"failures":failed,
        "owned_cleanup":"PGIDs and all HTTP threads joined before exact TempDir removal",**totals}))
    raise SystemExit(bool(failed))

if __name__=="__main__":
    main()
