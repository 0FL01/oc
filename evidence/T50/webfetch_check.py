#!/usr/bin/env python3
"""Bounded stdlib command/log recorder for this atomic qualification only."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument("log", type=Path)
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()
command = args.command[1:] if args.command[:1] == ["--"] else args.command
environment = dict(os.environ, CARGO_BUILD_JOBS="3", RUST_TEST_THREADS="1",
                   CARGO_NET_OFFLINE="true", TMPDIR="/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924")
assert args.log.parent.is_dir()
counts = [0, 0, 0]
summaries = 0
size = 0
truncated = False
interesting = []
started = time.monotonic()
with args.log.open("xb") as output:
    output.write(("COMMAND " + json.dumps(command) + "\n").encode())
    process = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    for line in process.stdout:
        if size + len(line) <= 262144:
            output.write(line)
            size += len(line)
        else:
            truncated = True
        text = line.decode(errors="replace").strip()
        match = re.search(r"test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored", text)
        if match:
            summaries += 1
            counts = [a + int(b) for a, b in zip(counts, match.groups())]
        if any(word in text for word in ("FAILED", "error:", "warning:", "panicked", "Ran ", "FAIL", "OK", "PASS", "Finished")):
            interesting.append(text[:600])
    exit_code = process.wait(timeout=900)
    result = {"command":command,"exit":exit_code,"seconds":round(time.monotonic()-started,2),
              "log":str(args.log.resolve()),"log_bytes":size,"truncated":truncated,
              "summaries":summaries,"passed":counts[0],"failed":counts[1],"ignored":counts[2]}
    output.write(("\nRECORDER " + json.dumps(result) + "\n").encode())
print(json.dumps(result))
for text in interesting[-30:]:
    print(text)
sys.exit(exit_code or int(truncated))
