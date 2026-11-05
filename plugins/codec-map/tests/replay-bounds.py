# SPDX-License-Identifier: MPL-2.0
"""Validate AE replay-bounds.jsx output (requires Pillow)."""
from pathlib import Path

from PIL import Image, ImageChops

root = Path(__file__).resolve().parents[3] / "target/codec-map-replay-bounds"
report = []


def read(name):
    with Image.open(root / f"{name}.png") as image:
        return image.convert("RGBA")


def equal(reference, result):
    a, b = read(reference), read(result)
    assert a.size == b.size, result
    assert all(band.getbbox() is None for band in ImageChops.difference(a, b).split()), result
    assert a.getbbox() is not None, f"Empty render: {reference}"
    report.append(f"PASS {reference} == {result}")


assert len(list(root.glob("*.png"))) == 55, "Expected 55 rendered PNGs"
for codec in range(1, 4):
    for direction in ("grow", "shrink"):
        for kind in ("codec", "shadow"):
            prefix = f"{kind}{codec}-{direction}"
            equal(prefix + "-reference", prefix + "-drop")
            equal(prefix + "-drop", prefix + "-repeat")
equal("memory-seed", "memory-270-mib")
equal("memory-270-mib", "memory-270-mib-repeat")
equal("bypass-reference", "bypass-mix-zero")
for name, value in (("bypass-map", 255), ("bypass-offset", 127)):
    image = read(name)
    assert image.size == (1920, 1080)
    assert image.getextrema() == ((value, value),) * 3 + ((255, 255),), name
    report.append(f"PASS {name}: uniform {value}")
(root / "pixel-report.txt").write_text("\n".join(report) + "\n", encoding="utf-8")
print("\n".join(report))
