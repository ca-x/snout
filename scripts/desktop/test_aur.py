from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


PKGBUILD = Path(__file__).resolve().parents[2] / "snout-desktop-bin" / "PKGBUILD"
CLI_PKGBUILD = PKGBUILD.parent.parent / "snout-bin" / "PKGBUILD"


@unittest.skipUnless(sys.platform.startswith("linux"), "Arch Linux package contract")
class DesktopAurTests(unittest.TestCase):
    def test_terminal_packages_mark_both_architectures_as_package_managed(self):
        for architecture in ["x86_64", "aarch64"]:
            with self.subTest(architecture=architecture), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "source"
                source.mkdir()
                binary = source / f"snout-9.8.7-linux-{architecture}"
                binary.write_bytes(b"terminal fixture")
                package = root / "package"
                subprocess.run([
                    "bash", "-euc", 'source "$1"; pkgver=9.8.7; srcdir="$2"; pkgdir="$3"; CARCH="$4"; package',
                    "aur-test", str(CLI_PKGBUILD), str(source), str(package), architecture,
                ], check=True)
                self.assertEqual((package / "usr/bin/snout").read_bytes(), binary.read_bytes())
                marker = package / "usr/share/snout/package-managed"
                self.assertEqual(marker.read_text(), "pacman\n")
                self.assertEqual(marker.stat().st_mode & 0o777, 0o644)

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
            for size in [256, 512]:
                icon = release / f"share/icons/hicolor/{size}x{size}/apps/io.github.snout.desktop.png"
                icon.parent.mkdir(parents=True)
                icon.write_bytes(f"icon fixture {size}".encode())
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
            for size in [256, 512]:
                relative = f"share/icons/hicolor/{size}x{size}/apps/io.github.snout.desktop.png"
                installed_icon = package_dir / "usr" / relative
                self.assertEqual(installed_icon.read_bytes(), (release / relative).read_bytes())
                self.assertEqual(installed_icon.stat().st_mode & 0o777, 0o644)
            installed_license = package_dir / "usr/share/licenses/snout-desktop-bin/LICENSE"
            self.assertEqual(installed_license.read_bytes(), license_file.read_bytes())
            self.assertEqual(installed_license.stat().st_mode & 0o777, 0o644)
            self.assertFalse((package_dir / "usr/bin/snout").exists())
            managed = package_dir / "usr/share/snout-desktop/package-managed"
            self.assertEqual(managed.read_text(), "pacman\n")
            self.assertEqual(managed.stat().st_mode & 0o777, 0o644)
            self.assertFalse((package_dir / "usr/nsh/app.json").exists())


if __name__ == "__main__":
    unittest.main()
