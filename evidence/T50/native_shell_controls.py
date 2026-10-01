#!/usr/bin/env python3
"""Frozen R2 supplementary proof: synthetic peer + real owned processes + PTY."""
import argparse
import codecs
import hashlib
import json
import os
from pathlib import Path
import re
import termios
import threading
import time

from native_background import Native, completed, tool, until


class Consumer(Native):
    def __init__(self, binary, script):
        super().__init__(binary, script, {"shell": "allow", "subagent": "allow", "read": "allow",
            "opencode_session_move": "allow", "external_directory": "allow"})
        self.height = 40
        self.grid_lock = threading.Lock()
        self.grid = [[" " for _ in range(110)] for _ in range(self.height)]
        self.cursor = (0, 0)
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        config = self.home / "config/opencode/opencode.json"
        value = json.loads(config.read_text())
        value["provider"]["fixture"]["models"]["gpt-shell-controls"] = value["provider"]["fixture"]["models"].pop("m")
        value["model"] = "fixture/gpt-shell-controls"
        value["agent"]["helper"] = {"mode": "subagent", "permission": {"shell": "allow"}}
        config.write_text(json.dumps(value))
        self.restored = False

    def stop(self, crash=False, expected_exit=0):
        if self.process is not None and not crash and self.process.poll() is None:
            self.send(b"\x03" * 6)
            self.process.wait(timeout=8)
            self.restored = bool(termios.tcgetattr(self.fd)[3] & termios.ICANON and termios.tcgetattr(self.fd)[3] & termios.ECHO)
            assert self.restored, "owned native PTY did not restore canonical/echo modes"
        super().stop(crash=crash, expected_exit=expected_exit)

    def consume_pty(self, chunk):
        # An incremental bounded grid keeps unchanged cells; a rolling raw tail
        # cannot reconstruct them after many small real terminal updates.
        with self.grid_lock:
            text = self.pending + self.decoder.decode(chunk)
            row, col = self.cursor
            i = 0
            while i < len(text):
                if text[i] == "\x1b":
                    match = re.match(r"\x1b\[[0-9;?]*[ -/]*[@-~]", text[i:])
                    if match:
                        part = match[0]; i += len(part)
                        if part[-1] in "Hf":
                            values = [int(v or "1") for v in part[2:-1].split(";")]
                            row, col = (values + [1])[0] - 1, (values + [1])[1] - 1
                        elif part.endswith("2J"):
                            self.grid = [[" " for _ in range(110)] for _ in range(self.height)]
                        elif part.endswith("K") and 0 <= row < self.height:
                            for x in range(max(col, 0), 110): self.grid[row][x] = " "
                        continue
                    osc = re.match(r"\x1b\][^\x07]*?(?:\x07|\x1b\\)", text[i:])
                    if osc: i += len(osc[0]); continue
                    if text[i:].startswith(("\x1b[", "\x1b]")) or i + 1 == len(text): break
                    i += 2; continue
                ch = text[i]; i += 1
                if ch == "\r": col = 0
                elif ch == "\n": row += 1
                elif ch.isprintable():
                    if 0 <= row < self.height and 0 <= col < 110: self.grid[row][col] = ch
                    col += 1
            self.pending = text[i:]
            assert len(self.pending) <= 4096, "fixture partial terminal sequence bound"
            self.cursor = (row, col)

    def screen(self):
        with self.grid_lock:
            return ["".join(line) for line in self.grid]

    def lower(self):
        return "\n".join(self.screen()[-16:])

    def list(self):
        self.send(b"\x13")
        until(lambda: "Shell" in self.lower(), "real lower Shell composer did not open")

    def pick(self, marker):
        for _ in range(8):
            if any("› RUNNING" in row and marker in row for row in self.screen()): return
            self.send(b"\x1b[B")
            time.sleep(.04)
        raise AssertionError("real Shell selection missing: " + marker + " " + self.lower())

    def view(self, marker):
        self.pick(marker)
        self.send(b"\r")
        until(lambda: "status: running" in self.lower() and "LIVE-" + marker in self.lower(), "real live viewer lacks supervisor output: " + marker)

    def jobs(self):
        return self.rows("SELECT operation_id,session_id,phase,provenance,process,outcome,delivery_id,message_id FROM shell_jobs ORDER BY rowid")

    def job_named(self, marker):
        return next((row for row in self.jobs() if marker + ".pid" in json.loads(row[3])["command"]), None)

    def terminal(self, marker, state, notice=True):
        job = until(lambda: self.job_named(marker) if self.job_named(marker) and self.job_named(marker)[2] == "terminal" and (not notice or self.job_named(marker)[7]) else None, "terminal durable receipt absent: " + marker)
        assert json.loads(job[5])["state"] == state
        return job


