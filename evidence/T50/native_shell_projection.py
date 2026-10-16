#!/usr/bin/env python3
"""Directed same-task model switch + manual compact + restart of one real capture."""
import argparse
import hashlib
import json
from pathlib import Path
from native_background import until
from native_live_model_switch import SwitchNative, A, B


def digest(binary):
    with binary.open("rb") as stream:
        return hashlib.file_digest(stream,"sha256").hexdigest()


def run(binary):
    with SwitchNative(binary,"tool",shell_capture=True) as native:
        native.start();native.send(b"One captured shell task\r")
        assert native.arrived[0].wait(10)
        until(lambda:native.project.joinpath("tool-started").exists(),"actual producer barrier absent")
        before=native.rows("SELECT id,descriptor FROM tool_output_resources")
        assert len(before)==1
        active=json.loads(before[0][1]);assert active["state"]=="Active" and active["bytes"]>2*1048576
        native.choose(B);native.commit(B)
        native.project.joinpath("tool-release").touch();native.gates[0].set()
        assert native.arrived[1].wait(10)
        request=native.requests[1]
        output=next(i["output"] for i in request["input"] if i.get("call_id")=="held-shell" and i.get("type")=="function_call_output")
        assert len(output.encode())<=65536 and "SHELL_DISTANT_SENTINEL" not in output
        assert "capture Complete" in output and "final-shell-flush" in output
        terminal=json.loads(native.rows("SELECT descriptor FROM tool_output_resources")[0][0])
        assert terminal["id"]==active["id"] and terminal["generation"]==active["generation"]
        assert terminal["path"] in output
        job=native.rows("SELECT provenance,outcome,process FROM shell_jobs")[0]
        assert json.loads(job[0])["model"]==A and json.loads(job[1])["exit"]==0
        pid=json.loads(job[2])["pid"];assert not Path(f"/proc/{pid}").exists()
        native.choose(A);native.commit(A);native.gates[1].set()
        until(lambda:native.rows("SELECT status FROM turns")==[("completed",)],"same task did not settle")
        assert len(native.requests)==3
        until(lambda:b"A finished" in native.tail,"actual latest-model consumer completion not painted")
        assert native.project.joinpath("shell.effect").read_text()=="one"
        native.shell_compact=True;native.send(b"/compact\r")
        try:
            until(lambda:len(native.summaries)==1 and native.rows("SELECT count(*) FROM session_compactions")==[(1,)],"manual compact did not durably finish")
        except AssertionError as error:
            snapshots=[json.loads(r[0]) for r in native.rows("SELECT snapshot FROM session_compactions")]
            facts=[{k:s.get(k) for k in ("state","error","reason")} for s in snapshots]
            raise AssertionError(f"{error}; main={len(native.requests)} summary={len(native.summaries)} auxiliary={len(native.auxiliary)} errors={native.errors} compactions={facts}") from error
        assert native.rows("SELECT count(*) FROM tool_output_resources")==[(1,)]
        assert native.rows("SELECT count(*) FROM shell_jobs")==[(1,)]
        native.stop();native.start();native.stop()
        assert len(native.requests)==3 and len(native.summaries)==1
        assert native.project.joinpath("shell.effect").read_text()=="one" and not native.errors,native.errors
        return {"status":"PASS","case":"large-shell-same-task-switch-compact-restart","main_requests":len(native.requests),
                "summary_requests":len(native.summaries),"auxiliary_requests":len(native.auxiliary),
                "physical_requests":native.physical_requests,
                "request_bytes":[len(json.dumps(r).encode()) for r in native.requests],"capture_id":terminal["id"],
                "capture_bytes":terminal["bytes"],"capture_state":terminal["state"],"tool_rows":native.rows("SELECT name,state FROM tool_operations ORDER BY rowid"),
                "preview_bytes":len(output.encode()),"effects":1,"leader_reaped":True,"owned_cleanup":"joined process/PTY readers/server handlers then exact TempDir"}


if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("binary",type=Path);args=parser.parse_args()
    binary=args.binary.resolve();before=digest(binary)
    print(json.dumps(run(binary)))
    after=digest(binary);assert before==after
    print(json.dumps({"sha256_before":before,"sha256_after":after}))
