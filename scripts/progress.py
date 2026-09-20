#!/usr/bin/env python3
"""Small single-writer Markdown checkpoint utility. No code execution or network.

STATE.json is canonical; indices are derived. A report's existence is not proof
that its claims are correct. Linux/macOS Python 3.11+, standard library only.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import fcntl
import json
import os
from pathlib import Path
import re
import sys
import tempfile
from typing import Any

NOTE_LIMIT = 6144
INDEX_LIMIT = 8192
REPORT_LIMIT = 16384
STATUSES = {"todo", "active", "blocked", "done"}
HEADINGS = ("## Result", "## Checks", "## Risks", "## Next")


class Invalid(ValueError):
    """Actionable validation failure; no automatic recovery or destructive reset."""


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        raise Invalid(f"Cannot read valid JSON: {path.name}") from exc


def atomic(path: Path, text: str, *, immutable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink() or (immutable and path.exists()):
        raise Invalid(f"Refusing replacement: {path.name}")
    fd, tmp = tempfile.mkstemp(prefix=".checkpoint-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as out:
            out.write(text)
            out.flush()
            os.fsync(out.fileno())
        os.replace(tmp, path)
        directory_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def contained(root: Path, relative: str) -> Path:
    p = Path(relative)
    if p.is_absolute() or ".." in p.parts:
        raise Invalid("Only repository-relative paths without '..' are allowed")
    target = root / p
    resolved = target.resolve()
    if not resolved.is_relative_to(root.resolve()):
        raise Invalid("Path escapes repository (including symlinks)")
    # Avoid changing the meaning of generated filenames through an internal symlink.
    parent = target
    while parent != root:
        if parent.is_symlink():
            raise Invalid(f"Symlink not permitted in checkpoint path: {relative}")
        parent = parent.parent
    return target


class Journal:
    def __init__(self, root: Path):
        self.root = root.resolve()
        registry = read_json(self.root / "planning/tasks.json")
        self.tasks = registry["tasks"]
        self.by_id = {t["id"]: t for t in self.tasks}
        if len(self.by_id) != len(self.tasks):
            raise Invalid("Duplicate task IDs")
        for t in self.tasks:
            if not re.fullmatch(r"T\d{2,}", t["id"]) or not re.fullmatch(r"M\d+", t["phase"]):
                raise Invalid("Invalid task/phase identifier")
            if any(dep not in self.by_id for dep in t["depends_on"]):
                raise Invalid("Unknown dependency")
        self.directory = contained(self.root, "progress")
        self.state_path = contained(self.root, "progress/STATE.json")

    @contextmanager
    def lock(self):
        self.directory.mkdir(parents=True, exist_ok=True)
        with contained(self.root, "progress/.lock").open("a", encoding="utf-8") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise Invalid("Another progress writer is active") from exc
            try:
                yield
            finally:
                fcntl.flock(lock, fcntl.LOCK_UN)

    def initialize(self) -> None:
        if self.state_path.exists():
            raise Invalid("State already exists; use check/reindex, not init")
        state = {"schema_version": 2, "updated_at": now(), "current": None, "last_checkpoint_task": None,
                 "tasks": {t["id"]: {"status": "todo", "last_seq": 0,
                     "latest": None, "evidence": None, "reopen_reason": None} for t in self.tasks}}
        self.save(state)
        self.render(state)

    def load(self) -> dict[str, Any]:
        state = read_json(self.state_path)
        self.validate(state)
        return state

    def validate(self, state: dict[str, Any]) -> None:
        if state.get("schema_version") != 2 or set(state.get("tasks", {})) != set(self.by_id):
            raise Invalid("Registry/state mismatch; reconcile explicitly")
        last = state.get("last_checkpoint_task")
        if last is not None and (last not in self.by_id or not state["tasks"][last].get("latest")):
            raise Invalid("Invalid last_checkpoint_task")
        active = []
        for tid, info in state["tasks"].items():
            if info.get("status") not in STATUSES:
                raise Invalid(f"Invalid status: {tid}")
            seq = info.get("last_seq")
            if type(seq) is not int or seq < 0:
                raise Invalid(f"Invalid sequence: {tid}")
            expected = self.leaf(tid, seq) if seq else None
            if info.get("latest") != expected:
                raise Invalid(f"Latest reference mismatch: {tid}")
            if seq and not contained(self.root, expected).is_file():
                raise Invalid(f"Missing latest checkpoint: {tid}")
            directory = contained(self.root, self.task_dir(tid))
            leaves = sorted(int(p.stem) for p in directory.glob("*.md")
                            if re.fullmatch(r"\d{4,}", p.stem))
            if leaves != list(range(1, seq + 1)):
                raise Invalid(f"Orphan/missing checkpoint for {tid}; see docs/PROGRESS.md")
            if info["status"] == "active":
                active.append(tid)
            if info["status"] in {"active", "done"}:
                if not self.dependencies_done(state, tid):
                    raise Invalid(f"Unfinished dependency for {tid}")
            if info["status"] == "done":
                if not seq or info.get("evidence") != self.by_id[tid]["evidence"]:
                    raise Invalid(f"Completed task lacks checkpoint/evidence: {tid}")
                self.report(info["evidence"])
        if len(active) > 1 or state.get("current") != (active[0] if active else None):
            raise Invalid("One-active-task invariant violated")

    def task_dir(self, tid: str) -> str:
        return f"progress/{self.by_id[tid]['phase']}/{tid}"

    def leaf(self, tid: str, seq: int) -> str:
        return f"{self.task_dir(tid)}/{seq:04d}.md"

    def dependencies_done(self, state: dict[str, Any], tid: str) -> bool:
        return all(state["tasks"][dep]["status"] == "done"
                   for dep in self.by_id[tid]["depends_on"])

    def ready(self, state: dict[str, Any]) -> list[str]:
        return [t["id"] for t in self.tasks if state["tasks"][t["id"]]["status"] == "todo"
                and self.dependencies_done(state, t["id"])]

    def save(self, state: dict[str, Any]) -> None:
        state["updated_at"] = now()
        atomic(self.state_path, json.dumps(state, ensure_ascii=False, indent=2) + "\n")

    def note(self, relative: str) -> str:
        path = contained(self.root, relative)
        try:
            if path.stat().st_size > NOTE_LIMIT:
                raise Invalid(f"Note exceeds {NOTE_LIMIT} UTF-8 bytes")
            text = path.read_text(encoding="utf-8").strip() + "\n"
        except (OSError, UnicodeError) as exc:
            raise Invalid("Cannot read UTF-8 note") from exc
        if len(text.encode("utf-8")) > NOTE_LIMIT:
            raise Invalid(f"Note exceeds {NOTE_LIMIT} UTF-8 bytes")
        for heading in HEADINGS:
            if heading not in text.splitlines():
                raise Invalid(f"Note needs heading: {heading}")
        return text

    def report(self, relative: str) -> None:
        if not relative.startswith("evidence/"):
            raise Invalid("Report must be inside evidence/")
        path = contained(self.root, relative)
        try:
            size = path.stat().st_size
            if not 1 <= size <= REPORT_LIMIT or not path.read_text(encoding="utf-8").strip():
                raise Invalid("Evidence report must contain 1..16384 UTF-8 bytes")
        except (OSError, UnicodeError) as exc:
            raise Invalid("Missing or unreadable evidence report") from exc

    def start(self, state: dict[str, Any], tid: str) -> None:
        if tid not in self.by_id:
            raise Invalid("Unknown task")
        if state["current"]:
            raise Invalid("Finish/checkpoint/block the active task first")
        if state["tasks"][tid]["status"] not in {"todo", "blocked"}:
            raise Invalid("Use reopen for a completed task")
        if not self.dependencies_done(state, tid):
            raise Invalid("Task has unfinished dependencies")
        state["current"] = tid
        state["tasks"][tid]["status"] = "active"
        self.save(state)
        self.render(state)

    def checkpoint(self, state: dict[str, Any], note_path: str,
                   action: str = "checkpoint", report: str | None = None) -> None:
        tid = state["current"]
        if not tid:
            raise Invalid("No active task")
        note = self.note(note_path)  # Validate all inputs BEFORE durable changes.
        if action == "finish":
            if report != self.by_id[tid]["evidence"]:
                raise Invalid(f"Expected report: {self.by_id[tid]['evidence']}")
            self.report(report)
        info = state["tasks"][tid]
        seq = info["last_seq"] + 1
        leaf = self.leaf(tid, seq)
        atomic(contained(self.root, leaf), note, immutable=True)
        info.update(last_seq=seq, latest=leaf)
        state["last_checkpoint_task"] = tid
        if action == "finish":
            info.update(status="done", evidence=report)
            state["current"] = None
        elif action == "block":
            info["status"] = "blocked"
            state["current"] = None
        self.save(state)
        self.render(state)

    def reopen(self, state: dict[str, Any], tid: str, reason: str) -> None:
        if tid not in self.by_id or state["tasks"][tid]["status"] != "done":
            raise Invalid("Only completed tasks can be reopened")
        if state["current"]:
            raise Invalid("Block/finish the active task before reopening")
        if not reason.strip() or len(reason.encode("utf-8")) > 512:
            raise Invalid("Reopen reason must contain 1..512 bytes")
        dependents = {tid}
        while True:
            expanded = dependents | {t["id"] for t in self.tasks
                                     if set(t["depends_on"]) & dependents}
            if expanded == dependents:
                break
            dependents = expanded
        if any(state["tasks"][dep]["status"] == "done" for dep in dependents - {tid}):
            raise Invalid("Completed dependents would become invalid; revise graph explicitly")
        state["tasks"][tid].update(status="todo", evidence=None, reopen_reason=reason)
        self.save(state)
        self.render(state)

    def views(self, state: dict[str, Any]) -> dict[str, str]:
        current = state["current"]
        ready = self.ready(state)
        lines = ["# NOW — актуальный handoff", "", f"State updated: {state['updated_at']}",
                 f"Active: {current or 'нет'}", "", "Сверить Git status/diff до выполнения команд."]
        if current:
            t, info = self.by_id[current], state["tasks"][current]
            lines += [f"Task: {current} — {t['title']}", f"Spec: {t['spec']}",
                      f"Evidence target: {t['evidence']}", "", t["work"]]
            if info["latest"]:
                lines += ["", "Последний checkpoint этой задачи (проверить актуальность по Git):", "",
                          contained(self.root, info["latest"]).read_text(encoding="utf-8")]
        else:
            previous = state.get("last_checkpoint_task")
            if previous:
                info = state["tasks"][previous]
                lines += ["", f"Последний срез: {previous} [{info['status']}]; сверить незакоммиченный diff.", "",
                          contained(self.root, info["latest"]).read_text(encoding="utf-8")]
            lines += ["", "Следующий шаг: проверить зависимости и начать первую ready-задачу."]
        blocked = [tid for tid, item in state["tasks"].items() if item["status"] == "blocked"]
        lines += ["", "Ready (до 5): " + (", ".join(ready[:5]) or "нет"),
                  "Blocked: " + (", ".join(blocked) or "нет"), "",
                  "Done в журнале не означает READY всего продукта; см. GOAL.md."]
        views = {"progress/NOW.md": "\n".join(lines) + "\n"}
        phases = list(dict.fromkeys(t["phase"] for t in self.tasks))
        root = ["# Progress index", "", "Canonical state: STATE.json. Resume: NOW.md.", ""]
        for phase in phases:
            ts = [t for t in self.tasks if t["phase"] == phase]
            done = sum(state["tasks"][t["id"]]["status"] == "done" for t in ts)
            root.append(f"- [{phase}]({phase}/INDEX.md): {done}/{len(ts)} tasks done (не % parity).")
            index = [f"# {phase} — task index", "", f"Plan: ../../roadmap/{phase}.md", ""]
            for task in ts:
                tid, info = task["id"], state["tasks"][task["id"]]
                index.append(f"- [{tid}]({tid}/INDEX.md) [{info['status']}] — {task['title']}; "
                             f"latest: {Path(info['latest']).name if info['latest'] else 'нет'}.")
                task_index = [f"# {tid} — {task['title']}", "", f"Status: {info['status']}",
                              f"Spec: ../../../{task['spec']}", "",
                              "Последние 12 записей; остальные доступны по номеру/targeted search."]
                if info["last_seq"]:
                    task_index.append("")
                for seq in range(max(1, info["last_seq"] - 11), info["last_seq"] + 1):
                    task_index.append(f"- [{seq:04d}]({seq:04d}.md)")
                views[f"{self.task_dir(tid)}/INDEX.md"] = "\n".join(task_index) + "\n"
            views[f"progress/{phase}/INDEX.md"] = "\n".join(index) + "\n"
        views["progress/INDEX.md"] = "\n".join(root) + "\n"
        for relative, text in views.items():
            if len(text.encode("utf-8")) > INDEX_LIMIT:
                raise Invalid(f"Generated view exceeds {INDEX_LIMIT} bytes: {relative}")
        return views

    def render(self, state: dict[str, Any]) -> None:
        for relative, text in self.views(state).items():
            atomic(contained(self.root, relative), text)

    def check_views(self, state: dict[str, Any]) -> None:
        for relative, text in self.views(state).items():
            path = contained(self.root, relative)
            if not path.is_file() or path.read_text(encoding="utf-8") != text:
                raise Invalid(f"Stale generated view: {relative}; run reindex")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("init", "show", "check", "reindex"):
        sub.add_parser(command)
    sub.add_parser("start").add_argument("task")
    for command in ("checkpoint", "finish", "block"):
        p = sub.add_parser(command)
        p.add_argument("--note", required=True)
        if command == "finish":
            p.add_argument("--evidence", required=True)
    p = sub.add_parser("reopen")
    p.add_argument("task")
    p.add_argument("--reason", required=True)
    args = parser.parse_args(argv)
    try:
        journal = Journal(args.root)
        with journal.lock():
            if args.command == "init":
                journal.initialize()
            else:
                state = journal.load()
                if args.command == "start":
                    journal.start(state, args.task)
                elif args.command in {"checkpoint", "finish", "block"}:
                    journal.checkpoint(state, args.note, args.command, getattr(args, "evidence", None))
                elif args.command == "reopen":
                    journal.reopen(state, args.task, args.reason)
                elif args.command == "reindex":
                    journal.render(state)
                elif args.command == "check":
                    journal.check_views(state)
                elif args.command == "show":
                    print("Active:", state["current"] or "none")
                    print("Ready:", ", ".join(journal.ready(state)[:5]) or "none")
                    print("Read progress/NOW.md; verify actual Git diff before acting.")
        if args.command != "show":
            print(f"OK: {args.command} (journal structure only, not product acceptance)")
        return 0
    except (Invalid, OSError, KeyError, TypeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
