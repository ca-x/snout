#!/usr/bin/env python3

import argparse
import hashlib
import json
from pathlib import Path
import re


APP_ID = "io.github.snout.desktop"
ASSETS = {
    "linux-x86_64": "linux-x86_64-gnu.tar.gz",
    "macos-x86_64": "macos-x86_64.tar.gz",
    "macos-aarch64": "macos-aarch64.tar.gz",
    "windows-x86_64": "windows-x86_64.zip",
}
CLI_ASSETS = {
    "cli-linux-x86_64": "linux-x86_64",
    "cli-linux-aarch64": "linux-aarch64",
    "cli-macos-x86_64": "macos-x86_64",
    "cli-macos-aarch64": "macos-aarch64",
    "cli-windows-x86_64": "windows-x86_64.exe",
}


def write_manifest(directory, tag, notes=None):
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("expected stable tag vX.Y.Z")
    platforms = {}
    filenames = {platform: f"snout-desktop-{tag}-{suffix}" for platform, suffix in ASSETS.items()}
    filenames.update({platform: f"snout-{tag}-{suffix}" for platform, suffix in CLI_ASSETS.items()})
    for platform, filename in filenames.items():
        asset = directory / filename
        with asset.open("rb") as payload:
            digest = hashlib.file_digest(payload, "sha256").hexdigest()
        size = asset.stat().st_size
        if size == 0:
            raise ValueError(f"empty update asset: {filename}")
        platforms[platform] = {
            "url": f"https://github.com/ca-x/snout/releases/download/{tag}/{filename}",
            "sha256": digest,
            "size": size,
        }
    manifest = {"id": APP_ID, "version": tag[1:], "platforms": platforms}
    if notes:
        manifest["notes"] = notes
    destination = directory / "latest.json"
    destination.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return destination


def main():
    parser = argparse.ArgumentParser(description="Generate the NSH desktop update manifest")
    parser.add_argument("directory", type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--notes", type=Path)
    arguments = parser.parse_args()
    notes = arguments.notes.read_text(encoding="utf-8") if arguments.notes and arguments.notes.exists() else None
    print(write_manifest(arguments.directory, arguments.tag, notes))


if __name__ == "__main__":
    main()