def command(marker):
    return (f"printf '%s' $$ > {marker}.pid; printf one >> {marker}.admission; printf 'LIVE-{marker}'; "
            f"touch {marker}.entered; while [ ! -f {marker}.release ]; do sleep .01; done; "
            f"printf 'FINAL-{marker}'; touch {marker}.effect")


def snapshot(native, marker):
    job = native.job_named(marker)
    provenance, process = json.loads(job[3]), json.loads(job[4])
    assert process["pid"] == int(native.project.joinpath(marker + ".pid").read_text())
    assert provenance["operation"] == job[0] and provenance["session"] == job[1]
    assert provenance["location"] == str(native.project) and provenance["cwd"] == str(native.project)
    assert provenance["model"] == "gpt-shell-controls" and provenance["provider"] == "fixture"
    assert native.project.joinpath(marker + ".admission").read_text() == "one"
    return {"operation": job[0], "source": job[1], "location": provenance["location"], "generation": provenance["generation"],
            "model": provenance["model"], "pid": process["pid"]}


def convert(binary):
    def script(owner, count, request):
        return tool("shell", {"command": command("FG"), "background": False}, "exact-fg-call") if count == 1 else completed("NATURAL-CONTINUATION")
    with Consumer(binary, script) as native:
        native.start(); native.send(b"foreground\r")
        until(lambda: native.project.joinpath("FG.entered").exists(), "real FG execution barrier absent")
        receipt = snapshot(native, "FG")
        assert len(native.requests) == 1 and not native.settled()
        assert native.rows("SELECT count(*) FROM tool_operations WHERE state='started'") == [(1,)]
        native.list(); native.view("FG")
        native.send(b"\x02\x02")
        until(native.settled, "conversion did not release original FG await naturally")
        assert len(native.requests) == 2
        outputs = [item for item in native.requests[1]["input"] if item.get("type") == "function_call_output"]
        assert len(outputs) == 1 and outputs[0]["call_id"] == "exact-fg-call"
        running = json.loads(outputs[0]["output"])
        assert running["status"] == "running" and running["shellID"] == receipt["operation"]
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_background'") == [(1,)]
        assert snapshot(native, "FG") == receipt
        native.project.joinpath("FG.release").touch()
        job = native.terminal("FG", "completed")
        until(lambda: "status: completed" in native.lower() and "FINAL-FG" in native.lower(), "already-open exact viewer lost final flush after running-list removal")
        assert "RUNNING" not in native.lower()
        native.send(b"\x02\x02\x04\x04")
        assert native.process.poll() is None
        assert len(native.requests) == 2 and native.project.joinpath("FG.effect").exists()
        assert native.rows("SELECT count(*) FROM tool_operations") == [(1,)]
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(1,)]
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_terminal'") == [(1,)]
        assert not Path(f"/proc/{receipt['pid']}").exists()
        native.stop(); native.start()
        assert len(native.requests) == 2 and snapshot(native, "FG") == receipt
        assert native.job_named("FG")[7] == job[7]
        native.stop()
        assert not native.errors, native.errors
        print(json.dumps({"case": "same-pid-fg-conversion-live-final-repeat-reopen", "status": "PASS", **receipt,
            "requests": 2, "intents": 1, "results": 1, "notices": 1, "effects": 1, "idlePOST": 0, "PTYrestored": native.restored}))


