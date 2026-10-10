"""Private storage guards; real raster/schema/comparisons are audited separately."""
import importlib.util
import io
import lzma
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("root_archive", Path(__file__).with_name("root-handoff-archive.py"))
ARCHIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ARCHIVE)
TEMP = "/home/opencode/.cache/opencode-tmp/opencode"


class ArchiveGuards(unittest.TestCase):
    def payload(self, items):
        result = io.BytesIO()
        with tarfile.open(fileobj=result, mode="w", format=tarfile.USTAR_FORMAT) as output:
            first = tarfile.TarInfo("ordinary/oc/first.png")
            first.size = 1
            output.addfile(first, io.BytesIO(b"x"))
            for name, target in items:
                item = tarfile.TarInfo(name)
                item.type, item.linkname = tarfile.LNKTYPE, target
                output.addfile(item)
        return lzma.compress(result.getvalue(), preset=0)

    def test_all_aliases_count_towards_member_limit_and_never_cross_sides(self):
        aliases = [(f"ordinary/oc/{i}.png", "ordinary/oc/first.png") for i in range(2000)]
        with self.assertRaises(AssertionError):
            ARCHIVE.unpack(self.payload(aliases))
        for name, target in (("ordinary/upstream/alias.png", "ordinary/oc/first.png"),
                             ("temporal/oc/alias.png", "ordinary/oc/first.png"),
                             ("ordinary/oc/alias.png", "../first.png")):
            with self.assertRaises(AssertionError):
                ARCHIVE.unpack(self.payload([(name, target)]))
        files, links, _ = ARCHIVE.unpack(self.payload([("ordinary/oc/alias.png", "ordinary/oc/first.png")]))
        self.assertEqual(files["ordinary/oc/alias.png"], b"x")
        self.assertEqual(len(links), 1)

    def test_fresh_root_symlink_refused_and_durable_writer_cannot_replace(self):
        with tempfile.TemporaryDirectory(prefix="root-archive-test-", dir=TEMP) as directory:
            root = Path(directory)
            target = root / "target"
            target.mkdir()
            (root / "ordinary").symlink_to(target, target_is_directory=True)
            with self.assertRaises(AssertionError):
                ARCHIVE.fresh_files(root)
            value = root / "immutable"
            ARCHIVE.durable_write(value, b"full bytes\x00\x1b[31m\n")
            with self.assertRaises(FileExistsError):
                ARCHIVE.durable_write(value, b"replacement")
            self.assertEqual(value.read_bytes(), b"full bytes\x00\x1b[31m\n")


if __name__ == "__main__":
    unittest.main()
