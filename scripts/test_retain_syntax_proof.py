import io
import json
import lzma
import tarfile
import unittest
from unittest.mock import patch

import retain_syntax_proof as proof


class SyntaxProofRetentionTests(unittest.TestCase):
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
