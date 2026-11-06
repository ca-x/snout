#!/usr/bin/env python3

import argparse
import hashlib
from pathlib import Path


def write_checksums(directory):
    assets = sorted(path for path in directory.iterdir() if path.name != "SHA256SUMS")
    if not assets:
        raise ValueError("no release assets found")
    lines = []
    for asset in assets:
        if not asset.is_file():
            raise ValueError(f"release assets must be flat files: {asset}")
        with asset.open("rb") as binary:
            digest = hashlib.file_digest(binary, "sha256").hexdigest()
        lines.append(f"{digest}  {asset.name}\n")
    manifest = directory / "SHA256SUMS"
    manifest.write_text("".join(lines), encoding="utf-8", newline="\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description="Hash every CLI and desktop release asset")
    parser.add_argument("directory", type=Path)
    arguments = parser.parse_args()
    print(write_checksums(arguments.directory))


if __name__ == "__main__":
    main()
