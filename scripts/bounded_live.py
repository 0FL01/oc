#!/usr/bin/env python3
"""Owned, test-only T46/R4 HTTP envelope. Stdlib only; no implicit live input.

init ROOT creates a NEW 0700 root exclusively and prints its inode-bound ID.
serve --campaign ID reads one explicit target manifest from stdin, then keeps
stdin as its ownership pipe. EOF/signal stops and reaps all request workers.
inspect --campaign ID prints counts/attempt outcomes only. Resume MUST reuse ID.
The journal is append-only, flock-serialized, fsynced BEFORE any upstream dial.
An interrupted/uncertain reservation is consumed permanently. No reset command.
"""

import argparse
import base64
import contextlib
import fcntl
import hashlib
import http.client
import ipaddress
import json
import os
import re
import select
import signal
import socket
import ssl
import stat
import sys
import time
import uuid
from urllib.parse import urlsplit

LIMITS = {"generation": 24, "mcp": 4, "control": 128}
JOURNAL_CAP = 64 * 1024
HEADER_CAP = 16 * 1024
REQUEST_CAP = 1024 * 1024
TOTAL_INPUT_CAP = 8 * 1024 * 1024
RESPONSE_CAP = 8 * 1024 * 1024
WORKERS = 4
DEADLINE = 300
PLACEHOLDER = "bounded-envelope-placeholder"
SAFE_ID = re.compile(r"[a-z][a-z0-9_-]{0,63}\Z")
HEADER_NAME = re.compile(r"[!#$%&'*+.^_`|~0-9a-z-]+\Z")
HOP_HEADERS = {"host", "connection", "transfer-encoding", "te", "trailer", "upgrade", "proxy-authorization", "proxy-connection", "keep-alive"}


class Closed(Exception):
    """Fixed safe codes only; never exception/URL/header/payload interpolation."""


def require(ok, code):
    if not ok:
        raise Closed(code)


def exact_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "duplicate_json_key")
            result[key] = value
        return result
    try:
        return json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: (_ for _ in ()).throw(Closed("invalid_json")))
    except (ValueError, UnicodeError, RecursionError):
        raise Closed("invalid_json") from None


def directory(path):
    """No-follow every component; no writable/untrusted ancestors."""
    require(isinstance(path, str) and path.startswith("/") and os.path.normpath(path) == path, "invalid_root")
    fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY)
    private_ancestor = False
    try:
        for component in path.split("/")[1:]:
            require(component not in ("", ".", ".."), "invalid_root")
            child = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
            info = os.fstat(fd)
            require(info.st_uid in (0, os.getuid()) and (private_ancestor or info.st_mode & 0o022 == 0), "untrusted_ancestor")
            # A no-follow owner-only ancestor (the dedicated account's 0700
            # HOME) prevents other UIDs from reaching group-writable cache
            # descendants. State root/file themselves still require 0700/0600.
            private_ancestor |= info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700
        return fd
    except BaseException:
        os.close(fd)
        raise


def trusted(info, mode, is_dir=False):
    require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == mode, "untrusted_state")
    require(stat.S_ISDIR(info.st_mode) if is_dir else stat.S_ISREG(info.st_mode) and info.st_nlink == 1, "untrusted_state")


def append(fd, value):
    raw = (json.dumps(value, separators=(",", ":"), sort_keys=True) + "\n").encode()
    require(os.fstat(fd).st_size + len(raw) <= JOURNAL_CAP, "journal_limit")
    # A partial write or failed fsync cannot authorize a dispatch. No rollback.
    require(os.write(fd, raw) == len(raw), "journal_write")
    os.fsync(fd)


def identity_binding(identity):
    # Bind the original path as well as inode/nonce. Re-encoding the same nonce
    # for a renamed root or replacement file must not create fresh authority.
    raw = json.dumps(identity, separators=(",", ":"), sort_keys=True).encode()
    return hashlib.sha256(raw).hexdigest()


