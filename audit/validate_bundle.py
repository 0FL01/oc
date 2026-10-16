#!/usr/bin/env python3
"""Read-only validation of this audit package. Never builds or tests OC."""
from __future__ import annotations
import json
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent


def load(name: str):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


def validate() -> dict[str, object]:
    tasks = load("tasks.append.json")["tasks"]
    tests = load("acceptance.append.json")["tests"]
    state = load("state.append.json")["tasks"]
    findings = load("FINDINGS.json")["findings"]
    sources = load("SOURCES.json")
    estimate = load("COMPLETION_ESTIMATE.json")
    by_id = {task["id"]: task for task in tasks}
    test_ids = {test["id"] for test in tests}
    source_ids = {source["id"] for source in sources["sources"]}
    assert len(by_id) == len(tasks) == 12
    assert len(test_ids) == len(tests) == 40
    assert set(by_id) == set(state) == {f"T{i}" for i in range(31, 43)}
    counts = {key: 0 for key in test_ids}
    visited: set[str] = set()
    visiting: set[str] = set()

    def visit(key: str) -> None:
        if key == "T00":
            return  # Existing upstream registry prerequisite, not supplied here.
        assert key in by_id, key
        assert key not in visiting, f"cycle: {key}"
        if key in visited:
            return
        visiting.add(key)
        for dep in by_id[key]["depends_on"]:
            visit(dep)
        visiting.remove(key)
        visited.add(key)

    for task in tasks:
        visit(task["id"])
        assert task["phase"] == "M7"
        assert (HERE.parent / task["spec"]).is_file(), task["spec"]
        assert task["evidence"] == f"evidence/{task['id']}/report.md"
        text = (HERE.parent / task["spec"]).read_text(encoding="utf-8")
        for key in task["tests"]:
            assert key in test_ids and key in text
            counts[key] += 1
        info = state[task["id"]]
        assert info["status"] == "todo" and info["last_seq"] == 0
        assert info["latest"] is None and info["evidence"] is None
    assert set(counts.values()) == {1}, "each acceptance needs exactly one owner"
    for case in tests:
        assert all(isinstance(case[key], str) and case[key] for key in ("id", "title", "expected"))
    assert len({item["id"] for item in findings}) == len(findings) == 18
    for finding in findings:
        assert finding["repair_task"] in by_id
        assert set(finding["sources"]) <= source_ids
    patches = load("dependencies.patch.json")["append_dependencies"]
    assert patches == {"T27": ["T42"], "T30": ["T42"]}
    assert all("T27" not in task["depends_on"] and "T30" not in task["depends_on"] for task in tasks)
    weights = estimate["gate_weights"]
    assert sum(row["weight_percent"] for row in weights) == 100
    assert abs(sum(row["weight_percent"] * row["estimated_credit_percent"] / 100 for row in weights)
               - estimate["weighted_estimate_percent"]) < 1e-9
    for file in HERE.rglob("*.md"):
        text = file.read_text(encoding="utf-8")
        assert "\x00" not in text
        for target in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            if "://" not in target and not target.startswith("#"):
                resolved = (file.parent / target.split("#", 1)[0]).resolve()
                assert resolved.is_relative_to(HERE.resolve()) and resolved.exists(), (file.name, target)
    regression_text = (HERE / "regressions/audit_regressions.rs").read_text(encoding="utf-8")
    count = len(re.findall(r"^#\[test\]$", regression_text, re.MULTILINE))
    assert count == 10
    return {"audit_commit": sources["commit"], "repair_tasks": len(tasks),
            "acceptance_specifications": len(tests), "findings": len(findings),
            "proposed_rust_tests_not_executed": count,
            "weighted_scope_estimate": estimate["weighted_estimate_percent"],
            "validation_scope": "package structure, graph, references and arithmetic only"}


if __name__ == "__main__":
    print(json.dumps(validate(), ensure_ascii=False, indent=2))
