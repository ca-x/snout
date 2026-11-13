import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from update_manifest import ASSETS, CLI_ASSETS, write_manifest


class UpdateManifestTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def assets(self):
        for suffix in ASSETS.values():
            (self.root / f"snout-desktop-v1.2.3-{suffix}").write_bytes(b"fixture")
        for suffix in CLI_ASSETS.values():
            (self.root / f"snout-v1.2.3-{suffix}").write_bytes(b"fixture")

    def test_manifest_covers_all_platforms_with_pinned_urls_and_verified_sizes(self):
        self.assets()
        manifest = json.loads(write_manifest(self.root, "v1.2.3", "更新说明").read_text(encoding="utf-8"))
        self.assertEqual(manifest["id"], "io.github.snout.desktop")
        self.assertEqual(manifest["version"], "1.2.3")
        self.assertEqual(manifest["notes"], "更新说明")
        self.assertEqual(set(manifest["platforms"]), set(ASSETS) | set(CLI_ASSETS))
        for platform, asset in manifest["platforms"].items():
            self.assertEqual(asset["size"], 7)
            self.assertEqual(asset["sha256"], hashlib.sha256(b"fixture").hexdigest())
            filename = f"snout-v1.2.3-{CLI_ASSETS[platform]}" if platform in CLI_ASSETS else f"snout-desktop-v1.2.3-{ASSETS[platform]}"
            self.assertEqual(asset["url"], f"https://github.com/ca-x/snout/releases/download/v1.2.3/{filename}")

    def test_missing_or_empty_asset_does_not_publish_partial_manifest(self):
        for empty in [False, True]:
            with self.subTest(empty=empty):
                self.assets()
                asset = self.root / "snout-desktop-v1.2.3-windows-x86_64.zip"
                if empty:
                    asset.write_bytes(b"")
                else:
                    asset.unlink()
                with self.assertRaises((FileNotFoundError, ValueError)):
                    write_manifest(self.root, "v1.2.3")
                self.assertFalse((self.root / "latest.json").exists())

    def test_unstable_or_malformed_tag_is_rejected(self):
        for tag in ["1.2.3", "v1.2.3-beta.1", "v1.2.3/other", ""]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                write_manifest(self.root, tag)


if __name__ == "__main__":
    unittest.main()