class Ledger:
    def __repr__(self):
        return "<bounded campaign ledger>"

    @staticmethod
    def create(root):
        require(os.getuid() != 0, "root_refused")
        parent, name = os.path.split(root)
        require(name not in ("", ".", ".."), "invalid_root")
        parent_fd = directory(parent)
        try:
            # Existing roots, even empty or exhausted ones, are never reused.
            os.mkdir(name, 0o700, dir_fd=parent_fd)
            os.fsync(parent_fd)
        finally:
            os.close(parent_fd)
        root_fd = directory(root)
        try:
            fd = os.open("attempts.jsonl", os.O_RDWR | os.O_APPEND | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=root_fd)
            try:
                root_stat, file_stat = os.fstat(root_fd), os.fstat(fd)
                trusted(root_stat, 0o700, True)
                trusted(file_stat, 0o600)
                identity = {"v": 1, "uid": os.getuid(), "root": root,
                            "dir": [root_stat.st_dev, root_stat.st_ino],
                            "file": [file_stat.st_dev, file_stat.st_ino], "id": uuid.uuid4().hex}
                append(fd, {"v": 1, "id": identity["id"], "binding": identity_binding(identity)})
                os.fsync(root_fd)
                return "b1." + base64.urlsafe_b64encode(json.dumps(identity, separators=(",", ":")).encode()).decode()
            finally:
                os.close(fd)
        finally:
            os.close(root_fd)

    def __init__(self, campaign):
        require(os.getuid() != 0, "root_refused")
        require(isinstance(campaign, str) and campaign.startswith("b1.") and len(campaign) <= 4096, "invalid_identity")
        try:
            self.identity = exact_json(base64.b64decode(campaign[3:], altchars=b"-_", validate=True))
        except (ValueError, TypeError):
            raise Closed("invalid_identity") from None
        i = self.identity
        require(isinstance(i, dict) and set(i) == {"v", "uid", "root", "dir", "file", "id"}, "invalid_identity")
        require(i["v"] == 1 and i["uid"] == os.getuid() and isinstance(i["id"], str) and re.fullmatch(r"[a-f0-9]{32}", i["id"]), "invalid_identity")
        require(all(isinstance(i[key], list) and len(i[key]) == 2 and all(type(n) is int and n >= 0 for n in i[key]) for key in ("dir", "file")), "invalid_identity")
        self.snapshot()  # Independently verify before accepting any target input.

    @contextlib.contextmanager
    def locked(self):
        i = self.identity
        root_fd = directory(i["root"])
        fd = None
        try:
            info = os.fstat(root_fd)
            trusted(info, 0o700, True)
            require([info.st_dev, info.st_ino] == i["dir"], "identity_mismatch")
            fd = os.open("attempts.jsonl", os.O_RDWR | os.O_APPEND | os.O_NOFOLLOW, dir_fd=root_fd)
            until = time.monotonic() + 2
            while True:
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    require(time.monotonic() < until, "journal_busy")
                    time.sleep(0.005)
            info = os.fstat(fd)
            trusted(info, 0o600)
            require([info.st_dev, info.st_ino] == i["file"], "identity_mismatch")
            named = os.stat("attempts.jsonl", dir_fd=root_fd, follow_symlinks=False)
            require([named.st_dev, named.st_ino] == i["file"], "identity_mismatch")
            current = os.stat(i["root"], follow_symlinks=False)
            require([current.st_dev, current.st_ino] == i["dir"], "identity_mismatch")
            require(0 < info.st_size <= JOURNAL_CAP, "corrupt_journal")
            os.lseek(fd, 0, os.SEEK_SET)
            raw = os.read(fd, JOURNAL_CAP + 1)
            require(raw.endswith(b"\n") and len(raw) == info.st_size, "corrupt_journal")
            rows = [exact_json(line) for line in raw.splitlines()]
            require(rows[0] == {"v": 1, "id": i["id"], "binding": identity_binding(i)}, "identity_mismatch")
            counts = {kind: 0 for kind in LIMITS}
            attempts, total = [], 0
            for row in rows[1:]:
                require(isinstance(row, dict), "corrupt_journal")
                if set(row) == {"reserve", "kind", "target", "bytes", "catalog_ids"}:
                    require(type(row["reserve"]) is int and row["reserve"] == len(attempts) + 1, "corrupt_journal")
                    require(row["kind"] in LIMITS and isinstance(row["target"], str) and SAFE_ID.fullmatch(row["target"]), "corrupt_journal")
                    require(type(row["bytes"]) is int and 0 <= row["bytes"] <= REQUEST_CAP, "corrupt_journal")
                    require(isinstance(row["catalog_ids"], list) and len(row["catalog_ids"]) <= 3 and len(set(row["catalog_ids"])) == len(row["catalog_ids"]) and all(isinstance(n, str) and SAFE_ID.fullmatch(n) for n in row["catalog_ids"]), "corrupt_journal")
                    counts[row["kind"]] += 1
                    total += row["bytes"]
                    attempts.append({**row, "outcome": "reserved"})
                else:
                    require(set(row) == {"attempt", "outcome"} and type(row["attempt"]) is int and 1 <= row["attempt"] <= len(attempts), "corrupt_journal")
                    attempt = attempts[row["attempt"] - 1]
                    require(attempt["outcome"] == "reserved" and row["outcome"] in ("complete", "uncertain"), "corrupt_journal")
                    attempt["outcome"] = row["outcome"]
            require(all(counts[k] <= LIMITS[k] for k in LIMITS) and total <= TOTAL_INPUT_CAP, "corrupt_journal")
            yield fd, {"id": i["id"], "counts": counts, "input_bytes": total, "attempts": attempts}
        finally:
            if fd is not None:
                os.close(fd)  # flock releases only here; never across network I/O.
            os.close(root_fd)

    def snapshot(self):
        with self.locked() as (_, state):
            return state

    def reserve(self, kind, target, size, catalog_ids=()):
        with self.locked() as (fd, state):
            require(kind in LIMITS and SAFE_ID.fullmatch(target) and type(size) is int and 0 <= size <= REQUEST_CAP, "invalid_reservation")
            require(state["counts"][kind] < LIMITS[kind], "exhausted_" + kind)
            require(state["input_bytes"] + size <= TOTAL_INPUT_CAP, "input_limit")
            seq = len(state["attempts"]) + 1
            require(len(catalog_ids) <= 3 and len(set(catalog_ids)) == len(catalog_ids) and all(SAFE_ID.fullmatch(n) for n in catalog_ids), "invalid_catalog_ids")
            append(fd, {"reserve": seq, "kind": kind, "target": target, "bytes": size, "catalog_ids": list(catalog_ids)})
            return seq

    def finish(self, seq, outcome):
        with self.locked() as (fd, state):
            require(1 <= seq <= len(state["attempts"]) and state["attempts"][seq - 1]["outcome"] == "reserved" and outcome in ("complete", "uncertain"), "invalid_outcome")
            append(fd, {"attempt": seq, "outcome": outcome})


