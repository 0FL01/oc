#!/usr/bin/python3
"""Bounded synthetic MCP prompt/resource fixture, never an environment dump.
Donor 2670273ff17da96f85c5826ced57aa1b368754fa, mcp/client.ts:240-283,
mcp/index.ts:649-735. Events contain only method, owned pid and booleans.
"""
import base64, json, os, signal, sys, threading, time

mode, report, gate = sys.argv[1:4]
lock = threading.Lock()
cancelled = {}
closed = False

def event(method):
    fd = os.open(report, os.O_CREAT | os.O_APPEND | os.O_WRONLY, 0o600)
    os.write(fd, (json.dumps({"method": method, "pid": os.getpid()}) + "\n").encode())
    os.close(fd)

def reply(identity, result=None, error=None):
    with lock:
        response = {"jsonrpc": "2.0", "id": identity}
        response["error" if error else "result"] = error if error else result
        print(json.dumps(response), flush=True)

def handle(request):
    method, identity = request.get("method", ""), request.get("id")
    event(method)
    if method == "notifications/cancelled":
        cancelled[request.get("params", {}).get("requestId")] = True
        return
    if identity is None:
        return
    if method == "initialize":
        capabilities = {"tools": {}}
        if mode != "absent":
            capabilities.update({"prompts": {}, "resources": {}})
        reply(identity, {"protocolVersion": "2025-11-25", "capabilities": capabilities,
                         "serverInfo": {"name": "fixture", "version": "1"}})
        return
    if method == "tools/list":
        reply(identity, {"tools": [{"name": "ping", "inputSchema": {"type": "object"}}]})
        return
    if mode == "held" and method in ("prompts/get", "resources/read"):
        deadline = time.monotonic() + 20
        while not os.path.exists(gate) and time.monotonic() < deadline:
            if cancelled.get(identity):
                event("cancel_observed")
                return
            time.sleep(0.01)
    params = request.get("params", {})
    if os.path.exists(report + ".fail"):
        reply(identity, error={"code": -32001, "message": "PRIVATE_LOOKUP_ERROR_BODY"})
    elif method == "prompts/list":
        if mode in ("oversized", "catalog64"):
            reply(identity, {"prompts": [{"name": "p" + str(i)} for i in range(65 if mode == "oversized" else 64)]})
        elif mode == "pages":
            page = int(params.get("cursor", "0"))
            reply(identity, {"prompts": [{"name": "p" + str(page)}], "nextCursor": str(page + 1)})
        elif mode == "loop":
            reply(identity, {"prompts": [{"name": "first" if not params.get("cursor") else "second"}], "nextCursor": "repeat"})
        elif mode == "duplicate":
            reply(identity, {"prompts": [{"name": "same"}, {"name": "same"}]})
        elif mode == "metadata-secret":
            reply(identity, {"prompts": [{"name": os.environ["LOOKUP_CANARY"]}]})
        elif params.get("cursor"):
            reply(identity, {"prompts": [{"name": "translate", "description": "Second"}]})
        else:
            reply(identity, {"prompts": [{"name": "outline", "description": "First",
                   "arguments": [{"name": "topic", "required": True},
                                 {"name": "style", "required": False}]}], "nextCursor": "second"})
    elif method == "resources/list":
        reply(identity, {"resources": [{"name": "text", "uri": "fixture://text",
                                         "mimeType": "text/plain", "description": "Text"}]})
    elif method == "resources/templates/list":
        reply(identity, {"resourceTemplates": [{"name": "by-key", "uriTemplate": "fixture://{key}",
                                                 "mimeType": "application/octet-stream"}]})
    elif method == "prompts/get":
        event("arguments_exact" if params.get("name") == "outline" and
              params.get("arguments") == {"topic": "synthetic"} else "arguments_other")
        reply(identity, {"description": "Explicit prompt", "messages": [
            {"role": "user", "content": {"type": "text", "text": "Lookup user text"}},
            {"role": "assistant", "content": {"type": "text", "text": "Lookup assistant text"}}]})
    elif method == "resources/read":
        text = "Explicit resource " + os.environ.get("LOOKUP_CANARY", "synthetic")
        if mode == "body-limit":
            reply(identity, {"contents": [{"uri": params["uri"], "text": "x" * (1024 * 1024)}]})
            return
        if mode == "input-required":
            reply(identity, {"resultType": "input_required", "requestState": "PRIVATE_REQUEST_STATE"})
            return
        blob = base64.b64encode(os.environ["LOOKUP_CANARY"].encode()).decode() if mode == "binary-secret" else "AAEC/w=="
        reply(identity, {"contents": [
            {"uri": params["uri"], "mimeType": "text/plain", "text": text},
            {"uri": "fixture://blob", "mimeType": "application/octet-stream", "blob": blob}]})
    else:
        reply(identity, error={"code": -32601, "message": "PRIVATE_UNSUPPORTED_BODY"})

def close_once():
    global closed
    previous = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    try:
        if not closed:
            closed = True
            event("closed")
    finally:
        signal.pthread_sigmask(signal.SIG_SETMASK, previous)

def stop(*_):
    close_once()
    os._exit(0)

signal.signal(signal.SIGTERM, stop)
event("spawn")
for line in sys.stdin:
    request = json.loads(line)
    # Bounded test caller concurrency; main remains able to receive cancellation.
    threading.Thread(target=handle, args=(request,), daemon=True).start()
close_once()
