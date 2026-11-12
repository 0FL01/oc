#!/usr/bin/env python3
"""Run the first hard gate against an explicitly selected stock or candidate ELF.

No user config/service is mounted. The child runs in a UID-preserving, loopback-
only PID/network namespace. Artifacts stay in an owned scratch directory.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--opencode", type=Path, required=True)
    parser.add_argument("--child", type=Path, required=True)
    parser.add_argument("--inner", action="store_true")
    parser.add_argument("--candidate", action="store_true", help="require the awaited upstream lifecycle API")
    parser.add_argument("--case", choices=["normal", "crash", "sighup", "slow-sighup", "sigterm", "sigint", "sigkill", "disabled", "disable", "reload", "resize", "missing", "bad-hello", "silent", "seed", "session", "shutdown-crash", "stuck"], default="normal")
    args = parser.parse_args()
    if not args.inner:
        root = Path(tempfile.mkdtemp(prefix="oc2-plugin-", dir=os.environ.get("TMPDIR")))
        print(f"artifacts: {root}", flush=True)
        command = [
            "bwrap", "--unshare-user", "--uid", str(os.getuid()), "--gid", str(os.getgid()),
            "--unshare-net", "--unshare-pid", "--die-with-parent", "--ro-bind", "/", "/",
            "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/home", "--tmpfs", "/run",
            "--tmpfs", "/opt", "--tmpfs", "/tmp", "--bind", str(root), "/tmp/work",
            "--ro-bind", str(args.opencode.resolve()), "/opt/opencode",
            "--ro-bind", str(Path(__file__).resolve()), "/opt/lifecycle.py",
            "--ro-bind", str(Path(__file__).parent.resolve() / "fixture"), "/opt/plugin",
        ]
        command += ["--ro-bind", str(args.child.resolve()), "/opt/terminal-child"]
        command += [
            "--chdir", "/tmp/work", "--", "/usr/bin/python3", "-I", "/opt/lifecycle.py",
            "--inner", "--opencode", "/opt/opencode", "--case", args.case,
        ]
        command += ["--child", "/opt/terminal-child"]
        if args.candidate:
            command += ["--candidate"]
        return subprocess.call(command, env={"PATH": "/usr/bin:/bin"})

    links = subprocess.check_output(["/usr/sbin/ip", "-o", "link"], text=True).splitlines()
    assert len(links) == 1 and ": lo:" in links[0], links
    assert not subprocess.check_output(["/usr/sbin/ip", "route", "show"])
    assert not subprocess.check_output(["/usr/sbin/ip", "-6", "route", "show", "default"])
    root = Path("/tmp/work")
    initial_session = None
    if args.case == "session":
        command = [sys.executable, "-I", __file__, "--inner", "--opencode", str(args.opencode), "--case", "seed"]
        command += ["--child", str(args.child)]
        if args.candidate:
            command += ["--candidate"]
        subprocess.run(command, check=True)
        seed = root / "events.jsonl"
        initial_session = next(e["id"] for e in map(json.loads, seed.read_text().splitlines()) if e["event"] == "seed")
        seed.rename(root / "seed.events.jsonl")
        (root / "result.json").rename(root / "seed.result.json")
    for name in ["home", "config", "data", "cache", "state", "runtime", "project"]:
        (root / name).mkdir(mode=0o700, exist_ok=True)
    config = {"$schema": "https://opencode.ai/config.json"}
    if args.case != "disabled":
        config["plugins"] = ["/opt/plugin"]
    (root / "project/opencode.json").write_text(json.dumps(config))
    env = {
        "PATH": "/opt:/usr/bin:/bin", "HOME": str(root / "home"), "TERM": "xterm-256color",
        "TMPDIR": "/tmp", "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_STATE_HOME": str(root / "state"), "XDG_RUNTIME_DIR": str(root / "runtime"),
        "OPENCODE_CONFIG_DIR": str(root / "config"), "OC2_LIFECYCLE_LOG": str(root / "events.jsonl"),
    }
    if args.case == "slow-sighup":
        env["OC2_LIFECYCLE_CLEANUP_DELAY"] = "0.25"
    env["OC2_LIFECYCLE_CHILD"] = str(args.child)
    env["OC2_LIFECYCLE_MODE"] = args.case
    env["OC2_LIFECYCLE_HOST"] = "candidate" if args.candidate else "stock"
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 120, 0, 0))
    original = termios.tcgetattr(slave)
    command = [str(args.opencode)] + (["--session", initial_session] if initial_session else [])
    process = subprocess.Popen(command, cwd=root / "project", env=env,
                               stdin=slave, stdout=slave, stderr=slave, start_new_session=True)
    capture = bytearray()

    def events():
        path = root / "events.jsonl"
        return [json.loads(line) for line in path.read_text().split("\n")[:-1]] if path.exists() else []

    def pump():
        if select.select([master], [], [], 0.02)[0]:
            data = os.read(master, 65536)
            capture.extend(data)
            assert len(capture) <= 16 * 1024 * 1024, "PTY output limit"
            if b"\x1b[6n" in data:
                os.write(master, b"\x1b[1;1R")

    def until(predicate, description, seconds=15):
        deadline = time.monotonic() + seconds
        while not predicate():
            assert time.monotonic() < deadline, f"timeout: {description}; events={events()}"
            pump()

    result = {}

    def stock_probe():
        assert process.poll() is None, "host exited instead of returning to stock"
        before = len(capture)
        os.write(master, b"\x10")  # Ctrl+P: stock command palette, no backend effects.
        until(lambda: b"Commands" in capture[before:], "stock command palette after return")

    try:
        if args.case == "disabled":
            until(lambda: b"OpenCode" in capture or b"opencode" in capture, "stock startup")
            assert not events(), events()
            process.send_signal(signal.SIGHUP)
        elif args.case in ["missing", "bad-hello", "silent"]:
            until(lambda: any(e["event"] == "setup.error" for e in events()), "startup refusal")
            until(lambda: any(e["event"] == "cleanup.end" for e in events()), "startup cleanup")
            assert not any(e["event"] in ["suspend", "activate", "child.ready"] for e in events()), events()
            stock_probe()
            process.send_signal(signal.SIGHUP)
        else:
            until(lambda: any(e["event"] == "child.ready" for e in events()), "child activation")
            log = events()
            lifecycle = [e for e in log if e["event"] not in ["spawn", "seed"]]
            assert [e["event"] for e in lifecycle[:4]] == ["setup", "hello", "suspend", "activate"], log
            assert len([e for e in log if e["event"] == "hello"]) == 1
            setup = lifecycle[0]
            assert setup["awaitedLifecycle"] == args.candidate, setup
            assert setup["version"] == "2.2.7", setup
            assert setup["location"]["directory"] == str(root / "project"), setup
            assert setup["clientLocation"]["directory"] == str(root / "project"), setup
            assert setup["route"] == ({"type": "session", "sessionID": initial_session} if initial_session else {"type": "home"}), setup
            if args.case == "resize":
                for _ in range(5):
                    pump()
                before = len(capture)
                raw = termios.tcgetattr(slave)
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))
                os.killpg(process.pid, signal.SIGWINCH)
                for _ in range(15):
                    pump()
                assert termios.tcgetattr(slave) == raw, "host changed child terminal mode on resize"
                assert len(capture) == before, f"host wrote {len(capture) - before} bytes while suspended"
            if args.case in ["disable", "reload"]:
                config["plugins"] = []
                (root / "project/opencode.json").write_text(json.dumps(config))
                until(lambda: any(e["event"] == "cleanup.end" for e in events()), "plugin disable")
                assert any(e["event"] == "resume" for e in events()), events()
                if args.case == "reload":
                    config["plugins"] = ["/opt/plugin"]
                    (root / "project/opencode.json").write_text(json.dumps(config))
                    until(lambda: len([e for e in events() if e["event"] == "child.ready"]) == 2, "plugin reload")
                    names = [e["event"] for e in events()]
                    assert names.index("cleanup.end") < names.index("setup", 1), names
            if args.case in ["normal", "crash", "seed", "session"]:
                os.write(master, b"c" if args.case == "crash" else b"q")
                until(lambda: any(e["event"] == "cleanup.end" for e in events()), "return to stock")
                assert any(e["event"] == "resume" for e in events()), events()
                stock_probe()
            process.send_signal({"sigterm": signal.SIGTERM, "sigint": signal.SIGINT, "sigkill": signal.SIGKILL}.get(args.case, signal.SIGHUP))
        until(lambda: process.poll() is not None, "parent exit")
        for _ in range(3):
            pump()
        child_pids = [e["pid"] for e in events() if e["event"] == "spawn"]
        assert all(not Path(f"/proc/{pid}").exists() for pid in child_pids), "owned child outlived host"
        restored = termios.tcgetattr(slave)
        modes = {}
        for match in re.finditer(rb"\x1b\[\?([0-9;]+)([hl])", capture):
            for mode in match[1].split(b";"):
                modes[int(mode)] = match[2] == b"h"
        result = {
            "case": args.case, "candidate": args.candidate, "exit": process.returncode,
            "terminal_restored": restored == original,
            "cursor_shown": capture.rfind(b"\x1b[?25h") > capture.rfind(b"\x1b[?25l"),
            "canonical": bool(restored[3] & termios.ICANON), "echo": bool(restored[3] & termios.ECHO),
            # Command audit, not a model of every terminal emulator's mode interactions.
            "last_mode_commands": modes,
            "events": events(),
        }
        assert restored == original, result
        assert result["cursor_shown"], "cursor left hidden"
        if args.candidate and args.case != "sigkill":
            assert not any(modes.get(mode, False) for mode in [1000, 1002, 1003, 1004, 1006, 1015, 1049, 2004, 2026]), result
            log = events()
            names = [e["event"] for e in log]
            if args.case not in ["disabled", "disable", "missing", "bad-hello", "silent"]:
                assert "renderer.destroy" in names, log
            if "renderer.destroy" in names:
                last_exit = max(i for i, name in enumerate(names) if name == "child.exit")
                last_cleanup = max(i for i, name in enumerate(names) if name == "cleanup.end")
                last_resume = max(i for i, name in enumerate(names) if name == "resume")
                assert last_exit < last_cleanup < names.index("renderer.destroy"), log
                assert last_exit < last_resume < names.index("renderer.destroy"), log
                assert all(not e["destroyed"] for e in log if e["event"] == "cleanup.end"), log
            if args.case == "stuck":
                assert any(e["event"] == "child.exit" and e["signal"] == "SIGKILL" for e in log), log
        print(json.dumps(result))
    finally:
        # Only this fixture's PID namespace/processes; never a user's owner.
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        (root / "pty.bin").write_bytes(capture)
        (root / "result.json").write_text(json.dumps(result, indent=2))
        os.close(master)
        os.close(slave)
    return 0


if __name__ == "__main__":
    sys.exit(main())
