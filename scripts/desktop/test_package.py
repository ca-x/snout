import importlib.util
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile


SCRIPT = Path(__file__).resolve().parent.parent / "package-desktop.py"
SPEC = importlib.util.spec_from_file_location("package_desktop", SCRIPT)
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


class DesktopPackageTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.target_dir = self.root / "desktop target"
        self.output_dir = self.root / "release assets"
        self.version = "0.3.0"
        self.binary_content = b"fixture desktop executable\n"

    def create_binary(self, target):
        executable = "snout-desktop.exe" if "windows" in target else "snout-desktop"
        binary = self.target_dir / target / "release" / executable
        binary.parent.mkdir(parents=True)
        binary.write_bytes(self.binary_content)

    def package(self, target):
        self.create_binary(target)
        return PACKAGER.package_desktop(
            target, self.version, self.target_dir, self.output_dir
        )

    def test_linux_gnu_archive_has_executable_and_matching_desktop_entry(self):
        artifact = self.package("x86_64-unknown-linux-gnu")
        self.assertEqual(artifact.name, "snout-desktop-v0.3.0-linux-x86_64-gnu.tar.gz")
        prefix = "snout-desktop-v0.3.0-linux-x86_64-gnu"
        with tarfile.open(artifact) as archive:
            binary = archive.getmember(f"{prefix}/bin/snout-desktop")
            self.assertEqual(binary.mode & 0o777, 0o755)
            self.assertEqual(archive.extractfile(binary).read(), self.binary_content)
            entry = archive.extractfile(
                f"{prefix}/share/applications/io.github.snout.desktop.desktop"
            ).read().decode("utf-8")
            self.assertIn("Exec=snout-desktop\n", entry)
            self.assertIn("Terminal=false\n", entry)
            self.assertIn(f"Icon={PACKAGER.APP_ID}\n", entry)
            for size, filename in [(256, "logo.png"), (512, "logo-512.png")]:
                icon = archive.getmember(
                    f"{prefix}/share/icons/hicolor/{size}x{size}/apps/{PACKAGER.APP_ID}.png"
                )
                self.assertEqual(icon.mode & 0o777, 0o644)
                self.assertEqual(archive.extractfile(icon).read(), (PACKAGER.ICON_DIR / filename).read_bytes())
            self.assertIn("StartupWMClass=io.github.snout.desktop\n", entry)
            self.assertNotIn(f"{prefix}/bin/snout", archive.getnames())
            self.assertEqual(json.load(archive.extractfile(f"{prefix}/nsh/app.json")), {"id": PACKAGER.APP_ID, "version": self.version})

    def test_macos_archives_have_app_bundle_and_versioned_plist(self):
        for architecture in ["x86_64", "aarch64"]:
            with self.subTest(architecture=architecture):
                artifact = self.package(f"{architecture}-apple-darwin")
                self.assertEqual(
                    artifact.name, f"snout-desktop-v0.3.0-macos-{architecture}.tar.gz"
                )
                with tarfile.open(artifact) as archive:
                    binary = archive.getmember("snout.app/Contents/MacOS/snout-desktop")
                    self.assertEqual(binary.mode & 0o777, 0o755)
                    self.assertEqual(archive.extractfile(binary).read(), self.binary_content)
                    plist = plistlib.loads(
                        archive.extractfile("snout.app/Contents/Info.plist").read()
                    )
                    self.assertEqual(plist["CFBundleExecutable"], "snout-desktop")
                    self.assertEqual(plist["CFBundleIconFile"], "snout.icns")
                    self.assertEqual(
                        archive.extractfile("snout.app/Contents/Resources/snout.icns").read(),
                        (PACKAGER.ICON_DIR / "snout.icns").read_bytes(),
                    )
                    self.assertEqual(plist["CFBundleIdentifier"], "io.github.snout.desktop")
                    self.assertEqual(plist["CFBundlePackageType"], "APPL")
                    self.assertEqual(plist["CFBundleVersion"], "0.3.0")
                    self.assertEqual(plist["CFBundleShortVersionString"], "0.3.0")
                    self.assertEqual(json.load(archive.extractfile("snout.app/Contents/Resources/nsh/app.json")), {"id": PACKAGER.APP_ID, "version": self.version})

    def test_windows_zip_contains_portable_directory_and_identity(self):
        artifact = self.package("x86_64-pc-windows-msvc")
        self.assertEqual(artifact.name, "snout-desktop-v0.3.0-windows-x86_64.zip")
        with zipfile.ZipFile(artifact) as archive:
            prefix = "snout-desktop-v0.3.0-windows-x86_64"
            self.assertEqual(archive.namelist(), [f"{prefix}/snout-desktop.exe", f"{prefix}/nsh/app.json"])
            self.assertEqual(archive.read(f"{prefix}/snout-desktop.exe"), self.binary_content)
            self.assertEqual(json.loads(archive.read(f"{prefix}/nsh/app.json")), {"id": PACKAGER.APP_ID, "version": self.version})

    def test_missing_binary_does_not_create_release_asset(self):
        with self.assertRaises(FileNotFoundError):
            PACKAGER.package_desktop(
                "x86_64-unknown-linux-gnu",
                self.version,
                self.target_dir,
                self.output_dir,
            )
        self.assertFalse(self.output_dir.exists())

    def test_cli_supports_target_directory_overrides_and_matching_tag(self):
        target = "x86_64-unknown-linux-gnu"
        self.create_binary(target)
        with (SCRIPT.parent.parent / "desktop" / "Cargo.toml").open("rb") as manifest:
            version = tomllib.load(manifest)["package"]["version"]
        for override in ["environment", "argument"]:
            with self.subTest(override=override):
                output = self.output_dir / override
                arguments = [
                    sys.executable,
                    "-B",
                    str(SCRIPT),
                    "--target",
                    target,
                    "--output-dir",
                    str(output),
                    "--tag",
                    f"v{version}",
                ]
                environment = dict(os.environ, CARGO_TARGET_DIR=str(self.target_dir))
                if override == "argument":
                    arguments.extend(["--target-dir", str(self.target_dir)])
                    environment["CARGO_TARGET_DIR"] = str(self.root / "unused target")
                result = subprocess.run(
                    arguments,
                    cwd=self.root,
                    env=environment,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(
                    (output / f"snout-desktop-v{version}-linux-x86_64-gnu.tar.gz").is_file()
                )

    def test_mismatched_tag_is_rejected_before_packaging(self):
        result = subprocess.run(
            [
                sys.executable,
                "-B",
                str(SCRIPT),
                "--target",
                "x86_64-unknown-linux-gnu",
                "--target-dir",
                str(self.target_dir),
                "--output-dir",
                str(self.output_dir),
                "--tag",
                "v-not-the-desktop-version",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("does not match desktop version", result.stderr)
        self.assertFalse(self.output_dir.exists())


if __name__ == "__main__":
    unittest.main()
