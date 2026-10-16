#!/usr/bin/env python3
"""Read-only final receipts: owned evidence, exact gate temporaries and ELFs."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import re


def main():
    root = Path(__file__).resolve().parents[2]
    evidence = root / "evidence/T50"
    receipts = [json.loads(line) for line in (evidence / "output-gates.jsonl").read_text().splitlines()]
    owned = list(evidence.glob("output-*")) + [
        evidence / name for name in (
            "tool-output-common.md", "check_tool_output_common.py",
            "native_tool_output_common.py", "native_tool_output_question.py", "audit_tool_output_common.py",
        )
    ]
    logs = []
    for path in owned:
        if path.suffix not in (".log", ".gz"):
            continue
        data = gzip.decompress(path.read_bytes()) if path.suffix == ".gz" else path.read_bytes()
        assert len(data) <= 16 * 1024 * 1024, path.name
        results = re.findall(rb"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored", data)
        logs.append({"name": path.name, "stored_bytes": path.stat().st_size,
                     "exact_log_bytes": len(data),
                     "completed_target_counts": len(results),
                     "passed": sum(int(p) for _,p,_,_ in results),
                     "failed": sum(int(f) for _,_,f,_ in results),
                     "ignored": sum(int(i) for _,_,_,i in results),
                     "failed_tests": [s.decode() for s in re.findall(rb"^test (.+) \.\.\. FAILED$", data, re.M)],
                     "completed_target_seconds": round(sum(float(s) for s in re.findall(rb"finished in ([0-9.]+)s", data)), 2)})
    remaining_recorded = sorted({r["owned_temporary"] for r in receipts if "owned_temporary" in r
                                and r["owned_pid_pgid"] != os.getpid()
                                and Path(r["owned_temporary"]).exists()})
    bench = Path("/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924")
    # Names/metadata only; do not inspect or remove unrelated fixtures.
    current_temporaries = {r["owned_temporary"] for r in receipts if "owned_temporary" in r
                           and r["owned_pid_pgid"] == os.getpid()}
    prefix_directories = [{"path": str(p), "mtime": p.stat().st_mtime}
                          for p in bench.iterdir() if p.name.startswith("t50-gate-")
                          and str(p) not in current_temporaries]
    hashes = {}
    for profile in ("debug", "release"):
        with (root / "target" / profile / "oc").open("rb") as binary:
            hashes[profile] = hashlib.file_digest(binary, "sha256").hexdigest()
    important = {"output-workspace3.log.gz", "output-workspace12.log.gz", "output-workspace-resumed-current.log.gz",
                 "output-owner-resume3.log", "output-fetch-resume.log", "output-mcp-media-resumed-current.log"}
    print(json.dumps({"owned_aggregate_bytes": sum(p.stat().st_size for p in owned),
                      "owned_file_count": len(owned), "remaining_recorded_temporaries": remaining_recorded,
                      "unclassified_owned_prefix_directories": prefix_directories,
                      "elf_sha256": hashes, "logs": [log for log in logs if log["name"] in important],
                      "first_wrapper_cleanup_timing": {name: (evidence / name).stat().st_mtime for name in
                         ("output-clippy5.log", "output-workspace1.log.gz")}}, sort_keys=True, indent=2))


if __name__ == "__main__":
    main()
