import io
import lzma
import tarfile
import unittest
from unittest.mock import patch

import retain_syntax_proof as proof


class SyntaxProofRetentionTests(unittest.TestCase):
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
