#!/usr/bin/env python3
"""Read-only receipts/size/hash/cleanup audit for this shell atomic only."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import re

ROOT=Path(__file__).resolve().parents[2]
EVIDENCE=ROOT/"evidence/T50"
files=set(EVIDENCE.glob("shell-*"))
files.update(EVIDENCE/name for name in ("check_shell_output.py","audit_shell_output.py","native_shell_projection.py",
    "native_tool_output_common.py","native_shell_controls.py","native_live_model_switch.py","native_background.py"))
stored=sum(p.stat().st_size for p in files if p.is_file())
assert stored<=1048576,(stored,"own evidence budget")
rows=[json.loads(line) for line in (EVIDENCE/"shell-gates.jsonl").read_text().splitlines()]
current_temp={row.get("temporary") or row.get("owned_temporary") for row in rows if row.get("pid_pgid")==os.getpid()}
temporary=[]
for row in rows:
    path=row.get("temporary") or row.get("owned_temporary")
    if path and path not in current_temp and Path(path).exists():temporary.append(path)
summary={}
for name in ("workspace-current","workspace-route2","workspace-live","owner-live2","media-final"):
    log=EVIDENCE/f"shell-{name}.log"
    raw=log.read_bytes() if log.exists() else gzip.decompress(log.with_suffix(".log.gz").read_bytes())
    assert len(raw)<=16*1024*1024
    matches=re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;[^\n]*finished in ([\d.]+)s",raw)
    finish=next(row for row in rows if row.get("name")==name and "exit" in row)
    summary[name]={"exit":finish["exit"],"seconds":finish["seconds"],"decoded_log_bytes":len(raw),
        "targets":len(matches),"passed":sum(int(m[0]) for m in matches),"failed":sum(int(m[1]) for m in matches),
        "ignored":sum(int(m[2]) for m in matches),"target_seconds":round(sum(float(m[3]) for m in matches),2)}
hashes={}
for profile in ("debug","release"):
    with (ROOT/"target"/profile/"oc").open("rb") as stream:hashes[profile]=hashlib.file_digest(stream,"sha256").hexdigest()
    raw=(EVIDENCE/f"shell-common-{profile}-final.log").read_text()
    before=json.loads(raw.splitlines()[-1])["sha256_before"]
    assert hashes[profile]==before,"normal ELF changed after direct proofs"
for p in files:
    if p.name.endswith(".log"):
        assert p.stat().st_size<=16*1024*1024
    elif p.name.endswith(".log.gz"):
        assert len(gzip.decompress(p.read_bytes()))<=16*1024*1024
assert not temporary,temporary
native_temporary=set()
def locations(value):
    if isinstance(value,dict):
        for key,item in value.items():
            if key=="location" and isinstance(item,str):
                project=Path(item)
                if project.parent.name.startswith("t50-background-owned-"):native_temporary.add(project.parent)
            locations(item)
    elif isinstance(value,list):
        for item in value:locations(item)
for profile in ("debug","release"):
    for line in (EVIDENCE/f"shell-controls-{profile}-final.log").read_text().splitlines():locations(json.loads(line))
remaining_native=[str(path) for path in native_temporary if path.exists()]
assert not remaining_native,remaining_native
native_cases={}
for profile in ("debug","release"):
    count=0;physical=0
    for lane in ("common","cap","quota","controls","projection","question","image"):
        cases=[json.loads(line) for line in (EVIDENCE/f"shell-{lane}-{profile}-final.log").read_text().splitlines() if line.startswith("{")]
        passed=[case for case in cases if case.get("status")=="PASS"]
        count+=len(passed)
        if lane in ("common","cap","quota","controls","projection"):
            physical+=sum(case.get("provider_requests",case.get("physical_requests",0)) for case in passed)
    assert count==27 and physical==71,(profile,count,physical)
    native_cases[profile]={"passed":count,"directed_shell_physical_posts":physical}
print(json.dumps({"own_stored_bytes":stored,"own_files":len(files),"gates":summary,"normal_sha256":hashes,
    "native_cases":native_cases,
    "remaining_recorded_temporary":temporary,"remaining_native_recorded_temporary":remaining_native,
    "source_base":"9d8b1b8186ec2a140fb28db1681fc58b89086120 + DIRTY",
    "archive_mapping":"original .log receipt maps to exact lossless .log.gz when archived"}))