class Target:
    def __repr__(self):
        return "<configured upstream target; headers redacted>"

    def __init__(self, url, headers, offline):
        require(isinstance(url, str) and len(url) <= 4096 and url.isascii() and not any(c.isspace() for c in url), "invalid_target")
        try:
            self.url = urlsplit(url)
            self.port = self.url.port or (443 if self.url.scheme == "https" else 80)
        except ValueError:
            raise Closed("invalid_target") from None
        require(self.url.scheme in (("http", "https") if offline else ("https",)) and self.url.hostname and not self.url.username and not self.url.password and not self.url.fragment and self.url.path.startswith("/"), "invalid_target")
        self.offline = offline
        self.headers = checked_headers(headers)
        require(not any(k in HOP_HEADERS or k == "content-length" for k in self.headers), "invalid_target_headers")
        self.path = self.url.path + ("?" + self.url.query if self.url.query else "")

    def connect(self):
        # No proxy env, redirect/fallback/probing or alternate endpoint. Resolve
        # only this configured host; each actual dial is checked. DNS is inside
        # the owned worker process, so shutdown/deadlines can always reap it.
        addresses = socket.getaddrinfo(self.url.hostname, self.port, type=socket.SOCK_STREAM)
        require(0 < len(addresses) <= 16, "address_limit")
        for _, _, _, _, address in addresses:
            ip = ipaddress.ip_address(address[0])
            require(ip.is_loopback if self.offline else ip.is_global, "target_address_refused")
        family, kind, proto, _, address = addresses[0]
        sock = socket.socket(family, kind, proto)
        try:
            sock.settimeout(DEADLINE)
            sock.connect(address)
            if self.url.scheme == "https":
                sock = ssl.create_default_context().wrap_socket(sock, server_hostname=self.url.hostname)
            conn = http.client.HTTPConnection(self.url.hostname, self.port, timeout=DEADLINE)
            conn.sock = sock
            return conn
        except BaseException:
            sock.close()
            raise


