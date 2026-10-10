"""Ordinary bounded Responses/write episode; never seeds transcript or styles."""
import hashlib
import json
from pathlib import Path
import threading

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "crates/oc-tui/assets/syntax/fixtures.json"
PROMPT = "VIS14 syntax proof: write the owned file, then show the full grammar inventory."
CONTENT = '/* café\n中文 😀 */\nfn main() {\n    let value = "世界";\n    println!("{}", value);\n}\n'
release = threading.Event()
lock = threading.Lock()
requests = 0


def document():
    fixtures = json.loads(FIXTURES.read_text())
    if len(fixtures) != 39:
        raise ValueError("Complete grammar inventory required")
    blocks = []
    for language, cases in fixtures.items():
        info = "RUST" if language == "rust" else language
        blocks.append(f"## GRAMMAR::{language}\n\n````{info}\n{cases[0].rstrip(chr(10))}\n````\n")
    return "\n".join(blocks) + "\nSYNTAX-INVENTORY-DONE\n"


def configure(spec, home, project, config):
    (project / "syntax-proof.rs").write_text("fn old() {}\n")
    config["mcp"] = {"servers": {}} if spec["origin"] == "upstream" else {}
    config["agents" if spec["origin"] == "upstream" else "agent"] = {"build": {"color": "#12ab34"}}
    config["permissions"] = ([{"action": "*", "resource": "*", "effect": "deny"},
        {"action": "edit", "resource": "syntax-proof.rs", "effect": "allow"}]
        if spec["origin"] == "upstream" else {"*": "deny", "write": {"*": "deny", "syntax-proof.rs": "allow"}})


def control(action):
    if action != "release-syntax":
        raise ValueError("Unknown syntax fixture action")
    release.set()
    return {"released": True}


def snapshot(home, project):
    data = (project / "syntax-proof.rs").read_bytes()
    if len(data) > 65536:
        raise ValueError("Owned syntax file exceeds fixture bound")
    cache = home / "data/opentui/tree-sitter"
    usage = []
    if cache.is_dir():
        for folder in ("languages", "queries"):
            for path in sorted((cache / folder).iterdir()):
                if path.is_symlink() or not path.is_file():
                    raise ValueError("Unexpected syntax cache member")
                accessed = path.stat().st_atime_ns
                with path.open("rb") as stream:
                    body = stream.read(16 * 1024 * 1024 + 1)
                if not body or len(body) > 16 * 1024 * 1024:
                    raise ValueError("Syntax cache member bound")
                usage.append({"file": folder + "/" + path.name, "atime_ns": accessed,
                              "sha256": hashlib.sha256(body).hexdigest()})
    return {"file": {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                     "content": data.decode()}, "cache_usage": usage}


def respond(handler, body, spec, emit):
    global requests
    with lock:
        requests += 1
        number = requests
    results = [item for item in body.get("input", []) if item.get("type") == "function_call_output"]
    title = not body.get("tools")
    valid = handler.path == "/v1/responses" and body.get("stream") is True and body.get("model") == "fixture-model-1" and number <= 4
    valid = valid and (title or "write" in {item.get("name") for item in body["tools"]})
    valid = valid and (title or len(results) <= 1 and all(item.get("call_id") == "call_syntax_write" for item in results))
    emit({"kind": "provider", "operation": "title" if title else "syntax_inventory", "valid": valid, "actual_results": results})
    if not valid:
        handler.send_error(400, "Syntax fixture contract rejected")
        return
    tool = not title and not results
    text = "Syntax inventory fixture" if title else document()
    item = ({"id": f"fc_syntax_{number}", "type": "function_call", "status": "completed",
             "call_id": "call_syntax_write", "name": "write", "arguments": json.dumps({"path": "syntax-proof.rs", "content": CONTENT})}
            if tool else {"id": f"msg_syntax_{number}", "type": "message", "role": "assistant", "status": "completed",
                          "content": [{"type": "output_text", "text": text, "annotations": []}]})
    response = {"id": f"resp_syntax_{number}", "object": "response", "created_at": 1700000000,
                "model": body["model"], "status": "in_progress", "output": [], "error": None, "incomplete_details": None}
    sequence = 0
    handler.send_response(200)
    handler.send_header("Content-Type", "text/event-stream")
    handler.send_header("Connection", "close")
    handler.end_headers()

    def event(value):
        nonlocal sequence
        handler.wfile.write(("event: " + value["type"] + "\ndata: " + json.dumps({**value, "sequence_number": sequence}) + "\n\n").encode())
        handler.wfile.flush()
        sequence += 1

    event({"type": "response.created", "response": response})
    event({"type": "response.output_item.added", "output_index": 0, "item": {**item, "status": "in_progress", **({"arguments": ""} if tool else {"content": []})}})
    if tool:
        emit({"kind": "fixture_tool_call", "name": "write", "arguments": {"path": "syntax-proof.rs", "content": CONTENT}})
        event({"type": "response.function_call_arguments.delta", "item_id": item["id"], "output_index": 0, "delta": item["arguments"]})
    else:
        event({"type": "response.content_part.added", "item_id": item["id"], "output_index": 0, "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}})
        boundary = text.index("/* café") + len("/* café") if not title else len(text)
        event({"type": "response.output_text.delta", "item_id": item["id"], "output_index": 0, "content_index": 0, "delta": text[:boundary]})
        if not title:
            emit({"kind": "syntax_live_ready"})
            if not release.wait(45):
                raise TimeoutError("Owned syntax streaming release deadline")
            event({"type": "response.output_text.delta", "item_id": item["id"], "output_index": 0, "content_index": 0, "delta": text[boundary:]})
        event({"type": "response.output_text.done", "item_id": item["id"], "output_index": 0, "content_index": 0, "text": text})
        event({"type": "response.content_part.done", "item_id": item["id"], "output_index": 0, "content_index": 0, "part": item["content"][0]})
    event({"type": "response.output_item.done", "output_index": 0, "item": item})
    event({"type": "response.completed", "response": {**response, "status": "completed", "output": [item],
          "usage": {"input_tokens": 1234, "output_tokens": 64, "total_tokens": 1298}}})
    emit({"kind": "provider_completed", "operation": "title" if title else "syntax_inventory"})
