import io
import json
import lzma
import tarfile
import unittest
from unittest.mock import patch

import retain_syntax_proof as proof


class SyntaxProofRetentionTests(unittest.TestCase):
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
