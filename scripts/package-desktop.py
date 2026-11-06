#!/usr/bin/env python3

import argparse
import os
from pathlib import Path
import plistlib
import shutil
import tarfile
import tempfile
import tomllib
import zipfile


APP_ID = "io.github.snout.desktop"
TARGETS = {
    "x86_64-unknown-linux-gnu": "linux-x86_64-gnu",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
}


def package_desktop(target, version, target_dir, output_dir):
    suffix = TARGETS[target]
    windows = target.endswith("windows-msvc")
    executable = "snout-desktop.exe" if windows else "snout-desktop"
    binary = target_dir / target / "release" / executable
    if not binary.is_file():
        raise FileNotFoundError(f"desktop binary not found: {binary}")

    name = f"snout-desktop-v{version}-{suffix}"
    output_dir.mkdir(parents=True, exist_ok=True)
    if windows:
        artifact = output_dir / f"{name}.zip"
        with zipfile.ZipFile(artifact, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.write(binary, executable)
        return artifact

    with tempfile.TemporaryDirectory(prefix="snout-desktop-") as directory:
        staging = Path(directory)
        if target.endswith("apple-darwin"):
            package = staging / "snout.app"
            installed_binary = package / "Contents" / "MacOS" / executable
            installed_binary.parent.mkdir(parents=True)
            with (package / "Contents" / "Info.plist").open("wb") as plist:
                plistlib.dump(
                    {
                        "CFBundleDevelopmentRegion": "en",
                        "CFBundleDisplayName": "snout",
                        "CFBundleExecutable": executable,
                        "CFBundleIdentifier": APP_ID,
                        "CFBundleInfoDictionaryVersion": "6.0",
                        "CFBundleName": "snout",
                        "CFBundlePackageType": "APPL",
                        "CFBundleShortVersionString": version,
                        "CFBundleVersion": version,
                        "NSHighResolutionCapable": True,
                    },
                    plist,
                )
        else:
            package = staging / name
            installed_binary = package / "bin" / executable
            installed_binary.parent.mkdir(parents=True)
            applications = package / "share" / "applications"
            applications.mkdir(parents=True)
            (applications / f"{APP_ID}.desktop").write_text(
                "[Desktop Entry]\n"
                "Version=1.0\n"
                "Type=Application\n"
                "Name=snout\n"
                "Comment=Manage Rime input method schemes and settings\n"
                "Exec=snout-desktop\n"
                "TryExec=snout-desktop\n"
                "Terminal=false\n"
                "Categories=Utility;\n"
                f"StartupWMClass={APP_ID}\n",
                encoding="utf-8",
                newline="\n",
            )
        shutil.copyfile(binary, installed_binary)
        installed_binary.chmod(0o755)
        artifact = output_dir / f"{name}.tar.gz"
        executable_entry = installed_binary.relative_to(staging).as_posix()

        def set_permissions(member):
            member.mode = 0o755 if member.isdir() or member.name == executable_entry else 0o644
            return member

        with tarfile.open(artifact, "w:gz") as archive:
            archive.add(package, arcname=package.name, filter=set_permissions)
    return artifact


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description="Package the native snout desktop binary")
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=Path(os.environ.get("CARGO_TARGET_DIR", root / "desktop" / "target")),
    )
    parser.add_argument("--output-dir", type=Path, default=root / "dist")
    parser.add_argument("--tag")
    arguments = parser.parse_args()
    with (root / "desktop" / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    if arguments.tag is not None and arguments.tag != f"v{version}":
        parser.error(f"release tag {arguments.tag!r} does not match desktop version v{version}")
    print(
        package_desktop(
            arguments.target, version, arguments.target_dir, arguments.output_dir
        )
    )


if __name__ == "__main__":
    main()
