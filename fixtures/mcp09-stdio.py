#!/usr/bin/python3
"""MCP09 offline peer: boolean canary checks/counters only, never an env dump."""
import json
import os
import signal
import sys
import time

report, expected_cwd, credentials, stage = sys.argv[1:5]
snapshot = {
    "pid": os.getpid(),
    "spawn": 1,
    "initialize": 0,
    "catalog": 0,
    "call": 0,
    "argv": len(sys.argv) == 7 and sys.argv[-2:] == ["two words", ""],
    "cwd": os.getcwd() == expected_cwd,
    "inherited": os.environ.get("MCP09_INHERITED") == "mcp09-inherited-canary",
    "overlay": os.environ.get("MCP09_OVERLAY") == "mcp09-configured-canary",
    "path": os.environ.get("PATH") == os.path.dirname(os.path.abspath(__file__)),
    "credentials": (os.environ.get("GLOBAL_PROVIDER_TOKEN") == "mcp09-domain-canary")
    if credentials == "global" else "GLOBAL_PROVIDER_TOKEN" not in os.environ,
    "alias": (os.environ.get("BENIGN_ALIAS") == "mcp09-domain-canary")
    if credentials == "global" else "BENIGN_ALIAS" not in os.environ,
}


def save():
    with open(report, "w", encoding="utf-8") as stream:
        json.dump(snapshot, stream)


def terminate(_number, _frame):
    snapshot["closed"] = 1
    save()
    sys.exit(0)


signal.signal(signal.SIGTERM, terminate)
save()
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        snapshot["initialize"] += 1
        save()
        if stage in ("initialize", "legacy-delay"):
            time.sleep(0.2 if stage == "legacy-delay" else 0.6)
        result = {
            "protocolVersion": "2026-07-28" if stage == "modern" else "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mcp09", "version": "1"},
            "instructions": "bounded guidance " + os.environ.get("MCP09_OVERLAY", ""),
        }
    elif method == "tools/list":
        snapshot["catalog"] += 1
        save()
        if stage == "catalog":
            time.sleep(0.6)
        result = {"tools": [{"name": "probe", "inputSchema": {"type": "object"}}]}
    elif method == "tools/call":
        snapshot["call"] += 1
        save()
        if stage == "execution":
            time.sleep(0.6)
        # Specific, synthetic canaries exercise redaction; no environment dump.
        text = "probe complete " + " ".join(os.environ.get(key, "") for key in
            ("MCP09_OVERLAY", "MCP09_INHERITED", "GLOBAL_PROVIDER_TOKEN"))
        print(text, file=sys.stderr, flush=True)
        result = {"content": [{"type": "text", "text": text}]}
    else:
        continue
    if "id" in request:
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
snapshot["closed"] = 1
save()
