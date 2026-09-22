#!/usr/bin/env python3
"""Validate this documentation package only. Does not build or test the product."""
from __future__ import annotations

import json
import hashlib
from pathlib import Path
import re
import sys
import tomllib

from progress import Invalid, Journal

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_GATES = tuple(f"A{i:02}" for i in range(1, 14))
JSON_INPUTS = (
    "planning/acceptance.json",
    "planning/baseline.lock.json",
    "planning/host-profile.json",
    "planning/tasks.json",
    "progress/STATE.json",
    "examples/static-models.user.json",
)
RUNNER_SURFACES = (
    "GOAL.md",
    "README.md",
    "AGENTS.md",
    "OPENCODE_RUST_MASTER_PLAN.md",
    "docs/AGENT_RUNBOOK.md",
    "docs/DECISIONS.md",
    "docs/CONTRACTS.md",
    "docs/DCP.md",
    "examples/oc-rs.toml",
    "planning/host-profile.json",
    "prompts/AGENT_GOAL.txt",
)
FORBIDDEN_RUNNER_MARKERS = (
    re.compile(r"\bCodex\s+CLI\b", re.IGNORECASE),
    re.compile(r"\bGPT\s*5(?:\.\d+)*\s+(?:Luna|Sol)\b", re.IGNORECASE),
    re.compile(r"\bCODEX_GOAL\.txt\b", re.IGNORECASE),
    re.compile(r"\brunner_model\b", re.IGNORECASE),
    re.compile(r"(?<!\w)/goal\b", re.IGNORECASE),
)


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
    for relative in JSON_INPUTS:
        json.loads((root / relative).read_text(encoding="utf-8"))
    for path in (root / "examples").glob("*.jsonc"):
        jsonc(path.read_text(encoding="utf-8"))
    for path in (root / "examples").glob("*.toml"):
        tomllib.loads(path.read_text(encoding="utf-8"))
    tasks = json.loads((root / "planning/tasks.json").read_text())["tasks"]
    tests = json.loads((root / "planning/acceptance.json").read_text())["tests"]
    by_id, test_ids = {t["id"]: t for t in tasks}, {t["id"] for t in tests}
    assert len(by_id) == len(tasks), "Duplicate task IDs"
    assert len(test_ids) == len(tests), "Duplicate test IDs"
    acceptance = (root / "GOAL.md").read_text()
    goal_gates = tuple(re.findall(r"^\*\*(A\d{2}) [^:]+:\*\*", acceptance, re.MULTILINE))
    assert goal_gates == EXPECTED_GATES, "GOAL must contain exact A01-A13 gate headings"
    # High-level gates are shared scope references, not detailed executable IDs.
    # Keep detailed acceptance ownership strict without rewriting protected tasks.
    gate_ids = set(goal_gates)
    assert not test_ids & gate_ids, "Detailed test IDs must not shadow GOAL gates"
    for test in tests:
        for field in ("id", "title", "expected"):
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

    owners = {test_id: [] for test_id in test_ids}
    for task in tasks:
        visit(task["id"])
        spec = root / task["spec"]
        assert spec.is_file(), f"Missing specification: {spec}"
        assert len(set(task["tests"])) == len(task["tests"]), f"Duplicate test reference: {task['id']}"
        assert set(task["tests"]) <= test_ids | gate_ids, f"Unknown test referenced by {task['id']}"
        for test_id in task["tests"]:
            if test_id in test_ids:
                owners[test_id].append(task["id"])
        assert task["evidence"] == f"evidence/{task['id']}/report.md", "Unexpected evidence target"
    assert all(owners.values()), f"Unassigned acceptance tests: {sorted(t for t, owner in owners.items() if not owner)}"
    duplicated = {test_id: owner for test_id, owner in owners.items() if len(owner) > 1}
    assert not duplicated, f"Acceptance tests need one owner: {duplicated}"
    objective = (root / "prompts/AGENT_GOAL.txt").read_text()
    assert objective.strip(), "Agent objective must not be empty"
    for relative in RUNNER_SURFACES:
        text = (root / relative).read_text(encoding="utf-8")
        for marker in FORBIDDEN_RUNNER_MARKERS:
            assert not marker.search(text), f"Runner-specific authoring contract in {relative}"
    final_template = (root / "evidence/FINAL.template.md").read_text()
    final_gates = tuple(re.findall(r"^## (A\d{2})$", final_template, re.MULTILINE))
    assert final_gates == EXPECTED_GATES, "FINAL template must contain exact A01-A13 sections"
    for path in ("docs/AGENT_RUNBOOK.md", "docs/ROADMAP.md", "roadmap/M6.md", "evidence/README.md"):
        assert "A01–A13" in (root / path).read_text(), f"Stale gate range in {path}"
    baseline = json.loads((root / "planning/baseline.lock.json").read_text())
    for project in ("opencode", "openproxy", "dcp"):
        assert re.fullmatch(r"[0-9a-f]{40}", baseline[project]["commit"])
    assert baseline["dcp"]["license"] == "AGPL-3.0-or-later"
    snapshot = root / baseline["discovery"]["snapshot"]
    assert snapshot.is_file()
    assert hashlib.sha256(snapshot.read_bytes()).hexdigest() == baseline["discovery"]["sha256"]
    reference = snapshot.read_text()
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
    return {"tasks": len(tasks), "acceptance_specifications": len(tests), "goal_characters": len(objective)}


if __name__ == "__main__":
    try:
        counts = validate()
        print("OK: documentation/registry/examples/journal structure only")
        print(json.dumps(counts, ensure_ascii=False))
    except (AssertionError, ValueError, OSError, KeyError, TypeError, Invalid) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
