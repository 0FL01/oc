#!/usr/bin/env python3
"""Read-only audit of corrective receipts; prior receipts are never backfilled."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[2]
EVIDENCE=ROOT/"evidence/T50"
rows=[json.loads(line) for line in (EVIDENCE/"shell-gates.jsonl").read_text().splitlines()]
summary={}
for name in ("review-red","review-green","review-owner","review-workspace","review-workspace2","review-workspace3","review-media"):
    log=EVIDENCE/f"shell-{name}.log"
    raw=log.read_bytes() if log.exists() else gzip.decompress(log.with_suffix(".log.gz").read_bytes())
    assert len(raw)<=16*1024*1024
    results=re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;[^\n]*finished in ([\d.]+)s",raw)
    finish=next(row for row in rows if row.get("name")==name and "exit" in row)
    summary[name]={"exit":finish["exit"],"seconds":finish["seconds"],"decoded_bytes":len(raw),"targets":len(results),
        "passed":sum(int(r[0]) for r in results),"failed":sum(int(r[1]) for r in results),"ignored":sum(int(r[2]) for r in results),
        "target_seconds":round(sum(float(r[3]) for r in results),2)}
hashes={};native={};native_roots=set()
def locations(value):
    if isinstance(value,dict):
        for key,item in value.items():
            if key=="location" and isinstance(item,str):
                path=Path(item).parent
                if path.name.startswith("t50-background-owned-"):native_roots.add(path)
            locations(item)
    elif isinstance(value,list):
        for item in value:locations(item)
for profile in ("debug","release"):
    with (ROOT/"target"/profile/"oc").open("rb") as stream:hashes[profile]=hashlib.file_digest(stream,"sha256").hexdigest()
    common=[json.loads(line) for line in (EVIDENCE/f"shell-review-common-{profile}.log").read_text().splitlines()]
    assert hashes[profile]==common[-1]["sha256_before"]==common[-1]["sha256_after"]
    count=0;physical=0
    for lane in ("common","incomplete","cap","quota","controls","projection","question","image"):
        values=[json.loads(line) for line in (EVIDENCE/f"shell-review-{lane}-{profile}.log").read_text().splitlines() if line.startswith("{")]
        passed=[case for case in values if case.get("status")=="PASS"]
        count+=len(passed)
        if lane not in ("question","image"):physical+=sum(case.get("provider_requests",case.get("physical_requests",0)) for case in passed)
        for value in values:locations(value)
    assert count==28 and physical==77,(profile,count,physical)
    native[profile]={"passed":count,"directed_capture_physical_posts":physical}
remaining=[row["owned_temporary"] for row in rows if "owned_temporary" in row and row["pid_pgid"]!=os.getpid() and Path(row["owned_temporary"]).exists()]
remaining_native=[str(path) for path in native_roots if path.exists()]
assert not remaining and not remaining_native,(remaining,remaining_native)
files=set(EVIDENCE.glob("shell-*"))
files.update(EVIDENCE/name for name in ("check_shell_output.py","audit_shell_output.py","audit_shell_review.py","native_shell_projection.py",
    "native_tool_output_common.py","native_shell_controls.py","native_live_model_switch.py","native_background.py"))
total=sum(path.stat().st_size for path in files if path.is_file())
assert total<=1048576,total
for path in files:
    if path.name.endswith(".log.gz"):assert len(gzip.decompress(path.read_bytes()))<=16*1024*1024
    elif path.name.endswith(".log"):assert path.stat().st_size<=16*1024*1024
print(json.dumps({"gates":summary,"normal_sha256":hashes,"native":native,"own_total_stored_bytes":total,"own_files":len(files),
    "remaining_recorded_temporary":remaining,"remaining_native_recorded_temporary":remaining_native,
    "source_base":"9d8b1b8186ec2a140fb28db1681fc58b89086120 + DIRTY; review entry718f56e36 + owned DIRTY",
    "archive_mapping":"original .log receipts map to exact lossless .log.gz when archived"}))
