#!/usr/bin/env python3
"""Generate desktop icons from assets/logo.png (requires Pillow)."""

from pathlib import Path

from PIL import Image


root = Path(__file__).resolve().parent.parent
icons = root / "desktop" / "assets"
icons.mkdir(exist_ok=True)
with Image.open(root / "assets" / "logo.png") as original:
    source = original.convert("RGBA")
for size, name in [(256, "logo.png"), (512, "logo-512.png")]:
    source.resize((size, size), Image.Resampling.LANCZOS).save(icons / name, optimize=True)
source.resize((1024, 1024), Image.Resampling.LANCZOS).save(icons / "snout.icns", format="ICNS")
source.resize((256, 256), Image.Resampling.LANCZOS).save(
    icons / "snout.ico",
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
