#!/usr/bin/python3
"""Offline MCP08/MCP10 counter fixture: only booleans/counters, no env dump."""
import json
import os
import signal
import sys
import time

report, label, initialize_gate, catalog_gate, expected_cwd = sys.argv[1:]
counts = {"spawn": 1, "pid": os.getpid(), "initialize": 0, "catalog": 0,
          "call": 0, "closed": 0, "cwd": os.getcwd() == expected_cwd,
          "activated": os.environ.get("MCP10_CANARY") == "mcp10-activated-canary"}


def event(stage):
    descriptor = os.open(report + ".events", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, (stage + " " + str(os.getpid()) + "\n").encode("ascii"))
    finally:
        os.close(descriptor)


def save():
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    try:
        with open(report + ".part", "w", encoding="utf8") as output:
            json.dump(counts, output)
        os.replace(report + ".part", report)
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


def gate(path):
    deadline = time.monotonic() + 20
    while path != "-" and not os.path.exists(path):
        if time.monotonic() > deadline:
            raise RuntimeError("fixture barrier expired")
        time.sleep(0.01)


def terminate(_signum, _frame):
    close_once()
    raise SystemExit(0)


def close_once():
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    try:
        if not counts["closed"]:
            counts["closed"] = 1
            event("closed")
        save()
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)


signal.signal(signal.SIGTERM, terminate)
event("spawn")
save()
try:
    for line in sys.stdin:
        request = json.loads(line)
        method = request["method"]
        if method == "initialize":
            counts["initialize"] += 1
            event("initialize")
            save()
            gate(initialize_gate)
            result = {"protocolVersion": "2025-11-25", "capabilities": {"tools": {}},
                      "serverInfo": {"name": label, "version": "fixture"},
                      "instructions": "LIFECYCLE_GUIDANCE"}
        elif method == "tools/list":
            counts["catalog"] += 1
            event("catalog")
            save()
            gate(catalog_gate)
            result = {"tools": [{"name": "ping", "inputSchema": {"type": "object"}}]}
        elif method == "tools/call":
            counts["call"] += 1
            event("call")
            save()
            result = {"content": [{"type": "text", "text": label + "-result"}], "isError": False}
        else:
            continue
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)
finally:
    close_once()
