#!/usr/bin/env python3
"""Validate this documentation package only. Does not build or test the product."""
from __future__ import annotations

import json
from pathlib import Path
import re
import sys
import tomllib

from progress import Invalid, Journal

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_GATES = tuple(f"A{i:02}" for i in range(1, 14))
EXPECTED_TASKS = 31
EXPECTED_ACCEPTANCE = 81


def jsonc(text: str):
    """Remove comments outside strings; then remove trailing commas outside strings."""
    out, i, quoted = [], 0, False
    while i < len(text):
        c = text[i]
        if quoted:
            out.append(c)
            if c == "\\":
                i += 1
                if i < len(text):
                    out.append(text[i])
            elif c == '"':
                quoted = False
        elif c == '"':
            quoted = True
            out.append(c)
        elif text.startswith("//", i):
            end = text.find("\n", i + 2)
            i = len(text) if end == -1 else end
            out.append("\n")
            continue
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            if end == -1:
                raise ValueError("Unclosed JSONC comment")
            out.append(" ")
            i = end + 2
            continue
        else:
            out.append(c)
        i += 1
    text, out, i, quoted = "".join(out), [], 0, False
    while i < len(text):
        c = text[i]
        if quoted:
            out.append(c)
            if c == "\\":
                i += 1
                if i < len(text):
                    out.append(text[i])
            elif c == '"':
                quoted = False
        elif c == '"':
            quoted = True
            out.append(c)
        elif c == ",":
            j = i + 1
            while j < len(text) and text[j].isspace():
                j += 1
            if j == len(text) or text[j] not in "}]":
                out.append(c)
        else:
            out.append(c)
        i += 1
    return json.loads("".join(out))


def validate(root: Path = ROOT) -> dict[str, int]:
    for path in root.rglob("*.json"):
        if ".local" not in path.parts:
            json.loads(path.read_text(encoding="utf-8"))
    for path in (root / "examples").glob("*.jsonc"):
        jsonc(path.read_text(encoding="utf-8"))
    for path in (root / "examples").glob("*.toml"):
        tomllib.loads(path.read_text(encoding="utf-8"))
    tasks = json.loads((root / "planning/tasks.json").read_text())["tasks"]
    tests = json.loads((root / "planning/acceptance.json").read_text())["tests"]
    by_id, test_ids = {t["id"]: t for t in tasks}, {t["id"] for t in tests}
    assert len(by_id) == len(tasks), "Duplicate task IDs"
    assert len(test_ids) == len(tests), "Duplicate test IDs"
    assert len(tasks) == EXPECTED_TASKS, f"Expected {EXPECTED_TASKS} tasks"
    assert len(tests) == EXPECTED_ACCEPTANCE, f"Expected {EXPECTED_ACCEPTANCE} acceptance specifications"
    for test in tests:
        for field in ("id", "title", "expected", "implementation_status"):
            assert isinstance(test.get(field), str) and test[field].strip(), f"Missing acceptance field: {field}"
    visited, visiting = set(), set()

    def visit(tid):
        assert tid in by_id, f"Missing dependency: {tid}"
        assert tid not in visiting, f"Cyclic dependency: {tid}"
        if tid in visited:
            return
        visiting.add(tid)
        for dependency in by_id[tid]["depends_on"]:
            visit(dependency)
        visiting.remove(tid)
        visited.add(tid)

    used = set()
    for task in tasks:
        visit(task["id"])
        spec = root / task["spec"]
        assert spec.is_file(), f"Missing specification: {spec}"
        assert task["id"] in spec.read_text(), "Task not mentioned in phase specification"
        assert set(task["tests"]) <= test_ids, f"Unknown test referenced by {task['id']}"
        used.update(task["tests"])
        assert task["evidence"] == f"evidence/{task['id']}/report.md", "Unexpected evidence target"
    assert used == test_ids, f"Unassigned acceptance tests: {sorted(test_ids - used)}"
    goal = (root / "prompts/CODEX_GOAL.txt").read_text()
    assert goal.startswith("/goal ") and len(goal) < 4000, "Goal exceeds Codex limit"
    acceptance = (root / "GOAL.md").read_text()
    goal_gates = tuple(re.findall(r"^\*\*(A\d{2}) [^:]+:\*\*", acceptance, re.MULTILINE))
    assert goal_gates == EXPECTED_GATES, "GOAL must contain exact A01-A13 gate headings"
    final_template = (root / "evidence/FINAL.template.md").read_text()
    final_gates = tuple(re.findall(r"^## (A\d{2})$", final_template, re.MULTILINE))
    assert final_gates == EXPECTED_GATES, "FINAL template must contain exact A01-A13 sections"
    test_plan = (root / "docs/TEST_PLAN.md").read_text()
    planned_ids = re.findall(r"^\*\*([A-Z0-9]+\d{2}) — ", test_plan, re.MULTILINE)
    assert len(planned_ids) == len(set(planned_ids)), "Duplicate TEST_PLAN scenario headings"
    assert set(planned_ids) == test_ids, "TEST_PLAN scenario headings differ from acceptance registry"
    for path in ("prompts/CODEX_GOAL.txt", "docs/AGENT_RUNBOOK.md", "docs/ROADMAP.md", "roadmap/M6.md", "evidence/README.md"):
        assert "A01–A13" in (root / path).read_text(), f"Stale gate range in {path}"
    baseline = json.loads((root / "planning/baseline.lock.json").read_text())
    for project in ("opencode", "openproxy", "dcp"):
        assert re.fullmatch(r"[0-9a-f]{40}", baseline[project]["commit"])
    assert baseline["dcp"]["license"] == "AGPL-3.0-or-later"
    assert (root / baseline["discovery"]["snapshot"]).is_file()
    reference = (root / baseline["discovery"]["snapshot"]).read_text()
    assert "DISCOVERY_ATTEMPT_TIMEOUT_MS = 15000" in reference
    assert "DISCOVERY_TOTAL_TIMEOUT_MS = 30000" in reference
    sample = jsonc((root / "examples/opencode.jsonc").read_text())
    assert sample["mcp"]["chrome-devtools"]["enabled"] is False
    assert sample["mcp"]["codex_web"]["oauth"] is False
    assert sample["provider"]["ludka2"]["npm"] == "@ai-sdk/openai"
    assert not sample["provider"]["ludka2"].get("models"), "Do not bake in dynamic model IDs"
    journal = Journal(root)
    with journal.lock():
        state = journal.load()
        journal.check_views(state)
    return {"tasks": len(tasks), "acceptance_specifications": len(tests), "goal_characters": len(goal)}


if __name__ == "__main__":
    try:
        counts = validate()
        print("OK: documentation/registry/examples/journal structure only")
        print(json.dumps(counts, ensure_ascii=False))
    except (AssertionError, ValueError, OSError, KeyError, TypeError, Invalid) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
