from pathlib import Path
import tempfile
import unittest

from checksums import write_checksums


class ReleaseChecksumTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def test_known_digests_cover_cli_and_desktop_assets_without_hashing_manifest(self):
        (self.root / "snout-v0.3.0-linux-x86_64").write_bytes(b"abc")
        (self.root / "snout-desktop-v0.3.0-windows-x86_64.zip").write_bytes(b"")
        (self.root / "SHA256SUMS").write_text("stale checksums", encoding="utf-8")
        expected = (
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            "  snout-desktop-v0.3.0-windows-x86_64.zip\n"
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            "  snout-v0.3.0-linux-x86_64\n"
        )

        for _ in range(2):
            manifest = write_checksums(self.root)
            self.assertEqual(manifest, self.root / "SHA256SUMS")
            self.assertEqual(manifest.read_bytes(), expected.encode("utf-8"))

    def test_empty_asset_directory_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "no release assets"):
            write_checksums(self.root)
        self.assertFalse((self.root / "SHA256SUMS").exists())

    def test_nested_asset_directory_is_rejected_instead_of_silently_skipped(self):
        directory = self.root / "unmerged-artifact"
        directory.mkdir()
        (directory / "snout").write_bytes(b"abc")

        with self.assertRaisesRegex(ValueError, "flat files"):
            write_checksums(self.root)
        self.assertFalse((self.root / "SHA256SUMS").exists())


if __name__ == "__main__":
    unittest.main()