def child_kill_move(binary):
    busy, continue_parent = threading.Event(), threading.Event()
    def script(owner, count, request):
        if count == 1:
            events = tool("shell", {"command": command("SIBLING"), "background": True}, "sibling-call")[:-1]
            events += tool("subagent", {"agent": "helper", "description": "Owned child", "prompt": "Shell child barrier"}, "child-call")
            return events
        if count == 2: return tool("shell", {"command": command("CHILD"), "background": False}, "child-shell-call")
        if count == 3: return completed("CHILD-NATURAL-CONTINUATION")
        if count == 4:
            busy.set(); assert continue_parent.wait(15)
            return tool("opencode_session_move", {"directory": str(owner.destination)}, "parent-move")
        return completed("PARENT-NATURAL-CONTINUATION")
    with Consumer(binary, script) as native:
        native.destination = native.root / "destination"; native.destination.mkdir()
        try:
            native.start(); native.send(b"children\r")
            until(lambda: native.project.joinpath("CHILD.entered").exists(), "child real execution barrier absent")
            child, sibling = snapshot(native, "CHILD"), snapshot(native, "SIBLING")
            assert child["source"] != sibling["source"]
            assert native.rows("SELECT parent_id FROM sessions WHERE id=?", (child["source"],)) == [(sibling["source"],)]
            native.list(); native.view("CHILD")
            native.send(b"\x04\x04\x02")
            job = native.terminal("CHILD", "cancelled", notice=False)
            assert busy.wait(5), "selected child kill cancelled unrelated parent turn"
            until(lambda: "status: cancelled" in native.lower(), "child opened viewer not frozen on its own terminal output")
            assert native.rows("SELECT status FROM turns WHERE session_id=?", (sibling["source"],)) == [("started",)]
            assert native.job_named("SIBLING")[2] == "running"
            assert Path(f"/proc/{sibling['pid']}").exists() and not Path(f"/proc/{child['pid']}").exists()
            assert not native.project.joinpath("CHILD.effect").exists() and job[7] is None
            continue_parent.set()
            until(lambda: native.rows("SELECT value FROM prefs WHERE key=?", ("tui.session_location." + sibling["source"],)) == [(str(native.destination),)], "same parent was not moved at safe terminal boundary")
            assert snapshot(native, "CHILD") == child and snapshot(native, "SIBLING") == sibling
            # Original opened child identity survives same-ID parent Location adoption.
            until(lambda: "status: cancelled" in native.lower(), "parent move retargeted/lost original child capture viewer")
            native.send(b"\x1b")
            until(lambda: "RUNNING BG" in native.lower(), "authoritative list did not remove cancelled child")
            native.view("SIBLING")
            native.project.joinpath("SIBLING.release").touch()
            native.terminal("SIBLING", "completed")
            until(lambda: "status: completed" in native.lower() and "FINAL-SIBLING" in native.lower(), "moved parent lost original sibling final flush")
            assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(1,)]
            assert len(native.requests) == 5
            assert native.project.joinpath("SIBLING.effect").exists() and not native.destination.joinpath("SIBLING.effect").exists()
            native.stop()
            assert not native.errors, native.errors
            print(json.dumps({"case": "selected-child-kill-sibling-parent-move-original-capture", "status": "PASS", "child": child,
                "sibling": sibling, "requests": 5, "jobs": 2, "notices": 1, "effects": 1, "parentInterrupted": False, "idlePOST": 0, "PTYrestored": native.restored}))
        finally:
            continue_parent.set()


def cancelled_conversion(binary):
    busy, release = threading.Event(), threading.Event()
    def script(owner, count, request):
        if count == 1: return tool("shell", {"command": command("CANCEL"), "background": False}, "cancel-fg")
        busy.set(); assert release.wait(15)
        return completed()
    with Consumer(binary, script) as native:
        try:
            native.start(); native.send(b"cancel-race\r")
            until(lambda: native.project.joinpath("CANCEL.entered").exists(), "FG cancel barrier absent")
            receipt = snapshot(native, "CANCEL")
            native.list(); native.view("CANCEL")
            native.send(b"\x02\x04\x04\x02")
            native.terminal("CANCEL", "cancelled")
            assert busy.wait(5)
            assert native.rows("SELECT status FROM turns") == [("started",)]
            assert not native.project.joinpath("CANCEL.effect").exists()
            assert len(native.requests) == 2
            release.set(); until(native.settled, "parent continuation was cancelled by selected shell controls")
            assert not Path(f"/proc/{receipt['pid']}").exists()
            native.stop()
            print(json.dumps({"case": "conversion-selected-cancel-repeat-parent-still-running", "status": "PASS", **receipt,
                "requests": 2, "jobs": 1, "notices": 1, "effects": 0, "parentInterrupted": False, "PTYrestored": native.restored}))
        finally: release.set()


