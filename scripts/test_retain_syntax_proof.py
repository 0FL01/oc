import io
import json
import lzma
import tarfile
import unittest
from unittest.mock import patch

import retain_syntax_proof as proof


class SyntaxProofRetentionTests(unittest.TestCase):
    def test_vis37_builder_rejects_cross_mode_duplicate_build_and_unsealed_fixture(self):
        hashes = {name: "a" * 64 for name in ("capture.mjs", "tool_preview.mjs", "tool_preview_fixture.py",
                                             "question_forms.mjs", "question_fixture.py")}
        manifest = {"scripts/tui_capture/" + name: value for name, value in hashes.items()}
        native, original = str(proof.REPO / "target/debug/oc"), str(proof.TEMP / "t44-reference/package/bin/opencode")
        cases = [("wide", "001", 120, 40), ("small", "002", 80, 24),
                 ("wide", "002", 120, 40), ("tall", "002", 160, 48)]
        acquisitions = [(p, a, c, r, False) for p, a, c, r in cases]
        acquisitions += [(p, a, c, r, True) for p, a, c, r in cases +
                         [(p, a, c, r) for a in ("003", "004", "005", "006", "007", "008", "009", "010", "011") for p, c, r in
                          (("small", 80, 24), ("wide", 120, 40), ("tall", 160, 48))]]
        for profile, attempt, columns, rows, form in acquisitions:
            lock = {"oc": {"executable_path": native, "build_command": ["cargo", "build", "--locked"]},
                    "upstream": {"executable_path": original}, "runner_hashes": hashes,
                    "sources": {"upstream_commit": "2670273ff17da96f85c5826ced57aa1b368754fa"},
                    "profile": {"columns": columns, "rows": rows, "question_forms": True}}
            argv = ["node", str(proof.REPO / "scripts/tui_capture/capture.mjs"), "--reference", original, "--oc", native,
                    "--build-oc", "true", "--geometry", "true", "--sample", "short", "--sidebar", "hide",
                    "--tool-preview", "true", "--question-forms", "true", "--columns", str(columns), "--rows", str(rows),
                    "--output", str(proof.TEMP / (("vis37-form-current-" if form else "vis37-current-") + attempt) / profile)]
            commands = [{"role": "capture runner invocation", "argv": argv},
                        {"argv": ["cargo", "build", "--locked"], "exit_code": 0}]
            proof.check_vis37_build(lock, manifest, commands, profile, attempt, form)
            for addition in (["--question-forms", "true"], ["--sessions-interaction", "true"],
                             ["--prompt-caret", "true"], ["--output", "/foreign"]):
                bad = json.loads(json.dumps(commands))
                bad[0]["argv"] += addition
                with self.subTest(profile=profile, addition=addition), self.assertRaises(ValueError):
                    proof.check_vis37_build(lock, manifest, bad, profile, attempt, form)
            with self.assertRaises(ValueError):
                proof.check_vis37_build(lock, manifest, commands + commands[1:], profile, attempt, form)
            with self.assertRaises(ValueError):
                proof.check_vis37_build(lock, {**manifest, "scripts/tui_capture/question_fixture.py": "b" * 64},
                                        commands, profile, attempt, form)
            with self.assertRaises(ValueError):
                proof.check_vis37_build(lock, manifest, commands, profile, "012" if form else "003", form)
            with self.assertRaises(ValueError):
                proof.check_vis37_build(lock, manifest, commands, profile, attempt, not form)

    def test_vis37_form_separates_all_failed_historical_and_current_roots_and_real_keyboard(self):
        roots = proof.vis37_form_roots()
        self.assertEqual(len(roots), 13)
        current = proof.vis37_form_roots(current_only=True)
        self.assertEqual(len(current), 5)
        self.assertEqual(current["current-logs"], proof.TEMP / "vis37-form-current-004")
        self.assertEqual(current["current-driver"], proof.TEMP / "vis37-form-capture-r4.py")
        focused = proof.vis37_form_roots(current_only=True, current_attempt="005")
        cancelled = proof.vis37_form_roots(current_only=True, current_attempt="006")
        self.assertEqual(cancelled["current-logs"], proof.TEMP / "vis37-form-current-006")
        self.assertEqual(cancelled["current-driver"], proof.TEMP / "vis37-form-capture-r6.py")
        self.assertEqual(set(cancelled), set(focused))
        gap_fixed = proof.vis37_form_roots(current_only=True, current_attempt="007")
        self.assertEqual(gap_fixed["current-logs"], proof.TEMP / "vis37-form-current-007")
        self.assertEqual(gap_fixed["current-driver"], proof.TEMP / "vis37-form-capture-r7.py")
        qualified = proof.vis37_form_roots(current_only=True, current_attempt="008")
        self.assertEqual(qualified["current-logs"], proof.TEMP / "vis37-form-current-008")
        padding = proof.vis37_form_roots(current_only=True, current_attempt="009")
        self.assertEqual(padding["current-logs"], proof.TEMP / "vis37-form-current-009")
        self.assertEqual(padding["current-driver"], proof.TEMP / "vis37-form-capture-r9.py")
        pulse = proof.vis37_form_roots(current_only=True, current_attempt="010")
        self.assertEqual(pulse["current-logs"], proof.TEMP / "vis37-form-current-010")
        self.assertEqual(pulse["current-driver"], proof.TEMP / "vis37-form-capture-r10.py")
        status_row = proof.vis37_form_roots(current_only=True, current_attempt="011")
        self.assertEqual(status_row["current-logs"], proof.TEMP / "vis37-form-current-011")
        self.assertEqual(status_row["current-driver"], proof.TEMP / "vis37-form-capture-r11.py")
        self.assertEqual(qualified["current-driver"], proof.TEMP / "vis37-form-capture-r8.py")
        self.assertEqual(len(focused), 5)
        self.assertEqual(focused["current-logs"], proof.TEMP / "vis37-form-current-005")
        self.assertEqual(focused["current-driver"], proof.TEMP / "vis37-form-capture-r5.py")
        with self.assertRaises(ValueError):
            proof.vis37_form_roots(current_only=True, current_attempt="012")
        for profile in ("small", "wide", "tall"):
            self.assertEqual(current["current-" + profile], proof.TEMP / "vis37-form-current-004" / profile)
            self.assertEqual(focused["current-" + profile], proof.TEMP / "vis37-form-current-005" / profile)
        self.assertEqual(roots["failed-wide"], proof.TEMP / "vis37-form-current-001/wide")
        for profile in ("small", "wide", "tall"):
            self.assertEqual(roots["historical-" + profile], proof.TEMP / "vis37-form-current-002" / profile)
            self.assertEqual(roots["current-" + profile], proof.TEMP / "vis37-form-current-003" / profile)
        for period, attempt, helper in (("failed", "001", "vis37-form-capture.py"),
                                        ("historical", "002", "vis37-form-capture-r2.py"),
                                        ("current", "003", "vis37-form-capture-r3.py")):
            self.assertEqual(roots[period + "-logs"], proof.TEMP / ("vis37-form-current-" + attempt))
            self.assertEqual(roots[period + "-driver"], proof.TEMP / helper)
        facts = {"question_forms": {"tab_and_backtab_committed_without_submit": True,
                                    "return_after_tab_reopens_custom": True, "optional_empty_custom_tab_did_not_submit": True}}
        inputs = [{"kind": kind, "base64": data} for kind, data in
                  (("question_empty_custom_tab", "CQ=="), ("question_empty_custom_backtab", "G1ta"),
                   ("question_custom_tab_commit", "CQ=="), ("question_custom_backtab", "G1ta"),
                   ("question_custom_reopen_edit", "DQ=="), ("question_commit_custom", "DQ=="),
                   ("question_explicit_reply", "DQ=="))]
        proof.check_vis37_form_keyboard(facts, inputs)
        for bad in (inputs[:-1], inputs + inputs[-1:], list(reversed(inputs)),
                    [{**inputs[0], "base64": "DQ=="}] + inputs[1:]):
            with self.assertRaises(ValueError):
                proof.check_vis37_form_keyboard(facts, bad)
        with self.assertRaises(ValueError):
            proof.check_vis37_form_keyboard({"question_forms": {key: False for key in facts["question_forms"]}}, inputs)

    def test_vis37_multiselect_requires_the_painted_focus_before_a_separate_toggle(self):
        checks = {"question_forms": {key: True for key in
                  ("tab_and_backtab_committed_without_submit", "return_after_tab_reopens_custom",
                   "optional_empty_custom_tab_did_not_submit", "multiselect_focus_painted_before_toggle")}}
        navigation = [("question_empty_custom_tab", "CQ=="), ("question_empty_custom_backtab", "G1ta"),
                      ("question_custom_tab_commit", "CQ=="), ("question_custom_backtab", "G1ta"),
                      ("question_custom_reopen_edit", "DQ=="), ("question_commit_custom", "DQ=="),
                      ("question_explicit_reply", "DQ==")]
        events = [("question_python_focus", "G1tC"), ("question_select_python", "IA==")]
        inputs = [{"kind": kind, "base64": data} for kind, data in events + navigation]
        proof.check_vis37_form_keyboard(checks, inputs, focus_painted=True)
        for bad in ([{"kind": "question_select_python", "base64": "G1tCIA=="}] + inputs[2:],
                    list(reversed(inputs[:2])) + inputs[2:], inputs[:2] + inputs[1:]):
            with self.assertRaises(ValueError):
                proof.check_vis37_form_keyboard(checks, bad, focus_painted=True)
        checks["question_forms"]["multiselect_focus_painted_before_toggle"] = False
        with self.assertRaises(ValueError):
            proof.check_vis37_form_keyboard(checks, inputs, focus_painted=True)

    def test_vis37_owned_wire_and_durable_sources_reject_replay_raw_rewrite_and_fake_answer(self):
        questions = [{"header": "Languages and native runtimes", "question": "Pick languages?", "multiple": True},
                     {"header": "Platform and execution targets", "question": "Where Ω界?", "multiple": False}]
        argument, answers = {"questions": questions}, [["Rust", "Python"], ["custom Ω界"]]
        names = ("question-home question-pending question-one-selected question-multiple-selected question-long-field "
                 "question-custom-empty question-custom-edited question-review question-completed question-answered-card "
                 "question-switched question-reopened question-restarted question-cancel-pending question-dismissed "
                 "question-cancel-switched question-cancel-reopened").split()
        prompt, cancel = "VIS37 paired question consumer", "VIS37 dismiss the next question"
        for side in ("oc", "upstream"):
            output = json.dumps({**argument, "answers": answers}) if side == "oc" else (
                'User has answered your questions: "Pick languages?"="Rust, Python", "Where Ω界?"="custom Ω界".'
                " You can now continue with the user's answers in mind.")
            protocol = [{"kind": "provider", "operation": "question_title", "valid": True, "index": 1}]
            for index in (1, 2, 3):
                protocol.append({"kind": "provider", "operation": "question_main", "valid": True, "index": index,
                                 "registered_tools": ["question"], "user_texts": [prompt] + ([cancel] if index == 3 else []),
                                 "actual_results": [] if index == 1 else [{"type": "function_call_output",
                                                                          "call_id": "call_vis37_answer", "output": output}]})
            protocol += [{"kind": "fixture_tool_call", "name": "question", "call_id": call, "arguments": argument}
                         for call in ("call_vis37_answer", "call_vis37_cancel")]
            snapshots = {}
            for key in ("completed", "reopened", "restarted", "dismissed", "final"):
                cancelled = key in ("dismissed", "final")
                if side == "oc":
                    operations = [{"id": "answer", "name": "question", "provider_call_id": "call_vis37_answer", "state": "completed",
                                   "input": json.dumps(argument), "output": output}]
                    if cancelled:
                        operations.append({"id": "dismissed", "name": "question", "provider_call_id": "call_vis37_cancel", "state": "cancelled",
                                           "input": json.dumps(argument), "output": "error: question dismissed"})
                    messages = [{"role": "user", "text": text} for text in ([prompt, cancel] if cancelled else [prompt])]
                else:
                    operations = [{"message_id": "answer", "data": {"type": "tool", "name": "question", "id": "call_vis37_answer",
                                   "state": {"status": "completed", "input": argument, "metadata": {"answers": answers},
                                             "content": [{"type": "text", "text": output}]}}}]
                    if cancelled:
                        operations.append({"message_id": "dismissed", "data": {"name": "question", "id": "call_vis37_cancel",
                                           "state": {"status": "error", "input": argument}}})
                    messages = [{"type": "user", "data": json.dumps({"text": text})} for text in ([prompt, cancel] if cancelled else [prompt])]
                data = {"question_records": {"operations": operations, "messages": messages, "readonly": True}}
                if side == "oc":
                    data.update(tool_operations=operations, tool_output_resources=[])
                snapshot = {"kind": "tool_preview_snapshot", "observations": [{"data": data}], "artifacts": {},
                            "shell_effect": None, "mcp_calls": [], "mcp_lifecycle": []}
                snapshots[key] = snapshot
                protocol.append(snapshot)
            protocol += [{"kind": "exit", "generation": 0, "code": 0},
                         {"kind": "exit", "generation": 1, "code": -15, "termination": "forced_stop"}]
            facts = {"status": "OBSERVED_OWNED_QUESTION", "provider_requests": 4, "main_requests": 3,
                     "explicit_user_submissions": 2, "explicit_answer_submissions": 1, "question_calls": 2,
                     "answers": answers, "no_edit_submission": True, "no_restart_replay": True,
                     "cancellation_not_answered": True, "readonly_native_and_donor_source_records": True, **snapshots}
            stages = [{"stage": name, "status": "CAPTURED_TOOL_PREVIEW", "requests": 0 if i == 0 else 2 if i < 8 else 3 if i < 13 else 4,
                       "calls": 0 if i == 0 else 1 if i < 13 else 2} for i, name in enumerate(names)]
            checks = {"origin": side, "status": "PASS_BEHAVIOR_ONLY", "stages": stages, "question_forms": facts}
            proof.check_vis37_facts(checks, protocol, side)
            for kind in ("bool-count", "edit-generation", "missing-stage", "raw-rewrite", "cancel-replay", "wrong-call", "extra-effect"):
                bad, trace = json.loads(json.dumps(checks)), json.loads(json.dumps(protocol))
                if kind == "bool-count":
                    bad["question_forms"]["explicit_answer_submissions"] = True
                elif kind == "edit-generation":
                    bad["stages"][7]["requests"] = 3
                elif kind == "missing-stage":
                    bad["stages"].pop()
                elif kind == "raw-rewrite":
                    bad["question_forms"]["restarted"]["observations"][0]["data"]["question_records"]["messages"].clear()
                elif kind == "cancel-replay":
                    trace.append(trace[3])
                elif kind == "wrong-call":
                    trace[2]["actual_results"][0]["call_id"] = "call_vis37_cancel"
                else:
                    bad["question_forms"]["final"]["mcp_calls"] = [{"unknown_effect": True}]
                with self.subTest(side=side, kind=kind), self.assertRaises(ValueError):
                    proof.check_vis37_facts(bad, trace, side)

    def test_vis37_answered_glyph_assertion_keeps_all_blank_cells_unmodified(self):
        for columns, rows in ((80, 24), (120, 40), (160, 48)):
            blank = {"symbol": " ", "fg": "#ffffff", "bg": "#1e1e1e", "width": 1, "modifiers": []}
            native = {"columns": columns, "rows": rows, "cells": [[dict(blank) for _ in range(columns)] for _ in range(rows)],
                      "cursor": {"x": 5, "y": rows - 6, "visible": True, "shape": "block"}}
            for y, text in enumerate(("", "# Questions", "", "Pick languages?", "Rust, Python", ""), 2):
                native["cells"][y][1]["symbol"] = "┃"
                for x, character in enumerate(text, 5):
                    native["cells"][y][x].update(symbol=character, fg="#eeeeee")
            original = json.loads(json.dumps(native))
            original["cells"][3][3]["fg"] = "#eeeeee"
            before = json.dumps((native, original))
            proof.check_vis37_answered_card(native, original)
            self.assertEqual(json.dumps((native, original)), before)
            for kind in ("missing-title", "wrong-answer", "wrong-role", "wrong-caret", "short-frame"):
                bad = json.loads(json.dumps(native))
                if kind == "missing-title":
                    bad["cells"][3][5]["symbol"] = " "
                elif kind == "wrong-answer":
                    bad["cells"][6][5]["symbol"] = "X"
                elif kind == "wrong-role":
                    bad["cells"][6][5]["fg"] = "#888888"
                elif kind == "wrong-caret":
                    bad["cursor"]["x"] += 1
                else:
                    bad["cells"].pop()
                with self.subTest(columns=columns, kind=kind), self.assertRaises(ValueError):
                    proof.check_vis37_answered_card(bad, original)

    def test_vis12_build_is_bound_to_each_exact_mode_fixture_and_repository_output(self):
        for episode, attempt in (("history", None), ("remap", None), ("caret", None),
                                 ("caret", "001"), ("caret", "002"), ("caret", "003")):
            probe = "prompt_caret" if episode == "caret" else "prompt_history"
            hashes = {name: "a" * 64 for name in ("capture.mjs", probe + ".mjs", probe + "_fixture.py")}
            manifest = {"scripts/tui_capture/" + name: value for name, value in hashes.items()}
            native = str(proof.REPO / "target/debug/oc")
            original = str(proof.TEMP / "t44-reference/package/bin/opencode")
            lock = {"oc": {"executable_path": native, "build_command": ["cargo", "build", "--locked"]},
                    "upstream": {"executable_path": original}, "runner_hashes": hashes,
                    "sources": {"upstream_commit": "2670273ff17da96f85c5826ced57aa1b368754fa"}}
            argv = ["node", str(proof.REPO / "scripts/tui_capture/capture.mjs"), "--reference", original,
                    "--oc", native, "--build-oc", "true", "--geometry", "true", "--sample", "short",
                    "--sidebar", "hide", "--tool-preview", "true", "--columns", "80" if episode == "history" else "120",
                    "--rows", "24" if episode == "history" else "40", "--output",
                    str(proof.TEMP / ("vis12-current-001" if attempt is None else "vis12-wrap-current-" + attempt) / episode),
                    "--prompt-caret" if episode == "caret" else "--prompt-history",
                    "true" if episode == "caret" else "default" if episode == "history" else "remap"]
            commands = [{"role": "capture runner invocation", "argv": argv},
                        {"argv": ["cargo", "build", "--locked"], "exit_code": 0}]
            proof.check_vis12_build(lock, manifest, commands, episode, wrap_attempt=attempt)
            for addition in (["--build-oc", "true"], ["--sessions-interaction", "true"],
                             ["--output", "/unrelated"], ["--prompt-history", "default"]):
                invalid = json.loads(json.dumps(commands))
                invalid[0]["argv"] += addition
                with self.subTest(episode=episode, addition=addition), self.assertRaises(ValueError):
                    proof.check_vis12_build(lock, manifest, invalid, episode, wrap_attempt=attempt)
            for bad in ([{"argv": ["cargo", "build", "--locked"], "exit_code": 1}],
                        commands[1:] * 2, [{"argv": ["cargo", "build", "--locked", "--release"], "exit_code": 0}]):
                with self.subTest(episode=episode, builds=bad), self.assertRaises(ValueError):
                    proof.check_vis12_build(lock, manifest, commands[:1] + bad, episode, wrap_attempt=attempt)
            invalid = dict(manifest)
            invalid["scripts/tui_capture/" + probe + ".mjs"] = "b" * 64
            with self.assertRaises(ValueError):
                proof.check_vis12_build(lock, invalid, commands, episode, wrap_attempt=attempt)
            with self.assertRaisesRegex(ValueError, "fixed VIS12 wrap attempt"):
                proof.check_vis12_build(lock, manifest, commands, episode, wrap_attempt="004")

    def test_wrap_launch_lineage_distinguishes_reused_failed_and_fresh_qualified_roots(self):
        for attempt in ("001", "002", "003"):
            output = proof.TEMP / ("vis12-wrap-current-" + attempt) / "caret"
            name = "caret" if attempt == "001" else "caret-" + proof.digest(str(output).encode())[:12]
            root = proof.TEMP / "t44-reference/runs" / name
            for side in ("oc", "upstream"):
                home = root / side / "home"
                env = {"HOME": str(home), **{"XDG_" + key + "_HOME": str(home / key.lower())
                                             for key in ("CONFIG", "CACHE", "DATA", "STATE")}}
                protocol = [{"kind": "launch", "cwd": str(root / "project"), "env": env}]
                if attempt != "001":
                    protocol.append({"kind": "relaunch", "cwd": str(root / "project")})
                proof.check_vis12_wrap_launches(protocol, attempt, side)
                wrong = json.loads(json.dumps(protocol))
                wrong[-1]["cwd"] = str(proof.TEMP / "t44-reference/runs/unrelated/project")
                with self.assertRaises(ValueError):
                    proof.check_vis12_wrap_launches(wrong, attempt, side)
                wrong = json.loads(json.dumps(protocol))
                wrong[0]["env"]["XDG_DATA_HOME"] = "/foreign/data"
                with self.assertRaises(ValueError):
                    proof.check_vis12_wrap_launches(wrong, attempt, side)
                with self.assertRaises(ValueError):
                    proof.check_vis12_wrap_launches(protocol + protocol, attempt, side)

    def test_wrap_geometry_checks_whole_caret_and_partial_tail_without_blank_normalization(self):
        cell = {"symbol": " ", "fg": "#ffffff", "bg": "#1e1e1e", "width": 1, "modifiers": []}
        original = {"columns": 120, "rows": 40, "cells": [[dict(cell) for _ in range(120)] for _ in range(40)],
                    "cursor": {"x": 86, "y": 27, "visible": True, "shape": "block"}}
        for y in range(15, 28):
            for x in range(26, 26 + (60 if y == 27 else 70)):
                original["cells"][y][x].update(symbol="z", fg="#eeeeee")
        native = json.loads(json.dumps(original))
        original["cells"][27][86]["fg"] = "#eeeeee"
        before = json.dumps((native, original))
        proof.check_vis12_wrap_geometry(native, original)
        self.assertEqual(json.dumps((native, original)), before)
        for kind in ("missing-tail", "wrong-style", "wrong-caret", "short-frame"):
            wrong = json.loads(json.dumps(native))
            if kind == "missing-tail":
                wrong["cells"][27][85]["symbol"] = " "
            elif kind == "wrong-style":
                wrong["cells"][27][85]["fg"] = "#ffffff"
            elif kind == "wrong-caret":
                wrong["cursor"]["x"] = 26
            else:
                wrong["cells"].pop()
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                proof.check_vis12_wrap_geometry(wrong, original)

    def test_vis12_caret_rejects_navigation_generation_effects_and_raw_rewrite(self):
        names = ("caret-home caret-home-word-clicked caret-home-word-edited caret-wide-second-clicked "
                 "caret-wide-second-edited caret-combining-clicked caret-combining-edited caret-zwj-start-clicked "
                 "caret-zwj-start-edited caret-trailing-clicked caret-trailing-edited caret-wrap-gap-clicked "
                 "caret-wrap-gap-edited caret-blank-line caret-resized-word-clicked caret-resized-word-edited "
                 "caret-chip-neighbor caret-chip-expanded caret-overlay caret-overlay-restored caret-before-submit "
                 "caret-submitted caret-session-edited caret-reopened caret-restarted").split()
        data = {"prompt_user_messages": [{"text": "Проведи RECXON, жду план"}],
                "tool_operations": [], "tool_output_resources": [], "presentation": []}
        snapshot = {"kind": "tool_preview_snapshot", "observations": [{"data": data}], "artifacts": {},
                    "shell_effect": None, "mcp_calls": [], "mcp_lifecycle": []}
        facts = {"status": "OBSERVED_PROMPT_CARET", "provider_requests": 2, "explicit_user_submissions": 1,
                 "user_text": "Проведи RECXON, жду план", "native_durable_user_verified": True,
                 "tool_effects": 0, "reopen_restart_no_replay": True,
                 **{key: snapshot for key in ("before", "after", "restarted")}}
        checks = {"origin": "oc", "status": "PASS_BEHAVIOR_ONLY", "prompt_caret": facts,
                  "stages": [{"stage": name, "status": "CAPTURED_TOOL_PREVIEW", "calls": 0,
                              "requests": 0 if index < 21 else 2} for index, name in enumerate(names)]}
        proof.check_vis12_facts(checks, "caret", "oc")
        for key, value in (("provider_requests", True), ("explicit_user_submissions", 2), ("tool_effects", 1),
                           ("user_text", "Проведи RECON, жду план"), ("native_durable_user_verified", False),
                           ("reopen_restart_no_replay", False)):
            bad = json.loads(json.dumps(checks))
            bad["prompt_caret"][key] = value
            with self.subTest(fact=key), self.assertRaises(ValueError):
                proof.check_vis12_facts(bad, "caret", "oc")
        for key, value in (("requests", 1), ("calls", True), ("stage", "caret-home-word-edited")):
            bad = json.loads(json.dumps(checks))
            bad["stages"][1][key] = value
            with self.subTest(stage=key), self.assertRaises(ValueError):
                proof.check_vis12_facts(bad, "caret", "oc")
        bad = json.loads(json.dumps(checks))
        bad["prompt_caret"]["restarted"]["observations"][0]["data"]["prompt_user_messages"][0]["text"] += "changed"
        with self.assertRaises(ValueError):
            proof.check_vis12_facts(bad, "caret", "oc")
        bad = json.loads(json.dumps(checks))
        bad["prompt_caret"]["after"]["mcp_calls"] = [{"replayed": True}]
        with self.assertRaises(ValueError):
            proof.check_vis12_facts(bad, "caret", "oc")
        reference = json.loads(json.dumps(checks))
        reference["origin"] = "upstream"
        reference["prompt_caret"]["native_durable_user_verified"] = False
        for key in ("before", "after", "restarted"):
            reference["prompt_caret"][key]["observations"] = [{"data": {}}]
        proof.check_vis12_facts(reference, "caret", "upstream")
        with self.assertRaises(ValueError):
            proof.check_vis12_facts(reference, "caret", "oc")

    def test_middle_identity_and_historical_failure_are_not_interchangeable(self):
        facts = {"origin": "oc", "middle_case": "compact", "status": "PASS_BEHAVIOR_ONLY",
                 "checks": [{"stage": "compact-geometry", "actual_drag": True, "width": 5,
                             "restart_persistence_claim": False},
                            {"stage": "proof", "root_ids": {"Alpha": "a", "Beta": "b", "Gamma": "c"},
                             "requests": 6, "tool_calls": 0, "history_unchanged": True,
                             "closed_deck_not_resurrected": True, "reopened_same_id": "a"}]}
        proof.check_middle_facts(facts, "compact", "oc")
        for key, value in (("requests", True), ("requests", 8), ("tool_calls", 1),
                           ("reopened_same_id", "b"), ("history_unchanged", False),
                           ("root_ids", {"Alpha": "a", "Beta": "a", "Gamma": "c"})):
            invalid = json.loads(json.dumps(facts))
            invalid["checks"][1][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                proof.check_middle_facts(invalid, "compact", "oc")
        invalid = json.loads(json.dumps(facts))
        invalid["checks"][0]["restart_persistence_claim"] = True
        with self.assertRaises(ValueError):
            proof.check_middle_facts(invalid, "compact", "oc")
        historical = {"origin": "oc", "middle_case": "compact", "status": "FAILED",
                      "checks": [{"stage": "failure", "reason": "Actual original predicate failed"}]}
        proof.check_middle_facts(historical, "compact", "oc", historical=True)
        for case, past in (("compact", False), ("horizontal", True)):
            with self.assertRaises(ValueError):
                proof.check_middle_facts(historical, case, "oc", historical=past)
        native, original = str(proof.REPO / "target/debug/oc"), str(proof.TEMP / "t44-reference/package/bin/opencode")
        lock = {"oc": {"executable_path": native, "build_command": ["cargo", "build", "--locked"]},
                "upstream": {"executable_path": original}, "runner_hashes": {"capture.mjs": "a" * 64},
                "sources": {"upstream_commit": "2670273ff17da96f85c5826ced57aa1b368754fa"}}
        commands = [{"role": "capture runner invocation", "argv": ["node", str(proof.REPO / "scripts/tui_capture/capture.mjs"),
                     "--oc", native, "--reference", original, "--build-oc", "true", "--tab-middle-close", "true"]},
                    {"argv": ["cargo", "build", "--locked"], "exit_code": 0}]
        manifest = {"scripts/tui_capture/capture.mjs": "a" * 64}
        proof.check_middle_build(lock, manifest, commands)
        for field, value in (("executable_path", "/unrelated/oc"), ("build_command", ["cargo", "build", "--locked", "--manifest-path", "other/Cargo.toml"])):
            invalid = json.loads(json.dumps(lock))
            invalid["oc"][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                proof.check_middle_build(invalid, manifest, commands)
        with self.assertRaises(ValueError):
            proof.check_middle_build(lock, {"scripts/tui_capture/capture.mjs": "b" * 64}, commands)
        durable = json.loads(json.dumps(facts))
        durable["checks"][0]["restart_persistence_claim"] = True
        durable["checks"].append({"stage": "durable-rail", "width": 5,
                                  "restored_on_history_reopen": True, "preference_scope": "Location",
                                  "native_cas_observed": True,
                                  "actual_ordinal": {"x": 2, "y": 2}})
        proof.check_middle_facts(durable, "compact", "oc", persistent=True)
        with self.assertRaises(ValueError):
            proof.check_middle_facts(durable, "compact", "oc")
        with self.assertRaises(ValueError):
            proof.check_middle_facts(durable, "compact", "oc", historical=True, persistent=True)
        for key, value in (("width", True), ("width", 42), ("restored_on_history_reopen", False),
                           ("preference_scope", "global"), ("native_cas_observed", False),
                           ("actual_ordinal", {"x": 5, "y": 2}), ("actual_ordinal", {"x": True, "y": 2})):
            invalid = json.loads(json.dumps(durable))
            invalid["checks"][-1][key] = value
            with self.subTest(durable=key), self.assertRaises(ValueError):
                proof.check_middle_facts(invalid, "compact", "oc", persistent=True)
        with self.assertRaises(ValueError):
            proof.check_middle_build(lock, manifest, commands, persistent=True)
        durable_commands = json.loads(json.dumps(commands))
        durable_commands[0]["argv"] += ["--rail-persistence", "true", "--middle-case", "compact"]
        proof.check_middle_build(lock, manifest, durable_commands, persistent=True)
        original_durable = json.loads(json.dumps(durable))
        original_durable["origin"] = "upstream"
        original_durable["checks"][-1].update(preference_scope="global", native_cas_observed=False)
        proof.check_middle_facts(original_durable, "compact", "upstream", persistent=True)
        aligned = json.loads(json.dumps(durable))
        aligned["checks"][-1]["actual_ordinal"] = {"x": 2, "y": 3}
        aligned["checks"].append({"stage": "compact-layout", "width": 5,
                                  "actual_ordinals": [{"x": 2, "y": y} for y in (3, 5, 7)]})
        proof.check_middle_facts(aligned, "compact", "oc", persistent=True, aligned=True)
        for index, value in ((0, {"x": 2, "y": 2}), (1, {"x": True, "y": 5}),
                             (2, {"x": 2, "y": 6})):
            bad = json.loads(json.dumps(aligned))
            bad["checks"][-1]["actual_ordinals"][index] = value
            with self.subTest(ordinal=index), self.assertRaises(ValueError):
                proof.check_middle_facts(bad, "compact", "oc", persistent=True, aligned=True)
        with self.assertRaises(ValueError):
            proof.check_middle_facts(durable, "compact", "oc", persistent=True, aligned=True)
        with self.assertRaises(ValueError):
            proof.check_middle_facts(aligned, "compact", "oc", aligned=True)
        functional = json.loads(json.dumps(aligned))
        functional["checks"].append({"stage": "rail-controls", "actual_search": {"x": 2, "y": 1},
                                      "actual_add": {"x": 2, "y": 9}, "matched_left_click": True,
                                      "dialog_opened": True, "cancelled": True, "same_root_answer": True,
                                      "draft_preserved": True, "caret_preserved": True,
                                      "history_deck_unchanged": True, "no_extra_requests": True})
        for key, symbol in (("search_hover_row", "⌕"), ("add_hover_row", "+")):
            functional["checks"][-1][key] = [{"symbol": symbol if x == 2 else " ", "bg": "#1e1e1e"}
                                           for x in range(5)]
        functional["checks"][-1]["add_idle_row"] = [{"symbol": "+" if x == 2 else " ", "bg": "#141414"}
                                                   for x in range(5)]
        proof.check_middle_facts(functional, "compact", "oc", persistent=True, aligned=True, controls=True)
        for key, value in (("cancelled", False), ("no_extra_requests", False),
                           ("actual_search", {"x": True, "y": 1}), ("actual_add", {"x": 5, "y": 9})):
            bad = json.loads(json.dumps(functional))
            bad["checks"][-1][key] = value
            with self.subTest(control=key), self.assertRaises(ValueError):
                proof.check_middle_facts(bad, "compact", "oc", persistent=True, aligned=True, controls=True)
        with self.assertRaises(ValueError):
            proof.check_middle_facts(aligned, "compact", "oc", persistent=True, aligned=True, controls=True)
        bad = json.loads(json.dumps(functional))
        bad["checks"][-1]["add_hover_row"][0]["bg"] = "#141414"
        with self.assertRaises(ValueError):
            proof.check_middle_facts(bad, "compact", "oc", persistent=True, aligned=True, controls=True)
        bad = json.loads(json.dumps(functional))
        bad["checks"][-1]["add_idle_row"][0]["bg"] = "#1e1e1e"
        with self.assertRaises(ValueError):
            proof.check_middle_facts(bad, "compact", "oc", persistent=True, aligned=True, controls=True)
        with self.assertRaises(ValueError):
            proof.check_middle_build(lock, manifest, durable_commands, persistent=True, controls=True)
        functional_commands = json.loads(json.dumps(durable_commands))
        functional_commands[0]["argv"] += ["--rail-controls", "true"]
        proof.check_middle_build(lock, manifest, functional_commands, persistent=True, controls=True)

    def test_blink_profile_requires_real_cycles_and_preserves_only_scoped_reference_failure(self):
        facts = {"origin": {"x": 5, "y": 74}, "left_moves": 2,
                 "paste_cursor": {"x": 24, "y": 74, "visible": True},
                 "inside_cursor": {"x": 21, "y": 74, "visible": True},
                 "resized_origin": {"x": 5, "y": 18},
                 "resized_cursor": {"x": 21, "y": 18, "visible": True}}
        states = [{"state": name, "moves": 270 if "hover" in name else 0,
                   "commands": 100 if "hover" in name else 0, "samples": 300,
                   "cycles": 3, "cycle_periods_ms": [1200, 1201, 1199],
                   "max_sample_gap_ms": 30.5, "phantom_command_states": 0,
                   "temporal_full_frames": 12, "final_caret_preserved": True,
                   "adequate_raster_sampling": True, "cadence_preserved": True,
                   "raster_blink_observed": True}
                  for name in ("idle", "hover", "restored", "search-idle", "search-hover", "composer-restored")]
        lock = {"profile": {"cursor_temporal": "blink", "cursor_case": "expanded",
                            "cursor_sync": "unsupported", "cursor_renderer": "webgl",
                            "columns": 80, "rows": 24, "cursor_blink": True}}
        cursor = {"status": "QUALIFIED_CURSOR_BEHAVIOR_ONLY", "mode": "blink",
                  "card_case": "expanded", "geometry": {"columns": 80, "rows": 24},
                  "draft_input": facts, "states": states}
        proof.check_temporal_profile(lock, cursor, "oc", "blink-fallback")
        search = json.loads(json.dumps(cursor))
        for state in search["states"][3:5]:
            state.update(cycle_periods_ms=[1320, 1320, 1320], max_sample_gap_ms=310)
        # Search owns a different blink baseline; do not compare it to composer.
        proof.check_temporal_profile(lock, search, "oc", "blink-fallback")
        for key, value in (("cycles", 2), ("cycles", True), ("phantom_command_states", 1),
                           ("max_sample_gap_ms", 400), ("raster_blink_observed", False)):
            bad = json.loads(json.dumps(cursor))
            bad["states"][1][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                proof.check_temporal_profile(lock, bad, "oc", "blink-fallback")
        baseline_lock = json.loads(json.dumps(lock))
        baseline_lock["profile"].update(cursor_case="collapsed", cursor_sync="supported")
        baseline = json.loads(json.dumps(cursor))
        baseline.update(status="FAILED_CURSOR_QUALIFICATION", card_case="collapsed")
        baseline["states"][1].update(cycles=1, cycle_periods_ms=[1644],
                                      cadence_preserved=False, raster_blink_observed=False)
        proof.check_temporal_profile(baseline_lock, baseline, "upstream", "blink-baseline")
        with self.assertRaises(ValueError):
            proof.check_temporal_profile(baseline_lock, baseline, "oc", "blink-baseline")
        with self.assertRaises(ValueError):
            proof.check_temporal_profile(lock, baseline, "upstream", "blink-fallback")
        steady_lock = json.loads(json.dumps(lock))
        steady_lock["profile"].update(cursor_temporal="steady", cursor_blink=False, columns=160, rows=48)
        steady = json.loads(json.dumps(cursor))
        steady.update(mode="steady", geometry={"columns": 160, "rows": 48})
        for state in steady["states"]:
            state.update(cycles=0, cycle_periods_ms=[], raster_blink_observed=False)
        proof.check_temporal_profile(steady_lock, steady, "oc", "blink-steady")
        with self.assertRaises(ValueError):
            proof.check_temporal_profile(steady_lock, cursor, "oc", "blink-steady")

    def test_caret_readiness_uses_real_painted_origins_without_coordinate_normalization(self):
        facts = {"origin": {"x": 5, "y": 74}, "left_moves": 2,
                 "paste_cursor": {"x": 24, "y": 74, "visible": True},
                 "inside_cursor": {"x": 21, "y": 74, "visible": True},
                 "resized_origin": {"x": 5, "y": 18},
                 "resized_cursor": {"x": 21, "y": 18, "visible": True}}
        proof.check_caret_readiness({"draft_input": facts})
        for field, key, value in (("inside_cursor", "x", 24), ("resized_cursor", "x", 24),
                                  ("paste_cursor", "visible", False), ("origin", "x", True)):
            altered = json.loads(json.dumps(facts))
            altered[field][key] = value
            with self.subTest(field=field, key=key), self.assertRaises(ValueError):
                proof.check_caret_readiness({"draft_input": altered})
        with self.assertRaises(ValueError):
            proof.check_caret_readiness({})

    def test_current_and_historical_producers_require_the_exact_canonical_source_seal(self):
        # JS locale collation puts app.rs before README.md; codepoint sort does not.
        manifest = {"crates/oc-tui/README.md": "b" * 64, "crates/oc-tui/app.rs": "a" * 64}
        canonical = {"crates/oc-tui/app.rs": "a" * 64, "crates/oc-tui/README.md": "b" * 64}
        seal = proof.digest(json.dumps(canonical, separators=(",", ":")).encode())
        lock = {"oc": {"source_manifest_sha256": seal}}
        proof.check_source_manifest(lock, manifest, 2)
        for name in ("/outside.rs", "crates/../outside.rs", "crates//outside.rs", ".local/live.env"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "source path"):
                proof.check_source_manifest(lock, {name: "a" * 64}, 1)
        with patch.object(proof.subprocess, "run") as enumerate_source:
            enumerate_source.return_value.returncode = 0
            enumerate_source.return_value.stdout = b"Cargo.toml\0"
            with self.assertRaisesRegex(ValueError, "exact current producer source set"):
                proof.check_current_source(manifest)
        bad = {"oc": {"source_manifest_sha256": "0" * 64}}
        with self.assertRaisesRegex(ValueError, "source manifest seal"):
            proof.check_source_manifest(bad, manifest, 2)
        for episode, checker, count in (("inventory", proof.producer_checks, 2),
                                        ("diagnostic-before", proof.diagnostic_checks, 707)):
            recorded = manifest if count == 2 else {f"crates/input-{index}.rs": "a" * 64
                                                    for index in range(count)}
            files = {episode + "/capture.lock.json": json.dumps(bad).encode(),
                     episode + "/source-manifest.json": json.dumps(recorded).encode()}
            with self.subTest(episode=episode), self.assertRaisesRegex(ValueError, "source manifest seal"):
                if count == 2:
                    checker(files, count)
                else:
                    checker(files)

    def test_full_byte_roundtrip_same_side_alias_and_independent_commitment(self):
        files = {"inventory/oc/a.png": b"original encoded PNG A\x00\xff",
                 "inventory/oc/b.png": b"original encoded PNG A\x00\xff",
                 "inventory/upstream/a.png": b"original encoded PNG A\x00\xff",
                 "inventory/upstream/b.png": b"original encoded PNG A\x00\xff",
                 "ordinary/oc/raw.vt": b"\x1b[?1049hraw\r\n\xe4\xb8\xad\x00",
                 "matched/report.json": b'{ "time": 1.2300, "status": "DIFFERENT" }\n'}
        aliases = proof.png_aliases(files)
        self.assertEqual(aliases, {"inventory/oc/b.png": "inventory/oc/a.png",
                                  "inventory/upstream/b.png": "inventory/upstream/a.png"})
        expected = proof.inventory(files, aliases)
        encoded, raw_sha, raw_bytes = proof.encode(files, aliases)
        restored, links, observed_sha, observed_bytes = proof.unpack(encoded)
        self.assertEqual(restored, files)
        self.assertEqual(proof.inventory(restored, links), expected)
        self.assertEqual((observed_sha, observed_bytes), (raw_sha, raw_bytes))
        self.assertNotEqual(proof.inventory({**restored, "matched/report.json": b"changed"}, links), expected)
        for invalid in (encoded + b"junk", encoded + encoded, encoded[:-1]):
            with self.assertRaises((ValueError, lzma.LZMAError)):
                proof.unpack(invalid)
        with self.assertRaisesRegex(ValueError, "TAR tail"):
            proof.unpack(lzma.compress(lzma.decompress(encoded) + b"hidden trailing member bytes"))

    def test_member_path_alias_scope_and_aggregate_member_bounds(self):
        def encoded(items):
            target = io.BytesIO()
            with tarfile.open(fileobj=target, mode="w") as archive:
                for name, link in items:
                    item = tarfile.TarInfo(name)
                    if link is None:
                        archive.addfile(item, io.BytesIO(b""))
                    else:
                        item.type, item.linkname = tarfile.LNKTYPE, link
                        archive.addfile(item)
            return lzma.compress(target.getvalue())

        for invalid in ([('inventory/../escape', None)],
                        [('inventory/oc/a.png', None), ('inventory/upstream/b.png', 'inventory/oc/a.png')],
                        [('inventory/oc/a.png', None), ('inventory/oc/a.png', None)],
                        [('inventory/oc/a.png', None), ('inventory/oc/b.png', 'inventory/oc/a.png'),
                         ('inventory/oc/c.png', 'inventory/oc/b.png')]):
            with self.assertRaises(ValueError):
                proof.unpack(encoded(invalid))
        items = [('inventory/oc/base.png', None),
                 *[(f'inventory/oc/alias-{index}.png', 'inventory/oc/base.png') for index in range(2000)]]
        with self.assertRaisesRegex(ValueError, 'membership'):
            proof.unpack(encoded(items))
        with patch.object(proof, 'MAX_TOTAL', 1024):
            with self.assertRaisesRegex(ValueError, 'bounded XZ'):
                proof.unpack(encoded([('inventory/oc/empty', None)]))


if __name__ == "__main__":
    unittest.main()
