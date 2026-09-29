#!/usr/bin/env python3
"""Offline actual-socket/process proofs for the T46/R4 envelope."""

import base64
import contextlib
import http.client
import json
import multiprocessing
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

import bounded_live as envelope

SECRET = "owned-envelope-canary-credential"
SCRIPT = str(Path(envelope.__file__).resolve())
GEN = {"stream": True, "model": "offline/arbitrary-model", "max_output_tokens": 2048, "input": []}
CALL = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search", "arguments": {"query": "offline", "response_length": "short"}}}


class Peer:
    def __init__(self, hold=False, chunked=False, stream_hold=False, oversize_header=False):
        self.hits, self.entered, self.release = [], threading.Event(), threading.Event()
        owner = self
        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"
            def log_message(self, *_):
                pass
            def do_POST(self):
                raw = self.rfile.read(int(self.headers.get("content-length", 0)))
                owner.hits.append((self.command, self.path, dict(self.headers), raw))
                owner.entered.set()
                if hold:
                    owner.release.wait(5)
                self.send_response(200)
                self.send_header("content-type", "text/event-stream")
                self.send_header("mcp-session-id", "owned-session")
                body = b"data: offline\n\n"
                if oversize_header:
                    self.send_header("content-length", str(envelope.RESPONSE_CAP + 1))
                elif not stream_hold:
                    self.send_header("transfer-encoding" if chunked else "content-length", "chunked" if chunked else str(len(body)))
                self.end_headers()
                try:
                    if not oversize_header:
                        self.wfile.write((f"{len(body):x}\r\n".encode() + body + b"\r\n0\r\n\r\n") if chunked else body)
                        self.wfile.flush()
                    if stream_hold:
                        owner.release.wait(5)
                        self.close_connection = True
                except (BrokenPipeError, ConnectionResetError):
                    pass
            do_GET = do_POST
            do_DELETE = do_POST
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = False
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01})
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_port}"

    def close(self):
        self.release.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(2)
        assert not self.thread.is_alive()


def manifest(peer):
    return {"provider": {"generation_url": peer.url + "/exact/prefix/%2Fresponses?configured=1", "discovery_url": peer.url + "/exact/models", "headers": {"authorization": "Bearer " + SECRET, "x-protected": SECRET}},
            "mcp": {"codex_web": {"url": peer.url + "/exact/mcp/%2F?configured=2", "headers": {"authorization": "Bearer " + SECRET}}}}