def checked_headers(value):
    require(isinstance(value, dict) and len(value) <= 64, "invalid_headers")
    result = {}
    for name, text in value.items():
        require(isinstance(name, str) and HEADER_NAME.fullmatch(name.lower()) and isinstance(text, str) and text.isascii() and not any(ord(c) < 32 or ord(c) == 127 for c in text), "invalid_headers")
        require(name.lower() not in result, "duplicate_header")
        result[name.lower()] = text
    require(sum(len(k) + len(v) + 4 for k, v in result.items()) <= HEADER_CAP, "header_limit")
    return result


def targets(manifest, offline):
    require(isinstance(manifest, dict) and set(manifest) == {"provider", "mcp"}, "invalid_manifest")
    provider = manifest["provider"]
    require(isinstance(provider, dict) and set(provider) == {"generation_url", "discovery_url", "headers"}, "invalid_manifest")
    routes = {
        "/provider/responses": ("provider", "generation", Target(provider["generation_url"], provider["headers"], offline)),
        "/provider/models": ("provider", "control", Target(provider["discovery_url"], provider["headers"], offline)),
    }
    require(isinstance(manifest["mcp"], dict) and 1 <= len(manifest["mcp"]) <= 3, "invalid_manifest")
    for name, entry in manifest["mcp"].items():
        require(isinstance(name, str) and SAFE_ID.fullmatch(name) and name != "provider" and isinstance(entry, dict) and set(entry) == {"url", "headers"}, "invalid_manifest")
        routes["/mcp/" + name] = (name, "mcp", Target(entry["url"], entry["headers"], offline))
    return routes


def read_request(client):
    reader = client.makefile("rb")
    line = reader.readline(HEADER_CAP + 1)
    require(len(line) <= HEADER_CAP and line.endswith(b"\r\n"), "invalid_request")
    try:
        method, path, version = line[:-2].decode("ascii").split(" ")
    except (ValueError, UnicodeError):
        raise Closed("invalid_request") from None
    require(method in ("POST", "GET", "DELETE") and version == "HTTP/1.1", "invalid_request")
    raw_headers, used = {}, len(line)
    while True:
        line = reader.readline(HEADER_CAP + 1)
        used += len(line)
        require(used <= HEADER_CAP and line.endswith(b"\r\n"), "header_limit")
        if line == b"\r\n":
            break
        try:
            name, value = line[:-2].decode("ascii").split(":", 1)
        except (ValueError, UnicodeError):
            raise Closed("invalid_headers") from None
        require(name.lower() not in raw_headers, "duplicate_header")
        raw_headers[name.lower()] = value.strip(" \t")
    headers = checked_headers(raw_headers)
    require("transfer-encoding" not in headers and "expect" not in headers, "unsupported_framing")
    length = headers.get("content-length", "0")
    require(re.fullmatch(r"[0-9]{1,8}", length), "invalid_length")
    length = int(length)
    require(length <= REQUEST_CAP, "request_limit")
    body = reader.read(length)
    require(len(body) == length, "incomplete_request")
    require(method == "POST" or not body, "unexpected_body")
    return method, path, headers, body


def classify(method, route_kind, body):
    if route_kind == "generation":
        require(method == "POST", "invalid_generation")
        value = exact_json(body)
        require(isinstance(value, dict) and type(value.get("max_output_tokens")) is int and 1 <= value["max_output_tokens"] <= 2048, "output_limit")
        require(value.get("stream") is True, "invalid_generation")
        return "generation"
    if route_kind == "control":
        require(method == "GET", "invalid_discovery")
        return "control"
    if method in ("GET", "DELETE"):
        return "control"  # optional MCP SSE receive/session close, byte/deadline bound.
    require(len(body) <= 16 * 1024, "rpc_limit")
    value = exact_json(body)
    require(isinstance(value, dict) and value.get("jsonrpc") == "2.0" and set(value) <= {"jsonrpc", "id", "method", "params"}, "unsupported_rpc")
    rpc = value.get("method")
    if rpc == "tools/call":
        params = value.get("params")
        require(isinstance(params, dict) and set(params) <= {"name", "arguments", "_meta"} and isinstance(params.get("name"), str) and len(params["name"]) <= 256, "invalid_call")
        # Pinned native rmcp adds a progress token even with no request options.
        # It is correlation metadata, not a task/input-required replay grant.
        meta = params.get("_meta", {})
        require(isinstance(meta, dict) and set(meta) <= {"progressToken"}, "unsupported_rpc")
        if "progressToken" in meta:
            token = meta["progressToken"]
            require((type(token) is int and 0 <= token < 2**64) or (isinstance(token, str) and len(token.encode()) <= 256), "unsupported_rpc")
        args = params.get("arguments")
        # Conservative: count ALL calls, and admit only bounded short searches.
        require(isinstance(args, dict) and set(args) == {"query", "response_length"} and args.get("response_length") == "short" and isinstance(args.get("query"), str) and 0 < len(args["query"].encode()) <= 256 and len(body) <= 4096, "short_search_required")
        return "mcp"
    require(rpc in {"initialize", "notifications/initialized", "notifications/cancelled", "ping", "tools/list", "prompts/list", "resources/list", "resources/templates/list"}, "unsupported_rpc")
    return "control"