def crash_conversion(binary):
    def script(owner, count, request):
        return tool("shell", {"command": command("CRASH"), "background": False}, "crash-fg") if count == 1 else completed()
    with Consumer(binary, script) as native:
        native.start(); native.send(b"crash-converted\r")
        until(lambda: native.project.joinpath("CRASH.entered").exists(), "FG crash barrier absent")
        receipt = snapshot(native, "CRASH")
        native.list(); native.view("CRASH"); native.send(b"\x02")
        until(native.settled, "converted await not released before crash")
        native.stop(crash=True); native.start()
        native.terminal("CRASH", "unknown")
        assert snapshot(native, "CRASH") == receipt
        assert len(native.requests) == 2 and not native.project.joinpath("CRASH.effect").exists()
        assert not Path(f"/proc/{receipt['pid']}").exists()
        native.stop(); native.start(); native.stop()
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(1,)]
        print(json.dumps({"case": "converted-crash-recovery-unknown-never-replay", "status": "PASS", **receipt,
            "requests": 2, "jobs": 1, "notices": 1, "effects": 0, "admissions": 1, "PTYrestored": native.restored}))


def completion(binary):
    held = ("printf '%s' $$ > FLUSH.pid; printf one >> FLUSH.admission; printf LIVE-FLUSH; touch FLUSH.entered; "
        "while [ ! -f FLUSH.release ]; do sleep .01; done; "
        "(trap 'printf FINAL-AFTER-LEADER-EXIT; touch FLUSH.effect; exit 0' TERM; touch descendant.ready; while :; do sleep .01; done) & "
        "printf '%s' $! > descendant.pid; while [ ! -f descendant.ready ]; do sleep .01; done; exit 0")
    def script(owner, count, request):
        return tool("shell", {"command": held, "background": False}, "flush-fg") if count == 1 else completed()
    with Consumer(binary, script) as native:
        native.start(); native.send(b"completion-race\r")
        until(lambda: native.project.joinpath("FLUSH.entered").exists(), "real FG flush barrier absent")
        receipt = snapshot(native, "FLUSH")
        native.list(); native.view("FLUSH")
        native.project.joinpath("FLUSH.release").touch()
        native.terminal("FLUSH", "completed", notice=False)
        until(native.settled, "unconverted FG did not settle terminally")
        until(lambda: "status: completed" in native.lower() and "FINAL-AFTER-LEADER-EXIT" in native.lower(), "supervisor final drain was lost after list removal: " + str(native.job_named("FLUSH")[5])[:600] + native.lower())
        native.send(b"\x02\x02\x04\x04")
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_background'") == [(0,)]
        assert native.rows("SELECT count(*) FROM events WHERE kind='shell_notice'") == [(0,)]
        outputs = [i for i in native.requests[1]["input"] if i.get("type") == "function_call_output"]
        assert len(outputs) == 1 and outputs[0]["call_id"] == "flush-fg" and "FINAL-AFTER-LEADER-EXIT" in outputs[0]["output"]
        assert len(native.requests) == 2
        assert not Path(f"/proc/{receipt['pid']}").exists()
        descendant = int(native.project.joinpath("descendant.pid").read_text())
        stat = Path(f"/proc/{descendant}/stat")
        until(lambda: not stat.exists() or stat.read_text().rsplit(") ", 1)[1].split()[0] == "Z", "owned final-flush descendant remained running")
        native.stop()
        print(json.dumps({"case": "completion-before-conversion-repeat-final-supervisor-drain", "status": "PASS", **receipt,
            "descendant": descendant, "requests": 2, "jobs": 1, "results": 1, "notices": 0, "effects": 1, "PTYrestored": native.restored}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(); parser.add_argument("binary", type=Path)
    parser.add_argument("--case", choices=["convert", "child", "cancel", "crash", "completion"])
    args = parser.parse_args(); binary = args.binary.resolve()
    print(json.dumps({"binary": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}))
    for name, run in [("convert", convert), ("child", child_kill_move), ("cancel", cancelled_conversion), ("crash", crash_conversion), ("completion", completion)]:
        if args.case is None or args.case == name: run(binary)