class Runner:
    def __init__(self, campaign, target_manifest):
        self.closed = False
        self.child = subprocess.Popen([sys_executable(), "-B", SCRIPT, "serve", "--offline", "--campaign", campaign], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        self.child.stdin.write(json.dumps(target_manifest).encode() + b"\n")
        self.child.stdin.flush()
        import select
        if not select.select([self.child.stdout], [], [], 3)[0]:
            self.close()
            raise AssertionError("interposer startup timeout")
        self.ready = json.loads(self.child.stdout.readline(32 * 1024))
        if "provider_base" not in self.ready:
            self.close()
            raise AssertionError("interposer failed closed")

    def close(self):
        if self.closed:
            return
        self.closed = True
        if self.child.stdin:
            self.child.stdin.close()
            self.child.stdin = None
        try:
            out, err = self.child.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(self.child.pid, signal.SIGKILL)
            out, err = self.child.communicate(timeout=3)
            raise AssertionError("interposer shutdown timeout") from None
        assert SECRET.encode() not in out + err


def sys_executable():
    import sys
    return sys.executable


def request(url, body, headers=None, method="POST"):
    from urllib.parse import urlsplit
    parsed = urlsplit(url)
    conn = http.client.HTTPConnection(parsed.hostname, parsed.port, timeout=5)
    raw = body if isinstance(body, bytes) else json.dumps(body).encode()
    headers = headers or {"authorization": "Bearer " + envelope.PLACEHOLDER}
    conn.request(method, parsed.path, raw, {"content-type": "application/json", **headers})
    response = conn.getresponse()
    status, fields, result = response.status, response.getheaders(), response.read(envelope.RESPONSE_CAP + 1)
    conn.close()
    return status, fields, result


def attempts_worker(campaign, target_manifest, count, queue):
    runner = Runner(campaign, target_manifest)
    try:
        successes = [0, 0]
        for _ in range(count):
            successes[0] += request(runner.ready["provider_base"] + "/responses", GEN)[0] == 200
            successes[1] += request(runner.ready["mcp"]["codex_web"]["url"], CALL)[0] == 200
        queue.put(successes)
    finally:
        runner.close()


class EnvelopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = str(Path(self.temp.name) / "campaign")
        self.campaign = envelope.Ledger.create(self.root)
        self.ledger = envelope.Ledger(self.campaign)

    def tearDown(self):
        self.temp.cleanup()

    @contextlib.contextmanager
    def guarded(self, **options):
        peer = Peer(**options)
        runner = Runner(self.campaign, manifest(peer))
        try:
            yield peer, runner
        finally:
            runner.close()
            peer.close()

    def test_actual_socket_limit_and_exact_forwarding(self):
        with self.guarded(chunked=True) as (peer, runner):
            body = json.dumps(GEN, separators=(",", ":")).encode()
            headers = {"authorization": "Bearer " + envelope.PLACEHOLDER, "x-protected": envelope.PLACEHOLDER, "mcp-protocol-version": "2025-11-25", "mcp-session-id": "owned-session", "accept": "application/json, text/event-stream", "user-agent": "oc/offline-proof"}
            for _ in range(25):
                request(runner.ready["provider_base"] + "/responses", body, headers)
            mcp_body = json.dumps({**CALL, "params": {**CALL["params"], "_meta": {"progressToken": 7}}}, separators=(",", ":")).encode()
            for _ in range(5):
                request(runner.ready["mcp"]["codex_web"]["url"], mcp_body, headers)
            self.assertEqual(len(peer.hits), 28)
            first = peer.hits[0]
            self.assertEqual(first[1], "/exact/prefix/%2Fresponses?configured=1")
            self.assertTrue(first[3] == body)
            lower = {k.lower(): v for k, v in first[2].items()}
            self.assertTrue(lower["authorization"] == "Bearer " + SECRET and lower["x-protected"] == SECRET)
            for field in ("mcp-protocol-version", "mcp-session-id", "accept", "user-agent"):
                self.assertTrue(lower[field] == headers[field])
            self.assertTrue(peer.hits[-1][1] == "/exact/mcp/%2F?configured=2")
            self.assertEqual(peer.hits[-1][3], mcp_body)
            state = self.ledger.snapshot()
            self.assertEqual((state["counts"]["generation"], state["counts"]["mcp"]), (24, 4))
            self.assertTrue(SECRET not in json.dumps(state) and SECRET.encode() not in Path(self.root, "attempts.jsonl").read_bytes())

    def test_concurrent_runner_processes_no_overshoot(self):
        peer = Peer()
        context = multiprocessing.get_context("spawn")
        queue = context.Queue()
        workers = [context.Process(target=attempts_worker, args=(self.campaign, manifest(peer), 16, queue)) for _ in range(2)]
        try:
            for worker in workers:
                worker.start()
            results = [queue.get(timeout=15) for _ in workers]
            for worker in workers:
                worker.join(5)
                self.assertEqual(worker.exitcode, 0)
            self.assertEqual(tuple(map(sum, zip(*results))), (24, 4))
            self.assertEqual(len(peer.hits), 28)
            self.assertEqual(self.ledger.snapshot()["counts"], {"generation": 24, "mcp": 4, "control": 0})
        finally:
            for worker in workers:
                if worker.is_alive():
                    worker.kill()
                    worker.join()
            queue.close()
            queue.join_thread()
            peer.close()

    def test_reservation_crash_is_consumed_on_restart(self):
        child = subprocess.Popen([sys_executable(), "-B", "-c", "import sys,os;sys.path.insert(0,sys.argv[1]);from bounded_live import Ledger;l=Ledger(sys.argv[2]);l.reserve('generation','provider',1);os._exit(19)", str(Path(SCRIPT).parent), self.campaign])
        self.assertEqual(child.wait(timeout=3), 19)
        resumed = envelope.Ledger(self.campaign)
        self.assertEqual(resumed.snapshot()["counts"]["generation"], 1)
        with self.guarded() as (peer, runner):
            for _ in range(24):
                request(runner.ready["provider_base"] + "/responses", GEN)
            self.assertEqual(len(peer.hits), 23)
        self.assertEqual(resumed.snapshot()["counts"]["generation"], 24)
        self.assertEqual(resumed.snapshot()["attempts"][0]["outcome"], "reserved")
        with self.assertRaises(FileExistsError):
            envelope.Ledger.create(self.root)

    def test_missing_corrupt_permissions_symlinks_relocation_and_io_fail_closed(self):
        with self.guarded() as (peer, runner):
            path = Path(self.root, "attempts.jsonl")
            original = path.read_bytes()
            url = runner.ready["provider_base"] + "/responses"
            for corrupt in (b"", b"garbage\n", original + b'{"reserve":1', original + b'{}\n'):
                path.write_bytes(corrupt)
                self.assertEqual(request(url, GEN)[0], 403)
            path.write_bytes(original)
            path.chmod(0o644)
            self.assertEqual(request(url, GEN)[0], 403)
            path.chmod(0o000)
            self.assertEqual(request(url, GEN)[0], 403)
            path.chmod(0o600)
            Path(self.root).chmod(0o755)
            self.assertEqual(request(url, GEN)[0], 403)
            Path(self.root).chmod(0o700)
            moved = path.with_name("moved")
            path.rename(moved)
            self.assertEqual(request(url, GEN)[0], 403)
            path.symlink_to(moved)
            self.assertEqual(request(url, GEN)[0], 403)
            path.unlink()
            moved.rename(path)
            other = str(Path(self.temp.name) / "other")
            os.rename(self.root, other)
            # Same actual directory/journal inodes and same nonce, but a caller
            # re-encodes a different root path: durable original binding refuses.
            identity = json.loads(base64.urlsafe_b64decode(self.campaign[3:]))
            identity["root"] = other
            forged = "b1." + base64.urlsafe_b64encode(json.dumps(identity).encode()).decode()
            with self.assertRaises(envelope.Closed):
                envelope.Ledger(forged)
            Path(self.root).symlink_to(other, target_is_directory=True)
            self.assertEqual(request(url, GEN)[0], 403)
            Path(self.root).unlink()
            os.rename(other, self.root)
            os.link(path, path.with_name("hardlink"))
            self.assertEqual(request(url, GEN)[0], 403)
            path.with_name("hardlink").unlink()
            identity = json.loads(base64.urlsafe_b64decode(self.campaign[3:]))
            new_id = envelope.Ledger.create(str(Path(self.temp.name) / "new"))
            other_identity = json.loads(base64.urlsafe_b64decode(new_id[3:]))
            identity["root"] = other_identity["root"]
            forged = "b1." + base64.urlsafe_b64encode(json.dumps(identity).encode()).decode()
            with self.assertRaises(envelope.Closed):
                envelope.Ledger(forged)
            self.assertEqual(len(peer.hits), 0)
            # Actual socketpair forward under injected journal I/O failure;
            # target.connect is not reached, not a declared harness counter.
            for operation in ("fsync", "write", "read"):
                a, b = socket.socketpair()
                raw = json.dumps(GEN).encode()
                b.sendall(b"POST /provider/responses HTTP/1.1\r\nhost: fixture\r\ncontent-length: " + str(len(raw)).encode() + b"\r\n\r\n" + raw)
                with patch.object(envelope.os, operation, side_effect=OSError("private failure")):
                    envelope.forward(a, self.ledger, envelope.targets(manifest(peer), True))
                self.assertTrue(b.recv(4096).startswith(b"HTTP/1.1 403"))
                a.close()
                b.close()
            self.assertEqual(len(peer.hits), 0)
            # fsync failure may leave a full reservation: it is still consumed.
            self.assertEqual(self.ledger.snapshot()["counts"]["generation"], 1)

    def test_rejects_unbounded_rpc_output_input_and_unknown_routes(self):
        with self.guarded() as (peer, runner):
            provider = runner.ready["provider_base"] + "/responses"
            mcp = runner.ready["mcp"]["codex_web"]["url"]
            for body in ({**GEN, "max_output_tokens": 2049}, {**GEN, "max_output_tokens": True}, {**GEN, "max_output_tokens": 8192}, {**GEN, "max_output_tokens": None}, b"x" * (envelope.REQUEST_CAP + 1)):
                self.assertEqual(request(provider, body)[0], 403)
            for body in ([CALL, CALL], {**CALL, "method": "sampling/createMessage"}, {**CALL, "method": "tasks/create"}, {**CALL, "params": {**CALL["params"], "task": {"ttl": None}}}, {**CALL, "params": {"name": "search", "arguments": {**CALL["params"]["arguments"], "queries": ["batch"]}}}, {**CALL, "params": {"name": "search", "arguments": {"query": "x" * 257, "response_length": "short"}}}, {**CALL, "params": {"name": "search", "arguments": {"query": "offline", "response_length": "long"}}}):
                self.assertEqual(request(mcp, body)[0], 403)
            self.assertEqual(request(runner.ready["provider_base"] + "/arbitrary-url", GEN)[0], 403)
            self.assertEqual(len(peer.hits), 0)
            self.assertEqual(self.ledger.snapshot()["counts"]["generation"], 0)

    def test_cancel_shutdown_reaps_workers_and_preserves_uncertain_attempt(self):
        with self.guarded(hold=True) as (peer, runner):
            from urllib.parse import urlsplit
            url = urlsplit(runner.ready["provider_base"])
            client = socket.create_connection((url.hostname, url.port))
            raw = json.dumps(GEN).encode()
            client.sendall(b"POST /provider/responses HTTP/1.1\r\nhost: fixture\r\ncontent-length: " + str(len(raw)).encode() + b"\r\n\r\n" + raw)
            self.assertTrue(peer.entered.wait(3))
            children_path = Path(f"/proc/{runner.child.pid}/task/{runner.child.pid}/children")
            children = [int(pid) for pid in children_path.read_text().split()]
            self.assertEqual(len(children), 1)
            client.close()
            until = time.monotonic() + 2
            while time.monotonic() < until and children_path.read_text().strip():
                time.sleep(0.01)
            self.assertEqual(children_path.read_text().strip(), "")
            runner.close()
            for pid in children:
                self.assertFalse(Path(f"/proc/{pid}").exists())
            self.assertEqual(self.ledger.snapshot()["attempts"][0]["outcome"], "reserved")

    def test_live_presence_without_explicit_opt_in_cannot_read_manifest_or_dispatch(self):
        peer = Peer()
        try:
            result = subprocess.run([sys_executable(), "-B", SCRIPT, "serve", "--campaign", self.campaign], input=json.dumps(manifest(peer)).encode() + b"\n", capture_output=True, timeout=3, env={"PATH": os.environ["PATH"], "LUDKA2_API_KEY": SECRET})
            self.assertEqual(result.returncode, 2)
            self.assertTrue(b"explicit_opt_in_required" in result.stdout)
            self.assertTrue(SECRET.encode() not in result.stdout + result.stderr)
            self.assertEqual(len(peer.hits), 0)
        finally:
            peer.close()

    def test_ownership_pipe_shutdown_reaps_four_held_workers(self):
        with self.guarded(hold=True) as (peer, runner):
            from urllib.parse import urlsplit
            url = urlsplit(runner.ready["provider_base"])
            clients = []
            try:
                for _ in range(envelope.WORKERS + 1):
                    client = socket.create_connection((url.hostname, url.port))
                    clients.append(client)
                    raw = json.dumps(GEN).encode()
                    client.sendall(b"POST /provider/responses HTTP/1.1\r\nhost: fixture\r\ncontent-length: " + str(len(raw)).encode() + b"\r\n\r\n" + raw)
                until = time.monotonic() + 3
                while time.monotonic() < until and len(peer.hits) < envelope.WORKERS:
                    time.sleep(0.01)
                self.assertEqual(len(peer.hits), 4)
                children = [int(pid) for pid in Path(f"/proc/{runner.child.pid}/task/{runner.child.pid}/children").read_text().split()]
                self.assertEqual(len(children), 4)
                runner.close()  # clients are STILL OPEN; ownership EOF is enough.
                for pid in children:
                    self.assertFalse(Path(f"/proc/{pid}").exists())
                self.assertEqual(self.ledger.snapshot()["counts"]["generation"], 4)
                self.assertTrue(all(row["outcome"] == "reserved" for row in self.ledger.snapshot()["attempts"]))
            finally:
                for client in clients:
                    client.close()

    def test_stream_is_incremental_and_oversize_response_fails_bounded(self):
        with self.guarded(stream_hold=True) as (peer, runner):
            from urllib.parse import urlsplit
            url = urlsplit(runner.ready["provider_base"])
            conn = http.client.HTTPConnection(url.hostname, url.port, timeout=3)
            conn.request("POST", url.path + "/responses", json.dumps(GEN), {"content-type":"application/json"})
            response = conn.getresponse()
            self.assertEqual(response.status, 200)
            self.assertEqual(response.read1(4096), b"data: offline\n\n")
            self.assertFalse(peer.release.is_set(), "first SSE bytes must arrive before upstream finishes")
            conn.close()
        with self.guarded(oversize_header=True) as (peer, runner):
            self.assertEqual(request(runner.ready["provider_base"] + "/responses", GEN)[0], 403)
            self.assertEqual(len(peer.hits), 1)
            self.assertEqual(self.ledger.snapshot()["counts"]["generation"], 2)
            self.assertEqual(self.ledger.snapshot()["attempts"][-1]["outcome"], "uncertain")


if __name__ == "__main__":
    unittest.main()