def local_error(client, code):
    raw = json.dumps({"error": "bounded_envelope", "code": code}).encode()
    client.sendall(b"HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: " + str(len(raw)).encode() + b"\r\n\r\n" + raw)


def forward(client, ledger, routes):
    attempt, conn, outcome, sent = None, None, "uncertain", False
    try:
        client.settimeout(DEADLINE)
        method, path, headers, body = read_request(client)
        require(path in routes, "unknown_route")
        target_id, route_kind, target = routes[path]
        kind = classify(method, route_kind, body)
        # Protected header values are never on disk or in the native config.
        # The explicit placeholder is replaced by the exact configured value.
        for name, value in target.headers.items():
            require(headers.get(name) in (None, PLACEHOLDER, "Bearer " + PLACEHOLDER), "header_source_mismatch")
            headers[name] = value
        require("authorization" not in headers or "authorization" in target.headers, "unconfigured_auth")
        catalog_ids = []
        if kind == "generation":
            advertised = exact_json(body).get("tools", [])
            require(isinstance(advertised, list), "invalid_generation")
            names = [tool.get("name", "") for tool in advertised if isinstance(tool, dict) and isinstance(tool.get("name", ""), str)]
            catalog_ids = sorted({name for name, route, _ in routes.values() if route == "mcp" and any(tool.startswith(name + "__") for tool in names)})
        attempt = ledger.reserve(kind, target_id, len(body), catalog_ids)
        # NOTHING external (including DNS) occurs before reserve + successful fsync.
        conn = target.connect()
        conn.putrequest(method, target.path, skip_host=True, skip_accept_encoding=True)
        conn.putheader("host", target.url.netloc)
        for name, value in headers.items():
            if name not in HOP_HEADERS:
                conn.putheader(name, value)
        conn.putheader("connection", "close")
        conn.endheaders(body if body else None)
        response = conn.getresponse()
        response_headers = response.getheaders()
        require(sum(len(k) + len(v) + 4 for k, v in response_headers) <= HEADER_CAP, "response_header_limit")
        require(len(response_headers) <= 64, "response_header_limit")
        length = response.getheader("content-length")
        require(length is None or (length.isdigit() and int(length) <= RESPONSE_CAP), "response_limit")
        # Preserve status, end-to-end headers and entity bytes. Transfer framing
        # is decoded incrementally and re-framed as connection-close HTTP/1.1.
        client.sendall(f"HTTP/1.1 {response.status} Upstream\r\n".encode())
        for name, value in response_headers:
            if name.lower() not in HOP_HEADERS and name.lower() != "content-length":
                client.sendall(f"{name}: {value}\r\n".encode("latin-1"))
        client.sendall(b"connection: close\r\n\r\n")
        sent, total = True, 0
        while True:
            chunk = response.read1(min(16 * 1024, RESPONSE_CAP - total + 1))
            if not chunk:
                break
            total += len(chunk)
            require(total <= RESPONSE_CAP, "response_limit")
            client.sendall(chunk)
        outcome = "complete"
    except Exception as error:
        if not sent:
            try:
                local_error(client, str(error) if isinstance(error, Closed) else "io_failure")
            except OSError:
                pass
    finally:
        if conn is not None:
            conn.close()
        if attempt is not None:
            try:
                ledger.finish(attempt, outcome)
            except Exception:
                pass  # reserved is consumed; corrupt/IO failure cannot reset it.


