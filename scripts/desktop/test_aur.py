from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


PKGBUILD = Path(__file__).resolve().parents[2] / "snout-desktop-bin" / "PKGBUILD"


@unittest.skipUnless(sys.platform.startswith("linux"), "Arch Linux package contract")
class DesktopAurTests(unittest.TestCase):
    def test_metadata_keeps_desktop_separate_from_cli(self):
        result = subprocess.run(
            [
                "bash",
                "-euc",
                'source "$1"; printf "%s\\n" "$pkgname" "$pkgver" "${arch[*]}" '
                '"${provides[*]}" "${conflicts[*]}" "${options[*]}" "${depends[*]}" '
                '"${source[*]}" "${source_x86_64[*]}" "${sha256sums[*]}" "${sha256sums_x86_64[*]}"',
                "aur-test",
                str(PKGBUILD),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        metadata = result.stdout.splitlines()
        self.assertEqual(metadata[0], "snout-desktop-bin")
        self.assertEqual(metadata[2], "x86_64")
        self.assertEqual(metadata[3:5], ["snout-desktop", "snout-desktop"])
        self.assertEqual(metadata[5].split(), ["!strip", "!debug"])
        self.assertIn("gtk4>=4.14", metadata[6].split())
        self.assertNotIn("snout-bin", metadata[6].split())
        self.assertIn(f"/v{metadata[1]}/LICENSE", metadata[7])
        self.assertTrue(
            metadata[8].endswith(
                f"/v{metadata[1]}/snout-desktop-v{metadata[1]}-linux-x86_64-gnu.tar.gz"
            )
        )
        self.assertEqual(metadata[9:], ["SKIP", "SKIP"])

    def test_package_installs_binary_launcher_and_license_for_selected_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source_dir = root / "source files"
            package_dir = root / "package files"
            version = "9.8.7"
            release = source_dir / f"snout-desktop-v{version}-linux-x86_64-gnu"
            binary = release / "bin" / "snout-desktop"
            launcher = release / "share/applications/io.github.snout.desktop.desktop"
            binary.parent.mkdir(parents=True)
            launcher.parent.mkdir(parents=True)
            binary.write_bytes(b"desktop executable fixture\n")
            launcher.write_text("[Desktop Entry]\nExec=snout-desktop\n", encoding="utf-8")
            license_file = source_dir / f"snout-LICENSE-{version}"
            license_file.write_text("MIT license fixture\n", encoding="utf-8")

            subprocess.run(
                [
                    "bash",
                    "-euc",
                    'source "$1"; pkgver="$2"; srcdir="$3"; pkgdir="$4"; '
                    'CARCH=x86_64; package',
                    "aur-test",
                    str(PKGBUILD),
                    version,
                    str(source_dir),
                    str(package_dir),
                ],
                check=True,
            )

            installed = package_dir / "usr/bin/snout-desktop"
            self.assertEqual(installed.read_bytes(), binary.read_bytes())
            self.assertEqual(installed.stat().st_mode & 0o777, 0o755)
            self.assertEqual(
                (package_dir / "usr/share/applications/io.github.snout.desktop.desktop").read_bytes(),
                launcher.read_bytes(),
            )
            installed_license = package_dir / "usr/share/licenses/snout-desktop-bin/LICENSE"
            self.assertEqual(installed_license.read_bytes(), license_file.read_bytes())
            self.assertEqual(installed_license.stat().st_mode & 0o777, 0o644)
            self.assertFalse((package_dir / "usr/bin/snout").exists())


if __name__ == "__main__":
    unittest.main()
