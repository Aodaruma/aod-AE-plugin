# SPDX-License-Identifier: MPL-2.0
"""Validate v03.jsx outputs after AE finishes writing the PNGs."""
from pathlib import Path
from PIL import Image, ImageChops

root = Path(__file__).resolve().parents[3] / "target/codec-map-v03/ui"
assert "Completed." in (root / "host-report.txt").read_text(errors="replace")
assert len(list(root.glob("*.png"))) == 16
report = []


def compare(a, b, same=True):
    with Image.open(root / f"{a}.png") as first, Image.open(root / f"{b}.png") as second:
        assert first.size == second.size
        difference = ImageChops.difference(first.convert("RGBA"), second.convert("RGBA"))
        equal = all(band.getbbox() is None for band in difference.split())
        assert equal == same, (a, b, same)
    report.append(f"PASS {a} / {b}: {'equal' if same else 'different'}")


for bits in (8, 16, 32):
    compare(f"reference{bits}", f"zero{bits}")
compare("4k-reference", "4k-zero")
compare("datamosh-reset-reference", "datamosh-reset")
compare("seed-before-edit", "seed-after-edit-warm", False)
compare("seed-after-edit-warm", "seed-after-edit-cold")
compare("compression", "blocks", False)
(root / "pixel-report.txt").write_text("\n".join(report) + "\n", encoding="utf-8")
print("\n".join(report))