class Interposer:
    """At most four owned leaf processes. Closing ownership pipe reaps all."""
    def __init__(self, ledger, routes):
        self.ledger, self.routes, self.children = ledger, routes, {}
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(WORKERS)
        self.listener.setblocking(False)
        self.base = "http://127.0.0.1:" + str(self.listener.getsockname()[1])

    def poll(self):
        for pid, (client, started) in list(self.children.items()):
            exited, _ = os.waitpid(pid, os.WNOHANG)
            cancel = time.monotonic() - started >= DEADLINE
            if not exited:
                try:
                    cancel |= client.recv(1, socket.MSG_PEEK | socket.MSG_DONTWAIT) == b""
                except BlockingIOError:
                    pass
                except OSError:
                    cancel = True
                if cancel:
                    os.kill(pid, signal.SIGKILL)
                    os.waitpid(pid, 0)
                    exited = pid
            if exited:
                client.close()
                del self.children[pid]
        while len(self.children) < WORKERS:
            try:
                client, _ = self.listener.accept()
            except BlockingIOError:
                break
            pid = os.fork()
            if pid == 0:
                signal.signal(signal.SIGTERM, signal.SIG_DFL)
                signal.signal(signal.SIGINT, signal.SIG_DFL)
                self.listener.close()
                for other, _ in self.children.values():
                    other.close()
                # No worker retains the runner's ownership pipe or output pipes.
                for fd in (0, 1, 2):
                    os.close(fd)
                try:
                    forward(client, self.ledger, self.routes)
                finally:
                    client.close()
                    os._exit(0)
            self.children[pid] = (client, time.monotonic())

    def close(self):
        self.listener.close()
        for pid, (client, _) in self.children.items():
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
            client.close()
        self.children.clear()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("init").add_argument("root")
    for name in ("inspect", "serve"):
        subparser = sub.add_parser(name)
        subparser.add_argument("--campaign", required=True)
        if name == "serve":
            subparser.add_argument("--offline", action="store_true")
            subparser.add_argument("--live-opt-in", choices=["bounded-v1"])
    args = parser.parse_args()
    try:
        if args.command == "init":
            print(Ledger.create(args.root), flush=True)
            return 0
        ledger = Ledger(args.campaign)
        if args.command == "inspect":
            print(json.dumps(ledger.snapshot(), separators=(",", ":")), flush=True)
            return 0
        require(args.offline or args.live_opt_in == "bounded-v1", "explicit_opt_in_required")
        require(all(ledger.snapshot()["counts"][k] < LIMITS[k] for k in ("generation", "mcp")), "exhausted_campaign")
        # Only now read the explicitly supplied target/header manifest. No env,
        # config, credentials, browser or network is inspected automatically.
        raw = sys.stdin.buffer.readline(32 * 1024 + 1)
        require(len(raw) <= 32 * 1024 and raw.endswith(b"\n"), "manifest_limit")
        routes = targets(exact_json(raw), args.offline)
        server = Interposer(ledger, routes)
        stopped = False
        def stop(_signum, _frame):
            nonlocal stopped
            stopped = True
        signal.signal(signal.SIGTERM, stop)
        signal.signal(signal.SIGINT, stop)
        ready = {"id": ledger.identity["id"], "provider_base": server.base + "/provider",
                 "mcp": {name: {"url": server.base + path, "headers": {header: ("Bearer " + PLACEHOLDER if header == "authorization" else PLACEHOLDER) for header in target.headers}}
                         for path, (name, kind, target) in routes.items() if kind == "mcp"},
                 "provider_headers": {name: PLACEHOLDER for name in routes["/provider/responses"][2].headers if name != "authorization"}}
        print(json.dumps(ready, separators=(",", ":")), flush=True)
        try:
            while not stopped:
                server.poll()
                readable, _, _ = select.select([sys.stdin.fileno()], [], [], 0.02)
                if readable:
                    require(os.read(sys.stdin.fileno(), 1) == b"", "unexpected_control_input")
                    stopped = True
        finally:
            server.close()
        return 0
    except Exception as error:
        print(json.dumps({"status": "blocked", "code": str(error) if isinstance(error, Closed) else "io_failure"}), flush=True)
        return 2


if __name__ == "__main__":
    sys.exit(main())
