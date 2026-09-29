#!/usr/bin/python3
"""Offline actual stdio media fixture; append-only method/PID counters, no env dump."""
import json
import os
import pathlib
import signal
import sys

report = sys.argv[1]
result = json.loads(pathlib.Path(__file__).with_name("mcp12-media.json").read_text())["result"]
closed = False


def record(method):
    fd = os.open(report, os.O_CREAT | os.O_APPEND | os.O_WRONLY, 0o600)
    try:
        os.write(fd, (json.dumps({"method": method, "pid": os.getpid()}) + "\n").encode())
    finally:
        os.close(fd)


def close(*_):
    global closed
    signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    if not closed:
        record("closed")
        closed = True
    if _:
        os._exit(0)


signal.signal(signal.SIGTERM, close)
try:
    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method", "")
        record(method)
        if "id" not in request:
            continue
        if method == "initialize":
            response = {"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "media-fixture", "version": "1"}}
        elif method == "tools/list":
            response = {"tools": [{"name": "mcp_media_mixed", "inputSchema": {"type": "object", "properties": {}}}]}
        elif method == "tools/call":
            response = result
        else:
            continue
        print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": response}), flush=True)
finally:
    close()
