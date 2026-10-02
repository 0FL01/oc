#!/usr/bin/env python3
"""Serial gate receipt; exact full stdout/stderr, command, exit, elapsed time."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import signal
import tempfile
import time


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("name")
    parser.add_argument("--watchdog",type=int,default=898)
    parser.add_argument("command",nargs=argparse.REMAINDER)
    args=parser.parse_args()
    command=args.command
    if command and command[0]=="--":command=command[1:]
    assert command and args.name.replace("-","").isalnum()
    root=Path(__file__).resolve().parents[2]
    log=Path(__file__).resolve().parent / ("output-"+args.name+".log")
    assert not log.exists(),"preserve failed attempts; use a new receipt name"
    environment=dict(os.environ)
    environment.update(CARGO_BUILD_JOBS="3",RUST_TEST_THREADS="1",CARGO_NET_OFFLINE="true",TMPDIR="/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924")
    started=time.monotonic()
    with tempfile.TemporaryDirectory(prefix="t50-gate-",dir=environment["TMPDIR"]) as temporary:
        if command[-1:]==["--help"]:
            environment={"HOME":temporary,"XDG_CONFIG_HOME":temporary,"PATH":"/usr/bin:/bin"}
        with log.open("wb") as output:
            process=subprocess.Popen(command,cwd=root,env=environment,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
            with (log.parent/"output-gates.jsonl").open("a") as stream:
                stream.write(json.dumps({"name":args.name,"phase":"started","owned_pid_pgid":process.pid,"owned_temporary":temporary})+"\n")
            try:
                code=process.wait(timeout=args.watchdog)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid,signal.SIGKILL)
                process.wait(timeout=10)
                code=124
    receipt={"name":args.name,"command":command,"exit":code,"seconds":round(time.monotonic()-started,3),"log":str(log.relative_to(root)),"bytes":log.stat().st_size}
    with (log.parent/"output-gates.jsonl").open("a") as stream:stream.write(json.dumps(receipt)+"\n")
    print(json.dumps(receipt))
    raise SystemExit(code)


if __name__=="__main__":main()
